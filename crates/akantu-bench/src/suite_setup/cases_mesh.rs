// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Mesh cases of the
// set-up suite: the ports of src/io/mesh_io/mesh_io_msh.cc,
// src/mesh_utils/mesh_utils.cc, src/mesh/group_manager.cc,
// src/mesh/mesh_periodic.cc, src/mesh_utils/cohesive_element_inserter.cc,
// src/mesh_utils/mesh_partition*.cc.

//! Serial mesh cases. Every mutating operation is a set-up phase, timed
//! once per repetition on a freshly read mesh (`--reps` gives the samples).

use std::path::PathBuf;

use akantu_cohesive::CohesiveElementInserter;
use akantu_core::enums::{MeshIoType, SpatialDirection};
use akantu_core::{Element, ElementType, Error, GhostType, Idx, Result, TypeFilter};
use akantu_io::WriteOptions;
use akantu_mesh::partition::MeshPartitioner;
use akantu_mesh::{Mesh, SharedMesh};

use super::harness::{CaseOut, Ctx, Digest};
use super::meshgen::{self, Grid};
use super::{mesh_digest, nb_volume_elements};

/// Cells per side of the Tet4 cube: 6 n³ tetrahedra (s: 1,296; m:
/// 162,000; l: 1,296,000).
pub fn tet4_n(ctx: &Ctx) -> usize {
    ctx.size.pick(6, 30, 60)
}

/// Cells per side of the Hex8 cube (s: 512; m: 125,000; l: 1,000,000).
fn hex8_n(ctx: &Ctx) -> usize {
    ctx.size.pick(8, 50, 100)
}

/// A per-process scratch directory (removed by [`Scratch`]'s drop).
pub struct Scratch(pub PathBuf);

impl Scratch {
    /// Create `$TMPDIR/akantu-suite-setup-<pid>-<tag>`.
    pub fn new(tag: &str) -> Result<Self> {
        let p =
            std::env::temp_dir().join(format!("akantu-suite-setup-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&p).map_err(|source| Error::Io {
            path: p.display().to_string(),
            source,
        })?;
        Ok(Self(p))
    }
    /// Write a file into the directory.
    pub fn write(&self, name: &str, text: &str) -> Result<PathBuf> {
        let p = self.0.join(name);
        std::fs::write(&p, text).map_err(|source| Error::Io {
            path: p.display().to_string(),
            source,
        })?;
        Ok(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn counts(out: &mut CaseOut, m: &Mesh) {
    out.n_elements = nb_volume_elements(m);
    out.n_nodes = m.nb_nodes();
}

fn read_case(ctx: &Ctx, grid: Grid, n: usize, v41: bool) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let dir = Scratch::new("read")?;
    let path = out.phase("generate", || {
        let text = if v41 {
            meshgen::msh41(grid, n)
        } else {
            meshgen::msh22(grid, n)
        };
        dir.write("cube.msh", &text)
    })?;
    let _ = ctx;
    let mut mesh = Mesh::new(grid.dim(), "mesh")?;
    out.phase("read", || {
        akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh)
    })?;
    counts(&mut out, &mesh);
    let mut d = Digest::new();
    mesh_digest(&mesh, &mut d);
    out.digest = d.value();
    Ok(out)
}

/// MSH 2.2 reader, Tet4 cube.
pub fn msh22_read_tet4(ctx: &Ctx) -> Result<CaseOut> {
    read_case(ctx, Grid::Tet4, tet4_n(ctx), false)
}

/// MSH 4.1 reader, Tet4 cube.
pub fn msh41_read_tet4(ctx: &Ctx) -> Result<CaseOut> {
    read_case(ctx, Grid::Tet4, tet4_n(ctx), true)
}

/// MSH 2.2 reader, Hex8 cube.
pub fn msh22_read_hex8(ctx: &Ctx) -> Result<CaseOut> {
    read_case(ctx, Grid::Hex8, hex8_n(ctx), false)
}

/// MSH writer.
pub fn msh_write_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mesh = out.phase("mesh", || meshgen::mesh(Grid::Tet4, tet4_n(ctx), "mesh"))?;
    let text = out.phase("write", || {
        akantu_io::write_mesh_to_string(
            &mesh,
            std::path::Path::new("cube.msh"),
            MeshIoType::Gmsh,
            &WriteOptions::default(),
        )
    })?;
    counts(&mut out, &mesh);
    out.digest = Digest::new().bytes(text.as_bytes()).value();
    Ok(out)
}

fn facets_case(ctx: &Ctx, grid: Grid, n: usize) -> Result<CaseOut> {
    let _ = ctx;
    let mut out = CaseOut::default();
    let mut mesh = out.phase("mesh", || meshgen::mesh(grid, n, "mesh"))?;
    let csr = out.phase("node_to_elements", || {
        akantu_mesh::utils::build_node_to_elements(&mesh, None)
    })?;
    drop(csr);
    out.phase("element_to_subelements", || {
        akantu_mesh::utils::fill_element_to_subelements_data(&mut mesh)
    })?;
    out.phase("init_mesh_facets", || {
        akantu_mesh::utils::init_mesh_facets(&mut mesh).map(|_| ())
    })?;
    counts(&mut out, &mesh);
    let mut d = Digest::new();
    mesh_digest(&mesh, &mut d);
    let fm = mesh
        .mesh_facets()
        .ok_or_else(|| Error::invalid("no facet mesh"))?;
    mesh_digest(fm, &mut d);
    let nb_facets: usize = fm
        .element_types(TypeFilter::new().dim(grid.dim() - 1))
        .map(|t| fm.nb_element(t, GhostType::NotGhost))
        .sum();
    #[allow(clippy::cast_precision_loss)] // a count
    out.extra.push(("n_facets", nb_facets as f64));
    out.digest = d.value();
    Ok(out)
}

/// Facet set-up, Tet4.
pub fn facets_tet4(ctx: &Ctx) -> Result<CaseOut> {
    facets_case(ctx, Grid::Tet4, tet4_n(ctx))
}

/// Facet set-up, Hex8.
pub fn facets_hex8(ctx: &Ctx) -> Result<CaseOut> {
    facets_case(ctx, Grid::Hex8, hex8_n(ctx))
}

/// Groups.
pub fn groups_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mut mesh = out.phase("mesh", || meshgen::mesh(Grid::Tet4, tet4_n(ctx), "mesh"))?;
    let body_nodes = {
        let g = mesh.groups().element_group("body")?;
        mesh.groups()
            .node_group_by_id(g.group().node_group_id())?
            .name()
            .to_owned()
    };
    let xs: Vec<f64> = mesh
        .nodes()
        .as_slice()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|x| x[0])
        .collect();
    out.phase("filtered_node_group", || {
        mesh.groups_mut()
            .create_filtered_node_group("x_low", &body_nodes, |n| {
                usize::try_from(n).is_ok_and(|n| xs[n] < 0.5)
            })
    })?;
    out.phase("element_group_from_node_group", || {
        mesh.create_element_group_from_node_group(
            "x_low_elements",
            "x_low",
            akantu_mesh::group::ALL_DIMENSIONS,
        )
    })?;
    out.phase("groups_from_mesh_data", || {
        mesh.create_groups_from_mesh_data("tag_1")
    })?;
    counts(&mut out, &mesh);
    let mut d = Digest::new();
    mesh_digest(&mesh, &mut d);
    out.digest = d.value();
    Ok(out)
}

/// Element removal and purify.
pub fn erase_purify_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mut mesh = out.phase("mesh", || meshgen::mesh(Grid::Tet4, tet4_n(ctx), "mesh"))?;
    let nb = mesh.nb_element(ElementType::Tetrahedron4, GhostType::NotGhost);
    let erase: Vec<Element> = (0..nb)
        .step_by(3)
        .map(|e| Element::try_new(ElementType::Tetrahedron4, e, GhostType::NotGhost))
        .collect::<Result<_>>()?;
    counts(&mut out, &mesh);
    out.phase("erase_elements", || {
        akantu_mesh::removal::erase_elements_setup(&mut mesh, &erase).map(|_| ())
    })?;
    out.phase("purify_mesh", || {
        akantu_mesh::utils::purify_mesh(&mut mesh).map(|_| ())
    })?;
    let mut d = Digest::new();
    mesh_digest(&mesh, &mut d);
    out.digest = d.value();
    Ok(out)
}

/// Periodicity.
pub fn periodic_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mut mesh = out.phase("mesh", || {
        let mut m = meshgen::mesh(Grid::Tet4, tet4_n(ctx), "mesh")?;
        m.compute_bounding_box()?;
        Ok(m)
    })?;
    out.phase("make_periodic", || {
        for dir in [
            SpatialDirection::X,
            SpatialDirection::Y,
            SpatialDirection::Z,
        ] {
            let _ = akantu_mesh::periodic::make_periodic(&mut mesh, dir, None)?;
        }
        Ok(())
    })?;
    counts(&mut out, &mesh);
    let mut pairs = akantu_mesh::periodic::periodic_pairs(&mesh);
    pairs.sort_unstable();
    let mut d = Digest::new();
    for (a, b) in &pairs {
        d.ints(&[*a, *b]);
    }
    #[allow(clippy::cast_precision_loss)] // a count
    out.extra.push(("n_pairs", pairs.len() as f64));
    out.digest = d.value();
    Ok(out)
}

/// Cohesive insertion on the facets of the slab 0.25 ≤ x ≤ 0.75.
pub fn cohesive_insert_tet4(ctx: &Ctx) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mesh = out.phase("mesh", || meshgen::mesh(Grid::Tet4, tet4_n(ctx), "mesh"))?;
    counts(&mut out, &mesh);
    let shared = SharedMesh::new(mesh);
    out.phase("init_mesh_facets", || {
        shared
            .setup(akantu_mesh::utils::init_mesh_facets)
            .map(|_| ())
    })?;
    let mut inserter = out.phase("flag_facets", || {
        let m = shared.read()?;
        let mut ins = CohesiveElementInserter::new(&m, "inserter")?;
        ins.set_limit(0, 0.25, 0.75)?;
        ins.flag_intrinsic_facets(&m)?;
        Ok(ins)
    })?;
    let mut result = None;
    out.phase("insert", || {
        shared.mutate(
            &mut [],
            |tx| {
                result = Some(inserter.insert_elements_in(tx, false)?);
                Ok(())
            },
            None,
        )
    })?;
    out.phase("update_insertion_facets", || {
        let m = shared.read()?;
        inserter.update_insertion_facets(&m)
    })?;
    let r = result.ok_or_else(|| Error::invalid("no insertion"))?;
    let m = shared.read()?;
    let mut d = Digest::new();
    mesh_digest(&m, &mut d);
    #[allow(clippy::cast_precision_loss)] // counts
    {
        out.extra.push(("n_cohesive", r.new_elements.len() as f64));
        out.extra
            .push(("n_doubled_nodes", r.doubled_nodes.len() as f64));
    }
    out.digest = d.value();
    Ok(out)
}

fn partition_digest(p: &akantu_mesh::partition::MeshPartition) -> u64 {
    let mut d = Digest::new();
    for (ty, ghost, parts) in p.partitions.as_map().iter_all() {
        d.bytes(format!("{ty:?}{ghost:?}").as_bytes());
        d.ints(parts.as_slice());
    }
    d.value()
}

fn partition_case(ctx: &Ctx, partitioner: &mut dyn MeshPartitioner) -> Result<CaseOut> {
    let mut out = CaseOut::default();
    let mut mesh = out.phase("mesh", || meshgen::mesh(Grid::Tet4, tet4_n(ctx), "mesh"))?;
    // the distribution path computes the adjacency before partitioning
    akantu_mesh::utils::fill_element_to_subelements_data(&mut mesh)?;
    let p = out.phase("partition", || partitioner.partition(&mesh, 8))?;
    counts(&mut out, &mesh);
    out.digest = partition_digest(&p);
    Ok(out)
}

/// RCB partition.
pub fn partition_rcb_tet4(ctx: &Ctx) -> Result<CaseOut> {
    partition_case(ctx, &mut akantu_mesh::partition::RcbPartitioner)
}

/// Scotch partition.
pub fn partition_scotch_tet4(ctx: &Ctx) -> Result<CaseOut> {
    #[cfg(feature = "scotch")]
    {
        partition_case(ctx, &mut akantu_mesh::scotch::ScotchPartitioner::new(0))
    }
    #[cfg(not(feature = "scotch"))]
    {
        let _ = ctx;
        Err(Error::invalid("needs the `scotch` feature"))
    }
}

/// The Idx of a count (helper for the other modules).
pub fn idx(n: usize) -> Result<Idx> {
    akantu_core::try_idx(n)
}
