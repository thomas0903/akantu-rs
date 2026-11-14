// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Runtime-defined
// cohesive laws (D-078; upstream python/py_material.cc
// PyMaterialCohesive and the MaterialCohesive base it derives from) driven
// from Rust, without the Python package: a law whose behaviour is a Rust
// `ForeignCohesiveMethods` object, created by an allocator of the model's
// material factory as the binding's `MaterialFactory.registerAllocator`
// does. The Python side (the same law type behind a Python subclass) is
// tested by python/tests/test_cohesive.py and the example EX-038.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::laws::foreign::ForeignCohesiveMethods;
use akantu_cohesive::material::MaterialCohesive;
use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Error, GhostType, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{Allocator, LawContext, LawSpec, ModelServices};
use akantu_parser::Parser;
use akantu_parser::param::{ParamValue, ParameterAccess};
use akantu_solid::{Material, RuntimeLawState};

const COH: ElementType = ElementType::Cohesive2d4;
const NG: GhostType = GhostType::NotGhost;

/// Two unit quadrangles stacked along y (interface at y = 1), as
/// `independent_checks.rs`.
fn two_quads() -> SharedMesh {
    let mut m = Mesh::new(2, "two_quads").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for p in [[0., 0.], [1., 0.], [1., 1.], [0., 1.], [0., 2.], [1., 2.]] {
            acc.push_node(&p).unwrap();
        }
        let mut c = acc.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
        c.try_push_tuple(&[0, 1, 2, 3]).unwrap();
        c.try_push_tuple(&[3, 2, 5, 4]).unwrap();
    }
    SharedMesh::new(m)
}

/// What the runtime law records of its calls.
#[derive(Debug, Default)]
struct Calls {
    init: AtomicUsize,
    traction: AtomicUsize,
}

/// A linear spring `t = k δ` (vector opening): the runtime parameter `k`
/// comes from the input file, the tractions are written into the law's own
/// `tractions` from its `opening`.
#[derive(Debug)]
struct Spring {
    calls: Arc<Calls>,
    /// Fail at this traction call (1-based), to test the error path.
    fail_at: Option<usize>,
    /// Whether the law has its own insertion check, run by the caller
    /// between the model's prepare and finish phases (as the Python
    /// binding runs a Python `checkInsertion`).
    checks: bool,
}

impl ForeignCohesiveMethods for Spring {
    fn init_material(&mut self, law: &mut MaterialCohesive, ctx: &LawContext<'_>) -> Result<()> {
        self.calls.init.fetch_add(1, Ordering::SeqCst);
        law.base_init(ctx)
    }

    fn compute_traction(
        &mut self,
        law: &mut MaterialCohesive,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let n = self.calls.traction.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_at == Some(n) {
            return Err(Error::invalid("spring: refused on purpose"));
        }
        let ParamValue::Real(k) = RuntimeLawState::get_param(law, "k")? else {
            return Err(Error::invalid("k is not a real parameter"));
        };
        let opening = law
            .internal("opening")?
            .values()
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        let tractions = law
            .own_real_internal_mut("tractions")
            .ok_or_else(|| Error::invalid("no tractions internal"))?;
        let mut t = tractions.values_mut().get_mut(ty, ghost)?;
        for (t, o) in t.as_mut_slice().iter_mut().zip(&opening) {
            *t = k * o;
        }
        Ok(())
    }

    fn has_insertion_check(&self) -> bool {
        self.checks
    }
}

fn spring_allocator(
    calls: &Arc<Calls>,
    fail_at: Option<usize>,
    checks: bool,
) -> Allocator<dyn Material> {
    let calls = Arc::clone(calls);
    Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
        let mut law = MaterialCohesive::new_foreign(
            spec,
            "rust_spring",
            services.mesh(),
            Box::new(Spring {
                calls: Arc::clone(&calls),
                fail_at,
                checks,
            }),
        )?;
        RuntimeLawState::register_param_real(
            &mut law,
            "k",
            ParameterAccess::PARSMOD,
            "spring stiffness",
            None,
        )?;
        Ok(Box::new(law) as Box<dyn Material>)
    })
}

const BULK: &str =
    "  material elastic [\n    name = body\n    rho = 1\n    E = 1e6\n    nu = 0.\n  ]\n";

/// The intrinsic model with one cohesive element of the runtime law
/// (stiffness `k`).
fn model(
    k: Real,
    method: AnalysisMethod,
    fail_at: Option<usize>,
) -> (SolidMechanicsModelCohesive, Arc<Calls>) {
    let dat = format!(
        "{BULK}material rust_spring [\n  name = insertion\n  sigma_c = 1\n  k = {k:e}\n]\n"
    );
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut m = SolidMechanicsModelCohesive::with_parser(
        two_quads(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    let calls = Arc::new(Calls::default());
    m.smm
        .physics
        .factory
        .register("rust_spring", spring_allocator(&calls, fail_at, false))
        .unwrap();
    m.ext.inserter.set_limit(1, 0.9, 1.1).unwrap();
    m.init_full(&ModelOptions::new(method), false).unwrap();
    (m, calls)
}

/// Move the top element rigidly by `(dx, dy)` (every other node fixed).
fn open(m: &mut SolidMechanicsModelCohesive, dx: Real, dy: Real) {
    let conn = m
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .connectivity(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap()
        .tuple(1)
        .to_vec();
    let mut u_g = m.smm.displacement_mut().unwrap();
    let u = u_g.as_mut_slice();
    u.fill(0.);
    for n in conn {
        u[2 * n as usize] = dx;
        u[2 * n as usize + 1] = dy;
    }
}

#[test]
fn runtime_cohesive_law_computes_the_tractions_the_model_assembles() {
    let _g = common::lock();
    let (mut m, calls) = model(2.5, AnalysisMethod::ExplicitLumpedMass, None);
    assert_eq!(calls.init.load(Ordering::SeqCst), 1, "initMaterial once");
    let law = m.cohesive_material("insertion").unwrap();
    assert!(law.is_foreign());
    assert_eq!(
        RuntimeLawState::get_param(law, "k").unwrap(),
        ParamValue::Real(2.5),
        "the runtime parameter is set from the input file"
    );
    assert_eq!(
        m.smm.physics.base.mesh.read().unwrap().nb_element(COH, NG),
        1
    );
    open(&mut m, 3e-3, 7e-3);
    m.assemble_internal_forces().unwrap();
    assert!(calls.traction.load(Ordering::SeqCst) >= 1);
    let law = m.cohesive_material("insertion").unwrap();
    let opening = law.array("opening", COH, NG).unwrap().as_slice().to_vec();
    let tractions = law.array("tractions", COH, NG).unwrap().as_slice().to_vec();
    assert_eq!(opening.len(), tractions.len());
    assert!(opening.iter().any(|&o| o != 0.0), "{opening:?}");
    for (t, o) in tractions.iter().zip(&opening) {
        assert_eq!(*t, 2.5 * o);
    }
    let f1 = m.smm.internal_force().unwrap().as_slice().to_vec();
    assert!(f1.iter().any(|&f| f != 0.0));
    // the assembled forces are linear in the law's tractions: doubling k
    // (an exact scaling) doubles every component bit for bit
    let (mut m2, _) = model(5.0, AnalysisMethod::ExplicitLumpedMass, None);
    open(&mut m2, 3e-3, 7e-3);
    m2.assemble_internal_forces().unwrap();
    let f2 = m2.smm.internal_force().unwrap().as_slice().to_vec();
    for (a, b) in f1.iter().zip(&f2) {
        assert_eq!(2.0 * a, *b);
    }
}

#[test]
fn runtime_cohesive_law_errors_reach_the_caller() {
    let _g = common::lock();
    let (mut m, _) = model(2.5, AnalysisMethod::ExplicitLumpedMass, Some(1));
    open(&mut m, 0.0, 1e-3);
    let e = m.assemble_internal_forces().unwrap_err();
    assert!(e.to_string().contains("refused on purpose"), "{e}");
    // the behaviour object is back in the law after the failed call: the
    // next call computes
    m.assemble_internal_forces().unwrap();
}

#[test]
fn runtime_cohesive_law_without_tangent_refuses_an_implicit_solve() {
    // upstream `MaterialCohesive::computeTangentTraction` is
    // `AKANTU_TO_IMPLEMENT`: a law that does not define it cannot take
    // part in a stiffness assembly; the port refuses, never assembles 0
    let _g = common::lock();
    let (mut m, _) = model(2.5, AnalysisMethod::Static, None);
    let e = m.solve_step(None).unwrap_err();
    assert!(e.to_string().contains("computeTangentTraction"), "{e}");
}

/// The extrinsic model whose runtime law checks the insertion itself (the
/// interface facet at y = 1 is the only checkable facet).
fn extrinsic_model() -> SolidMechanicsModelCohesive {
    let dat =
        format!("{BULK}material rust_spring [\n  name = insertion\n  sigma_c = 1\n  k = 1\n]\n");
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut m = SolidMechanicsModelCohesive::with_parser(
        two_quads(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    let calls = Arc::new(Calls::default());
    m.smm
        .physics
        .factory
        .register("rust_spring", spring_allocator(&calls, None, true))
        .unwrap();
    m.ext.inserter.set_limit(1, 0.9, 1.1).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
        .unwrap();
    m
}

/// The runtime law's check (the caller's part of the phased check): flag
/// every checkable facet in the staged, exportable flags.
fn flag_checkable(m: &mut SolidMechanicsModelCohesive) -> usize {
    let check = m
        .ext
        .inserter
        .check_facets()
        .get(ElementType::Segment2, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let staged = m
        .staged_insertion_facets_mut(ElementType::Segment2, NG)
        .unwrap();
    let mut flags = staged.borrow_mut();
    let mut n = 0;
    for (f, c) in flags.as_mut_slice().iter_mut().zip(&check) {
        if *c {
            *f = true;
            n += 1;
        }
    }
    n
}

fn nb_cohesive(m: &SolidMechanicsModelCohesive) -> usize {
    m.smm.physics.base.mesh.read().unwrap().nb_element(COH, NG)
}

fn nb_flagged(m: &SolidMechanicsModelCohesive) -> usize {
    m.ext
        .inserter
        .insertion_facets()
        .as_map()
        .iter_all()
        .map(|(_, _, a)| a.as_slice().iter().filter(|&&f| f).count())
        .sum()
}

#[test]
fn reentrant_insertion_check_is_refused_and_the_running_check_finishes() {
    // UD-985, port-only: while the runtime laws check
    // the insertion, a second check and the model's mesh-changing calls
    // are refused and change nothing; the running check then inserts what
    // its laws flagged, and later checks work
    let _g = common::lock();
    let mut m = extrinsic_model();
    let check = m.check_cohesive_stress_prepare().unwrap();
    assert_eq!(check.foreign_laws.len(), 1);
    assert!(m.insertion_check_in_progress());
    assert_eq!(flag_checkable(&mut m), 1);
    for e in [
        m.check_cohesive_stress_prepare().map(|_| 0).unwrap_err(),
        m.check_cohesive_stress().unwrap_err(),
        m.insert_elements().unwrap_err(),
        m.insert_intrinsic_elements().unwrap_err(),
        m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
            .unwrap_err(),
        m.insert_facets_only().map(|_| 0).unwrap_err(),
        m.init_constitutive_laws().unwrap_err(),
    ] {
        assert!(e.to_string().contains("inside a checkInsertion"), "{e}");
    }
    // the running check's flags are intact
    assert!(m.insertion_check_in_progress());
    let staged = m
        .staged_insertion_facets_mut(ElementType::Segment2, NG)
        .unwrap();
    assert_eq!(staged.borrow().as_slice().iter().filter(|&&f| f).count(), 1);
    assert_eq!(m.check_cohesive_stress_finish(check).unwrap(), 1);
    assert!(!m.insertion_check_in_progress());
    assert_eq!(nb_cohesive(&m), 1);
    assert_eq!(nb_flagged(&m), 0);
    // a later check: the inserter has its flag map (nothing left to insert)
    let again = m.check_cohesive_stress_prepare().unwrap();
    assert_eq!(m.check_cohesive_stress_finish(again).unwrap(), 0);
    assert_eq!(nb_cohesive(&m), 1);
}

#[test]
fn insertion_check_refuses_to_finish_after_a_mesh_change() {
    // A mesh changed between the phases (here by a
    // transaction from outside the cohesive model) abandons the check:
    // error, nothing inserted, flags restored; a stale check (finished or
    // superseded) changes nothing; the next check works
    let _g = common::lock();
    let mut m = extrinsic_model();
    let check = m.check_cohesive_stress_prepare().unwrap();
    assert_eq!(flag_checkable(&mut m), 1);
    let mesh = m.smm.physics.base.mesh.clone();
    let before = mesh.revision().unwrap();
    {
        let (token, mut parts) = m.smm.split_for_mutation();
        mesh.mutate(&mut [(token, &mut parts)], |_| Ok(()), None)
            .unwrap();
    }
    assert_ne!(mesh.revision().unwrap(), before, "the mutation commits");
    let e = m.check_cohesive_stress_finish(check).unwrap_err();
    assert!(e.to_string().contains("the mesh changed"), "{e}");
    assert!(!m.insertion_check_in_progress());
    assert_eq!(nb_cohesive(&m), 0);
    assert_eq!(nb_flagged(&m), 0, "the check's flags are restored");
    // a superseded check is refused at its finish and ignored by an abort
    let first = m.check_cohesive_stress_prepare().unwrap();
    m.abort_insertion_check(&first).unwrap();
    assert!(!m.insertion_check_in_progress());
    let second = m.check_cohesive_stress_prepare().unwrap();
    assert_eq!(flag_checkable(&mut m), 1);
    m.abort_insertion_check(&first).unwrap();
    assert!(
        m.insertion_check_in_progress(),
        "a stale abort changes nothing"
    );
    assert_eq!(m.check_cohesive_stress_finish(second).unwrap(), 1);
    let e = m.check_cohesive_stress_finish(first).unwrap_err();
    assert!(e.to_string().contains("not the open one"), "{e}");
    assert_eq!(nb_cohesive(&m), 1);
}

#[test]
fn an_unreadable_staged_block_returns_cleared_and_the_check_is_abandoned() {
    // A staged block whose values cannot be read when
    // the check ends (exported to a runtime that is not in a native
    // section; unreachable through the bindings, whose entries open one)
    // returns to the inserter cleared, with its shape, before the error is
    // reported: the inserter never loses its flag map, the check is
    // abandoned (nothing inserted, flags restored) and later checks work
    let _g = common::lock();
    let mut m = extrinsic_model();
    let seg = ElementType::Segment2;
    let nb_facets = |m: &SolidMechanicsModelCohesive| {
        m.ext
            .inserter
            .insertion_facets()
            .get(seg, NG)
            .map(akantu_core::Array::len)
    };
    let len = nb_facets(&m).unwrap();
    assert!(len > 0);
    let rt = akantu_core::ForeignRuntime::new();
    // finish: refused, nothing inserted
    let check = m.check_cohesive_stress_prepare().unwrap();
    assert_eq!(flag_checkable(&mut m), 1);
    let lease = m
        .staged_insertion_facets_mut(seg, NG)
        .unwrap()
        .export(&rt)
        .unwrap();
    let e = m.check_cohesive_stress_finish(check).unwrap_err();
    assert!(e.to_string().contains("native section"), "{e}");
    assert!(!m.insertion_check_in_progress());
    assert_eq!(
        nb_facets(&m).unwrap(),
        len,
        "the block is back, its shape kept"
    );
    assert_eq!(
        nb_flagged(&m),
        0,
        "cleared, then the check's flags restored"
    );
    assert_eq!(nb_cohesive(&m), 0);
    drop(lease);
    // abort: the same, and the open check is closed
    let check = m.check_cohesive_stress_prepare().unwrap();
    assert_eq!(flag_checkable(&mut m), 1);
    let lease = m
        .staged_insertion_facets_mut(seg, NG)
        .unwrap()
        .export(&rt)
        .unwrap();
    let e = m.abort_insertion_check(&check).unwrap_err();
    assert!(e.to_string().contains("native section"), "{e}");
    assert!(!m.insertion_check_in_progress());
    assert_eq!(nb_facets(&m).unwrap(), len);
    assert_eq!(nb_flagged(&m), 0);
    drop(lease);
    let e = m.check_cohesive_stress_finish(check).unwrap_err();
    assert!(e.to_string().contains("not the open one"), "{e}");
    // a later check inserts what it flags
    let again = m.check_cohesive_stress_prepare().unwrap();
    assert_eq!(flag_checkable(&mut m), 1);
    assert_eq!(m.check_cohesive_stress_finish(again).unwrap(), 1);
    assert_eq!(nb_cohesive(&m), 1);
}
