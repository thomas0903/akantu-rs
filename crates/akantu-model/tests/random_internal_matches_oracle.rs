// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): differential test of the
// law-level allocation and draw order of random internal fields against
// fixtures/cl/random_internal/smm_2d (written by
// oracle/probes/cpp/law_random_probe.cc from an upstream Material with
// RandomInternalFields in a SolidMechanicsModel; upstream
// src/model/common/constitutive_laws/{internal_field,random_internal_field,
// constitutive_law,constitutive_laws_handler}_tmpl.hh).

//! `cl::random_internal_matches_oracle` (CL-002/CL-003): a
//! constitutive law with the probe's internal fields — random `z_rand`
//! (1 component), `a_rand` (2), `m_rand` (1, history), `k_coh` (1, kind
//! `_ek_cohesive`) and plain `p_plain`, registered in that order and set
//! from the same `.dat` section — goes through the probe's sequence with the
//! generic law API: initialisation (`add_element` of the not-ghost elements,
//! one `resize_internals_from_filter`), `add_elements` of ghost elements,
//! `set_default_value` and `set_random_distribution` resets, element
//! addition, removal with the oracle's mesh renumbering
//! (`on_elements_removed`) and addition again. After every step the element
//! filter, the block layout of every internal and the values of the random
//! fields equal the oracle's — values bit for bit, drawn from the
//! process-global engine seeded like upstream (`seed = 1234` of the input
//! file) — and the engine's next values agree at the end (same number of
//! draws). The oracle is the upstream library, not the Rust RNG.
//!
//! Documented deviation checked explicitly: after the removal the port
//! compacts the history of `m_rand` (UD-106); upstream keeps the old sizes
//! until the next resize.
//!
//! Ghost-type order of one resize growing both ghost
//! types: `mixed_ghost_resize_matches_oracle` replays
//! fixtures/cl/random_internal/smm_2d_mixed — the same probe material,
//! empty after `initFull` (an `elastic` material takes every element), then
//! one `addElements` of every not-ghost and ghost element in an interleaved
//! list — values bit for bit and the next engine values;
//! `mixed_ghost_resize_follows_the_oracle_reset_order` additionally ties
//! that order to the oracle's s3 `reset()` of `smm_2d` (upstream `resize()`
//! and `reset()` both iterate `_not_ghost` then `_ghost`, element types in
//! enum order).

use std::sync::{Mutex, PoisonError};

use akantu_core::random::{IdxSlot, RandomDistributionType, RandomGenerator, RandomParameter};
use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result,
};
use akantu_model::constitutive_law::{ConstitutiveLaw, InternalFieldMut, InternalFieldRef};
use akantu_model::internal_field::InternalField;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    Parsable, table_get, table_info, table_names, table_set, table_set_from_input,
};
use akantu_parser::{ByteString, Parser, ParserType};
use akantu_validation::Fixture;

const TRI: ElementType = ElementType::Triangle3;
const QUAD: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;
const G: GhostType = GhostType::Ghost;

/// The tests of this binary share the process-global engine.
static ENGINE: Mutex<()> = Mutex::new(());

#[derive(Debug)]
struct ProbeLaw {
    name: ByteString,
    rho: Real,
    filter: ElementTypeMapArray<Idx>,
    z_rand: InternalField<Real>,
    a_rand: InternalField<Real>,
    m_rand: InternalField<Real>,
    k_coh: InternalField<Real>,
    p_plain: InternalField<Real>,
}

fn set_random(f: &mut InternalField<Real>, v: ParamValue) -> Result<()> {
    match v {
        ParamValue::Random(r) => {
            f.set_random_distribution(r.to_core());
            Ok(())
        }
        other => {
            f.set_default_value(other.as_real()?);
            Ok(())
        }
    }
}

macro_rules! random_param {
    ($name:literal, $field:ident) => {
        ParamDescriptor::<ProbeLaw>::new(
            $name,
            ParamKind::Random,
            ParameterAccess::PARSMOD,
            "random internal",
            |s| ParamValue::Real(s.$field.parameter_value()),
            Some(|s, v| set_random(&mut s.$field, v)),
        )
    };
}

static PARAMS: &[ParamDescriptor<ProbeLaw>] = &[
    ParamDescriptor::new(
        "name",
        ParamKind::ByteString,
        ParameterAccess(ParameterAccess::PARSABLE.0 | ParameterAccess::READABLE.0),
        "name",
        |s| ParamValue::ByteString(s.name.clone()),
        Some(|s, v| {
            s.name = v.into_byte_string()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "rho",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "density",
        |s| ParamValue::Real(s.rho),
        Some(|s, v| {
            s.rho = v.as_real()?;
            Ok(())
        }),
    ),
    random_param!("z_rand", z_rand),
    random_param!("a_rand", a_rand),
    random_param!("m_rand", m_rand),
    random_param!("k_coh", k_coh),
    ParamDescriptor::new(
        "p_plain",
        ParamKind::Real,
        ParameterAccess::PARSMOD,
        "plain field",
        |s| ParamValue::Real(s.p_plain.parameter_value()),
        Some(|s, v| {
            s.p_plain.set_default_value(v.as_real()?);
            Ok(())
        }),
    ),
];

impl ParameterRegistry for ProbeLaw {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(PARAMS, self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set(PARAMS, self, name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(PARAMS)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(PARAMS, name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set_from_input(PARAMS, self, name, value)
    }
}

impl Parsable for ProbeLaw {
    fn section_type(&self) -> ParserType {
        ParserType::Material
    }
}

impl ConstitutiveLaw for ProbeLaw {
    fn name(&self) -> &str {
        "probe"
    }
    fn law_id(&self) -> &str {
        "model:0:random_probe"
    }
    fn law_type(&self) -> &str {
        "random_probe"
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        &self.filter
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        &mut self.filter
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        f("z_rand", InternalFieldRef::Real(&self.z_rand));
        f("a_rand", InternalFieldRef::Real(&self.a_rand));
        f("m_rand", InternalFieldRef::Real(&self.m_rand));
        f("k_coh", InternalFieldRef::Real(&self.k_coh));
        f("p_plain", InternalFieldRef::Real(&self.p_plain));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        f("z_rand", InternalFieldMut::Real(&mut self.z_rand))?;
        f("a_rand", InternalFieldMut::Real(&mut self.a_rand))?;
        f("m_rand", InternalFieldMut::Real(&mut self.m_rand))?;
        f("k_coh", InternalFieldMut::Real(&mut self.k_coh))?;
        f("p_plain", InternalFieldMut::Real(&mut self.p_plain))
    }
}

impl ProbeLaw {
    /// The probe material's constructor: fields registered with the law's
    /// dimension (2) and `_ek_regular`, `m_rand` with history, `k_coh`
    /// switched to `_ek_cohesive`; the filter has a block for every
    /// dimension-2 regular type of the mesh and both ghost types (upstream
    /// `getElementFilter().initialize(mesh, _spatial_dimension = 2,
    /// _element_kind = _ek_regular)`).
    fn new() -> Self {
        let random = |id: &str, nbc| {
            let mut f = InternalField::new_random(id, nbc, RandomParameter::new(0.0)).unwrap();
            f.set_restriction(2, ElementKind::Regular);
            f
        };
        let mut m_rand = random("m_rand", 1);
        m_rand.initialize_history();
        let mut k_coh = random("k_coh", 1);
        k_coh.set_element_kind(ElementKind::Cohesive);
        let mut p_plain = InternalField::new("p_plain", 1, 0.0).unwrap();
        p_plain.set_restriction(2, ElementKind::Regular);
        let mut filter = ElementTypeMapArray::new("element_filter");
        for g in GhostType::ALL {
            for ty in [TRI, QUAD] {
                filter.alloc(0, 1, ty, g, 0).unwrap();
            }
        }
        Self {
            name: ByteString::default(),
            rho: 0.0,
            filter,
            z_rand: random("z_rand", 1),
            a_rand: random("a_rand", 2),
            m_rand,
            k_coh,
            p_plain,
        }
    }

    fn field(&self, id: &str) -> &InternalField<Real> {
        match id {
            "z_rand" => &self.z_rand,
            "a_rand" => &self.a_rand,
            "m_rand" | "previous_m_rand" => &self.m_rand,
            "k_coh" => &self.k_coh,
            "p_plain" => &self.p_plain,
            _ => panic!("unknown field {id}"),
        }
    }
}

fn ty_of(name: &str) -> ElementType {
    ElementType::ALL
        .into_iter()
        .find(|t| t.name() == name)
        .unwrap_or_else(|| panic!("unknown type {name}"))
}

fn ghost_of(name: &str) -> GhostType {
    match name {
        "not_ghost" => NG,
        "ghost" => G,
        _ => panic!("unknown ghost type {name}"),
    }
}

fn nb_quad(ty: ElementType, _: GhostType) -> usize {
    match ty {
        TRI => 1,
        QUAD => 4,
        _ => 0,
    }
}

/// Compare the law against the oracle's record of `step`.
fn check_step(fx: &Fixture, step: &serde_json::Value, law: &ProbeLaw) {
    let s = step["step"].as_str().unwrap();
    // element filter
    for f in step["filter"].as_array().unwrap() {
        let (ty, g) = (
            ty_of(f["type"].as_str().unwrap()),
            ghost_of(f["ghost"].as_str().unwrap()),
        );
        let expected: Vec<Idx> = f["elements"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| Idx::try_from(e.as_i64().unwrap()).unwrap())
            .collect();
        assert_eq!(
            law.filter.get(ty, g).unwrap().as_slice(),
            expected,
            "{s}: filter {f}"
        );
    }
    // block layout: every oracle block with values exists with that size;
    // zero-size oracle blocks are absent or empty in the port
    let mut nonempty = 0;
    for l in step["layout"].as_array().unwrap() {
        let id = l["field"].as_str().unwrap();
        let (ty, g) = (
            ty_of(l["type"].as_str().unwrap()),
            ghost_of(l["ghost"].as_str().unwrap()),
        );
        let size = usize::try_from(l["size"].as_i64().unwrap()).unwrap();
        let field = law.field(id);
        // (tuples, components) of the block: the history is a plain array,
        // the values are reached through a guard (D-047)
        let block = if id.starts_with("previous_") {
            field
                .previous(ty, g)
                .ok()
                .map(|a| (a.len(), a.nb_component()))
        } else {
            field.get(ty, g).ok().map(|a| (a.len(), a.nb_component()))
        };
        let ours = block.map_or(0, |(len, _)| len);
        if s == "s6" && id == "previous_m_rand" {
            // UD-106: the port compacts the history with the values
            let current = field.get(ty, g).map_or(0, |a| a.len());
            assert_eq!(ours, current, "{s}: {l}: compacted history (UD-106)");
            continue;
        }
        assert_eq!(ours, size, "{s}: layout {l}");
        if size > 0 {
            nonempty += 1;
            assert_eq!(
                block.unwrap().1,
                usize::try_from(l["nb_component"].as_i64().unwrap()).unwrap(),
                "{s}: {l}"
            );
        }
    }
    // the port has no block with values the oracle does not have
    let mut ours_nonempty = 0;
    for id in ["a_rand", "k_coh", "m_rand", "p_plain", "z_rand"] {
        for (_, g, a) in law.field(id).values().as_map().iter_all() {
            let _ = g;
            if !a.is_empty() {
                ours_nonempty += 1;
            }
        }
        if id == "m_rand" && s != "s6" {
            for g in GhostType::ALL {
                for ty in [TRI, QUAD] {
                    if law.m_rand.previous(ty, g).is_ok_and(|a| !a.is_empty()) {
                        ours_nonempty += 1;
                    }
                }
            }
        }
    }
    if s != "s6" {
        assert_eq!(ours_nonempty, nonempty, "{s}: number of non-empty blocks");
    }
    // values of the random fields
    let mut compared = 0;
    for name in fx.quantity_names(&format!("{s}/*")) {
        let parts: Vec<&str> = name.split('/').collect();
        let (id, ty, g) = (parts[1], ty_of(parts[2]), ghost_of(parts[3]));
        let expected = fx.load_f64(name).unwrap().data;
        let ours_g = law.field(id).get(ty, g).unwrap();
        let ours = ours_g.as_slice();
        let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(ours), bits(&expected), "{s}: {name}");
        compared += expected.len();
    }
    // every step with a non-empty oracle block has values (smm_2d_mixed s1:
    // only zero-size blocks)
    assert!(compared > 0 || nonempty == 0, "{s}: no value compared");
    eprintln!("{s}: {compared} values bit-identical");
}

#[test]
fn random_internal_matches_oracle() {
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let fx = Fixture::open_by_id("cl/random_internal/smm_2d").unwrap();
    fx.require_golden().unwrap();
    fx.verify_checksums().unwrap();
    let p = &fx.manifest().parameters;
    assert_eq!(p["nb_quadrature_points"], serde_json::json!([1, 4]));
    let steps = p["steps"].as_array().unwrap();
    let step = |name: &str| steps.iter().find(|s| s["step"] == name).unwrap();

    // `initialize(material.dat)`: the input file's seed
    let dat = std::fs::read_to_string(fx.input_path("material").unwrap()).unwrap();
    let parser = Parser::parse_str(&dat, "law_random_probe.dat").unwrap();
    let seed: i64 = parser
        .root
        .get_parameter_value("seed", akantu_parser::SearchScope::Current)
        .unwrap();
    assert_eq!(serde_json::json!(seed), p["seed"]);
    RandomGenerator::<IdxSlot>::seed(seed);

    let mut law = ProbeLaw::new();
    let section = parser
        .root
        .sub_sections_of(ParserType::Material)
        .next()
        .unwrap();
    law.parse_section(section, false).unwrap();

    // s1: initFull — the not-ghost elements in for_each_element order, one resize
    let s1: Vec<Element> = (0..4)
        .map(|e| Element::new(TRI, e, NG))
        .chain((0..2).map(|e| Element::new(QUAD, e, NG)))
        .collect();
    law.add_elements(&s1, &nb_quad).unwrap();
    check_step(&fx, step("s1"), &law);

    // s2: Material::addElements of ghost elements (list order)
    law.add_elements(
        &[
            Element::new(TRI, 1, G),
            Element::new(QUAD, 0, G),
            Element::new(TRI, 0, G),
        ],
        &nb_quad,
    )
    .unwrap();
    check_step(&fx, step("s2"), &law);

    // s3: setDefaultValue → reset of z_rand (new base value)
    law.z_rand.set_default_value(4.0);
    check_step(&fx, step("s3"), &law);

    // s4: setRandomDistribution → reset of a_rand
    law.a_rand.set_random_distribution(RandomParameter::make(
        -0.5,
        RandomDistributionType::Normal,
        1.0,
        0.125,
    ));
    check_step(&fx, step("s4"), &law);

    // s5: NewElementsEvent [tri 4, quad 2, tri 5]: the handler assigns the
    // new elements type by type (for_each_element over the event filter)
    law.add_elements(
        &[
            Element::new(TRI, 4, NG),
            Element::new(TRI, 5, NG),
            Element::new(QUAD, 2, NG),
        ],
        &nb_quad,
    )
    .unwrap();
    check_step(&fx, step("s5"), &law);

    // s6: eraseElements with the oracle's renumbering
    let removal = &p["removal"];
    let removed: Vec<Element> = removal["removed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            Element::new(
                ty_of(e["type"].as_str().unwrap()),
                Idx::try_from(e["element"].as_i64().unwrap()).unwrap(),
                ghost_of(e["ghost"].as_str().unwrap()),
            )
        })
        .collect();
    let mut numbering = ElementTypeMapArray::<Idx>::new("new_numbering");
    for b in removal["numbering"].as_array().unwrap() {
        let v: Vec<Idx> = b["new_numbering"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| Idx::try_from(x.as_i64().unwrap()).unwrap())
            .collect();
        numbering
            .as_map_mut()
            .insert(
                ty_of(b["type"].as_str().unwrap()),
                ghost_of(b["ghost"].as_str().unwrap()),
                Array::from_vec(v, 1).unwrap(),
            )
            .unwrap();
    }
    law.on_elements_removed(&removed, &numbering, &nb_quad)
        .unwrap();
    check_step(&fx, step("s6"), &law);

    // s7: one more triangle (new mesh index 5)
    law.add_elements(&[Element::new(TRI, 5, NG)], &nb_quad)
        .unwrap();
    check_step(&fx, step("s7"), &law);

    // same number of engine calls in total
    let mut g = RandomGenerator::<IdxSlot>::new();
    let next: Vec<i64> = (0..4).map(|_| i64::from(g.next_u32())).collect();
    assert_eq!(next, fx.load_i64("next_draws").unwrap().data);
}

/// The probe law with the oracle's `random_probe` section applied and the
/// engine seeded like the oracle (the input file's `seed`).
fn seeded_probe_law(fx: &Fixture) -> (ProbeLaw, Parser) {
    let dat = std::fs::read_to_string(fx.input_path("material").unwrap()).unwrap();
    let parser = Parser::parse_str(&dat, "law_random_probe.dat").unwrap();
    let seed: i64 = parser
        .root
        .get_parameter_value("seed", akantu_parser::SearchScope::Current)
        .unwrap();
    assert_eq!(serde_json::json!(seed), fx.manifest().parameters["seed"]);
    RandomGenerator::<IdxSlot>::seed(seed);
    let mut law = ProbeLaw::new();
    let section = parser
        .root
        .sub_sections_of(ParserType::Material)
        .find(|s| s.name == "random_probe")
        .unwrap();
    law.parse_section(section, false).unwrap();
    (law, parser)
}

/// Against the oracle: fixtures/cl/random_internal/
/// smm_2d_mixed (law_random_probe.cc, second case). s1: `initFull` gives the
/// probe material no element (zero-size blocks, no draw); s2: one
/// `add_elements` of the oracle's interleaved not-ghost/ghost list — one
/// resize growing both ghost types of every field (register-id order,
/// `_not_ghost` then `_ghost`, enum order): filter, layout and every random
/// value bit for bit, then the next engine values (same draw count).
/// Mutant M5 (ghost types reversed in `resize_from_filter`) fails here.
#[test]
fn mixed_ghost_resize_matches_oracle() {
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let fx = Fixture::open_by_id("cl/random_internal/smm_2d_mixed").unwrap();
    fx.require_golden().unwrap();
    fx.verify_checksums().unwrap();
    let p = fx.manifest().parameters.clone();
    assert_eq!(p["material_index"], serde_json::json!(1));
    let steps = p["steps"].as_array().unwrap().clone();
    assert_eq!(steps.len(), 2);
    let (mut law, _) = seeded_probe_law(&fx);

    // s1: initFull with an empty filter (zero-size blocks)
    law.resize_internals_from_filter(&nb_quad).unwrap();
    check_step(&fx, &steps[0], &law);

    // s2: one addElements of the recorded mixed list
    let added: Vec<Element> = p["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            Element::new(
                ty_of(e["type"].as_str().unwrap()),
                Idx::try_from(e["element"].as_i64().unwrap()).unwrap(),
                ghost_of(e["ghost"].as_str().unwrap()),
            )
        })
        .collect();
    assert!(added.iter().any(|e| e.ghost_type() == G));
    assert!(added.iter().any(|e| e.ghost_type() == NG));
    law.add_elements(&added, &nb_quad).unwrap();
    check_step(&fx, &steps[1], &law);

    let mut g = RandomGenerator::<IdxSlot>::new();
    let next: Vec<i64> = (0..4).map(|_| i64::from(g.next_u32())).collect();
    assert_eq!(next, fx.load_i64("next_draws").unwrap().data);
}

/// Catches the mutant that reverses the ghost types in
/// `InternalField::resize_from_filter`: one resize that grows both ghost
/// types draws in upstream order. Law A follows the oracle to s3 (s1, s2
/// and s3 checked bit for bit; s3 is a `reset()` of `z_rand` over the
/// not-ghost and ghost blocks, so the oracle pins that order). Law B — the
/// same law with only `z_rand` random — starts from the engine state before
/// s3 and receives every element of s1 and s2 in **one** interleaved
/// `add_elements`: its `z_rand` must equal A's after s3, bit for bit, and
/// both must leave the engine in the same state.
#[test]
fn mixed_ghost_resize_follows_the_oracle_reset_order() {
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let fx = Fixture::open_by_id("cl/random_internal/smm_2d").unwrap();
    fx.require_golden().unwrap();
    fx.verify_checksums().unwrap();
    let steps = fx.manifest().parameters["steps"]
        .as_array()
        .unwrap()
        .clone();
    let step = |name: &str| steps.iter().find(|s| s["step"] == name).unwrap().clone();

    // A: the oracle's s1, s2, s3
    let (mut a, _) = seeded_probe_law(&fx);
    let s1: Vec<Element> = (0..4)
        .map(|e| Element::new(TRI, e, NG))
        .chain((0..2).map(|e| Element::new(QUAD, e, NG)))
        .collect();
    a.add_elements(&s1, &nb_quad).unwrap();
    check_step(&fx, &step("s1"), &a);
    let s2 = [
        Element::new(TRI, 1, G),
        Element::new(QUAD, 0, G),
        Element::new(TRI, 0, G),
    ];
    a.add_elements(&s2, &nb_quad).unwrap();
    check_step(&fx, &step("s2"), &a);
    let before_s3 = RandomGenerator::<IdxSlot>::engine_snapshot();
    a.z_rand.set_default_value(4.0);
    check_step(&fx, &step("s3"), &a);
    let after_s3 = RandomGenerator::<IdxSlot>::engine_snapshot();

    // B: same parameters, only z_rand random, empty, at the engine state
    // before s3
    let (mut b, _) = seeded_probe_law(&fx);
    for (field, nbc) in [(&mut b.a_rand, 2), (&mut b.m_rand, 1), (&mut b.k_coh, 1)] {
        *field = InternalField::new(field.name().to_owned(), nbc, 0.0).unwrap();
        field.set_restriction(2, ElementKind::Regular);
    }
    b.z_rand.set_default_value(4.0); // no block yet: no draw
    RandomGenerator::<IdxSlot>::with_engine(|e| *e = before_s3.clone());
    // per-block order as in A, ghost and not-ghost interleaved
    let mixed = [
        Element::new(TRI, 1, G),
        Element::new(TRI, 0, NG),
        Element::new(QUAD, 0, G),
        Element::new(TRI, 1, NG),
        Element::new(QUAD, 0, NG),
        Element::new(TRI, 0, G),
        Element::new(TRI, 2, NG),
        Element::new(QUAD, 1, NG),
        Element::new(TRI, 3, NG),
    ];
    b.add_elements(&mixed, &nb_quad).unwrap();
    let mut compared = 0;
    for g in GhostType::ALL {
        for ty in [TRI, QUAD] {
            assert_eq!(
                b.filter.get(ty, g).unwrap().as_slice(),
                a.filter.get(ty, g).unwrap().as_slice(),
                "filter {ty} {g:?}"
            );
            let bits = |f: &InternalField<Real>| {
                f.get(ty, g)
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|x| x.to_bits())
                    .collect::<Vec<_>>()
            };
            let (va, vb) = (bits(&a.z_rand), bits(&b.z_rand));
            assert!(!va.is_empty());
            assert_eq!(
                vb, va,
                "z_rand {ty} {g:?}: one mixed resize vs the oracle's reset"
            );
            compared += va.len();
        }
    }
    assert_eq!(compared, 18);
    assert_eq!(
        format!("{:?}", RandomGenerator::<IdxSlot>::engine_snapshot()),
        format!("{after_s3:?}"),
        "same number of draws"
    );
}
