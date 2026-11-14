// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). D-105 /
// UD-1280: the Neumann load of doubled nodes on boundary
// elements whose nodal shares differ, on upstream's cohesive test meshes
// (test/test_model/test_solid_mechanics_model/test_cohesive/
// _cohesive_3d_8_hexahedron_8.msh, _cohesive_2d_6_triangle_6.msh,
// _cohesive_3d_12_tetrahedron_10.msh, _cohesive_3d_6_tetrahedron_4.msh: the
// interface "insertion" at y = 0 reaching the faces "sides"), the Neumann
// application of src/model/common/boundary_condition/
// boundary_condition_tmpl.hh and the boundary-row rewrite of
// src/mesh/mesh.cc:118-140 (`Mesh::sendEvent<NewNodesEvent>`).

//! When the cohesive insertion doubles a node, upstream's node event first
//! gives every boundary element the connectivity of its facet: most rows are
//! rewritten, mostly as permutations of the same nodes. The Neumann
//! store (D-105) keeps each element's load per node, so it must follow the
//! rows by node identity: where an element's nodal shares differ, following
//! them by slot moves part of another node's share to the new copy.
//!
//! Cases (each at 1, 2 and 4 ranks of the local world, recursive coordinate
//! bisection, the facets built with the corrected flip as the distributed
//! cohesive tests):
//! * `_quadrangle_4` boundaries of a hexahedral mesh whose nodes are moved by
//!   a smooth non-linear map (non-affine faces: the four shares of a
//!   constant traction differ);
//! * `_segment_3` boundaries of a `_triangle_6` mesh, `_triangle_6`
//!   boundaries of a `_tetrahedron_10` mesh, under a traction that depends
//!   on the position (unequal shares on straight / flat elements);
//! * `_triangle_3` boundaries of a `_tetrahedron_4` mesh under the same
//!   position-dependent traction: with the default one-point rule the three
//!   shares are equal whatever the traction (`N` at the centroid), so the
//!   case can only show rounding; it is kept as the benchmark's element type.
//!
//! Scenario: the traction on "sides"; half of the interface inserted (the
//! facets with a barycentre at x < 0: the first doubling rewrites the
//! boundary rows); the traction applied again (an increment recorded on the
//! rewritten rows); the rest of the interface inserted (moves of records
//! taken before and after the rewrite).
//!
//! Asserted with the corrected default (D-105), relative to the largest
//! nodal load of the serial run, C1F-1's bound 1e-14 (rounding: a nodal
//! force is a sum of a few element contributions per application, two
//! applications):
//! 1. on every rank, at **every** node it holds (local, master, slave, pure
//!    ghost), the external force equals a **re-application** of the same
//!    loads on the final topology with a boundary FE engine built afresh on
//!    the final mesh (an independent computation: the store, its moves and
//!    the model's engine play no part);
//! 2. at 2 and 4 ranks the owned copies (matched by reference position and
//!    side) carry the serial run's external force;
//! 3. the store's records sum to the external force at every node;
//! 4. non-vacuous: copies with a load were created, records moved, and
//!    loaded boundary rows were rewritten as permutations that hold a new
//!    copy (the case UD-1280 is about).
//!
//! With the switch `doubled_node_keeps_external_force` (upstream's
//! placement), serial: the comparison of 1 fails by a whole share (the power
//! of the comparison).
//!
//! The model's boundary FE engine is recomputed on the current rows after
//! each insertion ([`refresh_boundary_engine`]): the engine keeps the
//! jacobians of a rewritten row's old node order (upstream too), which on
//! non-affine elements misdistributes an application made after the
//! rewrite (measured without the refresh: 1.7e-3 of the largest nodal load
//! for the hexahedra, 4e-13 for the `_tetrahedron_10` mesh and 1.3e-13
//! for the `_triangle_6` mesh, whose planar quadratic boundaries are affine
//! only up to the mesh files' coordinate noise). That is the engine's
//! precomputed data, not the store: kept out of these tests and registered
//! as UD-1296 (category C, D-117).

mod common;

use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use akantu_cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{
    Array, Element, ElementKind, ElementType, GhostType, Idx, Real, Result, SharedArray, TypeFilter,
};
use akantu_fe::FeEngine;
use akantu_fe::IntegrationPoint;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FromTraction, NeumannFunctor, apply_neumann};
use akantu_parser::Parser;
use common::cohesive_mesh;

/// C1F-1 (`docs/NUMERICAL_VALIDATION.md`): summation order only.
const RTOL: Real = 1e-14;
const LOADED: &str = "sides";

/// Upstream's fixture materials with the bilinear law (an elastic branch:
/// insertion outside the extrinsic check is allowed, UD-224) and an
/// insertion box outside the mesh: the intrinsic initialisation inserts
/// nothing, the test flags the interface itself (the extrinsic stress check
/// is not available for hexahedra: elemental field interpolation is
/// `AKANTU_TO_IMPLEMENT` upstream).
const MATERIAL: &str = "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
    bounding_box = [[10, 11], [10, 11], [10, 11]]
  ]
  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
  material cohesive_bilinear [
    name = insertion
    beta = 1
    G_c = 10
    sigma_c = 1e6
    delta_0 = 1e-7
  ]
]
";

/// A traction that depends on the position: `t0 · (1 + 0.6 y + 0.3 x + 0.4 z)`.
struct Linear(Vec<Real>);

impl NeumannFunctor for Linear {
    fn apply(
        &self,
        _quad_point: &IntegrationPoint,
        dual: &mut [Real],
        coord: &[Real],
        _normals: &[Real],
    ) -> Result<()> {
        let z = coord.get(2).copied().unwrap_or(0.0);
        let s = 1.0 + 0.6 * coord[1] + 0.3 * coord[0] + 0.4 * z;
        for (d, t) in dual.iter_mut().zip(&self.0) {
            *d = t * s;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum Load {
    Constant,
    Linear,
}

#[derive(Clone, Copy, Debug)]
struct Case {
    name: &'static str,
    mesh: &'static str,
    dim: usize,
    load: Load,
    /// Move the nodes by a smooth non-linear map (non-affine faces).
    distort: bool,
    /// Reverse the node order of the loaded boundary elements on reading
    /// (the frozen mesh's rows already follow its facets', so without this
    /// the insertion would rewrite none of them).
    reverse: bool,
}

const HEX8: Case = Case {
    name: "hexahedron_8 / quadrangle_4 (non-affine, constant traction)",
    mesh: "_cohesive_3d_8_hexahedron_8.msh",
    dim: 3,
    load: Load::Constant,
    distort: true,
    reverse: false,
};
const TRI6: Case = Case {
    name: "triangle_6 / segment_3 (position-dependent traction)",
    mesh: "_cohesive_2d_6_triangle_6.msh",
    dim: 2,
    load: Load::Linear,
    distort: false,
    reverse: true,
};
const TET10: Case = Case {
    name: "tetrahedron_10 / triangle_6 (position-dependent traction)",
    mesh: "_cohesive_3d_12_tetrahedron_10.msh",
    dim: 3,
    load: Load::Linear,
    distort: false,
    reverse: false,
};
const TET4: Case = Case {
    name: "tetrahedron_4 / triangle_3 (position-dependent traction, one-point rule)",
    mesh: "_cohesive_3d_6_tetrahedron_4.msh",
    dim: 3,
    load: Load::Linear,
    distort: false,
    reverse: false,
};

impl Case {
    fn functor(&self) -> Box<dyn NeumannFunctor> {
        let t = [2.0e5, -3.0e5, 1.0e5][..self.dim].to_vec();
        match self.load {
            Load::Constant => Box::new(FromTraction::new(t)),
            Load::Linear => Box::new(Linear(t)),
        }
    }
}

/// (reference position bits, side key: the wrapping sum of the hashes of
/// the node's attached regular elements).
type Key = (Vec<u64>, u64);

fn hash_words(words: impl IntoIterator<Item = u64>) -> u64 {
    // FNV-1a over the words' bytes
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for w in words {
        for b in w.to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

fn pos_bits(x: &[Real], dim: usize, n: usize) -> Vec<u64> {
    x[n * dim..(n + 1) * dim]
        .iter()
        .map(|v| v.to_bits())
        .collect()
}

fn regular_types(m: &Mesh, ghost: GhostType) -> Vec<ElementType> {
    m.element_types(TypeFilter {
        spatial_dimension: Some(m.spatial_dimension()),
        ghost_type: ghost,
        kind: Some(ElementKind::Regular),
    })
    .collect()
}

fn node_keys(m: &Mesh) -> Result<Vec<Key>> {
    let dim = m.spatial_dimension();
    let x = m.nodes();
    let x = x.as_slice();
    let mut side = vec![0_u64; m.nb_nodes()];
    for g in GhostType::ALL {
        for ty in regular_types(m, g) {
            let conn = m.connectivity(ty, g)?;
            let npe = conn.nb_component();
            for nodes in conn.as_slice().chunks_exact(npe) {
                let mut p: Vec<Vec<u64>> = nodes
                    .iter()
                    .map(|&n| pos_bits(x, dim, n as usize))
                    .collect();
                p.sort_unstable();
                let h = hash_words(p.into_iter().flatten());
                for &n in nodes {
                    side[n as usize] = side[n as usize].wrapping_add(h);
                }
            }
        }
    }
    Ok((0..m.nb_nodes())
        .map(|n| (pos_bits(x, dim, n), side[n]))
        .collect())
}

/// The positions of the loaded boundary elements' nodes, slot by slot.
fn loaded_rows(m: &Mesh) -> Result<HashMap<Element, Vec<Vec<u64>>>> {
    let dim = m.spatial_dimension();
    let x = m.nodes();
    let x = x.as_slice();
    let g = m.groups().element_group(LOADED)?;
    let mut out = HashMap::new();
    for ghost in GhostType::ALL {
        for ty in g.element_types(ghost) {
            let conn = m.connectivity(ty, ghost)?;
            for &e in g.elements_of(ty, ghost).iter() {
                let row = conn
                    .tuple(e as usize)
                    .iter()
                    .map(|&n| pos_bits(x, dim, n as usize))
                    .collect();
                out.insert(Element::new(ty, e, ghost), row);
            }
        }
    }
    Ok(out)
}

/// The smooth non-linear map of the hexahedral case: the planes x = ±1,
/// y = 0, y = ±1 and z = ±0.5 stay planes (the interface and the loaded
/// faces stay where they are), every face inside them bends: the
/// quadrangles of "sides" are no longer parallelograms.
fn distort(m: &mut Mesh) {
    let mut a = MeshAccessor::new(m);
    let mut x = a.nodes_mut();
    for p in x.as_mut_slice().as_chunks_mut::<3>().0 {
        let (px, py, pz) = (p[0], p[1], p[2]);
        p[1] = py + 0.24 * py * (1.0 - py * py) * (pz + 0.5) * (1.0 + 0.5 * px);
        p[2] = pz + 0.2 * (1.0 - 4.0 * pz * pz) * py * (1.0 + 0.3 * px);
    }
}

/// Reverse the corner order of the loaded segments (`[a, b, m]` →
/// `[b, a, m]`: the same element in the other orientation).
fn reverse_loaded_rows(m: &mut Mesh) -> Result<()> {
    let list: Vec<(ElementType, Vec<Idx>)> = {
        let g = m.groups().element_group(LOADED)?;
        g.element_types(GhostType::NotGhost)
            .into_iter()
            .map(|ty| (ty, g.elements_of(ty, GhostType::NotGhost).to_vec()))
            .collect()
    };
    let mut a = MeshAccessor::new(m);
    for (ty, els) in list {
        let mut c = a.connectivity_mut(ty, GhostType::NotGhost);
        for e in els {
            c.tuple_mut(e as usize).swap(0, 1);
        }
    }
    Ok(())
}

/// Recompute the shapes, jacobians and normals of the model's boundary FE
/// engine on the current rows. The insertion rewrites boundary rows and the
/// engine keeps the data of the old node order (upstream as well: its FE
/// engine has no handler of the node event), so an application made after
/// a rewrite would integrate non-affine elements with the jacobians of the
/// old order: a separate matter (UD-1296, category C, D-117), kept out of
/// these tests so that they see the placement only.
fn refresh_boundary_engine(
    mesh: &SharedMesh,
    model: &mut SolidMechanicsModelCohesive,
) -> Result<()> {
    let m = mesh.read()?;
    for (name, fe) in model.smm.physics.base.fems.iter_mut() {
        if name.ends_with(":boundary") {
            for ghost in GhostType::ALL {
                fe.init_shape_functions(&m, ghost)?;
                fe.compute_normals_on_integration_points(&m, ghost)?;
            }
        }
    }
    Ok(())
}

fn setup(comm: &Communicator, case: Case) -> Result<SharedMesh> {
    use akantu_mesh::facet_synchronizer::DistributedFacetsOptions;
    use akantu_mesh::partition::{MeshPartitioner, RcbPartitioner};
    use akantu_mesh::utils::{FacetFlipMode, init_mesh_facets_with};
    let mut m = Mesh::with_communicator(case.dim, "mesh", comm.clone())?;
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut m, &cohesive_mesh(case.mesh), MeshIoType::Gmsh)?;
        if case.distort {
            distort(&mut m);
        }
        if case.reverse {
            reverse_loaded_rows(&mut m)?;
        }
    }
    if comm.size() > 1 {
        m.compute_bounding_box()?;
        let mut p = RcbPartitioner;
        let _ = akantu_mesh::distribution::distribute_with(
            &mut m,
            Some(&mut p as &mut dyn MeshPartitioner),
            0,
        )?;
        let _ = init_mesh_facets_with(
            &mut m,
            DistributedFacetsOptions {
                flip: FacetFlipMode::RemoteOrder,
            },
        )?;
    }
    Ok(SharedMesh::new(m))
}

fn model(mesh: &SharedMesh, dim: usize, keep: bool) -> Result<SolidMechanicsModelCohesive> {
    let parser = Parser::parse_str(MATERIAL, "material.dat")?;
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        Some(dim),
        "solid_mechanics_model_cohesive",
        parser,
    )?;
    model.ext.compat = CohesiveUpstreamCompat::default();
    model.ext.compat.doubled_node_keeps_external_force = keep;
    model.init_full(
        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        false,
    )?;
    Ok(model)
}

/// Flag the facets of the interface (barycentre at y = 0) whose barycentre
/// satisfies `pick(x)`, local and ghost alike, and insert them; returns the
/// number of cohesive elements of the rank.
fn insert_where(
    mesh: &SharedMesh,
    model: &mut SolidMechanicsModelCohesive,
    pick: impl Fn(Real) -> bool,
) -> Result<usize> {
    {
        let m = mesh.read()?;
        let dim = m.spatial_dimension();
        let fm = m
            .mesh_facets()
            .ok_or_else(|| akantu_core::Error::invalid("no facet mesh"))?;
        let (_, insertion) = model.ext.inserter.flags_mut();
        let mut b = vec![0.0; dim];
        for ghost in GhostType::ALL {
            let types: Vec<ElementType> = fm
                .element_types(TypeFilter {
                    spatial_dimension: Some(dim - 1),
                    ghost_type: ghost,
                    kind: Some(ElementKind::Regular),
                })
                .collect();
            for ty in types {
                let ins = insertion.get_mut(ty, ghost)?.as_mut_slice();
                for (f, flag) in ins.iter_mut().enumerate() {
                    fm.barycenter_into(Element::new(ty, f as Idx, ghost), &mut b)?;
                    if b[1].abs() < 1e-9 && pick(b[0]) {
                        *flag = true;
                    }
                }
            }
        }
    }
    model.insert_elements()
}

/// `functor` applied `times` times on `LOADED` into a zeroed array with the
/// boundary FE engine `fe`.
fn reapply(
    m: &Mesh,
    fe: &FeEngine,
    functor: &dyn NeumannFunctor,
    times: usize,
) -> Result<Vec<Real>> {
    let dim = m.spatial_dimension();
    let mut scratch = SharedArray::new(Array::try_zeros(m.nb_nodes(), dim)?);
    let group = m.groups().element_group(LOADED)?;
    for _ in 0..times {
        apply_neumann(functor, m, group, fe, dim, &mut scratch)?;
    }
    let out = scratch.read()?.as_slice().to_vec();
    Ok(out)
}

#[derive(Debug, Default)]
struct RankOut {
    /// owned nodes: key -> (new copy, f, fresh re-application)
    owned: Vec<(Key, bool, Vec<Real>, Vec<Real>)>,
    /// max over every node of the rank |f − f_fresh| (absolute)
    diff_fresh: Real,
    /// max over every node |f − f_model| (the model's own boundary engine)
    diff_model: Real,
    /// max over every node |f − Σ records| (the store's rows read with the
    /// mesh's rows: right after a move they coincide)
    record_residual: Real,
    /// largest |f_fresh| over the owned nodes
    fmax_owned: Real,
    /// owned new copies with a load
    doubled_loaded: usize,
    /// loaded boundary elements of the rank (local and ghost) whose row was
    /// rewritten as another order of the same positions / of them holding a
    /// new copy
    permuted: usize,
    permuted_with_copy: usize,
    inserted: usize,
    moved: u64,
    released: u64,
}

fn rank_case(comm: &Communicator, case: Case, keep: bool) -> Result<RankOut> {
    let mesh = setup(comm, case)?;
    let mut model = model(&mesh, case.dim, keep)?;
    let functor = case.functor();
    let n0 = mesh.read()?.nb_global_nodes();
    let rows_before = loaded_rows(&*mesh.read()?)?;
    model.smm.apply_neumann_bc(functor.as_ref(), LOADED)?;
    let mut out = RankOut {
        inserted: insert_where(&mesh, &mut model, |x| x < 0.0)?,
        ..RankOut::default()
    };
    refresh_boundary_engine(&mesh, &mut model)?;
    // an increment recorded on the rewritten rows
    model.smm.apply_neumann_bc(functor.as_ref(), LOADED)?;
    out.inserted += insert_where(&mesh, &mut model, |_| true)?;
    refresh_boundary_engine(&mesh, &mut model)?;
    let m = mesh.read()?;
    let dim = case.dim;
    let f = model.smm.external_force()?.as_slice().to_vec();
    let fresh = FeEngine::with_precomputation(
        &m,
        Some(dim - 1),
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "reapply:fresh",
    );
    let mut fresh = fresh?;
    fresh.compute_normals_on_integration_points(&m, GhostType::NotGhost)?;
    fresh.compute_normals_on_integration_points(&m, GhostType::Ghost)?;
    let f_fresh = reapply(&m, &fresh, functor.as_ref(), 2)?;
    let own = model.smm.physics.base.fems.existing_boundary("")?;
    let f_model = reapply(&m, own, functor.as_ref(), 2)?;
    // the records, read with the mesh's rows
    let mut sum = vec![0.0; f.len()];
    if let Some(st) = model.smm.neumann_store() {
        out.moved = st.stats().moved_contributions;
        out.released = released(&st.stats());
        for ghost in GhostType::ALL {
            for ty in st.forces().element_types(TypeFilter {
                spatial_dimension: None,
                ghost_type: ghost,
                kind: None,
            }) {
                let conn = m.connectivity(ty, ghost)?;
                let block = st.forces().get(ty, ghost)?;
                for e in 0..block.len() {
                    for (k, &n) in conn.tuple(e).iter().enumerate() {
                        for d in 0..dim {
                            sum[dim * n as usize + d] += block.tuple(e)[dim * k + d];
                        }
                    }
                }
            }
        }
    }
    let keys = node_keys(&m)?;
    for (n, key) in keys.into_iter().enumerate() {
        let r = n * dim..(n + 1) * dim;
        for k in r.clone() {
            out.diff_fresh = out.diff_fresh.max((f[k] - f_fresh[k]).abs());
            out.diff_model = out.diff_model.max((f[k] - f_model[k]).abs());
            out.record_residual = out.record_residual.max((f[k] - sum[k]).abs());
        }
        let ni = n as Idx;
        if !m.is_local_or_master_node(ni)? {
            continue;
        }
        let new = usize::try_from(m.node_global_id(ni)).is_ok_and(|g| g >= n0);
        let loaded = f_fresh[r.clone()].iter().any(|v| *v != 0.0);
        out.doubled_loaded += usize::from(new && loaded);
        for v in &f_fresh[r.clone()] {
            out.fmax_owned = out.fmax_owned.max(v.abs());
        }
        out.owned
            .push((key, new, f[r.clone()].to_vec(), f_fresh[r].to_vec()));
    }
    // rewritten loaded rows
    let rows_after = loaded_rows(&m)?;
    for (el, before) in &rows_before {
        let Some(after) = rows_after.get(el) else {
            continue;
        };
        if after != before {
            out.permuted += 1;
            let conn = m.connectivity(el.ty(), el.ghost_type())?;
            if conn
                .tuple(el.idx())
                .iter()
                .any(|&n| usize::try_from(m.node_global_id(n)).is_ok_and(|g| g >= n0))
            {
                out.permuted_with_copy += 1;
            }
        }
    }
    Ok(out)
}

/// The store's released entries where the counter exists; zero
/// otherwise.
fn released(s: &akantu_model::neumann_store::NeumannStoreStats) -> u64 {
    s.released_entries
}

fn on(np: usize, case: Case, keep: bool) -> Vec<RankOut> {
    run_local(np, Duration::from_secs(600), |c| rank_case(&c, case, keep))
        .into_iter()
        .enumerate()
        .map(|(r, o)| o.unwrap_or_else(|e| panic!("{}: np {np} rank {r}: {e}", case.name)))
        .collect()
}

type ByKey = BTreeMap<Key, (bool, Vec<Real>, Vec<Real>)>;

fn by_key(ranks: &[RankOut]) -> ByKey {
    let mut out = ByKey::new();
    for r in ranks {
        for (k, new, f, a) in &r.owned {
            assert!(
                out.insert(k.clone(), (*new, f.clone(), a.clone()))
                    .is_none(),
                "a node owned twice"
            );
        }
    }
    out
}

/// One case at 1, 2 and 4 ranks, then the switch serially.
fn check_case(case: Case) {
    let serial = on(1, case, false);
    let s = by_key(&serial);
    let fmax = serial[0].fmax_owned;
    assert!(fmax > 0.0, "{}: loaded", case.name);
    let parallel = [(2, on(2, case, false)), (4, on(4, case, false))];
    let runs = std::iter::once((1, &serial)).chain(parallel.iter().map(|(np, r)| (*np, r)));
    for (np, ranks) in runs {
        let got = by_key(ranks);
        let diff_fresh = ranks.iter().map(|r| r.diff_fresh).fold(0.0, Real::max) / fmax;
        let diff_model = ranks.iter().map(|r| r.diff_model).fold(0.0, Real::max) / fmax;
        let residual = ranks.iter().map(|r| r.record_residual).fold(0.0, Real::max) / fmax;
        let mut vs_serial = 0.0_f64;
        let mut flips = 0;
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            s.keys().collect::<Vec<_>>(),
            "{}: np {np}: the same nodes and sides as the serial run",
            case.name
        );
        for (k, (new, f, _)) in &got {
            let (snew, sf, _) = &s[k];
            flips += usize::from(new != snew);
            for (a, b) in f.iter().zip(sf) {
                vs_serial = vs_serial.max((a - b).abs() / fmax);
            }
        }
        let doubled: usize = ranks.iter().map(|r| r.doubled_loaded).sum();
        let permuted: usize = ranks.iter().map(|r| r.permuted).sum();
        let with_copy: usize = ranks.iter().map(|r| r.permuted_with_copy).sum();
        let moved: u64 = ranks.iter().map(|r| r.moved).sum();
        let released: u64 = ranks.iter().map(|r| r.released).sum();
        let inserted = ranks[0].inserted;
        eprintln!(
            "{} | np {np}: {inserted} cohesive elements (rank 0); {doubled} doubled loaded nodes; \
             {permuted} loaded rows rewritten as another order ({with_copy} holding a new copy; \
             summed over the ranks' local and ghost elements); {moved} records moved, {released} \
             released; max |f − f_fresh| / fmax = {diff_fresh:.3e}; max |f − f_model| / fmax = \
             {diff_model:.3e} (the model's own engine, refreshed); max |f − f_serial| \
             / fmax = {vs_serial:.3e} ({flips} copies original/new other than serial); max |f − \
             Σ records| / fmax = {residual:.3e}",
            case.name
        );
        assert!(
            doubled > 0 && moved > 0 && with_copy > 0,
            "{}: np {np}: vacuous (doubled {doubled}, moved {moved}, rewritten rows with a copy \
             {with_copy})",
            case.name
        );
        assert!(
            diff_fresh <= RTOL,
            "{}: np {np}: the external force of some copy differs from the re-application by \
             {diff_fresh:.3e} of the largest nodal load",
            case.name
        );
        assert!(
            vs_serial <= RTOL,
            "{}: np {np}: an owned copy differs from the serial run by {vs_serial:.3e}",
            case.name
        );
        assert!(
            residual <= RTOL,
            "{}: np {np}: the records do not sum to the external force ({residual:.3e})",
            case.name
        );
        assert_eq!(
            released, 0,
            "{}: the corrected default releases nothing",
            case.name
        );
    }
    // the switch (upstream's placement): the comparison has the power to see
    // a misplaced share
    let keep = on(1, case, true);
    let d = keep[0].diff_fresh / fmax;
    eprintln!(
        "{} | switch, serial: max |f − f_fresh| / fmax = {d:.3e}",
        case.name
    );
    assert!(
        d > 1e-3,
        "{}: upstream's placement misplaces whole shares",
        case.name
    );
}

#[test]
fn quadrangle_4_boundaries_of_a_non_affine_hexahedral_mesh() {
    check_case(HEX8);
}

#[test]
fn segment_3_boundaries_of_a_triangle_6_mesh() {
    check_case(TRI6);
}

#[test]
fn triangle_6_boundaries_of_a_tetrahedron_10_mesh() {
    check_case(TET10);
}

#[test]
fn triangle_3_boundaries_under_a_position_dependent_traction() {
    check_case(TET4);
}
