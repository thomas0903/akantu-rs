// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Node removal
// (SMM-015, UD-209): a node removal under a live
// SolidMechanicsModel after erasing elements **in the middle** of their
// block (upstream mesh.cc eraseElements fills the holes with the last
// elements, so the law filters of the live model and of a fresh model
// differ in order and no bitwise equality is expected).

//! Criterion: **patch test** (analytical, independent of every numbering).
//! Linear triangles reproduce a linear displacement field exactly; with the
//! field prescribed on every boundary node of the final mesh (outer
//! boundary and the edges of the erased cells), the interior nodes must
//! carry the same linear field after the removal: static (interior nodes
//! start from zero and are solved for), explicit and implicit dynamic (the
//! field is stationary: zero velocity, zero acceleration). Every check is
//! made **by node coordinates**, so the expected values do not depend on
//! the port's renumbering or compaction. A rounding-level agreement with a
//! fresh model is measured as well (reported, not a pass/fail tolerance
//! beyond a sanity bound).

mod node_removal_common;
mod par_smm;

use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{MeshPart, MeshTransaction, SharedMesh};
use akantu_model::ModelOptions;
use akantu_solid::SolidMechanicsModel;
use node_removal_common::{model_on, scrambled_square};

const NX: usize = 6;
const NY: usize = 4;

fn options(method: AnalysisMethod) -> ModelOptions {
    let mut o = ModelOptions::new(method);
    if method != AnalysisMethod::ExplicitLumpedMass {
        o.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    o
}

/// The two triangles of cell `(i, j)` of `scrambled_square(NX, NY, false)`.
fn cell(i: usize, j: usize) -> Vec<Element> {
    let e = 2 * (j * NX + i);
    (e..e + 2)
        .map(|k| Element::new(ElementType::Triangle3, k as Idx, GhostType::NotGhost))
        .collect()
}

/// The change: the bottom-right cell `(5, 0)` (elements 10, 11 of 48) and
/// the interior cell `(2, 1)` (elements 16, 17) erased — both in the middle
/// of the block — then `purify` (the corner `(6, 0)` is removed, every other
/// node permuted).
fn change(tx: &mut MeshTransaction<'_>) -> akantu_core::Result<()> {
    // one call: two `erase_elements` calls on one block in one mutation are
    // refused before the commit
    let mut els = cell(5, 0);
    els.extend(cell(2, 1));
    tx.erase_elements(MeshPart::Main, &els)?;
    tx.purify(MeshPart::Main)
}

/// Coordinates of the boundary of the **final** mesh: the outer boundary
/// and the corners of the erased cells.
fn on_final_boundary(x: &[Real]) -> bool {
    let (a, b) = (x[0], x[1]);
    let outer = a == 0.0 || b == 0.0 || a == NX as Real || b == NY as Real;
    let in_cell = |i: Real, j: Real| (a == i || a == i + 1.0) && (b == j || b == j + 1.0);
    outer || in_cell(5.0, 0.0) || in_cell(2.0, 1.0)
}

/// The linear field.
fn exact(x: &[Real]) -> [Real; 2] {
    [
        1e-3 * (1.0 + 2.0 * x[0] + 3.0 * x[1]),
        1e-3 * (-1.0 + 0.5 * x[0] - 1.5 * x[1]),
    ]
}

fn coords(m: &SolidMechanicsModel) -> Vec<Real> {
    m.physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec()
}

/// Boundary nodes blocked at the exact field; interior nodes at the exact
/// field too (`interior_exact`) or zero.
fn set_patch(m: &mut SolidMechanicsModel, interior_exact: bool) {
    let x = coords(m);
    let n = x.len() / 2;
    {
        let mut b = m.blocked_dofs_mut().unwrap();
        for k in 0..n {
            let bd = on_final_boundary(&x[2 * k..2 * k + 2]);
            b.as_mut_slice()[2 * k] = bd;
            b.as_mut_slice()[2 * k + 1] = bd;
        }
    }
    let mut u = m.displacement_mut().unwrap();
    for k in 0..n {
        let p = &x[2 * k..2 * k + 2];
        let e = if interior_exact || on_final_boundary(p) {
            exact(p)
        } else {
            [0.0, 0.0]
        };
        u.as_mut_slice()[2 * k..2 * k + 2].copy_from_slice(&e);
    }
}

/// max |u − exact| over all nodes (by coordinates), and the largest
/// velocity / acceleration component.
fn patch_error(m: &SolidMechanicsModel) -> (Real, Real, Real) {
    let x = coords(m);
    let u = m.displacement().unwrap().as_slice().to_vec();
    let mut err: Real = 0.0;
    for k in 0..x.len() / 2 {
        let e = exact(&x[2 * k..2 * k + 2]);
        err = err
            .max((u[2 * k] - e[0]).abs())
            .max((u[2 * k + 1] - e[1]).abs());
    }
    let maxabs = |v: Option<Vec<Real>>| {
        v.unwrap_or_default()
            .iter()
            .fold(0.0, |a: Real, b| a.max(b.abs()))
    };
    (
        err,
        maxabs(m.velocity().ok().map(|v| v.as_slice().to_vec())),
        maxabs(m.acceleration().ok().map(|v| v.as_slice().to_vec())),
    )
}

fn erased_in_the_middle_then(method: AnalysisMethod) -> (Real, Real, Real) {
    let shared = SharedMesh::new(scrambled_square(NX, NY, false));
    let mut a = model_on(&shared).unwrap();
    a.init_full(&options(method)).unwrap();
    let dynamic = method != AnalysisMethod::Static;
    set_patch(&mut a, dynamic);
    if dynamic {
        let dt = a.stable_time_step().unwrap();
        let dt = if method == AnalysisMethod::ExplicitLumpedMass {
            0.8 * dt
        } else {
            4.0 * dt
        };
        a.set_time_step(dt, None).unwrap();
        for _ in 0..3 {
            a.solve_step(None).unwrap();
        }
    } else {
        // a first static solve on the full mesh (assembles and factorises K)
        a.solve_step(None).unwrap();
        // interior back to zero: the solve after the removal must find it
        set_patch(&mut a, false);
    }
    let before = shared.read().unwrap().nb_nodes();
    let committed = a.mutate_mesh(change, None).unwrap();
    let after = shared.read().unwrap().nb_nodes();
    assert_eq!(after, before - 1, "the corner (6, 0) is removed");
    let numbering = node_removal_common::removal_numbering(&committed, before);
    assert!(
        numbering
            .iter()
            .enumerate()
            .any(|(i, &n)| n >= 0 && n as usize != i)
    );
    // the blocked flags followed their nodes (by coordinates)
    {
        let x = coords(&a);
        let b = a.blocked_dofs().unwrap().as_slice().to_vec();
        for k in 0..after {
            assert_eq!(
                b[2 * k],
                on_final_boundary(&x[2 * k..2 * k + 2]),
                "node {k}"
            );
        }
    }
    let steps = if dynamic { 5 } else { 1 };
    for _ in 0..steps {
        a.solve_step(None).unwrap();
    }
    let interior_free = {
        let b = a.blocked_dofs().unwrap().as_slice().to_vec();
        b.iter().filter(|&&x| !x).count()
    };
    assert!(
        interior_free >= 2 * 10,
        "a non-trivial patch ({interior_free} free DOFs)"
    );
    patch_error(&a)
}

/// Bound of the patch test: the field is O(1e-2); the solves are direct
/// (faer) or diagonal (lumped) on a 47-node mesh with E = 1, so rounding
/// stays near 1e-17 · κ; 1e-13 leaves three orders of margin and is far
/// below any renumbering error (a misplaced DOF moves a node by ≥ 1e-3).
const PATCH_TOL: Real = 1e-13;

#[test]
fn v134_static_patch_after_a_removal_in_the_middle_of_the_block() {
    let (e, _, _) = erased_in_the_middle_then(AnalysisMethod::Static);
    eprintln!("static patch error {e:e}");
    assert!(e <= PATCH_TOL, "static patch error {e:e}");
}

#[test]
fn v134_explicit_patch_after_a_removal_in_the_middle_of_the_block() {
    let (e, v, acc) = erased_in_the_middle_then(AnalysisMethod::ExplicitLumpedMass);
    eprintln!("explicit patch error {e:e}, |v| {v:e}, |a| {acc:e}");
    assert!(
        e <= PATCH_TOL && v <= 1e-11 && acc <= 1e-9,
        "{e:e} {v:e} {acc:e}"
    );
}

#[test]
fn v134_implicit_patch_after_a_removal_in_the_middle_of_the_block() {
    let (e, v, acc) = erased_in_the_middle_then(AnalysisMethod::ImplicitDynamic);
    eprintln!("implicit patch error {e:e}, |v| {v:e}, |a| {acc:e}");
    assert!(
        e <= PATCH_TOL && v <= 1e-11 && acc <= 1e-9,
        "{e:e} {v:e} {acc:e}"
    );
}

/// Measurement (not an equivalence claim): the live model after the
/// middle-of-block removal against a fresh model on the final mesh with the
/// pre-removal state moved by the test, explicit dynamics with a non-trivial
/// motion. Reports the largest difference; the sanity bound only rules out
/// a wrong state (a misplaced DOF gives differences of the size of the
/// field itself).
#[test]
fn v134_explicit_live_vs_fresh_after_a_removal_in_the_middle_of_the_block() {
    let method = AnalysisMethod::ExplicitLumpedMass;
    let setup = |m: &mut SolidMechanicsModel| {
        let x = coords(m);
        let n = x.len() / 2;
        {
            let mut b = m.blocked_dofs_mut().unwrap();
            for k in 0..n {
                let bd = x[2 * k] == 0.0;
                b.as_mut_slice()[2 * k] = bd;
                b.as_mut_slice()[2 * k + 1] = bd;
            }
        }
        let mut u = m.displacement_mut().unwrap();
        for k in 0..n {
            let s = x[2 * k] / NX as Real;
            u.as_mut_slice()[2 * k] = 1e-3 * s * s * (1.0 + 0.3 * x[2 * k + 1]);
            u.as_mut_slice()[2 * k + 1] = 5e-4 * s * (0.5 - 0.1 * x[2 * k]);
        }
    };
    let shared_a = SharedMesh::new(scrambled_square(NX, NY, false));
    let mut a = model_on(&shared_a).unwrap();
    a.init_full(&options(method)).unwrap();
    setup(&mut a);
    let dt = 0.8 * a.stable_time_step().unwrap();
    a.set_time_step(dt, None).unwrap();
    for _ in 0..6 {
        a.solve_step(None).unwrap();
    }
    let before = shared_a.read().unwrap().nb_nodes();
    let pre = node_removal_common::snapshot(&a).unwrap();
    let committed = a.mutate_mesh(change, None).unwrap();
    let after = shared_a.read().unwrap().nb_nodes();
    let numbering = node_removal_common::removal_numbering(&committed, before);
    let state = node_removal_common::moved(&pre, &numbering, after);
    node_removal_common::check_state(&a, &state, "right after the removal").unwrap();
    let shared_b = SharedMesh::new(scrambled_square(NX, NY, false));
    shared_b.mutate(&mut [], change, None).unwrap();
    let mut b = model_on(&shared_b).unwrap();
    b.init_full(&options(method)).unwrap();
    b.set_time_step(dt, None).unwrap();
    node_removal_common::set_state(&mut b, &state).unwrap();
    let mut worst: Real = 0.0;
    let mut bitwise = true;
    for _ in 0..6 {
        a.solve_step(None).unwrap();
        b.solve_step(None).unwrap();
        let ua = a.displacement().unwrap().as_slice().to_vec();
        let ub = b.displacement().unwrap().as_slice().to_vec();
        let scale = ub.iter().fold(0.0, |s: Real, v| s.max(v.abs()));
        let d = ua
            .iter()
            .zip(&ub)
            .fold(0.0, |s: Real, (p, q)| s.max((p - q).abs()));
        bitwise &= ua.iter().zip(&ub).all(|(p, q)| p.to_bits() == q.to_bits());
        worst = worst.max(d / scale);
    }
    let (ea, eb) = (
        a.energy("potential").unwrap(),
        b.energy("potential").unwrap(),
    );
    let (ka, kb) = (a.energy("kinetic").unwrap(), b.energy("kinetic").unwrap());
    eprintln!(
        "live vs fresh after a middle-of-block removal: max relative |Δu| {worst:e} \
         (bitwise: {bitwise}); potential {ea:e} vs {eb:e}; kinetic {ka:e} vs {kb:e}"
    );
    assert!(worst <= 1e-10, "{worst:e}");
    assert!(((ea + ka) - (eb + kb)).abs() <= 1e-10 * (eb + kb).abs());
}
