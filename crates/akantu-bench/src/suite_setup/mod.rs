// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The set-up benchmark
// suite (no upstream counterpart; the cases exercise the ports of
// src/io/mesh_io, src/mesh_utils, src/mesh, src/synchronizer, src/io/parser
// and src/io/dumper).

//! `suite_setup`: timing cases of everything a run does besides the models'
//! own loops (meshes, partitioning and distribution, the MPI layer, the
//! parser, the dumpers) and a Rust twin of the Python benchmarks
//! (`python/benchmarks/`). See `crates/akantu-bench/README.md`.

pub mod cases_dist;
pub mod cases_io;
pub mod cases_mesh;
pub mod harness;
pub mod meshgen;

use akantu_core::GhostType;
use akantu_mesh::Mesh;

use harness::{Case, Digest, Kind};

/// Feed a mesh into a digest: nodes, then every connectivity (type order,
/// not-ghost then ghost), then the element groups (name order) with their
/// elements and nodes.
pub fn mesh_digest(m: &Mesh, d: &mut Digest) {
    d.reals(m.nodes().as_slice());
    for (ty, ghost, conn) in m.connectivities().as_map().iter_all() {
        d.bytes(format!("{ty:?}{ghost:?}").as_bytes());
        d.ints(conn.borrow().as_slice());
    }
    for g in m.groups().element_groups() {
        d.bytes(g.name().as_bytes());
        for ghost in [GhostType::NotGhost, GhostType::Ghost] {
            for ty in g.group().element_types(ghost) {
                d.ints(&g.elements_of(ty, ghost));
            }
        }
        d.ints(g.nodes().as_slice());
    }
}

/// Number of elements of the mesh's own dimension (not ghost).
pub fn nb_volume_elements(m: &Mesh) -> usize {
    let dim = m.spatial_dimension();
    m.element_types(akantu_core::TypeFilter::new().dim(dim))
        .map(|t| m.nb_element(t, GhostType::NotGhost))
        .sum()
}

const NO_SCOTCH: Option<&str> = if cfg!(feature = "scotch") {
    None
} else {
    Some("needs the `scotch` feature (Docker image akantu-rs-scotch)")
};

/// Every case of the suite, in `--list` order.
pub fn cases() -> Vec<Case> {
    use cases_dist as d;
    use cases_io as io;
    use cases_mesh as m;
    let serial = |name, description, run| Case {
        name,
        description,
        kind: Kind::Serial,
        unavailable: None,
        run,
    };
    let parallel = |name, description, run| Case {
        name,
        description,
        kind: Kind::Parallel,
        unavailable: None,
        run,
    };
    vec![
        serial(
            "msh22_read_tet4",
            "Gmsh MSH 2.2 ASCII reader on a Tet4 cube with physical groups: lexer, staging, groups from tags",
            m::msh22_read_tet4,
        ),
        serial(
            "msh41_read_tet4",
            "Gmsh MSH 4.1 ASCII reader (entities, node blocks) on the same Tet4 cube",
            m::msh41_read_tet4,
        ),
        serial(
            "msh22_read_hex8",
            "Gmsh MSH 2.2 ASCII reader on a Hex8 cube (fewer, larger elements per node)",
            m::msh22_read_hex8,
        ),
        serial(
            "msh_write_tet4",
            "Gmsh MSH 2.2 writer (17-digit numbers) of the Tet4 cube to a string",
            m::msh_write_tet4,
        ),
        serial(
            "facets_tet4",
            "node-to-element CSR, element-to-subelement adjacency and the facet mesh (init_mesh_facets) of a Tet4 cube",
            m::facets_tet4,
        ),
        serial(
            "facets_hex8",
            "the same set-up steps on a Hex8 cube (Quad4 facets)",
            m::facets_hex8,
        ),
        serial(
            "groups_tet4",
            "node and element groups: filtered node groups, element groups from node groups, groups from mesh data",
            m::groups_tet4,
        ),
        serial(
            "erase_purify_tet4",
            "element removal set-up (a third of the elements) and purify_mesh (unused nodes removed, renumbering)",
            m::erase_purify_tet4,
        ),
        serial(
            "periodic_tet4",
            "make_periodic in x, y and z on a Tet4 cube (node pairing on the faces)",
            m::periodic_tet4,
        ),
        serial(
            "cohesive_insert_tet4",
            "cohesive insertion topology on every facet of a slab (facet doubling, node doubling, new elements)",
            m::cohesive_insert_tet4,
        ),
        serial(
            "partition_rcb_tet4",
            "RCB partitioner into 8 parts (barycentres, sorting) of a Tet4 cube",
            m::partition_rcb_tet4,
        ),
        Case {
            unavailable: NO_SCOTCH,
            ..serial(
                "partition_scotch_tet4",
                "Scotch partitioner into 8 parts: dual graph and the Scotch call",
                m::partition_scotch_tet4,
            )
        },
        parallel(
            "dist_mesh_tet4",
            "partition on rank 0 (Scotch if built, else RCB) and centralised distribution to every rank",
            d::dist_mesh_tet4,
        ),
        parallel(
            "dist_facets_tet4",
            "init_mesh_facets on a distributed mesh: facet synchronizer set-up and global facet numbering",
            d::dist_facets_tet4,
        ),
        parallel(
            "ghost_nodes_tet4",
            "node synchronizer: slave reduction then synchronize of 3 reals per shared node (the residual exchange)",
            d::ghost_nodes_tet4,
        ),
        parallel(
            "ghost_elements_tet4",
            "element synchronizer: owner to ghost exchange of 9 reals per element (stress-sized)",
            d::ghost_elements_tet4,
        ),
        parallel(
            "reduce_scalars",
            "all_reduce of 2 reals (the per-step energies) and of 4096 reals",
            d::reduce_scalars,
        ),
        parallel(
            "explicit_step_dist_tet4",
            "distributed explicit elastic solve_step (the exchanges inside a real step); also runs serially",
            d::explicit_step_dist_tet4,
        ),
        serial(
            "parser_dat",
            "parse a .dat input of 2000 sections with expressions, then evaluate every parameter",
            io::parser_dat,
        ),
        serial(
            "dump_paraview_tet4",
            "ParaView dump of an elastic model (displacement, velocity, stress, strain): VTU per dump",
            io::dump_paraview_tet4,
        ),
        serial(
            "dump_paraview_ascii_tet4",
            "the same ParaView dump in text mode (ASCII number formatting)",
            io::dump_paraview_ascii_tet4,
        ),
        serial(
            "dump_text_tet4",
            "text dumper of nodal fields of the same model",
            io::dump_text_tet4,
        ),
        serial(
            "py_twin_explicit_tri3",
            "Rust twin of python/benchmarks: explicit elastic Tri3 square, the same loop as the Python case",
            io::py_twin_explicit_tri3,
        ),
    ]
}
