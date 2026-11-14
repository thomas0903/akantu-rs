// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/phase_field_model/
// phase_field_parallel.cc (rank 0 reads, mesh.distribute(), the coupler on
// the distributed mesh, Dirichlet bottom / left, top increment, staggered
// solves; material_notch.dat); akantu-rs scenarios: the
// solid–phase-field coupler on distributed meshes against the serial run,
// shared by the local-world tests (`phase_field_parallel.rs`) and the
// real-MPI binary (`mpi_phase_field.rs`).

//! The solid–phase-field coupler on distributed meshes (MPI-044…047).
//!
//! A structured Tri3 square `[0, 1]²` (`n × n` cells, two triangles each),
//! built on rank 0 and distributed with the bisection partitioner; the
//! static coupler of `phase_field_parallel` (`phasefield` solid law,
//! `quadratic` phase field of `material_notch.dat`) with `y = 0` blocked in
//! y, `x = 0` blocked in x and the top face pulled in y by an increment per
//! load step, each load step followed by staggered solves. The same run on
//! the serial mesh is the reference, per **global node** on rank 0.

#![allow(dead_code)]

use akantu_comm::{Communicator, Synchronizer};
use akantu_core::enums::{AnalysisMethod, SparseSolverType, SynchronizationTag};
use akantu_core::{ElementType, GhostType, Idx, Real, Result};
use akantu_couplers::CouplerSolidPhaseField;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::synchronizers::NodeEntities;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;

pub type Outcome = std::result::Result<(), String>;

pub fn ok<T>(r: Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// `material_notch.dat` of the upstream example.
pub const DAT: &str = "model solid_mechanics_model [\n  material phasefield [\n    name = plate\n    \
    rho = 1.\n    E = 210.0\n    nu = 0.3\n    eta = 0\n    Plane_Stress = false\n  ]\n]\n\
    model phase_field_model [\n  phasefield quadratic [\n    name = plate\n    l0 = 0.2\n    \
    gc = 2.7e-3\n    E = 210.0\n    nu = 0.3\n  ]\n]\n";

/// Structured Tri3 square `[0, 1]²`: `(nodes, connectivity)`.
pub fn square_tri3(n: usize) -> (Vec<[Real; 2]>, Vec<[Idx; 3]>) {
    let h = 1.0 / n as Real;
    let id = |i: usize, j: usize| (j * (n + 1) + i) as Idx;
    let mut nodes = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            nodes.push([i as Real * h, j as Real * h]);
        }
    }
    let mut tris = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, d) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
            tris.push([a, b, c]);
            tris.push([a, c, d]);
        }
    }
    (nodes, tris)
}

/// The square on `comm` (full on rank 0, empty elsewhere).
pub fn square_mesh(comm: Communicator, n: usize, full: bool) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(2, "square", comm), "mesh")?;
    if full {
        let (nodes, tris) = square_tri3(n);
        let mut a = MeshAccessor::new(&mut m);
        for x in &nodes {
            ok(a.push_node(x), "node")?;
        }
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for t in &tris {
            c.push_tuple(t);
        }
    }
    ok(m.compute_bounding_box(), "bbox")?;
    Ok(m)
}

/// Distributed square (`comm.size()` parts, bisection partitioner).
pub fn distributed_square(comm: &Communicator, n: usize) -> std::result::Result<Mesh, String> {
    let mut m = square_mesh(comm.clone(), n, comm.rank() == 0)?;
    let mut p = RcbPartitioner;
    let _ = ok(
        distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0),
        "distribute",
    )?;
    Ok(m)
}

/// Result of a run, per global node on rank 0.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub u: Vec<Real>,
    pub d: Vec<Real>,
    pub dissipated: Real,
    pub potential: Real,
}

fn gather(
    mesh: &Mesh,
    a: &[Real],
    nbc: usize,
    parallel: bool,
) -> std::result::Result<Option<Vec<Real>>, String> {
    if parallel {
        let ns: &Synchronizer<Idx> = mesh.synchronizers().node.as_ref().ok_or("no node sync")?;
        let e = NodeEntities { mesh };
        ok(ns.gather(&e, a, nbc), "gather")
    } else if mesh.communicator().rank() == 0 {
        let mut g = vec![0.0; a.len()];
        for n in 0..mesh.nb_nodes() {
            let gid = usize::try_from(mesh.node_global_id(n as Idx)).unwrap_or(0);
            g[nbc * gid..nbc * gid + nbc].copy_from_slice(&a[nbc * n..nbc * n + nbc]);
        }
        Ok(Some(g))
    } else {
        Ok(None)
    }
}

/// The coupler on `mesh`, initialised (both models static, faer), with the
/// boundary conditions of the example.
pub fn coupler(mesh: Mesh) -> std::result::Result<(CouplerSolidPhaseField, SharedMesh), String> {
    let parser = ok(Parser::parse_str(DAT, "material_notch.dat"), "parser")?;
    let shared = SharedMesh::new(mesh);
    let mut c = ok(
        CouplerSolidPhaseField::with_parser(shared.clone(), None, "coupler", parser),
        "coupler",
    )?;
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    ok(c.solid.init_full(&options), "solid init_full")?;
    ok(c.phase.init_full(&options), "phase init_full")?;
    let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
    let nb = coords.len() / 2;
    {
        let mut b = ok(c.solid.blocked_dofs_mut(), "blocked")?;
        let b = b.as_mut_slice();
        for n in 0..nb {
            let (x, y) = (coords[2 * n], coords[2 * n + 1]);
            if y == 0.0 || y == 1.0 {
                b[2 * n + 1] = true;
            }
            if x == 0.0 {
                b[2 * n] = true;
            }
        }
    }
    Ok((c, shared))
}

/// `steps` load steps of `increment` on the top face, each followed by
/// `staggered` coupler solves; gathered on rank 0.
pub fn run(
    mesh: Mesh,
    steps: usize,
    staggered: usize,
    increment: Real,
) -> std::result::Result<Option<Snapshot>, String> {
    let comm = mesh.communicator().clone();
    let parallel = mesh.synchronizers().node.is_some() && comm.size() > 1;
    let (mut c, shared) = coupler(mesh)?;
    let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
    let nb = coords.len() / 2;
    for s in 0..steps {
        {
            // upstream BC::Dirichlet::IncrementValue(increment, _y) on "top"
            let mut u = ok(c.solid.displacement_mut(), "u")?;
            let u = u.as_mut_slice();
            for n in 0..nb {
                if coords[2 * n + 1] == 1.0 {
                    u[2 * n + 1] += increment;
                }
            }
        }
        for k in 0..staggered {
            ok(c.solve(None, None), &format!("solve {s}.{k}"))?;
        }
    }
    let dissipated = ok(c.phase.energy("dissipated"), "dissipated")?;
    let potential = ok(c.solid.energy("potential"), "potential")?;
    let u = ok(c.solid.displacement(), "u")?.as_slice().to_vec();
    let d = ok(c.phase.damage(), "d")?.as_slice().to_vec();
    let m = ok(shared.read(), "read")?;
    let (gu, gd) = (gather(&m, &u, 2, parallel)?, gather(&m, &d, 1, parallel)?);
    Ok(match (gu, gd) {
        (Some(u), Some(d)) => Some(Snapshot {
            u,
            d,
            dissipated,
            potential,
        }),
        _ => None,
    })
}

fn max_abs_diff(a: &[Real], b: &[Real]) -> (Real, Real, usize) {
    let mut d: Real = 0.0;
    let mut mag: Real = 0.0;
    let mut eq = 0;
    for (x, y) in a.iter().zip(b) {
        let e = (x - y).abs();
        d = if e.is_nan() || e > d { e } else { d };
        mag = mag.max(y.abs());
        if x.to_bits() == y.to_bits() {
            eq += 1;
        }
    }
    if a.len() != b.len() {
        d = Real::INFINITY;
    }
    (d, mag, eq)
}

/// The distributed coupler against the serial one: per global node,
/// displacement and damage within `rel_tol` of the largest value; the
/// energies (rank-ordered reductions) within `rel_tol`; the damage must
/// develop (`max d > 0.01`).
pub fn distributed_vs_serial(
    comm: Communicator,
    n: usize,
    steps: usize,
    staggered: usize,
    rel_tol: Real,
) -> Outcome {
    let rank = comm.rank();
    let size = comm.size();
    let m = distributed_square(&comm, n)?;
    let par = run(m, steps, staggered, 2e-3)?;
    if rank != 0 {
        return Ok(());
    }
    let par = par.ok_or("no gathered result")?;
    let ser = run(
        square_mesh(Communicator::serial(), n, true)?,
        steps,
        staggered,
        2e-3,
    )?
    .ok_or("no serial result")?;
    let (du, mu, eu) = max_abs_diff(&par.u, &ser.u);
    let (dd, md, ed) = max_abs_diff(&par.d, &ser.d);
    println!(
        "INFO phase-field coupler n={n} ranks={size}: |du|max={du:.3e} (|u|max={mu:.3e}, {eu}/{} \
         bitwise), |dd|max={dd:.3e} (|d|max={md:.3e}, {ed}/{} bitwise), Edis {:.17e} vs {:.17e}, \
         Epot {:.17e} vs {:.17e}",
        par.u.len(),
        par.d.len(),
        par.dissipated,
        ser.dissipated,
        par.potential,
        ser.potential
    );
    if md <= 0.01 {
        return Err(format!("no damage developed (max d = {md:e})"));
    }
    if du > rel_tol * mu || dd > rel_tol * md {
        return Err(format!(
            "coupler at {size} ranks differs from serial: du {du:e} (|u| {mu:e}), dd {dd:e} (|d| {md:e})"
        ));
    }
    let rel = |a: Real, b: Real| (a - b).abs() / b.abs().max(Real::MIN_POSITIVE);
    if rel(par.dissipated, ser.dissipated) > rel_tol || rel(par.potential, ser.potential) > rel_tol
    {
        return Err(format!(
            "energies differ: dissipated {} vs {}, potential {} vs {}",
            par.dissipated, ser.dissipated, par.potential, ser.potential
        ));
    }
    Ok(())
}

/// MPI-044 / MPI-045 / MPI-046 / MPI-047: the model and coupler tags on a
/// distributed mesh. After a staggered solve: `_pfm_damage` makes the ghost
/// elements' driving force, driving energy and damage those of their owner
/// (a second exchange changes nothing; overwriting the ghost values and
/// exchanging again restores them); `_for_dump` restores the nodal
/// damage of the pure-ghost nodes overwritten with garbage (and every other
/// node of a ghost element keeps its value: the senders' copies agree);
/// `synchronize_nodal(_pfm_damage)` restores the slaves'; `_csp_damage` /
/// `_csp_strain` run and change nothing; an unregistered tag is refused
/// (UD-553), a no-op with the compat switch.
pub fn model_synchronize_tags(comm: Communicator) -> Outcome {
    use akantu_core::NodeFlag;
    let m = distributed_square(&comm, 6)?;
    let (mut c, shared) = coupler(m)?;
    {
        let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
        let mut u = ok(c.solid.displacement_mut(), "u")?;
        let u = u.as_mut_slice();
        for n in 0..coords.len() / 2 {
            if coords[2 * n + 1] == 1.0 {
                u[2 * n + 1] += 4e-3;
            }
        }
    }
    ok(c.solve(None, None), "solve")?;
    // ghost driving data after the residual assemblies of the solve
    let law_ghost = |c: &CouplerSolidPhaseField| -> std::result::Result<Vec<Real>, String> {
        let b = ok(c.phase.phase_field(0), "law")?.base();
        let mut v = Vec::new();
        for f in [&b.driving_force, &b.driving_energy, &b.damage] {
            if let Ok(a) = f.get(ElementType::Triangle3, GhostType::Ghost) {
                v.extend_from_slice(a.as_slice());
            }
        }
        Ok(v)
    };
    // the coupler's last transfer wrote the laws' ghost `damage` from the
    // nodal damage (pure-ghost nodes included, upstream
    // computeDamageOnQuadPoints(_ghost)); the exchange gives the owners'
    // values, a second one changes nothing
    ok(
        c.phase.synchronize(SynchronizationTag::PfmDamage),
        "_pfm_damage",
    )?;
    ghosts_hold_owners_driving_data(&c, &shared)?;
    let before = law_ghost(&c)?;
    ok(
        c.phase.synchronize(SynchronizationTag::PfmDamage),
        "_pfm_damage",
    )?;
    let after = law_ghost(&c)?;
    if !akantu_validation::exact::exact_slices_eq(&before, &after) {
        return Err("a second _pfm_damage exchange changed the ghost driving data".into());
    }
    {
        let b = ok(c.phase.phase_field_mut(0), "law")?.base_mut();
        for f in [&mut b.driving_force, &mut b.driving_energy, &mut b.damage] {
            if let Ok(mut a) = f.get_mut(ElementType::Triangle3, GhostType::Ghost) {
                a.as_mut_slice().fill(Real::NAN);
            }
        }
    }
    ok(
        c.phase.synchronize(SynchronizationTag::PfmDamage),
        "_pfm_damage",
    )?;
    if !akantu_validation::exact::exact_slices_eq(&before, &law_ghost(&c)?) {
        return Err("_pfm_damage did not restore the ghost driving data".into());
    }
    // nodal damage: _for_dump over the element synchronizer, _pfm_damage
    // over the node synchronizer
    let (flags, ghost_nodes) = {
        let mesh = ok(shared.read(), "read")?;
        let flags: Vec<NodeFlag> = (0..mesh.nb_nodes())
            .map(|n| mesh.node_flag(n as Idx))
            .collect();
        let mut gn = std::collections::BTreeSet::new();
        for e in 0..mesh.nb_element(ElementType::Triangle3, GhostType::Ghost) {
            let conn = ok(
                mesh.element_connectivity(akantu_core::Element::new(
                    ElementType::Triangle3,
                    e as Idx,
                    GhostType::Ghost,
                )),
                "conn",
            )?;
            for &n in &conn {
                gn.insert(usize::try_from(n).unwrap_or(0));
            }
        }
        (flags, gn)
    };
    // a solve never writes pure-ghost DOFs (no global equation, in no node
    // scheme, as upstream): _for_dump gives them their owners' values and
    // leaves every other node of a ghost element as it is (the senders'
    // copies agree)
    let d0 = ok(c.phase.damage(), "d")?.as_slice().to_vec();
    ok(
        c.phase.synchronize(SynchronizationTag::ForDump),
        "_for_dump",
    )?;
    let d1 = ok(c.phase.damage(), "d")?.as_slice().to_vec();
    let mut refreshed = 0usize;
    for &n in &ghost_nodes {
        if flags[n].is_pure_ghost() {
            refreshed += usize::from(d1[n].to_bits() != d0[n].to_bits());
        } else if d1[n].to_bits() != d0[n].to_bits() {
            return Err(format!(
                "node {n} (flags {:#x}): _for_dump changed {} into {}",
                flags[n].bits(),
                d0[n],
                d1[n]
            ));
        }
    }
    let mut any = [u64::try_from(refreshed).unwrap_or(0)];
    ok(
        comm.all_reduce(&mut any, akantu_comm::ReduceOp::Sum),
        "all_reduce",
    )?;
    if comm.size() > 1 && any[0] == 0 {
        return Err("_for_dump refreshed no pure-ghost node".into());
    }
    {
        let mut d = ok(c.phase.damage_mut(), "d")?;
        let d = d.as_mut_slice();
        for &n in &ghost_nodes {
            if flags[n].is_pure_ghost() {
                d[n] = Real::NAN;
            }
        }
    }
    ok(
        c.phase.synchronize(SynchronizationTag::ForDump),
        "_for_dump",
    )?;
    if !akantu_validation::exact::exact_slices_eq(&d1, ok(c.phase.damage(), "d")?.as_slice()) {
        return Err("_for_dump did not restore the pure-ghost nodal damage".into());
    }
    let d0 = d1;
    {
        let mut d = ok(c.phase.damage_mut(), "d")?;
        let d = d.as_mut_slice();
        for (n, f) in flags.iter().enumerate() {
            if f.is_slave() {
                d[n] = Real::NAN;
            }
        }
    }
    ok(
        c.phase.synchronize_nodal(SynchronizationTag::PfmDamage),
        "synchronize_nodal _pfm_damage",
    )?;
    {
        let d = ok(c.phase.damage(), "d")?;
        for (n, f) in flags.iter().enumerate() {
            if f.is_slave() && d.as_slice()[n].to_bits() != d0[n].to_bits() {
                return Err(format!(
                    "slave {n}: nodal _pfm_damage gave {}",
                    d.as_slice()[n]
                ));
            }
        }
    }
    if c.phase
        .synchronize_nodal(SynchronizationTag::Diffusion)
        .is_ok()
        && shared
            .read()
            .map(|m| m.synchronizers().node.is_some())
            .unwrap_or(false)
    {
        return Err("synchronize_nodal(_diffusion) accepted".into());
    }
    // `_constitutive_law_id` after the initialisation
    // every ghost element already has its law, so the
    // exchange appends nothing (upstream `current != -1` guard of the
    // generic accessor)
    let law_filters = |c: &CouplerSolidPhaseField| -> std::result::Result<_, String> {
        let b = ok(c.phase.phase_field(0), "law")?.base();
        Ok((
            b.filter(ElementType::Triangle3, GhostType::NotGhost)
                .to_vec(),
            b.filter(ElementType::Triangle3, GhostType::Ghost).to_vec(),
            b.damage
                .get(ElementType::Triangle3, GhostType::Ghost)
                .map(|a| a.len())
                .unwrap_or(0),
        ))
    };
    let filters0 = law_filters(&c)?;
    ok(
        c.phase.synchronize(SynchronizationTag::ConstitutiveLawId),
        "_constitutive_law_id",
    )?;
    if law_filters(&c)? != filters0 {
        return Err("_constitutive_law_id after init changed the law filters".into());
    }
    // coupler tags: empty exchanges
    let u0 = ok(c.solid.displacement(), "u")?.as_slice().to_vec();
    ok(c.synchronize(SynchronizationTag::CspDamage), "_csp_damage")?;
    ok(c.synchronize(SynchronizationTag::CspStrain), "_csp_strain")?;
    if !akantu_validation::exact::exact_slices_eq(&u0, ok(c.solid.displacement(), "u")?.as_slice())
        || !akantu_validation::exact::exact_slices_eq(&before, &law_ghost(&c)?)
    {
        return Err("_csp_damage / _csp_strain changed the state".into());
    }
    // UD-553
    let has_sync = shared
        .read()
        .map(|m| m.synchronizers().element.is_some())
        .unwrap_or(false);
    for (what, r) in [
        (
            "phase _smm_stress",
            c.phase.synchronize(SynchronizationTag::SmmStress),
        ),
        (
            "coupler _pfm_damage",
            c.synchronize(SynchronizationTag::PfmDamage),
        ),
    ] {
        if has_sync && r.is_ok() {
            return Err(format!("{what}: an unregistered tag was accepted"));
        }
    }
    c.set_upstream_compat(true);
    ok(
        c.phase.synchronize(SynchronizationTag::SmmStress),
        "compat no-op",
    )?;
    ok(c.synchronize(SynchronizationTag::PfmDamage), "compat no-op")?;
    Ok(())
}

/// Independent check of `_pfm_damage`: every rank all-gathers
/// `(barycentre, driving force, driving energy, damage)` of the points of
/// its not-ghost elements (law 0), and every ghost element's values must be
/// bitwise its owner's (looked up by barycentre: the structured square's
/// barycentres are distinct).
fn ghosts_hold_owners_driving_data(c: &CouplerSolidPhaseField, shared: &SharedMesh) -> Outcome {
    let ty = ElementType::Triangle3;
    let mesh = ok(shared.read(), "read")?;
    let b = ok(c.phase.phase_field(0), "law")?.base();
    let per_elem = |g: GhostType, e_local: usize| -> std::result::Result<Vec<Real>, String> {
        let mut v = Vec::new();
        for f in [&b.driving_force, &b.driving_energy, &b.damage] {
            let a = ok(f.get(ty, g), "internal")?;
            let n = b.filter(ty, g).len();
            let per = a.as_slice().len().checked_div(n).unwrap_or(0);
            v.extend_from_slice(&a.as_slice()[e_local * per..(e_local + 1) * per]);
        }
        Ok(v)
    };
    let mut mine = Vec::new();
    let mut width = 0usize;
    for (l, &e) in b.filter(ty, GhostType::NotGhost).iter().enumerate() {
        let bc = ok(
            mesh.barycenter(akantu_core::Element::new(ty, e, GhostType::NotGhost)),
            "barycentre",
        )?;
        let vals = per_elem(GhostType::NotGhost, l)?;
        width = vals.len();
        mine.extend_from_slice(&bc[..2]);
        mine.extend_from_slice(&vals);
    }
    let comm = mesh.communicator().clone();
    let widths = ok(
        comm.all_gather(&[u64::try_from(width).unwrap_or(0)]),
        "widths",
    )?;
    let all = ok(comm.all_gather_v(&mine), "all_gather_v")?;
    let mut owners = std::collections::BTreeMap::new();
    for (r, chunk) in all.iter().enumerate() {
        let w = usize::try_from(widths[r]).unwrap_or(0);
        if w == 0 {
            continue;
        }
        for rec in chunk.chunks(2 + w) {
            owners.insert([rec[0].to_bits(), rec[1].to_bits()], rec[2..].to_vec());
        }
    }
    let mut checked = 0usize;
    for (l, &e) in b.filter(ty, GhostType::Ghost).iter().enumerate() {
        let bc = ok(
            mesh.barycenter(akantu_core::Element::new(ty, e, GhostType::Ghost)),
            "barycentre",
        )?;
        let want = owners
            .get(&[bc[0].to_bits(), bc[1].to_bits()])
            .ok_or_else(|| format!("ghost {e}: no owner"))?;
        let have = per_elem(GhostType::Ghost, l)?;
        if !akantu_validation::exact::exact_slices_eq(want, &have) {
            return Err(format!("ghost {e}: driving data are not the owner's"));
        }
        checked += 1;
    }
    if comm.size() > 1 && checked == 0 {
        return Err("no ghost element checked".into());
    }
    Ok(())
}

/// Check a collective failure caused by the last rank (see the solid
/// model's `par_laws`): every rank fails (the last with `own`, the others
/// with "failed on another rank"), `fatal` = agreed fatal, promptly; the
/// communicator stays usable (every rank reaches the final reduction).
fn check_agreed_failure(
    comm: &Communicator,
    r: Result<()>,
    own: &str,
    fatal: bool,
    elapsed: std::time::Duration,
) -> Outcome {
    let last = comm.size() as i32 - 1;
    let local = (|| {
        let e = match r {
            Ok(()) => return Err("the operation must fail on every rank".to_string()),
            Err(e) => e,
        };
        let msg = e.to_string();
        let is_fatal = matches!(e, akantu_core::Error::Fatal { .. });
        if is_fatal != fatal {
            return Err(format!("fatal = {fatal} expected, got: {msg}"));
        }
        if is_fatal && e.fatal_kind() != Some(akantu_core::FatalKind::Agreed) {
            return Err(format!("an agreed fatal error expected, got: {msg}"));
        }
        let want = if comm.rank() == last {
            own
        } else {
            "failed on another rank"
        };
        if !msg.contains(want) {
            return Err(format!("rank {}: '{want}' expected in: {msg}", comm.rank()));
        }
        if elapsed > std::time::Duration::from_secs(5) {
            return Err(format!("returned after {elapsed:?}"));
        }
        Ok(())
    })();
    let mut x = [1u64];
    ok(
        comm.all_reduce(&mut x, akantu_comm::ReduceOp::Sum),
        "all_reduce after the failure",
    )?;
    if x[0] != comm.size() as u64 {
        return Err(format!("all_reduce after the failure: {}", x[0]));
    }
    local
}

/// Selector failing on one rank only.
#[derive(Debug)]
struct FailOnRank(i32);

impl akantu_model::constitutive_law::ConstitutiveLawSelector for FailOnRank {
    fn select(
        &self,
        ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        _element: akantu_core::Element,
    ) -> Result<Option<usize>> {
        if ctx.mesh.communicator().rank() == self.0 {
            Err(akantu_core::Error::invalid(format!(
                "selector fails on rank {} only",
                self.0
            )))
        } else {
            Ok(Some(0))
        }
    }
}

fn phase_options() -> ModelOptions {
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    options
}

/// Failure injection: a phase-field selector failing on the
/// last rank only → `init_full` fails on every rank (agreed, recoverable,
/// nothing committed: no law holds an element), promptly; a working
/// selector then initialises the model (every local and ghost element
/// once).
pub fn pf_selector_failure_is_agreed(comm: Communicator) -> Outcome {
    let m = distributed_square(&comm, 4)?;
    let parser = ok(Parser::parse_str(DAT, "material_notch.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut c = ok(
        CouplerSolidPhaseField::with_parser(shared.clone(), None, "coupler", parser),
        "coupler",
    )?;
    c.phase
        .set_phase_field_selector(Box::new(FailOnRank(comm.size() as i32 - 1)));
    let t0 = std::time::Instant::now();
    let r = c.phase.init_full(&phase_options());
    let own = format!("selector fails on rank {} only", comm.size() - 1);
    check_agreed_failure(&comm, r, &own, false, t0.elapsed())?;
    let held = |c: &CouplerSolidPhaseField, g: GhostType| -> usize {
        (0..c.phase.nb_phase_fields())
            .map(|i| {
                c.phase
                    .phase_field(i)
                    .map(|l| l.base().filter(ElementType::Triangle3, g).len())
                    .unwrap_or(0)
            })
            .sum()
    };
    if GhostType::ALL.into_iter().any(|g| held(&c, g) > 0) {
        return Err("a failed selection must leave the law filters empty".into());
    }
    c.phase
        .set_phase_field_selector(Box::new(akantu_model::constitutive_law::DefaultSelector(0)));
    ok(
        c.phase.init_phase_fields(),
        "init_phase_fields after the failure",
    )?;
    let mesh = ok(shared.read(), "read")?;
    for g in GhostType::ALL {
        let n = mesh.nb_element(ElementType::Triangle3, g);
        let h = held(&c, g);
        if h != n {
            return Err(format!("{g:?}: laws hold {h} elements, mesh has {n}"));
        }
    }
    Ok(())
}

/// Failure injection: the last rank's input has an extra AT1
/// (`linear`) law with `irreversibility_type = penalization` and `l0 = 10`
/// on the unit square, refused by its `initPhaseField` (UD-385) → agreed
/// **fatal** on every rank (the laws hold their elements), promptly; and an
/// unknown parameter in the last rank's input (law instantiation fails
/// before the scope) → agreed fatal too.
pub fn pf_init_failures_are_agreed_fatal(comm: Communicator) -> Outcome {
    let last = comm.size() as i32 - 1;
    for (extra, own) in [
        (
            "  phasefield linear [\n    name = bad\n    l0 = 10\n    gc = 1\n    E = 1\n    \
             nu = 0.3\n    irreversibility_type = penalization\n  ]\n",
            "positive recovery penalty",
        ),
        (
            "  phasefield quadratic [\n    name = bad\n    l0 = 0.2\n    gc = 1\n    E = 1\n    \
             nu = 0.3\n    bogus_param = 3\n  ]\n",
            "bogus_param",
        ),
    ] {
        let m = distributed_square(&comm, 4)?;
        let dat = if comm.rank() == last {
            // inside the phase_field_model section (before its closing `]`)
            let i = DAT.rfind(']').ok_or("no section end")?;
            format!("{}{extra}{}", &DAT[..i], &DAT[i..])
        } else {
            DAT.to_string()
        };
        let parser = ok(Parser::parse_str(&dat, "material_notch.dat"), "parser")?;
        let mut c = ok(
            CouplerSolidPhaseField::with_parser(SharedMesh::new(m), None, "coupler", parser),
            "coupler",
        )?;
        let t0 = std::time::Instant::now();
        let r = c.phase.init_full(&phase_options());
        check_agreed_failure(&comm, r, own, true, t0.elapsed())?;
    }
    Ok(())
}

/// Failure injection: the `_pfm_damage` exchange fails on the
/// last rank only (a law-local index of one of its sent elements is out of
/// range: the packing fails before any message). Standalone
/// (`synchronize`): agreed recoverable on every rank, the ghost driving
/// data unchanged everywhere. Inside a staggered solve (the residual
/// assembly of the phase-field Newton step): the solve fails on every rank
/// and the nodal damage is unchanged on every rank (the failed step
/// restores the DOFs).
pub fn pfm_damage_failure_is_agreed(comm: Communicator) -> Outcome {
    if comm.size() < 2 {
        return Ok(());
    }
    let m = distributed_square(&comm, 6)?;
    let (mut c, shared) = coupler(m)?;
    {
        let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
        let mut u = ok(c.solid.displacement_mut(), "u")?;
        let u = u.as_mut_slice();
        for n in 0..coords.len() / 2 {
            if coords[2 * n + 1] == 1.0 {
                u[2 * n + 1] += 4e-3;
            }
        }
    }
    ok(c.solve(None, None), "solve")?;
    ok(
        c.phase.synchronize(SynchronizationTag::PfmDamage),
        "_pfm_damage",
    )?;
    let ghost_data = |c: &CouplerSolidPhaseField| -> std::result::Result<Vec<Real>, String> {
        let b = ok(c.phase.phase_field(0), "law")?.base();
        let mut v = Vec::new();
        for f in [&b.driving_force, &b.driving_energy, &b.damage] {
            if let Ok(a) = f.get(ElementType::Triangle3, GhostType::Ghost) {
                v.extend_from_slice(a.as_slice());
            }
        }
        Ok(v)
    };
    let before = ghost_data(&c)?;
    let d0 = ok(c.phase.damage(), "d")?.as_slice().to_vec();
    let last = comm.size() as i32 - 1;
    if comm.rank() == last {
        let mesh = ok(shared.read(), "read")?;
        let es = mesh
            .synchronizers()
            .element
            .as_ref()
            .ok_or("no element sync")?;
        let e = es
            .communications
            .send
            .values()
            .flatten()
            .next()
            .copied()
            .ok_or("the last rank sends no element")?;
        drop(mesh);
        ok(
            c.phase
                .physics
                .phasefields
                .local_numbering_map_mut()
                .element_mut(e),
            "local numbering",
        )?[0] = 1_000_000;
    }
    let t0 = std::time::Instant::now();
    let r = c.phase.synchronize(SynchronizationTag::PfmDamage);
    check_agreed_failure(&comm, r, "has no values", false, t0.elapsed())?;
    if !akantu_validation::exact::exact_slices_eq(&before, &ghost_data(&c)?) {
        return Err("the failed _pfm_damage exchange changed the ghost driving data".into());
    }
    {
        let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
        let mut u = ok(c.solid.displacement_mut(), "u")?;
        let u = u.as_mut_slice();
        for n in 0..coords.len() / 2 {
            if coords[2 * n + 1] == 1.0 {
                u[2 * n + 1] += 4e-3;
            }
        }
    }
    let t0 = std::time::Instant::now();
    let r = c.solve(None, None);
    check_agreed_failure(&comm, r, "has no values", false, t0.elapsed())?;
    if !akantu_validation::exact::exact_slices_eq(&d0, ok(c.phase.damage(), "d")?.as_slice()) {
        return Err("the failed solve changed the nodal damage".into());
    }
    Ok(())
}

/// The reduced `phase_field_parallel` of the oracle probe (v5,
/// `fixtures/parallel/phase_field_square_n8`): the coupler on the Tri3
/// square n = 8 with the `_eigen` solver and upstream's `_auto`
/// non-linear solver, `y = 0` / `y = 1` blocked in y, `x = 0` in x, 3 load
/// steps of 2e-3 on `y = 1`, 2 staggered solves each. One rank: bitwise.
/// At 2 and 4 ranks upstream needs MUMPS (`_eigen` is not distributed),
/// whose results vary between runs in the last bits: those rank counts are
/// compared with every recorded upstream result under the envelope rule of
/// D-082 ([`phase_field_square_matches_result_set`]).
///
/// Bitwise: the distribution, the law filters, the displacement, the nodal
/// damage, the phase field's damage / `phi` / driving force of the
/// not-ghost points and the energies (reduced and per-rank).
pub fn phase_field_square_matches_oracle(comm: Communicator) -> Outcome {
    let scenario = "phase_field_square_n8";
    let (np, rank) = (comm.size(), comm.rank());
    let fx =
        akantu_validation::Fixture::open_by_id(&format!("parallel/{scenario}/np{np}/rank{rank}"))
            .map_err(|e| e.to_string())?;
    let lf = |name: &str| {
        fx.load_f64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let bits = |have: &[Real], want: &[Real]| akantu_validation::exact::exact_slices_eq(want, have);
    let m = distributed_square(&comm, 8)?;
    super::par_common::compare_with_oracle(&m, &fx)?;
    let parser = ok(Parser::parse_str(DAT, "material_notch.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut c = ok(
        CouplerSolidPhaseField::with_parser(shared.clone(), None, "coupler", parser),
        "coupler",
    )?;
    let options = ModelOptions::new(AnalysisMethod::Static).with_solver_options(
        akantu_solver::ModelSolverOptions {
            non_linear_solver_type: Some(akantu_core::enums::NonLinearSolverType::Auto),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..akantu_solver::ModelSolverOptions::default()
        },
    );
    ok(c.solid.init_full(&options), "solid init_full")?;
    ok(c.phase.init_full(&options), "phase init_full")?;
    for (who, filter) in [
        (
            "solid_law0",
            ok(c.solid.physics.materials.law(0), "law")?
                .base()
                .element_filter
                .get(ElementType::Triangle3, GhostType::NotGhost)
                .map(|a| a.as_slice().to_vec())
                .unwrap_or_default(),
        ),
        (
            "phase_law0",
            ok(c.phase.phase_field(0), "law")?
                .base()
                .filter(ElementType::Triangle3, GhostType::NotGhost)
                .to_vec(),
        ),
    ] {
        let have: Vec<i64> = filter.iter().map(|&v| i64::from(v)).collect();
        if have != li(&format!("{who}/filter/_triangle_3/not_ghost"))? {
            return Err(format!("{who}: not-ghost filter differs from the oracle"));
        }
    }
    let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
    let nb = coords.len() / 2;
    {
        let mut b = ok(c.solid.blocked_dofs_mut(), "blocked")?;
        let b = b.as_mut_slice();
        for n in 0..nb {
            if coords[2 * n + 1] == 0.0 || coords[2 * n + 1] == 1.0 {
                b[2 * n + 1] = true;
            }
            if coords[2 * n] == 0.0 {
                b[2 * n] = true;
            }
        }
    }
    let (mut edis, mut edis_local, mut epot, mut epot_local) = (vec![], vec![], vec![], vec![]);
    for s in 0..3 {
        {
            let mut u = ok(c.solid.displacement_mut(), "u")?;
            let u = u.as_mut_slice();
            for n in 0..nb {
                if coords[2 * n + 1] == 1.0 {
                    u[2 * n + 1] += 2e-3;
                }
            }
        }
        for k in 0..2 {
            ok(c.solve(None, None), &format!("solve {s}.{k}"))?;
        }
        edis.push(ok(c.phase.energy("dissipated"), "dissipated")?);
        epot.push(ok(c.solid.energy("potential"), "potential")?);
        // per-rank contributions: the sums over the laws on this rank (one
        // rank: the reduced values)
        edis_local.push(*edis.last().unwrap_or(&0.0));
        epot_local.push(*epot.last().unwrap_or(&0.0));
    }
    for (name, have) in [
        ("energy/dissipated", &edis),
        ("energy/potential", &epot),
        ("energy/dissipated_local", &edis_local),
        ("energy/potential_local", &epot_local),
    ] {
        if !bits(have, &lf(name)?) {
            return Err(format!("{name} differs from the oracle: {have:?}"));
        }
    }
    if !bits(
        ok(c.solid.displacement(), "u")?.as_slice(),
        &lf("solid/displacement")?,
    ) {
        return Err("displacement differs from the oracle".into());
    }
    if !bits(ok(c.phase.damage(), "d")?.as_slice(), &lf("phase/damage")?) {
        return Err("nodal damage differs from the oracle".into());
    }
    let b = ok(c.phase.phase_field(0), "law")?.base();
    for (name, f) in [
        ("damage", &b.damage),
        ("phi", &b.phi),
        ("driving_force", &b.driving_force),
    ] {
        let have = f
            .get(ElementType::Triangle3, GhostType::NotGhost)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default();
        if !bits(
            &have,
            &lf(&format!("phase_law0/{name}/_triangle_3/not_ghost"))?,
        ) {
            return Err(format!("phase law {name} differs from the oracle"));
        }
    }
    Ok(())
}

/// Envelope of decision D-082: `1e-14` relative to the
/// largest reference value, a regression guard (measured margins:
/// NUMERICAL_VALIDATION).
pub const ENVELOPE: Real = 1e-14;

/// Directories `r1`, `r2`, … of the recorded distinct upstream results of
/// `phase_field_square_n8` at `np` ranks (D-082 result set).
pub fn result_set(np: usize) -> Vec<String> {
    let root = akantu_validation::fixtures_root()
        .join("parallel")
        .join("phase_field_square_n8")
        .join(format!("np{np}"));
    let mut out: Vec<(usize, String)> = std::fs::read_dir(&root)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_str()?.to_owned();
                    let k = name.strip_prefix('r')?.parse::<usize>().ok()?;
                    Some((k, name))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_unstable();
    out.into_iter().map(|(_, n)| n).collect()
}

/// What one run of the reduced `phase_field_parallel` holds on this rank.
struct LocalRun {
    u: Vec<Real>,
    d: Vec<Real>,
    law: Vec<(&'static str, Vec<Real>)>,
    edis: Vec<Real>,
    epot: Vec<Real>,
    epot_local: Vec<Real>,
}

/// The probe's scenario on the distributed mesh `m` with `solver`.
fn square_run(m: Mesh, solver: SparseSolverType) -> std::result::Result<LocalRun, String> {
    let parser = ok(Parser::parse_str(DAT, "material_notch.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut c = ok(
        CouplerSolidPhaseField::with_parser(shared.clone(), None, "coupler", parser),
        "coupler",
    )?;
    let options = ModelOptions::new(AnalysisMethod::Static).with_solver_options(
        akantu_solver::ModelSolverOptions {
            non_linear_solver_type: Some(akantu_core::enums::NonLinearSolverType::Auto),
            sparse_solver_type: Some(solver),
            ..akantu_solver::ModelSolverOptions::default()
        },
    );
    ok(c.solid.init_full(&options), "solid init_full")?;
    ok(c.phase.init_full(&options), "phase init_full")?;
    let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
    let nb = coords.len() / 2;
    {
        let mut b = ok(c.solid.blocked_dofs_mut(), "blocked")?;
        let b = b.as_mut_slice();
        for n in 0..nb {
            if coords[2 * n + 1] == 0.0 || coords[2 * n + 1] == 1.0 {
                b[2 * n + 1] = true;
            }
            if coords[2 * n] == 0.0 {
                b[2 * n] = true;
            }
        }
    }
    let (mut edis, mut epot, mut epot_local) = (vec![], vec![], vec![]);
    for s in 0..3 {
        {
            let mut u = ok(c.solid.displacement_mut(), "u")?;
            let u = u.as_mut_slice();
            for n in 0..nb {
                if coords[2 * n + 1] == 1.0 {
                    u[2 * n + 1] += 2e-3;
                }
            }
        }
        for k in 0..2 {
            ok(c.solve(None, None), &format!("solve {s}.{k}"))?;
        }
        edis.push(ok(c.phase.energy("dissipated"), "dissipated")?);
        epot.push(ok(c.solid.energy("potential"), "potential")?);
        epot_local.push(ok(c.solid.local_energy("potential"), "local potential")?);
    }
    let b = ok(c.phase.phase_field(0), "law")?.base();
    let law = [
        ("damage", &b.damage),
        ("phi", &b.phi),
        ("driving_force", &b.driving_force),
    ]
    .into_iter()
    .map(|(name, f)| {
        (
            name,
            f.get(ElementType::Triangle3, GhostType::NotGhost)
                .map(|a| a.as_slice().to_vec())
                .unwrap_or_default(),
        )
    })
    .collect();
    Ok(LocalRun {
        u: ok(c.solid.displacement(), "u")?.as_slice().to_vec(),
        d: ok(c.phase.damage(), "d")?.as_slice().to_vec(),
        law,
        edis,
        epot,
        epot_local,
    })
}

/// `(max |have − want|, max |want|)` of one rank's arrays; a length
/// mismatch or a NaN difference gives an infinite difference.
fn diff_and_scale(have: &[Real], want: &[Real]) -> (Real, Real) {
    let (d, mag, _) = max_abs_diff(have, want);
    (if d.is_nan() { Real::INFINITY } else { d }, mag)
}

/// D-082: the reduced `phase_field_parallel` of the
/// oracle probe at `comm.size()` = 2 or 4 ranks, on the oracle's Scotch
/// partition, with the port's centralised Eigen (bitwise SparseLU
/// translation) and faer solvers, against **every** distinct upstream
/// result of the variation study recorded in
/// `fixtures/parallel/phase_field_square_n8/np{P}/r{k}` (upstream's MUMPS
/// distributed factorisation makes its result run-dependent; the port does
/// not run it, so no run of the port can equal them bit for bit). Exact:
/// the distribution and the law filters. Envelope [`ENVELOPE`] (relative to
/// the largest reference value over all ranks): the displacement, the nodal
/// damage, the phase field's damage / `phi` / driving force of the
/// not-ghost points (the driving force relative to `2 max |φ|`, the size of
/// the terms that cancel in it), and per step the reduced dissipated and
/// potential energies and this rank's potential-energy contribution. Prints the
/// measured relative differences (the margins of
/// `docs/NUMERICAL_VALIDATION.md`). The result set must come from ≥ 12 runs.
pub fn phase_field_square_matches_result_set(comm: Communicator) -> Outcome {
    let scenario = "phase_field_square_n8";
    let (np, rank) = (comm.size(), comm.rank());
    let results = result_set(np);
    if results.is_empty() {
        return Err(format!("no recorded upstream result at {np} ranks"));
    }
    let open = |r: &str, k: i32| {
        akantu_validation::Fixture::open_by_id(&format!("parallel/{scenario}/np{np}/{r}/rank{k}"))
            .map_err(|e| e.to_string())
    };
    // the recorded set: every case states the whole set, ≥ 12 runs (D-082)
    let mut freq = Vec::new();
    for (k, r) in results.iter().enumerate() {
        let fx = open(r, rank)?;
        let set = &fx.manifest().parameters["result_set"];
        let runs = set["runs"].as_u64().unwrap_or(0);
        if runs < 12 || set["distinct_results"].as_u64() != Some(results.len() as u64) {
            return Err(format!(
                "{r}: result set {set} (≥ 12 runs, {} results)",
                results.len()
            ));
        }
        if set["this_result"].as_u64() != Some(k as u64 + 1) {
            return Err(format!("{r}: this_result {}", set["this_result"]));
        }
        freq.push(set["frequency"].as_u64().unwrap_or(0));
        if freq.iter().sum::<u64>() > runs {
            return Err(format!("{r}: frequencies exceed {runs} runs"));
        }
    }
    let parts = if rank == 0 {
        let fx = open(&results[0], 0)?;
        let lin = fx
            .load_i64("partition/linearized")
            .map_err(|e| e.to_string())?;
        lin.data.iter().map(|&v| v as Idx).collect()
    } else {
        Vec::new()
    };
    let mut worst: std::collections::BTreeMap<String, Real> = std::collections::BTreeMap::new();
    for solver in [SparseSolverType::Eigen, SparseSolverType::Faer] {
        let mut m = square_mesh(comm.clone(), 8, rank == 0)?;
        super::par_common::distribute_prescribed(&mut m, parts.clone())?;
        for r in &results {
            super::par_common::compare_with_oracle(&m, &open(r, rank)?)?;
        }
        let have = square_run(m, solver)?;
        for r in &results {
            let fx = open(r, rank)?;
            let lf = |name: &str| {
                fx.load_f64(name)
                    .map(|f| f.data)
                    .map_err(|e| format!("{r} {name}: {e}"))
            };
            let mut pairs: Vec<(String, &[Real], Vec<Real>)> = vec![
                ("u".into(), &have.u, lf("solid/displacement")?),
                ("d".into(), &have.d, lf("phase/damage")?),
            ];
            for (name, v) in &have.law {
                pairs.push((
                    format!("law {name}"),
                    v,
                    lf(&format!("phase_law0/{name}/_triangle_3/not_ghost"))?,
                ));
            }
            pairs.push(("E_dis".into(), &have.edis, lf("energy/dissipated")?));
            pairs.push(("E_pot".into(), &have.epot, lf("energy/potential")?));
            pairs.push((
                "E_pot rank".into(),
                &have.epot_local,
                lf("energy/potential_local")?,
            ));
            // scale of the driving force: the terms that cancel in it
            // (upstream `−2φ + d (2φ + g_c/l0)`, ≈ 1e-17 at convergence)
            let phi_scale = 2.0
                * lf("phase_law0/phi/_triangle_3/not_ghost")?
                    .iter()
                    .fold(0.0 as Real, |m, x| m.max(x.abs()));
            for (name, h, w) in &pairs {
                // global scale and difference: max over the ranks
                let (d, mag) = diff_and_scale(h, w);
                let mag = if name == "law driving_force" {
                    phi_scale
                } else {
                    mag
                };
                let mut v = [d, mag];
                ok(comm.all_reduce(&mut v, akantu_comm::ReduceOp::Max), "max")?;
                let rel = if v[1] > 0.0 { v[0] / v[1] } else { v[0] };
                let key = format!("{name} ({solver:?})");
                let e = worst.entry(key).or_insert(0.0);
                *e = e.max(rel);
                if rel.is_nan() || rel > ENVELOPE {
                    return Err(format!(
                        "{r} ({solver:?}, {np} ranks): {name} differs by {:e} (scale {:e}, \
                         relative {rel:e} > {ENVELOPE:e})",
                        v[0], v[1]
                    ));
                }
            }
        }
    }
    if rank == 0 {
        let line: Vec<String> = worst.iter().map(|(k, v)| format!("{k} {v:.2e}")).collect();
        println!(
            "INFO phase_field_square_n8 np{np} vs {} upstream result(s) (frequencies {freq:?}): \
             worst relative {}",
            results.len(),
            line.join(", ")
        );
    }
    Ok(())
}

/// UD-905: `CouplerSolidPhaseField::check_convergence`
/// is one collective answer on every rank. Arrays of zeros, then:
///
/// * **A** — one displacement difference of 1e-6 at a node only the last
///   rank holds (a purely local node): a per-rank evaluation would say "not
///   converged" on the last rank and "converged" on the others (checked
///   here with upstream's local expression); the default answer is "not
///   converged" on every rank;
/// * **B** — a damage difference of 0.8e-8 at one purely local node of
///   every rank: every rank's own norm is below 1e-8 but the global one,
///   `0.8e-8·√P`, is not (from 2 ranks): "not converged" on every rank;
/// * **C** — all differences zero: "converged" on every rank;
/// * **D** — a damage difference of 0.8e-8 at one node shared by several
///   ranks, on every copy (master, slaves, pure ghosts, as a solve leaves
///   them): counted once, "converged" on every rank (counting the copies
///   would give `0.8e-8·√k > 1e-8`);
/// * the serial coupler on the same global arrays gives the same answers;
/// * with `per_rank_convergence_check` (upstream's per-rank evaluation):
///   A is an agreed error on every rank (the ranks' answers differ), B,
///   C and D "converged" on every rank (upstream's local answers agree);
/// * a damage array of the wrong length on the last rank only: an agreed
///   recoverable failure on every rank, promptly, the communicator usable;
/// * `upstream_convergence_formula` without
///   `per_rank_convergence_check` refused on every rank of a distributed
///   mesh (accepted on a mesh without a node synchronizer); both switches
///   together accepted.
pub fn check_convergence_is_collective(comm: Communicator) -> Outcome {
    use akantu_core::Array;
    let (size, rank) = (comm.size(), comm.rank());
    let last = size as i32 - 1;
    // 12 x 12 cells: every rank holds purely local nodes up to 8 ranks
    let n_cells = 12;
    let m = distributed_square(&comm, n_cells)?;
    let (mut c, shared) = coupler(m)?;
    let (nb, flags, gids, blocked) = {
        let mesh = ok(shared.read(), "read")?;
        let nb = mesh.nb_nodes();
        let flags: Vec<akantu_core::NodeFlag> = (0..nb).map(|n| mesh.node_flag(n as Idx)).collect();
        let gids: Vec<i64> = (0..nb).map(|n| mesh.node_global_id(n as Idx)).collect();
        drop(mesh);
        let blocked = ok(c.solid.blocked_dofs(), "blocked")?.as_slice().to_vec();
        (nb, flags, gids, blocked)
    };
    // a purely local node (on this rank only), free in x; the scenario
    // needs one on every rank (agreed below, before any other collective:
    // a rank returning alone would leave the others waiting)
    let found = (0..nb).find(|&n| flags[n].is_local() && !blocked[2 * n]);
    let count = |b: bool| -> std::result::Result<u64, String> {
        let mut v = [u64::from(b)];
        ok(comm.all_reduce(&mut v, akantu_comm::ReduceOp::Sum), "count")?;
        Ok(v[0])
    };
    let with_node = count(found.is_some())?;
    let Some(local_node) = found.filter(|_| with_node == size as u64) else {
        return Err(format!(
            "{with_node} of {size} ranks hold a purely local free node (scenario invalid)"
        ));
    };
    let arr = |n: usize, nbc: usize, set: &[(usize, Real)]| {
        let mut a = Array::<Real>::zeros(n, nbc);
        for &(i, v) in set {
            a.as_mut_slice()[i] = v;
        }
        a
    };
    let zeros_u = arr(nb, 2, &[]);
    let zeros_d = arr(nb, 1, &[]);
    let u_a = if rank == last {
        arr(nb, 2, &[(2 * local_node, 1e-6)])
    } else {
        arr(nb, 2, &[])
    };
    let d_b = arr(nb, 1, &[(local_node, 0.8e-8)]);
    // upstream's per-rank expression (local arrays, no reduction)
    let local_answer = |un: &Array<Real>, dn: &Array<Real>| -> bool {
        let mut nu: Real = 0.0;
        for (i, x) in un.as_slice().iter().enumerate() {
            if !blocked[i] {
                nu += x * x;
            }
        }
        let nd: Real = dn.as_slice().iter().map(|x| x * x).sum();
        nu.sqrt().max(nd.sqrt()) < 1e-8
    };
    // D: the smallest global id of a node shared by several ranks
    let masters: Vec<i64> = (0..nb)
        .filter(|&n| flags[n].is_master())
        .map(|n| gids[n])
        .collect();
    let shared_gid = ok(comm.all_gather_v(&masters), "masters")?
        .into_iter()
        .flatten()
        .min();
    let shared_local = shared_gid.and_then(|g| gids.iter().position(|&x| x == g));
    let copies = count(shared_local.is_some())?;
    if size > 1 && copies < 2 {
        return Err(format!("D: the shared node has {copies} copies"));
    }
    let d_d = match shared_local {
        Some(n) => arr(nb, 1, &[(n, 0.8e-8)]),
        None => arr(nb, 1, &[]),
    };
    // the premise: without the reduction the ranks disagree in A, and every
    // rank's own answer in B is "converged"
    let a_local = count(local_answer(&u_a, &zeros_d))?;
    if size > 1 && (a_local == 0 || a_local == size as u64) {
        return Err(format!("A: per-rank answers agree ({a_local} of {size})"));
    }
    if count(local_answer(&zeros_u, &d_b))? != size as u64 {
        return Err("B: a rank's own norm is not below the tolerance".into());
    }
    // default: one answer on every rank
    let mut answers = Vec::new();
    for (case, un, dn) in [
        ("A", &u_a, &zeros_d),
        ("B", &zeros_u, &d_b),
        ("C", &zeros_u, &zeros_d),
        ("D", &zeros_u, &d_d),
    ] {
        let r = ok(
            c.check_convergence(un, &zeros_u, dn, &zeros_d),
            &format!("check_convergence {case}"),
        )?;
        let n = count(r)?;
        if n != 0 && n != size as u64 {
            return Err(format!("{case}: {n} of {size} ranks converged"));
        }
        answers.push(r);
    }
    let want = [false, size == 1, true, true];
    if answers != want {
        return Err(format!("answers {answers:?}, expected {want:?}"));
    }
    // the serial coupler on the same global arrays (rank 0)
    let chosen = [gids[local_node]];
    let all = ok(comm.all_gather_v(&chosen[..]), "all_gather_v")?;
    // (its outcome is agreed: rank 0 must not leave the collectives below
    // alone)
    let serial_check = (|| -> Outcome {
        if rank != 0 {
            return Ok(());
        }
        let (mut s, sm) = coupler(square_mesh(Communicator::serial(), n_cells, true)?)?;
        let ns = ok(sm.read(), "read")?.nb_nodes();
        let g: Vec<usize> = all.iter().map(|v| v[0] as usize).collect();
        let su_a = arr(ns, 2, &[(2 * g[size - 1], 1e-6)]);
        let sd_b = arr(ns, 1, &g.iter().map(|&k| (k, 0.8e-8)).collect::<Vec<_>>());
        let (zu, zd) = (arr(ns, 2, &[]), arr(ns, 1, &[]));
        let serial = [
            ok(s.check_convergence(&su_a, &zu, &zd, &zd), "serial A")?,
            ok(s.check_convergence(&zu, &zu, &sd_b, &zd), "serial B")?,
            ok(s.check_convergence(&zu, &zu, &zd, &zd), "serial C")?,
            ok(
                s.check_convergence(
                    &zu,
                    &zu,
                    &arr(
                        ns,
                        1,
                        &shared_gid
                            .map(|k| (k as usize, 0.8e-8))
                            .into_iter()
                            .collect::<Vec<_>>(),
                    ),
                    &zd,
                ),
                "serial D",
            )?,
        ];
        if serial != want {
            return Err(format!("serial answers {serial:?}, expected {want:?}"));
        }
        Ok(())
    })();
    if count(serial_check.is_err())? > 0 {
        return serial_check.and(Err("the serial comparison failed on rank 0".into()));
    }
    // upstream's per-rank evaluation, agreed (UD-905 compat)
    c.compat.per_rank_convergence_check = true;
    let r = c.check_convergence(&u_a, &zeros_u, &zeros_d, &zeros_d);
    match (&r, size > 1) {
        (Err(e), true) if e.to_string().contains("UD-905") => {}
        (Ok(false), false) => {}
        _ => return Err(format!("compat A at {size} ranks: {r:?}")),
    }
    for (case, un, dn) in [
        ("B", &zeros_u, &d_b),
        ("C", &zeros_u, &zeros_d),
        ("D", &zeros_u, &d_d),
    ] {
        if !ok(
            c.check_convergence(un, &zeros_u, dn, &zeros_d),
            &format!("compat {case}"),
        )? {
            return Err(format!("compat {case}: not converged"));
        }
    }
    c.compat.per_rank_convergence_check = false;
    // a wrong length on the last rank only: agreed failure
    let short = arr(nb.saturating_sub(1), 1, &[]);
    let dn = if rank == last { &short } else { &zeros_d };
    let t0 = std::time::Instant::now();
    let r = c.check_convergence(&zeros_u, &zeros_u, dn, &zeros_d);
    let agreed = check_agreed_failure(
        &comm,
        r.map(|_| ()),
        "checkConvergence",
        false,
        t0.elapsed(),
    );
    if count(agreed.is_err())? > 0 {
        return agreed.and(Err(
            "the agreed failure was not observed on another rank".into()
        ));
    }
    // still usable: the next check succeeds on every rank
    if !ok(
        c.check_convergence(&zeros_u, &zeros_u, &zeros_d, &zeros_d),
        "check after the failure",
    )? {
        return Err("C after the failure: not converged".into());
    }
    // Upstream's formula (UD-620) without the per-rank
    // evaluation (UD-905) is refused on a distributed mesh, on every rank
    // with its own message (the refusal is local and the same everywhere),
    // and accepted on a mesh without a node synchronizer
    let distributed = ok(shared.read(), "read")?.synchronizers().node.is_some();
    c.compat.upstream_convergence_formula = true;
    let r = c.check_convergence(&zeros_u, &zeros_u, &zeros_d, &zeros_d);
    let hybrid = match (&r, distributed) {
        (Err(e), true)
            if e.to_string()
                .contains("has no meaning on a distributed mesh")
                && e.to_string().contains("per_rank_convergence_check") =>
        {
            Ok(())
        }
        (Ok(true), false) => Ok(()),
        _ => Err(format!(
            "agreed refusal at {size} ranks (distributed {distributed}): {r:?}"
        )),
    };
    if count(hybrid.is_err())? > 0 {
        return hybrid.and(Err(
            "agreed refusal: refused differently on another rank".into()
        ));
    }
    // both switches together: accepted, one answer on every rank
    c.compat.per_rank_convergence_check = true;
    let r = ok(
        c.check_convergence(&zeros_u, &zeros_u, &zeros_d, &zeros_d),
        "both upstream switches",
    )?;
    if count(r)? != size as u64 {
        return Err("both upstream switches, C: not converged on every rank".into());
    }
    c.compat.upstream_convergence_formula = false;
    c.compat.per_rank_convergence_check = false;
    Ok(())
}

/// The phase-field model refuses element
/// additions and removals on a distributed mesh before the mesh commit
/// (`check_mesh_event`: the new ghost elements' `_constitutive_law_id`
/// exchange is not ported); on a mesh without element synchronizer the same
/// event is accepted by the model's check.
pub fn pf_distributed_element_events_refused(comm: Communicator) -> Outcome {
    use akantu_mesh::{MeshEvent, MeshEventHandler};
    let m = distributed_square(&comm, 4)?;
    let (c, shared) = coupler(m)?;
    let mesh = ok(shared.read(), "read")?;
    let event = MeshEvent::ElementsAdded {
        elements: Vec::new(),
        constitutive_law: None,
        origin: "test".into(),
        extension: None,
    };
    let r = c.phase.check_mesh_event(&mesh, &event);
    if mesh.synchronizers().element.is_some() {
        match &r {
            Err(e)
                if e.to_string()
                    .contains("element additions or removals on a distributed") => {}
            _ => return Err(format!("distributed ElementsAdded: {r:?}")),
        }
    } else {
        ok(r, "ElementsAdded without element synchronizer")?;
    }
    drop(mesh);
    if comm.rank() == 0 {
        let (s, sm) = coupler(square_mesh(Communicator::serial(), 4, true)?)?;
        let mesh = ok(sm.read(), "read")?;
        ok(
            s.phase.check_mesh_event(&mesh, &event),
            "serial ElementsAdded",
        )?;
    }
    Ok(())
}

/// UD-004: after `compute_driving_forces`
/// (upstream `beforeSolveStep`) with the linear nodal damage
/// `d = 0.1 + 0.2 x + 0.3 y` on every local node (its gradient `(0.2, 0.3)`
/// is exact on Tri3): by default the not-ghost `grad_d` block keeps its
/// points and the ghost block holds the ghost gradients; with
/// `ghost_gradient_into_not_ghost` (upstream) the not-ghost block holds the
/// ghost points' gradients (resized to them) and the ghost block is not
/// written.
pub fn ud004_ghost_gradient_switch(comm: Communicator) -> Outcome {
    let ty = ElementType::Triangle3;
    // no early return inside the loop: the next iteration distributes a
    // mesh (collective)
    let mut failure: Option<String> = None;
    for upstream in [false, true] {
        let m = distributed_square(&comm, 4)?;
        let (mut c, shared) = coupler(m)?;
        c.phase
            .set_upstream_compat(akantu_phase_field::PhaseFieldUpstreamCompat {
                ghost_gradient_into_not_ghost: upstream,
                ..akantu_phase_field::PhaseFieldUpstreamCompat::default()
            });
        let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
        {
            let mut d = ok(c.phase.damage_mut(), "d")?;
            for (n, v) in d.as_mut_slice().iter_mut().enumerate() {
                *v = 0.1 + 0.2 * coords[2 * n] + 0.3 * coords[2 * n + 1];
            }
        }
        ok(c.phase.compute_driving_forces(), "compute_driving_forces")?;
        let b = ok(c.phase.phase_field(0), "law")?.base();
        let nq = |g: GhostType| -> usize {
            let n = b.filter(ty, g).len();
            b.damage
                .get(ty, g)
                .map(|a| a.len().checked_div(n.max(1)).unwrap_or(0) * n)
                .unwrap_or(0)
        };
        let block = |g: GhostType| -> Vec<Real> {
            b.grad_d
                .get(ty, g)
                .map(|a| a.as_slice().to_vec())
                .unwrap_or_default()
        };
        let is_gradient = |v: &[Real]| {
            v.chunks(2)
                .all(|p| (p[0] - 0.2).abs() < 1e-12 && (p[1] - 0.3).abs() < 1e-12)
        };
        let (ng, g) = (block(GhostType::NotGhost), block(GhostType::Ghost));
        let (nq_ng, nq_g) = (nq(GhostType::NotGhost), nq(GhostType::Ghost));
        if nq_g == 0 {
            continue; // no ghost element (one rank)
        }
        if nq_g == nq_ng {
            failure.get_or_insert_with(|| {
                "as many ghost as not-ghost points: the check cannot tell".into()
            });
            continue;
        }
        let ok_default =
            ng.len() == 2 * nq_ng && is_gradient(&ng) && g.len() == 2 * nq_g && is_gradient(&g);
        let ok_upstream = ng.len() == 2 * nq_g && is_gradient(&ng) && !is_gradient(&g);
        if (!upstream && !ok_default) || (upstream && !ok_upstream) {
            failure.get_or_insert(format!(
                "UD-004 upstream = {upstream}: not-ghost block {} values (points {nq_ng}), ghost \
                 block {} values (points {nq_g}), gradients {} / {}",
                ng.len(),
                g.len(),
                is_gradient(&ng),
                is_gradient(&g)
            ));
        }
    }
    failure.map_or(Ok(()), Err)
}
