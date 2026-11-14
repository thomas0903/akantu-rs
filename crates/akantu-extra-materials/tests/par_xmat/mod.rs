// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/{test_material_damage_iterative_non_local_parallel.cc,
// test_material_damage_iterative_non_local_parallel.sh, two_materials.dat,
// one_circular_inclusion.msh}; the extra non-local laws on distributed meshes.
// Shared by non_local_parallel.rs (local world) and mpi_xmat.rs
// (real MPI).

//! Scenarios of the extra non-local laws on distributed meshes (see
//! `non_local_parallel.rs`): [`test_187`] (TEST-187), [`laws_vs_serial`]
//! (the four laws against the serial run), [`xn11`].

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::time::Duration;

use super::common::{data, set_newton};
use akantu_comm::{Communicator, ReduceOp};
use akantu_core::enums::{AnalysisMethod, MeshIoType, NonLinearSolverType, SpatialDirection};
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_extra_materials::{
    ExtraLawOptions, as_iterative_damage_mut, register_extra_materials_with,
    register_extra_non_local_materials_with,
};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_model::constitutive_law::{ElementDataSelector, InternalFieldRef, LawPhase};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

/// Outcome of a scenario on one rank.
pub type Outcome = Result<(), String>;

fn ok<T>(r: akantu_core::Result<T>, what: &str) -> Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// `msh` read on rank 0, distributed with the bisection partitioner
/// (upstream: Scotch; one rank: not distributed).
fn mesh_on(comm: &Communicator, msh: &str) -> Result<Mesh, String> {
    let mut mesh = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        ok(
            akantu_io::read_mesh(&mut mesh, &data(msh), MeshIoType::Gmsh),
            "read",
        )?;
    }
    if comm.size() > 1 {
        let mut p = RcbPartitioner;
        let _events = ok(
            distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0),
            "distribute",
        )?;
    }
    Ok(mesh)
}

fn model_on(mesh: Mesh, dat: &str) -> Result<SolidMechanicsModel, String> {
    let parser = ok(Parser::parse_str(dat, "material.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(
            SharedMesh::new(mesh),
            None,
            "solid_mechanics_model",
            parser,
        ),
        "model",
    )?;
    ok(
        register_extra_materials_with(&mut model.physics.factory, ExtraLawOptions::default()),
        "register",
    )?;
    ok(
        register_extra_non_local_materials_with(
            &mut model.physics.factory,
            ExtraLawOptions::default(),
        ),
        "register non-local",
    )?;
    Ok(model)
}

/// Displacement of every owned node by global id.
fn owned_displacements(model: &SolidMechanicsModel) -> Result<BTreeMap<i64, [Real; 2]>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let u = ok(model.displacement(), "u")?;
    let d = mesh.is_distributed();
    let mut out = BTreeMap::new();
    for (k, uu) in u.as_slice().as_chunks::<2>().0.iter().enumerate() {
        let n = Idx::try_from(k).map_err(|e| e.to_string())?;
        if d && !mesh.node_flag(n).is_local_or_master() {
            continue;
        }
        let g = if d { mesh.node_global_id(n) } else { k as i64 };
        out.insert(g, [uu[0], uu[1]]);
    }
    Ok(out)
}

/// Upstream's program (module documentation): returns, per solve, the
/// owned displacements.
fn damage_iterative_program(comm: &Communicator) -> Result<Vec<BTreeMap<i64, [Real; 2]>>, String> {
    let dat = std::fs::read_to_string(data("test_material_damage/two_materials.dat"))
        .map_err(|e| e.to_string())?;
    let mesh = mesh_on(comm, "test_material_damage/one_circular_inclusion.msh")?;
    let mut model = model_on(mesh, &dat)?;
    model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    options.solver_options.non_linear_solver_type =
        Some(NonLinearSolverType::NewtonRaphsonModified);
    ok(model.init_full(&options), "init_full")?;
    set_newton(&mut model, 2, 1e-12, "solution");
    for (value, axis, group) in [
        (0., SpatialDirection::X, "left"),
        (0., SpatialDirection::Y, "bottom"),
        (2., SpatialDirection::Y, "top"),
    ] {
        ok(
            model.apply_dirichlet_bc(&FixedValue { value, axis }, group),
            "bc",
        )?;
    }
    let mut out = Vec::new();
    ok(model.solve_step(None), "Did not converge")?;
    out.push(owned_displacements(&model)?);
    // the law with the larger (global) maximum equivalent stress damages
    let maxima: Vec<Real> = (0..2)
        .map(|m| {
            model
                .with_material(m, LawPhase::AfterSolveStep, |law, _| {
                    Ok(as_iterative_damage_mut(law)
                        .map(|it| it.norm_max_equivalent_stress())
                        .unwrap_or(Real::NAN))
                })
                .map_err(|e| e.to_string())
        })
        .collect::<Result<_, _>>()?;
    let which = usize::from(maxima[0] <= maxima[1]);
    let damaged = model
        .with_material(which, LawPhase::AfterSolveStep, |law, ctx| {
            as_iterative_damage_mut(law)
                .ok_or_else(|| akantu_core::Error::invalid("not an iterative law"))?
                .update_damage(ctx)
        })
        .map_err(|e| e.to_string())?;
    if damaged == 0 {
        return Err("no element damaged".into());
    }
    ok(model.solve_step(None), "Did not converge")?;
    out.push(owned_displacements(&model)?);
    Ok(out)
}

/// TEST-187 at `np` ranks (upstream `checkDisplacement`).
pub fn test_187(comm: Communicator) -> Outcome {
    let serial = damage_iterative_program(&Communicator::serial())?;
    let dist = damage_iterative_program(&comm)?;
    for (step, (d, s)) in dist.iter().zip(&serial).enumerate() {
        let mut max_error: Real = 0.;
        for (g, u) in d {
            let us = s
                .get(g)
                .ok_or_else(|| format!("node {g} not in the serial run"))?;
            let e = ((u[0] - us[0]).powi(2) + (u[1] - us[1]).powi(2)).sqrt();
            if e.is_nan() {
                max_error = Real::INFINITY;
            } else {
                max_error = max_error.max(e);
            }
        }
        let mut v = [max_error];
        ok(comm.all_reduce(&mut v, ReduceOp::Max), "all_reduce")?;
        if comm.rank() == 0 {
            eprintln!(
                "INFO TEST-187 np{} step {}: max displacement error {:e}",
                comm.size(),
                step + 1,
                v[0]
            );
        }
        if v[0].is_nan() || v[0] > 1.0e-9 {
            return Err(format!("Displacement error of {} is too big!", v[0]));
        }
    }
    Ok(())
}

// ---------------------------------------------- the four laws distributed

const LAWS: [(&str, &str, AnalysisMethod); 4] = [
    (
        "damage_iterative_non_local",
        " E = 1\n nu = 0.2\n Sc = 10\n",
        AnalysisMethod::Static,
    ),
    (
        "brittle_non_local",
        " E = 1e9\n nu = 0.2\n",
        AnalysisMethod::ExplicitLumpedMass,
    ),
    (
        "vreepeerlings_non_local",
        " E = 1e9\n nu = 0.2\n Kapaoi = 0.1\n Kapac = 2\n Crate = 0.1\n",
        AnalysisMethod::ExplicitLumpedMass,
    ),
    (
        "damage_orthotropic_iterative_non_local",
        " E = 1\n nu = 0.2\n Sc = 10\n",
        AnalysisMethod::Static,
    ),
];

type PointId = (Vec<i64>, usize);

/// Every `Real` internal named `non local` / `non_local` of law 0, at the
/// owned points, by global identity.
fn averaged(model: &SolidMechanicsModel) -> Result<BTreeMap<(String, PointId), Vec<Real>>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let law = ok(model.material(0), "law")?;
    let filter = law.element_filter().clone();
    let mut out = BTreeMap::new();
    let mut err = None;
    law.for_each_internal(&mut |name, f| {
        if !(name.contains("non local") || name.contains("non_local")) {
            return;
        }
        let InternalFieldRef::Real(f) = f else {
            return;
        };
        for (ty, g, a) in f.values().as_map().iter_all() {
            if g != GhostType::NotGhost {
                continue;
            }
            let (Ok(a), Ok(fl)) = (a.read(), filter.get(ty, g)) else {
                continue;
            };
            if fl.is_empty() {
                continue;
            }
            let nc = a.nb_component();
            let nq = a.len() / fl.len();
            for (k, &el) in fl.as_slice().iter().enumerate() {
                let e = akantu_core::Element::new(ty, el, g);
                let mut c: Vec<i64> = match mesh.element_connectivity(e) {
                    Ok(c) => c
                        .iter()
                        .map(|&n| {
                            if mesh.is_distributed() {
                                mesh.node_global_id(n)
                            } else {
                                i64::from(n)
                            }
                        })
                        .collect(),
                    Err(e) => {
                        err.get_or_insert(e.to_string());
                        return;
                    }
                };
                c.sort_unstable();
                for q in 0..nq {
                    let off = (k * nq + q) * nc;
                    out.insert(
                        (name.to_owned(), (c.clone(), q)),
                        a.as_slice()[off..off + nc].to_vec(),
                    );
                }
            }
        }
    });
    if let Some(e) = err {
        return Err(e);
    }
    Ok(out)
}

fn law_run(
    comm: &Communicator,
    kw: &str,
    params: &str,
    method: AnalysisMethod,
) -> Result<BTreeMap<(String, PointId), Vec<Real>>, String> {
    let dat = format!(
        "material {kw} [\n name = m\n rho = 1\n{params}]\n\
         non_local m base_wf [\n radius = 0.5\n]\n"
    );
    let mesh = mesh_on(comm, "test_material_damage/plate.msh")?;
    let mut model = model_on(mesh, &dat)?;
    ok(model.init_full(&ModelOptions::new(method)), "init_full")?;
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
            uu[0] = 1e-3 * (0.6 * x + 0.3 * y * y);
            uu[1] = 1e-3 * (0.2 * x * y - 0.1 * y);
        }
    }
    ok(model.assemble_internal_forces(), "assemble")?;
    averaged(&model)
}

/// Gather `(key, values)` of every rank on every rank (small meshes).
fn gather(
    comm: &Communicator,
    local: &BTreeMap<(String, PointId), Vec<Real>>,
) -> Result<BTreeMap<(String, PointId), Vec<Real>>, String> {
    let mut buf: Vec<i64> = Vec::new();
    for ((name, (c, q)), v) in local {
        buf.push(name.len() as i64);
        buf.extend(name.bytes().map(i64::from));
        buf.push(c.len() as i64);
        buf.extend(c);
        buf.push(*q as i64);
        buf.push(v.len() as i64);
        buf.extend(v.iter().map(|x| x.to_bits() as i64));
    }
    let all = ok(comm.all_gather_v(&buf), "all_gather")?;
    let mut out = BTreeMap::new();
    for b in all {
        let mut i = 0;
        while i < b.len() {
            let nl = b[i] as usize;
            let name: String = b[i + 1..i + 1 + nl]
                .iter()
                .map(|&x| x as u8 as char)
                .collect();
            i += 1 + nl;
            let nc = b[i] as usize;
            let c = b[i + 1..i + 1 + nc].to_vec();
            i += 1 + nc;
            let q = b[i] as usize;
            let nv = b[i + 1] as usize;
            let v: Vec<Real> = b[i + 2..i + 2 + nv]
                .iter()
                .map(|&x| Real::from_bits(x as u64))
                .collect();
            i += 2 + nv;
            out.insert((name, (c, q)), v);
        }
    }
    Ok(out)
}

pub fn laws_vs_serial(comm: Communicator) -> Outcome {
    for (kw, params, method) in LAWS {
        let serial = law_run(&Communicator::serial(), kw, params, method)?;
        let dist = gather(&comm, &law_run(&comm, kw, params, method)?)?;
        if dist.len() != serial.len() || serial.is_empty() {
            return Err(format!(
                "{kw}: {} averaged values over the ranks, {} in serial",
                dist.len(),
                serial.len()
            ));
        }
        let mut scale: BTreeMap<&str, Real> = BTreeMap::new();
        for ((n, _), v) in &serial {
            let s = scale.entry(n.as_str()).or_insert(0.);
            for x in v {
                *s = s.max(x.abs());
            }
        }
        let mut worst: Real = 0.;
        for (k, v) in &dist {
            let s = serial
                .get(k)
                .ok_or_else(|| format!("{kw}: {k:?} not in the serial run"))?;
            let sc = scale[k.0.as_str()].max(Real::MIN_POSITIVE);
            for (a, b) in v.iter().zip(s) {
                let d = (a - b).abs() / sc;
                worst = if d.is_nan() {
                    Real::INFINITY
                } else {
                    worst.max(d)
                };
            }
        }
        if comm.rank() == 0 {
            eprintln!("INFO {kw} np{}: worst |Δ|/max {worst:e}", comm.size());
        }
        if worst.is_nan() || worst > 1e-14 {
            return Err(format!("{kw}: averages differ from serial by {worst:e}"));
        }
    }
    Ok(())
}

const T3: ElementType = ElementType::Triangle3;

/// On a distributed mesh the orthotropic non-local law
/// with UD-1005's compat (rank-local maximum) and UD-870's default (the
/// rank of the global maximum damages) is refused at `init_full`: every
/// rank returns an error, at least one rank the refusal itself (the others
/// may report the failure of another rank), promptly, and the communicator
/// stays usable. Both compat switches (upstream: each rank damages its
/// local maximum) and both defaults (the global maximum damages once) run.
pub fn xn11(comm: Communicator) -> Outcome {
    for (all_reduced, global_only, refused) in [
        (false, true, true),
        (false, false, false),
        (true, true, false),
    ] {
        let opts = ExtraLawOptions {
            orthotropic_non_local_max_all_reduced: all_reduced,
            orthotropic_damage_global_maximum_only: global_only,
            ..ExtraLawOptions::default()
        };
        let dat = "material damage_orthotropic_iterative_non_local [\n name = m\n rho = 1\n E = 1\n nu = 0.2\n Sc = 0.001\n]\n\
                   non_local m base_wf [\n radius = 0.5\n]\n";
        let mesh = mesh_on(&comm, "test_material_damage/plate.msh")?;
        let parser = ok(Parser::parse_str(dat, "material.dat"), "parser")?;
        let mut model = ok(
            SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser),
            "model",
        )?;
        ok(
            register_extra_materials_with(&mut model.physics.factory, opts),
            "register",
        )?;
        ok(
            register_extra_non_local_materials_with(&mut model.physics.factory, opts),
            "register non-local",
        )?;
        // The refused combination fails at init_full already
        // (agreed on every rank by the model's init_material agreement)
        let t0 = std::time::Instant::now();
        let init = model.init_full(&ModelOptions::new(AnalysisMethod::Static));
        match (refused && comm.size() > 1, init) {
            (true, Ok(())) => {
                return Err(
                    "the rank-local / global combination was not refused at init_full".into(),
                );
            }
            (true, Err(e)) => {
                let msg = e.to_string();
                let refusal = msg.contains("the rank-local maxima cannot designate the global one");
                if !refusal && !msg.contains("another rank") {
                    return Err(format!("unexpected init_full error: {e}"));
                }
                if t0.elapsed() > Duration::from_secs(10) {
                    return Err("the refusal at init_full was not prompt".into());
                }
                let mut one = [1u64];
                ok(comm.all_reduce(&mut one, ReduceOp::Sum), "all_reduce")?;
                if one[0] != comm.size() as u64 {
                    return Err("the communicator is not usable after the refusal".into());
                }
                let mut refused_here = [u64::from(refusal)];
                ok(
                    comm.all_reduce(&mut refused_here, ReduceOp::Sum),
                    "all_reduce",
                )?;
                if refused_here[0] == 0 {
                    return Err(format!(
                        "no rank received the refusal itself (this rank: {e})"
                    ));
                }
                continue;
            }
            (false, r) => ok(r, "init_full")?,
        }
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
                uu[1] = 1e-2 * nodes[2 * n + 1] * (1. + 0.1 * nodes[2 * n]);
            }
        }
        ok(model.assemble_internal_forces(), "assemble")?;
        let r = model.with_material(0, LawPhase::AfterSolveStep, |law, ctx| {
            as_iterative_damage_mut(law)
                .ok_or_else(|| akantu_core::Error::invalid("not iterative"))?
                .update_damage(ctx)
        });
        let n = ok(r, "update_damage")?;
        let mut v = [u64::from(n)];
        ok(comm.all_reduce(&mut v, ReduceOp::Max), "all_reduce")?;
        if v[0] == 0 {
            return Err("nothing damaged".into());
        }
    }
    Ok(())
}
