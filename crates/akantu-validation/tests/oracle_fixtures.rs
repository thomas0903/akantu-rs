// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: integrity and independent mathematical checks of the
// committed oracle fixtures (fixtures/**), written by oracle/probes.
//
// These tests check the *oracle data* (and the fixture format), not the
// Rust port: every committed manifest must validate, every checksum must
// match, and the element-class data must satisfy identities that hold
// independently of Akantu (exactness of Gauss rules, partition of unity,
// derivatives of a partition of unity sum to zero, ...).

use akantu_validation::reduce::NanMinMax;
use std::path::{Path, PathBuf};

use akantu_validation::fixture::{Fixture, MANIFEST_FILE};
use akantu_validation::manifest::{Layout, StorageMode};
use akantu_validation::npy::Dtype;
use akantu_validation::{FixtureStatus, fixtures_root};

fn all_cases(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                if p.join(MANIFEST_FILE).is_file() {
                    out.push(p.clone());
                }
                walk(&p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

#[test]
fn every_committed_fixture_is_valid_and_loadable() {
    let root = fixtures_root();
    let cases = all_cases(&root);
    assert!(!cases.is_empty(), "no fixtures under {}", root.display());
    for dir in cases {
        let fx = Fixture::open(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        if fx.manifest().storage.mode == StorageMode::External && fx.check_materialized().is_err() {
            continue; // external data not generated locally
        }
        fx.verify_checksums().unwrap_or_else(|e| panic!("{e}"));
        for q in &fx.manifest().quantities {
            match q.parsed_dtype() {
                Some(Dtype::F64) => {
                    fx.load_f64(&q.name).unwrap_or_else(|e| panic!("{e}"));
                }
                _ => {
                    fx.load_i64(&q.name).unwrap_or_else(|e| panic!("{e}"));
                }
            }
        }
        if fx.manifest().status == FixtureStatus::Golden {
            let b = fx
                .manifest()
                .build
                .as_ref()
                .expect("golden without build info");
            assert!(!b.compiler.is_empty() && !b.arch.is_empty());
        }
    }
}

/// D-073 (1): the platform variants of the committed tree (golden values
/// that depend on the oracle's numerical environment, class X2).
/// Checked whatever the running platform: every case with variants
/// holds verifiable, loadable values for each recorded environment with the
/// same quantities as its top level; the set of such cases is exactly the
/// decided one (x86_64 `Haswell` top level, aarch64 variant); and such a
/// case refuses an unknown running environment.
#[test]
fn platform_variants_are_complete_and_verifiable() {
    let root = fixtures_root();
    let mut with_variants = Vec::new();
    for dir in all_cases(&root) {
        let top = Fixture::open_top(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let envs = top.environments().unwrap_or_else(|e| panic!("{e}"));
        if envs.len() == 1 {
            continue;
        }
        let id = top.case_id().to_string();
        assert_eq!(envs, ["x86_64-haswell", "aarch64-auto"], "{id}");
        for env in &envs {
            let fx = Fixture::open_case_in(&root, &id, Some(env))
                .unwrap_or_else(|e| panic!("{id} ({env}): {e}"));
            assert_eq!(&fx.environment(), env, "{id}");
            assert_eq!(fx.variant().is_some(), env != &envs[0], "{id}");
            fx.verify_checksums()
                .unwrap_or_else(|e| panic!("{id} ({env}): {e}"));
            let names = |f: &Fixture| {
                f.manifest()
                    .quantities
                    .iter()
                    .map(|q| (q.name.clone(), q.shape.clone(), q.dtype.clone()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(names(&fx), names(&top), "{id} ({env})");
            for q in &fx.manifest().quantities {
                match q.parsed_dtype() {
                    Some(Dtype::F64) => {
                        fx.load_f64(&q.name).unwrap_or_else(|e| panic!("{e}"));
                    }
                    _ => {
                        fx.load_i64(&q.name).unwrap_or_else(|e| panic!("{e}"));
                    }
                }
            }
        }
        assert!(
            matches!(
                Fixture::open_case_in(&root, &id, None),
                Err(akantu_validation::Error::FixtureEnvironment {
                    environment: None,
                    ..
                })
            ),
            "{id}: a case with variants must refuse an unknown environment"
        );
        with_variants.push(id);
    }
    with_variants.sort();
    assert_eq!(
        with_variants,
        [
            "model/implicit_dynamic/patch_triangle_6_plastic",
            "model/static/loaded_quadrangle_4",
            "model/static/loaded_tetrahedron_4",
            "model/static/patch_hexahedron_20",
            "model/static/patch_quadrangle_8",
            "model/static/patch_tetrahedron_4",
            "parallel/smm_static_cube_n3/np1/rank0",
            "parallel/smm_static_cube_n3/np2/rank0",
            "parallel/smm_static_cube_n3/np2/rank1",
        ]
    );
}

/* ------------------------------------------------------------------------ */
/* Element-class identities                                                 */
/* ------------------------------------------------------------------------ */

fn element_cases() -> Vec<Fixture> {
    let root = fixtures_root().join("fe").join("element_class");
    all_cases(&root)
        .into_iter()
        .map(|d| Fixture::open(&d).unwrap())
        .collect()
}

fn param_str(fx: &Fixture, key: &str) -> String {
    fx.manifest().parameters[key]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn param_usize(fx: &Fixture, key: &str) -> usize {
    usize::try_from(fx.manifest().parameters[key].as_i64().unwrap()).unwrap()
}

fn orders(fx: &Fixture) -> Vec<usize> {
    fx.manifest().parameters["quadrature_orders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| usize::try_from(v.as_i64().unwrap()).unwrap())
        .collect()
}

/// Reference-element geometry family, derived from the element type name.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Family {
    Point,
    Segment,
    Square,
    Cube,
    Triangle,
    Tetrahedron,
    Prism,
}

fn family(fx: &Fixture) -> Family {
    let t = param_str(fx, "element_type");
    let nd = param_usize(fx, "natural_space_dimension");
    let tri = |s: &str| s.contains("triangle") || s == "_cohesive_3d_6" || s == "_cohesive_3d_12";
    match nd {
        0 => Family::Point,
        1 => Family::Segment,
        2 if tri(&t) => Family::Triangle,
        2 => Family::Square,
        3 if t.contains("tetrahedron") => Family::Tetrahedron,
        3 if t.contains("pentahedron") => Family::Prism,
        3 => Family::Cube,
        _ => unreachable!(),
    }
}

fn factorial(n: u32) -> f64 {
    (1..=n).map(f64::from).product()
}

/// Exact integral of xi^a eta^b zeta^c over the reference element.
fn reference_moment(f: Family, e: [u32; 3]) -> f64 {
    let seg = |a: u32| {
        if a % 2 == 1 {
            0.0
        } else {
            2.0 / f64::from(a + 1)
        }
    };
    let tri = |a: u32, b: u32| factorial(a) * factorial(b) / factorial(a + b + 2);
    match f {
        Family::Point => 1.0,
        Family::Segment => seg(e[0]),
        Family::Square => seg(e[0]) * seg(e[1]),
        Family::Cube => seg(e[0]) * seg(e[1]) * seg(e[2]),
        Family::Triangle => tri(e[0], e[1]),
        Family::Tetrahedron => {
            factorial(e[0]) * factorial(e[1]) * factorial(e[2]) / factorial(e[0] + e[1] + e[2] + 3)
        }
        // Akantu's prism: xi in [-1, 1] (segment) x (eta, zeta) in the triangle.
        Family::Prism => seg(e[0]) * tri(e[1], e[2]),
    }
}

#[test]
fn gauss_rules_integrate_polynomials_exactly() {
    let cases = element_cases();
    assert!(
        cases.len() >= 13,
        "expected all element types, found {}",
        cases.len()
    );
    let mut checked = 0;
    for fx in &cases {
        let fam = family(fx);
        let nd = param_usize(fx, "natural_space_dimension");
        for o in orders(fx) {
            let pts = fx.load_f64(&format!("quad_points/order_{o}")).unwrap();
            let w = fx.load_f64(&format!("quad_weights/order_{o}")).unwrap();
            let nq = w.data.len();
            let stride = pts.shape[1];
            // Akantu's pentahedron rules are documented as order 3 in x and
            // 2 in (y, z) for orders 1-2 (gauss_integration_tmpl.hh); check
            // the degree the rule actually guarantees.
            let degree = match (fam, o) {
                (Family::Prism, 1 | 2) => 2,
                (Family::Prism, 3) => 3,
                _ => o,
            };
            #[allow(clippy::cast_possible_truncation)] // degree <= 7
            let deg = degree as u32;
            for a in 0..=deg {
                for b in 0..=(if nd > 1 { deg - a } else { 0 }) {
                    for c in 0..=(if nd > 2 { deg - a - b } else { 0 }) {
                        let e = [a, b, c];
                        let mut s = 0.0;
                        for q in 0..nq {
                            let mut m = 1.0;
                            for (d, &ex) in e.iter().enumerate().take(nd) {
                                m *= pts.data[q * stride + d].powi(i32::try_from(ex).unwrap());
                            }
                            s += w.data[q] * m;
                        }
                        let exact = reference_moment(fam, e);
                        assert!(
                            (s - exact).abs() <= 1e-14 * exact.abs().max(1.0),
                            "{} order {o}: moment {e:?} = {s:e}, exact {exact:e}",
                            fx.case_id()
                        );
                        checked += 1;
                    }
                }
            }
        }
    }
    assert!(checked > 500, "only {checked} moments checked");
}

#[test]
fn shapes_form_a_partition_of_unity() {
    for fx in element_cases() {
        for o in orders(&fx) {
            let Ok(n) = fx.load_f64(&format!("shapes/order_{o}")) else {
                continue;
            };
            let nn = n.shape[1];
            for (q, row) in n.data.chunks(nn).enumerate() {
                let s: f64 = row.iter().sum();
                assert!(
                    (s - 1.0).abs() < 1e-14,
                    "{} order {o} quad {q}: sum N = {s:e}",
                    fx.case_id()
                );
            }
            if let Ok(d) = fx.load_f64(&format!("dnds/order_{o}")) {
                let nn = d.shape[2];
                for (k, row) in d.data.chunks(nn).enumerate() {
                    let s: f64 = row.iter().sum();
                    assert!(
                        s.abs() < 1e-13,
                        "{} order {o} row {k}: sum dN/ds = {s:e}",
                        fx.case_id()
                    );
                }
            }
        }
    }
}

#[test]
fn physical_element_identities() {
    for fx in element_cases() {
        let nd = param_usize(&fx, "natural_space_dimension");
        if nd == 0 || fx.quantity("physical/affine/jacobians").is_err() {
            continue;
        }
        // Affine map: constant Jacobian determinant.
        let j = fx.load_f64("physical/affine/jacobians").unwrap();
        let j0 = j.data[0];
        for v in &j.data {
            assert!(
                (v - j0).abs() <= 1e-14 * j0.abs(),
                "{}: affine detJ not constant",
                fx.case_id()
            );
        }
        // The volume (integral of 1) of an affine element is the same for
        // every rule, and equals |detJ| times the reference measure.
        let fam = family(&fx);
        for o in orders(&fx) {
            let ints = fx
                .load_f64(&format!("physical/affine/integrals/order_{o}"))
                .unwrap();
            let vol = j0 * reference_moment(fam, [0, 0, 0]);
            assert!(
                (ints.data[0] - vol).abs() <= 1e-13 * vol.abs(),
                "{} order {o}: volume",
                fx.case_id()
            );
        }
        // Inverse map recovers the natural coordinates.
        for tag in ["affine", "curved"] {
            let xi = fx
                .load_f64(&format!("physical/{tag}/inverse_map/xi"))
                .unwrap();
            let ex = fx
                .load_f64(&format!("physical/{tag}/inverse_map/xi_exact"))
                .unwrap();
            for (a, b) in xi.data.iter().zip(&ex.data) {
                assert!(
                    (a - b).abs() < 1e-9,
                    "{} {tag}: inverse map {a} vs {b}",
                    fx.case_id()
                );
            }
        }
        // Normals of embedded elements are unit vectors.
        if let Ok(n) = fx.load_f64("physical/embedded/normals") {
            let d = n.shape[1];
            for v in n.data.chunks(d) {
                let norm = v.iter().map(|x| x * x).sum::<f64>().sqrt();
                assert!((norm - 1.0).abs() < 1e-14, "{}: |n| = {norm}", fx.case_id());
            }
        }
    }
}

/// Every `f64` quantity of every golden fixture resolves to a justified
/// rule **that can be applied to the quantity's axes**: a grouped scale
/// (`max_abs_per_component`, `max_abs_per_step`, `max_abs_per_group`) on an
/// axis the quantity does not have is rejected (at the
/// version before the fix 451 rules silently degraded to a whole-array scale).
#[test]
fn every_floating_point_quantity_has_a_justified_applicable_tolerance() {
    let root = fixtures_root();
    let mut bad = Vec::new();
    let mut checked = 0usize;
    for dir in all_cases(&root) {
        let fx = Fixture::open(&dir).unwrap();
        if fx.manifest().status != FixtureStatus::Golden {
            continue;
        }
        for q in &fx.manifest().quantities {
            if q.parsed_dtype() != Some(Dtype::F64) {
                continue;
            }
            let t = fx
                .tolerance(&q.name)
                .unwrap_or_else(|e| panic!("{}: {e}", fx.case_id()));
            assert!(!t.spec.justification.trim().is_empty());
            let ragged_row = q
                .ragged
                .as_ref()
                .filter(|_| q.layout == Layout::RaggedValues)
                .map(|r| r.row_axis.as_str());
            if let Err(e) = t.spec.check_axes(&q.name, &q.axes, ragged_row) {
                bad.push(format!(
                    "{} :: {} ({}): {e}",
                    fx.case_id(),
                    q.name,
                    t.origin
                ));
            }
            checked += 1;
        }
    }
    assert!(checked > 1000, "only {checked} f64 quantities found");
    assert!(
        bad.is_empty(),
        "{} rule(s) cannot be applied to their quantity:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

/// Cases whose purpose is to carry explicit special values (NaN among
/// them) through a format: the NaNs are inputs, not results.
const SPECIAL_VALUE_CASES: [&str; 3] = [
    // writer self-tests: special doubles written by the C++/Python writers
    "selftest/cpp_writer_roundtrip",
    "selftest/python_writer_roundtrip",
    // std::ostream << double / saveMatrix / saveVector of ±nan, ±inf, ±0
    "solver/text/matrix_market",
];

/// NaNs of golden cases whose manifests carry no structured NaN inventory
/// (cases merged from main): `(case, quantity, NaN count, why)`. Each must
/// be a default NaN, with exactly these counts and no other NaN-bearing
/// quantity in the case. Documented in `docs/NUMERICAL_VALIDATION.md`;
/// the manifests should gain the `notes` inventory at their next
/// regeneration (fixtures are not edited here).
const NAN_REGISTERED: [(&str, &str, usize, &str); 22] = [
    (
        "core/common/bbox_emptiness",
        "contains/points",
        1,
        "input: a NaN coordinate among the query points of BBox::contains",
    ),
    (
        "core/common/math",
        "newton/x",
        1,
        "sentinel written by the probe when Math::NewtonRaphson threw (quantity description)",
    ),
    (
        "geometry/adversarial/kernel_adv",
        "coords",
        2070,
        "oracle behaviour: CGAL Simple_cartesian constructions overflow to ±inf, then inf − inf (prose notes; x86_64 record)",
    ),
    (
        "material/anisotropic_damage_mazars/1d_uniaxial",
        "stress",
        4,
        "upstream output from step index 16 (UD-031; prose notes)",
    ),
    (
        "material/anisotropic_damage_mazars/1d_uniaxial",
        "energy/potential",
        4,
        "upstream output from step index 16 (UD-031; prose notes)",
    ),
    (
        "material/anisotropic_damage_mazars/2d_uniaxial",
        "stress",
        16,
        "upstream output from step index 16 (UD-031; prose notes)",
    ),
    (
        "material/anisotropic_damage_mazars/2d_uniaxial",
        "tangent",
        57,
        "upstream output from step index 8 (UD-031; prose notes)",
    ),
    (
        "material/anisotropic_damage_mazars/2d_uniaxial",
        "energy/potential",
        4,
        "upstream output from step index 16 (UD-031; prose notes)",
    ),
    // Split-node contact cases (the manifests document each NaN in prose
    // notes).
    (
        "model/ntn_contact/adv_3d_ras_reversal",
        "frictional_strength",
        633,
        "upstream output from step 96: unstable explicit ageing update, ln of a negative state (UD-039; structured prose in the manifest notes)",
    ),
    (
        "model/ntn_contact/adv_restart_continue",
        "b_after_read/contact_pressure",
        22,
        "std::numeric_limits<Real>::quiet_NaN() initial value never restored by upstream's readRestart (UD-394; manifest notes)",
    ),
    (
        "model/ntn_contact/law_coulomb__spc_tstar0",
        "frictional_strength",
        1650,
        "upstream output: Prakash-Clifton regularisation with t_star = 0 gives inf/inf (UD-397; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "cumulative_slip",
        1529,
        "upstream output from step 11: Dieterich coefficient at rest is -inf (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "external_work",
        278,
        "upstream output from step 11 (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "friction_state",
        1529,
        "upstream output from step 11 (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "friction_traction",
        3069,
        "upstream output from step 10 (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "frictional_strength",
        110,
        "upstream output, steps 0-9: -inf coefficient times a zero contact pressure (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "slip",
        1529,
        "upstream output from step 11 (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "slip_velocity_norm",
        1529,
        "upstream output from step 11 (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/law_dieterich_ageing_at_rest__none",
        "displacement_every_10",
        5788,
        "upstream output from sample 1 = step 10 (UD-039; manifest notes)",
    ),
    (
        "model/ntn_contact/miiasym_fields",
        "pressure_after_noop",
        22,
        "std::numeric_limits<Real>::quiet_NaN() initial contact pressure; MIIASYMContact::computeContactPressure is a no-op by design (manifest notes)",
    ),
    (
        "model/ntn_contact/ras_revised_n_shaped_slip",
        "sampled/friction_state",
        985,
        "upstream output from sample 1: slip state law 0*(-inf) at V = 0 (UD-405; manifest notes)",
    ),
    (
        "model/ntn_contact/ras_revised_n_shaped_slip",
        "sampled/frictional_strength",
        985,
        "upstream output from sample 1: NaN state gives a NaN strength (UD-405; manifest notes)",
    ),
];

/// `(quantity, NaN count)` pairs and the stated total of a NaN inventory.
type NanInventory = (Vec<(String, usize)>, Option<usize>);

/// The structured NaN inventory of a case's `notes` (D-046 (4)): the
/// segment `NaN quantities (count of N values): q1 (n1), q2 (n2), …;`
/// (up to the first `;`). Returns `(quantity, count)` pairs and the stated
/// total, or `None` if the notes have no such segment. Quantity names are
/// whole tokens (substring matching let a NaN planted
/// in `stress/_hexahedron_20` pass through `cauchy/stress/_hexahedron_20`).
fn nan_inventory(notes: &str) -> Option<NanInventory> {
    let count = |t: &str| t.trim().replace(',', "").parse::<usize>().ok();
    let start = notes.find("NaN quantities")?;
    let rest = &notes[start + "NaN quantities".len()..];
    let colon = rest.find(':')?;
    let total = rest[..colon]
        .trim()
        .strip_prefix("(count of ")
        .and_then(|t| t.strip_suffix(" values)"))
        .and_then(count);
    let list = &rest[colon + 1..];
    let list = &list[..list.find(';').unwrap_or(list.len())];
    let mut out = Vec::new();
    for item in list.split("),") {
        let item = item.trim().trim_end_matches(')');
        let open = item.rfind(" (")?;
        out.push((item[..open].trim().to_owned(), count(&item[open + 2..])?));
    }
    Some((out, total))
}

#[test]
fn nan_inventory_parses_whole_names() {
    let (q, total) = nan_inventory(
        "x. NaN quantities (count of 2,036 values): displacement (21), \
         cauchy/stress/_hexahedron_20 (576), energy/potential (1); stress/_hexahedron_20 (zero)",
    )
    .unwrap();
    assert_eq!(total, Some(2036));
    assert_eq!(
        q,
        [
            ("displacement".to_owned(), 21),
            ("cauchy/stress/_hexahedron_20".to_owned(), 576),
            ("energy/potential".to_owned(), 1)
        ]
    );
    assert!(!q.iter().any(|(n, _)| n == "stress/_hexahedron_20"));
    assert!(nan_inventory("no inventory here").is_none());
}

/// Decision D-046 (4): a golden case holding NaN in a floating-point
/// quantity documents it in its manifest `notes` (which quantities, from
/// which step, why); an undocumented NaN is a defect of the fixture
/// (`model/smm_fd/explicit_patch_hexahedron_20` held 2,036
/// undocumented NaNs). The notes carry the structured inventory
/// [`nan_inventory`]: every NaN-bearing quantity is listed by its whole
/// name with its exact NaN count, every listed quantity holds exactly that
/// many NaNs, the counts add up to the stated total, and every documented
/// NaN is a default quiet NaN (D-046 (1); a payload or signalling NaN is
/// not covered by the convention). Scope: the manifest corpus under
/// `fixtures/` (golden cases; external cases only when materialised);
/// crate-local oracle tables are listed in NUMERICAL_VALIDATION
/// "Default-NaN convention (D-046)".
#[test]
fn every_nan_in_a_golden_fixture_is_documented() {
    let root = fixtures_root();
    let mut undocumented = Vec::new();
    let mut documented = Vec::new();
    for dir in all_cases(&root) {
        let fx = Fixture::open(&dir).unwrap();
        let m = fx.manifest();
        if m.status != FixtureStatus::Golden
            || (m.storage.mode == StorageMode::External && fx.check_materialized().is_err())
        {
            continue;
        }
        if SPECIAL_VALUE_CASES.contains(&m.case_id.as_str()) {
            continue;
        }
        // NaN count and non-default NaNs of every f64 quantity
        let mut nans: Vec<(&str, usize, usize)> = Vec::new();
        for q in m
            .quantities
            .iter()
            .filter(|q| q.parsed_dtype() == Some(Dtype::F64))
        {
            let data = fx.load_f64(&q.name).unwrap_or_else(|e| panic!("{e}")).data;
            let n = data.iter().filter(|x| x.is_nan()).count();
            let other = data
                .iter()
                .filter(|x| x.is_nan() && !akantu_validation::exact::is_default_nan(**x))
                .count();
            if n > 0 {
                nans.push((q.name.as_str(), n, other));
            }
        }
        let notes = m.notes.as_deref().unwrap_or("");
        let inventory = nan_inventory(notes);
        if nans.is_empty() && inventory.is_none() {
            continue;
        }
        let id = &m.case_id;
        let registered: Vec<_> = NAN_REGISTERED.iter().filter(|(c, ..)| c == id).collect();
        if inventory.is_none() && !registered.is_empty() {
            let same = nans.len() == registered.len()
                && nans.iter().all(|&(q, n, other)| {
                    other == 0 && registered.iter().any(|(_, rq, rn, _)| *rq == q && *rn == n)
                });
            if same {
                documented.push(format!("{id} (registered)"));
            } else {
                undocumented.push(format!(
                    "{id}: NaN {nans:?} differ from the registered NaNs"
                ));
            }
            continue;
        }
        let Some((listed, total)) = inventory else {
            undocumented.push(format!("{id}: NaN in {nans:?} without a NaN inventory"));
            continue;
        };
        let mut problems = Vec::new();
        for &(q, n, other) in &nans {
            match listed.iter().find(|(name, _)| name == q) {
                None => problems.push(format!("{q}: {n} NaN, not listed")),
                Some((_, c)) if *c != n => problems.push(format!("{q}: {n} NaN, listed {c}")),
                Some(_) => {}
            }
            if other > 0 {
                problems.push(format!("{q}: {other} NaN(s) that are not default NaNs"));
            }
        }
        for (name, c) in &listed {
            if !nans.iter().any(|(q, _, _)| q == name) {
                problems.push(format!("{name}: listed with {c} NaN, holds none"));
            }
        }
        let sum: usize = listed.iter().map(|(_, c)| c).sum();
        if total.is_some_and(|t| t != sum) {
            problems.push(format!("stated total {total:?} != listed sum {sum}"));
        }
        if problems.is_empty() {
            documented.push(format!("{id} ({sum} NaN)"));
        } else {
            undocumented.push(format!("{id}: {}", problems.join("; ")));
        }
    }
    eprintln!("golden cases with documented NaN: {documented:?}");
    assert!(
        undocumented.is_empty(),
        "golden fixtures with undocumented NaN (D-046):\n{}",
        undocumented.join("\n")
    );
    documented.sort();
    assert_eq!(
        documented,
        [
            "core/common/bbox_emptiness (registered)",
            "core/common/math (registered)",
            "geometry/adversarial/kernel_adv (registered)",
            "material/anisotropic_damage_mazars/1d_uniaxial (registered)",
            "material/anisotropic_damage_mazars/2d_uniaxial (registered)",
            "material/anisotropic_damage_mazars/variant_ud031/1d_uniaxial (12 NaN)",
            "material/anisotropic_damage_mazars/variant_ud031/2d_uniaxial (93 NaN)",
            "material/anisotropic_damage_mazars/variant_ud031/3d_uniaxial (211 NaN)",
            // adversarial cases: explicit solves
            // dividing by a zero lumped capacity, by design (notes)
            "model/diffusion/v102_ud281_lumped (10 NaN)",
            "model/diffusion/v102_zerorho_explicit (17 NaN)",
            "model/non_local/avg_plate_dbands_r0p5 (350 NaN)",
            "model/non_local/avg_plate_dnear1_r0p5 (322 NaN)",
            "model/non_local/avg_plate_dnear1_r5 (690 NaN)",
            "model/ntn_contact/adv_3d_ras_reversal (registered)",
            "model/ntn_contact/adv_restart_continue (registered)",
            "model/ntn_contact/law_coulomb__spc_tstar0 (registered)",
            "model/ntn_contact/law_dieterich_ageing_at_rest__none (registered)",
            "model/ntn_contact/miiasym_fields (registered)",
            "model/ntn_contact/ras_revised_n_shaped_slip (registered)",
            "model/smm_fd/explicit_patch_hexahedron_20 (2036 NaN)",
        ],
        "the golden cases documenting NaN (D-046 (4))"
    );
}

/* ------------------------------------------------------------------------ */
/* Model, material and mesh identities                                      */
/* ------------------------------------------------------------------------ */

fn cases_under(sub: &str) -> Vec<Fixture> {
    let root = sub.split('/').fold(fixtures_root(), |p, s| p.join(s));
    all_cases(&root)
        .into_iter()
        .map(|d| Fixture::open(&d).unwrap())
        .collect()
}

/// Static patch tests (the loaded cases have no affine field).
fn patch_cases() -> Vec<Fixture> {
    let cases: Vec<Fixture> = cases_under("model/static")
        .into_iter()
        .filter(|f| f.case_id().contains("/patch_"))
        .collect();
    assert!(cases.len() >= 4);
    cases
}

#[test]
fn static_patch_tests_reproduce_the_affine_field() {
    for fx in patch_cases() {
        let p = &fx.manifest().parameters;
        let a: Vec<Vec<f64>> = serde_json::from_value(p["A"].clone()).unwrap();
        let b: Vec<f64> = serde_json::from_value(p["b"].clone()).unwrap();
        let x = fx.load_f64("mesh/nodes").unwrap();
        let u = fx.load_f64("displacement").unwrap();
        let dim = x.shape[1];
        let umax = u.data.iter().fold(0.0f64, |m, v| m.nan_max(v.abs()));
        for (n, xn) in x.data.chunks(dim).enumerate() {
            for i in 0..dim {
                let exact: f64 = b[i] + (0..dim).map(|j| a[i][j] * xn[j]).sum::<f64>();
                let got = u.data[n * dim + i];
                assert!(
                    (got - exact).abs() <= 1e-13 * umax,
                    "{} node {n} comp {i}: {got:e} vs {exact:e}",
                    fx.case_id()
                );
            }
        }
    }
}

/// Young's modulus, Poisson ratio and plane-stress flag from a material file.
fn elastic_params(path: &Path) -> (f64, f64, bool) {
    let text = std::fs::read_to_string(path).unwrap();
    let get = |key: &str| -> Option<f64> {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().parse().ok())?
        })
    };
    let ps = text
        .lines()
        .any(|l| l.replace(' ', "").to_lowercase() == "plane_stress=true");
    (get("E").unwrap(), get("nu").unwrap(), ps)
}

#[test]
fn elastic_material_points_follow_hookes_law() {
    let cases = cases_under("material/elastic");
    assert!(!cases.is_empty());
    for fx in cases {
        let (e, nu, plane_stress) = elastic_params(&fx.input_path("material").unwrap());
        let mu = e / (2.0 * (1.0 + nu));
        let lambda = if plane_stress {
            nu * e / ((1.0 + nu) * (1.0 - nu))
        } else {
            nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu))
        };
        let g = fx.load_f64("grad_u").unwrap();
        let s = fx.load_f64("stress").unwrap();
        let d = g.shape[1];
        let smax = s.data.iter().fold(0.0f64, |m, v| m.nan_max(v.abs()));
        for (k, (gk, sk)) in g.data.chunks(d * d).zip(s.data.chunks(d * d)).enumerate() {
            let tr: f64 = (0..d).map(|i| gk[i * d + i]).sum();
            for i in 0..d {
                for j in 0..d {
                    let eps = 0.5 * (gk[i * d + j] + gk[j * d + i]);
                    // MaterialElastic<1> is specialised: sigma = E eps.
                    let want = if d == 1 {
                        e * eps
                    } else {
                        2.0 * mu * eps + if i == j { lambda * tr } else { 0.0 }
                    };
                    assert!(
                        (sk[i * d + j] - want).abs() <= 1e-14 * smax,
                        "{} step {k} ({i},{j}): {} vs {want}",
                        fx.case_id(),
                        sk[i * d + j]
                    );
                }
            }
        }
    }
}

#[test]
fn neohookean_is_recorded_as_not_implemented() {
    // Inventory MAT-025 predicted it; the oracle confirms it.
    let cases = cases_under("material/neohookean");
    assert!(!cases.is_empty());
    for fx in cases {
        let ex = &fx.manifest().expect;
        assert_eq!(
            ex.status,
            akantu_validation::manifest::ExpectedStatus::Error
        );
        assert!(
            ex.error_kind
                .as_deref()
                .unwrap_or("")
                .contains("NotImplementedException")
        );
    }
}

#[test]
fn mesh_connectivities_reference_existing_nodes() {
    let cases = cases_under("mesh_io");
    assert!(
        cases.len() >= 80,
        "expected every upstream test mesh, found {}",
        cases.len()
    );
    for fx in cases {
        if fx.check_materialized().is_err() {
            continue; // external
        }
        let n_nodes = fx.load_f64("nodes").unwrap().shape[0];
        for q in fx.quantity_names("connectivity/*") {
            let c = fx.load_i64(q).unwrap();
            assert!(
                c.data
                    .iter()
                    .all(|&v| v >= 0 && usize::try_from(v).unwrap() < n_nodes),
                "{} {q}: node index out of range",
                fx.case_id()
            );
        }
    }
}

/* ------------------------------------------------------------------------ */
/* Regression checks on the oracle data                                     */
/* ------------------------------------------------------------------------ */

/// Bit-exact sequential 2-norm over free DOFs (Akantu's testConvergence).
fn free_norm(v: &[f64], blocked: &[bool]) -> f64 {
    v.iter()
        .zip(blocked)
        .filter(|(_, b)| !**b)
        .fold(0.0, |acc, (x, _)| acc + x * x)
        .sqrt()
}

fn blocked(fx: &Fixture) -> Vec<bool> {
    let b = fx.load("blocked_dofs").unwrap();
    match b.data {
        akantu_validation::npy::NpyData::Bool(v) => v,
        other => panic!("blocked_dofs is not boolean: {other:?}"),
    }
}

/// Tensors are stored `[row, col]` = M(row, col). The patch tests
/// impose u = A X + b with an *asymmetric* A, so the material's grad_u at
/// every integration point (several per element for quadrangle_8 and
/// hexahedron_20) must equal A, not Aᵀ (the probe v1 reshape produced Aᵀ).
#[test]
fn static_patch_gradients_equal_the_asymmetric_affine_matrix() {
    for fx in patch_cases() {
        let p = &fx.manifest().parameters;
        let a: Vec<Vec<f64>> = serde_json::from_value(p["A"].clone()).unwrap();
        let dim = a.len();
        let amax = a.iter().flatten().fold(0.0f64, |m, v| m.nan_max(v.abs()));
        assert!(
            (0..dim).any(|i| (0..dim).any(|j| (a[i][j] - a[j][i]).abs() > 0.1 * amax)),
            "{}: A must be asymmetric to test the layout",
            fx.case_id()
        );
        let names: Vec<String> = fx
            .quantity_names("grad_u/*")
            .into_iter()
            .map(String::from)
            .collect();
        assert!(!names.is_empty(), "{}: no grad_u", fx.case_id());
        for q in names {
            let g = fx.load_f64(&q).unwrap();
            assert_eq!(&g.axes[2..], &["row".to_string(), "col".to_string()]);
            let nq = g.shape[1];
            if fx.case_id().contains("quadrangle_8") || fx.case_id().contains("hexahedron_20") {
                assert!(nq > 1, "{q}: several quadrature points expected");
            }
            for (k, t) in g.data.chunks(dim * dim).enumerate() {
                for r in 0..dim {
                    for c in 0..dim {
                        assert!(
                            (t[r * dim + c] - a[r][c]).abs() <= 1e-13 * amax,
                            "{} {q} tuple {k} ({r},{c}): {} vs A = {}",
                            fx.case_id(),
                            t[r * dim + c],
                            a[r][c]
                        );
                    }
                }
            }
        }
    }
}

/// The sentinel round trip through the real bindings. M(r, c) of
/// tuple k was written at buffer component r + c*dim with value
/// 1000 k + 10 r + c + 1; record_stress must read it back as [.., r, c].
#[test]
fn tensor_layout_sentinel_round_trip() {
    let cases = cases_under("model/layout");
    assert_eq!(cases.len(), 2);
    for fx in cases {
        let dim = usize::try_from(
            fx.manifest().parameters["spatial_dimension"]
                .as_i64()
                .unwrap(),
        )
        .unwrap();
        for q in fx.quantity_names("stress/*") {
            let s = fx.load_f64(q).unwrap();
            let raw = fx
                .load_f64(&q.replacen("stress/", "stress_buffer/", 1))
                .unwrap();
            let (ne, nq) = (s.shape[0], s.shape[1]);
            assert!(
                ne > 1 && nq > 1,
                "{q}: several tuples and quadrature points"
            );
            for k in 0..ne * nq {
                for r in 0..dim {
                    for c in 0..dim {
                        #[allow(clippy::cast_precision_loss)] // small integers
                        let want = 1000.0 * k as f64 + 10.0 * r as f64 + c as f64 + 1.0;
                        assert_eq!(s.data[(k * dim + r) * dim + c], want, "{q} {k} ({r},{c})");
                        assert_eq!(raw.data[k * dim * dim + r + c * dim], want);
                    }
                }
            }
        }
    }
}

/// Akantu's static residual is f_ext + f_int (both coefficients +1,
/// solid_mechanics_model.cc:245-257). Under a non-zero load it vanishes on
/// free DOFs, while f_ext - f_int (the probe v1 formula) is ~2 f_ext.
#[test]
fn loaded_equilibrium_residual_is_f_ext_plus_f_int() {
    let cases: Vec<Fixture> = cases_under("model/static")
        .into_iter()
        .filter(|f| f.case_id().contains("/loaded_"))
        .collect();
    assert_eq!(cases.len(), 2);
    for fx in cases {
        let fe = fx.load_f64("external_force").unwrap().data;
        let fi = fx.load_f64("internal_force").unwrap().data;
        let r = fx.load_f64("residual").unwrap().data;
        let b = blocked(&fx);
        for ((x, y), z) in fe.iter().zip(&fi).zip(&r) {
            assert_eq!(x + y, *z, "{}: residual is not f_ext + f_int", fx.case_id());
        }
        let load = free_norm(&fe, &b);
        assert!(load > 1e5, "{}: the case must be loaded", fx.case_id());
        let res = free_norm(&r, &b);
        assert!(
            res <= 1e-12 * load,
            "{}: |f_ext + f_int| = {res:e}",
            fx.case_id()
        );
        let wrong: Vec<f64> = fe.iter().zip(&fi).map(|(x, y)| x - y).collect();
        assert!(free_norm(&wrong, &b) >= 1.9 * load, "{}", fx.case_id());
        // Global equilibrium: f_int is self-equilibrated, so the reactions
        // (residual on blocked DOFs) balance the load component-wise.
        let dim = fx.load_f64("displacement").unwrap().shape[1];
        for c in 0..dim {
            let total: f64 = r.iter().skip(c).step_by(dim).sum();
            let applied: f64 = fe.iter().skip(c).step_by(dim).sum();
            assert!(
                (total - applied).abs() <= 1e-9 * load,
                "{} comp {c}",
                fx.case_id()
            );
        }
    }
}

/// The implicit dynamic residual history is Akantu's own
/// convergence residual: nb_iterations + 1 entries per step, the last equal
/// to the solver error bit for bit, below threshold * first. The recorded
/// pre-inertia part f_ext + f_int equals M a on free DOFs up to that error
/// (full residual f_ext + f_int - M a); with the wrong sign it would be
/// ~2 |M a|.
#[test]
fn implicit_dynamic_residuals_match_the_solver() {
    let fx = Fixture::open_by_id("model/implicit_dynamic/patch_triangle_6_plastic").unwrap();
    let p = &fx.manifest().parameters;
    let threshold = p["solver"]["threshold"].as_f64().unwrap();
    let iters = fx.load_i64("newton/nb_iterations").unwrap().data;
    let error = fx.load_f64("newton/error").unwrap().data;
    let full = fx.load_ragged_f64("newton/residual_norm").unwrap();
    let stat = fx.load_ragged_f64("newton/static_residual_norm").unwrap();
    let last = fx.load_f64("newton/static_residual_last").unwrap();
    let acc = fx.load_f64("acceleration").unwrap();
    let b = blocked(&fx);
    let rows = fx.load_i64("M/rows").unwrap().data;
    let cols = fx.load_i64("M/cols").unwrap().data;
    let vals = fx.load_f64("M/values").unwrap().data;
    let symmetric = p["M"]["symmetric_storage"].as_bool().unwrap();
    let n = b.len();
    for s in 0..iters.len() {
        let h = &full[s];
        assert_eq!(h.len(), usize::try_from(iters[s]).unwrap() + 1, "step {s}");
        assert_eq!(h.last().unwrap().to_bits(), error[s].to_bits(), "step {s}");
        assert!(*h.last().unwrap() < threshold * h[0], "step {s}");
        assert_eq!(stat[s].len(), h.len(), "one static norm per assembly");
        // M a with the bit-exact consistent mass matrix.
        let a = &acc.data[s * n..(s + 1) * n];
        let mut ma = vec![0.0; n];
        for ((&i, &j), &v) in rows.iter().zip(&cols).zip(&vals) {
            let (i, j) = (usize::try_from(i).unwrap(), usize::try_from(j).unwrap());
            ma[i] += v * a[j];
            if symmetric && i != j {
                ma[j] += v * a[i];
            }
        }
        let st = &last.data[s * n..(s + 1) * n];
        let st_norm = free_norm(st, &b);
        assert_eq!(
            st_norm.to_bits(),
            stat[s].last().unwrap().to_bits(),
            "step {s}"
        );
        let diff: Vec<f64> = st.iter().zip(&ma).map(|(x, y)| x - y).collect();
        let slack = 1e-12 * (st_norm + free_norm(&ma, &b));
        assert!(
            free_norm(&diff, &b) <= error[s] + slack,
            "step {s}: |f_ext + f_int - M a| = {:e} > error {:e}",
            free_norm(&diff, &b),
            error[s]
        );
        let wrong: Vec<f64> = st.iter().zip(&ma).map(|(x, y)| -x - y).collect();
        assert!(free_norm(&wrong, &b) > 1.9 * st_norm, "step {s}");
    }
}

/// The tolerance justifications cite measured conditioning; the
/// recorded measurements must stay within the cited values. The normwise
/// backward error of the oracle's MUMPS solve depends on the OpenBLAS
/// kernels (class X2, D-073): the justification texts cite the maxima of
/// both recorded environments (aarch64 8.32e-17, x86_64 `Haswell` 1.26e-16),
/// and each loaded golden is checked against the maximum
/// cited for ITS environment (`Fixture::environment`, not `cfg`). The
/// derived bounds use `eta_oracle + eta_port` with `eta_port <= 10 u`
/// dominating, so the rule values do not depend on the platform.
#[test]
fn recorded_conditioning_supports_the_tolerance_justifications() {
    for fx in cases_under("model/static") {
        let c = &fx.manifest().parameters["conditioning"];
        let kappa = c["cond2_K_free"].as_f64().unwrap();
        let eta = c["backward_error"].as_f64().unwrap();
        let bound = if fx.case_id().contains("/loaded_") {
            312.5
        } else {
            10.8
        };
        let eta_max = match fx.environment().as_str() {
            "aarch64-auto" => 8.4e-17,
            "x86_64-haswell" => 1.3e-16,
            env => panic!("{}: no cited backward error for {env}", fx.case_id()),
        };
        assert!((1.0..=bound).contains(&kappa), "{}: {kappa}", fx.case_id());
        assert!(
            eta <= eta_max,
            "{} ({}): backward error {eta:e} > {eta_max:e}",
            fx.case_id(),
            fx.environment()
        );
    }
    let fx = Fixture::open_by_id("model/implicit_dynamic/patch_triangle_6_plastic").unwrap();
    let p = &fx.manifest().parameters;
    for k in p["conditioning"]["cond2_J_free_per_step"]
        .as_array()
        .unwrap()
    {
        assert!(k.as_f64().unwrap() < 30.0);
    }
    for r in p["newton_contraction"]["max_ratio_per_step"]
        .as_array()
        .unwrap()
    {
        assert!(r.as_f64().unwrap() < 0.35);
    }
}

#[path = "../examples/regression_seed.rs"]
mod regression_seed;

/// One real replay. The committed golden case was produced by
/// persisting `regression_seed::seed()` and running its stored command
/// (`generate-fixtures.sh regression:<case_id>`) through the material
/// probe; re-persisting the seed must give bit-identical inputs.
#[test]
fn regression_replay_round_trip() {
    use akantu_validation::regression::{persist_regression_case, replay_command};
    let dir = tempfile::tempdir().unwrap();
    let pending =
        Fixture::open(&persist_regression_case(dir.path(), &regression_seed::seed()).unwrap())
            .unwrap();
    assert_eq!(pending.manifest().status, FixtureStatus::PendingOracle);
    let cmd = replay_command(regression_seed::CASE_ID);
    assert_eq!(pending.manifest().generator.command, cmd);
    let golden = Fixture::open_by_id(regression_seed::CASE_ID).unwrap();
    golden.verify_checksums().unwrap();
    let m = golden.manifest();
    assert_eq!(m.status, FixtureStatus::Golden);
    assert_eq!(m.generator.probe, "material_probe");
    assert_eq!(
        m.generator.command, cmd,
        "replayed with the persisted command"
    );
    let replayed = m.inputs.iter().find(|i| i.role == "grad_u").unwrap();
    assert_eq!(
        replayed.sha256,
        pending.quantity("grad_u").unwrap().sha256,
        "the oracle consumed exactly the persisted input"
    );
    // The prescribed history is echoed exactly, and the elastic stress obeys
    // Hooke's law (plane strain) on the recorded gradient.
    let h = golden.load_f64("input/grad_u_prescribed").unwrap();
    assert_eq!(h.data, regression_seed::GRAD_U.to_vec());
    let (e, nu) = (1e9, 0.25);
    let mu = e / (2.0 * (1.0 + nu));
    let lambda = nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu));
    let g = golden.load_f64("grad_u").unwrap().data;
    let s = golden.load_f64("stress").unwrap().data;
    let smax = s.iter().fold(0.0f64, |m, v| m.nan_max(v.abs()));
    for (gk, sk) in g.chunks(4).zip(s.chunks(4)) {
        let tr = gk[0] + gk[3];
        for (i, j) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            let eps = 0.5 * (gk[i * 2 + j] + gk[j * 2 + i]);
            let want = 2.0 * mu * eps + if i == j { lambda * tr } else { 0.0 };
            assert!((sk[i * 2 + j] - want).abs() <= 1e-14 * smax);
        }
    }
}

/// On a real error-path fixture: checking its (empty) stress without
/// observing the expected NotImplementedException must not pass.
#[test]
fn error_fixture_needs_the_expected_error_observed() {
    use akantu_validation::DiffTest;
    let out = tempfile::tempdir().unwrap();
    let mut t = DiffTest::new(
        "oracle::neohookean_unobserved",
        "material/neohookean/2d_composed",
    )
    .unwrap()
    .with_output_root(out.path());
    assert!(t.expects_error());
    assert!(t.check_f64("stress", &[]));
    let f = t.finish().unwrap_err();
    assert!(f.to_string().contains("did not observe one"), "{f}");
    let mut t = DiffTest::new(
        "oracle::neohookean_observed",
        "material/neohookean/2d_composed",
    )
    .unwrap()
    .with_output_root(out.path());
    assert!(t.observe_error(
        "akantu::debug::NotImplementedException",
        "computeStress : not implemented yet !"
    ));
    assert!(t.finish().is_ok());
}

/// The tangent is stored in Akantu's Voigt order as
/// recorded in `parameters.voigt_order` (1D xx; 2D xx yy xy; 3D xx yy zz yz
/// xz xy, engineering shear strains). The elastic tangent must be the
/// isotropic matrix in exactly that order.
#[test]
fn elastic_tangent_follows_the_documented_voigt_order() {
    let mut checked = 0;
    for fx in cases_under("material/elastic") {
        let (e, nu, plane_stress) = elastic_params(&fx.input_path("material").unwrap());
        let p = &fx.manifest().parameters;
        let dim = usize::try_from(p["spatial_dimension"].as_i64().unwrap()).unwrap();
        let order: Vec<String> = serde_json::from_value(p["voigt_order"].clone()).unwrap();
        let expected_order: &[&str] = match dim {
            1 => &["xx"],
            2 => &["xx", "yy", "xy"],
            _ => &["xx", "yy", "zz", "yz", "xz", "xy"],
        };
        assert_eq!(order, expected_order, "{}", fx.case_id());
        let mu = e / (2.0 * (1.0 + nu));
        let lambda = if plane_stress {
            nu * e / ((1.0 + nu) * (1.0 - nu))
        } else {
            nu * e / ((1.0 + nu) * (1.0 - 2.0 * nu))
        };
        let n = order.len();
        let want = |r: usize, c: usize| -> f64 {
            if dim == 1 {
                return e;
            }
            let normal = |i: usize| i < dim;
            match (normal(r), normal(c)) {
                (true, true) => lambda + if r == c { 2.0 * mu } else { 0.0 },
                (false, false) if r == c => mu,
                _ => 0.0,
            }
        };
        let t = fx.load_f64("tangent").unwrap();
        assert_eq!(&t.shape[1..], &[n, n]);
        let tmax = t.data.iter().fold(0.0f64, |m, v| m.nan_max(v.abs()));
        for (k, step) in t.data.chunks(n * n).enumerate() {
            for r in 0..n {
                for c in 0..n {
                    assert!(
                        (step[r * n + c] - want(r, c)).abs() <= 1e-14 * tmax,
                        "{} step {k} ({}, {}): {} vs {}",
                        fx.case_id(),
                        order[r],
                        order[c],
                        step[r * n + c],
                        want(r, c)
                    );
                }
            }
        }
        checked += 1;
    }
    assert!(checked >= 10);
}
