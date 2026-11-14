// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io.hh, mesh_io.cc
// (`MeshIO::getMeshIO`, `read`, `write`), src/mesh/mesh.cc (`Mesh::read`,
// `Mesh::write`, `makeReady`), src/io/mesh_io/mesh_io_msh.*,
// mesh_io_msh_struct.*, mesh_io_diana.*.

//! Mesh readers and writers (upstream `MeshIO`).
//!
//! | [`MeshIoType`] | read | write |
//! |---|---|---|
//! | `Gmsh` | ASCII MSH 1.0, 2.0–2.2, 4.0, 4.1 ([`msh`]) | MSH 2.2 ASCII ([`writer`]) |
//! | `GmshStruct` | same, structural element types | MSH 2.2 ASCII |
//! | `Diana` | [`diana`] | `Error::NotImplemented` (upstream `AKANTU_TO_IMPLEMENT`) |
//! | `Abaqus` | `Error::Unsupported` (D-027, UD-049) | `Error::Unsupported` |
//! | `Auto` | by extension: `.msh` → Gmsh, `.diana` → Diana | idem |
//!
//! Reading follows upstream `Mesh::read`: the file is parsed into a staging
//! area, every index is validated, and only then is the (empty) mesh filled
//! (nodes, not-ghost connectivities, `tag_*` / `physical_names` / data-block
//! mesh data, element groups with their `<name>_nodes` node groups and
//! standalone node groups), its bounding box set and a warning issued
//! when no element of the mesh dimension was read. A failed read leaves the
//! mesh unchanged (complete state, revision included), also when only
//! writing the Diana material side file fails (it is written before the
//! mesh is filled).
//!
//! A read is a **local** operation and never communicates, also on a mesh
//! whose communicator has several ranks: the target is
//! not distributed, so its global bounding box is the box of its nodes, as
//! upstream `computeBoundingBox` reduces only for distributed meshes. This
//! is what reading on one rank before distributing needs
//! (`docs/ARCHITECTURE.md`). The box is computed while staging and the
//! mesh is filled by moves only, so no fallible step follows the first
//! change of the target.
//!
//! Every error caused by the content of the input is an `Error::Parse`
//! carrying `file:line:column`, or an `Error::Unsupported` (binary file,
//! unsupported format version, parametric nodes) whose message starts with
//! `file:line:column:`.
//!
//! Not done here (yet): upstream finishes every Gmsh/Diana read with
//! `MeshUtils::fillElementToSubElementsData` (mesh data
//! `element_to_subelement` / `subelement_to_element`), which belongs to
//! the mesh crate (`akantu_mesh::utils`, MUTIL-006) and is not implemented there
//! yet; see `docs/FEATURE_PARITY.md` IO-005.

use std::path::Path;

use akantu_core::enums::MeshIoType;
use akantu_core::{Error, Result, TypeFilter};
use akantu_mesh::Mesh;

pub mod diana;
mod lexer;
pub mod msh;
mod numfmt;
mod staging;
mod tables;
pub mod writer;

pub use diana::{DianaMaterial, DianaMaterials};
pub use writer::WriteOptions;

/// Options of [`read_mesh_with`].
#[derive(Clone, Debug)]
pub struct ReadOptions {
    /// MSH 4: also put the elements of physical groups without a
    /// `$PhysicalNames` entry into the `msh_block_<tag>` groups the reader
    /// creates (UD-045 fix, default `true`). `false` reproduces upstream,
    /// which leaves those groups empty.
    pub fill_unnamed_physical_groups: bool,
    /// Diana: write the material definitions found in the file to
    /// `material_<file name>` next to the input, as upstream does (default
    /// `true`). The parsed materials are always returned in
    /// [`ReadReport::diana_materials`].
    pub write_diana_material_file: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            fill_unnamed_physical_groups: true,
            write_diana_material_file: true,
        }
    }
}

/// What a read produced besides the mesh.
#[derive(Clone, Debug, Default)]
pub struct ReadReport {
    /// Format actually read (never `Auto`).
    pub format: Option<MeshIoType>,
    /// Gmsh format version as upstream encodes it (`major * 1000 + minor`,
    /// `1000` for MSH 1.0); `None` for other formats.
    pub msh_version: Option<i32>,
    /// Warnings upstream prints with `AKANTU_DEBUG_WARNING` (skipped element
    /// types, unknown sections, wrong mesh dimension, …); also sent to the
    /// `log` facade.
    pub warnings: Vec<String>,
    /// Diana `'MATERIALS'` section, if any.
    pub diana_materials: Option<DianaMaterials>,
}

/// Format selected for `path` (upstream `MeshIO::getMeshIO` with
/// `_miot_auto`): extension `msh` → Gmsh, `diana` → Diana, anything else is
/// an error ("Cannot guess the type of file").
pub fn detect_format(path: &Path) -> Result<MeshIoType> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("msh") => Ok(MeshIoType::Gmsh),
        Some("diana") => Ok(MeshIoType::Diana),
        ext => Err(Error::invalid(format!(
            "Cannot guess the type of file of {} (ext {}). Please provide the MeshIOType to the read function",
            path.display(),
            ext.unwrap_or("")
        ))),
    }
}

fn resolve(path: &Path, format: MeshIoType) -> Result<MeshIoType> {
    let f = match format {
        MeshIoType::Auto => detect_format(path)?,
        f => f,
    };
    if f == MeshIoType::Abaqus {
        return Err(Error::Unsupported(
            "the Abaqus mesh format (_miot_abaqus) has no reader or writer in Akantu (D-027, UD-049)"
                .into(),
        ));
    }
    Ok(f)
}

/// Read a mesh file into an empty `mesh` (upstream `Mesh::read`).
pub fn read_mesh(mesh: &mut Mesh, path: &Path, format: MeshIoType) -> Result<()> {
    read_mesh_with(mesh, path, format, &ReadOptions::default()).map(|_| ())
}

/// [`read_mesh`] with options; returns warnings and format details.
///
/// Transactional: the file is parsed and validated and
/// everything the mesh receives is built (bounding box included), then the
/// Diana material side file is written, and only then is `mesh` filled by
/// infallible moves; if any step fails (including writing the side file)
/// `mesh` is unchanged. No communication takes place (see the module docs).
pub fn read_mesh_with(
    mesh: &mut Mesh,
    path: &Path,
    format: MeshIoType,
    opts: &ReadOptions,
) -> Result<ReadReport> {
    let format = resolve(path, format)?;
    staging::MeshBuild::check_target(mesh)?;
    let bytes = std::fs::read(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let text = String::from_utf8_lossy(&bytes);
    let name = path.display().to_string();
    let (mut report, prepared) = parse(mesh, &name, &text, format, opts)?;
    if let (Some(m), true) = (&report.diana_materials, opts.write_diana_material_file) {
        diana::write_material_file(path, m)?;
    }
    fill(mesh, &name, prepared, &mut report)?;
    Ok(report)
}

/// Read mesh text `text` (`name` is used in messages) into an empty `mesh`.
/// Same as [`read_mesh_with`] without file access (Diana material files
/// are not written).
pub fn read_mesh_from_str(
    mesh: &mut Mesh,
    name: &str,
    text: &str,
    format: MeshIoType,
    opts: &ReadOptions,
) -> Result<ReadReport> {
    let format = resolve(Path::new(name), format)?;
    staging::MeshBuild::check_target(mesh)?;
    let (mut report, prepared) = parse(mesh, name, text, format, opts)?;
    fill(mesh, name, prepared, &mut report)?;
    Ok(report)
}

/// Parse and validate `text` for `mesh` without modifying it.
fn parse(
    mesh: &Mesh,
    name: &str,
    text: &str,
    format: MeshIoType,
    opts: &ReadOptions,
) -> Result<(ReadReport, staging::PreparedMesh)> {
    let dim = mesh.spatial_dimension();
    let mut report = ReadReport {
        format: Some(format),
        ..ReadReport::default()
    };
    let build = match format {
        MeshIoType::Gmsh | MeshIoType::GmshStruct => {
            let flavor = if format == MeshIoType::Gmsh {
                msh::Flavor::Standard
            } else {
                msh::Flavor::Structural
            };
            let r = msh::read(name, text, dim, flavor, opts)?;
            report.msh_version = Some(r.version);
            report.warnings = r.warnings;
            r.build
        }
        MeshIoType::Diana => {
            let r = diana::read(name, text, dim)?;
            report.warnings = r.warnings;
            report.diana_materials = r.materials;
            r.build
        }
        MeshIoType::Auto | MeshIoType::Abaqus => unreachable!("resolved above"),
    };
    let prepared = build.prepare(mesh)?;
    Ok((report, prepared))
}

/// Move a parsed read into `mesh` and finish the report.
fn fill(
    mesh: &mut Mesh,
    name: &str,
    prepared: staging::PreparedMesh,
    report: &mut ReadReport,
) -> Result<()> {
    prepared.apply(mesh)?;
    let dim = mesh.spatial_dimension();
    if mesh
        .element_types(TypeFilter::new().dim(dim))
        .next()
        .is_none()
    {
        report.warnings.push(format!(
            "The mesh contained in the file {name} does not seem to be of the good dimension. \
             No element of dimension {dim} were read."
        ));
    }
    for w in &report.warnings {
        log::warn!("{w}");
    }
    Ok(())
}

/// Write `mesh` (upstream `Mesh::write`; Gmsh MSH 2.2 only upstream).
pub fn write_mesh(mesh: &Mesh, path: &Path, format: MeshIoType) -> Result<()> {
    write_mesh_with(mesh, path, format, &WriteOptions::default())
}

/// [`write_mesh`] with options.
pub fn write_mesh_with(
    mesh: &Mesh,
    path: &Path,
    format: MeshIoType,
    opts: &WriteOptions,
) -> Result<()> {
    let text = write_mesh_to_string(mesh, path, format, opts)?;
    std::fs::write(path, text).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

/// The text [`write_mesh_with`] would write (`path` only selects the format
/// for `Auto`).
pub fn write_mesh_to_string(
    mesh: &Mesh,
    path: &Path,
    format: MeshIoType,
    opts: &WriteOptions,
) -> Result<String> {
    match resolve(path, format)? {
        MeshIoType::Gmsh => writer::write_msh(mesh, opts, writer::Flavor::Standard),
        MeshIoType::GmshStruct => writer::write_msh(mesh, opts, writer::Flavor::Structural),
        MeshIoType::Diana => Err(Error::NotImplemented("MeshIODiana::write")),
        MeshIoType::Auto | MeshIoType::Abaqus => unreachable!("resolved above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_detection() {
        assert_eq!(
            detect_format(Path::new("a/b.msh")).unwrap(),
            MeshIoType::Gmsh
        );
        assert_eq!(
            detect_format(Path::new("x.diana")).unwrap(),
            MeshIoType::Diana
        );
        for bad in ["x.dat", "x.vtu", "x", "x.MSH"] {
            assert!(detect_format(Path::new(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn abaqus_is_unsupported_and_leaves_the_mesh_alone() {
        let mut m = Mesh::new(2, "m").unwrap();
        let e = read_mesh(&mut m, Path::new("does/not/exist.inp"), MeshIoType::Abaqus).unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)), "{e}");
        assert_eq!(m.nb_nodes(), 0);
        let e = write_mesh(&m, Path::new("x.inp"), MeshIoType::Abaqus).unwrap_err();
        assert!(matches!(e, Error::Unsupported(_)), "{e}");
    }
}
