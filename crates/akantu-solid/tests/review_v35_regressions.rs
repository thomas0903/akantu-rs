// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// for the solid laws: `vg01_*`, the internal-field type
// restriction (upstream src/model/common/constitutive_laws/
// internal_field.hh:160-174 and :245-256, internal_field_tmpl.hh:31-56 and
// :92-175): every InternalField of a law is constructed with the law's
// spatial dimension and `_ek_regular`, and `resize()` / `reset()` /
// initialisation only touch the filter types of that dimension and kind.

//! `review_v35_regressions` (row CL-002): production laws —
//! [`MaterialElastic`], a runtime [`ForeignMaterial`] with a random
//! internal, and a [`SolidMechanicsModel`] reassignment — never allocate,
//! fill or draw internal-field blocks for element types of another spatial
//! dimension (`_segment_2` in 2-D) or another kind (`_cohesive_2d_4`), even
//! when such elements are in the law's element filter.

mod model_common;

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use akantu_core::array_shared::ArrayRef;
use akantu_core::random::{
    IdxSlot, MinstdRand0, RandomDistributionType, RandomGenerator, RandomParameter,
};
use akantu_core::{Array, Element, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, ConstitutiveLawSelector, InternalFieldRef, LawContext, LawPhase, LawSpec,
    ModelServices, SelectorContext,
};
use akantu_model::internal_field::{AnyInternalField, InternalField};
use akantu_parser::Parser;
use akantu_parser::param::ParameterRegistry;
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
    SolidMechanicsModel,
};
use akantu_validation::fixtures_root;
use model_common::read_mesh;

const TRI: ElementType = ElementType::Triangle3;
const SEG: ElementType = ElementType::Segment2;
const COH: ElementType = ElementType::Cohesive2d4;
const NG: GhostType = GhostType::NotGhost;
const G: GhostType = GhostType::Ghost;

/// The process-global `RandomGenerator<Idx>` is shared by the tests of
/// this binary.
static ENGINE: Mutex<()> = Mutex::new(());

fn spec(dim: usize, id: &str) -> LawSpec {
    LawSpec {
        spatial_dimension: dim,
        option: String::new(),
        id: id.into(),
    }
}

/// Model services with one integration point for **every** element type,
/// so that a block of an excluded type would hold (and, for a random
/// field, draw) values.
struct Services {
    mesh: Mesh,
    displacement: Array<Real>,
}

impl Services {
    fn new() -> Self {
        let mut mesh = Mesh::new(2, "v35").unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
                a.push_node(&x).unwrap();
            }
            let mut c = a.connectivity_mut(TRI, NG);
            c.push_tuple(&[0, 1, 2]);
            c.push_tuple(&[1, 3, 2]);
        }
        Self {
            mesh,
            displacement: Array::zeros(4, 2),
        }
    }
}

impl ModelServices for Services {
    fn spatial_dimension(&self) -> usize {
        2
    }
    fn mesh(&self) -> &Mesh {
        &self.mesh
    }
    fn time_step(&self) -> Real {
        0.0
    }
    fn nb_integration_points(&self, _: ElementType, _: GhostType) -> usize {
        1
    }
    fn integration_point_coordinates(
        &self,
        _: ElementType,
        _: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        *out = Array::zeros(filter.len(), 2);
        Ok(())
    }
    fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>> {
        match name {
            "displacement" => Ok((&self.displacement).into()),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }
}

/// Every `(field, type, ghost)` block of every internal of `law`, history
/// blocks included (as `previous_<field>`).
fn internal_blocks(law: &dyn ConstitutiveLaw) -> Vec<(String, ElementType, GhostType, usize)> {
    fn push<T: Clone>(
        out: &mut Vec<(String, ElementType, GhostType, usize)>,
        name: &str,
        f: &InternalField<T>,
    ) {
        for (ty, g, a) in f.values().as_map().iter_all() {
            out.push((name.to_owned(), ty, g, a.len()));
            if let Ok(p) = f.previous(ty, g) {
                out.push((format!("previous_{name}"), ty, g, p.len()));
            }
        }
        // a history block without a current block would be missed above
        for ty in [SEG, COH] {
            for g in GhostType::ALL {
                if f.previous(ty, g).is_ok() && !f.exists(ty, g) {
                    out.push((format!("previous_{name}"), ty, g, usize::MAX));
                }
            }
        }
    }
    let mut out = Vec::new();
    law.for_each_internal(&mut |name, f| match f {
        InternalFieldRef::Real(f) => push(&mut out, name, f),
        InternalFieldRef::Int(f) => push(&mut out, name, f),
        InternalFieldRef::UInt(f) => push(&mut out, name, f),
        InternalFieldRef::Bool(f) => push(&mut out, name, f),
    });
    out
}

fn assert_only_triangles(law: &dyn ConstitutiveLaw, what: &str) {
    let blocks = internal_blocks(law);
    assert!(!blocks.is_empty(), "{what}: no internal block at all");
    for (name, ty, g, _) in &blocks {
        assert_eq!(
            *ty, TRI,
            "{what}: internal {name} has a {ty} ({g:?}) block (upstream: never)"
        );
    }
}

/// A 2-D `MaterialElastic`
/// whose filter holds a triangle, a segment (other dimension) and a 2-D
/// cohesive element (other kind). Neither the explicit-block resize
/// (`ConstitutiveLaw::resize_internals`) nor `initMaterial` (which sizes
/// the internals from the filter) gives any internal — base, thermal or
/// history — a block of the excluded types.
#[test]
fn vg01_elastic_internals_follow_the_restriction() {
    let mut m = MaterialElastic::<2>::new(&spec(2, "m:0:elastic"), None).unwrap();
    m.add_element(Element::new(TRI, 0, NG)).unwrap();
    m.add_element(Element::new(SEG, 0, NG)).unwrap();
    m.add_element(Element::new(COH, 0, NG)).unwrap();
    m.resize_internals(&[(TRI, NG, 1), (SEG, NG, 2), (COH, NG, 2)])
        .unwrap();
    assert_only_triangles(&m, "resize_internals");
    assert_eq!(m.base().stress.get(TRI, NG).unwrap().len(), 1);

    let services = Services::new();
    let ctx = LawContext {
        phase: LawPhase::Init,
        services: &services,
    };
    let mut m = MaterialElastic::<2>::new(&spec(2, "m:1:elastic"), None).unwrap();
    m.set_param("E", akantu_parser::param::ParamValue::Real(1.0))
        .unwrap();
    for e in [
        Element::new(TRI, 0, NG),
        Element::new(SEG, 0, NG),
        Element::new(COH, 0, NG),
        Element::new(TRI, 1, NG),
        Element::new(SEG, 0, G),
    ] {
        m.add_element(e).unwrap();
    }
    m.init_material(&ctx).unwrap();
    assert_only_triangles(&m, "init_material");
    for (name, f) in m.base().internals() {
        assert_eq!(f.get(TRI, NG).unwrap().len(), 2, "{name}");
    }
    // the base reports the blocks it sizes: the handled types only
    let blocks = m.base().blocks(&ctx);
    assert!(
        blocks.iter().all(|&(ty, _, _)| ty == TRI),
        "MaterialBase::blocks: {blocks:?}"
    );
}

#[derive(Debug)]
struct Noop;

impl ForeignMaterialMethods for Noop {
    fn compute_stress(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
    ) -> Result<()> {
        Ok(())
    }
    fn push_wave_speed(&self, _: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(0.0)
    }
}

/// `vg01_*` with a random internal (D-006): a runtime law with a random
/// internal `Yd` and a segment, a cohesive element and triangles (both
/// ghost types) in its filter draws exactly one value per handled
/// quadrature point — the triangles — in upstream order, and the global
/// engine's next value is the one after those draws (an excluded block
/// would shift every later draw).
#[test]
fn vg01_random_internal_draws_only_for_handled_types() {
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let services = Services::new();
    let ctx = LawContext {
        phase: LawPhase::Init,
        services: &services,
    };
    let mut m =
        ForeignMaterial::new(&spec(2, "m:0:random_law"), "random_law", Box::new(Noop)).unwrap();
    let yd = RandomParameter::make(50.0, RandomDistributionType::Uniform, 0.0, 10.0);
    m.state
        .internals
        .register(
            "Yd",
            AnyInternalField::Real(InternalField::new_random("Yd", 1, yd.clone()).unwrap()),
        )
        .unwrap();
    for e in [
        Element::new(SEG, 0, NG),
        Element::new(TRI, 0, NG),
        Element::new(COH, 0, NG),
        Element::new(TRI, 1, NG),
        Element::new(SEG, 1, G),
        Element::new(TRI, 0, G),
    ] {
        m.add_element(e).unwrap();
    }
    RandomGenerator::<IdxSlot>::seed(4242);
    m.init_material(&ctx).unwrap();
    assert_only_triangles(&m, "foreign init_material");

    // independent replica of the expected draws: not-ghost triangles, then
    // the ghost triangle
    let mut replica = MinstdRand0::with_seed(4242);
    let mut p = yd;
    let mut not_ghost = [0.0; 2];
    p.set_values_with(&mut replica, &mut not_ghost);
    let mut ghost = [0.0; 1];
    p.set_values_with(&mut replica, &mut ghost);
    let f = m.state.internals.real("Yd").unwrap();
    assert_eq!(f.get(TRI, NG).unwrap().as_slice(), &not_ghost);
    assert_eq!(f.get(TRI, G).unwrap().as_slice(), &ghost);
    // next draw: the engine advanced by exactly those draws
    assert_eq!(
        RandomGenerator::<IdxSlot>::new().next_u32(),
        replica.next_u32(),
        "the global engine drew for an excluded block"
    );
}

/// Frozen upstream mesh `test/test_model/patch_tests/data/bar_triangle_3.msh`:
/// triangles and, in the main mesh, the boundary segments of group `BC`.
fn bar() -> Mesh {
    let f: PathBuf = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/patch_tests/data")
        .join("bar_triangle_3.msh");
    read_mesh(&f, 2)
}

#[derive(Debug)]
struct Alternate;

impl ConstitutiveLawSelector for Alternate {
    fn select(&self, _: &SelectorContext<'_>, e: Element) -> Result<Option<usize>> {
        Ok(Some(e.idx() % 2))
    }
}

#[derive(Debug)]
struct AllToFirst;

impl ConstitutiveLawSelector for AllToFirst {
    fn select(&self, _: &SelectorContext<'_>, _: Element) -> Result<Option<usize>> {
        Ok(Some(0))
    }
}

/// `vg01_*` through the model: a boundary segment put into a material's
/// filter (upstream `addElements` accepts any element) is ignored by the
/// model's resize of that material's internals when a reassignment adds
/// triangles to it (`reassignMaterial` → `resizeInternals`).
#[test]
fn vg01_model_reassignment_resize_honours_the_restriction() {
    let dat = "material elastic [\n  name = first\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n\
               material elastic [\n  name = second\n  rho = 2\n  E = 3\n  nu = 0.2\n]\n";
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(bar()),
        None,
        "smm",
        Parser::parse_str(dat, "m.dat").unwrap(),
    )
    .unwrap();
    m.set_material_selector(Box::new(Alternate));
    m.init_full(&ModelOptions::new(
        akantu_core::enums::AnalysisMethod::ExplicitLumpedMass,
    ))
    .unwrap();
    let nt = m.physics.base.mesh.read().unwrap().nb_element(TRI, NG);
    assert!(m.physics.base.mesh.read().unwrap().nb_element(SEG, NG) > 0);
    m.material_mut(0)
        .unwrap()
        .add_element(Element::new(SEG, 0, NG))
        .unwrap();
    m.set_material_selector(Box::new(AllToFirst));
    m.reassign_material().unwrap();
    let law = m.material(0).unwrap();
    assert_eq!(law.base().element_filter.get(TRI, NG).unwrap().len(), nt);
    assert_only_triangles(law, "model reassignment");
    let nq = law.base().stress.get(TRI, NG).unwrap().len() / nt;
    assert!(nq > 0);
}

/// The nine inelastic laws with their own
/// `impl ConstitutiveLaw` give their own internals (damage, `Y`, `Yd`,
/// `K0`, plastic strains, Maxwell branches, …) the base's
/// `(dimension, _ek_regular)` restriction. A direct
/// `ConstitutiveLaw::resize_internals` with blocks of a segment (other
/// dimension) and a 2-D cohesive element (other kind) allocates none of
/// them (without the override, the law-owned internals took every block).
#[test]
fn v59_inelastic_laws_restrict_their_own_internals() {
    use akantu_solid::materials::{
        anisotropic_damage::MaterialAnisotropicDamage, drucker_prager::MaterialDruckerPrager,
        marigo::MaterialMarigo, maxwell::MaterialViscoelasticMaxwell,
        mazars::MaterialMazarsElastic, neohookean::MaterialNeohookean,
        phasefield::MaterialPhaseField, plastic::MaterialLinearIsotropicHardening,
        sls_deviatoric::MaterialStandardLinearSolidDeviatoric,
    };
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let laws: Vec<(&str, Box<dyn ConstitutiveLaw>)> = vec![
        (
            "anisotropic_damage",
            Box::new(MaterialAnisotropicDamage::<2>::new(&spec(2, "m:0:ad"), None).unwrap()),
        ),
        (
            "drucker_prager",
            Box::new(MaterialDruckerPrager::<2>::new(&spec(2, "m:1:dp"), None).unwrap()),
        ),
        (
            "marigo",
            Box::new(MaterialMarigo::<2>::new(&spec(2, "m:2:marigo"), None).unwrap()),
        ),
        (
            "maxwell",
            Box::new(MaterialViscoelasticMaxwell::<2>::new(&spec(2, "m:3:mx"), None).unwrap()),
        ),
        (
            "mazars",
            Box::new(MaterialMazarsElastic::<2>::new(&spec(2, "m:4:mazars"), None).unwrap()),
        ),
        (
            "neohookean",
            Box::new(MaterialNeohookean::<2>::new(&spec(2, "m:5:nh"), None).unwrap()),
        ),
        (
            "phasefield",
            Box::new(MaterialPhaseField::<2>::new(&spec(2, "m:6:pf"), None).unwrap()),
        ),
        (
            "plastic",
            Box::new(
                MaterialLinearIsotropicHardening::<2>::new(&spec(2, "m:7:plastic"), None).unwrap(),
            ),
        ),
        (
            "sls_deviatoric",
            Box::new(
                MaterialStandardLinearSolidDeviatoric::<2>::new(&spec(2, "m:8:sls"), None).unwrap(),
            ),
        ),
    ];
    for (what, mut law) in laws {
        assert!(
            law.internal_restriction().is_some(),
            "{what}: no restriction"
        );
        for e in [
            Element::new(TRI, 0, NG),
            Element::new(SEG, 0, NG),
            Element::new(COH, 0, NG),
        ] {
            law.add_element(e).unwrap();
        }
        law.resize_internals(&[(TRI, NG, 1), (SEG, NG, 2), (COH, NG, 2), (SEG, G, 1)])
            .unwrap();
        assert_only_triangles(law.as_ref(), what);
        // every internal got its triangle block
        law.for_each_internal(&mut |name, f| {
            let n = match f {
                InternalFieldRef::Real(f) => f.get(TRI, NG).map(|a| a.len()),
                InternalFieldRef::Int(f) => f.get(TRI, NG).map(|a| a.len()),
                InternalFieldRef::UInt(f) => f.get(TRI, NG).map(|a| a.len()),
                InternalFieldRef::Bool(f) => f.get(TRI, NG).map(|a| a.len()),
            };
            assert_eq!(n.ok(), Some(1), "{what}: internal {name}");
        });
    }
}

/// with the random internals (D-006): `marigo`'s `Yd` and
/// `mazars`'s `K0`, resized directly with blocks of excluded types (the
/// reproducer path), draw exactly one value per handled quadrature
/// point (not-ghost triangles, then ghost triangles) and nothing for the
/// segment and cohesive blocks: the values equal an independent replica of
/// the engine and the engine's next value is the one after those draws.
#[test]
fn v59_marigo_and_mazars_random_thresholds_draw_only_for_handled_types() {
    use akantu_solid::materials::{marigo::MaterialMarigo, mazars::MaterialMazarsElastic};
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let blocks = [
        (SEG, NG, 3),
        (TRI, NG, 2),
        (COH, NG, 2),
        (TRI, G, 1),
        (SEG, G, 2),
        (COH, G, 1),
    ];
    let elements = [
        Element::new(SEG, 0, NG),
        Element::new(TRI, 0, NG),
        Element::new(COH, 0, NG),
        Element::new(TRI, 1, NG),
        Element::new(TRI, 0, G),
        Element::new(SEG, 1, G),
        Element::new(COH, 0, G),
    ];
    let check = |what: &str, field: &InternalField<Real>, p: RandomParameter, seed: u64| {
        let mut replica = MinstdRand0::with_seed(seed);
        let mut p = p;
        let mut not_ghost = [0.0; 2];
        p.set_values_with(&mut replica, &mut not_ghost);
        let mut ghost = [0.0; 1];
        p.set_values_with(&mut replica, &mut ghost);
        assert_eq!(field.get(TRI, NG).unwrap().as_slice(), &not_ghost, "{what}");
        assert_eq!(field.get(TRI, G).unwrap().as_slice(), &ghost, "{what}");
        for (ty, g, _) in blocks {
            if ty != TRI {
                assert!(!field.exists(ty, g), "{what}: {ty} ({g:?}) block");
            }
        }
        assert_eq!(
            RandomGenerator::<IdxSlot>::new().next_u32(),
            replica.next_u32(),
            "{what}: the global engine drew for an excluded block"
        );
    };

    let yd = RandomParameter::make(50.0, RandomDistributionType::Uniform, 0.0, 10.0);
    let mut marigo = MaterialMarigo::<2>::new(&spec(2, "m:0:marigo"), None).unwrap();
    marigo.yd.set_random_distribution(yd.clone());
    for e in elements {
        marigo.add_element(e).unwrap();
    }
    RandomGenerator::<IdxSlot>::seed(5151);
    marigo.resize_internals(&blocks).unwrap();
    assert_only_triangles(&marigo, "marigo");
    check("marigo Yd", &marigo.yd, yd, 5151);

    let k0 = RandomParameter::make(1e-4, RandomDistributionType::Uniform, 0.0, 1e-5);
    let mut mazars = MaterialMazarsElastic::<2>::new(&spec(2, "m:1:mazars"), None).unwrap();
    mazars.k0.set_random_distribution(k0.clone());
    for e in elements {
        mazars.add_element(e).unwrap();
    }
    RandomGenerator::<IdxSlot>::seed(6262);
    mazars.resize_internals(&blocks).unwrap();
    assert_only_triangles(&mazars, "mazars");
    check("mazars K0", &mazars.k0, k0, 6262);
}
