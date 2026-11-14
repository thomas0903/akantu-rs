// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// for the phase-field laws, row CL-002: upstream
// src/model/common/constitutive_laws/constitutive_law_tmpl.hh:63-72
// (`registerInternal` builds every internal with the law's spatial
// dimension) and internal_field_tmpl.hh:31-56 / internal_field.hh:256
// (`_ek_regular` by default): a phase-field law's internals never hold
// blocks of element types of another dimension or kind.

//! `review_v59_regressions` (row CL-002): `PhaseFieldBase::blocks`
//! reports only the `(dimension, _ek_regular)` filter blocks, every
//! phase-field internal carries that restriction, and the random `g_c`
//! draws only for the handled types.

use std::sync::{Mutex, PoisonError};

use akantu_core::array_shared::ArrayRef;
use akantu_core::random::{
    IdxSlot, MinstdRand0, RandomDistributionType, RandomGenerator, RandomParameter,
};
use akantu_core::{Array, Element, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldRef, LawContext, LawPhase, LawSpec, ModelServices,
};
use akantu_phase_field::{EnergySplitKind, PhaseFieldLinear, PhaseFieldQuadratic};

const TRI: ElementType = ElementType::Triangle3;
const SEG: ElementType = ElementType::Segment2;
const COH: ElementType = ElementType::Cohesive2d4;
const NG: GhostType = GhostType::NotGhost;
const G: GhostType = GhostType::Ghost;

/// The process-global `RandomGenerator<Idx>` is shared by the tests of
/// this binary.
static ENGINE: Mutex<()> = Mutex::new(());

fn spec(id: &str) -> LawSpec {
    LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: id.into(),
    }
}

/// Two triangles of the unit square.
fn mesh() -> Mesh {
    let mut mesh = Mesh::new(2, "v59").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
            a.push_node(&x).unwrap();
        }
        let mut c = a.connectivity_mut(TRI, NG);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[1, 3, 2]);
    }
    mesh
}

/// Model services with one integration point for **every** element type,
/// so that a block of an excluded type would hold (and draw) values.
struct Services {
    mesh: Mesh,
    displacement: Array<Real>,
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

/// Every `(field, type, ghost)` block of every internal of `law`.
fn internal_blocks(law: &dyn ConstitutiveLaw) -> Vec<(String, ElementType, GhostType)> {
    let mut out = Vec::new();
    law.for_each_internal(&mut |name, f| {
        if let InternalFieldRef::Real(f) = f {
            for (ty, g, _) in f.values().as_map().iter_all() {
                out.push((name.to_owned(), ty, g));
            }
        }
    });
    out
}

fn assert_only_triangles(law: &dyn ConstitutiveLaw, what: &str) {
    let blocks = internal_blocks(law);
    assert!(!blocks.is_empty(), "{what}: no internal block at all");
    for (name, ty, g) in &blocks {
        assert_eq!(
            *ty, TRI,
            "{what}: internal {name} has a {ty} ({g:?}) block (upstream: never)"
        );
    }
}

/// Both phase-field laws, with a segment (other dimension) and a
/// 2-D cohesive element (other kind) in their filter. The law reports its
/// restriction, `PhaseFieldBase::blocks` reports triangle blocks only, and
/// a direct `resize_internals` with blocks of the excluded types allocates
/// none of them (before the fix every internal took every block).
#[test]
fn v59_phasefield_internals_follow_the_restriction() {
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let services = Services {
        mesh: mesh(),
        displacement: Array::zeros(4, 2),
    };
    let ctx = LawContext {
        phase: LawPhase::Init,
        services: &services,
    };
    let m = mesh();
    let laws: Vec<(&str, Box<dyn ConstitutiveLaw>)> = vec![
        (
            "quadratic",
            Box::new(
                PhaseFieldQuadratic::new(&spec("pf:0:quadratic"), EnergySplitKind::NoSplit, &m)
                    .unwrap(),
            ),
        ),
        (
            "linear",
            Box::new(
                PhaseFieldLinear::new(&spec("pf:1:linear"), EnergySplitKind::NoSplit, &m).unwrap(),
            ),
        ),
    ];
    for (what, mut law) in laws {
        let r = law.internal_restriction().expect("phase-field restriction");
        assert!(
            r.handles(TRI) && !r.handles(SEG) && !r.handles(COH),
            "{what}"
        );
        for e in [
            Element::new(TRI, 0, NG),
            Element::new(SEG, 0, NG),
            Element::new(COH, 0, NG),
            Element::new(TRI, 1, NG),
            Element::new(SEG, 0, G),
        ] {
            law.add_element(e).unwrap();
        }
        law.resize_internals(&[(TRI, NG, 2), (SEG, NG, 1), (COH, NG, 1), (SEG, G, 1)])
            .unwrap();
        assert_only_triangles(law.as_ref(), what);
        law.for_each_internal(&mut |name, f| {
            if let InternalFieldRef::Real(f) = f {
                assert_eq!(f.get(TRI, NG).unwrap().len(), 2, "{what}: {name}");
            }
        });
    }
    // the base's block list (what `init_phase_field` and the model resize
    // with) holds the handled types only
    let mut law =
        PhaseFieldQuadratic::new(&spec("pf:2:quadratic"), EnergySplitKind::NoSplit, &m).unwrap();
    for e in [
        Element::new(TRI, 0, NG),
        Element::new(SEG, 0, NG),
        Element::new(COH, 0, NG),
    ] {
        law.add_element(e).unwrap();
    }
    let blocks = law.base.blocks(&ctx);
    assert!(
        blocks.iter().all(|&(ty, _, _)| ty == TRI),
        "PhaseFieldBase::blocks: {blocks:?}"
    );
    assert!(blocks.contains(&(TRI, NG, 1)), "{blocks:?}");
}

/// The same with the random `g_c` (D-006): one draw per handled
/// quadrature point (not-ghost triangles, then ghost triangles), none for
/// the segment and cohesive blocks; the engine's next value is the one
/// after those draws.
#[test]
fn v59_phasefield_random_gc_draws_only_for_handled_types() {
    let _engine = ENGINE.lock().unwrap_or_else(PoisonError::into_inner);
    let m = mesh();
    let mut law =
        PhaseFieldQuadratic::new(&spec("pf:0:quadratic"), EnergySplitKind::NoSplit, &m).unwrap();
    let gc = RandomParameter::make(5e-3, RandomDistributionType::Uniform, 0.0, 1e-3);
    law.base.g_c.set_random_distribution(gc.clone());
    for e in [
        Element::new(SEG, 0, NG),
        Element::new(TRI, 0, NG),
        Element::new(COH, 0, NG),
        Element::new(TRI, 1, NG),
        Element::new(TRI, 0, G),
        Element::new(SEG, 1, G),
    ] {
        law.add_element(e).unwrap();
    }
    RandomGenerator::<IdxSlot>::seed(7373);
    law.resize_internals(&[
        (SEG, NG, 3),
        (TRI, NG, 2),
        (COH, NG, 2),
        (TRI, G, 1),
        (SEG, G, 1),
    ])
    .unwrap();
    assert_only_triangles(&law, "quadratic");
    let mut replica = MinstdRand0::with_seed(7373);
    let mut p = gc;
    let mut not_ghost = [0.0; 2];
    p.set_values_with(&mut replica, &mut not_ghost);
    let mut ghost = [0.0; 1];
    p.set_values_with(&mut replica, &mut ghost);
    assert_eq!(law.base.g_c.get(TRI, NG).unwrap().as_slice(), &not_ghost);
    assert_eq!(law.base.g_c.get(TRI, G).unwrap().as_slice(), &ghost);
    assert_eq!(
        RandomGenerator::<IdxSlot>::new().next_u32(),
        replica.next_u32(),
        "the global engine drew for an excluded block"
    );
}
