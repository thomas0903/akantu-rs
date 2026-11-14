// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential
// tests against fixtures/model/phase_field/* (oracle/probes/phase_field/
// phase_field_probe.cc, strict reference build, Eigen sparse solver).
//
// Every case is replayed with
//
// * `PhaseFieldUpstreamCompat::upstream()` — every recorded value must be
//   bitwise identical (D-008): nodal damage, internal force, Newton
//   iteration counts, errors and converged flags (a fixture without
//   `newton/converged` converged at every step: the probe aborts otherwise),
//   dissipated energies after every step, every law internal (current and
//   history), the stiffness K (COO in storage order), per-element and
//   per-group energies;
// * for each upstream-defect switch S (UD-377…UD-380, UD-385, UD-387,
//   UD-388):
//   upstream switches except S corrected — the replay must differ from the
//   fixture in **exactly** the quantity groups the case declares for S
//   (`expected`; an empty declaration means bitwise identical), or be
//   refused at initialisation where the case declares it (UD-385). So
//   flipping any single correction back to upstream behaviour fails this
//   test (mutation evidence in docs/NUMERICAL_VALIDATION.md);
// * the default (all corrected) switches — exactly the union of the
//   per-switch declarations (or refused if any switch refuses).
//
// The test also asserts the number of committed cases (`EXPECTED_CASES`):
// a missing fixture tree fails instead of checking nothing.
//
// The random `gc` cases draw from the process-global generator
// (`RandomGenerator<Idx>`, D-006), so the replays of this binary run one at
// a time (`SERIAL`).
//
// The fixtures are generated on the x86_64 oracle
// (oracle/probes/phase_field/generate.sh): 30 cases, the 9
// adversarial `rev_*` cases and the 3
// `patch_*_linear_voldev_history` cases (generated
// with the D-050 pin, committed).

use akantu_validation::reduce::NanMinMax;
use std::collections::BTreeMap;
use std::sync::Mutex;

use akantu_core::enums::{
    AnalysisMethod, MeshIoType, NonLinearSolverType, SparseSolverType, SpatialDirection,
};
use akantu_core::random::{IdxSlot, RandomGenerator};
use akantu_core::{Element, ElementType, GhostType, Idx};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_phase_field::selector::MeshDataPhaseFieldSelector;
use akantu_phase_field::{PhaseFieldModel, PhaseFieldUpstreamCompat};
use akantu_solver::ModelSolverOptions;
use akantu_validation::{Fixture, fixtures_root};

static SERIAL: Mutex<()> = Mutex::new(());

/// Committed cases under fixtures/model/phase_field: 30 + 9 (adversarial
/// cases) + 3 (AT1 + history +
/// volumetric-deviatoric split on the 1-D/2-D/3-D patch meshes).
/// The dump cases `dump_*`
/// are not solve replays: `pf_dump_set_time_step.rs` replays them.
const EXPECTED_CASES: usize = 42;

fn cases() -> Vec<String> {
    let dir = fixtures_root().join("model").join("phase_field");
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .filter(|e| e.path().join("manifest.json").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with("dump_"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn open(name: &str) -> Fixture {
    let dir = fixtures_root().join("model").join("phase_field").join(name);
    let f = Fixture::open(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    f
}

fn param<'a>(f: &'a Fixture, key: &str) -> &'a serde_json::Value {
    &f.manifest().parameters[key]
}

fn f64s(f: &Fixture, name: &str) -> Vec<f64> {
    f.load_f64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

fn i64s(f: &Fixture, name: &str) -> Vec<i64> {
    f.load_i64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

fn has(f: &Fixture, name: &str) -> bool {
    f.quantity(name).is_ok()
}

/// Mismatch bookkeeping of one replay: quantity group → (values compared,
/// values whose bits differ, largest relative difference).
#[derive(Debug, Default)]
struct Replay {
    groups: BTreeMap<&'static str, (usize, usize, f64)>,
}

impl Replay {
    fn compare(&mut self, group: &'static str, got: &[f64], want: &[f64]) {
        let e = self.groups.entry(group).or_default();
        let scale = want.iter().fold(0.0f64, |m, v| m.nan_max(v.abs()));
        e.0 += want.len();
        e.1 += got.len().abs_diff(want.len());
        for (x, y) in got.iter().zip(want) {
            // D-046: bit identity, or default NaNs of either sign
            if !akantu_validation::exact_eq(*y, *x) {
                e.1 += 1;
                e.2 = e.2.nan_max((x - y).abs() / scale.max(f64::MIN_POSITIVE));
            }
        }
    }

    fn compare_i(&mut self, group: &'static str, got: &[i64], want: &[i64]) {
        let e = self.groups.entry(group).or_default();
        e.0 += want.len();
        e.1 += got.len().abs_diff(want.len());
        e.1 += got.iter().zip(want).filter(|(a, b)| a != b).count();
    }

    fn differing(&self) -> Vec<&'static str> {
        self.groups
            .iter()
            .filter(|(_, v)| v.1 > 0)
            .map(|(k, _)| *k)
            .collect()
    }

    fn compared(&self) -> usize {
        self.groups.values().map(|v| v.0).sum()
    }
}

fn eigen_options() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// Replay the probe's sequence for `f` with the switches `compat`; `Err`
/// when the model refuses the case at initialisation (UD-385).
fn replay(f: &Fixture, compat: PhaseFieldUpstreamCompat) -> Result<Replay, String> {
    let mut r = Replay::default();
    let dim = param(f, "spatial_dimension").as_u64().unwrap() as usize;
    let steps = param(f, "steps").as_u64().unwrap() as usize;
    let save = param(f, "save_previous_state").as_bool().unwrap();
    let solve = param(f, "solve").as_bool().unwrap();
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    RandomGenerator::<IdxSlot>::seed(param(f, "seed").as_i64().unwrap());
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, &f.input_path("mesh").unwrap(), MeshIoType::Gmsh).unwrap();
    let mut model =
        PhaseFieldModel::with_parser(SharedMesh::new(mesh), None, "phase_field_model", parser)
            .unwrap();
    model.set_upstream_compat(compat);
    if param(f, "selector_physical_names").as_bool().unwrap() {
        model.set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    }
    model
        .init_full(&eigen_options())
        .map_err(|e| e.to_string())?;
    {
        let nls = model.solver.non_linear_solver_mut(None).unwrap();
        nls.set_param(
            "max_iterations",
            ParamValue::Int(param(f, "newton_max_iterations").as_i64().unwrap()),
        )
        .unwrap();
        nls.set_param(
            "threshold",
            ParamValue::Real(param(f, "newton_threshold").as_f64().unwrap()),
        )
        .unwrap();
        nls.set_param(
            "convergence_type",
            ParamValue::Enum(param(f, "newton_convergence_type").as_str().unwrap().into()),
        )
        .unwrap();
    }
    let bc_value = param(f, "bc_value").as_f64().unwrap();
    let bc_group = param(f, "bc_group").as_str().unwrap_or("");
    if bc_group.is_empty() {
        if has(f, "bc_nodes") {
            for n in i64s(f, "bc_nodes") {
                let n = n as usize;
                model.damage_mut().unwrap().as_mut_slice()[n] = bc_value;
                model.blocked_dofs_mut().unwrap().as_mut_slice()[n] = true;
            }
        }
    } else {
        model
            .apply_dirichlet_bc(
                &FixedValue {
                    value: bc_value,
                    axis: SpatialDirection::X,
                },
                bc_group,
            )
            .unwrap();
    }
    if has(f, "initial_damage") {
        let d0 = f64s(f, "initial_damage");
        model
            .damage_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&d0);
    }
    let nb_laws = model.nb_phase_fields();
    let mut iterations = Vec::new();
    let mut errors = Vec::new();
    let mut converged: Vec<i64> = Vec::new();
    let mut energies = Vec::new();
    for k in 0..steps {
        for l in 0..nb_laws {
            let law = model.phase_field_mut(l).unwrap();
            for ty in law.base().filter_types(GhostType::NotGhost) {
                if law.base().filter(ty, GhostType::NotGhost).is_empty() {
                    continue;
                }
                let want = f64s(f, &format!("step/{k}/strain/{l}/{ty}"));
                let mut s = law
                    .base_mut()
                    .strain
                    .get_mut(ty, GhostType::NotGhost)
                    .unwrap();
                assert_eq!(
                    s.as_slice().len(),
                    want.len(),
                    "{}: strain block",
                    f.case_id()
                );
                s.as_mut_slice().copy_from_slice(&want);
            }
        }
        if solve {
            // a failing solve (upstream Newton divergence) is recorded as in
            // the probe: the state after the failed solve is compared
            converged.push(i64::from(model.solve_step(Some("static")).is_ok()));
            let nls = model.solver.non_linear_solver(None).unwrap();
            iterations.push(nls.get_param("nb_iterations").unwrap().as_int().unwrap());
            errors.push(nls.get_param("error").unwrap().as_real().unwrap());
            r.compare(
                "damage",
                model.damage().unwrap().as_slice(),
                &f64s(f, &format!("step/{k}/damage")),
            );
            r.compare(
                "internal_force",
                model.internal_force().unwrap().as_slice(),
                &f64s(f, &format!("step/{k}/internal_force")),
            );
            energies.push(model.energy("dissipated").unwrap());
            if save {
                model.save_previous_state().unwrap();
            }
        } else {
            model.assemble_internal_forces().unwrap();
            model.assemble_stiffness_matrix().unwrap();
            r.compare(
                "internal_force",
                model.internal_force().unwrap().as_slice(),
                &f64s(f, &format!("step/{k}/internal_force")),
            );
            energies.push(model.energy("dissipated").unwrap());
        }
    }
    if solve {
        r.compare_i("newton", &iterations, &i64s(f, "newton/nb_iterations"));
        r.compare("newton", &errors, &f64s(f, "newton/error"));
        let want_converged = if has(f, "newton/converged") {
            i64s(f, "newton/converged")
        } else {
            vec![1; steps]
        };
        r.compare_i("newton", &converged, &want_converged);
    }
    r.compare("energy", &energies, &f64s(f, "energy/dissipated"));
    // internals
    for l in 0..nb_laws {
        let law = model.phase_field(l).unwrap();
        let base = law.base();
        for ty in base.filter_types(GhostType::NotGhost) {
            if !has(f, &format!("law/{l}/filter/{ty}")) {
                // large cases record no internals (size policy)
                continue;
            }
            let filter: Vec<i64> = base
                .filter(ty, GhostType::NotGhost)
                .iter()
                .map(|&x| i64::from(x))
                .collect();
            r.compare_i("filter", &filter, &i64s(f, &format!("law/{l}/filter/{ty}")));
            for (name, field) in base.internals() {
                let group: &'static str = if name == "damage" || name == "grad_d" {
                    "internal_damage"
                } else {
                    "internals"
                };
                r.compare(
                    group,
                    field.get(ty, GhostType::NotGhost).unwrap().as_slice(),
                    &f64s(f, &format!("law/{l}/{name}/{ty}")),
                );
                if name == "damage" || name == "phi" {
                    r.compare(
                        group,
                        field.previous(ty, GhostType::NotGhost).unwrap().as_slice(),
                        &f64s(f, &format!("law/{l}/previous_{name}/{ty}")),
                    );
                }
            }
        }
    }
    // stiffness
    if has(f, "K/values") {
        let k = model.dof_manager().matrix("K").unwrap();
        let (irn, jcn, a) = k.coo();
        let rows: Vec<i64> = irn.to_vec();
        let cols: Vec<i64> = jcn.to_vec();
        r.compare_i("K_profile", &rows, &i64s(f, "K/rows"));
        r.compare_i("K_profile", &cols, &i64s(f, "K/cols"));
        r.compare("K", a, &f64s(f, "K/values"));
    }
    {
        let mesh_h = model.mesh_handle().clone();
        let types: Vec<ElementType> = {
            let mesh = mesh_h.read().unwrap();
            mesh.element_types(akantu_core::TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: GhostType::NotGhost,
                kind: Some(akantu_core::ElementKind::Regular),
            })
            .collect()
        };
        for ty in types {
            let n = mesh_h.read().unwrap().nb_element(ty, GhostType::NotGhost);
            let got: Vec<f64> = (0..n)
                .map(|e| {
                    model
                        .energy_of_element(
                            "dissipated",
                            Element::new(ty, e as Idx, GhostType::NotGhost),
                        )
                        .unwrap()
                })
                .collect();
            r.compare(
                "element_energy",
                &got,
                &f64s(f, &format!("energy/element/{ty}")),
            );
        }
        let groups: Vec<String> = param(f, "energy_groups")
            .as_array()
            .map(|a| a.iter().map(|g| g.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default();
        let got: Vec<f64> = groups
            .iter()
            .map(|g| model.energy_of_group("dissipated", g).unwrap())
            .collect();
        if !groups.is_empty() {
            r.compare("group_energy", &got, &f64s(f, "energy/group"));
        }
    }
    Ok(r)
}

/// The upstream-defect switches of `PhaseFieldUpstreamCompat`: UD id and the
/// function that sets the **corrected** behaviour on an upstream set.
type Switch = (&'static str, fn(&mut PhaseFieldUpstreamCompat));
const SWITCHES: [Switch; 7] = [
    ("UD-377", |c| c.unfiltered_damage_interpolation = false),
    ("UD-378", |c| c.stiffness_stops_at_empty_filter = false),
    ("UD-379", |c| c.element_energy_first_point_gc = false),
    ("UD-380", |c| {
        c.element_energy_local_index_as_mesh_index = false
    }),
    ("UD-385", |c| c.accept_nonpositive_recovery_penalty = false),
    ("UD-388", |c| c.at1_strict_irreversibility_indicator = false),
    ("UD-387", |c| c.in_block_deviator_energy = false),
];

/// What a replay with one corrected switch (others upstream) must give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    /// Bitwise identical to the fixture (the defect is not triggered).
    Same,
    /// Differs in exactly these quantity groups (sorted).
    Differs(&'static [&'static str]),
    /// Refused at initialisation (UD-385).
    Refused,
}

/// Groups a converged Newton–Raphson path changes when only its iterates
/// change (UD-388: the corrected tangent adds `γ` at `d = d_prev`, so the
/// iteration counts/errors differ and the converged state differs at the
/// solver-threshold level).
const NEWTON_PATH: &[&str] = &[
    "damage",
    "element_energy",
    "energy",
    "internal_damage",
    "internal_force",
    "internals",
    "newton",
];

/// Groups a corrected driving energy changes (UD-387: `φ` enters the
/// residual, the point density `2φ + …` of `K` and every quantity after the
/// solve).
const DRIVING_ENERGY: &[&str] = &[
    "K",
    "damage",
    "element_energy",
    "energy",
    "internal_damage",
    "internal_force",
    "internals",
    "newton",
];

/// Declared effect of correcting `ud` on `case` (see the module header).
/// Every case/switch pair not listed is `Same`.
fn expected(case: &str, ud: &str) -> Expect {
    use Expect::{Differs, Refused, Same};
    match (case, ud) {
        // UD-377: each law reads the damage of the first mesh elements
        // (history laws: K does not depend on d, no solve in this case)
        ("multi_law_segments", "UD-377") => Differs(&[
            "element_energy",
            "energy",
            "group_energy",
            "internal_damage",
            "internal_force",
            "internals",
        ]),
        // UD-380: per-element energies integrated with the law-local index
        // (group energies are sums of per-element energies)
        ("multi_law_segments" | "rev_mixed_three_laws", "UD-380") => {
            Differs(&["element_energy", "group_energy"])
        }
        // UD-378: the quadrangle of law `quad` is missing from K upstream;
        // no quadrangle is assembled at all, so the profile lacks it too
        ("mixed_types_stiffness", "UD-378") => Differs(&["K", "K_profile"]),
        // UD-378: law `c`'s quadrangle is missing from K upstream; laws a/b
        // assemble quadrangles, and the profile is added once per (matrix,
        // type) over every element of the mesh (upstream addToProfile), so
        // only the values change
        ("rev_mixed_three_laws", "UD-378") => Differs(&["K"]),
        // UD-377: as multi_law_segments; law `a` is penalised, so K (its
        // density γ·[d < d_prev]) depends on the interpolated damage
        ("rev_mixed_three_laws", "UD-377") => Differs(&[
            "K",
            "element_energy",
            "energy",
            "group_energy",
            "internal_damage",
            "internal_force",
            "internals",
        ]),
        // UD-379: random gc with several points per element
        (
            "patch_hexahedron_8_quadratic_tao_random_gc"
            | "patch_quadrangle_4_quadratic_tao_random_gc"
            | "rev_random_gc_seed_quadrangle_8",
            "UD-379",
        ) => Differs(&["element_energy"]),
        // UD-385: AT1 penalisation with ρ_rec < 0 (diam = √2 < 2 l0 = 2)
        ("rev_at1_negative_rho_rec", "UD-385") => Refused,
        // UD-388: AT1 penalisation (AT2 penalisation cases are unaffected).
        // Converged paths: the state differs at the threshold level in every
        // quantity recorded after the solve; the last assembled K only where
        // an indicator differs at the last iterate.
        (
            "patch_hexahedron_20_linear_voldev_penalization"
            | "patch_quadrangle_4_linear_voldev_penalization"
            | "patch_tetrahedron_4_linear_voldev_penalization",
            "UD-388",
        ) => Differs(NEWTON_PATH),
        // one point per element: interpolated damage and energies round to
        // the same values
        ("patch_triangle_3_linear_voldev_penalization", "UD-388") => {
            Differs(&["damage", "internal_force", "internals", "newton"])
        }
        ("rev_at1_full_unload_triangle_3", "UD-388") => Differs(&[
            "damage",
            "element_energy",
            "internal_damage",
            "internal_force",
            "internals",
            "newton",
        ]),
        ("rev_at1_unload_recovery", "UD-388") => Differs(&[
            "damage",
            "energy",
            "internal_damage",
            "internal_force",
            "internals",
            "newton",
        ]),
        // upstream diverges (full unload: singular tangent; negative ρ_rec
        // accepted by the UD-385 compat path): the corrected tangent changes
        // everything, K included
        ("rev_at1_full_unload_quadrangle_4" | "rev_at1_negative_rho_rec", "UD-388") => Differs(&[
            "K",
            "damage",
            "element_energy",
            "energy",
            "internal_damage",
            "internal_force",
            "internals",
            "newton",
        ]),
        // UD-387 (category B, D-066 (2)): the consistent φ⁺ of the
        // volumetric–deviatoric split adds μ(3 − D)(tr ε/3)² in 1-D/2-D
        // (tension and compression alike); 3-D and no_split cases are the
        // same (`phi_consistent` returns `phi` there, bit for bit)
        ("one_element_deviatoric", "UD-387") => Differs(&[
            "K",
            "damage",
            "element_energy",
            "energy",
            "group_energy",
            "internal_damage",
            "internal_force",
            "internals",
            "newton",
        ]),
        (
            "patch_quadrangle_4_linear_voldev_penalization"
            | "patch_segment_2_linear_voldev_history"
            | "patch_segment_3_quadratic_voldev_penalization"
            | "patch_triangle_3_linear_voldev_history"
            | "patch_triangle_3_linear_voldev_penalization"
            | "patch_triangle_3_quadratic_voldev_penalization"
            | "patch_triangle_6_quadratic_voldev_penalization"
            | "rev_at1_negative_rho_rec"
            | "rev_at1_unload_recovery"
            | "rev_voldev_compression_segment_2"
            | "rev_voldev_compression_triangle_3_plane_strain",
            "UD-387",
        ) => Differs(DRIVING_ENERGY),
        // only the 2-D vol/dev law of the three changes: its φ (internals),
        // the density 2φ in K and the residual; the recorded damage and
        // energies of this case are unchanged
        ("rev_mixed_three_laws", "UD-387") => Differs(&["K", "internal_force", "internals"]),
        _ => Same,
    }
}

/// Observed outcome of a replay against the fixture.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Groups(Vec<&'static str>),
    Refused(String),
}

fn outcome(r: &Result<Replay, String>) -> Outcome {
    match r {
        Ok(r) => Outcome::Groups(r.differing()),
        Err(e) => Outcome::Refused(e.clone()),
    }
}

fn matches(got: &Outcome, want: Expect) -> bool {
    match (got, want) {
        (Outcome::Groups(g), Expect::Same) => g.is_empty(),
        (Outcome::Groups(g), Expect::Differs(w)) => {
            let mut w = w.to_vec();
            w.sort_unstable();
            *g == w
        }
        (Outcome::Refused(msg), Expect::Refused) => msg.contains("UD-385"),
        _ => false,
    }
}

fn report(case: &str, what: &str, r: &Result<Replay, String>) {
    match r {
        Ok(r) => {
            for (g, v) in &r.groups {
                if v.1 > 0 {
                    eprintln!(
                        "{case}: {what}: {g}: {} of {} differ (max rel {:e})",
                        v.1, v.0, v.2
                    );
                }
            }
        }
        Err(e) => eprintln!("{case}: {what}: refused at initialisation: {e}"),
    }
}

#[test]
fn phase_field_fixtures_bitwise() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let list = cases();
    assert_eq!(
        list.len(),
        EXPECTED_CASES,
        "phase_field_fixtures_bitwise: cases under {}: {list:?}",
        fixtures_root().join("model/phase_field").display()
    );
    let mut total = 0;
    let mut failures: Vec<String> = Vec::new();
    let mut exercised: BTreeMap<&'static str, usize> = BTreeMap::new();
    for case in &list {
        let f = open(case);
        match replay(&f, PhaseFieldUpstreamCompat::upstream()) {
            Ok(up) => {
                if !up.differing().is_empty() {
                    failures.push(format!(
                        "{case}: upstream-compat replay differs in {:?}: {:?}",
                        up.differing(),
                        up.groups
                    ));
                }
                total += up.compared();
            }
            Err(e) => failures.push(format!("{case}: upstream-compat replay refused: {e}")),
        }
        let mut union: Vec<&'static str> = Vec::new();
        let mut refused = false;
        for (ud, correct) in SWITCHES {
            let mut c = PhaseFieldUpstreamCompat::upstream();
            correct(&mut c);
            let r = replay(&f, c);
            let got = outcome(&r);
            let want = expected(case, ud);
            if !matches(&got, want) {
                failures.push(format!(
                    "{case}: {ud} corrected: expected {want:?}, got {got:?}"
                ));
            }
            match want {
                Expect::Same => {}
                Expect::Differs(g) => {
                    *exercised.entry(ud).or_default() += 1;
                    union.extend_from_slice(g);
                    report(case, &format!("{ud} corrected"), &r);
                }
                Expect::Refused => {
                    *exercised.entry(ud).or_default() += 1;
                    refused = true;
                    report(case, &format!("{ud} corrected"), &r);
                }
            }
        }
        union.sort_unstable();
        union.dedup();
        let r = replay(&f, PhaseFieldUpstreamCompat::default());
        let got = outcome(&r);
        let ok = match &got {
            Outcome::Refused(msg) => refused && msg.contains("UD-385"),
            Outcome::Groups(g) => !refused && *g == union,
        };
        if !ok {
            failures.push(format!(
                "{case}: all corrected: expected {}, got {got:?}",
                if refused {
                    "refused (UD-385)".to_owned()
                } else {
                    format!("{union:?}")
                }
            ));
        }
    }
    for (ud, _) in SWITCHES {
        if exercised.get(ud).copied().unwrap_or(0) == 0 {
            failures.push(format!("{ud}: no case exercises the corrected behaviour"));
        }
    }
    assert!(
        failures.is_empty(),
        "phase_field_fixtures_bitwise: {} failure(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!(
        "phase_field_fixtures_bitwise: {} cases, {total} values bitwise (upstream compat); \
         per-switch corrected replays as declared: {exercised:?}",
        list.len()
    );
}
