// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential scenarios (oracle/probes/par4) and the
// parallel parts of upstream's test/test_model/test_common/
// test_non_local_toolbox/{test_build_neighborhood_parallel.cc,
// test_pair_computation.cc} (ported intent), shared by
// non_local_parallel.rs (local world) and mpi_smm.rs (real MPI).

//! Non-local laws on distributed meshes (MPI-048…050, TEST-079 / 080).
//!
//! * `*_matches_oracle`: the probe's scenarios on the oracle's Scotch
//!   partition with upstream's sequence (`non_local_ghost_laws_after_insertion`,
//!   `non_local_cleanup_erases_distribution_ghosts`; UD-1102 / UD-1101) and
//!   upstream's manager options: mesh after the initialisation, ghost law
//!   filters, grid synchroniser schemes, pair lists and weights (SHA-256 of
//!   the exact bytes), volumes, averaged fields and internal forces,
//!   bitwise at 1, 2 and 4 ranks.
//! * `*_default_vs_serial`: the corrected default against the serial run:
//!   the pair sets by global identity equal to the serial ones, the
//!   averages within the summation-order class
//!   (`docs/NUMERICAL_VALIDATION.md`).

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use akantu_comm::Communicator;
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{ElementType, GhostType, Idx, Real, Result};
use akantu_io::read_mesh;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ElementDataSelector, InternalFieldRef};
use akantu_model::non_local::NonLocalOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::SolverCallback;
use akantu_validation::Fixture;

use super::non_local_common::{TestNonLocalMaterial, register};
use super::par_common;

pub mod v140;

/// Outcome of a scenario on one rank.
pub type Outcome = std::result::Result<(), String>;

fn ok<T>(r: Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// Material file of the probe's `nl_region` (test_build_neighborhood_parallel).
pub const REGION_DAT: &str = "material par4_region_material [\n  name = mat_1\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
material par4_region_material [\n  name = mat_2\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
non_local test_region remove_wf [\n  radius = 0.5\n  weight_function weight_parameter [\n    update_rate = 1\n    damage_limit = 0.9\n  ]\n]\n";

/// Material file of the probe's `nl_pair` (test_pair_computation).
pub const PAIR_DAT: &str = "material par4_damage_material [\n  name = mat_1\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
material par4_damage_material [\n  name = mat_2\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
non_local mat_1 remove_wf [\n  radius = 0.5\n  weight_function weight_parameter [\n    update_rate = 1\n    damage_limit = 0.9\n  ]\n]\n\
non_local mat_2 remove_wf [\n  radius = 0.5\n  weight_function weight_parameter [\n    update_rate = 1\n    damage_limit = 0.9\n  ]\n]\n";

const ELASTIC: &str = "  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n";

/// Material file of the probe's `nl_damage_wf` (`nl_region` with
/// `damage_wf`).
pub const DAMAGE_WF_DAT: &str = "material par4_region_material [\n  name = mat_1\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
material par4_region_material [\n  name = mat_2\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
non_local test_region damage_wf [\n  radius = 0.5\n  weight_function weight_parameter [\n    update_rate = 1\n  ]\n]\n";

/// Material file of the probe's `nl_mazars` / `nl_mazars_two` /
/// `nl_mazars_two_strips` / `nl_mixed`.
pub fn mazars_dat(variant: &str) -> String {
    match variant {
        "nl_mazars_two" | "nl_mazars_two_strips" => format!(
            "material mazars_non_local [\n  name = mat_1\n{ELASTIC}]\n\
             material mazars_non_local [\n  name = mat_2\n{ELASTIC}  average_on_damage = true\n]\n\
             non_local mat_1 base_wf [\n  radius = 0.3\n]\n\
             non_local mat_2 base_wf [\n  radius = 0.25\n]\n"
        ),
        "nl_mixed" => format!(
            "material mazars_non_local [\n  name = mat_1\n{ELASTIC}]\n\
             material elastic [\n  name = mat_2\n  rho = 1\n  E = 1e9\n  nu = 0.2\n]\n\
             non_local mat_1 base_wf [\n  radius = 0.3\n]\n"
        ),
        _ => format!(
            "material mazars_non_local [\n  name = body\n{ELASTIC}]\n\
             non_local body base_wf [\n  radius = 0.3\n]\n"
        ),
    }
}

/// Which sequence the distributed initialisation follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sequence {
    /// Upstream's (UD-1101 / UD-1102 compat switches): oracle replays.
    Upstream,
    /// The corrected default.
    Corrected,
}

pub fn fixture(scenario: &str, comm: &Communicator) -> std::result::Result<Fixture, String> {
    Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{}/rank{}",
        comm.size(),
        comm.rank()
    ))
    .map_err(|e| e.to_string())
}

/// The case's mesh read on rank 0 and distributed with the oracle's
/// partition (`partition/linearized` of rank 0; one rank: the bisection
/// partitioner, as the replays).
pub fn distributed_mesh(
    comm: &Communicator,
    scenario: &str,
    np_of_partition: usize,
    mesh_path: &std::path::Path,
) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(read_mesh(&mut m, mesh_path, MeshIoType::Gmsh), "read")?;
    }
    if comm.size() > 1 {
        let parts = if comm.rank() == 0 {
            par_common::oracle_partition(scenario, np_of_partition)?
        } else {
            Vec::new()
        };
        par_common::distribute_prescribed(&mut m, parts)?;
    } else {
        par_common::distribute_rcb(&mut m)?;
    }
    Ok(m)
}

/// The model of a scenario: the test laws registered under the probe's
/// keywords, the physical-name selector when `selector`, the sequence.
pub fn model(
    mesh: Mesh,
    dat: &str,
    selector: bool,
    seq: Sequence,
) -> std::result::Result<SolidMechanicsModel, String> {
    model_on(SharedMesh::new(mesh), dat, selector, seq)
}

/// [`model`] on a shared mesh (several models in turn on one mesh).
pub fn model_on(
    mesh: SharedMesh,
    dat: &str,
    selector: bool,
    seq: Sequence,
) -> std::result::Result<SolidMechanicsModel, String> {
    let parser = ok(Parser::parse_str(dat, "material.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(mesh, None, "solid_mechanics_model", parser),
        "model",
    )?;
    register(
        &mut model.physics.factory,
        "par4_region_material",
        TestNonLocalMaterial::test_material,
    );
    register(
        &mut model.physics.factory,
        "par4_damage_material",
        TestNonLocalMaterial::test_material_damage,
    );
    if selector {
        model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    }
    if seq == Sequence::Upstream {
        model
            .physics
            .upstream_compat
            .non_local_ghost_laws_after_insertion = true;
        model
            .physics
            .upstream_compat
            .non_local_cleanup_erases_distribution_ghosts = true;
        model.physics.non_local_options = NonLocalOptions::upstream();
    }
    Ok(model)
}

fn init(model: &mut SolidMechanicsModel) -> Outcome {
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )
}

/// First mismatch of two exact slices.
fn bits(name: &str, got: &[Real], want: &[Real]) -> Outcome {
    if got.len() != want.len() {
        return Err(format!(
            "{name}: {} values, oracle {}",
            got.len(),
            want.len()
        ));
    }
    for (k, (a, b)) in got.iter().zip(want).enumerate() {
        if a.to_bits() != b.to_bits() {
            return Err(format!("{name}[{k}]: {a:e} vs oracle {b:e}"));
        }
    }
    Ok(())
}

fn ints(name: &str, got: &[i64], want: &[i64]) -> Outcome {
    if got != want {
        let k = got
            .iter()
            .zip(want)
            .position(|(a, b)| a != b)
            .unwrap_or(got.len().min(want.len()));
        return Err(format!(
            "{name}: differs at {k} ({:?} vs oracle {:?}; lengths {} / {})",
            got.get(k),
            want.get(k),
            got.len(),
            want.len()
        ));
    }
    Ok(())
}

fn sha256_i64(v: &[i64]) -> String {
    let mut b = Vec::with_capacity(8 * v.len());
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    akantu_validation::checksum::sha256_bytes(&b)
}

fn sha256_f64(v: &[Real]) -> String {
    let mut b = Vec::with_capacity(8 * v.len());
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    akantu_validation::checksum::sha256_bytes(&b)
}

fn gname(g: GhostType) -> &'static str {
    match g {
        GhostType::NotGhost => "not_ghost",
        GhostType::Ghost => "ghost",
    }
}

fn param<'a>(fx: &'a Fixture, key: &str) -> std::result::Result<&'a str, String> {
    fx.manifest().parameters[key]
        .as_str()
        .ok_or_else(|| format!("parameter {key} missing"))
}

/// The mesh after the initialisation (probe `record_mesh`).
pub fn check_mesh(model: &SolidMechanicsModel, fx: &Fixture, pre: &str) -> Outcome {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let distributed = mesh.is_distributed();
    for g in GhostType::ALL {
        for ty in mesh.element_types(akantu_core::TypeFilter::new().dim(2).ghost(g)) {
            let name = format!("{pre}/conn/{ty}/{}", gname(g));
            let want = fx.load_i64(&name).map_err(|e| e.to_string())?;
            let conn = ok(mesh.connectivity(ty, g), "connectivity")?;
            let got: Vec<i64> = conn
                .as_slice()
                .iter()
                .map(|&n| {
                    if distributed {
                        mesh.node_global_id(n)
                    } else {
                        i64::from(n)
                    }
                })
                .collect();
            ints(&name, &got, &want.data)?;
            if g == GhostType::Ghost && distributed {
                let name = format!("{pre}/ghost_counters/{ty}");
                let want = fx.load_i64(&name).map_err(|e| e.to_string())?;
                let c = ok(mesh.ghosts_counters().get(ty, g), "counters")?;
                let got: Vec<i64> = c.as_slice().iter().map(|&v| i64::from(v)).collect();
                ints(&name, &got, &want.data)?;
            }
        }
    }
    let n = mesh.nb_nodes();
    let mut gid = Vec::with_capacity(n);
    let mut flags = Vec::with_capacity(n);
    for k in 0..n {
        let k = Idx::try_from(k).map_err(|e| e.to_string())?;
        gid.push(if distributed {
            mesh.node_global_id(k)
        } else {
            i64::from(k)
        });
        flags.push(i64::from(mesh.node_flag(k).bits()));
    }
    ints(
        &format!("{pre}/node_global_ids"),
        &gid,
        &fx.load_i64(&format!("{pre}/node_global_ids"))
            .map_err(|e| e.to_string())?
            .data,
    )?;
    ints(
        &format!("{pre}/node_flags"),
        &flags,
        &fx.load_i64(&format!("{pre}/node_flags"))
            .map_err(|e| e.to_string())?
            .data,
    )?;
    bits(
        &format!("{pre}/nodes"),
        mesh.nodes().as_slice(),
        &fx.load_f64(&format!("{pre}/nodes"))
            .map_err(|e| e.to_string())?
            .data,
    )
}

/// The laws' ghost assignment (probe `record_laws`).
pub fn check_laws(model: &SolidMechanicsModel, fx: &Fixture, pre: &str) -> Outcome {
    let h = &model.physics.materials;
    for (ty, g, a) in h.law_index_map().as_map().iter_all() {
        if g != GhostType::Ghost || ty.spatial_dimension() != 2 {
            continue;
        }
        let name = format!("{pre}/law_index/{ty}/ghost");
        let want = fx.load_i64(&name).map_err(|e| e.to_string())?;
        let a = ok(a.read(), "law index")?;
        let got: Vec<i64> = a.as_slice().iter().map(|&v| i64::from(v)).collect();
        ints(&name, &got, &want.data)?;
    }
    for (m, law) in h.laws().iter().enumerate() {
        for (ty, g, f) in law.element_filter().as_map().iter_all() {
            let name = format!("{pre}/law{m}/filter/{ty}/{}", gname(g));
            let Ok(want) = fx.load_i64(&name) else {
                if f.is_empty() {
                    continue;
                }
                return Err(format!(
                    "{name}: missing in the oracle, {} entries here",
                    f.len()
                ));
            };
            let got: Vec<i64> = f.as_slice().iter().map(|&v| i64::from(v)).collect();
            ints(&name, &got, &want.data)?;
        }
    }
    Ok(())
}

/// Pair lists, weights and grid schemes of a neighbourhood (probe
/// `record_neighborhood`).
pub fn check_neighborhood(
    model: &SolidMechanicsModel,
    fx: &Fixture,
    name: &str,
    pre: &str,
    with_pairs: bool,
) -> Outcome {
    let mgr = model
        .non_local_manager()
        .ok_or_else(|| "no non-local manager".to_owned())?;
    let nb = ok(mgr.neighborhood(name), "neighbourhood")?;
    for g in GhostType::ALL {
        if with_pairs {
            let mut v = Vec::new();
            for (q1, q2) in nb.pairs(g) {
                for q in [q1, q2] {
                    v.push(i64::from(q.ty as u8));
                    v.push(q.ghost as i64);
                    v.push(i64::from(q.element));
                    v.push(i64::from(q.num_point));
                    v.push(i64::from(q.global_num));
                }
            }
            let count = fx
                .load_i64(&format!("{pre}/pairs/{}/count", gname(g)))
                .map_err(|e| e.to_string())?;
            ints(
                &format!("{pre}/pairs/{}/count", gname(g)),
                &[i64::try_from(nb.pair_list(g).len()).map_err(|e| e.to_string())?],
                &count.data,
            )?;
            let want = param(fx, &format!("{pre}/pairs/{}/sha256", gname(g)))?;
            if sha256_i64(&v) != want {
                let lines: Vec<String> = v
                    .chunks(10)
                    .map(|c| c.iter().map(i64::to_string).collect::<Vec<_>>().join(" "))
                    .collect();
                let at = dump_full_list(fx, pre, "pairs", g, &lines);
                return Err(format!(
                    "{pre}/pairs/{}: SHA-256 differs (the port's list: {at})",
                    gname(g)
                ));
            }
        }
        let w: Vec<Real> = nb.pair_weights(g).iter().flatten().copied().collect();
        let want = param(fx, &format!("{pre}/weights/{}/sha256", gname(g)))?;
        if sha256_f64(&w) != want {
            let lines: Vec<String> = w
                .chunks(2)
                .map(|c| {
                    c.iter()
                        .map(|x| format!("{:x}", x.to_bits()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect();
            let at = dump_full_list(fx, pre, "weights", g, &lines);
            return Err(format!(
                "{pre}/weights/{}: SHA-256 differs (the port's list: {at})",
                gname(g)
            ));
        }
    }
    if with_pairs && let Some(gh) = model.physics.non_local_ghosts.as_ref() {
        let sync = gh
            .synchronizer(name)
            .ok_or_else(|| format!("no grid synchroniser {name}"))?;
        for (sr, dir) in [
            (akantu_comm::synchronizer::SendRecv::Send, "send"),
            (akantu_comm::synchronizer::SendRecv::Recv, "recv"),
        ] {
            for (p, scheme) in sync.communications.schemes(sr) {
                if scheme.is_empty() {
                    continue;
                }
                let qn = format!("{pre}/grid/{dir}/{p}");
                let want = fx.load_i64(&qn).map_err(|e| e.to_string())?;
                let got: Vec<i64> = scheme
                    .iter()
                    .flat_map(|e| {
                        [
                            i64::from(e.ty() as u8),
                            e.idx() as i64,
                            e.ghost_type() as i64,
                        ]
                    })
                    .collect();
                ints(&qn, &got, &want.data)?;
            }
        }
    }
    Ok(())
}

/// On a digest mismatch, the port's list in the probe's
/// `PAR4_PROBE_FULL_LISTS` format (one entry per line: the ten integers of
/// a pair, or the two hexadecimal bit patterns of a pair's weights), at
/// `<target tmp>/par4-full-lists/<case id>/<pre>_<what>_<ghost>.txt`, so that
/// the comparison with the probe's file is one `diff`. Returns the path (or
/// why nothing was written).
fn dump_full_list(fx: &Fixture, pre: &str, what: &str, g: GhostType, lines: &[String]) -> String {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("par4-full-lists")
        .join(fx.case_id());
    let file = dir.join(format!("{}_{what}_{}.txt", pre.replace('/', "_"), gname(g)));
    let mut text = lines.join("\n");
    text.push('\n');
    match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, text)) {
        Ok(()) => file.display().to_string(),
        Err(e) => format!("not written: {e}"),
    }
}

/// Per-slot manager array (volumes, jacobians) against the probe's
/// per-block record.
pub fn check_slots(model: &SolidMechanicsModel, fx: &Fixture, pre: &str, jac: bool) -> Outcome {
    let mgr = model
        .non_local_manager()
        .ok_or_else(|| "no non-local manager".to_owned())?;
    let values = if jac { mgr.jacobians() } else { mgr.volumes() };
    for (ty, g, off, n) in mgr.layout().blocks() {
        let name = format!("{pre}/{ty}/{}", gname(g));
        let Ok(want) = fx.load_f64(&name) else {
            if n == 0 {
                continue;
            }
            return Err(format!("{name}: missing in the oracle"));
        };
        bits(&name, &values[off..off + n], &want.data)?;
    }
    Ok(())
}

/// A `Real` internal of every law (not-ghost blocks), against `pre/law{m}/{id}/…`.
pub fn check_internal(model: &SolidMechanicsModel, fx: &Fixture, id: &str, pre: &str) -> Outcome {
    for m in 0..model.nb_materials() {
        let law = ok(model.material(m), "law")?;
        let mut blocks: Vec<(ElementType, Vec<Real>)> = Vec::new();
        law.for_each_internal(&mut |n, f| {
            if n != id {
                return;
            }
            if let InternalFieldRef::Real(f) = f {
                for (ty, g, a) in f.values().as_map().iter_all() {
                    if g == GhostType::NotGhost
                        && let Ok(a) = a.read()
                    {
                        blocks.push((ty, a.as_slice().to_vec()));
                    }
                }
            }
        });
        for (ty, got) in blocks {
            let name = format!("{pre}/law{m}/{}/{ty}/not_ghost", id.replace(' ', "_"));
            let want = fx.load_f64(&name).map_err(|e| e.to_string())?;
            bits(&name, &got, &want.data)?;
        }
    }
    Ok(())
}

/// Integration-point coordinates of the not-ghost elements of `filter`.
fn point_coords(
    model: &SolidMechanicsModel,
    ty: ElementType,
    filter: &[Idx],
) -> std::result::Result<Vec<Real>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let fe = ok(model.physics.base.fems.get(""), "fe")?;
    let mut out = akantu_core::Array::new(2);
    ok(
        fe.interpolate_on_integration_points(
            &mesh,
            mesh.nodes().view(),
            &mut out,
            2,
            ty,
            GhostType::NotGhost,
            Some(filter),
        ),
        "coordinates",
    )?;
    Ok(out.as_slice().to_vec())
}

/// The probe's `set_eigen_grad_u`: a polynomial of the point coordinates,
/// same operation order.
pub fn set_eigen_grad_u(model: &mut SolidMechanicsModel) -> Outcome {
    for m in 0..model.nb_materials() {
        let blocks: Vec<(ElementType, Vec<Idx>)> = {
            let law = ok(model.material(m), "law")?;
            law.element_filter()
                .as_map()
                .iter_all()
                .filter(|(ty, g, _)| *g == GhostType::NotGhost && ty.spatial_dimension() == 2)
                .map(|(ty, _, f)| (ty, f.as_slice().to_vec()))
                .collect()
        };
        for (ty, filter) in blocks {
            let x = point_coords(model, ty, &filter)?;
            let law = ok(model.material_mut(m), "law")?;
            let mut eg = ok(
                law.base_mut().eigen_grad_u.get_mut(ty, GhostType::NotGhost),
                "eg",
            )?;
            let e = eg.as_mut_slice();
            for (p, xy) in x.as_chunks::<2>().0.iter().enumerate() {
                let (xx, yy) = (xy[0], xy[1]);
                for i in 0..2 {
                    for j in 0..2 {
                        e[p * 4 + i * 2 + j] = (i as Real + 1.) * xx
                            + (j as Real + 2.) * yy * yy
                            + 0.5 * xx * yy
                            + 0.25 * (i as Real - j as Real) * xx * xx;
                    }
                }
            }
        }
    }
    Ok(())
}

/// The probe's `set_damage_band`.
pub fn set_damage_band(model: &mut SolidMechanicsModel) -> Outcome {
    for m in 0..model.nb_materials() {
        let blocks: Vec<(ElementType, Vec<Idx>)> = {
            let law = ok(model.material(m), "law")?;
            law.element_filter()
                .as_map()
                .iter_all()
                .filter(|(ty, g, _)| *g == GhostType::NotGhost && ty.spatial_dimension() == 2)
                .map(|(ty, _, f)| (ty, f.as_slice().to_vec()))
                .collect()
        };
        for (ty, filter) in blocks {
            let x = point_coords(model, ty, &filter)?;
            let law = ok(model.material_mut(m), "law")?;
            let mut ok_set = Ok(());
            law.for_each_internal_mut(&mut |n, f| {
                if n == "damage"
                    && let akantu_model::constitutive_law::InternalFieldMut::Real(f) = f
                {
                    match f.get_mut(ty, GhostType::NotGhost) {
                        Ok(mut d) => {
                            for (p, xy) in x.as_chunks::<2>().0.iter().enumerate() {
                                d.as_mut_slice()[p] =
                                    if (xy[0] - 0.1).abs() < 0.15 { 0.95 } else { 0. };
                            }
                        }
                        Err(e) => ok_set = Err(e.to_string()),
                    }
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            ok_set?;
        }
    }
    Ok(())
}

/// Probe `nl_region` replayed with upstream's sequence: bitwise.
pub fn region_matches_oracle(comm: Communicator) -> Outcome {
    let fx = fixture("nl_region", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "nl_region",
        comm.size(),
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let mut model = model(mesh, REGION_DAT, true, Sequence::Upstream)?;
    init(&mut model)?;
    check_mesh(&model, &fx, "init")?;
    check_laws(&model, &fx, "init")?;
    check_neighborhood(&model, &fx, "test_region", "init/test_region", true)?;
    check_slots(&model, &fx, "init/volumes", false)?;
    check_slots(&model, &fx, "init/jacobians", true)?;
    debug_schemes(&model)?;
    set_eigen_grad_u(&mut model)?;
    ok(model.assemble_internal_forces(), "assemble 0")?;
    check_internal(&model, &fx, "grad_u", "call0")?;
    check_internal(&model, &fx, "grad_u non local", "call0")?;
    check_neighborhood(&model, &fx, "test_region", "call0/test_region", false)?;
    check_slots(&model, &fx, "call0/volumes", false)?;
    bits(
        "call0/internal_force",
        ok(model.internal_force(), "f")?.as_slice(),
        &fx.load_f64("call0/internal_force")
            .map_err(|e| e.to_string())?
            .data,
    )?;
    set_damage_band(&mut model)?;
    ok(model.assemble_internal_forces(), "assemble 1")?;
    check_internal(&model, &fx, "grad_u non local", "call1")?;
    check_internal(&model, &fx, "damage", "call1")?;
    check_neighborhood(&model, &fx, "test_region", "call1/test_region", false)?;
    check_slots(&model, &fx, "call1/volumes", false)
}

/// Probe `nl_damage_wf` (UD-1103) replayed with upstream's
/// sequence and `WeightExchange::Upstream` (set by `NonLocalOptions::upstream()`:
/// `damage_wf` exchanges nothing for `_mnl_weight`): bitwise. With the
/// corrected weight exchange instead (everything else upstream's), the
/// recorded averages after the damage band differ on more than one rank,
/// so the fixture rejects the default (the switch selects upstream's
/// behaviour), and are bitwise equal on one rank.
pub fn damage_wf_matches_oracle(comm: Communicator) -> Outcome {
    let fx = fixture("nl_damage_wf", &comm)?;
    let mesh_path = fx.input_path("mesh").map_err(|e| e.to_string())?;
    let run = |exchange: akantu_model::non_local::WeightExchange| -> Outcome {
        let mesh = distributed_mesh(&comm, "nl_damage_wf", comm.size(), &mesh_path)?;
        let mut model = model(mesh, DAMAGE_WF_DAT, true, Sequence::Upstream)?;
        model.physics.non_local_options.weight_exchange = exchange;
        init(&mut model)?;
        // the comparisons between collectives keep their first error and
        // let every rank reach the next collective (no rank leaves early)
        let before = (|| {
            check_mesh(&model, &fx, "init")?;
            check_laws(&model, &fx, "init")?;
            check_neighborhood(&model, &fx, "test_region", "init/test_region", true)?;
            check_slots(&model, &fx, "init/volumes", false)
        })();
        set_eigen_grad_u(&mut model)?;
        ok(model.assemble_internal_forces(), "assemble 0")?;
        let call0 = (|| {
            check_internal(&model, &fx, "grad_u non local", "call0")?;
            check_neighborhood(&model, &fx, "test_region", "call0/test_region", false)?;
            check_slots(&model, &fx, "call0/volumes", false)
        })();
        set_damage_band(&mut model)?;
        ok(model.assemble_internal_forces(), "assemble 1")?;
        before?;
        call0?;
        check_internal(&model, &fx, "damage", "call1")?;
        check_internal(&model, &fx, "grad_u non local", "call1")?;
        check_neighborhood(&model, &fx, "test_region", "call1/test_region", false)?;
        check_slots(&model, &fx, "call1/volumes", false)?;
        bits(
            "call1/internal_force",
            ok(model.internal_force(), "f")?.as_slice(),
            &fx.load_f64("call1/internal_force")
                .map_err(|e| e.to_string())?
                .data,
        )
    };
    run(akantu_model::non_local::WeightExchange::Upstream)?;
    // the corrected exchange against the same records: rejected on more
    // than one rank (on at least one rank), equal on one rank
    let corrected = run(akantu_model::non_local::WeightExchange::AllInternals);
    let mut differs = [u64::from(corrected.is_err())];
    ok(
        comm.all_reduce(&mut differs, akantu_comm::ReduceOp::Sum),
        "sum",
    )?;
    match (comm.size(), differs[0]) {
        (1, 0) => Ok(()),
        (1, _) => Err(format!(
            "one rank: the corrected weight exchange differs from the oracle: {corrected:?}"
        )),
        (_, 0) => Err(
            "the corrected weight exchange reproduces the oracle: the fixture does not show \
             UD-1103"
                .into(),
        ),
        _ => Ok(()),
    }
}

/// Probe `nl_pair` replayed with upstream's sequence: bitwise; the probe's
/// verdict of upstream's criteria is "pass".
pub fn pair_matches_oracle(comm: Communicator) -> Outcome {
    let fx = fixture("nl_pair", &comm)?;
    let mesh = distributed_mesh(
        &comm,
        "nl_pair",
        comm.size(),
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    // two neighbourhoods: upstream's per-neighbourhood erasure with a stale
    // set (UD-1100) completes on these partitions (it aborts on 4 ranks)
    let mut model = model(mesh, PAIR_DAT, true, Sequence::Upstream)?;
    init(&mut model)?;
    if param(&fx, "pair_check")? != "pass" {
        return Err("the oracle's pair check did not pass".into());
    }
    check_mesh(&model, &fx, "init")?;
    check_laws(&model, &fx, "init")?;
    check_neighborhood(&model, &fx, "mat_1", "init/mat_1", true)?;
    check_neighborhood(&model, &fx, "mat_2", "init/mat_2", true)?;
    set_eigen_grad_u(&mut model)?;
    ok(model.assemble_internal_forces(), "assemble 0")?;
    check_internal(&model, &fx, "grad_u non local", "call0")?;
    check_slots(&model, &fx, "call0/volumes", false)
}

/// Probe `nl_mazars` / `nl_mazars_two` / `nl_mixed` replayed with upstream's
/// sequence: bitwise.
pub fn mazars_matches_oracle(comm: Communicator, scenario: &str) -> Outcome {
    let fx = fixture(scenario, &comm)?;
    let mesh = distributed_mesh(
        &comm,
        scenario,
        comm.size(),
        &fx.input_path("mesh").map_err(|e| e.to_string())?,
    )?;
    let selector = scenario != "nl_mazars";
    let mut model = model(mesh, &mazars_dat(scenario), selector, Sequence::Upstream)?;
    model.physics.law_options = Some(akantu_solid::material::LawOptions::upstream());
    init(&mut model)?;
    check_mesh(&model, &fx, "init")?;
    check_laws(&model, &fx, "init")?;
    let names: Vec<String> = (0..model.nb_materials())
        .filter_map(|m| model.material(m).ok())
        .filter(|l| l.as_non_local().is_some())
        .map(|l| l.name().to_owned())
        .collect();
    for n in &names {
        check_neighborhood(&model, &fx, n, &format!("init/{n}"), true)?;
    }
    let lambdas = [1., 2., 3., 4., 5., 6., 7., 8., 6., 9.];
    let nodes = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        mesh.nodes().as_slice().to_vec()
    };
    for (s, lam) in lambdas.iter().enumerate() {
        let l = 1e-4 * lam;
        {
            let mut u = ok(model.displacement_mut(), "u")?;
            for (n, uu) in u
                .as_mut_slice()
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .enumerate()
            {
                let (x, y) = (nodes[2 * n], nodes[2 * n + 1]);
                uu[0] = l * (0.6 * x + 0.3 * y * y);
                uu[1] = l * (0.2 * x * y - 0.1 * y);
            }
        }
        ok(
            model
                .physics
                .before_solve_step(&mut model.solver.dof_manager),
            "before",
        )?;
        ok(model.assemble_internal_forces(), "assemble")?;
        ok(
            model
                .physics
                .after_solve_step(&mut model.solver.dof_manager, true),
            "after",
        )?;
        let pre = format!("step{s}");
        check_internal(&model, &fx, "damage", &pre)?;
        check_internal(&model, &fx, "non_local_variable", &pre)?;
        bits(
            &format!("{pre}/internal_force"),
            ok(model.internal_force(), "f")?.as_slice(),
            &fx.load_f64(&format!("{pre}/internal_force"))
                .map_err(|e| e.to_string())?
                .data,
        )?;
        if s + 1 == lambdas.len() {
            check_internal(&model, &fx, "Ehat", &pre)?;
            check_internal(&model, &fx, "stress", &pre)?;
        }
    }
    Ok(())
}

/// Identity of an integration point across partitions: the sorted global
/// connectivity of its element and its point number.
type PointId = (Vec<i64>, Idx);

fn point_ids(model: &SolidMechanicsModel, name: &str) -> std::result::Result<Vec<PointId>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let mgr = model
        .non_local_manager()
        .ok_or_else(|| "no non-local manager".to_owned())?;
    let nb = ok(mgr.neighborhood(name), "neighbourhood")?;
    let mut out = Vec::new();
    for q in nb.points() {
        let e = akantu_core::Element::new(q.ty, q.element, q.ghost);
        let mut c: Vec<i64> = ok(mesh.element_connectivity(e), "conn")?
            .iter()
            .map(|&n| {
                if mesh.is_distributed() {
                    mesh.node_global_id(n)
                } else {
                    i64::from(n)
                }
            })
            .collect();
        c.sort_unstable();
        out.push((c, q.num_point));
    }
    Ok(out)
}

/// Undirected pair set of a neighbourhood by global point identity (owned
/// pairs, plus the ghost pairs seen from the owned point).
fn pair_set(
    model: &SolidMechanicsModel,
    name: &str,
) -> std::result::Result<BTreeSet<(PointId, PointId)>, String> {
    let ids = point_ids(model, name)?;
    let mgr = model.non_local_manager().ok_or("no manager")?;
    let nb = ok(mgr.neighborhood(name), "neighbourhood")?;
    let mut s = BTreeSet::new();
    for g in GhostType::ALL {
        for &(i, j) in nb.pair_list(g) {
            let (a, b) = (ids[i].clone(), ids[j].clone());
            s.insert(if a <= b { (a, b) } else { (b, a) });
        }
    }
    Ok(s)
}

/// Averaged `grad_u non local` of the owned points by global identity.
fn averages_by_id(
    model: &SolidMechanicsModel,
) -> std::result::Result<BTreeMap<PointId, Vec<Real>>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let mut out = BTreeMap::new();
    for m in 0..model.nb_materials() {
        let law = ok(model.material(m), "law")?;
        let filter = law.element_filter().clone();
        let mut res = Ok(());
        law.for_each_internal(&mut |n, f| {
            if n != "grad_u non local" {
                return;
            }
            if let InternalFieldRef::Real(f) = f {
                for (ty, g, a) in f.values().as_map().iter_all() {
                    if g != GhostType::NotGhost {
                        continue;
                    }
                    let Ok(a) = a.read() else {
                        continue;
                    };
                    let Ok(fl) = filter.get(ty, g) else {
                        continue;
                    };
                    let nc = a.nb_component();
                    let nq = if fl.is_empty() { 0 } else { a.len() / fl.len() };
                    for (k, &el) in fl.as_slice().iter().enumerate() {
                        let e = akantu_core::Element::new(ty, el, g);
                        let mut c: Vec<i64> = match mesh.element_connectivity(e) {
                            Ok(c) => c
                                .iter()
                                .map(|&nn| {
                                    if mesh.is_distributed() {
                                        mesh.node_global_id(nn)
                                    } else {
                                        i64::from(nn)
                                    }
                                })
                                .collect(),
                            Err(er) => {
                                res = Err(er.to_string());
                                return;
                            }
                        };
                        c.sort_unstable();
                        for q in 0..nq {
                            let off = (k * nq + q) * nc;
                            out.insert(
                                (c.clone(), Idx::try_from(q).unwrap_or(-1)),
                                a.as_slice()[off..off + nc].to_vec(),
                            );
                        }
                    }
                }
            }
        });
        res?;
    }
    Ok(out)
}

/// Gather `(id, values)` of every rank on every rank (small test meshes).
fn gather_all(
    comm: &Communicator,
    local: &BTreeMap<PointId, Vec<Real>>,
) -> std::result::Result<BTreeMap<PointId, Vec<Real>>, String> {
    // encode: [n_conn, conn…, q, n_val, values as bits…] as i64
    let mut buf: Vec<i64> = Vec::new();
    for ((c, q), v) in local {
        buf.push(c.len() as i64);
        buf.extend(c);
        buf.push(i64::from(*q));
        buf.push(v.len() as i64);
        buf.extend(v.iter().map(|x| x.to_bits() as i64));
    }
    let all = ok(comm.all_gather_v(&buf), "all_gather")?;
    let mut out = BTreeMap::new();
    for b in all {
        let mut i = 0;
        while i < b.len() {
            let nc = b[i] as usize;
            let c = b[i + 1..i + 1 + nc].to_vec();
            let q = b[i + 1 + nc] as Idx;
            let nv = b[i + 2 + nc] as usize;
            let v: Vec<Real> = b[i + 3 + nc..i + 3 + nc + nv]
                .iter()
                .map(|&x| Real::from_bits(x as u64))
                .collect();
            out.insert((c, q), v);
            i += 3 + nc + nv;
        }
    }
    Ok(out)
}

/// The corrected default against the serial run (TEST-080's criterion,
/// "pair lists identical in serial and parallel", by global identity; the
/// averages within the summation-order bound `rel`).
pub fn region_default_vs_serial(comm: Communicator, rel: Real) -> Outcome {
    // the oracle's partition where the probe recorded this rank count;
    // otherwise (8 ranks in the par tier) the case's mesh with the port's
    // bisection partitioner
    let recorded = par_common::oracle_rank_counts("nl_region").contains(&comm.size());
    let fx = if recorded {
        fixture("nl_region", &comm)?
    } else {
        Fixture::open_by_id("parallel/nl_region/np1/rank0").map_err(|e| e.to_string())?
    };
    let path = fx.input_path("mesh").map_err(|e| e.to_string())?;
    // serial reference on every rank (the whole mesh, one-rank communicator)
    let serial_comm = Communicator::serial();
    let mut serial_mesh = ok(Mesh::with_communicator(2, "mesh", serial_comm), "mesh")?;
    ok(read_mesh(&mut serial_mesh, &path, MeshIoType::Gmsh), "read")?;
    let mut serial = model(serial_mesh, REGION_DAT, true, Sequence::Corrected)?;
    init(&mut serial)?;
    set_eigen_grad_u(&mut serial)?;
    ok(serial.assemble_internal_forces(), "serial assemble")?;
    let serial_pairs = pair_set(&serial, "test_region")?;
    let serial_avg = averages_by_id(&serial)?;
    // distributed, corrected default
    let mesh = if recorded {
        distributed_mesh(&comm, "nl_region", comm.size(), &path)?
    } else {
        let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
        if comm.rank() == 0 {
            ok(read_mesh(&mut m, &path, MeshIoType::Gmsh), "read")?;
        }
        par_common::distribute_rcb(&mut m)?;
        m
    };
    let mut model = model(mesh, REGION_DAT, true, Sequence::Corrected)?;
    init(&mut model)?;
    let pairs = pair_set(&model, "test_region")?;
    // every local pair is a serial pair; an owned point has all its serial
    // pairs locally
    let ids = point_ids(&model, "test_region")?;
    let mgr = model.non_local_manager().ok_or("no manager")?;
    let nb = ok(mgr.neighborhood("test_region"), "nbh")?;
    let owned: BTreeSet<PointId> = nb
        .points()
        .iter()
        .zip(&ids)
        .filter(|(q, _)| q.ghost == GhostType::NotGhost)
        .map(|(_, id)| id.clone())
        .collect();
    for p in &pairs {
        if !serial_pairs.contains(p) {
            return Err(format!("pair {p:?} is not a serial pair"));
        }
    }
    for p in &serial_pairs {
        if (owned.contains(&p.0) || owned.contains(&p.1)) && !pairs.contains(p) {
            return Err(format!("serial pair {p:?} of an owned point is missing"));
        }
    }
    set_eigen_grad_u(&mut model)?;
    ok(model.assemble_internal_forces(), "assemble")?;
    let avg = gather_all(&comm, &averages_by_id(&model)?)?;
    if avg.len() != serial_avg.len() {
        return Err(format!(
            "{} averaged points over the ranks, {} in serial",
            avg.len(),
            serial_avg.len()
        ));
    }
    let scale = serial_avg
        .values()
        .flatten()
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    let mut worst: Real = 0.;
    for (k, v) in &avg {
        let s = serial_avg
            .get(k)
            .ok_or_else(|| format!("point {k:?} has no serial average"))?;
        for (a, b) in v.iter().zip(s) {
            let d = (a - b).abs() / scale;
            if d.is_nan() || d > worst {
                worst = if d.is_nan() { Real::INFINITY } else { d };
            }
        }
    }
    if comm.rank() == 0 {
        eprintln!(
            "INFO region_default_vs_serial np{}: {} pairs (serial {}), worst |Δ|/max {worst:.3e}",
            comm.size(),
            pairs.len(),
            serial_pairs.len()
        );
    }
    if worst.is_nan() || worst > rel {
        return Err(format!(
            "averages differ from serial by {worst:e} > {rel:e}"
        ));
    }
    Ok(())
}

fn debug_schemes(model: &SolidMechanicsModel) -> Outcome {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let Some(es) = &mesh.synchronizers().element else {
        return Ok(());
    };
    let h = &model.physics.materials;
    for sr in [
        akantu_comm::synchronizer::SendRecv::Send,
        akantu_comm::synchronizer::SendRecv::Recv,
    ] {
        for (p, scheme) in es.communications.schemes(sr) {
            for e in scheme {
                let li = h.law_index_map().element_ref(*e).map(|v| v[0]);
                match li {
                    Ok(v) if v >= 0 => {}
                    other => {
                        return Err(format!(
                            "{sr:?} {p}: {e} law {other:?} (nb ghost {})",
                            mesh.nb_element(e.ty(), e.ghost_type())
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Upstream test data of the non-local toolbox (akantu-model copy).
fn upstream_data(name: &str) -> std::path::PathBuf {
    super::non_local_common::data(name)
}

/// The upstream program's mesh read on rank 0 and distributed with the
/// port's bisection partitioner (upstream: Scotch).
fn upstream_mesh(comm: &Communicator, name: &str) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(
            read_mesh(&mut m, &upstream_data(name), MeshIoType::Gmsh),
            "read",
        )?;
    }
    par_common::distribute_rcb(&mut m)?;
    Ok(m)
}

fn upstream_model(
    mesh: Mesh,
    dat: &str,
    ctor: fn(
        &akantu_model::constitutive_law::LawSpec,
        Option<&Mesh>,
    ) -> Result<TestNonLocalMaterial<2>>,
) -> std::result::Result<SolidMechanicsModel, String> {
    let parser = ok(Parser::parse_file(upstream_data(dat)), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        ),
        "model",
    )?;
    register(&mut model.physics.factory, "test_material", ctor);
    model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    Ok(model)
}

/// Integration-point coordinates of every element of `(ty, ghost)`.
fn all_point_coords(
    model: &SolidMechanicsModel,
    ty: ElementType,
    ghost: GhostType,
) -> std::result::Result<Vec<Real>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let fe = ok(model.physics.base.fems.get(""), "fe")?;
    let mut out = akantu_core::Array::new(2);
    ok(
        fe.interpolate_on_integration_points(
            &mesh,
            mesh.nodes().view(),
            &mut out,
            2,
            ty,
            ghost,
            None,
        ),
        "coordinates",
    )?;
    Ok(out.as_slice().to_vec())
}

/// TEST-080 (`test_pair_computation`, PARALLEL_LEVEL 1 2; here 1, 2 and 4
/// ranks): upstream's program and criteria — `computePairs` (every owned
/// point against every point of both ghost types of the same law, distance
/// ≤ R + tolerance, half lists for owned pairs) over the mesh after the
/// initialisation, compared with the pair lists of the neighbourhoods
/// `mat_1` and `mat_2`: the same numbers of owned and ghost pairs, every
/// grid pair found by the brute force ("The number of pairs is not
/// correct", "The pairs are not correct").
pub fn test_pair_computation(comm: Communicator) -> Outcome {
    let mesh = upstream_mesh(&comm, "pair_test.msh")?;
    let mut model = upstream_model(
        mesh,
        "material_remove_damage.dat",
        TestNonLocalMaterial::test_material_damage,
    )?;
    init(&mut model)?;
    let r = 0.5;
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    let h = &model.physics.materials;
    let law_of = |ty: ElementType, g: GhostType, e: usize| -> i64 {
        h.law_index_map()
            .get(ty, g)
            .ok()
            .and_then(|a| a.as_slice().get(e).copied())
            .map_or(-1, i64::from)
    };
    type P = (u8, u8, i64, i64, i64);
    let mut brute: [BTreeSet<(P, P)>; 2] = [BTreeSet::new(), BTreeSet::new()];
    let types: Vec<ElementType> = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        mesh.element_types(
            akantu_core::TypeFilter::new()
                .dim(2)
                .ghost(GhostType::NotGhost),
        )
        .collect()
    };
    let nq = |ty: ElementType, g: GhostType| -> usize {
        model
            .physics
            .base
            .fems
            .get("")
            .ok()
            .and_then(|fe| fe.nb_integration_points(ty, g).ok())
            .unwrap_or(0)
    };
    for &t1 in &types {
        let x1 = all_point_coords(&model, t1, GhostType::NotGhost)?;
        let n1 = nq(t1, GhostType::NotGhost);
        for (k1, c1) in x1.as_chunks::<2>().0.iter().enumerate() {
            let (e1, p1) = (k1 / n1, k1 % n1);
            let m1 = law_of(t1, GhostType::NotGhost, e1);
            for g2 in GhostType::ALL {
                let types2: Vec<ElementType> = {
                    let mesh_h = model.physics.base.mesh.clone();
                    let mesh = ok(mesh_h.read(), "mesh")?;
                    mesh.element_types(akantu_core::TypeFilter::new().dim(2).ghost(g2))
                        .collect()
                };
                for t2 in types2 {
                    let x2 = all_point_coords(&model, t2, g2)?;
                    let n2 = nq(t2, g2);
                    for (k2, c2) in x2.as_chunks::<2>().0.iter().enumerate() {
                        let (e2, p2) = (k2 / n2, k2 % n2);
                        if m1 != law_of(t2, g2, e2) {
                            continue;
                        }
                        let d = akantu_model::non_local::distance(c1, c2);
                        if d <= r + tol && (g2 == GhostType::Ghost || k1 <= k2) {
                            let a: P = (t1 as u8, 0, e1 as i64, p1 as i64, k1 as i64);
                            let b: P = (t2 as u8, g2 as u8, e2 as i64, p2 as i64, k2 as i64);
                            brute[g2 as usize].insert((a, b));
                        }
                    }
                }
            }
        }
    }
    let mgr = model.non_local_manager().ok_or("no non-local manager")?;
    let mut counts = [0usize; 2];
    for name in ["mat_1", "mat_2"] {
        let nb = ok(mgr.neighborhood(name), "neighbourhood")?;
        for g in GhostType::ALL {
            counts[g as usize] += nb.pair_list(g).len();
            for (q1, q2) in nb.pairs(g) {
                let a: P = (
                    q1.ty as u8,
                    q1.ghost as u8,
                    i64::from(q1.element),
                    i64::from(q1.num_point),
                    i64::from(q1.global_num),
                );
                let b: P = (
                    q2.ty as u8,
                    q2.ghost as u8,
                    i64::from(q2.element),
                    i64::from(q2.num_point),
                    i64::from(q2.global_num),
                );
                if !brute[g as usize].contains(&(a, b)) {
                    return Err("The pairs are not correct".into());
                }
            }
        }
    }
    if counts[0] != brute[0].len() || counts[1] != brute[1].len() {
        return Err(format!(
            "The number of pairs is not correct: grid {counts:?}, brute force [{}, {}]",
            brute[0].len(),
            brute[1].len()
        ));
    }
    Ok(())
}

/// TEST-079 (`test_build_neighborhood_parallel`, PARALLEL_LEVEL 2; here 1,
/// 2 and 4 ranks): upstream's program on `parallel_test.msh` /
/// `material_parallel_test.dat` (two `test_material` laws sharing the
/// neighbourhood `test_region`, `remove_wf`): distribution, `initFull`, the
/// constant strain written into `grad_u` (doubled at the point closest to
/// the centre, found with an all-reduced minimum), two
/// `assembleInternalForces` with the damage of that point set to 0.9 in
/// between. The program checks nothing (it dumps); the port checks what
/// the dumps show: the ghost layer after the initialisation keeps every
/// ghost element of the distribution (UD-1101) and holds every element
/// within the radius of an owned point (the grid synchroniser's ghosts,
/// UD-1102: a ghost point of a pair for each), and — the program's own
/// outcome — `grad_u non local` is 0 after both calls: `grad_u` written by
/// the program is recomputed from `u = 0` by `computeAllStresses` (UD-1104,
/// the program's strain field never reaches the averaging). The averaging
/// itself is checked against the serial run by `region_default_vs_serial`.
pub fn test_build_neighborhood_parallel(comm: Communicator) -> Outcome {
    let mesh = upstream_mesh(&comm, "parallel_test.msh")?;
    let ghosts_before: BTreeSet<Vec<i64>> = ghost_conn_set(&mesh)?;
    let mut model = upstream_model(
        mesh,
        "material_parallel_test.dat",
        TestNonLocalMaterial::test_material,
    )?;
    init(&mut model)?;
    let ghosts_after = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        ghost_conn_set(&mesh)?
    };
    if !ghosts_before.is_subset(&ghosts_after) {
        return Err("the cleanup removed ghost elements of the distribution".into());
    }
    // every ghost point within R of an owned point is a ghost point of a pair
    {
        let mgr = model.non_local_manager().ok_or("no manager")?;
        let nb = ok(mgr.neighborhood("test_region"), "nbh")?;
        let r = nb.radius() + akantu_core::math::DEFAULT_TOLERANCE;
        let owned: Vec<usize> = (0..nb.points().len())
            .filter(|&i| nb.points()[i].ghost == GhostType::NotGhost)
            .collect();
        let in_pairs = nb.relevant_ghost_points();
        for (j, q) in nb.points().iter().enumerate() {
            if q.ghost != GhostType::Ghost {
                continue;
            }
            let near = owned
                .iter()
                .any(|&i| akantu_model::non_local::distance(nb.coords(i), nb.coords(j)) <= r);
            if near != in_pairs.contains(&j) {
                return Err(format!(
                    "ghost point {q}: within the radius {near}, in a pair {}",
                    !near
                ));
            }
        }
        if comm.size() > 1 && in_pairs.is_empty() {
            return Err("no ghost point in a pair on a distributed mesh".into());
        }
    }
    // the program's strain field and damage
    let applied = [2.0, 0.0, 0.0, 2.0];
    let mut best = (Real::INFINITY, None::<(usize, ElementType, usize)>);
    for m in 0..model.nb_materials() {
        let blocks: Vec<(ElementType, Vec<Idx>)> = {
            let law = ok(model.material(m), "law")?;
            law.element_filter()
                .as_map()
                .iter_all()
                .filter(|(_, g, _)| *g == GhostType::NotGhost)
                .map(|(ty, _, f)| (ty, f.as_slice().to_vec()))
                .collect()
        };
        for (ty, filter) in blocks {
            let x = point_coords(&model, ty, &filter)?;
            for (k, xy) in x.as_chunks::<2>().0.iter().enumerate() {
                let d = (xy[0] * xy[0] + xy[1] * xy[1]).sqrt();
                if d < best.0 {
                    best = (d, Some((m, ty, k)));
                }
            }
            let law = ok(model.material_mut(m), "law")?;
            let mut res = Ok(());
            law.for_each_internal_mut(&mut |n, f| {
                if n == "grad_u"
                    && let akantu_model::constitutive_law::InternalFieldMut::Real(f) = f
                {
                    match f.get_mut(ty, GhostType::NotGhost) {
                        Ok(mut g) => {
                            for t in g.as_mut_slice().as_chunks_mut::<4>().0.iter_mut() {
                                for (v, a) in t.iter_mut().zip(applied) {
                                    *v = -a;
                                }
                            }
                        }
                        Err(e) => res = Err(e.to_string()),
                    }
                }
                Ok(())
            })
            .map_err(|e| e.to_string())?;
            res?;
        }
    }
    let mut global = [best.0];
    ok(
        comm.all_reduce(&mut global, akantu_comm::ReduceOp::Min),
        "min",
    )?;
    let is_min = best.0 == global[0];
    let set_at_min =
        |model: &mut SolidMechanicsModel, field: &str, f: &dyn Fn(&mut [Real])| -> Outcome {
            if let (true, Some((m, ty, k))) = (is_min, best.1) {
                let law = ok(model.material_mut(m), "law")?;
                let mut res = Ok(());
                law.for_each_internal_mut(&mut |n, fi| {
                    if n == field
                        && let akantu_model::constitutive_law::InternalFieldMut::Real(fi) = fi
                    {
                        match fi.get_mut(ty, GhostType::NotGhost) {
                            Ok(mut a) => {
                                let nc = a.nb_component();
                                f(&mut a.as_mut_slice()[k * nc..(k + 1) * nc]);
                            }
                            Err(e) => res = Err(e.to_string()),
                        }
                    }
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
                res?;
            }
            Ok(())
        };
    set_at_min(&mut model, "grad_u", &|t| {
        for (v, a) in t.iter_mut().zip(applied) {
            *v += a;
        }
    })?;
    ok(model.assemble_internal_forces(), "assemble 0")?;
    zero_averages(&model)?;
    set_at_min(&mut model, "damage", &|t| t[0] = 0.9)?;
    ok(model.assemble_internal_forces(), "assemble 1")?;
    zero_averages(&model)
}

fn ghost_conn_set(mesh: &Mesh) -> std::result::Result<BTreeSet<Vec<i64>>, String> {
    let mut s = BTreeSet::new();
    for ty in mesh.element_types(
        akantu_core::TypeFilter::new()
            .dim(2)
            .ghost(GhostType::Ghost),
    ) {
        let conn = ok(mesh.connectivity(ty, GhostType::Ghost), "conn")?;
        for c in conn.tuples() {
            let mut g: Vec<i64> = c.iter().map(|&n| mesh.node_global_id(n)).collect();
            g.sort_unstable();
            s.insert(g);
        }
    }
    Ok(s)
}

/// `grad_u non local` is 0 at every owned point (UD-1104).
fn zero_averages(model: &SolidMechanicsModel) -> Outcome {
    let avg = averages_by_id(model)?;
    if avg.values().flatten().any(|&v| v != 0.) {
        return Err(
            "grad_u non local is not 0 (the program's grad_u is recomputed from u = 0)".into(),
        );
    }
    if avg.is_empty() {
        return Err("no averaged point".into());
    }
    Ok(())
}

/// MPI-050 (`_nh_criterion`, the intended `NeighborhoodMaxCriterion` on a
/// distributed mesh; no upstream behaviour: UD-563): `Ehat` of a local
/// Mazars law under a prescribed displacement, the maximal owned points of
/// every rank by global identity equal the serial ones (no tie in this
/// field).
pub fn max_criterion_vs_serial(comm: Communicator) -> Outcome {
    max_criterion_vs_serial_on(comm, "parallel_test.msh", 3, false)
}

/// [`max_criterion_vs_serial`] on `pair_test.msh`
/// (quadrangles, four integration points per element) with an oscillating
/// displacement on a uniform stretch (`Ehat` positive everywhere and free
/// of ties: a tie across ranks keeps both points, module docs of
/// `NeighborhoodMaxCriterion`) and a radius below the element size (83
/// maxima, many next to the partition boundaries): `_nh_criterion` uses
/// the exchanges' per-point slot mapping, and unpacking the points of each
/// element in reverse order changes the maxima
/// (86 at 2 ranks).
pub fn max_criterion_multi_point_vs_serial(comm: Communicator) -> Outcome {
    max_criterion_vs_serial_on(comm, "pair_test.msh", 4, true)
}

/// [`max_criterion_vs_serial`] on the upstream mesh `mesh_name`, whose
/// elements have `nodes_per_element` nodes (checked on the maxima);
/// `oscillating`: a displacement with many local maxima of `Ehat` and the
/// radius 0.1.
fn max_criterion_vs_serial_on(
    comm: Communicator,
    mesh_name: &'static str,
    nodes_per_element: usize,
    oscillating: bool,
) -> Outcome {
    // the oscillating case uses a radius below the element size (0.18), so
    // the neighbours of a point are its own element's and the adjacent
    // elements' points
    let dat = format!(
        "material mazars [\n  name = body\n  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n]\n\
         neighborhood max [\n  radius = {}\n]\n",
        if oscillating { "0.1" } else { "0.3" }
    );
    let run = |comm: &Communicator| -> std::result::Result<BTreeSet<PointId>, String> {
        let mesh = if comm.size() > 1 {
            upstream_mesh(comm, mesh_name)?
        } else {
            let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
            ok(
                read_mesh(&mut m, &upstream_data(mesh_name), MeshIoType::Gmsh),
                "read",
            )?;
            m
        };
        let parser = ok(Parser::parse_str(&dat, "material.dat"), "parser")?;
        let section = parser
            .root
            .sub_sections_of(akantu_parser::ParserType::Neighborhood)
            .next()
            .cloned()
            .ok_or("no neighborhood section")?;
        let mut model = ok(
            SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser),
            "model",
        )?;
        init(&mut model)?;
        let nodes = {
            let mesh_h = model.physics.base.mesh.clone();
            let mesh = ok(mesh_h.read(), "mesh")?;
            mesh.nodes().as_slice().to_vec()
        };
        {
            let mut u = ok(model.displacement_mut(), "u")?;
            for (n, uu) in u
                .as_mut_slice()
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .enumerate()
            {
                let (x, y) = (nodes[2 * n], nodes[2 * n + 1]);
                if oscillating {
                    uu[0] = 1e-4 * (0.1 * (7.0 * x + 0.3).sin() * (5.0 * y - 0.2).cos() + x);
                    uu[1] = 1e-4
                        * (0.1 * (6.0 * x).cos() * (8.0 * y + 0.5).sin() + 0.8 * y + 0.05 * y * x);
                } else {
                    uu[0] = 1e-4 * (0.6 * x + 0.3 * y * y + 0.7 * x * x * y);
                    uu[1] = 1e-4 * (0.2 * x * y - 0.1 * y + 0.4 * y * y * x);
                }
            }
        }
        ok(model.assemble_internal_forces(), "assemble")?;
        let crit = ok(
            model.new_neighborhood_max_criterion("Ehat", "max", &section),
            "max criterion",
        )?;
        let found = ok(model.find_max_quads(&crit), "find_max_quads")?;
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        let mut out = BTreeSet::new();
        for q in found {
            let e = akantu_core::Element::new(q.ty, q.element, q.ghost);
            let mut c: Vec<i64> = ok(mesh.element_connectivity(e), "conn")?
                .iter()
                .map(|&n| {
                    if mesh.is_distributed() {
                        mesh.node_global_id(n)
                    } else {
                        i64::from(n)
                    }
                })
                .collect();
            c.sort_unstable();
            out.insert((c, q.num_point));
        }
        Ok(out)
    };
    let serial = run(&Communicator::serial())?;
    let local = run(&comm)?;
    // union over the ranks
    let mut buf: Vec<i64> = Vec::new();
    for (c, q) in &local {
        buf.push(c.len() as i64);
        buf.extend(c);
        buf.push(i64::from(*q));
    }
    let all = ok(comm.all_gather_v(&buf), "all_gather")?;
    let mut union = BTreeSet::new();
    for b in all {
        let mut i = 0;
        while i < b.len() {
            let n = b[i] as usize;
            union.insert((b[i + 1..i + 1 + n].to_vec(), b[i + 1 + n] as Idx));
            i += n + 2;
        }
    }
    if comm.rank() == 0 {
        eprintln!(
            "INFO max_criterion {mesh_name} np{}: {} maxima (serial {})",
            comm.size(),
            union.len(),
            serial.len()
        );
    }
    if serial.iter().any(|(c, _)| c.len() != nodes_per_element) {
        return Err(format!(
            "a maximum of {mesh_name} is not on an element of {nodes_per_element} nodes"
        ));
    }
    if union != serial || serial.is_empty() {
        return Err(format!(
            "maxima differ from serial: {} distributed, {} serial",
            union.len(),
            serial.len()
        ));
    }
    Ok(())
}

/// UD-1103 (`_mnl_weight` of `damage_wf`): upstream's `DamagedWeightFunction`
/// packs nothing, so its ghost points are weighed with the ghost elements'
/// own (never computed) damage. Default: the owners' damage is exchanged and
/// the averages equal the serial ones within the summation-order bound;
/// `WeightExchange::Upstream`: they differ near the partition boundaries
/// (on more than one rank).
pub fn damage_wf_exchange(comm: Communicator) -> Outcome {
    const DAT: &str = "material par4_region_material [\n  name = mat_1\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
material par4_region_material [\n  name = mat_2\n  rho = 1\n  E = 1\n  nu = 0\n]\n\
non_local test_region damage_wf [\n  radius = 0.5\n  weight_function weight_parameter [\n    update_rate = 1\n  ]\n]\n";
    let run = |comm: &Communicator,
               exchange: akantu_model::non_local::WeightExchange|
     -> std::result::Result<BTreeMap<PointId, Vec<Real>>, String> {
        let mesh = if comm.size() > 1 {
            upstream_mesh(comm, "parallel_test.msh")?
        } else {
            let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
            ok(
                read_mesh(
                    &mut m,
                    &upstream_data("parallel_test.msh"),
                    MeshIoType::Gmsh,
                ),
                "read",
            )?;
            m
        };
        let mut model = model(mesh, DAT, true, Sequence::Corrected)?;
        model.physics.non_local_options.weight_exchange = exchange;
        init(&mut model)?;
        set_eigen_grad_u(&mut model)?;
        // damage 0.5 on a band (not removed by damage_wf, which weighs it)
        for m in 0..model.nb_materials() {
            let blocks: Vec<(ElementType, Vec<Idx>)> = {
                let law = ok(model.material(m), "law")?;
                law.element_filter()
                    .as_map()
                    .iter_all()
                    .filter(|(_, g, _)| *g == GhostType::NotGhost)
                    .map(|(ty, _, f)| (ty, f.as_slice().to_vec()))
                    .collect()
            };
            for (ty, filter) in blocks {
                let x = point_coords(&model, ty, &filter)?;
                let law = ok(model.material_mut(m), "law")?;
                law.for_each_internal_mut(&mut |n, f| {
                    if n == "damage"
                        && let akantu_model::constitutive_law::InternalFieldMut::Real(f) = f
                        && let Ok(mut d) = f.get_mut(ty, GhostType::NotGhost)
                    {
                        for (p, xy) in x.as_chunks::<2>().0.iter().enumerate() {
                            d.as_mut_slice()[p] = if (xy[0] - 0.1).abs() < 0.3 { 0.5 } else { 0. };
                        }
                    }
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
            }
        }
        ok(model.assemble_internal_forces(), "assemble")?;
        averages_by_id(&model)
    };
    use akantu_model::non_local::WeightExchange;
    let serial = run(&Communicator::serial(), WeightExchange::AllInternals)?;
    let scale = serial
        .values()
        .flatten()
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    let worst = |d: &BTreeMap<PointId, Vec<Real>>| -> std::result::Result<Real, String> {
        let mut w: Real = 0.;
        for (k, v) in d {
            let s = serial.get(k).ok_or("point not in the serial run")?;
            for (a, b) in v.iter().zip(s) {
                let e = (a - b).abs() / scale;
                w = if e.is_nan() { Real::INFINITY } else { w.max(e) };
            }
        }
        Ok(w)
    };
    let default = gather_all(&comm, &run(&comm, WeightExchange::AllInternals)?)?;
    let compat = gather_all(&comm, &run(&comm, WeightExchange::Upstream)?)?;
    let (wd, wc) = (worst(&default)?, worst(&compat)?);
    if comm.rank() == 0 {
        eprintln!(
            "INFO damage_wf np{}: default {wd:e}, upstream {wc:e}",
            comm.size()
        );
    }
    if wd.is_nan() || wd > 1e-14 {
        return Err(format!("default differs from serial by {wd:e}"));
    }
    if comm.size() > 1 && (wc.is_nan() || wc <= 1e-10) {
        return Err(format!(
            "the UD-1103 compat switch changes nothing ({wc:e})"
        ));
    }
    Ok(())
}

/// Internal force of every owned node by global id.
fn owned_forces(
    model: &SolidMechanicsModel,
) -> std::result::Result<BTreeMap<i64, Vec<Real>>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let f = ok(model.internal_force(), "f")?;
    let d = mesh.is_distributed();
    let mut out = BTreeMap::new();
    for (k, ff) in f.as_slice().as_chunks::<2>().0.iter().enumerate() {
        let n = Idx::try_from(k).map_err(|e| e.to_string())?;
        if d && !mesh.node_flag(n).is_local_or_master() {
            continue;
        }
        out.insert(
            if d { mesh.node_global_id(n) } else { k as i64 },
            ff.to_vec(),
        );
    }
    Ok(out)
}

/// UD-1101: a mesh mixing a non-local law (`mazars_non_local`, mat_1) and a
/// local one (`elastic`, mat_2). Default: the cleanup keeps the
/// distribution's ghost layer, and the internal forces of the owned nodes
/// equal the serial ones within the summation-order bound `rel`; upstream's
/// cleanup (compat) erases the local law's ghost elements of that layer.
pub fn mixed_cleanup_ud1101(comm: Communicator, rel: Real) -> Outcome {
    let dat = mazars_dat("nl_mixed");
    let run = |comm: &Communicator,
               compat: bool|
     -> std::result::Result<(BTreeMap<i64, Vec<Real>>, bool), String> {
        let mesh = if comm.size() > 1 {
            upstream_mesh(comm, "parallel_test.msh")?
        } else {
            let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
            ok(
                read_mesh(
                    &mut m,
                    &upstream_data("parallel_test.msh"),
                    MeshIoType::Gmsh,
                ),
                "read",
            )?;
            m
        };
        let before = if mesh.is_distributed() {
            ghost_conn_set(&mesh)?
        } else {
            BTreeSet::new()
        };
        let mut model = model(mesh, &dat, true, Sequence::Corrected)?;
        model
            .physics
            .upstream_compat
            .non_local_cleanup_erases_distribution_ghosts = compat;
        init(&mut model)?;
        let kept = {
            let mesh_h = model.physics.base.mesh.clone();
            let mesh = ok(mesh_h.read(), "mesh")?;
            !mesh.is_distributed() || before.is_subset(&ghost_conn_set(&mesh)?)
        };
        let nodes = {
            let mesh_h = model.physics.base.mesh.clone();
            let mesh = ok(mesh_h.read(), "mesh")?;
            mesh.nodes().as_slice().to_vec()
        };
        {
            let mut u = ok(model.displacement_mut(), "u")?;
            for (n, uu) in u
                .as_mut_slice()
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .enumerate()
            {
                let (x, y) = (nodes[2 * n], nodes[2 * n + 1]);
                uu[0] = 5e-4 * (0.6 * x + 0.3 * y * y);
                uu[1] = 5e-4 * (0.2 * x * y - 0.1 * y);
            }
        }
        ok(model.assemble_internal_forces(), "assemble")?;
        Ok((owned_forces(&model)?, kept))
    };
    let (serial, _) = run(&Communicator::serial(), false)?;
    let scale = serial
        .values()
        .flatten()
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    let (dist, kept) = run(&comm, false)?;
    let (_, kept_compat) = run(&comm, true)?;
    let mut worst: Real = 0.;
    for (g, f) in &dist {
        let s = serial.get(g).ok_or("node not in the serial run")?;
        for (a, b) in f.iter().zip(s) {
            let e = (a - b).abs() / scale;
            worst = if e.is_nan() {
                Real::INFINITY
            } else {
                worst.max(e)
            };
        }
    }
    let mut w = [worst];
    ok(comm.all_reduce(&mut w, akantu_comm::ReduceOp::Max), "max")?;
    let mut flags = [u64::from(!kept), u64::from(!kept_compat)];
    ok(
        comm.all_reduce(&mut flags, akantu_comm::ReduceOp::Sum),
        "sum",
    )?;
    if comm.rank() == 0 {
        eprintln!(
            "INFO mixed np{}: owned internal force vs serial {:e}; ranks losing ghosts: default {}, upstream {}",
            comm.size(),
            w[0],
            flags[0],
            flags[1]
        );
    }
    if flags[0] != 0 {
        return Err("the default cleanup removed ghost elements of the distribution".into());
    }
    if comm.size() > 1 && flags[1] == 0 {
        return Err(
            "upstream's cleanup (compat) removed no ghost element of the distribution".into(),
        );
    }
    if w[0].is_nan() || w[0] > rel {
        return Err(format!("internal forces differ from serial by {:e}", w[0]));
    }
    Ok(())
}

/// Agreed failure: every rank errs with `fatal` (or any error when
/// `fatal` is false) promptly, and the communicator stays usable.
fn agreed_failure(
    comm: &Communicator,
    r: Result<()>,
    t0: std::time::Instant,
    fatal: bool,
) -> Outcome {
    let e = match r {
        Ok(()) => return Err("a rank-specific failure was not reported on this rank".into()),
        Err(e) => e,
    };
    if fatal && !matches!(e, akantu_core::Error::Fatal { .. }) {
        return Err(format!("expected an agreed fatal error, got: {e}"));
    }
    if t0.elapsed() > std::time::Duration::from_secs(10) {
        return Err(format!("the failure was not prompt ({:?})", t0.elapsed()));
    }
    let mut one = [1u64];
    ok(
        comm.all_reduce(&mut one, akantu_comm::ReduceOp::Sum),
        "all_reduce",
    )?;
    if one[0] != comm.size() as u64 {
        return Err("the communicator is not usable after the failure".into());
    }
    Ok(())
}

/// Rank-specific failure of the distributed non-local initialisation (the
/// last rank's input has no `non_local` section: its neighbourhood cannot
/// be created in the owned-points stage): every rank's `init_full` fails at
/// the same agreement, as a fatal error (the laws are initialised; rebuild
/// the model), promptly, the communicator usable.
pub fn nl_init_failure_is_agreed(comm: Communicator) -> Outcome {
    if comm.size() == 1 {
        return Ok(());
    }
    let last = comm.rank() as usize == comm.size() - 1;
    let dat = if last {
        REGION_DAT
            .split("non_local")
            .next()
            .unwrap_or(REGION_DAT)
            .to_owned()
    } else {
        REGION_DAT.to_owned()
    };
    let mesh = upstream_mesh(&comm, "parallel_test.msh")?;
    let mut model = model(mesh, &dat, true, Sequence::Corrected)?;
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    agreed_failure(&comm, r, t0, true)
}

/// Rank-specific failure inside the averaging cycle (the last rank's
/// manager invalidated: it refuses to average before the exchanges): every
/// rank's `assemble_internal_forces` fails at the same exchange, promptly,
/// the communicator usable.
pub fn nl_cycle_failure_is_agreed(comm: Communicator) -> Outcome {
    if comm.size() == 1 {
        return Ok(());
    }
    let mesh = upstream_mesh(&comm, "parallel_test.msh")?;
    let mut model = model(mesh, REGION_DAT, true, Sequence::Corrected)?;
    init(&mut model)?;
    set_eigen_grad_u(&mut model)?;
    ok(model.assemble_internal_forces(), "first cycle")?;
    if comm.rank() as usize == comm.size() - 1 {
        let m = model.non_local_manager_mut().ok_or("no manager")?;
        let o = m.options();
        m.set_options(o);
    }
    let t0 = std::time::Instant::now();
    let r = model.assemble_internal_forces();
    agreed_failure(&comm, r, t0, false)
}

/// Rank-specific failure of the max criterion's `_nh_criterion` step (the
/// last rank asks with a criterion that is not an internal of any law):
/// every rank fails at the exchange, promptly, the communicator usable.
pub fn max_criterion_failure_is_agreed(comm: Communicator) -> Outcome {
    if comm.size() == 1 {
        return Ok(());
    }
    const DAT: &str = "material mazars [\n  name = body\n  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n]\n\
                       neighborhood max [\n  radius = 0.3\n]\n";
    let mesh = upstream_mesh(&comm, "parallel_test.msh")?;
    let parser = ok(Parser::parse_str(DAT, "material.dat"), "parser")?;
    let section = parser
        .root
        .sub_sections_of(akantu_parser::ParserType::Neighborhood)
        .next()
        .cloned()
        .ok_or("no neighborhood section")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser),
        "model",
    )?;
    init(&mut model)?;
    let crit = ok(
        model.new_neighborhood_max_criterion("Ehat", "max", &section),
        "max criterion",
    )?;
    // a corrupt layout on the last rank: the exchange's preflight refuses it
    let crit = if comm.rank() as usize == comm.size() - 1 {
        let mesh_h = model.physics.base.mesh.clone();
        let m = ok(mesh_h.read(), "mesh")?;
        let fe = ok(model.physics.base.fems.get(""), "fe")?;
        let mut c = crit;
        // a layout without the ghost blocks: the ghost slots of the scheme
        // elements do not exist on this rank
        let mut serial_like = ok(
            akantu_model::non_local::max_criterion::NeighborhoodMaxCriterion::new(
                &m, fe, "Ehat", "max", &section, false,
            ),
            "serial criterion",
        )?;
        if let Some(s) = c.synchronizer() {
            serial_like.set_synchronizer(s.clone());
        }
        c = serial_like;
        c
    } else {
        crit
    };
    let t0 = std::time::Instant::now();
    let r = model.find_max_quads(&crit).map(drop);
    agreed_failure(&comm, r, t0, false)
}

/// The numbers of nodes and of local and ghost elements per type: the
/// mesh state a refused initialisation must leave unchanged.
fn mesh_counts(mesh: &SharedMesh) -> std::result::Result<Vec<(String, usize)>, String> {
    let m = ok(mesh.read(), "mesh")?;
    let mut out = vec![("nodes".to_string(), m.nb_nodes())];
    for g in GhostType::ALL {
        for ty in m.element_types(akantu_core::TypeFilter::new().ghost(g)) {
            out.push((format!("{ty:?}/{g:?}"), m.nb_element(ty, g)));
        }
    }
    Ok(out)
}

/// FNV-1a of a message: compared across the ranks with one `all_gather`.
fn message_hash(msg: &str) -> u64 {
    msg.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// D-097 (1): the summation
/// route `SummationOrder::KeyOrder` is refused at `init_full` on a
/// distributed mesh (its key uses `QuadPoint::global_element`, which a
/// distributed mesh does not fill with partition-independent ids). On
/// `nl_region`'s distributed setting (the oracle's partition where
/// recorded, else bisection), on every rank: `init_full` returns
/// `Error::Unsupported` naming `KeyOrder`, with the same message on every
/// rank, promptly, and the mesh is unchanged (nodes, local and ghost
/// elements per type: the refusal comes before any ghost change). The
/// refusal is **recoverable**: it is agreed in the
/// selection phase of `init_materials`, before the selection is committed,
/// so the error is `Error::Unsupported(..)` itself (not an agreed fatal
/// error of the model). Then, **on the same model**, the default route and
/// `init_materials()` initialise it (only that third step of `init_full`
/// had failed), and `nl_region`'s first assembly gives internal forces
/// bitwise equal to those of a model on a freshly distributed mesh (the
/// refused attempt left no trace). One rank, mesh not distributed:
/// `KeyOrder` is accepted (as the serial tests `non_local_event_route.rs`).
pub fn key_order_refused_on_a_distributed_mesh(comm: Communicator) -> Outcome {
    use akantu_core::Error;
    use akantu_model::non_local::SummationOrder;
    let size = comm.size();
    let recorded = par_common::oracle_rank_counts("nl_region").contains(&size);
    let fx = if recorded {
        fixture("nl_region", &comm)?
    } else {
        Fixture::open_by_id("parallel/nl_region/np1/rank0").map_err(|e| e.to_string())?
    };
    let path = fx.input_path("mesh").map_err(|e| e.to_string())?;
    if size == 1 {
        let mut m = ok(
            Mesh::with_communicator(2, "mesh", Communicator::serial()),
            "mesh",
        )?;
        ok(read_mesh(&mut m, &path, MeshIoType::Gmsh), "read")?;
        let mut model = model(m, REGION_DAT, true, Sequence::Corrected)?;
        model.physics.non_local_options.order = SummationOrder::KeyOrder;
        init(&mut model)?;
        set_eigen_grad_u(&mut model)?;
        return ok(
            model.assemble_internal_forces(),
            "KeyOrder on a mesh that is not distributed: first assembly",
        );
    }
    let distributed = || -> std::result::Result<Mesh, String> {
        if recorded {
            distributed_mesh(&comm, "nl_region", size, &path)
        } else {
            let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
            if comm.rank() == 0 {
                ok(read_mesh(&mut m, &path, MeshIoType::Gmsh), "read")?;
            }
            par_common::distribute_rcb(&mut m)?;
            Ok(m)
        }
    };
    let shared = SharedMesh::new(distributed()?);
    let before = mesh_counts(&shared)?;
    let mut refused = model_on(shared.clone(), REGION_DAT, true, Sequence::Corrected)?;
    refused.physics.non_local_options.order = SummationOrder::KeyOrder;
    let t0 = std::time::Instant::now();
    let r = refused.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    let elapsed = t0.elapsed();
    // rank-local checks, agreed before the next collective call
    let local = (|| -> std::result::Result<String, String> {
        let e = match r {
            Ok(()) => return Err("KeyOrder was accepted on a distributed mesh".into()),
            Err(e) => e,
        };
        let msg = match &e {
            Error::Unsupported(msg) => msg.clone(),
            Error::Fatal { .. } => {
                return Err(format!(
                    "the refusal is fatal; expected the recoverable Error::Unsupported: {e}"
                ));
            }
            other => return Err(format!("expected Error::Unsupported, got: {other}")),
        };
        if !msg.contains("KeyOrder") {
            return Err(format!("the refusal does not name KeyOrder: {msg}"));
        }
        if elapsed > std::time::Duration::from_secs(10) {
            return Err(format!("the refusal was not prompt ({elapsed:?})"));
        }
        let after = mesh_counts(&shared)?;
        if after != before {
            return Err(format!(
                "the refused init_full changed the mesh: {before:?} -> {after:?}"
            ));
        }
        Ok(msg)
    })();
    let hashes = ok(
        comm.all_gather(&[local.as_ref().map_or(0, |m| message_hash(m))]),
        "gather the refusal messages",
    )?;
    let msg = local?;
    if hashes.iter().any(|&h| h != hashes[0]) {
        return Err(format!(
            "the refusal message differs between the ranks (this rank: {msg})"
        ));
    }
    // the same model, the default route: `init_materials` (the step of
    // `init_full` that refused) initialises it, and the first assembly
    // equals a fresh model's on a freshly distributed mesh (collective
    // steps: every rank enters them, the refusal having been agreed)
    refused.physics.non_local_options.order = SummationOrder::default();
    ok(
        refused.init_materials(),
        "init_materials of the same model after the refusal",
    )?;
    set_eigen_grad_u(&mut refused)?;
    ok(refused.assemble_internal_forces(), "first assembly")?;
    let after_refusal = ok(refused.internal_force(), "f_int")?.as_slice().to_vec();
    let fresh = {
        let mut model = model_on(
            SharedMesh::new(distributed()?),
            REGION_DAT,
            true,
            Sequence::Corrected,
        )?;
        init(&mut model)?;
        set_eigen_grad_u(&mut model)?;
        ok(model.assemble_internal_forces(), "first assembly")?;
        ok(model.internal_force(), "f_int")?.as_slice().to_vec()
    };
    bits(
        "internal force of the same model after the refusal vs a fresh distributed mesh",
        &after_refusal,
        &fresh,
    )
}
