// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS); no upstream test:
// the deferred history saves of internal fields are port-specific
// (upstream copies at every `saveCurrentValues`, material.cc).

//! Deferred history saves against saves by copy, after failed steps.
//!
//! A block of an internal field that no foreign code views is saved lazily
//! and, when its writer announces a complete rewrite, by an exchange of
//! allocations. A block exported to a foreign runtime (a NumPy view) is
//! copied at every save, as upstream does. These tests run the same model
//! twice, once with the blocks under test exported (the reference: saves
//! by copy) and once without (deferred saves), make a step fail, and
//! require the current and previous values of the two runs to be the same
//! bits.

use akantu_core::array_shared::ForeignRuntime;
use akantu_core::array_storage::ExportLease;
use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::{InternalField, ModelOptions};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

const TRI: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

/// A 2×2-cell square of triangles.
fn square() -> SharedMesh {
    let mut mesh = Mesh::new(2, "square").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for j in 0..3 {
            for i in 0..3 {
                acc.push_node(&[0.5 * f64::from(i), 0.5 * f64::from(j)])
                    .unwrap();
            }
        }
        let mut c = acc.connectivity_mut(TRI, NG);
        for j in 0..2 {
            for i in 0..2 {
                let n = j * 3 + i;
                c.push_tuple(&[n, n + 1, n + 4]);
                c.push_tuple(&[n, n + 4, n + 3]);
            }
        }
    }
    SharedMesh::new(mesh)
}

fn model(law: &str) -> SolidMechanicsModel {
    let dat = format!("model solid_mechanics_model [\n{law}\n]\n");
    let parser = Parser::parse_str(&dat, "history.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(square(), None, "solid_mechanics_model", parser).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.5 * dt, None).unwrap();
    {
        let mut u = m.displacement_mut().unwrap();
        for (k, v) in u.as_mut_slice().iter_mut().enumerate() {
            *v = 1e-3 * (k as Real).sin();
        }
    }
    m
}

const ELASTIC: &str = "material elastic [\n name = a\n rho = 1\n E = 1\n nu = 0.3\n]";

type Field = fn(&mut SolidMechanicsModel) -> &mut InternalField<Real>;

fn grad_u(m: &mut SolidMechanicsModel) -> &mut InternalField<Real> {
    &mut m.material_mut(0).unwrap().base_mut().grad_u
}

fn stress(m: &mut SolidMechanicsModel) -> &mut InternalField<Real> {
    &mut m.material_mut(0).unwrap().base_mut().stress
}

fn bits(a: &[Real]) -> Vec<u64> {
    a.iter().map(|x| x.to_bits()).collect()
}

/// Current and previous values (bits) of each field.
type State = Vec<(Vec<u64>, Vec<u64>)>;

/// Current and previous values of the fields.
fn state(m: &mut SolidMechanicsModel, fields: &[Field]) -> State {
    fields
        .iter()
        .map(|f| {
            let f = f(m);
            (
                bits(f.get(TRI, NG).unwrap().as_slice()),
                bits(f.previous(TRI, NG).unwrap().as_slice()),
            )
        })
        .collect()
}

/// Two steps, then a step that fails as `break_step` arranges, then
/// `repair`; returns the state after the failure and after one more step.
fn run(
    law: &str,
    fields: &[Field],
    copy_every_save: bool,
    break_step: fn(&mut SolidMechanicsModel),
    repair: fn(&mut SolidMechanicsModel),
) -> (State, State) {
    let mut m = model(law);
    let rt = ForeignRuntime::new();
    // exported blocks are saved by copy (their leases live for the run)
    let _leases: Vec<ExportLease<Real>> = if copy_every_save {
        fields
            .iter()
            .map(|f| {
                f(&mut m)
                    .values_mut()
                    .shared_mut(TRI, NG)
                    .unwrap()
                    .export(&rt)
                    .unwrap()
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut interp = rt.enter().unwrap();
    let _native = interp.native().unwrap();
    m.solve_step(None).unwrap();
    m.solve_step(None).unwrap();
    break_step(&mut m);
    assert!(m.solve_step(None).is_err(), "the step must fail");
    repair(&mut m);
    let failed = state(&mut m, fields);
    m.solve_step(None).unwrap();
    (failed, state(&mut m, fields))
}

/// A filter index outside the mesh: the gradient fails in its input check,
/// after the history of `grad_u` was exchanged.
fn bad_filter(m: &mut SolidMechanicsModel) {
    let f = m.material_mut(0).unwrap().element_filter_mut();
    f.get_mut(TRI, NG).unwrap().as_mut_slice()[1] = 1000;
}

fn good_filter(m: &mut SolidMechanicsModel) {
    let f = m.material_mut(0).unwrap().element_filter_mut();
    f.get_mut(TRI, NG).unwrap().as_mut_slice()[1] = 1;
}

#[test]
fn a_failed_gradient_leaves_grad_u_as_saves_by_copy_do() {
    let fields: [Field; 2] = [grad_u, stress];
    let copied = run(ELASTIC, &fields, true, bad_filter, good_filter);
    let deferred = run(ELASTIC, &fields, false, bad_filter, good_filter);
    assert_eq!(deferred.0, copied.0, "after the failed step");
    assert_eq!(deferred.1, copied.1, "after the next step");
    // the failure left current = previous = the values of the last save
    for (cur, prev) in &deferred.0 {
        assert_eq!(cur, prev);
    }
}
