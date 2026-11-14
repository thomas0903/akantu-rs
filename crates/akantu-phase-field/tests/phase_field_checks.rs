// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent
// mathematical checks of the phase-field model (no oracle): the stiffness is
// the exact Jacobian of the (linear) residual, row sums, closed-form
// dissipated energies of polynomial damage fields, analytical damage in 1-D
// and 3-D with both energy splits, history, element filter, energies per
// element / group; regressions of the upstream defects UD-377, UD-378,
// UD-380 (corrected default vs compat switch) and the error paths.

mod common;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_parser::Parser;
use akantu_phase_field::selector::MeshDataPhaseFieldSelector;
use akantu_phase_field::{
    EnergySplit, EnergySplitKind, PhaseFieldElementFilter, PhaseFieldModel,
    PhaseFieldUpstreamCompat,
};
use akantu_validation::reduce::NanMinMax;
use common::*;

const AT2: &str =
    "phasefield quadratic [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.2\n]\n";

/// `nx × ny` bilinear quadrangles on `[0, lx] × [0, ly]` (counter-clockwise).
fn quad_mesh(nx: usize, ny: usize, lx: Real, ly: Real) -> Mesh {
    let mut mesh = Mesh::new(2, "quads").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for j in 0..=ny {
            for i in 0..=nx {
                a.push_node(&[lx * i as Real / nx as Real, ly * j as Real / ny as Real])
                    .unwrap();
            }
        }
        let mut c = a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
        let id = |i: usize, j: usize| (i + j * (nx + 1)) as Idx;
        for j in 0..ny {
            for i in 0..nx {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

/// Distorted triangle mesh of the unit square (`n × n` cells, two
/// triangles each, interior nodes shifted).
fn tri_mesh(n: usize) -> Mesh {
    let mut mesh = Mesh::new(2, "tris").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for j in 0..=n {
            for i in 0..=n {
                let (x, y) = (i as Real / n as Real, j as Real / n as Real);
                let interior = i > 0 && i < n && j > 0 && j < n;
                let s = if interior {
                    0.07 * ((i * 3 + j * 5) % 7) as Real / 7.
                } else {
                    0.
                };
                a.push_node(&[x + s / n as Real, y - 0.5 * s / n as Real])
                    .unwrap();
            }
        }
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        let id = |i: usize, j: usize| (i + j * (n + 1)) as Idx;
        for j in 0..n {
            for i in 0..n {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                c.push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

/// One unit hexahedron.
fn hex_mesh() -> Mesh {
    let mut mesh = Mesh::new(3, "hex").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for p in [
            [0., 0., 0.],
            [1., 0., 0.],
            [1., 1., 0.],
            [0., 1., 0.],
            [0., 0., 1.],
            [1., 0., 1.],
            [1., 1., 1.],
            [0., 1., 1.],
        ] {
            a.push_node(&p).unwrap();
        }
        a.connectivity_mut(ElementType::Hexahedron8, GhostType::NotGhost)
            .push_tuple(&[0, 1, 2, 3, 4, 5, 6, 7]);
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

/// Segments with nodes at `xs`.
fn segment_mesh(xs: &[Real]) -> Mesh {
    let mut mesh = Mesh::new(1, "segments").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for &x in xs {
            a.push_node(&[x]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        for i in 0..xs.len() - 1 {
            c.push_tuple(&[i as Idx, (i + 1) as Idx]);
        }
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

/// Strain field `ε(x)` written at the quadrature points of every law.
fn set_strain_field(model: &mut PhaseFieldModel, f: &dyn Fn(&[Real]) -> Vec<Real>) {
    let dim = model.spatial_dimension();
    let mesh_h = model.mesh_handle().clone();
    let mesh = mesh_h.read().unwrap();
    for l in 0..model.nb_phase_fields() {
        let types = model
            .phase_field(l)
            .unwrap()
            .base()
            .filter_types(GhostType::NotGhost);
        for ty in types {
            let filter = model
                .phase_field(l)
                .unwrap()
                .base()
                .filter(ty, GhostType::NotGhost)
                .to_vec();
            let mut coords = Array::new(dim);
            model
                .fe_engine()
                .unwrap()
                .integration_points_coordinates(
                    &mesh,
                    &mut coords,
                    ty,
                    GhostType::NotGhost,
                    Some(&filter),
                )
                .unwrap();
            let law = model.phase_field_mut(l).unwrap();
            let mut s = law
                .base_mut()
                .strain
                .get_mut(ty, GhostType::NotGhost)
                .unwrap();
            for (eps, x) in s
                .as_mut_slice()
                .chunks_exact_mut(dim * dim)
                .zip(coords.as_slice().chunks_exact(dim))
            {
                eps.copy_from_slice(&f(x));
            }
        }
    }
}

/// `K·v` from the COO storage of `K` (symmetric storage: one triangle).
fn k_times(model: &PhaseFieldModel, v: &[Real]) -> Vec<Real> {
    let k = model.dof_manager().matrix("K").unwrap();
    let (irn, jcn, a) = k.coo();
    let sym = k.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
    let mut y = vec![0.0; v.len()];
    for ((&i, &j), &aij) in irn.iter().zip(jcn).zip(a) {
        let (i, j) = (i as usize, j as usize);
        y[i] += aij * v[j];
        if sym && i != j {
            y[j] += aij * v[i];
        }
    }
    y
}

/// Residual `f_ext + f_int` at the current damage, per node.
fn residual(model: &mut PhaseFieldModel) -> Vec<Real> {
    model.dof_manager_mut().zero_residual();
    model.assemble_residual().unwrap();
    let eq: Vec<usize> = model
        .dof_manager()
        .local_equation_numbers("damage")
        .unwrap()
        .iter()
        .map(|&e| e as usize)
        .collect();
    let r = &model.dof_manager().residual().values();
    eq.iter().map(|&e| r[e]).collect()
}

fn set_damage(model: &mut PhaseFieldModel, f: &dyn Fn(&[Real]) -> Real) {
    let dim = model.spatial_dimension();
    let mesh_h = model.mesh_handle().clone();
    let mesh = mesh_h.read().unwrap();
    let x = mesh.nodes().as_slice().to_vec();
    let mut d_g = model.damage_mut().unwrap();
    let d = d_g.as_mut_slice();
    for (n, dn) in d.iter_mut().enumerate() {
        *dn = f(&x[n * dim..(n + 1) * dim]);
    }
}

/// The residual is linear in the damage and `K = −∂r/∂d` exactly (AT2 and
/// AT1, both splits, history): `r(d) = r(0) − K d` for an arbitrary damage
/// field, on a distorted triangle mesh with a non-uniform strain.
#[test]
fn stiffness_is_the_exact_jacobian_of_the_residual() {
    for (law, split) in [
        ("quadratic", "no_split"),
        ("quadratic", "volumetric_deviatoric"),
        ("linear", "no_split"),
        ("linear", "volumetric_deviatoric"),
    ] {
        let dat = format!(
            "phasefield {law} {split} [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.2\n]\n"
        );
        let mut model = model_with(tri_mesh(4), &dat);
        init_static(&mut model);
        set_strain_field(&mut model, &|x| {
            vec![
                1e-2 * (1. + x[0]),
                3e-3 * x[1],
                3e-3 * x[1],
                -4e-3 + 2e-2 * x[0] * x[1],
            ]
        });
        set_damage(&mut model, &|_| 0.0);
        let r0 = residual(&mut model);
        let d: Vec<Real> = (0..r0.len())
            .map(|n| 0.5 + 0.4 * ((n * 7919 % 101) as Real / 101. - 0.5))
            .collect();
        model
            .damage_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&d);
        let rd = residual(&mut model);
        model.assemble_stiffness_matrix().unwrap();
        let kd = k_times(&model, &d);
        let scale = kd.iter().fold(0.0f64, |m, v| m.nan_max(v.abs()));
        for n in 0..d.len() {
            let want = r0[n] - kd[n];
            assert!(
                (rd[n] - want).abs() <= 1e-12 * scale,
                "{law} {split}: node {n}: r(d) = {} vs r(0) − Kd = {want}",
                rd[n]
            );
        }
    }
}

/// `Σ_ij K_ij = ∫ (2φ + g_c/l0)` for a uniform strain (`B·1 = 0`,
/// `Σ_i N_i = 1`) over the area of the mesh.
#[test]
fn stiffness_sums_to_the_integrated_density() {
    let mut model = model_with(quad_mesh(3, 2, 1.5, 1.0), AT2);
    init_static(&mut model);
    set_uniform_strain(&mut model, &uniaxial(2, 2e-3));
    model.compute_driving_forces().unwrap();
    model.assemble_stiffness_matrix().unwrap();
    let ones = vec![1.0; model.damage().unwrap().len()];
    let total: Real = k_times(&model, &ones).iter().sum();
    let s = EnergySplit::new(EnergySplitKind::NoSplit, 210., 0.3, false);
    let phi = 0.5 * (s.lambda + 2. * s.mu) * 4e-6;
    let want = (2. * phi + 5e-3 / 0.2) * 1.5;
    assert!((total - want).abs() <= 1e-13 * want, "{total} vs {want}");
}

/// Dissipated energy of an affine damage `d = a + b x + c y` on
/// parallelogram quadrangles (2×2 Gauss is exact): AT2
/// `g_c/(2 l0) ∫ d² + ½ g_c l0 (b² + c²) A`, AT1
/// `3g_c/(8 l0) ∫ d + ⅜ g_c l0 (b² + c²) A`; also per element and per
/// group.
#[test]
fn dissipated_energy_of_affine_damage_is_exact() {
    let (a, b, c) = (0.2, 0.15, -0.1);
    let (lx, ly): (Real, Real) = (2.0, 1.0);
    // ∫ d over [0,lx]×[0,ly], ∫ d²
    let int_d = a * lx * ly + b * lx * lx / 2. * ly + c * ly * ly / 2. * lx;
    let int_d2 = a * a * lx * ly
        + b * b * lx.powi(3) / 3. * ly
        + c * c * ly.powi(3) / 3. * lx
        + 2. * a * b * lx * lx / 2. * ly
        + 2. * a * c * ly * ly / 2. * lx
        + 2. * b * c * (lx * lx / 2.) * (ly * ly / 2.);
    let (gc, l0) = (5e-3, 0.2);
    for (law, want) in [
        (
            "quadratic",
            gc / (2. * l0) * int_d2 + 0.5 * gc * l0 * (b * b + c * c) * lx * ly,
        ),
        (
            "linear",
            3. * gc / (8. * l0) * int_d + 3. / 8. * gc * l0 * (b * b + c * c) * lx * ly,
        ),
    ] {
        let mut mesh = quad_mesh(4, 3, lx, ly);
        let elements: Vec<Element> = (0..12)
            .map(|e| Element::new(ElementType::Quadrangle4, e, GhostType::NotGhost))
            .collect();
        {
            let mut g = mesh.groups_mut().create_element_group("all", 2).unwrap();
            for &e in &elements {
                g.add(e, None, false).unwrap();
            }
        }
        let dat = format!(
            "phasefield {law} [\n name = body\n E = 210\n nu = 0.3\n gc = {gc}\n l0 = {l0}\n]\n"
        );
        let mut model = model_with(mesh, &dat);
        init_static(&mut model);
        set_damage(&mut model, &|x| a + b * x[0] + c * x[1]);
        let e = model.energy("dissipated").unwrap();
        assert!((e - want).abs() <= 1e-14 * want, "{law}: {e} vs {want}");
        let by_element: Real = elements
            .iter()
            .map(|&el| model.energy_of_element("dissipated", el).unwrap())
            .sum();
        assert!(
            (by_element - want).abs() <= 1e-14 * want,
            "{law}: Σ elements {by_element}"
        );
        let group = model.energy_of_group("dissipated", "all").unwrap();
        assert_eq!(group, by_element, "{law}: group = Σ elements (same order)");
        assert_eq!(
            model.energy("potential").unwrap(),
            0.0,
            "unknown energy id → 0"
        );
    }
}

/// 3-D volumetric–deviatoric split (one hexahedron, uniform strain,
/// history): tension degrades with `½κ tr² + μ dev:dev`, compression only
/// with `μ dev:dev`; the damage of the point problem is
/// `d = 2φ⁺_max / (2φ⁺_max + g_c/l0)`.
#[test]
fn three_d_volumetric_deviatoric_history() {
    let dat = "phasefield quadratic volumetric_deviatoric [\n name = body\n E = 100\n nu = 0.25\n gc = 1e-2\n l0 = 0.5\n]\n";
    let mut model = model_with(hex_mesh(), dat);
    init_static(&mut model);
    let s = EnergySplit::new(EnergySplitKind::VolumetricDeviatoric, 100., 0.25, false);
    let mut phi_max: Real = 0.;
    for (k, t) in [2e-3, 5e-3, 1e-3, -6e-3, -1.2e-2, 3e-3]
        .into_iter()
        .enumerate()
    {
        // ε = t·diag(1, −0.2, 0.5) + shear
        let eps = [
            t,
            0.3 * t,
            0.,
            0.3 * t,
            -0.2 * t,
            0.1 * t,
            0.,
            0.1 * t,
            0.5 * t,
        ];
        set_uniform_strain(&mut model, &eps);
        model.solve_step(None).unwrap();
        model.save_previous_state().unwrap();
        let m: [[Real; 3]; 3] = [
            [eps[0], eps[1], eps[2]],
            [eps[3], eps[4], eps[5]],
            [eps[6], eps[7], eps[8]],
        ];
        phi_max = phi_max.nan_max(s.phi(&m));
        let want = 2. * phi_max / (2. * phi_max + 1e-2 / 0.5);
        for &d in model.damage().unwrap().as_slice() {
            assert!(rel(d, want) <= 1e-12, "step {k}: {d} vs {want}");
        }
    }
}

/// 1-D AT1 bar (history, uniform strain): `d = max(0, 1 − 3g_c/(16 l0 φ))`
/// with `φ = (½λ + μ) ε²`.
#[test]
fn one_d_linear_bar() {
    let dat = "phasefield linear [\n name = body\n E = 10\n nu = 0.2\n gc = 1e-3\n l0 = 0.25\n]\n";
    let mut model = model_with(segment_mesh(&[0., 0.3, 0.7, 1.2, 2.0]), dat);
    init_static(&mut model);
    let s = EnergySplit::new(EnergySplitKind::NoSplit, 10., 0.2, false);
    for k in 0..20 {
        let eps = 1e-3 * k as Real;
        set_uniform_strain(&mut model, &[eps]);
        model.solve_step(None).unwrap();
        let phi = (0.5 * s.lambda + s.mu) * eps * eps;
        let want = (1. - 3. * 1e-3 / (16. * 0.25 * phi)).max(0.);
        for &d in model.damage().unwrap().as_slice() {
            assert!((d - want).abs() <= 1e-12, "step {k}: {d} vs {want}");
        }
    }
}

/// History (AT2): after unloading, `φ = φ_prev` keeps the damage of the
/// maximum load; without `savePreviousState` the damage follows the load.
#[test]
fn history_keeps_the_damage_of_the_maximum_load() {
    let mut model = model_with(quad_mesh(2, 1, 1.0, 0.5), AT2);
    init_static(&mut model);
    set_uniform_strain(&mut model, &uniaxial(2, 4e-3));
    model.solve_step(None).unwrap();
    let d_max = model.damage().unwrap().as_slice()[0];
    set_uniform_strain(&mut model, &uniaxial(2, 1e-3));
    model.solve_step(None).unwrap();
    let d_low = model.damage().unwrap().as_slice()[0];
    assert!(d_low < d_max, "no save: reversible");
    set_uniform_strain(&mut model, &uniaxial(2, 4e-3));
    model.solve_step(None).unwrap();
    model.save_previous_state().unwrap();
    set_uniform_strain(&mut model, &uniaxial(2, 1e-3));
    model.solve_step(None).unwrap();
    for &d in model.damage().unwrap().as_slice() {
        assert!(rel(d, d_max) <= 1e-12, "{d} vs {d_max}");
    }
}

/// Dirichlet condition on the damage (FixedValue on a group) and the
/// clustering filter: elements whose points all reach `max_damage` are
/// broken.
#[test]
fn dirichlet_bc_and_element_filter() {
    let mut mesh = quad_mesh(4, 1, 4.0, 1.0);
    {
        let mut g = mesh.groups_mut().create_element_group("left", 1).unwrap();
        g.add_node(0, false);
        g.add_node(5, false);
    }
    // h/l0 = 0.5: the consistent N^T N term does not make the coupling
    // positive (h/l0 > √6 gives oscillating discrete profiles)
    let dat = "phasefield quadratic [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 2\n]\n";
    let mut model = model_with(mesh, dat);
    init_static(&mut model);
    model
        .apply_dirichlet_bc(
            &FixedValue {
                value: 1.0,
                axis: SpatialDirection::X,
            },
            "left",
        )
        .unwrap();
    model.solve_step(None).unwrap();
    let d = model.damage().unwrap().as_slice().to_vec();
    assert_eq!(d[0], 1.0);
    assert_eq!(d[5], 1.0);
    assert!(model.blocked_dofs().unwrap().as_slice()[0]);
    // monotone decay away from the crack
    assert!(d[1] < 1.0 && d[2] < d[1] && d[3] < d[2] && d[4] < d[3]);
    model.compute_driving_forces().unwrap();
    let filter = PhaseFieldElementFilter::default();
    let e0 = Element::new(ElementType::Quadrangle4, 0, GhostType::NotGhost);
    assert!(filter.is_unbroken(&model, e0).unwrap(), "points below 1");
    assert!(
        !PhaseFieldElementFilter::new(0.0)
            .is_unbroken(&model, e0)
            .unwrap()
    );
    let unbroken = PhaseFieldElementFilter::new(d[1])
        .unbroken_elements(&model)
        .unwrap();
    assert!(!unbroken.contains(&e0) && unbroken.len() == 3);
}

fn data_mesh(name: &str, dim: usize) -> Mesh {
    read_mesh(
        &std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/wp_pf")
            .join(name),
        dim,
    )
}

const MULTI: &str = "phasefield quadratic no_split [\n  name = chocolate\n  E = 2\n  gc = 0.02\n  l0 = 0.5\n]\nphasefield quadratic no_split [\n  name = chewing-gum\n  E = 1\n  gc = 0.01\n  l0 = 0.7\n]\nphasefield quadratic no_split [\n  name = candy\n  E = 3\n  gc = 0.03\n  l0 = 0.9\n]\n";

fn multi_law_model(compat: PhaseFieldUpstreamCompat) -> PhaseFieldModel {
    let mut model = model_with(data_mesh("multi_law_segments.msh", 1), MULTI);
    model.set_upstream_compat(compat);
    model.set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    init_static(&mut model);
    model.set_upstream_compat(compat);
    model
}

/// UD-377: upstream interpolates the nodal damage on every element of the
/// type into each law's `damage` internal and reads it positionally, so
/// law `k` (element `k`) uses element 0's damage; the port interpolates on
/// the law's own elements. UD-380: the per-element energy integrates with
/// the law-local index (element 0's length) upstream.
#[test]
fn ud377_ud380_multi_law_damage_and_element_energy() {
    let seg = ElementType::Segment2;
    for compat in [
        PhaseFieldUpstreamCompat::default(),
        PhaseFieldUpstreamCompat::upstream(),
    ] {
        let mut model = multi_law_model(compat);
        // nodes at x = 0, 1, 2.5, 4.5: d = x / 4.5
        set_damage(&mut model, &|x| x[0] / 4.5);
        model.compute_driving_forces().unwrap();
        let centre = |e: usize| [0.5, 1.75, 3.5][e] / 4.5;
        for law in 0..3 {
            let dq = law_damage(&model, law, seg);
            if compat.unfiltered_damage_interpolation {
                assert_eq!(dq.len(), 3, "upstream resizes to the mesh block");
                assert_eq!(dq[0], centre(0), "law {law} reads element 0");
            } else {
                assert_eq!(dq.len(), 1);
                assert_eq!(dq[0], centre(law), "law {law} reads its element");
            }
        }
        // per-element energy of element 2 (law candy, local index 0)
        let e2 = Element::new(seg, 2, GhostType::NotGhost);
        let energy = model.energy_of_element("dissipated", e2).unwrap();
        let (gc, l0) = (0.03, 0.9);
        let grad: Real = 1. / 4.5;
        let d_used = if compat.unfiltered_damage_interpolation {
            centre(0)
        } else {
            centre(2)
        };
        let density = 0.5 * gc * l0 * grad * grad + gc * d_used * d_used / (2. * l0);
        let length = if compat.element_energy_local_index_as_mesh_index {
            1.0
        } else {
            2.0
        };
        assert!(
            rel(energy, density * length) <= 1e-14,
            "compat {compat:?}: {energy} vs {}",
            density * length
        );
    }
}

/// UD-378: law `quad` has no triangle; upstream's stiffness loop returns at
/// its empty triangle filter, so the quadrangle never reaches `K` (its
/// private nodes get empty rows and the solve fails); the port skips the
/// empty type.
#[test]
fn ud378_mixed_types_stiffness() {
    let dat = "phasefield quadratic no_split [\n  name = tri\n  E = 210\n  nu = 0.3\n  gc = 5e-3\n  l0 = 0.3\n]\nphasefield quadratic no_split [\n  name = quad\n  E = 100\n  nu = 0.2\n  gc = 4e-3\n  l0 = 0.4\n]\n";
    for compat in [
        PhaseFieldUpstreamCompat::default(),
        PhaseFieldUpstreamCompat::upstream(),
    ] {
        let mut model = model_with(data_mesh("mixed_types.msh", 2), dat);
        model.set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
        init_static(&mut model);
        model.set_upstream_compat(compat);
        set_uniform_strain(&mut model, &[1e-3, 5e-4, 5e-4, 2e-3]);
        model.compute_driving_forces().unwrap();
        model.assemble_stiffness_matrix().unwrap();
        let k = model.dof_manager().matrix("K").unwrap();
        let (irn, jcn, a) = k.coo();
        // node 4 (x = 2.5) belongs to the quadrangle only
        let diag4: Real = irn
            .iter()
            .zip(jcn)
            .zip(a)
            .filter(|((i, j), _)| **i == 4 && **j == 4)
            .map(|(_, v)| *v)
            .sum();
        let solved = model.solve_step(None);
        if compat.stiffness_stops_at_empty_filter {
            assert_eq!(diag4, 0.0, "upstream: quadrangle missing from K");
            assert!(solved.is_err(), "singular K");
        } else {
            assert!(diag4 > 0.0);
            solved.unwrap();
            assert!(
                model
                    .damage()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .all(|d| *d > 0. && *d < 1.)
            );
        }
    }
}

/// Error paths: unknown irreversibility type or energy split, the dynamic
/// solver (upstream `AKANTU_TO_IMPLEMENT`), an unknown matrix id, an
/// element without a law, a mesh with ghost elements.
#[test]
fn error_paths() {
    let bad =
        "phasefield quadratic [\n name = b\n gc = 1\n l0 = 1\n irreversibility_type = magic\n]\n";
    let mut m = model_with(quad_mesh(1, 1, 1., 1.), bad);
    let err = m
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Unknown irreversibility type"), "{err}");
    let bad_split = "phasefield quadratic spectral [\n name = b\n]\n";
    let mut m = model_with(quad_mesh(1, 1, 1., 1.), bad_split);
    let err = m
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap_err()
        .to_string();
    assert!(err.contains("Unknown energy split type"), "{err}");
    let mut m = model_with(quad_mesh(1, 1, 1., 1.), AT2);
    let err = m
        .init_full(&ModelOptions::new(AnalysisMethod::ImplicitDynamic))
        .unwrap_err();
    assert!(
        matches!(err, akantu_core::Error::NotImplemented(_)),
        "{err}"
    );
    // unknown matrix id through the solver callback
    let mut m = model_with(quad_mesh(1, 1, 1., 1.), AT2);
    init_static(&mut m);
    {
        use akantu_solver::SolverCallback;
        let PhaseFieldModel {
            physics, solver, ..
        } = &mut m;
        let err = physics
            .assemble_matrix(&mut solver.dof_manager, "M")
            .unwrap_err()
            .to_string();
        assert!(err.contains("Unknown Matrix ID"), "{err}");
    }
    // a boundary element has no phase field
    let mut mesh = quad_mesh(1, 1, 1., 1.);
    {
        let mut a = MeshAccessor::new(&mut mesh);
        a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
            .push_tuple(&[0, 1]);
    }
    {
        let mut g = mesh.groups_mut().create_element_group("edge", 1).unwrap();
        g.add(
            Element::new(ElementType::Segment2, 0, GhostType::NotGhost),
            None,
            false,
        )
        .unwrap();
    }
    let mut m = model_with(mesh, AT2);
    init_static(&mut m);
    assert!(m.energy_of_group("dissipated", "edge").is_err());
    // ghost elements: distributed meshes are refused
    let mut mesh = quad_mesh(1, 1, 1., 1.);
    {
        let mut a = MeshAccessor::new(&mut mesh);
        a.connectivity_mut(ElementType::Quadrangle4, GhostType::Ghost)
            .push_tuple(&[0, 1, 2, 3]);
    }
    let parser = Parser::parse_str(AT2, "m.dat").unwrap();
    let err = PhaseFieldModel::with_parser(SharedMesh::new(mesh), None, "pf", parser).unwrap_err();
    assert!(matches!(err, akantu_core::Error::Unsupported(_)), "{err}");
}

/// Parameters: upstream names, kinds and access (parsable + readable,
/// never writable), defaults of the laws.
#[test]
fn parameters_and_defaults() {
    let mut m = model_with(
        quad_mesh(1, 1, 1., 1.),
        "phasefield linear [\n name = l\n]\n",
    );
    init_static(&mut m);
    let law = m.phase_field(0).unwrap();
    assert_eq!(
        law.get_param("irreversibility_tol")
            .unwrap()
            .as_real()
            .unwrap(),
        1e-2
    );
    assert_eq!(
        law.get_param("recovery_tol").unwrap().as_real().unwrap(),
        1e-2
    );
    assert_eq!(law.get_param("l0").unwrap().as_real().unwrap(), 0.0);
    assert_eq!(law.get_param("gc").unwrap().as_real().unwrap(), 0.0);
    assert_eq!(
        law.get_param("irreversibility_type")
            .unwrap()
            .as_str()
            .unwrap(),
        "history"
    );
    assert!(!law.get_param("Plane_Stress").unwrap().as_bool().unwrap());
    let names = law.param_names();
    assert_eq!(
        names,
        vec![
            "name",
            "l0",
            "gc",
            "E",
            "nu",
            "irreversibility_type",
            "Plane_Stress",
            "irreversibility_tol",
            "recovery_tol"
        ]
    );
    let law = m.phase_field_mut(0).unwrap();
    assert!(
        law.set_param("l0", akantu_parser::ParamValue::Real(1.0))
            .is_err(),
        "not writable upstream"
    );
    assert_eq!(law.law_type(), "linear");
    assert_eq!(law.law_id(), "phase_field_model:0:linear");
}

/// The `phase_field` ParaView dumper (PF-012): the nodal fields of
/// upstream's `createNodalField*` and the elemental `partitions` are
/// written; other names are refused.
#[test]
fn dumper_writes_the_upstream_fields() {
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("pf_dump");
    let _ = std::fs::remove_dir_all(&dir);
    let mut model = model_with(quad_mesh(2, 1, 1.0, 0.5), AT2);
    init_static(&mut model);
    model.set_directory(&dir).unwrap();
    model.set_base_name("pf").unwrap();
    for f in [
        "damage",
        "external_force",
        "internal_force",
        "blocked_dofs",
        "partitions",
    ] {
        model.add_dump_field(f).unwrap();
    }
    assert!(model.add_dump_field("stress").is_err());
    set_uniform_strain(&mut model, &uniaxial(2, 3e-3));
    model.solve_step(None).unwrap();
    model.dump(None, None).unwrap();
    let mut found = Vec::new();
    for e in walk(&dir) {
        if e.extension().is_some_and(|x| x == "vtu") {
            found.push(std::fs::read_to_string(&e).unwrap());
        }
    }
    assert!(!found.is_empty(), "no .vtu written under {}", dir.display());
    for f in [
        "damage",
        "external_force",
        "internal_force",
        "blocked_dofs",
        "partitions",
    ] {
        assert!(
            found.iter().any(|t| t.contains(&format!("Name=\"{f}\""))),
            "{f} missing"
        );
    }
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Topology events through `SharedMesh::mutate` (upstream
/// `ConstitutiveLawsHandler::onElementsAdded/Removed`): an appended segment
/// is assigned to the law (internals and the DOF family grow) and the model
/// solves on the longer bar; erasing it compacts the law's filter and
/// internals again.
#[test]
fn element_insertion_and_removal_events() {
    use akantu_mesh::{MeshEvent, MeshPart};
    let dat = "phasefield quadratic [\n name = body\n E = 1\n nu = 0\n gc = 1\n l0 = 1\n]\n";
    let mut model = model_with(segment_mesh(&[0., 0.5, 1.0, 1.5]), dat);
    init_static(&mut model);
    model.damage_mut().unwrap().as_mut_slice()[0] = 1.0;
    model.blocked_dofs_mut().unwrap().as_mut_slice()[0] = true;
    model.solve_step(None).unwrap();
    model
        .mutate_mesh(
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[2.0])?;
                let e = tx.push_element(
                    MeshPart::Main,
                    ElementType::Segment2,
                    GhostType::NotGhost,
                    &[3, n],
                )?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsAdded {
                        elements: vec![e],
                        constitutive_law: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    let law = model.phase_field(0).unwrap();
    assert_eq!(
        law.base()
            .filter(ElementType::Segment2, GhostType::NotGhost),
        &[0, 1, 2, 3]
    );
    let nq = law
        .base()
        .strain
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .len();
    assert_eq!(nq % 4, 0, "internals cover the new element");
    assert_eq!(model.damage().unwrap().len(), 5);
    assert_eq!(model.external_force().unwrap().len(), 5);
    model.solve_step(None).unwrap();
    let d = model.damage().unwrap().as_slice().to_vec();
    assert!(d.windows(2).all(|w| w[1] < w[0]), "monotone profile: {d:?}");
    model
        .mutate_mesh(
            |tx| {
                tx.erase_elements(
                    MeshPart::Main,
                    &[Element::new(ElementType::Segment2, 1, GhostType::NotGhost)],
                )
            },
            None,
        )
        .unwrap();
    // upstream renumbering (`Mesh::eraseElements`): the kept elements
    // keep their law-local order, mapped to their new mesh indices
    let law = model.phase_field(0).unwrap();
    let filter = law
        .base()
        .filter(ElementType::Segment2, GhostType::NotGhost)
        .to_vec();
    let mut sorted = filter.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 1, 2], "filter {filter:?}");
    assert_eq!(
        law.base()
            .strain
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .len()
            * 4,
        nq * 3
    );
    let local = model
        .phase_field_local_numbering()
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec();
    for (i, &e) in filter.iter().enumerate() {
        assert_eq!(
            local[e as usize], i as Idx,
            "law-local number of element {e}"
        );
    }
    model.solve_step(None).unwrap();
}

/// Exact solution of the AT1 point problem with penalization (uniform
/// fields): `−2φ + 3g_c/(8l0) + 2φ d + γ min(0, d − d_p) + ρ min(0, d) = 0`
/// is piecewise linear in `d`; the branch whose activity pattern is
/// consistent with its root is the solution.
fn at1_penalized(phi: Real, gc: Real, l0: Real, gamma: Real, rho: Real, dp: Real) -> Real {
    let f0 = -2. * phi + 3. * gc / (8. * l0);
    for (ir, rec) in [(false, false), (true, false), (false, true), (true, true)] {
        let k = 2. * phi + if ir { gamma } else { 0. } + if rec { rho } else { 0. };
        let rhs = -f0 + if ir { gamma * dp } else { 0. };
        let d = rhs / k;
        if (d < dp) == ir && (d < 0.) == rec {
            return d;
        }
    }
    panic!("no consistent branch");
}

/// AT1 with `irreversibility_type = penalization` (PF-014): loading,
/// unloading (irreversibility penalty) and a strain below the AT1 threshold
/// from zero damage (recovery penalty on `d < 0`, `ρ_rec > 0` because the
/// mesh diagonal exceeds `2 l0`); uniform fields, exact point problem.
#[test]
fn linear_penalization_point_problem() {
    let (gc, l0, tol_ir, tol_rec) = (5e-3, 0.5, 1e-2, 1e-2);
    let dat = format!(
        "phasefield linear [\n name = body\n E = 210\n nu = 0.3\n gc = {gc}\n l0 = {l0}\n irreversibility_type = penalization\n irreversibility_tol = {tol_ir}\n recovery_tol = {tol_rec}\n]\n"
    );
    let mut model = model_with(quad_mesh(4, 1, 4.0, 1.0), &dat);
    init_static(&mut model);
    set_newton(&mut model, 100, 1e-12, "residual");
    let diam = (17.0 as Real).sqrt();
    let gamma = gc / l0 * 27. / (64. * tol_ir * tol_ir);
    let rho = gc / l0 * 9. * (diam / l0 - 2.) / (64. * tol_rec);
    assert!(rho > 0.0);
    let s = EnergySplit::new(EnergySplitKind::NoSplit, 210., 0.3, false);
    let c = 0.5 * (s.lambda + 2. * s.mu);
    let mut negative = false;
    for (k, eps) in [1e-3, 4e-3, 7e-3, 3e-3, 0.5e-3].into_iter().enumerate() {
        let dp = model
            .phase_field(0)
            .unwrap()
            .base()
            .damage
            .previous(ElementType::Quadrangle4, GhostType::NotGhost)
            .unwrap()
            .as_slice()[0];
        set_uniform_strain(&mut model, &uniaxial(2, eps));
        model.solve_step(None).unwrap();
        model.compute_driving_forces().unwrap();
        model.save_previous_state().unwrap();
        let want = at1_penalized(c * eps * eps, gc, l0, gamma, rho, dp);
        negative |= want < 0.;
        for &d in model.damage().unwrap().as_slice() {
            assert!(
                (d - want).abs() <= 1e-12 * want.abs().max(1e-3),
                "step {k} (ε = {eps}): {d} vs {want}"
            );
        }
    }
    assert!(negative, "the recovery branch (d < 0) is exercised");
}

/// `irreversibility_type = tao` (PF-015/018): neither history nor penalty —
/// the damage follows the load reversibly and AT1 becomes negative below
/// its threshold (the bounds `0 ≤ d ≤ d_prev…1` are the job of the TAO
/// solver, NLS-007).
#[test]
fn tao_mode_has_no_irreversibility_without_solver_bounds() {
    let s = EnergySplit::new(EnergySplitKind::NoSplit, 210., 0.3, false);
    let c = 0.5 * (s.lambda + 2. * s.mu);
    for law in ["quadratic", "linear"] {
        let dat = format!(
            "phasefield {law} [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.5\n irreversibility_type = tao\n]\n"
        );
        let mut model = model_with(quad_mesh(2, 1, 1.0, 0.5), &dat);
        init_static(&mut model);
        for eps in [6e-3, 2e-3, 6e-3] {
            set_uniform_strain(&mut model, &uniaxial(2, eps));
            model.solve_step(None).unwrap();
            model.save_previous_state().unwrap();
            let phi = c * eps * eps;
            let want = if law == "quadratic" {
                2. * phi / (2. * phi + 5e-3 / 0.5)
            } else {
                1. - 3. * 5e-3 / (16. * 0.5 * phi)
            };
            for &d in model.damage().unwrap().as_slice() {
                assert!((d - want).abs() <= 1e-12, "{law} ε = {eps}: {d} vs {want}");
            }
        }
    }
}

/// AT1 on a programmatic mesh without bounding box (upstream would compute
/// an infinite `ρ_rec` and NaN penalties): refused with a clear error.
#[test]
fn linear_law_needs_the_mesh_bounding_box() {
    let mut mesh = Mesh::new(1, "no_bbox").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        a.push_node(&[0.]).unwrap();
        a.push_node(&[1.]).unwrap();
        a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
            .push_tuple(&[0, 1]);
    }
    let mut model = model_with(
        mesh,
        "phasefield linear [\n name = b\n gc = 1\n l0 = 1\n]\n",
    );
    let err = model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap_err()
        .to_string();
    assert!(err.contains("bounding box"), "{err}");
}

/// PF-002: the two explicit analysis methods
/// (`phase_field_model.cc:107-188`) end as upstream does on the oracle.
/// Probe `oracle/probes/phase_field_explicit` (strict
/// x86_64 oracle image `45122c21`, upstream `material_coupling.dat` +
/// `test_one_element.msh`, three runs equal up to the debug timestamp):
///
/// * `_explicit_lumped_mass` → solver `explicit_lumped`
///   (`_dynamic_lumped`, `_lumped` NLS, central difference on `damage`):
///   `initFull` succeeds, `setTimeStep` succeeds, `solveStep` throws "No
///   derivatives of order 1 present in phase_field_model:dof_manager_default
///   for dof damage" (the model never registers the velocity of `damage`;
///   `assembleLumpedMatrix` is a no-op);
/// * `_explicit_consistent_mass` → `_dynamic`: `initFull` throws
///   "initSolver : not implemented yet !" (`AKANTU_TO_IMPLEMENT`).
///
/// The port fails at the same calls, with the same causes, and leaves the
/// damage untouched by the failed step.
#[test]
fn explicit_solver_branches_end_as_upstream() {
    use akantu_core::enums::TimeStepSolverType;
    use akantu_phase_field::model::default_solver_id;
    assert_eq!(
        default_solver_id(AnalysisMethod::ExplicitLumpedMass),
        ("explicit_lumped".into(), TimeStepSolverType::DynamicLumped)
    );
    assert_eq!(
        default_solver_id(AnalysisMethod::ExplicitConsistentMass),
        ("explicit".into(), TimeStepSolverType::Dynamic)
    );

    let mut m = model_from_files("test_one_element.msh", "material_coupling.dat", 2);
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .expect("upstream: initFull(_explicit_lumped_mass) succeeds");
    m.set_time_step(1e-3, None)
        .expect("upstream: setTimeStep succeeds");
    let before = m.damage().unwrap().as_slice().to_vec();
    let err = m.solve_step(None).unwrap_err();
    assert!(
        matches!(&err, akantu_core::Error::NotFound { kind, id }
            if *kind == "dof derivative" && id == "damage:1"),
        "upstream: solveStep throws 'No derivatives of order 1 … for dof damage'; got {err}"
    );
    assert_eq!(m.damage().unwrap().as_slice(), &before[..]);

    let mut m = model_from_files("test_one_element.msh", "material_coupling.dat", 2);
    let err = m
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitConsistentMass))
        .unwrap_err();
    assert!(
        matches!(err, akantu_core::Error::NotImplemented(_)),
        "upstream: initFull(_explicit_consistent_mass) throws AKANTU_TO_IMPLEMENT; got {err}"
    );
}

/// UD-757 (category A): upstream's `Model` registers itself as a mesh event
/// handler (`model.cc:38`) and never unregisters (`Model::~Model() =
/// default`), so a destroyed model leaves a dangling handler in the mesh
/// and a new model built at the same address on that mesh is refused ("This
/// event handler was already registered (priority: 94)" — seen by the
/// oracle probe `oracle/probes/phase_field_explicit`). The port leaves no
/// registration behind: a model dropped from a mesh is not notified any
/// more, and further models on the same mesh, in sequence or alive
/// together, are accepted and receive the events.
#[test]
fn ud757_a_dropped_model_leaves_no_handler_on_the_mesh() {
    let mesh = SharedMesh::new(quad_mesh(1, 1, 1., 1.));
    let new_model = |id: &str| {
        let parser = Parser::parse_str(AT2, "m.dat").unwrap();
        PhaseFieldModel::with_parser(mesh.clone(), None, id, parser).unwrap()
    };
    {
        let mut first = new_model("pf");
        init_static(&mut first);
    }
    let mut second = new_model("pf");
    init_static(&mut second);
    let mut third = new_model("pf2");
    init_static(&mut third);
    // a coordinated mesh mutation needs every registered consumer: it
    // succeeds with the two live models only (the dropped one left no
    // registration behind), and both receive the event
    use akantu_core::enums::EventPriority;
    use akantu_mesh::{MeshEvent, MeshEventHandler, MeshPart};
    let scratch = SharedMesh::new(Mesh::new(2, "scratch").unwrap());
    let swap = |model: &mut PhaseFieldModel, name: &str| {
        let placeholder = scratch
            .register_consumer(name, EventPriority::MODEL, MeshPart::Main)
            .unwrap();
        std::mem::replace(&mut model.token, placeholder)
    };
    let t2 = swap(&mut second, "s2");
    let t3 = swap(&mut third, "s3");
    mesh.mutate(
        &mut [
            (&t2, &mut second as &mut dyn MeshEventHandler),
            (&t3, &mut third as &mut dyn MeshEventHandler),
        ],
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[2.0, 0.0])?;
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "test".into(),
                    extension: None,
                },
            );
            Ok(())
        },
        None,
    )
    .unwrap();
    second.token = t2;
    third.token = t3;
    for m in [&second, &third] {
        assert_eq!(m.damage().unwrap().len(), 5);
        assert_eq!(m.external_force().unwrap().len(), 5);
    }
}

/// SMM-015: the phase-field model
/// follows node removal (refused before, UD-209);
/// what its forwarded capability check still refuses **before** the commit
/// — recoverable, mesh and model untouched, the domain usable, instead of
/// poisoning the domain after the commit — is a node appended and removed
/// again in one mutation (the DOF manager's composition refusal: the
/// addition names a node that does not exist in the final mesh).
#[test]
fn fu01_node_events_the_model_cannot_follow_are_refused_before_commit() {
    use akantu_core::enums::EventPriority;
    use akantu_mesh::{MeshEvent, MeshEventHandler, MeshPart};
    let dat = "phasefield quadratic [\n name = body\n E = 1\n nu = 0\n gc = 1\n l0 = 1\n]\n";
    let mut model = model_with(segment_mesh(&[0., 0.5, 1.0, 1.5]), dat);
    init_static(&mut model);
    let shared = model.mesh_handle().clone();
    let (nodes_before, revision) = (
        shared.read().unwrap().nodes().as_slice().to_vec(),
        shared.revision().unwrap(),
    );
    let damage_before = model.damage().unwrap().as_slice().to_vec();
    let scratch = SharedMesh::new(Mesh::new(1, "scratch").unwrap());
    let swap = |model: &mut PhaseFieldModel, name: &str| {
        let placeholder = scratch
            .register_consumer(name, EventPriority::MODEL, MeshPart::Main)
            .unwrap();
        std::mem::replace(&mut model.token, placeholder)
    };
    let token = swap(&mut model, "p1");
    let r = shared.mutate(
        &mut [(&token, &mut model as &mut dyn MeshEventHandler)],
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[2.0])?;
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "fu01".into(),
                    extension: None,
                },
            );
            tx.remove_nodes(MeshPart::Main, &[0, 1, 2, 3, -1])
        },
        None,
    );
    model.token = token;
    let Err(e) = &r else {
        panic!("expected the composition refusal, got {r:?}");
    };
    assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert!(format!("{e}").contains("record the removal first"), "{e}");
    assert_eq!(shared.read().unwrap().nodes().as_slice(), &nodes_before[..]);
    assert_eq!(shared.revision().unwrap(), revision);
    assert_eq!(model.damage().unwrap().as_slice(), &damage_before[..]);
    // the domain stays usable
    let token = swap(&mut model, "p2");
    let again = shared.mutate(
        &mut [(&token, &mut model as &mut dyn MeshEventHandler)],
        |_| Ok(()),
        None,
    );
    model.token = token;
    assert!(again.is_ok(), "{again:?}");
    model.solve_step(None).unwrap();
}
