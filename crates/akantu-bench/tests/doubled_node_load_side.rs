// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); acceptance test of
// D-105 / UD-1280 (the reproducer,
// inverted) on the set-up of performance-testing-cohesive @ 483bace8
// (cohesive_extrinsic.cc: the frozen h = 0.12 cube, material-cohesive.dat,
// the compression and shear tractions on "top", "bottom", "side left",
// "side right") and Akantu's solid_mechanics_model_cohesive.cc
// (`onNodesAdded`: the external force is not copied to a doubled node).

//! When cohesive insertion doubles a node of a loaded face, upstream gives
//! the new copy a zero external force and the original copy keeps the load
//! of every boundary element it had; which copy is the original follows
//! from the doubled facet's orientation, which on a distributed mesh
//! depends on the partition.
//! D-105 (default): the solid model records its Neumann loads per boundary
//! element and the doubling moves each record to the copy its element uses.
//!
//! Set-up: the benchmark's initial loads (compression on "top" / "bottom",
//! shear on the four groups), then every insertable facet whose barycentre
//! lies within 0.1 of "side left" / "side right" with |z| ≤ 0.5 inserted at
//! once, at 1 rank and at 2 / 4 ranks on the C++ harness's partitions.
//! Owned nodes are matched by reference position and side (the hash of
//! their attached tetrahedra, `akantu_bench::diag::node_keys`).
//!
//! **Default** (asserted, bounds relative to the largest serial nodal load,
//! the rule C1F-1 of `docs/NUMERICAL_VALIDATION.md`: summation order only):
//! 1. the external force per (position, side) at 2 and 4 ranks equals the
//!    serial one within 1e-14;
//! 2. the serial per-copy force equals a **re-application** of the same
//!    loads on the post-insertion topology (external force zeroed, the
//!    loads applied again: an independent computation) within 1e-14, and so
//!    does every rank's;
//! 3. the sum of the external force over all owned copies is unchanged by
//!    the doubling (1e-14 × the number of loaded nodes);
//! 4. on every rank, at **every** node (local, slave and ghost copies), the
//!    external force equals the sum of the recorded contributions of the
//!    rank's boundary elements at it (1e-14): the move reached every copy
//!    the rank holds (owned, slave and ghost) — the mutant "move skipped for
//!    ghost copies" (the copies a rank does not own) fails here,
//!    the mutant "contribution moved for an element that does not use the
//!    new copy" fails 1 and 2.
//!
//! **Switch** `doubled_node_keeps_external_force` (upstream's placement, the
//! Reproducer): at 2 or 4 ranks some doubled loaded node
//! keeps its load on another side than in the serial run, and the serial
//! per-copy force differs from the re-application by whole nodal loads.
//!
//! **The driver's per-copy check** (`akantu_bench::load_check`,
//! the mode `rust_load_check_*` of the benchmark driver) runs on the same
//! forced band at 1 / 2 / 4 ranks, right after the insertion: with the
//! default it passes (every copy of every rank within C1F-1's 1e-14 of the
//! largest nodal load; the driver's own verdict uses D-105 (3)'s 1e-12)
//! and its count of doubled loaded nodes equals this test's own count;
//! with the switch it fails by a whole nodal load; in both cases it
//! leaves the external force, the store's counters and its records bit for
//! bit as they were.
//!
//! **A rescaled external force** (D-108, serial): after
//! the loads the external force is written as `f ← 0.1 f`, which the
//! corrected default does not support; every record the insertion moves is
//! counted as taken from a written node and the insertion prints one
//! warning; with the switch nothing moves and nothing is printed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use akantu_bench::Config;
use akantu_bench::load_check::{self, GlobalCheck, LoadApplication};
use akantu_cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu_comm::{Communicator, run_local};
use akantu_core::debug::{DebugLevel, capture};
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::tensor::MatrixX;
use akantu_core::{Element, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter};
use akantu_mesh::distribution::{PrescribedPartition, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FromHigherDim;
use akantu_model::neumann_store::{MoveReport, NeumannStoreStats};

const SHEARED: [&str; 4] = ["top", "bottom", "side left", "side right"];
/// C1F-1 (`docs/NUMERICAL_VALIDATION.md`, 1e-14): a nodal force is a sum of about a dozen triangle
/// contributions per application, two applications here (≤ about 24
/// additions, ≤ 5e-15 a priori on either side); measured ≤ 2.7e-16
/// (the label said C1F-1 while the value was C1F-1's former 1e-12).
const RTOL: Real = 1e-14;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// (reference position bits, side key).
type Key = ([u64; 3], u64);

#[derive(Clone, Debug, Default)]
struct RankResult {
    /// owned nodes: key -> (new copy, external force, re-applied force)
    nodes: Vec<(Key, bool, [Real; 3], [Real; 3])>,
    /// sum of the external force over the owned nodes before / after
    total_before: [Real; 3],
    total_after: [Real; 3],
    /// max over every node of the rank |f − Σ records| (absolute)
    record_residual: Real,
    /// copies of the rank (all nodes, ghost included) that are new
    new_copies: usize,
    /// of them, copies the rank does not own (slave or pure ghost nodes)
    new_foreign_copies: usize,
    /// the driver's per-copy check right after the insertion (reduced)
    check: GlobalCheck,
}

fn read_parts(path: &Path) -> Vec<Idx> {
    let text = std::fs::read_to_string(path).expect("partition.csv");
    let mut tets: Vec<(usize, Idx)> = text
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            (f[0] == "_tetrahedron_4").then(|| (f[1].parse().unwrap(), f[2].parse().unwrap()))
        })
        .collect();
    tets.sort_unstable();
    assert!(tets.iter().enumerate().all(|(k, (e, _))| *e == k));
    tets.into_iter().map(|(_, p)| p).collect()
}

/// `Matrix<Real>{{a, b, c}, {d, e, f}, {g, h, i}}` (symmetric here).
fn sym3(m: [[Real; 3]; 3]) -> Result<FromHigherDim> {
    let mut data = Vec::with_capacity(9);
    for c in 0..3 {
        for r in m {
            data.push(r[c]);
        }
    }
    Ok(FromHigherDim::new(MatrixX::from_col_major(3, 3, data)?))
}

/// The benchmark's initial tractions (compression, shear).
fn tractions(cfg: &Config) -> Result<(FromHigherDim, FromHigherDim)> {
    let (c, s) = (cfg.compression, cfg.shear);
    Ok((
        sym3([[c, 0., 0.], [0., c, 0.], [0., 0., c]])?,
        sym3([[0., 0., s], [0., 0., 0.], [s, 0., 0.]])?,
    ))
}

fn apply_loads(model: &mut SolidMechanicsModelCohesive, cfg: &Config) -> Result<()> {
    let (compression, shear) = tractions(cfg)?;
    let smm = &mut model.smm;
    smm.external_force_mut()?.as_mut_slice().fill(0.0);
    for g in ["top", "bottom"] {
        smm.apply_neumann_bc(&compression, g)?;
    }
    for g in SHEARED {
        smm.apply_neumann_bc(&shear, g)?;
    }
    Ok(())
}

fn owned_total(m: &Mesh, f: &[Real]) -> Result<[Real; 3]> {
    let mut s = [0.0; 3];
    for n in 0..m.nb_nodes() {
        if m.is_local_or_master_node(Idx::try_from(n).unwrap())? {
            for d in 0..3 {
                s[d] += f[3 * n + d];
            }
        }
    }
    Ok(s)
}

/// The store's records, block by block (bitwise comparisons).
fn records(model: &SolidMechanicsModelCohesive) -> Vec<Vec<u64>> {
    let Some(st) = model.smm.neumann_store() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ghost in GhostType::ALL {
        for ty in st.forces().element_types(TypeFilter {
            spatial_dimension: None,
            ghost_type: ghost,
            kind: None,
        }) {
            let b = st.forces().get(ty, ghost).unwrap();
            out.push(b.as_slice().iter().map(|v| v.to_bits()).collect());
        }
    }
    out
}

/// The driver's per-copy check (`akantu_bench::load_check`) on the current
/// state, asserting that it changes nothing of the model.
fn driver_check(
    comm: &Communicator,
    shared: &SharedMesh,
    model: &SolidMechanicsModelCohesive,
    cfg: &Config,
    n0: usize,
) -> Result<GlobalCheck> {
    let (compression, shear) = tractions(cfg)?;
    let loads: [LoadApplication<'_>; 2] = [(&compression, &["top", "bottom"]), (&shear, &SHEARED)];
    let bits = |m: &SolidMechanicsModelCohesive| -> Result<Vec<u64>> {
        Ok(m.smm
            .external_force()?
            .as_slice()
            .iter()
            .map(|v| v.to_bits())
            .collect())
    };
    let (f0, s0, r0) = (
        bits(model)?,
        model.smm.neumann_store().map(|s| s.stats()),
        records(model),
    );
    let m = shared.read()?;
    let (_, global) = load_check::check(comm, &m, &model.smm, &loads, n0)?;
    drop(m);
    assert!(
        f0 == bits(model)?
            && s0 == model.smm.neumann_store().map(|s| s.stats())
            && r0 == records(model),
        "rank {}: the check changed the external force or the store",
        comm.rank()
    );
    Ok(global)
}

/// max over every node of the rank of |f − Σ recorded contributions|.
fn record_residual(m: &Mesh, model: &SolidMechanicsModelCohesive, f: &[Real]) -> Result<Real> {
    let st = model
        .smm
        .neumann_store()
        .ok_or_else(|| Error::invalid("no Neumann store"))?;
    let mut sum = vec![0.0; f.len()];
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
                    for d in 0..3 {
                        sum[3 * n as usize + d] += block.tuple(e)[3 * k + d];
                    }
                }
            }
        }
    }
    Ok(f.iter()
        .zip(&sum)
        .fold(0.0_f64, |a, (x, y)| a.max((x - y).abs())))
}

/// The model on the frozen h = 0.12 cube (distributed on `partition` at
/// more than one rank), upstream's laws, the load placement of `keep`.
fn setup(
    comm: &Communicator,
    partition: Option<&Path>,
    keep: bool,
) -> Result<(SharedMesh, SolidMechanicsModelCohesive, Config)> {
    let dir = fixtures().join("perf_cohesive/coh_np2_global");
    let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join("doubled_node_load_side");
    let cfg = Config::from_file(&dir.join("input.dat"), &dir, &out)?;
    let root = comm.rank() == 0;
    let mut mesh = Mesh::with_communicator(3, "mesh", comm.clone())?;
    if root {
        akantu_io::read_mesh(&mut mesh, &cfg.mesh_path(), MeshIoType::Gmsh)?;
    }
    if comm.size() > 1 {
        let mut p = PrescribedPartition::new(if root {
            read_parts(partition.expect("partition"))
        } else {
            Vec::new()
        });
        // no consumer of the distribution events in this test
        let _events = distribute_with(
            &mut mesh,
            if root {
                Some(&mut p as &mut dyn akantu_mesh::distribution::MeshPartitioner)
            } else {
                None
            },
            0,
        )?;
    }
    let shared = SharedMesh::new(mesh);
    let mut model = SolidMechanicsModelCohesive::with_parser(
        shared.clone(),
        Some(3),
        akantu_cohesive::model::DEFAULT_ID,
        cfg.parser.clone(),
    )?;
    model.ext.compat = CohesiveUpstreamCompat::upstream();
    model.ext.compat.doubled_node_keeps_external_force = keep;
    model.init_full(&ModelOptions::new(AnalysisMethod::Static), true)?;
    Ok((shared, model, cfg))
}

/// Flag every insertable facet whose barycentre lies within 0.1 of "side
/// left" / "side right" with |z| ≤ 0.5 (the forced insertion of the
/// set-up; nothing is inserted yet).
fn flag_side_band(shared: &SharedMesh, model: &mut SolidMechanicsModelCohesive) -> Result<()> {
    let m = shared.read()?;
    let fm = m
        .mesh_facets()
        .ok_or_else(|| Error::invalid("no facet mesh"))?;
    let (check, insertion) = model.ext.inserter.flags_mut();
    let mut b = [0.0; 3];
    for ghost in [GhostType::NotGhost, GhostType::Ghost] {
        let Ok(chk) = check.get(ElementType::Triangle3, ghost) else {
            continue;
        };
        let chk = chk.as_slice().to_vec();
        let ins = insertion.get_mut(ElementType::Triangle3, ghost)?;
        let ins = ins.as_mut_slice();
        for (f, ok) in chk.iter().enumerate() {
            let e = Element::new(ElementType::Triangle3, Idx::try_from(f).unwrap(), ghost);
            fm.barycenter_into(e, &mut b)?;
            if *ok && b[0].abs() >= 0.9 && b[2].abs() <= 0.5 {
                ins[f] = true;
            }
        }
    }
    Ok(())
}

fn case(comm: &Communicator, partition: Option<&Path>, keep: bool) -> Result<RankResult> {
    let (shared, mut model, cfg) = setup(comm, partition, keep)?;
    apply_loads(&mut model, &cfg)?;
    let (n0, total_before) = {
        let m = shared.read()?;
        let f = model.smm.external_force()?;
        (m.nb_global_nodes(), owned_total(&m, f.as_slice())?)
    };

    // ---- forced insertion next to the side faces ------------------------------
    flag_side_band(&shared, &mut model)?;
    let stats = |m: &SolidMechanicsModelCohesive| {
        m.smm.neumann_store().map(|s| s.stats()).unwrap_or_default()
    };
    let before = stats(&model);
    let (inserted, out) = capture(DebugLevel::WARNING, || model.insert_elements());
    inserted?;
    // No model path writes the external force
    // between the loads and the move on any rank, so the D-108 rule is not
    // exercised here: no foreign write, no record counted, no warning
    let after = stats(&model);
    assert_eq!(
        (after.foreign_writes, after.moved_from_written_nodes),
        (before.foreign_writes, before.moved_from_written_nodes),
        "rank {}: {after:?}",
        comm.rank()
    );
    assert!(
        !out.contains("Neumann store"),
        "rank {}: {out}",
        comm.rank()
    );

    // ---- after the doubling ---------------------------------------------------------
    let check = driver_check(comm, &shared, &model, &cfg, n0)?;
    let mut res = RankResult {
        total_before,
        check,
        ..RankResult::default()
    };
    let f_moved = model.smm.external_force()?.as_slice().to_vec();
    {
        let m = shared.read()?;
        res.total_after = owned_total(&m, &f_moved)?;
        res.record_residual = record_residual(&m, &model, &f_moved)?;
        for n in 0..m.nb_nodes() {
            let ni = Idx::try_from(n).unwrap();
            if usize::try_from(m.node_global_id(ni)).unwrap() >= n0 {
                res.new_copies += 1;
                if !m.is_local_or_master_node(ni)? {
                    res.new_foreign_copies += 1;
                }
            }
        }
    }
    // the independent computation: the same loads applied again on the
    // post-insertion topology
    apply_loads(&mut model, &cfg)?;
    let f_again = model.smm.external_force()?.as_slice().to_vec();
    let m = shared.read()?;
    let keys = akantu_bench::diag::node_keys(&m)?;
    for (n, key) in keys.into_iter().enumerate() {
        let ni = Idx::try_from(n).unwrap();
        if !m.is_local_or_master_node(ni)? {
            continue;
        }
        let is_new = usize::try_from(m.node_global_id(ni)).unwrap() >= n0;
        let r = 3 * n..3 * n + 3;
        res.nodes.push((
            key,
            is_new,
            f_moved[r.clone()].try_into().unwrap(),
            f_again[r].try_into().unwrap(),
        ));
    }
    Ok(res)
}

fn on(np: usize, partition: Option<&Path>, keep: bool) -> Vec<RankResult> {
    run_local(np, Duration::from_secs(1800), |c| case(&c, partition, keep))
        .into_iter()
        .enumerate()
        .map(|(r, o)| o.unwrap_or_else(|e| panic!("np {np} rank {r}: {e}")))
        .collect()
}

type ByKey = BTreeMap<Key, (bool, [Real; 3], [Real; 3])>;

fn by_key(np: usize, ranks: &[RankResult]) -> ByKey {
    let mut got = ByKey::new();
    for r in ranks {
        for (k, n, f, a) in &r.nodes {
            assert!(
                got.insert(*k, (*n, *f, *a)).is_none(),
                "np {np}: node owned twice"
            );
        }
    }
    got
}

/// Statistics of a run against the serial one: copies whose original/new
/// status differs, of them loaded, max |Δf| vs serial, max |f − re-applied|
/// (both relative to `fmax`).
fn compare(
    np: usize,
    serial: &ByKey,
    ranks: &[RankResult],
    fmax: Real,
) -> (usize, usize, Real, Real) {
    let got = by_key(np, ranks);
    let ka: BTreeSet<&Key> = serial.keys().collect();
    let kb: BTreeSet<&Key> = got.keys().collect();
    assert_eq!(
        ka, kb,
        "np {np}: the same insertion gives the same nodes and sides"
    );
    let (mut flips, mut loaded, mut worst, mut reapply) = (0, 0, 0.0_f64, 0.0_f64);
    for (k, (na, fa, _)) in serial {
        let (nb, fb, ab) = got[k];
        if *na != nb {
            flips += 1;
            if fa.iter().chain(fb.iter()).any(|v| *v != 0.0) {
                loaded += 1;
            }
        }
        for d in 0..3 {
            worst = worst.max((fa[d] - fb[d]).abs() / fmax);
            reapply = reapply.max((fb[d] - ab[d]).abs() / fmax);
        }
    }
    eprintln!(
        "np {np}: {} owned nodes; original/new status differs from serial at {flips} copies \
         ({loaded} loaded); max |f − f_serial| / max|f| = {worst:.3e}; max |f − f_reapplied| / \
         max|f| = {reapply:.3e}",
        got.len()
    );
    (flips, loaded, worst, reapply)
}

fn checks_per_rank(np: usize, ranks: &[RankResult], fmax: Real, nb_loaded: usize) {
    let mut before = [0.0; 3];
    let mut after = [0.0; 3];
    for (r, rr) in ranks.iter().enumerate() {
        eprintln!(
            "np {np} rank {r}: max |f − Σ records| over every node = {:.3e} of max|f|; new copies \
             {} ({} not owned: slave or ghost)",
            rr.record_residual / fmax,
            rr.new_copies,
            rr.new_foreign_copies
        );
        assert!(
            rr.record_residual <= RTOL * fmax,
            "np {np} rank {r}: the external force of some copy is not the sum of its elements' \
             records ({:.3e} of max|f|)",
            rr.record_residual / fmax
        );
        for d in 0..3 {
            before[d] += rr.total_before[d];
            after[d] += rr.total_after[d];
        }
    }
    #[allow(clippy::cast_precision_loss)] // a node count
    let bound = RTOL * fmax * nb_loaded as Real;
    for d in 0..3 {
        assert!(
            (before[d] - after[d]).abs() <= bound,
            "np {np}: the doubling changed the total load {d}: {} vs {}",
            before[d],
            after[d]
        );
    }
}

/// The doubled loaded copies by this test's own data: owned new copies
/// whose re-applied force is not zero.
fn doubled_loaded(ranks: &[RankResult]) -> u64 {
    ranks
        .iter()
        .flat_map(|r| r.nodes.iter())
        .filter(|(_, new, _, again)| *new && again.iter().any(|v| *v != 0.0))
        .count() as u64
}

/// The driver's check agrees on every rank and passes, with doubled loaded
/// nodes (this test's count) and moved records.
fn driver_check_passes(np: usize, ranks: &[RankResult]) {
    let c = &ranks[0].check;
    eprintln!(
        "np {np}: driver check: max |f − f_reapplied| / max|f| = {:.3e} (owned {:.3e}) over {} \
         copies differing ({} owned); {} doubled loaded nodes, {} records moved (sum over the \
         ranks), worst {:?}",
        c.max_rel,
        c.max_rel_owned,
        c.differing_copies,
        c.differing_owned,
        c.doubled_loaded,
        c.moved,
        c.worst
            .map(|w| (w.rank, w.gid, load_check::flag_name(w.flag)))
    );
    assert!(
        ranks.iter().all(|r| r.check == *c),
        "np {np}: ranks disagree"
    );
    assert!(
        c.passes() && c.max_rel <= RTOL,
        "np {np}: driver check fails ({:.3e}; the driver's bound {:e}, C1F-1 {RTOL:e})",
        c.max_rel,
        load_check::RTOL
    );
    assert_eq!(c.doubled_loaded, doubled_loaded(ranks), "np {np}");
    assert!(
        c.doubled_loaded > 100 && c.moved > 0,
        "np {np}: vacuous check"
    );
    assert_eq!(c.ranks_with_store, np as u64);
    assert_eq!((c.foreign_writes, c.moved_from_written), (0, 0));
}

fn serial_reference(keep: bool) -> (ByKey, Vec<RankResult>, Real, usize) {
    let serial = on(1, None, keep);
    let s = by_key(1, &serial);
    let fmax = s
        .values()
        .flat_map(|(_, f, _)| f.iter())
        .fold(0.0_f64, |a, v| a.max(v.abs()));
    let loaded = s
        .values()
        .filter(|(_, f, _)| f.iter().any(|v| *v != 0.0))
        .count();
    let new = s.values().filter(|(n, _, _)| *n).count();
    assert!(new > 100, "the forced insertion must double nodes");
    (s, serial, fmax, loaded)
}

/// D-105 acceptance: the load follows the boundary elements at every rank
/// count (default).
#[test]
fn doubled_loaded_nodes_carry_the_load_of_their_own_elements() {
    let (serial, serial_ranks, fmax, nb_loaded) = serial_reference(false);
    checks_per_rank(1, &serial_ranks, fmax, nb_loaded);
    driver_check_passes(1, &serial_ranks);
    let (_, _, _, reapply1) = compare(1, &serial, &serial_ranks, fmax);
    assert!(
        reapply1 <= RTOL,
        "serial: the per-copy force differs from the re-application ({reapply1:.3e})"
    );
    let p2 = fixtures().join("perf_cohesive_x86_64/coh_np2_global/partition.csv");
    let p4 = fixtures().join("perf_cohesive_mumps_x86_64/partition-np4.csv");
    for (np, p) in [(2, p2), (4, p4)] {
        let ranks = on(np, Some(&p), false);
        checks_per_rank(np, &ranks, fmax, nb_loaded);
        driver_check_passes(np, &ranks);
        let (flips, _, worst, reapply) = compare(np, &serial, &ranks, fmax);
        assert!(
            flips > 0,
            "np {np}: the case must swap original copies against serial (else it proves nothing)"
        );
        assert!(
            worst <= RTOL,
            "np {np}: external force per copy differs from the serial one ({worst:.3e})"
        );
        assert!(
            reapply <= RTOL,
            "np {np}: external force per copy differs from the re-application ({reapply:.3e})"
        );
        let foreign: usize = ranks.iter().map(|r| r.new_foreign_copies).sum();
        assert!(
            foreign > 0,
            "np {np}: the case must double copies a rank does not own"
        );
    }
}

/// With upstream's placement the driver's check fails by a whole nodal load
/// (the negative control), and nothing moved.
fn driver_check_fails(np: usize, ranks: &[RankResult]) {
    let c = &ranks[0].check;
    eprintln!(
        "switch, np {np}: driver check: max |f − f_reapplied| / max|f| = {:.3e}; {} doubled \
         loaded nodes, worst {:?}",
        c.max_rel, c.doubled_loaded, c.worst
    );
    assert!(
        ranks.iter().all(|r| r.check == *c),
        "np {np}: ranks disagree"
    );
    assert!(!c.passes() && c.max_rel > 0.1, "np {np}: {:.3e}", c.max_rel);
    assert_eq!(c.doubled_loaded, doubled_loaded(ranks), "np {np}");
    assert!(c.doubled_loaded > 100 && c.moved == 0, "np {np}");
}

/// The switch keeps upstream's placement: the reproducer
/// (a doubled loaded node keeps its load on a partition-dependent side).
#[test]
fn with_the_switch_the_load_side_depends_on_the_partition() {
    let (serial, serial_ranks, fmax, _) = serial_reference(true);
    let (_, _, _, reapply1) = compare(1, &serial, &serial_ranks, fmax);
    eprintln!("switch, serial: max |f − f_reapplied| / max|f| = {reapply1:.3e}");
    driver_check_fails(1, &serial_ranks);
    assert!(
        reapply1 > 0.1,
        "upstream's placement differs from the re-application by whole nodal loads"
    );
    let p2 = fixtures().join("perf_cohesive_x86_64/coh_np2_global/partition.csv");
    let p4 = fixtures().join("perf_cohesive_mumps_x86_64/partition-np4.csv");
    let r2 = on(2, Some(&p2), true);
    let r4 = on(4, Some(&p4), true);
    driver_check_fails(2, &r2);
    driver_check_fails(4, &r4);
    let (_, loaded2, worst2, _) = compare(2, &serial, &r2, fmax);
    let (_, loaded4, worst4, _) = compare(4, &serial, &r4, fmax);
    assert!(
        loaded2 + loaded4 > 0,
        "a loaded copy swapped against serial"
    );
    assert!(worst2.max(worst4) > 0.1, "a whole nodal load misplaced");
}

/// One serial run of the case: the loads, then `f ← λ f` written into the
/// external force (λ = 0.1, a load ramp), then the forced insertion,
/// captured at the warning level. Returns the misplacement against λ × a
/// re-application of the loads on the new topology (relative to its
/// largest nodal force), the number of nodes with a component of the wrong
/// sign, the store's counters and the captured messages.
fn rescaled(keep: bool) -> Result<(Real, usize, NeumannStoreStats, MoveReport, String)> {
    const LAMBDA: Real = 0.1;
    let comm = Communicator::serial();
    let (shared, mut model, cfg) = setup(&comm, None, keep)?;
    apply_loads(&mut model, &cfg)?;
    for v in model.smm.external_force_mut()?.as_mut_slice() {
        *v *= LAMBDA;
    }
    flag_side_band(&shared, &mut model)?;
    let before = model
        .smm
        .neumann_store()
        .ok_or_else(|| Error::invalid("no Neumann store"))?
        .stats();
    let (inserted, out) = capture(DebugLevel::WARNING, || model.insert_elements());
    assert!(inserted? > 0, "the forced insertion inserts");
    let f = model.smm.external_force()?.as_slice().to_vec();
    let stats = model
        .smm
        .neumann_store()
        .ok_or_else(|| Error::invalid("no Neumann store"))?
        .stats();
    // the insertion's one move, from the counters
    let report = MoveReport {
        moved: usize::try_from(stats.moved_contributions - before.moved_contributions).unwrap(),
        moved_from_written_nodes: usize::try_from(
            stats.moved_from_written_nodes - before.moved_from_written_nodes,
        )
        .unwrap(),
        ..MoveReport::default()
    };
    apply_loads(&mut model, &cfg)?;
    let ideal: Vec<Real> = model
        .smm
        .external_force()?
        .as_slice()
        .iter()
        .map(|v| LAMBDA * v)
        .collect();
    let fmax = ideal.iter().fold(0.0_f64, |a, v| a.max(v.abs()));
    let worst = f
        .iter()
        .zip(&ideal)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max)
        / fmax;
    let flipped = f
        .chunks(3)
        .zip(ideal.chunks(3))
        .filter(|(a, b)| {
            a.iter()
                .zip(*b)
                .any(|(x, y)| x * y < 0.0 && y.abs() > 1e-6 * fmax)
        })
        .count();
    Ok((worst, flipped, stats, report, out))
}

/// D-108 (case `f ← 0.1 f`). Rescaling
/// the Neumann loads by writing the external force is not supported by the
/// corrected default: the records still hold the unscaled loads and the
/// doubling moves them in full (worse than upstream's placement here:
/// measured 8.8 against 0.98
/// times the largest nodal force). It is now visible: every record the
/// insertion moves comes from a node written since it was recorded, the
/// store counts them (`moved_from_written_nodes`) and the insertion prints
/// exactly one warning. With the switch nothing moves: no count, no
/// warning.
#[test]
fn a_rescaled_external_force_is_counted_and_warned() {
    let (d, fd, stats, report, out) = rescaled(false).unwrap();
    let (u, fu, stats_u, report_u, out_u) = rescaled(true).unwrap();
    eprintln!(
        "λ = 0.1: max |f − λ f_reapplied| / max|λ f_reapplied|: default {d:.3e} ({fd} nodes with a \
         component of the wrong sign), upstream's placement {u:.3e} ({fu} nodes); default: \
         {report:?}, store {stats:?}; switch: store {stats_u:?}"
    );
    // the move itself is the corrected default's, unchanged
    assert!(d > u, "default {d} vs upstream {u}");
    // the count: every moved record comes from a written node here
    assert!(report.moved > 100, "{report:?}");
    assert_eq!(report.moved_from_written_nodes, report.moved, "{report:?}");
    assert_eq!(stats.moved_from_written_nodes, stats.moved_contributions);
    // the warning: once per move
    assert_eq!(
        out.matches("Neumann store (D-105, D-108, UD-1280)").count(),
        1,
        "one warning for the insertion's move:\n{out}"
    );
    assert!(
        out.contains(&format!(
            "{} recorded Neumann contribution(s) moved from",
            report.moved
        )) && out.contains("doubled_node_keeps_external_force"),
        "{out}"
    );
    // the switch: nothing moves, nothing is counted or printed
    assert_eq!((report_u.moved, stats_u.moved_from_written_nodes), (0, 0));
    assert!(!out_u.contains("Neumann store"), "{out_u}");
}
