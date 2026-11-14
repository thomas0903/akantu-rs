// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test of internals on their own FE engine / element filter inside a
// solid mechanics model, ported from Akantu (EPFL/LSMS)
// src/model/common/constitutive_laws/constitutive_law.hh:74-86 (registerInternal
// with fe_engine_id / element_filter), constitutive_law_tmpl.hh:344-410
// (onElementsRemoved), :529-578 (flattenInternal), internal_field_tmpl.hh:222-264
// (removeIntegrationPoints), src/model/solid_mechanics/material.cc:771-807
// (extrapolateInternal). Grown from the reproducer.

//! CL-001 inside a model. A user law `u`
//! registers an internal `on_high` on a second FE engine (integration
//! order 2: three points per triangle, one for the default engine) and, in
//! some cases, an internal `on_own` on that engine with its own element
//! filter. (The first reproduction registered `"high"` with the
//! registry's `register(name, mesh, 2)`, whose third argument is the
//! element dimension: that engine has one point per triangle like the
//! default, so its dump output was correct; the defect itself (the
//! consumers ignored the field's engine and filter) stands.)
//!
//! * An element removal (or a reassignment) that such an internal
//!   cannot follow is refused **before any change** — in the capability
//!   check before the mesh commit (recoverable: the revision, the law's
//!   filter and its internals unchanged, the mesh domain usable); a removal
//!   that only renumbers the law's elements commits and leaves the
//!   internals as they are; a reassignment that adds elements to the law
//!   sizes them on their supports.
//! * The dumper and `extrapolate_internal` read such an internal
//!   with its own engine (three values per element) and filter.

use std::path::PathBuf;
use std::sync::Arc;

use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real,
    Result,
};
use akantu_fe::FeEngine;
use akantu_fe::shape::{IntegrationOrders, ShapeFamily};
use akantu_mesh::{MeshPart, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, InternalFieldMut, InternalFieldRef, LawSpec, ModelServices,
    SelectorContext,
};
use akantu_parser::Parser;
use akantu_solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
    SolidMechanicsModel,
};
use akantu_validation::fixtures_root;

const TRI: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;
/// Triangles of `bar_triangle_3.msh`.
const NB_TRI: usize = 70;

#[derive(Debug)]
struct Inert;

impl ForeignMaterialMethods for Inert {
    fn compute_stress(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
    ) -> Result<()> {
        Ok(())
    }
    fn compute_tangent_moduli(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
        _: &mut Array<Real>,
    ) -> Result<()> {
        Ok(())
    }
    fn push_wave_speed(&self, _: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(1.0)
    }
}

/// Law 1 (`v`, plain) for the elements `<= last_to_v`, law 0 (`u`) for the
/// others.
#[derive(Debug)]
struct Split {
    last_to_v: Option<Idx>,
}

impl ConstitutiveLawSelector for Split {
    fn select(&self, _: &SelectorContext<'_>, e: Element) -> Result<Option<usize>> {
        Ok(Some(usize::from(
            self.last_to_v.is_some_and(|l| e.index() <= l),
        )))
    }
}

struct Setup {
    /// Element 0 goes to the plain law `v` (else `u` holds every triangle).
    first_to_v: bool,
    /// Own element filter of `on_own` (none: no such internal).
    own_filter: Option<Vec<Idx>>,
}

fn filter_of(elements: &[Idx]) -> ElementTypeMapArray<Idx> {
    let mut f = ElementTypeMapArray::new("own filter");
    f.as_map_mut()
        .insert(TRI, NG, Array::from_vec(elements.to_vec(), 1).unwrap())
        .unwrap();
    f
}

fn model(setup: &Setup) -> SolidMechanicsModel {
    let f: PathBuf = fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/patch_tests/data")
        .join("bar_triangle_3.msh");
    let mut mesh = akantu_mesh::Mesh::new(2, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, &f, akantu_core::enums::MeshIoType::Gmsh).unwrap();
    assert_eq!(mesh.nb_element(TRI, NG), NB_TRI);
    let dat = "material user [\n  name = u\n  rho = 1\n]\n\
               material user_plain [\n  name = v\n  rho = 1\n]\n";
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "smm",
        Parser::parse_str(dat, "m.dat").unwrap(),
    )
    .unwrap();
    let own = setup.own_filter.clone();
    m.physics
        .factory
        .register(
            "user",
            Arc::new(
                move |spec: &LawSpec, _: &dyn ModelServices| -> Result<Box<dyn Material>> {
                    let mut l = ForeignMaterial::new(spec, "user", Box::new(Inert))?;
                    l.register_internal_real("plain", 1)?;
                    // upstream registerInternal(id, 1, "high"): the law's
                    // filter, another engine
                    l.register_internal_real_on("on_high", 1, "high", None)?;
                    if let Some(own) = &own {
                        // registerInternal(id, 1, "high", filter)
                        l.register_internal_real_on("on_own", 1, "high", Some(filter_of(own)))?;
                    }
                    Ok(Box::new(l))
                },
            ),
        )
        .unwrap();
    m.physics
        .factory
        .register(
            "user_plain",
            Arc::new(
                |spec: &LawSpec, _: &dyn ModelServices| -> Result<Box<dyn Material>> {
                    let mut l = ForeignMaterial::new(spec, "user_plain", Box::new(Inert))?;
                    l.register_internal_real("plain", 1)?;
                    Ok(Box::new(l))
                },
            ),
        )
        .unwrap();
    {
        // the engine "high": integration order 2 on triangles, three points
        // each (registered, then replaced by an engine configured before its
        // precomputation: the registry's `register` takes the default order)
        let shared = m.physics.base.mesh.clone();
        let g = shared.read().unwrap();
        m.physics.base.fems.register("high", &g, 2).unwrap();
        let mut high = FeEngine::new(
            &g,
            Some(2),
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "smm:fem:high",
        )
        .unwrap();
        high.set_integration_orders(IntegrationOrders {
            per_type: vec![(TRI, 2)],
            mass: None,
        })
        .unwrap();
        high.init_shape_functions(&g, NG).unwrap();
        high.init_shape_functions(&g, GhostType::Ghost).unwrap();
        assert_eq!(high.nb_integration_points(TRI, NG).unwrap(), 3);
        *m.physics.base.fems.get_mut("high").unwrap() = high;
    }
    m.set_material_selector(Box::new(Split {
        last_to_v: setup.first_to_v.then_some(0),
    }));
    m.init_full(&ModelOptions::default()).unwrap();
    // values `100 l + q` (law-local element `l`, point `q`) in `on_high`,
    // `1000 + 100 k + q` in `on_own` (own-filter position `k`)
    let law = m.physics.materials.law_mut(0).unwrap();
    law.for_each_internal_mut(&mut |n, f| {
        let base = match n {
            "on_high" => 0.0,
            "on_own" => 1000.0,
            _ => return Ok(()),
        };
        if let InternalFieldMut::Real(f) = f {
            let mut a = f.get_mut(TRI, NG)?;
            for (i, v) in a.as_mut_slice().iter_mut().enumerate() {
                *v = base + (100 * (i / 3) + i % 3) as Real;
            }
        }
        Ok(())
    })
    .unwrap();
    m
}

fn erase(m: &mut SolidMechanicsModel, index: Idx) -> Result<()> {
    let shared = m.physics.base.mesh.clone();
    let (token, mut parts) = m.split_for_mutation();
    shared
        .mutate(
            &mut [(token, &mut parts)],
            |tx| tx.erase_elements(MeshPart::Main, &[Element::new(TRI, index, NG)]),
            None,
        )
        .map(|_| ())
}

/// Law `i`'s triangle filter.
fn law_filter(m: &SolidMechanicsModel, i: usize) -> Vec<Idx> {
    let law = m.physics.materials.law(i).unwrap();
    law.base()
        .element_filter
        .get(TRI, NG)
        .map_or_else(|_| Vec::new(), |a| a.as_slice().to_vec())
}

/// Law `i`'s internal `name` on triangles (empty when absent).
fn internal(m: &SolidMechanicsModel, i: usize, name: &str) -> Vec<Real> {
    let mut out = Vec::new();
    m.physics
        .materials
        .law(i)
        .unwrap()
        .for_each_internal(&mut |n, f| {
            if n == name
                && let InternalFieldRef::Real(f) = f
                && let Ok(a) = f.get(TRI, NG)
            {
                out = a.as_slice().to_vec();
            }
        });
    out
}

/// `100 l + q` for `nb` law-local elements, three points each.
fn engine_values(nb: usize) -> Vec<Real> {
    (0..nb * 3)
        .map(|i| (100 * (i / 3) + i % 3) as Real)
        .collect()
}

/// Control: without the plain law the whole mesh belongs to `u`; removing a
/// triangle of a model whose laws have no internal on their own support
/// commits (control case, now with the plain law `v` only touched).
#[test]
fn cl001_removal_of_a_plain_law_element_commits() {
    let mut m = model(&Setup {
        first_to_v: true,
        own_filter: None,
    });
    let rev = m.physics.base.mesh.revision().unwrap();
    // element 0 is `v`'s; the last triangle (69, `u`'s) fills its hole:
    // `u` loses nothing, its law-local numbering is the identity
    erase(&mut m, 0).unwrap();
    assert!(m.physics.base.mesh.revision().unwrap() > rev);
    let mut expected: Vec<Idx> = (1..69).collect();
    expected.push(0);
    assert_eq!(law_filter(&m, 0), expected);
    assert!(law_filter(&m, 1).is_empty());
    // the internal on the second engine follows the law's filter unchanged
    assert_eq!(internal(&m, 0, "on_high"), engine_values(69));
    assert_eq!(internal(&m, 0, "plain").len(), 69);
}

/// Removing an element of a law with an internal on another FE
/// engine is refused in the capability check before the commit: not fatal,
/// revision, law filter and internals unchanged, and the mesh domain stays
/// usable (a following removal commits).
#[test]
fn cl001_removal_refused_before_the_commit_is_recoverable() {
    let mut m = model(&Setup {
        first_to_v: true,
        own_filter: None,
    });
    let shared = m.physics.base.mesh.clone();
    let rev = shared.revision().unwrap();
    let e = erase(&mut m, 5).expect_err("an element of `u` cannot leave `on_high`");
    assert!(e.fatal_kind().is_none(), "fatal refusal: {e}");
    assert!(
        e.to_string().contains("on_high") && e.to_string().contains("UD-1083"),
        "{e}"
    );
    assert_eq!(shared.revision().unwrap(), rev);
    assert_eq!(law_filter(&m, 0), (1..70).collect::<Vec<Idx>>());
    assert_eq!(law_filter(&m, 1), [0]);
    assert_eq!(internal(&m, 0, "plain").len(), 69);
    assert_eq!(internal(&m, 0, "on_high"), engine_values(69));
    // the domain is not poisoned: the next mutation commits
    erase(&mut m, 0).unwrap();
    assert!(shared.revision().unwrap() > rev);
    assert_eq!(internal(&m, 0, "on_high"), engine_values(69));
}

/// Own element filter: a removal that renumbers an element of the
/// filter (the last triangle, moved into the hole of element 0) is refused
/// before the commit; one that leaves the filter's elements in place
/// commits with the internal unchanged.
#[test]
fn cl001_removal_renumbering_an_own_filter_is_refused() {
    let mut m = model(&Setup {
        first_to_v: true,
        own_filter: Some(vec![69]),
    });
    let rev = m.physics.base.mesh.revision().unwrap();
    let e = erase(&mut m, 0).expect_err("element 69 of the own filter would move");
    assert!(e.fatal_kind().is_none(), "fatal refusal: {e}");
    assert!(
        e.to_string().contains("on_own") && e.to_string().contains("own element filter"),
        "{e}"
    );
    assert_eq!(m.physics.base.mesh.revision().unwrap(), rev);
    assert_eq!(internal(&m, 0, "on_own"), [1000.0, 1001.0, 1002.0]);

    let mut m = model(&Setup {
        first_to_v: true,
        own_filter: Some(vec![10, 11]),
    });
    erase(&mut m, 0).unwrap();
    assert_eq!(
        internal(&m, 0, "on_own"),
        [1000.0, 1001.0, 1002.0, 1100.0, 1101.0, 1102.0]
    );
    let law = m.physics.materials.law(0).unwrap();
    let mut own = Vec::new();
    law.for_each_internal(&mut |n, f| {
        if n == "on_own" {
            own = f
                .element_filter()
                .unwrap()
                .get(TRI, NG)
                .unwrap()
                .as_slice()
                .to_vec();
        }
    });
    assert_eq!(own, [10, 11]);
}

/// Reassignment: a law with an internal on another engine that
/// would lose elements refuses the reassignment before any law changes; a
/// reassignment that only adds elements to it sizes its internals on their
/// supports.
#[test]
fn cl001_reassignment_checks_every_law_before_changing_one() {
    let mut m = model(&Setup {
        first_to_v: true,
        own_filter: Some(vec![10, 11]),
    });
    // element 1 would leave `u` for `v`
    m.set_material_selector(Box::new(Split { last_to_v: Some(1) }));
    let e = m.reassign_material().unwrap_err();
    assert!(matches!(e, Error::Unsupported(_)), "{e}");
    assert_eq!(law_filter(&m, 0), (1..70).collect::<Vec<Idx>>());
    assert_eq!(law_filter(&m, 1), [0]);
    assert_eq!(internal(&m, 0, "on_high"), engine_values(69));
    assert_eq!(internal(&m, 1, "plain").len(), 1);
    let index = m.physics.materials.law_index_map().get(TRI, NG).unwrap();
    assert_eq!(&index.as_slice()[..2], [1, 0]);
    drop(index);

    // every element to `u`: `v` loses element 0 (no such internal), `u`
    // gains it at law-local index 69 (default values on the second engine)
    m.set_material_selector(Box::new(Split { last_to_v: None }));
    m.reassign_material().unwrap();
    let mut expected: Vec<Idx> = (1..70).collect();
    expected.push(0);
    assert_eq!(law_filter(&m, 0), expected);
    assert!(law_filter(&m, 1).is_empty());
    let mut on_high = engine_values(69);
    on_high.extend([0.0; 3]);
    assert_eq!(internal(&m, 0, "on_high"), on_high);
    assert_eq!(internal(&m, 0, "plain").len(), 70);
    assert_eq!(internal(&m, 0, "on_own").len(), 6);
}

/// The `DataArray` named `name` of the single `.vtu` file under `dir`.
fn vtu_array(dir: &std::path::Path, name: &str) -> Vec<f64> {
    let mut vtu = String::new();
    for e in walk(dir) {
        if e.extension().is_some_and(|x| x == "vtu") {
            vtu = std::fs::read_to_string(&e).unwrap();
        }
    }
    let start = vtu
        .find(&format!("Name=\"{name}\""))
        .unwrap_or_else(|| panic!("{name} array"));
    let tag_end = start + vtu[start..].find('>').unwrap();
    let body = &vtu[tag_end + 1..tag_end + vtu[tag_end..].find("</DataArray>").unwrap()];
    body.split_whitespace()
        .map(|s| s.parse().unwrap())
        .collect()
}

fn walk(d: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(d) {
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

/// The original failing case, now asserting the fix: the dump of an internal on
/// another engine reads its three points per element, each element its own
/// (`100 e + q`: the law holds every triangle in order) — 210 values when
/// written raw, their per-element means `100 e + 1` with the default
/// homogenisation of internals (upstream `AvgHomogenizingFunctor` over
/// `getInternalDataPerElem`, the field's engine); an internal on its own
/// filter of another size writes its elements only, zeros elsewhere
/// (upstream `flattenInternal`: the field's engine and filter).
#[test]
fn cl001_dump_reads_an_internal_on_its_own_support() {
    let mut m = model(&Setup {
        first_to_v: false,
        own_filter: Some(vec![10, 11]),
    });
    let dir = std::env::temp_dir().join(format!("cl001-dump-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    m.set_directory(&dir).unwrap();
    m.dumpers.dumpable.set_text_mode().unwrap();
    m.add_dump_field("on_high").unwrap();
    m.add_dump_field("on_own").unwrap();
    // the same internal without the homogenisation: every point
    let mut raw =
        akantu_io::dumper::solid_mechanics_field("on_high", None, false, |_| true).unwrap();
    raw.name = "on_high_raw".into();
    raw.homogenize = false;
    m.dumpers.dumpable.add_dump_field(raw).unwrap();
    m.dump(None, None).unwrap();
    let on_high = vtu_array(&dir, "on_high");
    let on_high_raw = vtu_array(&dir, "on_high_raw");
    let on_own = vtu_array(&dir, "on_own");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(on_high_raw, engine_values(NB_TRI));
    let means: Vec<Real> = (0..NB_TRI).map(|e| (100 * e + 1) as Real).collect();
    assert_eq!(on_high, means);
    let mut expected = vec![0.0; NB_TRI];
    expected[10] = 1001.0;
    expected[11] = 1101.0;
    assert_eq!(on_own, expected);
}

/// `extrapolate_internal`: the points of the internal's engine (the
/// first point with a positive norm: point 1 for element 0, whose point 0
/// is zero), and an internal on its own filter at the element's position
/// in that filter; an element outside that filter is an error.
#[test]
fn cl001_extrapolate_internal_reads_the_internal_support() {
    let m = model(&Setup {
        first_to_v: false,
        own_filter: Some(vec![10, 11]),
    });
    let mut out = [f64::NAN; 2];
    m.extrapolate_internal(0, "on_high", Element::new(TRI, 0, NG), &mut out)
        .unwrap();
    assert_eq!(out, [1.0, 1.0]);
    m.extrapolate_internal(0, "on_high", Element::new(TRI, 5, NG), &mut out)
        .unwrap();
    assert_eq!(out, [500.0, 500.0]);
    m.extrapolate_internal(0, "on_own", Element::new(TRI, 11, NG), &mut out)
        .unwrap();
    assert_eq!(out, [1100.0, 1100.0]);
    let e = m
        .extrapolate_internal(0, "on_own", Element::new(TRI, 12, NG), &mut out)
        .unwrap_err();
    assert!(e.to_string().contains("own element filter"), "{e}");
}
