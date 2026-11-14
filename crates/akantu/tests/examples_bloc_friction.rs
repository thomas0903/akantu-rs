// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/bloc_friction (port of
// examples/c++/contact_mechanics_model/friction/bloc_friction.cc, upstream
// ctest `bloc_friction-test`).

#[path = "../examples/bloc_friction/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod bloc_friction;
mod examples_common;

use std::collections::BTreeMap;

use akantu::model::Model;
use examples_common::{oracle, scratch};

/// Every 30th step of the oracle variant `bloc_friction_full`: step →
/// `[nb_contact, epot, ekin, sha256(displacement|velocity|gaps)]`.
fn oracle_records() -> BTreeMap<usize, Vec<String>> {
    let o = oracle::oracle("bloc_friction_full");
    oracle::kept_records(
        &o,
        "energy_full.csv",
        "step,nb_contact,potential,kinetic,sha256(displacement|velocity|gaps)",
        ',',
    )
    .into_iter()
    .map(|r| (r[0].parse().unwrap(), r[1..].to_vec()))
    .collect()
}

/// A run with upstream's contact defects against the oracle variant
/// (every 30th step: contact elements, energies bitwise, full state);
/// after every step of the compression phase it also checks that the
/// `loading` edge carries the imposed increments (`u_y = −(k + 1) / 10000`
/// accumulated as the boundary condition accumulates them, `u_x = 0`).
/// Returns the run and the number of compared steps.
fn run_compared(
    out: &std::path::Path,
    steps: Option<usize>,
) -> (bloc_friction::Outcome, String, usize) {
    let want = oracle_records();
    let mut ex = bloc_friction::example().for_test(out);
    ex.steps = steps;
    ex.upstream_compat = true;
    let mut compared = 0;
    let mut loading: Vec<usize> = Vec::new();
    let mut imposed = 0.0;
    let r = bloc_friction::run_with(&ex, |k, c| {
        if k < 10000 {
            if loading.is_empty() {
                let mesh = c.solid.mesh().read()?;
                let g = mesh.groups().element_group("loading")?;
                loading = g
                    .nodes()
                    .as_slice()
                    .iter()
                    .map(|&n| usize::try_from(n).unwrap())
                    .collect();
                assert!(!loading.is_empty());
            }
            imposed += -1.0 / 10000.;
            let u = c.solid.displacement()?;
            for &n in &loading {
                let un = &u.as_slice()[2 * n..2 * n + 2];
                assert_eq!(
                    un,
                    [0.0, imposed],
                    "imposed displacement of node {n} at step {k}"
                );
            }
        }
        if let Some(w) = want.get(&k) {
            let sha = oracle::state_sha(&[
                c.solid.displacement()?.as_slice(),
                c.solid.velocity()?.as_slice(),
                c.contact.gaps().as_slice(),
            ]);
            assert_eq!(
                c.contact.contact_elements().len().to_string(),
                w[0],
                "contact at step {k}"
            );
            assert_eq!(sha, w[3], "state at step {k}");
            compared += 1;
        }
        Ok(())
    })
    .unwrap();
    for (k, e) in r.energies.iter().enumerate() {
        if let Some(w) = want.get(&k) {
            oracle::assert_csv_values_bitwise(&format!("energies at step {k}"), e, &[&w[1], &w[2]]);
        }
    }
    (r, ex.transcript(), compared)
}

/// Reduced run (301 compression steps, before the first contact at step
/// 1846: the contact and the friction are compared by the release tests
/// below): every 30th state bitwise, the initial dump and those of steps 0,
/// 100, 200 and 300 byte-identical to the upstream binary's, the first
/// output lines; independent checks: the `loading` edge carries the
/// imposed increments at every step (see [`run_compared`]) and no contact
/// element exists yet.
#[test]
fn bloc_friction_reduced_matches_the_oracle() {
    let out = scratch("bloc_friction");
    let (r, transcript, compared) = run_compared(&out, Some(301));
    assert_eq!(compared, 11);
    let o = oracle::oracle("bloc_friction");
    // the progress lines of the compression phase (the reduced run then
    // prints the end-of-phase lines at once)
    let progress = transcript.split("Compression done !").next().unwrap();
    assert!(progress.ends_with("Step 300\t\r"), "{transcript:?}");
    assert!(o.stdout().starts_with(progress), "{transcript:?}");
    assert_eq!(o.assert_files_match(&out, false, &|_| false), 2 * 5);
    assert_eq!(r.nb_contact.len(), 301);
    assert!(
        r.nb_contact.iter().all(|&n| n == 0),
        "contact before step 301: {:?}",
        r.nb_contact
    );
}

/// Medium tier (optimised builds only): the whole run (3 × 10 000 steps):
/// every 30th state bitwise, the 301 dumps and the standard output
/// identical to the upstream binary's; contact persists through the
/// sliding phase.
#[cfg(not(debug_assertions))]
#[test]
fn bloc_friction_full_run_matches_the_oracle() {
    let out = scratch("bloc_friction_full");
    let (r, transcript, compared) = run_compared(&out, None);
    assert_eq!(compared, 1000);
    let o = oracle::oracle("bloc_friction");
    assert_eq!(transcript, o.stdout());
    assert_eq!(o.assert_files_match(&out, true, &|_| false), 2 * 301);
    assert!(r.nb_contact[10000..20000].iter().all(|&n| n > 0));
}

/// The full-state oracle variant `bloc_friction_full` writes the same dumps as the
/// upstream binary (the records are its only addition).
#[test]
fn bloc_friction_full_variant_dumps_equal_the_upstream_binarys() {
    assert_eq!(
        oracle::assert_variant_dumps_equal("bloc_friction", "bloc_friction_full"),
        602
    );
}

// ---------------------------------------------------------------------------
// Default mode
// ---------------------------------------------------------------------------

/// The contact state of the previous step: master facet of every slave in
/// contact, tangential tractions, tangents and current positions.
#[cfg(not(debug_assertions))]
struct PreviousStep {
    masters: BTreeMap<akantu::core::Idx, akantu::core::Element>,
    tractions: Vec<f64>,
    tangents: Vec<f64>,
    positions: Vec<f64>,
}

/// Independent checks of the friction of one run, after every step, on the
/// set-up's closed form: the foundation is the (non-deformable) master and
/// receives no force, so its surface stays the straight line `y = 1` with
/// collinear facets; the slip of a slave over it between two steps is
/// therefore the slave's own tangential displacement increment.
#[cfg(not(debug_assertions))]
#[derive(Default)]
struct FrictionCheck {
    /// Mesh node coordinates.
    nodes: Vec<f64>,
    /// The previous step.
    previous: Option<PreviousStep>,
    /// `(step, slave)` of every crossing: a slave in contact at two
    /// consecutive steps whose master facet changes between them.
    crossings: Vec<(usize, akantu::core::Idx)>,
    /// First step (and slave, model traction, closed-form traction) at
    /// which the return-map check fails.
    first_violation: Option<(usize, akantu::core::Idx, f64, f64)>,
    /// Largest `|t − t_closed_form|` seen.
    max_error: f64,
    /// Contact-element steps in stick / in slip, per phase.
    stick: [usize; 3],
    slip: [usize; 3],
    /// Slaves checked at their first contact step (no history).
    first_contacts: usize,
}

#[cfg(not(debug_assertions))]
impl FrictionCheck {
    /// The penalty parameters of `material.dat`.
    const EPSILON_N: f64 = 4e13;
    const EPSILON_T: f64 = 1e12;

    /// Bound of the return-map check: the model measures the slip as the
    /// difference of two natural projections on the master line, the check
    /// as the difference of two nodal positions; both are sums of a few
    /// rounded operations on coordinates of magnitude ≤ 16 (the mesh spans
    /// x ∈ [−2, 16]), so they agree within 8 ulp(16), which the tangential
    /// penalty `ε_t` turns into a traction; plus 64 ε relative for the
    /// transport and the return map (see docs/NUMERICAL_VALIDATION.md).
    fn bound(tangent_norm: f64, traction: f64) -> f64 {
        Self::EPSILON_T * tangent_norm * 8.0 * (16.0_f64.next_up() - 16.0)
            + 64.0 * f64::EPSILON * traction.abs()
    }

    fn step(
        &mut self,
        k: usize,
        c: &akantu::couplers::CouplerSolidContact,
    ) -> akantu::core::Result<()> {
        use akantu::contact::ContactState;
        let mu = c.contact.resolutions()[0].mu();
        assert_eq!(mu, 0.3);
        if self.nodes.is_empty() {
            self.nodes = c.solid.mesh().read()?.nodes().as_slice().to_vec();
        }
        let x: Vec<f64> = self
            .nodes
            .iter()
            .zip(c.solid.displacement()?.as_slice())
            .map(|(a, b)| a + b)
            .collect();
        let ct = &c.contact;
        // read guards of the contact model's shared fields, held for the check
        let normal_force_g = ct.nodal_field_real("normal_force").unwrap();
        let tangential_force_g = ct.nodal_field_real("tangential_force").unwrap();
        let tangents_g = ct.nodal_field_real("tangents").unwrap();
        let gaps_g = ct.gaps();
        let normal_force = normal_force_g.as_slice();
        let tangential_force = tangential_force_g.as_slice();
        let tangents = tangents_g.as_slice();
        let tractions = ct.tangential_tractions().as_slice();
        let gaps = gaps_g.as_slice();
        let states = ct.contact_state().as_slice();
        let phase = (k / 10000).min(2);
        let dot = |a: &[f64], b: &[f64]| a[0] * b[0] + a[1] * b[1];
        let mut masters = BTreeMap::new();
        for el in ct.contact_elements() {
            let s = usize::try_from(el.slave).unwrap();
            masters.insert(el.slave, el.master);
            let (fs, ts) = (
                &normal_force[2 * s..2 * s + 2],
                &tangential_force[2 * s..2 * s + 2],
            );
            let (f_n, f_t) = (dot(fs, fs).sqrt(), dot(ts, ts).sqrt());
            // Coulomb bound on the nodal forces: stick inside, slip on it
            assert!(
                f_n > 0.0,
                "step {k}: slave {s} in contact without normal force"
            );
            assert!(
                f_t <= mu * f_n * (1.0 + 64.0 * f64::EPSILON),
                "step {k}: slave {s}: |f_T| = {f_t:e} > mu |f_N| = {:e}",
                mu * f_n
            );
            match states[s] {
                ContactState::Stick => self.stick[phase] += 1,
                ContactState::Slip => {
                    self.slip[phase] += 1;
                    assert!(
                        (f_t - mu * f_n).abs() <= 64.0 * f64::EPSILON * mu * f_n,
                        "step {k}: slave {s} slips below the Coulomb bound: {f_t:e} vs {:e}",
                        mu * f_n
                    );
                }
                ContactState::NoContact => {}
            }
            let previous = self
                .previous
                .as_ref()
                .and_then(|p| p.masters.get(&el.slave).map(|&m| (m, p)));
            let Some((previous_master, p)) = previous else {
                // first contact step of this slave: no history, no friction
                assert_eq!(f_t, 0.0, "step {k}: slave {s}: friction without history");
                assert_eq!(tractions[s], 0.0);
                self.first_contacts += 1;
                continue;
            };
            if previous_master != el.master {
                self.crossings.push((k, el.slave));
            }
            // Coulomb return map of the penalty law with the slave's own
            // slip increment (covariant component on the current tangent)
            let (t_prev, x_prev) = (&p.tractions, &p.positions);
            let (tau, tau_p) = (&tangents[2 * s..2 * s + 2], &p.tangents[2 * s..2 * s + 2]);
            let dx = [x[2 * s] - x_prev[2 * s], x[2 * s + 1] - x_prev[2 * s + 1]];
            let trial =
                t_prev[s] * dot(tau_p, tau) / dot(tau_p, tau_p) - Self::EPSILON_T * dot(tau, &dx);
            let tau_norm = dot(tau, tau).sqrt();
            let p_n = Self::EPSILON_N * gaps[s].max(0.0);
            let norm = trial.abs() / tau_norm;
            let expected = if norm <= mu * p_n {
                trial
            } else {
                trial / norm * mu * p_n
            };
            let error = (tractions[s] - expected).abs();
            self.max_error = self.max_error.max(error);
            if error > Self::bound(tau_norm, expected) && self.first_violation.is_none() {
                self.first_violation = Some((k, el.slave, tractions[s], expected));
            }
        }
        self.previous = Some(PreviousStep {
            masters,
            tractions: tractions.to_vec(),
            tangents: tangents.to_vec(),
            positions: x,
        });
        Ok(())
    }
}

/// A run without dumps, `on_step` checks of [`FrictionCheck`].
#[cfg(not(debug_assertions))]
fn run_checked(
    name: &str,
    switches: bloc_friction::Switches,
    steps: Option<usize>,
) -> (bloc_friction::Outcome, FrictionCheck) {
    let mut ex = bloc_friction::example().for_test(&scratch(name));
    ex.steps = steps;
    ex.dump = false;
    let mut check = FrictionCheck::default();
    let r = bloc_friction::run_switched(&ex, switches, |k, c| check.step(k, c)).unwrap();
    for e in &r.energies {
        assert!(e[0].is_finite() && e[1].is_finite(), "{e:?}");
    }
    (r, check)
}

/// First step at which two runs differ (contact elements or energies).
#[cfg(not(debug_assertions))]
fn first_difference(a: &bloc_friction::Outcome, b: &bloc_friction::Outcome) -> Option<usize> {
    (0..a.energies.len().min(b.energies.len()))
        .find(|&k| a.energies[k] != b.energies[k] || a.nb_contact[k] != b.nb_contact[k])
}

/// Medium tier (optimised builds only): the **default mode** (what `cargo
/// run --example bloc_friction` runs, corrected contact model)
/// past the first contact, which departs from upstream at step 1911.
///
/// - The whole default run (3 × 10 000 steps) satisfies, at every step and
///   for every contact element, the Coulomb bound on the nodal forces
///   (`|f_T| ≤ μ |f_N|`, equality in slip), no friction at a slave's first
///   contact step, and the penalty return map with the slave's **own** slip
///   increment (closed form on the static straight master), within the
///   rounding bound of [`FrictionCheck::bound`]; both stick and slip
///   occur.
/// - The departure is UD-365 (`trial_slip_on_previous_element`): the
///   default and the compat run are bitwise equal before the first step at
///   which a slave in contact changes master facet (step 1911, node 67 over
///   the master node at x = 6) and differ from it on; at that step the
///   compat run fails the return-map check (upstream evaluates the current
///   projection on the previous facet's nodes, a spurious slip of one
///   facet length that reverses the friction); the compat run with only
///   that switch off is the default run bitwise, and (the converse)
///   the default run with only that switch on is the compat run
///   bitwise (2500 steps).
#[cfg(not(debug_assertions))]
#[test]
fn bloc_friction_default_mode_past_first_contact() {
    use akantu::contact::ContactUpstreamCompat;
    use akantu::couplers::SolidContactCompat;
    use bloc_friction::Switches;
    let upstream = Switches {
        contact: ContactUpstreamCompat::all(),
        coupler: SolidContactCompat::upstream(),
    };
    let mut without_ud365 = upstream;
    without_ud365.contact.trial_slip_on_previous_element = false;
    // the converse direction of the attribution: the
    // default run with only UD-365 switched on
    let mut only_ud365 = Switches::default();
    only_ud365.contact.trial_slip_on_previous_element = true;
    let compared = Some(2500);
    let ((d, dc), (c, cc), (w, _), (o, _)) = std::thread::scope(|scope| {
        let d = scope.spawn(|| run_checked("bloc_friction_default", Switches::default(), None));
        let c = scope.spawn(|| run_checked("bloc_friction_compat_2500", upstream, compared));
        let w = scope.spawn(|| run_checked("bloc_friction_no_ud365_2500", without_ud365, compared));
        let o = scope.spawn(|| run_checked("bloc_friction_only_ud365_2500", only_ud365, compared));
        (
            d.join().unwrap(),
            c.join().unwrap(),
            w.join().unwrap(),
            o.join().unwrap(),
        )
    });
    eprintln!(
        "bloc_friction default: max |t - t_closed_form| {:e}, stick {:?}, slip {:?}, first contacts {}, crossings {}",
        dc.max_error,
        dc.stick,
        dc.slip,
        dc.first_contacts,
        dc.crossings.len()
    );
    assert_eq!(d.nb_contact.len(), 30000);
    // the contact: first at step 1846 in both modes, then never lost
    let first_contact = d.nb_contact.iter().position(|&n| n > 0);
    assert_eq!(first_contact, Some(1846));
    assert_eq!(c.nb_contact.iter().position(|&n| n > 0), first_contact);
    assert!(d.nb_contact[1846..].iter().all(|&n| n > 0));
    // the independent checks of the default run
    assert_eq!(dc.first_violation, None, "default mode return map");
    assert!(dc.first_contacts > 0);
    assert!(
        dc.stick.iter().all(|&n| n > 0),
        "stick per phase {:?}",
        dc.stick
    );
    assert!(
        dc.slip[0] > 0 && dc.slip[1] > 0,
        "slip per phase {:?}",
        dc.slip
    );
    // the departure is UD-365, at the first crossing
    let (first_crossing, slave) = dc.crossings[0];
    assert_eq!((first_crossing, slave), (1911, 67));
    assert_eq!(first_difference(&d, &c), Some(first_crossing));
    assert_eq!(cc.crossings.first().copied(), Some((first_crossing, slave)));
    let (k, s, t, t_closed_form) = cc.first_violation.expect("UD-365 departure");
    assert_eq!((k, s), (first_crossing, slave));
    assert!(
        t * t_closed_form < 0.0,
        "friction reversed: {t:e} vs {t_closed_form:e}"
    );
    assert_eq!(
        first_difference(&d, &w),
        None,
        "compat without UD-365 = default"
    );
    // the converse: the default with only UD-365 is the
    // compat run, and departs from the default at the first crossing
    assert_eq!(
        first_difference(&o, &c),
        None,
        "default with only UD-365 = compat"
    );
    assert_eq!(
        first_difference(&d, &o),
        Some(first_crossing),
        "default with only UD-365 departs from the default at the first crossing"
    );
}
