// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): regressions
// of the
// structural mechanics model (src/model/structural_mechanics/
// structural_mechanics_model.cc, structural_mechanics_model_mass.cc).
// No upstream counterpart.

//! Each test fails on the version before the fix or on its kernel-API
//! adaptation, and passes after the fixes.

mod structural_model_common;

use akantu_core::enums::{AnalysisMethod, EventPriority};
use akantu_core::{ElementType, Error, Idx, Real, Result};
use akantu_fe::structural::EXTRA_NORMAL;
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart};
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

fn model_of(
    mesh: Mesh,
    mat: StructuralMaterial,
    method: AnalysisMethod,
    compat: StructuralModelUpstreamCompat,
) -> StructuralMechanicsModel {
    let mut m = StructuralMechanicsModel::with_compat(shared(mesh), None, "v76", compat).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(method).unwrap();
    m
}

fn beam_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 2e5,
        a: 0.03,
        i: 3e-4,
        iz: 2.5e-4,
        iy: 1.1e-4,
        gj: 60.,
        rho: 7.8,
        ..StructuralMaterial::default()
    }
}

/// Inclined 2-D chain (the polyline of the oracle case
/// `v76_beam2_lumped_pre_energy`).
fn chain_2d() -> Mesh {
    let nodes = vec![
        vec![0., 0.],
        vec![0.6, 0.35],
        vec![1.1, 0.9],
        vec![1.75, 1.2],
        vec![2.2, 1.8],
    ];
    let conn: Vec<Vec<Idx>> = (0..4).map(|i| vec![i, i + 1]).collect();
    mesh_of(2, ElementType::BernoulliBeam2, &nodes, &conn, None)
}

/// Unit normal orthogonal to `b − a`, from a helper vector (`(b − a) × h`).
fn unit_normal(a: &[Real], b: &[Real], h: &[Real]) -> Vec<Real> {
    let x = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let n = [
        x[1] * h[2] - x[2] * h[1],
        x[2] * h[0] - x[0] * h[2],
        x[0] * h[1] - x[1] * h[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    vec![n[0] / l, n[1] / l, n[2] / l]
}

/// 3-D polyline with varying normals (`v76_beam3_*`).
fn polyline_3d() -> Mesh {
    let nodes = vec![
        vec![0., 0., 0.],
        vec![0.4, 0.3, 0.2],
        vec![0.75, 0.7, 0.3],
        vec![1.2, 0.9, 0.55],
        vec![1.5, 1.35, 0.6],
    ];
    let conn: Vec<Vec<Idx>> = (0..4).map(|i| vec![i, i + 1]).collect();
    let normals: Vec<Vec<Real>> = conn
        .iter()
        .enumerate()
        .map(|(e, c)| {
            let e = e as Real;
            let h = [(0.9 * e + 0.2).sin(), (1.7 * e).cos(), 0.6 + 0.1 * e];
            unit_normal(&nodes[c[0] as usize], &nodes[c[1] as usize], &h)
        })
        .collect();
    mesh_of(
        3,
        ElementType::BernoulliBeam3,
        &nodes,
        &conn,
        Some(&normals),
    )
}

fn pseudo(i: usize, j: usize, scale: Real) -> Real {
    scale * (0.7 * i as Real + 1.3 * j as Real + 0.25).sin()
}

fn set_velocity(m: &mut StructuralMechanicsModel) {
    let ndof = m.nb_degree_of_freedom();
    let mut v_g = m.velocity_mut().unwrap();
    let v = v_g.as_mut_slice();
    for (k, x) in v.iter_mut().enumerate() {
        *x = pseudo(k / ndof, k % ndof, 0.5);
    }
}

/// `½ vᵀ M v` from the dense consistent mass (independent of the model's
/// energy code; round-off only).
fn dense_kinetic_energy(m: &StructuralMechanicsModel) -> Real {
    let mm = dense(m, "M");
    let v = m.velocity().unwrap().as_slice().to_vec();
    let mv = mat_vec(&mm, &v);
    0.5 * v.iter().zip(&mv).map(|(a, b)| a * b).sum::<Real>()
}

/// UD-510: upstream creates the sparse `M` (and `C`) when the
/// integration scheme of an `_explicit_lumped_mass` solver is set, so
/// before the first lumped assembly `getKineticEnergy` takes the
/// **consistent** branch; the port must do the same (oracle values
/// 5.320198364029312e-2 / 1.666334946565129e-1 in
/// `fixtures/model/structural/v76_beam{2,3}_lumped_pre_energy`).
#[test]
fn st01_explicit_lumped_kinetic_energy_before_the_first_step_uses_the_consistent_mass() {
    for dim in [2, 3] {
        let make = |method| {
            let mesh = if dim == 2 { chain_2d() } else { polyline_3d() };
            model_of(
                mesh,
                beam_material(),
                method,
                StructuralModelUpstreamCompat::default(),
            )
        };
        let mut lumped = make(AnalysisMethod::ExplicitLumpedMass);
        let mut implicit = make(AnalysisMethod::ImplicitDynamic);
        assert!(lumped.dof_manager().has_matrix("M"), "{dim}-D: sparse M");
        assert!(lumped.dof_manager().has_matrix("C"), "{dim}-D: sparse C");
        assert!(!lumped.dof_manager().has_matrix("K"), "{dim}-D: no K");
        assert!(
            !lumped.dof_manager().has_lumped_matrix("M"),
            "{dim}-D: no lumped M after init_full"
        );
        set_velocity(&mut lumped);
        set_velocity(&mut implicit);
        let e_lumped = lumped.kinetic_energy().unwrap();
        let e_consistent = implicit.kinetic_energy().unwrap();
        assert_eq!(
            e_lumped.to_bits(),
            e_consistent.to_bits(),
            "{dim}-D: pre-step kinetic energy {e_lumped} must be the consistent one \
             {e_consistent}"
        );
        let e_dense = dense_kinetic_energy(&lumped);
        assert!(
            (e_lumped - e_dense).abs() <= 1e-13 * e_dense,
            "{dim}-D: {e_lumped} vs ½vᵀMv {e_dense}"
        );
        // the energy query does not create the lumped matrix (upstream:
        // has_lumped_M_after = false)
        assert!(!lumped.dof_manager().has_lumped_matrix("M"));
        // after the lumped assembly the lumped branch is taken
        lumped.assemble_lumped_mass_matrix().unwrap();
        let e_after = lumped.kinetic_energy().unwrap();
        // ½ (Σ m v² + ωᵀJω) with the nodal rotary blocks (D-059)
        let want = lumped_kinetic_energy_reference(&lumped);
        assert!(
            (e_after - want).abs() <= 1e-13 * want,
            "{dim}-D: lumped {e_after} vs {want}"
        );
        assert_ne!(e_after.to_bits(), e_lumped.to_bits());
    }
}

/// A zero lumped mass on a free DOF (`ρ = 0`, upstream's default)
/// is refused at the lumped assembly instead of producing `NaN`/`inf` in
/// the explicit update (`solve_lumped` divides by it); blocked DOFs may
/// have a zero mass.
#[test]
fn st09_zero_lumped_mass_on_a_free_dof_is_refused() {
    let massless = StructuralMaterial {
        rho: 0.0,
        ..beam_material()
    };
    for dim in [2, 3] {
        let mesh = if dim == 2 { chain_2d() } else { polyline_3d() };
        let mut m = model_of(
            mesh,
            massless,
            AnalysisMethod::ExplicitLumpedMass,
            StructuralModelUpstreamCompat::default(),
        );
        let ndof = m.nb_degree_of_freedom();
        m.external_force_mut().unwrap().as_mut_slice()[ndof] = 1.0;
        let err = m.solve_step(None).unwrap_err();
        assert!(
            matches!(err, akantu_core::Error::Numerical(ref s) if s.contains("lumped mass")),
            "{dim}-D: {err}"
        );
        assert!(
            !m.has_lumped_mass()
                || m.lumped_mass()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .all(|v| *v == 0.0)
        );
        assert!(
            m.displacement()
                .unwrap()
                .as_slice()
                .iter()
                .all(|v| v.is_finite())
        );
    }
    // a node without any beam: free → refused, blocked → accepted
    let nodes = vec![
        vec![0., 0.],
        vec![0.6, 0.35],
        vec![1.1, 0.9],
        vec![3.0, 3.0],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    let mesh = mesh_of(2, ElementType::BernoulliBeam2, &nodes, &conn, None);
    let mut m = model_of(
        mesh,
        beam_material(),
        AnalysisMethod::ExplicitLumpedMass,
        StructuralModelUpstreamCompat::default(),
    );
    let err = m.assemble_lumped_mass_matrix().unwrap_err();
    assert!(err.to_string().contains("node 3"), "{err}");
    m.blocked_dofs_mut().unwrap().as_mut_slice()[9..12].fill(true);
    m.assemble_lumped_mass_matrix().unwrap();
    // blocked DOFs may carry a zero mass: every DOF blocked, ρ = 0
    let mut m = model_of(
        chain_2d(),
        massless,
        AnalysisMethod::ExplicitLumpedMass,
        StructuralModelUpstreamCompat::default(),
    );
    m.blocked_dofs_mut().unwrap().as_mut_slice().fill(true);
    m.assemble_lumped_mass_matrix().unwrap();
    assert!(
        m.lumped_mass()
            .unwrap()
            .as_slice()
            .iter()
            .all(|v| *v == 0.0)
    );
}

// ----: extra_normal edits ---------------------------------------

/// Handler of a data-only mesh mutation (no event is recorded).
#[derive(Debug)]
struct DataOnly;

impl MeshEventHandler for DataOnly {
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }
    fn event_priorities(&self) -> Vec<EventPriority> {
        vec![
            EventPriority::FE_ENGINE,
            EventPriority::DOF_MANAGER,
            EventPriority::MODEL,
        ]
    }
    fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        Ok(())
    }
    /// A data-only mutation records no event: refuses any before the commit,
    /// as the model's own data-only handler does.
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        Err(Error::invalid(
            "a data-only mesh mutation of the structural model recorded a topology event",
        ))
    }
}

fn cantilever_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 2e5,
        a: 0.01,
        iz: 3e-6,
        iy: 1e-6,
        gj: 50.,
        rho: 7.8,
        ..StructuralMaterial::default()
    }
}

/// Cantilever along x (3 elements, L = 1.5), normal `normal`, clamped at
/// node 0, tip forces 2 (y) and 1 (z).
fn cantilever(
    method: AnalysisMethod,
    normal: [Real; 3],
    compat: StructuralModelUpstreamCompat,
) -> StructuralMechanicsModel {
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        3,
        1.5,
        &[0., 0., 0.],
        &[1., 0., 0.],
        Some(&normal),
    );
    let mut m = model_of(mesh, cantilever_material(), method, compat);
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..6].fill(true);
    {
        let mut f_guard = m.external_force_mut().unwrap();
        let f = f_guard.as_mut_slice();
        f[3 * 6 + 1] = 2.0;
        f[3 * 6 + 2] = 1.0;
    }
    m
}

/// Rewrite the normals through a data-only mutation (no event: what a
/// zero-copy view of the mesh data or a foreign writer does).
fn edit_normals_data_only(m: &mut StructuralMechanicsModel, normal: [Real; 3]) {
    let mesh = m.physics.base.mesh.clone();
    mesh.mutate(
        &mut [(&m.token, &mut DataOnly as &mut dyn MeshEventHandler)],
        |tx| {
            let map = tx
                .data_mut(MeshPart::Main)?
                .reals_mut_or_insert(EXTRA_NORMAL)?;
            let a = map
                .as_map_mut()
                .get_mut(ElementType::BernoulliBeam3, G)
                .expect("normals");
            let mut a = a.borrow_mut();
            for e in 0..a.len() {
                a.tuple_mut(e).copy_from_slice(&normal);
            }
            Ok(())
        },
        None,
    )
    .unwrap();
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

fn is_stale<T>(r: &Result<T>) -> bool {
    matches!(r, Err(Error::InvalidArgument(s)) if s.contains("precomputed at mesh revision"))
}

/// After a data-only `extra_normal` edit (no event) every operation
/// that would reuse matrices assembled with the old normals refuses
/// (`potential_energy` used to return the old energy bit for bit); after
/// `refresh_geometry` the model equals a fresh model with the new normals.
#[test]
fn st03_data_only_normal_edit_never_reuses_stale_matrices() {
    let compat = StructuralModelUpstreamCompat::default();
    let mut m = cantilever(AnalysisMethod::Static, [0., 0., 1.], compat);
    m.solve_step(None).unwrap();
    let e0 = m.potential_energy().unwrap();
    edit_normals_data_only(&mut m, [0., 1., 0.]);
    assert!(m.physics.base.mesh.check_current(&m.token).is_ok());
    let ep = m.potential_energy();
    assert!(is_stale(&ep), "potential_energy: {ep:?} (old {e0})");
    assert!(is_stale(&m.assemble_stiffness_matrix()));
    assert!(is_stale(&m.assemble_matrix("K")));
    assert!(is_stale(
        &m.rotations(ElementType::BernoulliBeam3, G).map(|_| ())
    ));
    assert!(is_stale(&m.solve_step(None)));
    m.refresh_geometry().unwrap();
    // solve from rest again (a linear solve from a non-zero start is not
    // bitwise the solve from rest)
    m.displacement_mut().unwrap().as_mut_slice().fill(0.0);
    m.solve_step(None).unwrap();
    let e1 = m.potential_energy().unwrap();
    let mut fresh = cantilever(AnalysisMethod::Static, [0., 1., 0.], compat);
    fresh.solve_step(None).unwrap();
    let ef = fresh.potential_energy().unwrap();
    assert_eq!(e1.to_bits(), ef.to_bits(), "{e1} vs fresh {ef}");
    assert_ne!(e1.to_bits(), e0.to_bits());

    // masses: consistent (kinetic energy) and lumped
    for method in [
        AnalysisMethod::ImplicitDynamic,
        AnalysisMethod::ExplicitLumpedMass,
    ] {
        let mut m = cantilever(method, [0., 0., 1.], compat);
        m.velocity_mut().unwrap().as_mut_slice().fill(0.1);
        if method == AnalysisMethod::ExplicitLumpedMass {
            m.assemble_lumped_mass_matrix().unwrap();
        }
        m.assemble_mass_matrix().unwrap();
        m.kinetic_energy().unwrap();
        edit_normals_data_only(&mut m, [0., 1., 0.]);
        assert!(is_stale(&m.assemble_mass_matrix()), "{method:?}: M");
        assert!(is_stale(&m.kinetic_energy()), "{method:?}: kinetic energy");
        if method == AnalysisMethod::ExplicitLumpedMass {
            assert!(is_stale(&m.assemble_lumped_mass_matrix()), "lumped M");
        }
        m.refresh_geometry().unwrap();
        m.kinetic_energy().unwrap();
    }
}

/// `set_extra_normals` validates every new normal before the mesh
/// is touched; a refused normal never stays in the mesh and the model stays
/// usable. Default: non-unit and skew normals are refused (UD-343; the
/// non-unit `(0,0,2)` gave a 4× too large deflection); `UPSTREAM` /
/// `beam3_normal_unchecked`: accepted raw, as upstream.
#[test]
fn st04_set_extra_normals_is_transactional_and_validates_by_default() {
    let nonunit = [0., 0., 2.].repeat(3);
    let skew = [0.6, 0., 0.8].repeat(3);
    let parallel = [1., 0., 0.].repeat(3);
    let (p_y, l) = (1.0, 1.5);
    let e_iy = cantilever_material().e * cantilever_material().iy;
    let w_exact = p_y * l * l * l / (3. * e_iy);
    for (label, compat) in [
        ("default", StructuralModelUpstreamCompat::default()),
        ("UPSTREAM", StructuralModelUpstreamCompat::UPSTREAM),
    ] {
        let mut m = cantilever(AnalysisMethod::Static, [0., 0., 1.], compat);
        m.solve_step(None).unwrap();
        let before = stored_normals(&m);
        let u_ok = m.displacement().unwrap().as_slice().to_vec();
        // zero / parallel normals are refused in every mode (UD-296)
        let err = m
            .set_extra_normals(ElementType::BernoulliBeam3, &parallel)
            .unwrap_err();
        assert!(matches!(err, Error::Numerical(_)), "{label}: {err}");
        assert_eq!(stored_normals(&m), before, "{label}: mesh unchanged");
        m.displacement_mut().unwrap().as_mut_slice().fill(0.0);
        m.solve_step(None).unwrap();
        assert_eq!(m.displacement().unwrap().as_slice(), &u_ok[..]);
        for (what, bad) in [("non-unit", &nonunit), ("skew", &skew)] {
            let r = m.set_extra_normals(ElementType::BernoulliBeam3, bad);
            if label == "default" {
                assert!(
                    matches!(r, Err(Error::InvalidArgument(ref s)) if s.contains("UD-343")),
                    "{what}: {r:?}"
                );
                assert_eq!(stored_normals(&m), before, "{what}: mesh unchanged");
                m.displacement_mut().unwrap().as_mut_slice().fill(0.0);
                m.solve_step(None).unwrap();
                assert_eq!(m.displacement().unwrap().as_slice(), &u_ok[..]);
            } else {
                r.unwrap();
                assert_eq!(stored_normals(&m), bad.to_vec());
                // restore for the next variant
                m.set_extra_normals(ElementType::BernoulliBeam3, &before)
                    .unwrap();
            }
        }
        // tip deflection under the z force alone with the non-unit normal
        let mut m = cantilever(AnalysisMethod::Static, [0., 0., 1.], compat);
        {
            let mut f_guard = m.external_force_mut().unwrap();
            let f = f_guard.as_mut_slice();
            f.fill(0.0);
            f[3 * 6 + 2] = p_y;
        }
        if label == "default" {
            m.solve_step(None).unwrap();
            let w = m.displacement().unwrap().as_slice()[3 * 6 + 2];
            assert!((w - w_exact).abs() <= 1e-10 * w_exact, "{w} vs {w_exact}");
            assert!(
                m.set_extra_normals(ElementType::BernoulliBeam3, &nonunit)
                    .is_err()
            );
        } else {
            m.set_extra_normals(ElementType::BernoulliBeam3, &nonunit)
                .unwrap();
            m.solve_step(None).unwrap();
            let w = m.displacement().unwrap().as_slice()[3 * 6 + 2];
            // upstream's non-orthogonal frame: 4× the exact deflection
            assert!(
                (w - 4. * w_exact).abs() <= 1e-10 * w_exact,
                "{w} vs 4 × {w_exact}"
            );
        }
    }
    // only the UD-343 switch: accepted as well
    let mut compat = StructuralModelUpstreamCompat::default();
    compat.fe.beam3_normal_unchecked = true;
    let mut m = cantilever(AnalysisMethod::Static, [0., 0., 1.], compat);
    m.set_extra_normals(ElementType::BernoulliBeam3, &skew)
        .unwrap();
    // an inadmissible normal given to the constructor is refused by default
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        3,
        1.5,
        &[0., 0., 0.],
        &[1., 0., 0.],
        Some(&[0., 0., 2.]),
    );
    let mut m = StructuralMechanicsModel::new(shared(mesh), None, "bad").unwrap();
    m.add_material(cantilever_material(), "").unwrap();
    let err = m.init_full_with(AnalysisMethod::Static).unwrap_err();
    assert!(err.to_string().contains("UD-343"), "{err}");
}

// ---- (c): serial only ------------------------------------------

/// A distributed mesh and a communicator with more than one rank
/// are refused at construction (the model is serial).
#[test]
fn st05_serial_only_refusals() {
    let mut mesh = chain_2d();
    MeshAccessor::new(&mut mesh).set_distributed();
    let err = StructuralMechanicsModel::new(shared(mesh), None, "dist").unwrap_err();
    assert!(
        matches!(err, Error::Unsupported(ref s) if s.contains("distributed")),
        "{err}"
    );
    let out = akantu_comm::run_local(2, std::time::Duration::from_secs(20), |comm| {
        let mut mesh = Mesh::with_communicator(2, "two_ranks", comm).unwrap();
        {
            let mut acc = MeshAccessor::new(&mut mesh);
            acc.push_node(&[0., 0.]).unwrap();
            acc.push_node(&[1., 0.]).unwrap();
            acc.connectivity_mut(ElementType::BernoulliBeam2, G)
                .push_tuple(&[0, 1]);
        }
        StructuralMechanicsModel::new(shared(mesh), None, "ranks")
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    for r in out {
        let e = r.unwrap_err();
        assert!(e.contains("serial") && e.contains("2 rank"), "{e}");
    }
}

/// A `MeshIsDistributed` event is refused explicitly (the model is
/// serial), like the topology events.
#[test]
fn st08_mesh_is_distributed_event_is_refused() {
    let mut m = model_of(
        chain_2d(),
        beam_material(),
        AnalysisMethod::Static,
        StructuralModelUpstreamCompat::default(),
    );
    let mesh_h = m.physics.base.mesh.clone();
    let mesh = mesh_h.read().unwrap();
    let ev = MeshEvent::MeshIsDistributed {
        origin: "test".into(),
    };
    let err = m
        .on_mesh_event_at(&mesh, &ev, EventPriority::FE_ENGINE)
        .unwrap_err();
    assert!(
        matches!(err, Error::Unsupported(ref s) if s.contains("serial")),
        "{err}"
    );
}

// ----: material changes by writers other than the &mut accessors ------

/// UD-508: materials and the element → material map changed by a
/// writer that does not go through the invalidating `&mut` accessors (the
/// Python layer's zero-copy views, D-047, emulated by the raw binding
/// accessors) are detected at the next assembly by the content check: `K`,
/// `M` and the lumped mass are reassembled, never reused stale.
#[test]
fn st07_material_changes_by_other_writers_trigger_reassembly() {
    let soft = StructuralMaterial {
        e: 1e5,
        rho: 3.9,
        ..beam_material()
    };
    for method in [
        AnalysisMethod::Static,
        AnalysisMethod::ImplicitDynamic,
        AnalysisMethod::ExplicitLumpedMass,
    ] {
        let fresh = |second_material: bool| {
            let mut m = model_of(
                chain_2d(),
                beam_material(),
                method,
                StructuralModelUpstreamCompat::default(),
            );
            m.add_material(soft, "soft").unwrap();
            if second_material {
                m.element_material_mut(ElementType::BernoulliBeam2, G)
                    .unwrap()
                    .as_mut_slice()[3] = 1;
            }
            m.blocked_dofs_mut().unwrap().as_mut_slice()[..3].fill(true);
            m.external_force_mut().unwrap().as_mut_slice()[13] = -0.5;
            m.velocity_mut()
                .map(|mut v| v.as_mut_slice().fill(0.2))
                .ok();
            m
        };
        let energies = |m: &mut StructuralMechanicsModel| {
            if method == AnalysisMethod::ExplicitLumpedMass {
                m.assemble_lumped_mass_matrix().unwrap();
            }
            (
                m.potential_energy().unwrap(),
                m.kinetic_energy().unwrap(),
                if method == AnalysisMethod::Static {
                    Vec::new()
                } else {
                    m.assemble_mass_matrix().unwrap();
                    dense(m, "M").concat()
                },
            )
        };
        // (a) element → material map written behind the model's back
        let mut m = fresh(false);
        m.displacement_mut().unwrap().as_mut_slice()[13] = 1e-3;
        let before = energies(&mut m);
        m.element_material_map_mut()
            .get_mut(ElementType::BernoulliBeam2, G)
            .unwrap()
            .as_mut_slice()[3] = 1;
        let after = energies(&mut m);
        let mut want = fresh(true);
        want.displacement_mut().unwrap().as_mut_slice()[13] = 1e-3;
        let want = energies(&mut want);
        assert_eq!(
            after.0.to_bits(),
            want.0.to_bits(),
            "{method:?}: K after map write"
        );
        assert_ne!(after.0.to_bits(), before.0.to_bits());
        assert_eq!(
            after.1.to_bits(),
            want.1.to_bits(),
            "{method:?}: kinetic energy"
        );
        assert_eq!(after.2, want.2, "{method:?}: M");
        // (b) a material parameter written behind the model's back
        let mut m = fresh(true);
        m.displacement_mut().unwrap().as_mut_slice()[13] = 1e-3;
        let before = energies(&mut m);
        m.materials_mut()[1].e *= 0.5;
        m.materials_mut()[1].rho *= 2.0;
        let after = energies(&mut m);
        assert_ne!(
            after.0.to_bits(),
            before.0.to_bits(),
            "{method:?}: K after E write"
        );
        if method != AnalysisMethod::Static {
            assert_ne!(after.1.to_bits(), before.1.to_bits(), "{method:?}: E_kin");
        }
        // unchanged inputs: nothing is reassembled (same values)
        let again = energies(&mut m);
        assert_eq!(again.0.to_bits(), after.0.to_bits());
    }
}
