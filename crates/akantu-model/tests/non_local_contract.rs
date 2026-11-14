// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): cycle of src/model/common/
// non_local_toolbox/non_local_manager.cc (computeAllNonLocalContribution,
// computeWeights, updateWeightFunctionInternals), weights and averages of
// non_local_neighborhood_tmpl.hh, base weight of
// base_weight_function_inline_impl.hh; the rebuild contract (replaces the
// contract tests of the former non_local_cycle.rs).

//! Contract of the non-local manager (ARCHITECTURE):
//!
//! * the cycle order (flatten → weights → average → inflate → non-local
//!   stresses) and the weight-function internals (flattened before the
//!   weights, never averaged);
//! * averages equal an independent brute-force evaluation of
//!   `Σ W·J·v / Σ W·J` (both routes);
//! * the opt-in [`SummationOrder::KeyOrder`] route is independent of the
//!   point insertion order (bitwise), the property partition independence
//!   rests on; the upstream route is not, at rounding level (measured);
//! * stale neighbourhoods are refused (never initialised, after a topology
//!   event of the model's dimension), events on other dimensions do not
//!   invalidate, a rebuild keeps the weight functions and their parameters
//!   and equals a fresh manager bit for bit; `update_rate` controls when the
//!   weights are recomputed.

use akantu_validation::reduce::nan_max;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use akantu_core::element_type_map::TypeFilter;
use akantu_core::enums::MeshIoType;
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, GhostType, Idx, Int, Real,
    Result,
};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_io::read_mesh;
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler};
use akantu_model::non_local::{
    BaseWeightFunction, NoGhosts, NonLocalManager, NonLocalManagerCallback, NonLocalOptions,
    PointRef, QuadPoint, SummationOrder, VariableRegistration, WeightFunction, WeightFunctionBase,
    WeightInternals, base_weight, distance,
};
use akantu_parser::{ParameterRegistry, Parsable, Parser, ParserType};

const QUAD: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/upstream/non_local")
        .join(name)
}

/// `base_wf · (1 − d(q2))` with `d` the manager internal `damage`; counts
/// its internal updates (retained across rebuilds).
#[derive(Debug, Default)]
struct DamagedBase {
    base: BaseWeightFunction,
    damage: Vec<Real>,
    updates: Arc<Mutex<usize>>,
}

impl ParameterRegistry for DamagedBase {
    fn get_param(&self, name: &str) -> Result<akantu_parser::ParamValue> {
        self.base.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: akantu_parser::ParamValue) -> Result<()> {
        self.base.set_param(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.base.param_names()
    }
    fn param_info(&self, name: &str) -> Option<akantu_parser::param::ParamInfo> {
        self.base.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: akantu_parser::ParamValue) -> Result<()> {
        self.base.set_param_from_input(name, value)
    }
}

impl Parsable for DamagedBase {
    fn section_type(&self) -> ParserType {
        ParserType::WeightFunction
    }
}

impl WeightFunction for DamagedBase {
    fn type_name(&self) -> &str {
        "damaged_base"
    }
    fn update_rate(&self) -> Int {
        self.base.update_rate()
    }
    fn radius(&self) -> Real {
        self.base.radius()
    }
    fn set_radius(&mut self, radius: Real) {
        self.base.set_radius(radius);
    }
    fn internal_names(&self) -> Vec<String> {
        vec!["damage".into()]
    }
    fn update_internals(&mut self, internals: &WeightInternals) -> Result<()> {
        self.damage = internals["damage"].clone();
        *self.updates.lock().unwrap() += 1;
        Ok(())
    }
    fn weight(&self, r: Real, q1: PointRef<'_>, q2: PointRef<'_>) -> Real {
        self.base.weight(r, q1, q2) * (1. - self.damage[q2.slot])
    }
}

/// A "law" on every quadrangle of the mesh: local field `strain` (2
/// components), its average `strain non local`, and `damage` read by the
/// weight function only.
struct Law {
    order: Vec<usize>,
    coords: Array<Real>,
    strain: Vec<Real>,
    damage: Vec<Real>,
    averaged: Vec<Real>,
    log: Vec<String>,
}

impl Law {
    fn new(mesh: &Mesh, fe: &FeEngine, order: Vec<usize>) -> Self {
        let mut coords = Array::new(2);
        fe.integration_points_coordinates(mesh, &mut coords, QUAD, NG, None)
            .unwrap();
        let n = coords.len();
        let mut strain = Vec::new();
        let mut damage = Vec::new();
        for (k, x) in coords.as_slice().chunks(2).enumerate() {
            strain.push(x[0] * x[0] + 0.3 * x[1]);
            strain.push(1. - 0.5 * x[0] * x[1]);
            #[allow(clippy::cast_precision_loss)] // small index
            damage.push(((k * 7) % 10) as Real / 20.);
        }
        Self {
            order,
            coords,
            strain,
            damage,
            averaged: vec![0.; 2 * n],
            log: Vec::new(),
        }
    }
}

impl NonLocalManagerCallback for Law {
    fn insert_integration_points_in_neighborhoods(
        &mut self,
        ghost: GhostType,
        manager: &mut NonLocalManager,
    ) -> Result<()> {
        self.log.push(format!("insert {ghost:?}"));
        manager.register_neighborhood("law", "law")?;
        manager.register_non_local_variable("strain", "strain non local", 2);
        manager
            .neighborhood_mut("law")?
            .register_non_local_variable("strain non local");
        if ghost == GhostType::Ghost {
            return Ok(());
        }
        // the points of each element in `order` (4 points per quadrangle)
        for &e in &self.order {
            for q in 0..4 {
                let k = e * 4 + q;
                let p = QuadPoint::new(
                    QUAD,
                    NG,
                    Idx::try_from(e).unwrap(),
                    Idx::try_from(q).unwrap(),
                    4,
                );
                manager.insert_integration_point("law", p, self.coords.tuple(k))?;
            }
        }
        Ok(())
    }
    fn update_local_internal(
        &self,
        field: &str,
        ghost: GhostType,
        flat: &mut ElementTypeMapArray<Real>,
    ) -> Result<()> {
        if ghost == GhostType::Ghost {
            return Ok(());
        }
        let (values, nbc) = match field {
            "strain" => (&self.strain, 2),
            "damage" => (&self.damage, 1),
            _ => return Ok(()),
        };
        let a = flat.alloc(values.len() / nbc, nbc, QUAD, NG, 0.)?;
        a.as_mut_slice().copy_from_slice(values);
        Ok(())
    }
    fn update_non_local_internal(
        &mut self,
        field: &str,
        ghost: GhostType,
        flat: &ElementTypeMapArray<Real>,
        _strict: bool,
    ) -> Result<()> {
        if ghost == NG {
            self.log.push(format!("inflate {field}"));
            self.averaged
                .copy_from_slice(flat.get(QUAD, NG)?.as_slice());
        }
        Ok(())
    }
    fn compute_non_local_contribution(&mut self, _ghost: GhostType) -> Result<()> {
        self.log.push("stresses".into());
        Ok(())
    }
}

struct Setup {
    mesh: Mesh,
    fe: FeEngine,
    parser: Parser,
}

fn setup() -> Setup {
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    read_mesh(&mut mesh, &data("plate.msh"), MeshIoType::Gmsh).unwrap();
    let parser = Parser::parse_str(
        "non_local law damaged_base [\n  radius = 0.6\n  weight_function weight_parameter [\n    update_rate = 2\n  ]\n]\n",
        "contract.dat",
    )
    .unwrap();
    let fe = FeEngine::with_precomputation(
        &mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap();
    Setup { mesh, fe, parser }
}

fn manager(s: &Setup, order: SummationOrder, updates: &Arc<Mutex<usize>>) -> NonLocalManager {
    let mut m = NonLocalManager::new("nl", 2);
    m.set_options(NonLocalOptions {
        order,
        ..NonLocalOptions::default()
    });
    let u = Arc::clone(updates);
    m.factory_mut().register(
        "damaged_base",
        Arc::new(move || {
            Box::new(DamagedBase {
                updates: Arc::clone(&u),
                ..DamagedBase::default()
            }) as Box<dyn WeightFunction>
        }),
    );
    m.read_sections(&s.parser.root, false);
    m.set_grid_center_from(&s.mesh).unwrap();
    m
}

/// Brute force `Σ W·J·v / Σ W·J` over every point within the radius.
fn brute_force(s: &Setup, law: &Law, radius: Real) -> Vec<Real> {
    let j = s.fe.jacobians(QUAD, NG).unwrap().as_slice();
    let n = law.coords.len();
    let mut out = vec![0.; 2 * n];
    for a in 0..n {
        let (mut num, mut den) = ([0.; 2], 0.);
        for (b, &jb) in j.iter().enumerate().take(n) {
            let r = distance(law.coords.tuple(a), law.coords.tuple(b));
            if r > radius + akantu_core::math::DEFAULT_TOLERANCE {
                continue;
            }
            let w = base_weight(r, radius, radius * radius) * (1. - law.damage[b]) * jb;
            num[0] += w * law.strain[2 * b];
            num[1] += w * law.strain[2 * b + 1];
            den += w;
        }
        out[2 * a] = num[0] / den;
        out[2 * a + 1] = num[1] / den;
    }
    out
}

/// Largest relative difference; NaN-propagating (main's
/// rule): a NaN anywhere makes it NaN and the `<=` checks fail.
fn max_rel(a: &[Real], b: &[Real]) -> Real {
    assert_eq!(a.len(), b.len(), "max_rel: lengths");
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs() / nan_max(y.abs(), 1e-300))
        .fold(0., nan_max)
}

#[test]
fn cycle_order_weight_internals_and_brute_force_averages() {
    let s = setup();
    let n_el = s.mesh.nb_element(QUAD, NG);
    for order in [SummationOrder::UpstreamHalfPairs, SummationOrder::KeyOrder] {
        let updates = Arc::default();
        let mut m = manager(&s, order, &updates);
        let mut law = Law::new(&s.mesh, &s.fe, (0..n_el).collect());
        m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
            .unwrap();
        assert_eq!(law.log, ["insert NotGhost", "insert Ghost"]);
        m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
            .unwrap();
        assert_eq!(
            m.trace,
            [
                "flatten",
                "weights",
                "average",
                "inflate",
                "compute_non_local_contribution"
            ]
        );
        assert_eq!(&law.log[2..], ["inflate strain non local", "stresses"]);
        // damage is a weight internal, not an averaged variable
        assert_eq!(m.variables().len(), 1);
        assert!(m.weight_internals().contains_key("damage"));
        // independent check: the averages are Σ W J v / Σ W J. Bound: n ≤ 40
        // terms with weights in [0, 1]: the two routes and the brute force
        // differ by rounding only (≤ (2n + 4)·ε relative to the values,
        // which are O(1) and positive here).
        let bf = brute_force(&s, &law, 0.6);
        let rel = max_rel(&law.averaged, &bf);
        assert!(
            rel <= 84. * Real::EPSILON,
            "{order:?}: max relative difference {rel:e}"
        );
    }
}

#[test]
fn key_order_is_insertion_order_independent_upstream_route_is_not() {
    let s = setup();
    let n_el = s.mesh.nb_element(QUAD, NG);
    let run = |order: SummationOrder, elements: Vec<usize>| {
        let updates = Arc::default();
        let mut m = manager(&s, order, &updates);
        let mut law = Law::new(&s.mesh, &s.fe, elements);
        m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
            .unwrap();
        m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
            .unwrap();
        law.averaged
    };
    let forward: Vec<usize> = (0..n_el).collect();
    let reversed: Vec<usize> = (0..n_el).rev().collect();
    let k1 = run(SummationOrder::KeyOrder, forward.clone());
    let k2 = run(SummationOrder::KeyOrder, reversed.clone());
    assert_eq!(
        k1.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        k2.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
        "KeyOrder depends on the insertion order"
    );
    let u1 = run(SummationOrder::UpstreamHalfPairs, forward);
    let u2 = run(SummationOrder::UpstreamHalfPairs, reversed);
    let diff_routes = max_rel(&u1, &k1);
    let diff_orders = max_rel(&u1, &u2);
    eprintln!(
        "upstream route vs key order: max relative difference {diff_routes:e}; \
         upstream route, forward vs reversed insertion: {diff_orders:e}"
    );
    assert!(diff_routes <= 84. * Real::EPSILON && diff_orders <= 84. * Real::EPSILON);
}

#[test]
fn stale_neighbourhoods_are_refused_and_a_rebuild_equals_a_fresh_manager() {
    let s = setup();
    let n_el = s.mesh.nb_element(QUAD, NG);
    let updates: Arc<Mutex<usize>> = Arc::default();
    let mut m = manager(&s, SummationOrder::UpstreamHalfPairs, &updates);
    let mut law = Law::new(&s.mesh, &s.fe, (0..n_el).collect());
    // never initialised
    let e = m.compute_all_non_local_contribution(&mut law, &mut NoGhosts);
    assert!(e.is_err_and(|e| e.to_string().contains("never built")));
    m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
        .unwrap();
    m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
        .unwrap();
    let first = law.averaged.clone();
    // a facet event does not invalidate
    let facet = Element::new(ElementType::Segment2, 0, NG);
    m.on_mesh_event(
        &s.mesh,
        &MeshEvent::ElementsAdded {
            elements: vec![facet],
            constitutive_law: None,
            origin: "test".into(),
            extension: None,
        },
    )
    .unwrap();
    assert!(m.is_valid());
    // neither does a changed-elements event on facets
    // (the same relevance filter as for added/removed elements)
    let changed = |old: Element, new: Element| MeshEvent::ElementsChanged {
        old: vec![old],
        new: vec![new],
        new_numbering: ElementTypeMapArray::new("new_numbering"),
        origin: "test".into(),
    };
    m.on_mesh_event(
        &s.mesh,
        &changed(facet, Element::new(ElementType::Segment2, 1, NG)),
    )
    .unwrap();
    assert!(
        m.is_valid(),
        "facet ElementsChanged keeps the neighbourhoods"
    );
    // an element of the model's dimension does
    m.on_mesh_event(
        &s.mesh,
        &MeshEvent::ElementsAdded {
            elements: vec![Element::new(QUAD, 0, NG)],
            constitutive_law: None,
            origin: "test".into(),
            extension: None,
        },
    )
    .unwrap();
    assert!(!m.is_valid());
    let averaged_before = law.averaged.clone();
    let e = m.compute_all_non_local_contribution(&mut law, &mut NoGhosts);
    assert!(e.is_err_and(|e| e.to_string().contains("stale")));
    assert_eq!(
        law.averaged, averaged_before,
        "nothing inflated while stale"
    );
    // (ElementsChanged on regular elements: separate test below)
    // rebuild: weight function and its parameters retained
    let rate_before = m
        .neighborhood("law")
        .unwrap()
        .weight_function()
        .update_rate();
    assert_eq!(rate_before, 2);
    m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
        .unwrap();
    assert_eq!(
        m.neighborhood("law")
            .unwrap()
            .weight_function()
            .update_rate(),
        2
    );
    m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
        .unwrap();
    // equal to a fresh manager at the same number of averaging calls
    let mut fresh = manager(&s, SummationOrder::UpstreamHalfPairs, &Arc::default());
    let mut law2 = Law::new(&s.mesh, &s.fe, (0..n_el).collect());
    fresh
        .initialize(&s.mesh, &s.fe, &mut law2, &mut NoGhosts)
        .unwrap();
    fresh
        .compute_all_non_local_contribution(&mut law2, &mut NoGhosts)
        .unwrap();
    assert_eq!(law.averaged, law2.averaged);
    assert_eq!(first, law2.averaged);
}

#[test]
fn update_rate_controls_the_weight_recomputation() {
    let s = setup();
    let n_el = s.mesh.nb_element(QUAD, NG);
    let updates: Arc<Mutex<usize>> = Arc::default();
    let mut m = manager(&s, SummationOrder::UpstreamHalfPairs, &updates);
    let mut law = Law::new(&s.mesh, &s.fe, (0..n_el).collect());
    m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
        .unwrap();
    assert_eq!(
        *updates.lock().unwrap(),
        1,
        "weights computed at initialisation"
    );
    let mut seen = Vec::new();
    for call in 0..5 {
        // damage grows: only the due updates (calls 0, 2, 4 with rate 2) see it
        for d in &mut law.damage {
            *d = (*d + 0.05).min(0.9);
        }
        m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
            .unwrap();
        seen.push(*updates.lock().unwrap());
        assert_eq!(m.nb_stress_calls(), call + 1);
    }
    assert_eq!(seen, [2, 2, 3, 3, 4]);
}

#[test]
fn layout_errors_are_reported() {
    // a point outside the mesh blocks is an error, not an out-of-range access
    struct Bad;
    impl NonLocalManagerCallback for Bad {
        fn insert_integration_points_in_neighborhoods(
            &mut self,
            ghost: GhostType,
            manager: &mut NonLocalManager,
        ) -> Result<()> {
            if ghost == NG {
                manager.register_neighborhood("law", "law")?;
                let q = QuadPoint::new(QUAD, NG, 1000, 0, 4);
                manager.insert_integration_point("law", q, &[0., 0.])?;
            }
            Ok(())
        }
        fn update_local_internal(
            &self,
            _: &str,
            _: GhostType,
            _: &mut ElementTypeMapArray<Real>,
        ) -> Result<()> {
            Ok(())
        }
        fn update_non_local_internal(
            &mut self,
            _: &str,
            _: GhostType,
            _: &ElementTypeMapArray<Real>,
            _: bool,
        ) -> Result<()> {
            Ok(())
        }
        fn compute_non_local_contribution(&mut self, _: GhostType) -> Result<()> {
            Ok(())
        }
    }
    let s = setup();
    let mut m = manager(&s, SummationOrder::UpstreamHalfPairs, &Arc::default());
    let e = m.initialize(&s.mesh, &s.fe, &mut Bad, &mut NoGhosts);
    assert!(e.is_err_and(|e| e.to_string().contains("outside the mesh blocks")));
    assert!(!m.is_valid());
    // unknown section / weight-function type
    let mut m = NonLocalManager::new("nl", 2);
    assert!(m.register_neighborhood("x", "x").is_err());
    let _ = (
        WeightFunctionBase::default(),
        BTreeMap::<String, Real>::new(),
        TypeFilter::new(),
    );
}

/* -------------------------------------------------------------------------- */
/* UD-558: per-neighbourhood registration of the averaged variables           */
/* -------------------------------------------------------------------------- */

/// Two "laws" on the two halves of the plate's quadrangles, each with its
/// own neighbourhood (`a`, `b`, base_wf R = 0.6), averaging different local
/// variables under the **same** non-local name `nlv` (like two
/// `mazars_non_local` laws, one with `average_on_damage`): law `a` declares
/// `ehat`, law `b` declares `dmg`. Both laws own both local fields.
/// `(neighbourhood, local, non-local, components)` of one declaration.
type Declaration = (&'static str, &'static str, &'static str, usize);

struct TwoLaws {
    coords: Array<Real>,
    ehat: Vec<Real>,
    dmg: Vec<Real>,
    split: usize,
    /// `(neighbourhood, local, non-local, components)` declarations, in order.
    declarations: Vec<Declaration>,
    averaged: Vec<Real>,
}

impl TwoLaws {
    fn new(s: &Setup) -> Self {
        let mut coords = Array::new(2);
        s.fe.integration_points_coordinates(&s.mesh, &mut coords, QUAD, NG, None)
            .unwrap();
        let (mut ehat, mut dmg) = (Vec::new(), Vec::new());
        for x in coords.as_slice().chunks(2) {
            ehat.push(1e-4 * (1. + x[0] * x[0] + 0.25 * x[1]));
            dmg.push(0.1 + 0.3 * x[0] * x[1]);
        }
        let n = coords.len();
        Self {
            coords,
            ehat,
            dmg,
            split: s.mesh.nb_element(QUAD, NG) / 2,
            declarations: vec![("a", "ehat", "nlv", 1), ("b", "dmg", "nlv", 1)],
            averaged: vec![0.; n],
        }
    }

    fn neighbourhood_of(&self, element: usize) -> &'static str {
        if element < self.split { "a" } else { "b" }
    }
}

impl NonLocalManagerCallback for TwoLaws {
    fn insert_integration_points_in_neighborhoods(
        &mut self,
        ghost: GhostType,
        manager: &mut NonLocalManager,
    ) -> Result<()> {
        for name in ["a", "b"] {
            manager.register_neighborhood(name, name)?;
        }
        for &(nh, local, nl, nbc) in &self.declarations {
            manager.register_neighborhood_variable(nh, local, nl, nbc)?;
        }
        if ghost == GhostType::Ghost {
            return Ok(());
        }
        for k in 0..self.coords.len() {
            let (e, q) = (k / 4, k % 4);
            let p = QuadPoint::new(
                QUAD,
                NG,
                Idx::try_from(e).unwrap(),
                Idx::try_from(q).unwrap(),
                4,
            );
            manager.insert_integration_point(self.neighbourhood_of(e), p, self.coords.tuple(k))?;
        }
        Ok(())
    }
    fn update_local_internal(
        &self,
        field: &str,
        ghost: GhostType,
        flat: &mut ElementTypeMapArray<Real>,
    ) -> Result<()> {
        if ghost == GhostType::Ghost {
            return Ok(());
        }
        let values = match field {
            "ehat" => &self.ehat,
            "dmg" => &self.dmg,
            _ => return Ok(()),
        };
        let a = flat.alloc(values.len(), 1, QUAD, NG, 0.)?;
        a.as_mut_slice().copy_from_slice(values);
        Ok(())
    }
    fn update_non_local_internal(
        &mut self,
        field: &str,
        ghost: GhostType,
        flat: &ElementTypeMapArray<Real>,
        _strict: bool,
    ) -> Result<()> {
        if ghost == NG && field == "nlv" {
            self.averaged
                .copy_from_slice(flat.get(QUAD, NG)?.as_slice());
        }
        Ok(())
    }
    fn compute_non_local_contribution(&mut self, _ghost: GhostType) -> Result<()> {
        Ok(())
    }
}

fn two_law_manager(s: &Setup, registration: VariableRegistration) -> NonLocalManager {
    let parser = Parser::parse_str(
        "non_local a base_wf [\n  radius = 0.6\n]\nnon_local b base_wf [\n  radius = 0.6\n]\n",
        "ud558.dat",
    )
    .unwrap();
    let mut m = NonLocalManager::new("nl", 2);
    m.set_options(NonLocalOptions {
        registration,
        ..NonLocalOptions::default()
    });
    m.read_sections(&parser.root, false);
    m.set_grid_center_from(&s.mesh).unwrap();
    m
}

/// Brute force `Σ W·J·v / Σ W·J` of `v` over the points of the same
/// neighbourhood within the radius (base_wf).
fn brute_force_split(s: &Setup, law: &TwoLaws, v_of: impl Fn(&str) -> Vec<Real>) -> Vec<Real> {
    let j = s.fe.jacobians(QUAD, NG).unwrap().as_slice();
    let n = law.coords.len();
    let radius = 0.6;
    (0..n)
        .map(|a| {
            let nh = law.neighbourhood_of(a / 4);
            let v = v_of(nh);
            let (mut num, mut den) = (0., 0.);
            for b in 0..n {
                if law.neighbourhood_of(b / 4) != nh {
                    continue;
                }
                let r = distance(law.coords.tuple(a), law.coords.tuple(b));
                if r > radius + akantu_core::math::DEFAULT_TOLERANCE {
                    continue;
                }
                let w = base_weight(r, radius, radius * radius) * j[b];
                num += w * v[b];
                den += w;
            }
            num / den
        })
        .collect()
}

/// UD-558 corrected default: every neighbourhood averages the variable its
/// law declared (`a`: `ehat`, `b`: `dmg`), checked against an independent
/// brute-force evaluation; upstream's registration (compat) makes both
/// average the last registered variable (`dmg`). Bound: as above (≤ 32
/// neighbours, weights in [0, 1], positive O(1)·scale values: ≤ (2n + 4)·ε
/// relative, 84·ε).
#[test]
fn ud558_each_neighbourhood_averages_its_declared_variable() {
    let s = setup();
    for order in [SummationOrder::UpstreamHalfPairs, SummationOrder::KeyOrder] {
        let mut law = TwoLaws::new(&s);
        let mut m = two_law_manager(&s, VariableRegistration::PerNeighborhood);
        let mut o = m.options();
        o.order = order;
        m.set_options(o);
        m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
            .unwrap();
        m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
            .unwrap();
        // upstream's shared map keeps one entry (the last registered)
        assert_eq!(m.variables().len(), 1);
        assert_eq!(m.variables()["nlv"].local, "dmg");
        assert_eq!(m.resolved_variable("a", "nlv").unwrap().local, "ehat");
        assert_eq!(m.resolved_variable("b", "nlv").unwrap().local, "dmg");
        let (ehat, dmg) = (law.ehat.clone(), law.dmg.clone());
        let expected =
            brute_force_split(
                &s,
                &law,
                |nh| if nh == "a" { ehat.clone() } else { dmg.clone() },
            );
        let rel = max_rel(&law.averaged, &expected);
        assert!(
            rel <= 84. * Real::EPSILON,
            "{order:?}: max relative difference {rel:e}"
        );
    }
    // compat: both neighbourhoods average `dmg` (upstream's last-wins)
    let mut law = TwoLaws::new(&s);
    let mut m = two_law_manager(&s, VariableRegistration::UpstreamShared);
    m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
        .unwrap();
    m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
        .unwrap();
    assert_eq!(m.resolved_variable("a", "nlv").unwrap().local, "dmg");
    let dmg = law.dmg.clone();
    let expected = brute_force_split(&s, &law, |_| dmg.clone());
    let rel = max_rel(&law.averaged, &expected);
    assert!(
        rel <= 84. * Real::EPSILON,
        "compat: max relative difference {rel:e}"
    );
}

/// UD-558 corrected default: declarations that one averaged field per
/// non-local name cannot represent are refused with an error naming both;
/// upstream's registration accepts them silently (compat).
#[test]
fn ud558_conflicting_declarations_are_refused_by_default() {
    let s = setup();
    let cases: [(&str, Vec<Declaration>, &str); 2] = [
        (
            "one neighbourhood, two local variables",
            vec![
                ("a", "ehat", "nlv", 1),
                ("a", "dmg", "nlv", 1),
                ("b", "dmg", "nlv", 1),
            ],
            "declared as the average of ehat",
        ),
        (
            "two neighbourhoods, different component counts",
            vec![("a", "ehat", "nlv", 1), ("b", "dmg", "nlv", 2)],
            "has 1 components in the neighborhood a and 2",
        ),
    ];
    for (what, declarations, message) in cases {
        let mut law = TwoLaws::new(&s);
        law.declarations = declarations.clone();
        let mut m = two_law_manager(&s, VariableRegistration::PerNeighborhood);
        let e = m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts);
        assert!(
            e.as_ref().is_err_and(|e| e.to_string().contains(message)),
            "{what}: {e:?}"
        );
        let mut law = TwoLaws::new(&s);
        law.declarations = declarations;
        let mut m = two_law_manager(&s, VariableRegistration::UpstreamShared);
        m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
            .unwrap();
    }
    // an unknown neighbourhood is an error in both modes
    let mut m = two_law_manager(&s, VariableRegistration::UpstreamShared);
    assert!(m.register_neighborhood_variable("zz", "x", "y", 1).is_err());
}

/// The key-order route caches its neighbour lists per
/// rebuild and its `W·J` products per weight update, so it follows the
/// `update_rate` schedule like upstream's route: with `update_rate = 2`,
/// the averaging call 1 uses the weights of call 0 (damage before the
/// change), call 2 the new ones. Checked against the brute force (84·ε,
/// as above).
#[test]
fn key_order_weights_follow_the_update_rate_schedule() {
    let s = setup();
    let n_el = s.mesh.nb_element(QUAD, NG);
    let updates = Arc::default();
    let mut m = manager(&s, SummationOrder::KeyOrder, &updates);
    let mut law = Law::new(&s.mesh, &s.fe, (0..n_el).collect());
    m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
        .unwrap();
    let old_damage = law.damage.clone();
    let mut expected_old = brute_force(&s, &law, 0.6);
    for call in 0..3 {
        if call == 1 {
            // change the damage: the weights are not due at call 1
            for d in &mut law.damage {
                *d = 0.5 * (1. - *d);
            }
        }
        m.compute_all_non_local_contribution(&mut law, &mut NoGhosts)
            .unwrap();
        let expected = if call < 2 {
            // weights of the damage before the change; the strain is unchanged
            let mut l = Law::new(&s.mesh, &s.fe, (0..n_el).collect());
            l.damage.clone_from(&old_damage);
            expected_old = brute_force(&s, &l, 0.6);
            expected_old.clone()
        } else {
            brute_force(&s, &law, 0.6)
        };
        let rel = max_rel(&law.averaged, &expected);
        assert!(rel <= 84. * Real::EPSILON, "call {call}: {rel:e}");
    }
    let changed = brute_force(&s, &law, 0.6);
    assert!(
        max_rel(&changed, &expected_old) > 1e-6,
        "the damage change matters"
    );
}

/// `ElementsChanged` invalidates only when its old or
/// new ids hold an element of the model's dimension (regular kind).
#[test]
fn elements_changed_invalidates_only_regular_elements_of_the_dimension() {
    let s = setup();
    let n_el = s.mesh.nb_element(QUAD, NG);
    let mut m = manager(&s, SummationOrder::UpstreamHalfPairs, &Arc::default());
    let mut law = Law::new(&s.mesh, &s.fe, (0..n_el).collect());
    m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
        .unwrap();
    let event = |old: Element, new: Element| MeshEvent::ElementsChanged {
        old: vec![old],
        new: vec![new],
        new_numbering: ElementTypeMapArray::new("new_numbering"),
        origin: "test".into(),
    };
    let seg = |k| Element::new(ElementType::Segment2, k, NG);
    m.on_mesh_event(&s.mesh, &event(seg(0), seg(1))).unwrap();
    assert!(m.is_valid());
    m.on_mesh_event(&s.mesh, &event(seg(0), Element::new(QUAD, 0, NG)))
        .unwrap();
    assert!(!m.is_valid(), "a regular element among the new ids");
    m.initialize(&s.mesh, &s.fe, &mut law, &mut NoGhosts)
        .unwrap();
    m.on_mesh_event(&s.mesh, &event(Element::new(QUAD, 1, NG), seg(2)))
        .unwrap();
    assert!(!m.is_valid(), "a regular element among the old ids");
}
