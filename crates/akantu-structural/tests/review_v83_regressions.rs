// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): regressions
// of the structural mechanics model (src/model/structural_mechanics/
// structural_mechanics_model.cc, structural_mechanics_model_mass.cc).
// No upstream counterpart.

//! * Each guard of `set_extra_normals` on its own path (fails with the
//!   validation removed or with the restore removed);
//! * One test per compared material field of the `st07_*` content
//!   check (fails with any one field dropped from the comparison);
//! * The zero / non-finite lumped-mass check (UD-518) also holds
//!   when a DOF is unblocked after the lumped assembly (fails on the
//!   version before the fix: `solve_step` returned `Ok` with
//!   non-finite accelerations).

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Error, Idx, Real};
use akantu_fe::structural::EXTRA_NORMAL;
use akantu_mesh::Mesh;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

fn material() -> StructuralMaterial {
    StructuralMaterial {
        e: 2e5,
        a: 0.01,
        i: 2e-6,
        iz: 3e-6,
        iy: 1e-6,
        gj: 50.,
        rho: 7.8,
        t: 0.1,
        nu: 0.3,
    }
}

fn model_of(
    mesh: Mesh,
    mat: StructuralMaterial,
    method: AnalysisMethod,
    compat: StructuralModelUpstreamCompat,
) -> StructuralMechanicsModel {
    let mut m = StructuralMechanicsModel::with_compat(shared(mesh), None, "v83", compat).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(method).unwrap();
    m
}

// ----: the two guards of set_extra_normals ----------------------------

/// Static cantilever along x (3 elements of total length `len`, normal z),
/// clamped at node 0, tip forces 2 (y) and 1 (z).
fn cantilever(len: Real, compat: StructuralModelUpstreamCompat) -> StructuralMechanicsModel {
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        3,
        len,
        &[0., 0., 0.],
        &[1., 0., 0.],
        Some(&[0., 0., 1.]),
    );
    let mut m = model_of(mesh, material(), AnalysisMethod::Static, compat);
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..6].fill(true);
    {
        let mut f_guard = m.external_force_mut().unwrap();
        let f = f_guard.as_mut_slice();
        f[3 * 6 + 1] = 2.0;
        f[3 * 6 + 2] = 1.0;
    }
    m
}

fn stored_normals(m: &StructuralMechanicsModel) -> Vec<Real> {
    m.physics
        .base
        .mesh
        .read()
        .unwrap()
        .data()
        .reals(EXTRA_NORMAL)
        .unwrap()
        .get(ElementType::BernoulliBeam3, G)
        .unwrap()
        .as_slice()
        .to_vec()
}

fn revision(m: &StructuralMechanicsModel) -> u64 {
    m.physics.base.mesh.revision().unwrap()
}

/// Validation guard alone (mutant M10): a refused (skew) normal
/// leaves the mesh untouched, not even a data-only mutation (the mesh
/// revision does not move), and the error says so. Without the validation
/// the restore still puts the rows back, but the mesh goes through two
/// data-only mutations and two FE rebuilds.
#[test]
fn st04_validation_path_leaves_the_mesh_revision_unchanged() {
    let mut m = cantilever(1.5, StructuralModelUpstreamCompat::default());
    m.solve_step(None).unwrap();
    let before = stored_normals(&m);
    let r0 = revision(&m);
    let err = m
        .set_extra_normals(ElementType::BernoulliBeam3, &[0.6, 0., 0.8].repeat(3))
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidArgument(ref s) if s.contains("UD-343") && s.contains("mesh unchanged")),
        "{err}"
    );
    assert_eq!(
        revision(&m),
        r0,
        "a refused normal must not mutate the mesh"
    );
    assert_eq!(stored_normals(&m), before);
}

/// Restore guard alone (mutant M11): in `UPSTREAM` mode (unchecked
/// normals, UD-343) the normal `(0, 0, 1e-10)` of a `1e-150` long beam
/// passes the validation (same frame construction as the refresh, without
/// the refresh's finite-cache guard), then the engine's refresh refuses the
/// non-finite `B·T` cache it produces. The old rows must be put back and
/// the FE data rebuilt from them; without the restore the refused rows
/// stay in the mesh.
#[test]
fn st04_restore_path_after_a_normal_dependent_refresh_failure() {
    let mut m = cantilever(1e-150, StructuralModelUpstreamCompat::UPSTREAM);
    let before = stored_normals(&m);
    let rot_before = m
        .rotations(ElementType::BernoulliBeam3, G)
        .unwrap()
        .as_slice()
        .to_vec();
    let refused = [0., 0., 1e-10].repeat(3);
    let err = m
        .set_extra_normals(ElementType::BernoulliBeam3, &refused)
        .unwrap_err();
    let msg = err.to_string();
    assert!(
        !msg.contains("mesh unchanged"),
        "must be refused by the refresh, not by the validation: {msg}"
    );
    assert!(
        matches!(err, Error::Numerical(ref s) if s.contains("non-finite")),
        "{err}"
    );
    assert_eq!(stored_normals(&m), before, "old rows restored");
    let rot_after = m
        .rotations(ElementType::BernoulliBeam3, G)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(rot_after, rot_before, "FE data rebuilt from the old rows");
    // the restored model is usable and equals a fresh one (the tiny beams
    // cannot be solved, K overflows: compare the stress-free state)
    let fresh = cantilever(1e-150, StructuralModelUpstreamCompat::UPSTREAM);
    let rot_fresh = fresh
        .rotations(ElementType::BernoulliBeam3, G)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(rot_after, rot_fresh);
}

// ----: one test per field of the content check ------------------

#[derive(Clone, Copy, Debug)]
enum Kind {
    Beam2,
    Beam3,
    Dkt,
}

fn mesh_of_kind(kind: Kind) -> Mesh {
    match kind {
        Kind::Beam2 => mesh_of(
            2,
            ElementType::BernoulliBeam2,
            &[vec![0., 0.], vec![0.6, 0.35], vec![1.1, 0.9]],
            &[vec![0, 1], vec![1, 2]],
            None,
        ),
        Kind::Beam3 => {
            let nodes = vec![vec![0., 0., 0.], vec![0.4, 0.3, 0.2], vec![0.75, 0.7, 0.3]];
            let normals: Vec<Vec<Real>> = [[0usize, 1], [1, 2]]
                .iter()
                .map(|&[a, b]| {
                    let x: Vec<Real> = (0..3).map(|d| nodes[b][d] - nodes[a][d]).collect();
                    let h = [0.1, -0.3, 1.0];
                    let n = [
                        x[1] * h[2] - x[2] * h[1],
                        x[2] * h[0] - x[0] * h[2],
                        x[0] * h[1] - x[1] * h[0],
                    ];
                    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                    vec![n[0] / l, n[1] / l, n[2] / l]
                })
                .collect();
            mesh_of(
                3,
                ElementType::BernoulliBeam3,
                &nodes,
                &[vec![0, 1], vec![1, 2]],
                Some(&normals),
            )
        }
        Kind::Dkt => mesh_of(
            3,
            ElementType::DiscreteKirchhoffTriangle18,
            &[
                vec![0., 0., 0.],
                vec![1., 0., 0.],
                vec![0., 1., 0.],
                vec![1.2, 1.1, 0.],
            ],
            &[vec![0, 1, 2], vec![1, 3, 2]],
            None,
        ),
    }
}

/// Beams: explicit lumped analysis (`K`, `M` and the lumped mass); DKT:
/// static (`K` only; no lumped mass for shells). Non-zero displacements.
fn field_model(kind: Kind, mat: StructuralMaterial) -> StructuralMechanicsModel {
    let method = match kind {
        Kind::Dkt => AnalysisMethod::Static,
        _ => AnalysisMethod::ExplicitLumpedMass,
    };
    let mut m = model_of(
        mesh_of_kind(kind),
        mat,
        method,
        StructuralModelUpstreamCompat::default(),
    );
    for (k, u) in m
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *u = 1e-3 * (0.7 * k as Real + 0.3).sin();
    }
    m
}

/// What an assembly produces, as bit patterns: the potential energy (`K`),
/// and for beams the dense consistent `M` and the lumped mass.
fn snapshot(m: &mut StructuralMechanicsModel, kind: Kind) -> (u64, Vec<u64>, Vec<u64>) {
    let ep = m.potential_energy().unwrap().to_bits();
    if matches!(kind, Kind::Dkt) {
        return (ep, Vec::new(), Vec::new());
    }
    m.assemble_mass_matrix().unwrap();
    m.assemble_lumped_mass_matrix().unwrap();
    let mm = dense(m, "M").concat().iter().map(|v| v.to_bits()).collect();
    let lm = m
        .lumped_mass()
        .unwrap()
        .as_slice()
        .iter()
        .map(|v| v.to_bits())
        .collect();
    (ep, mm, lm)
}

/// A write of exactly one material parameter through `materials_mut()`
/// (the raw binding accessor, no invalidation; D-047 zero-copy views) is
/// detected by the next assembly, on every element type where the field
/// acts: the result differs from the stale one and equals, bitwise, a fresh
/// model built with the new value.
fn single_field_write_triggers_reassembly(edit: fn(&mut StructuralMaterial), kinds: &[Kind]) {
    for &kind in kinds {
        let mut m = field_model(kind, material());
        let stale = snapshot(&mut m, kind);
        edit(&mut m.materials_mut()[0]);
        let after = snapshot(&mut m, kind);
        let mut mat = material();
        edit(&mut mat);
        let mut fresh = field_model(kind, mat);
        let want = snapshot(&mut fresh, kind);
        assert_ne!(after, stale, "{kind:?}: stale assembly reused");
        assert_eq!(after, want, "{kind:?}: differs from a fresh model");
        // unchanged inputs afterwards: the same values again
        assert_eq!(snapshot(&mut m, kind), after, "{kind:?}: second call");
    }
}

#[test]
fn st07_field_e_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.e *= 1.5, &[Kind::Beam2, Kind::Beam3, Kind::Dkt]);
}

#[test]
fn st07_field_a_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.a *= 1.5, &[Kind::Beam2, Kind::Beam3]);
}

#[test]
fn st07_field_i_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.i *= 1.5, &[Kind::Beam2]);
}

#[test]
fn st07_field_iz_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.iz *= 1.5, &[Kind::Beam3]);
}

#[test]
fn st07_field_iy_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.iy *= 1.5, &[Kind::Beam3]);
}

#[test]
fn st07_field_gj_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.gj *= 1.5, &[Kind::Beam3]);
}

#[test]
fn st07_field_rho_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.rho *= 1.5, &[Kind::Beam2, Kind::Beam3]);
}

#[test]
fn st07_field_t_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.t *= 1.5, &[Kind::Dkt]);
}

#[test]
fn st07_field_nu_alone_triggers_reassembly() {
    single_field_write_triggers_reassembly(|m| m.nu = 0.2, &[Kind::Dkt]);
}

// ----: unblocking after the lumped assembly ---------------------------

fn all_finite(m: &StructuralMechanicsModel) -> bool {
    let finite = |a: &[Real]| a.iter().all(|v| v.is_finite());
    finite(m.displacement().unwrap().as_slice())
        && finite(m.velocity().unwrap().as_slice())
        && finite(m.acceleration().unwrap().as_slice())
}

fn is_lumped_mass_refusal(err: &Error, node: usize) -> bool {
    matches!(err, Error::Numerical(s)
        if s.starts_with(&format!("lumped mass of node {node} DOF 0 is 0e0")))
}

/// A node without any beam, blocked at the lumped assembly and
/// unblocked afterwards, is refused at the next `solve_step` (and at the
/// next lumped assembly) with the error a fresh assembly gives, instead of
/// reaching the explicit update with a zero mass (`solve_step` used to
/// return `Ok` with non-finite accelerations). Blocking it again makes the
/// step valid.
#[test]
fn st09_unblocking_a_beamless_node_after_the_lumped_assembly_is_refused() {
    let nodes = vec![
        vec![0., 0.],
        vec![0.6, 0.35],
        vec![1.1, 0.9],
        vec![3.0, 3.0],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    let make = || {
        model_of(
            mesh_of(2, ElementType::BernoulliBeam2, &nodes, &conn, None),
            material(),
            AnalysisMethod::ExplicitLumpedMass,
            StructuralModelUpstreamCompat::default(),
        )
    };
    // the error of a fresh assembly with node 3 free
    let expected = make().assemble_lumped_mass_matrix().unwrap_err();
    assert!(is_lumped_mass_refusal(&expected, 3), "{expected}");

    let mut m = make();
    m.blocked_dofs_mut().unwrap().as_mut_slice()[9..12].fill(true);
    m.assemble_lumped_mass_matrix().unwrap();
    m.blocked_dofs_mut().unwrap().as_mut_slice()[9..12].fill(false);
    m.external_force_mut().unwrap().as_mut_slice()[9] = 1.0;
    let err = m.solve_step(None).unwrap_err();
    assert_eq!(err.to_string(), expected.to_string());
    assert!(all_finite(&m), "non-finite state after the refused step");
    let err = m.assemble_lumped_mass_matrix().unwrap_err();
    assert_eq!(err.to_string(), expected.to_string());
    // blocked again: valid
    m.blocked_dofs_mut().unwrap().as_mut_slice()[9..12].fill(true);
    m.solve_step(None).unwrap();
    assert!(all_finite(&m));
}

/// The lumped-mass check with `ρ = 0` (upstream's default density): every DOF blocked at
/// the lumped assembly (zero masses accepted), then node 1 unblocked: the
/// next `solve_step` is refused, in 2-D and 3-D.
#[test]
fn st09_unblocking_a_massless_node_after_the_lumped_assembly_is_refused() {
    let massless = StructuralMaterial {
        rho: 0.0,
        ..material()
    };
    for dim in [2, 3] {
        let mesh = if dim == 2 {
            line_mesh(
                2,
                ElementType::BernoulliBeam2,
                3,
                1.5,
                &[0., 0.],
                &[1., 0.],
                None,
            )
        } else {
            line_mesh(
                3,
                ElementType::BernoulliBeam3,
                3,
                1.5,
                &[0., 0., 0.],
                &[1., 0., 0.],
                Some(&[0., 0., 1.]),
            )
        };
        let mut m = model_of(
            mesh,
            massless,
            AnalysisMethod::ExplicitLumpedMass,
            StructuralModelUpstreamCompat::default(),
        );
        let ndof = m.nb_degree_of_freedom();
        m.blocked_dofs_mut().unwrap().as_mut_slice().fill(true);
        m.assemble_lumped_mass_matrix().unwrap();
        m.blocked_dofs_mut().unwrap().as_mut_slice()[ndof..2 * ndof].fill(false);
        m.external_force_mut().unwrap().as_mut_slice()[ndof] = 1.0;
        let err = m.solve_step(None).unwrap_err();
        assert!(is_lumped_mass_refusal(&err, 1), "{dim}-D: {err}");
        assert!(all_finite(&m), "{dim}-D: non-finite state");
    }
}
