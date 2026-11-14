// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io/mesh_io_msh.cc
// (`MeshIOMSH::write`), src/io/mesh_io/mesh_io_msh_struct.cc.

//! Gmsh MSH 2.2 ASCII writer (upstream `MeshIOMSH::write`).
//!
//! Layout as upstream: `$MeshFormat` `2.2 0 8`; `$Nodes` (`i+1 x y z`,
//! missing coordinates written as `0`, exponent in upper case);
//! `$Elements` (not-ghost elements of every type in enum order,
//! `id type 2 tag_0 tag_1 nodes…`, tags from `Int` mesh data `tag_0` /
//! `tag_1` when present, `0` otherwise); one `$NodeData` per nodal `Real`
//! dataset. Differences (documented in `docs/UPSTREAM_DEFECTS.md`):
//!
//! * UD-061: numbers use 17 significant digits ([`WriteOptions`]) so that a
//!   read → write → read round trip is bit-identical (upstream: 15, lossy);
//! * UD-044: one `$ElementData` block per elemental `Real` dataset and
//!   (extension) one `$ElementNodeData` block per element-node `Real`
//!   dataset (upstream enumerates the wrong names and never writes them, or
//!   throws);
//! * UD-093: nodes of 10-node tetrahedra, prisms and 20-node hexahedra are
//!   written in Gmsh order (undoing the reader's permutation; upstream
//!   writes Akantu order, so reading its output back permutes the
//!   connectivity);
//! * element types without a Gmsh code (cohesive, IGFEM) are skipped with a
//!   warning (upstream writes type `0`, which its own reader skips).

use std::fmt::Write as _;

use akantu_core::{ElementType, Error, GhostType, Result, TypeFilter};
use akantu_mesh::data::NodalDataValues;
use akantu_mesh::{Mesh, MeshDataValues};

use super::numfmt::format_g;
use super::tables::{akantu_to_msh, msh_read_order};

/// Writer options.
#[derive(Clone, Debug)]
pub struct WriteOptions {
    /// Significant digits of real numbers (default 17, round-trip exact;
    /// upstream uses `digits10` = 15).
    pub significant_digits: usize,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            significant_digits: 17,
        }
    }
}

impl WriteOptions {
    /// Upstream's text precision (`std::numeric_limits<double>::digits10`),
    /// for byte comparisons with files written by the C++ oracle.
    #[must_use]
    pub fn upstream_precision() -> Self {
        Self {
            significant_digits: 15,
        }
    }
}

/// Writer flavour (node order of the structural writer is the identity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Flavor {
    Standard,
    Structural,
}

/// Writing order: Gmsh node `j` is the Akantu slot `order[j]` (the reader
/// puts Gmsh node `j` into slot `read_order[j]`, so writing slot
/// `read_order[j]` at position `j` restores the Gmsh order).
fn write_order(ty: ElementType, flavor: Flavor) -> Option<&'static [usize]> {
    match flavor {
        Flavor::Standard => msh_read_order(ty),
        Flavor::Structural => None,
    }
}

/// Produce the MSH 2.2 text of `mesh`.
pub(crate) fn write_msh(mesh: &Mesh, opts: &WriteOptions, flavor: Flavor) -> Result<String> {
    let p = opts.significant_digits.clamp(1, 40);
    let mut out = String::new();
    out.push_str("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n");

    // Nodes.
    let nodes = mesh.nodes();
    let dim = nodes.nb_component();
    out.push_str("$Nodes\n");
    let _ = writeln!(out, "{}", nodes.len());
    for (i, x) in nodes.tuples().enumerate() {
        let _ = write!(out, "{}", i + 1);
        for &c in x {
            let _ = write!(out, " {}", format_g(c, p, true));
        }
        for _ in dim..3 {
            out.push_str(" 0");
        }
        out.push('\n');
    }
    out.push_str("$EndNodes\n");

    // Elements.
    let ghost = GhostType::NotGhost;
    let types: Vec<ElementType> = mesh.element_types(TypeFilter::new().ghost(ghost)).collect();
    let mut written: Vec<(ElementType, u32)> = Vec::new();
    for &ty in &types {
        match akantu_to_msh(ty) {
            Some(code) => written.push((ty, code)),
            None => {
                log::warn!("MSH writer: {ty} elements have no Gmsh equivalent and are not written")
            }
        }
    }
    let total: usize = written
        .iter()
        .map(|&(t, _)| mesh.nb_element(t, ghost))
        .sum();
    out.push_str("$Elements\n");
    let _ = writeln!(out, "{total}");
    let int_tag = |name: &str, ty: ElementType| -> Option<Vec<i32>> {
        match mesh.data().elemental(name).ok()? {
            MeshDataValues::Int(m) => m.get(ty, ghost).ok().map(|a| a.as_slice().to_vec()),
            _ => None,
        }
    };
    // First Gmsh id of each written type (ids are 1-based and contiguous).
    let mut first_id: Vec<(ElementType, usize)> = Vec::new();
    let mut id = 1usize;
    for &(ty, code) in &written {
        first_id.push((ty, id));
        let conn = mesh.connectivity(ty, ghost)?;
        let tags = [int_tag("tag_0", ty), int_tag("tag_1", ty)];
        let order = write_order(ty, flavor);
        for (e, c) in conn.tuples().enumerate() {
            let _ = write!(out, "{id} {code} 2");
            for t in &tags {
                let v = t.as_ref().and_then(|t| t.get(e).copied()).unwrap_or(0);
                let _ = write!(out, " {v}");
            }
            for j in 0..c.len() {
                let n = c[order.map_or(j, |o| o[j])];
                let _ = write!(out, " {}", i64::from(n) + 1);
            }
            out.push('\n');
            id += 1;
        }
    }
    out.push_str("$EndElements\n");

    // Nodal data.
    for name in mesh.data().nodal_names() {
        match mesh.data().nodal(name)? {
            NodalDataValues::Real(a) => {
                data_header(&mut out, "$NodeData", name, a.nb_component(), a.len())?;
                for (i, v) in a.tuples().enumerate() {
                    let _ = write!(out, "{}", i + 1);
                    for &x in v {
                        let _ = write!(out, " {}", format_g(x, p, false));
                    }
                    out.push('\n');
                }
                out.push_str("$EndNodeData\n");
            }
            _ => log::warn!(
                "The field {name} is ignored by the MSH writer, msh files do not support non-real data"
            ),
        }
    }

    // Elemental and element-node data (UD-044).
    for name in mesh.data().elemental_names() {
        let MeshDataValues::Real(m) = mesh.data().elemental(name)? else {
            if !name.starts_with("tag_") && name != "physical_names" {
                log::warn!(
                    "The field {name} is ignored by the MSH writer, msh files do not support non-real data"
                );
            }
            continue;
        };
        let per_node = mesh.data().is_elemental_nodal(name);
        let mut blocks = Vec::new();
        let mut nc: Option<usize> = None;
        for &(ty, first) in &first_id {
            // absent block skipped; a protocol violation is returned
            let Some(a) = akantu_core::error::if_present(m.get(ty, ghost))? else {
                continue;
            };
            let nnpe = ty.nb_nodes_per_element();
            let ne = mesh.nb_element(ty, ghost);
            if a.len() != ne {
                return Err(Error::Mesh(format!(
                    "MSH writer: dataset '{name}' has {} {ty} tuples for {ne} elements",
                    a.len()
                )));
            }
            let comps = a.nb_component();
            let this_nc = if per_node {
                if nnpe == 0 || comps % nnpe != 0 {
                    return Err(Error::Mesh(format!(
                        "MSH writer: element-node dataset '{name}' has {comps} components per {ty} element"
                    )));
                }
                comps / nnpe
            } else {
                comps
            };
            if *nc.get_or_insert(this_nc) != this_nc {
                return Err(Error::Mesh(format!(
                    "MSH writer: dataset '{name}' has different numbers of components per element type"
                )));
            }
            blocks.push((ty, first, a));
        }
        let Some(nc) = nc else { continue };
        let size: usize = blocks.iter().map(|(_, _, a)| a.len()).sum();
        if size == 0 {
            continue;
        }
        let section = if per_node {
            "$ElementNodeData"
        } else {
            "$ElementData"
        };
        data_header(&mut out, section, name, nc, size)?;
        for (ty, first, a) in blocks {
            let nnpe = ty.nb_nodes_per_element();
            let order = write_order(ty, flavor);
            for (e, v) in a.tuples().enumerate() {
                let _ = write!(out, "{}", first + e);
                if per_node {
                    let _ = write!(out, " {nnpe}");
                    for j in 0..nnpe {
                        let s = order.map_or(j, |o| o[j]);
                        for &x in &v[s * nc..(s + 1) * nc] {
                            let _ = write!(out, " {}", format_g(x, p, false));
                        }
                    }
                } else {
                    for &x in v {
                        let _ = write!(out, " {}", format_g(x, p, false));
                    }
                }
                out.push('\n');
            }
        }
        let _ = writeln!(out, "$End{}", &section[1..]);
    }
    Ok(out)
}

/// Data-block header as upstream writes it: one string tag (the name), one
/// real tag (time 0.0), three integer tags (step 0, components, entries).
/// The name is quoted with `\\` escapes for `"` and `\\` so that the
/// reader's `std::quoted` extraction gives it back unchanged, boundary
/// quotes, blanks and backslashes included (upstream does not
/// escape: identical output for names without those characters). A name
/// with a line break cannot be represented in the line-oriented format and
/// is an error.
fn data_header(out: &mut String, section: &str, name: &str, nc: usize, size: usize) -> Result<()> {
    if name.contains(['\n', '\r']) {
        return Err(Error::invalid(format!(
            "MSH writer: dataset name {name:?} contains a line break, which a {section} \
             string tag cannot hold"
        )));
    }
    let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
    let _ = write!(
        out,
        "{section}\n1\n\"{escaped}\"\n1\n0.0\n3\n0\n{nc}\n{size}\n"
    );
    Ok(())
}
