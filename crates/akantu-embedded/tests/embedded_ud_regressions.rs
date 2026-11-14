// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): regression tests of the upstream defects
// of src/model/solid_mechanics/solid_mechanics_model_embedded_interface/*
// and materials/material_embedded/* (docs/UPSTREAM_DEFECTS.md UD-587…599).

//! Regression tests of the upstream-defect handling (D-003) and of the
//! model API: default (corrected) behaviour and compatibility switches.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Error, GhostType, Real};
use akantu_embedded::{
    EmbeddedInterfaceModel, EmbeddedInterfaceModelOptions, InterfaceMeshDataMaterialSelector,
    MaterialReinforcement, REINFORCEMENT_DUMPER,
};
use akantu_mesh::Mesh;
use akantu_parser::ParamValue;

const CONCRETE: &str = "material elastic [\n name = concrete\n E = 1e3\n nu = 0.2\n]\n";

fn reinforcement(name: &str, extra: &str) -> String {
    format!("material reinforcement elastic [\n name = {name}\n E = 2e4\n area = 0.01\n{extra}]\n")
}

fn one_bar(name: &str) -> Mesh {
    common::reinforcement_mesh(2, &[&[0.1, 0.3], &[1.7, 0.6]], &[[0, 1]], &[name])
}

fn model_2d(primitive: Mesh, dat: &str) -> EmbeddedInterfaceModel {
    common::model(
        common::grid_triangles(4, 2, 2.0, 1.0, "concrete"),
        primitive,
        dat,
    )
}

fn static_opts() -> EmbeddedInterfaceModelOptions {
    EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static)
}

fn real(v: ParamValue) -> Real {
    match v {
        ParamValue::Real(x) => x,
        other => panic!("not a real: {other:?}"),
    }
}

#[test]
fn ud588_pre_stress_parameter_is_the_internal_default() {
    let dat = format!(
        "{CONCRETE}{}",
        reinforcement("steel", " pre_stress = 12.5\n")
    );
    let mut model = model_2d(one_bar("steel"), &dat);
    model.init_full(&static_opts()).unwrap();
    let i = model.smm.material_index("steel").unwrap();
    let law = model.smm.material(i).unwrap();
    assert_eq!(real(law.get_param("pre_stress").unwrap()), 12.5);
    assert_eq!(real(law.get_param("area").unwrap()), 0.01);
    let r = law
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialReinforcement<2>>()
        .unwrap();
    let ps = r
        .pre_stress
        .get(ElementType::Segment2, GhostType::NotGhost)
        .unwrap();
    assert!(!ps.is_empty());
    assert!(ps.as_slice().iter().all(|&x| x == 12.5));
    drop(ps);
    // `set` resets the internal to the new default (upstream setDefaultValue)
    model
        .smm
        .material_mut(i)
        .unwrap()
        .set_param("pre_stress", ParamValue::Real(-3.0))
        .unwrap();
    let r = model
        .smm
        .material(i)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<MaterialReinforcement<2>>()
        .unwrap();
    assert!(
        r.pre_stress
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .iter()
            .all(|&x| x == -3.0)
    );
    // area defaults to 1
    let dat = format!("{CONCRETE}material reinforcement elastic [\n name = steel\n E = 1\n]\n");
    let mut model = model_2d(one_bar("steel"), &dat);
    model.init_full(&static_opts()).unwrap();
    let law = model.smm.material_by_name("steel").unwrap();
    assert_eq!(real(law.get_param("area").unwrap()), 1.0);
    assert_eq!(real(law.get_param("pre_stress").unwrap()), 0.0);
    // the parameters of the 1-D elastic law come first (upstream class
    // chain), then `area`, `pre_stress`
    let names = law.param_names();
    let pos = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!(pos("E") < pos("area") && pos("area") < pos("pre_stress"));
}

#[test]
fn ud589_reinforcement_law_is_one_dimensional() {
    let dat = format!("{CONCRETE}{}", reinforcement("steel", ""));
    let mut model = model_2d(one_bar("steel"), &dat);
    model.init_full(&static_opts()).unwrap();
    let law = model.smm.material_by_name("steel").unwrap();
    assert_eq!(law.base().spatial_dimension, 1);
    assert_eq!(law.base().stress.nb_component(), 1);
    assert_eq!(law.base().grad_u.nb_component(), 1);
    assert_eq!(law.law_type(), "reinforcement");
    // 1-D: ν forced to 0 (upstream MaterialElastic<1>::initMaterial)
    assert_eq!(real(law.get_param("nu").unwrap()), 0.0);
}

#[test]
fn ud592_finite_deformation_is_refused() {
    let dat = format!(
        "{CONCRETE}{}",
        reinforcement("steel", " finite_deformation = true\n")
    );
    let mut model = model_2d(one_bar("steel"), &dat);
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(e.to_string().contains("finite_deformation"), "{e}");
}

#[test]
fn ud594_eigen_grad_u_is_ignored_as_upstream() {
    let run = |extra: &str| {
        let dat = format!("{CONCRETE}{}", reinforcement("steel", extra));
        let mut model = model_2d(one_bar("steel"), &dat);
        model.init_full(&static_opts()).unwrap();
        model.assemble_internal_forces().unwrap();
        model.smm.internal_force().unwrap().as_slice().to_vec()
    };
    assert_eq!(run(""), run(" eigen_grad_u = [[0.1]]\n"));
}

#[test]
fn ud595_unknown_interface_name_refused_by_default_compat_law_0() {
    // the bar is named `rebar`, the only reinforcement law `steel`
    let dat = format!("{}{CONCRETE}", reinforcement("steel", ""));
    let mut model = model_2d(one_bar("rebar"), &dat);
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(e.to_string().contains("UD-595"), "{e}");
    // compat: law 0 (here the reinforcement law `steel`, first in the file)
    let mut model = model_2d(one_bar("rebar"), &dat);
    model.compat.interface_selector_falls_back_to_first_law = true;
    model.init_full(&static_opts()).unwrap();
    let law = model.smm.material_by_name("steel").unwrap();
    assert!(
        !law.base()
            .element_filter
            .get(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ud596_wrong_law_kinds_are_refused() {
    // an interface segment named after a bulk law
    let dat = format!("{CONCRETE}{}", reinforcement("steel", ""));
    let mut model = model_2d(one_bar("concrete"), &dat);
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(e.to_string().contains("UD-596"), "{e}");
    // a bulk element given to a reinforcement law (background named `steel`)
    let mesh = common::grid_triangles(4, 2, 2.0, 1.0, "steel");
    let mut model = common::model(mesh, one_bar("steel"), &dat);
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(e.to_string().contains("background mesh"), "{e}");
    // compat UD-595 still refuses law 0 when it is not a reinforcement
    let mut model = model_2d(one_bar("rebar"), &dat);
    model.compat.interface_selector_falls_back_to_first_law = true;
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(e.to_string().contains("UD-596"), "{e}");
}

#[test]
fn ud599_near_duplicates_along_a_shared_edge() {
    // a 3-D bar along a mesh line shared by several tetrahedra
    let primitive = || {
        common::reinforcement_mesh(
            3,
            &[&[0.0, 0.5, 0.5], &[1.0, 0.5, 0.5]],
            &[[0, 1]],
            &["steel"],
        )
    };
    let dat = format!(
        "material elastic [\n name = concrete\n E = 1e3\n nu = 0.2\n]\n{}",
        reinforcement("steel", "")
    );
    let length = |model: &EmbeddedInterfaceModel| -> Real {
        common::interface_segments(model)
            .iter()
            .map(|(a, b, ..)| common::dist(a, b))
            .sum()
    };
    let mut fixed = common::model(
        common::grid_tetrahedra(3, 1.0, "concrete"),
        primitive(),
        &dat,
    );
    fixed.init_full(&static_opts()).unwrap();
    assert!((length(&fixed) - 1.0).abs() <= 1e-12);
    // compat: exact deduplication only — some pieces are counted twice
    let mut upstream = common::model(
        common::grid_tetrahedra(3, 1.0, "concrete"),
        primitive(),
        &dat,
    );
    upstream.intersector.remove_near_duplicates = false;
    upstream.init_full(&static_opts()).unwrap();
    assert!(length(&upstream) > 1.0 + 1e-3, "{}", length(&upstream));
}

#[test]
fn reinforcement_allocator_errors_verbatim() {
    let dat = format!("{CONCRETE}material reinforcement plastic [\n name = steel\n E = 1\n]\n");
    let mut model = model_2d(one_bar("steel"), &dat);
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(
        e.to_string()
            .contains("Reinforcement typeplastic is not recognized"),
        "{e}"
    );
}

#[test]
fn no_embedded_model_in_1d() {
    let mut mesh = Mesh::new(1, "mesh").unwrap();
    {
        let mut acc = akantu_mesh::MeshAccessor::new(&mut mesh);
        acc.push_node(&[0.0]).unwrap();
        acc.push_node(&[1.0]).unwrap();
        acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
            .try_push_tuple(&[0, 1])
            .unwrap();
    }
    common::name_elements(&mut mesh, ElementType::Segment2, 1, "concrete");
    let primitive = common::reinforcement_mesh(1, &[&[0.2], &[0.8]], &[[0, 1]], &["steel"]);
    let dat = "material elastic [\n name = concrete\n E = 1\n]\n";
    let mut model = common::model(mesh, primitive, dat);
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(e.to_string().contains("No embedded model in 1D"), "{e}");
}

#[test]
fn missing_physical_names_on_the_primitive_mesh() {
    let mut primitive = Mesh::new(2, "reinforcement_mesh").unwrap();
    {
        let mut acc = akantu_mesh::MeshAccessor::new(&mut primitive);
        acc.push_node(&[0.1, 0.1]).unwrap();
        acc.push_node(&[0.9, 0.2]).unwrap();
        acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost)
            .try_push_tuple(&[0, 1])
            .unwrap();
    }
    let dat = format!("{CONCRETE}{}", reinforcement("steel", ""));
    let mut model = model_2d(primitive, &dat);
    let e = model.init_full(&static_opts()).unwrap_err();
    assert!(
        e.to_string().contains("You must define physical names"),
        "{e}"
    );
}

#[test]
fn init_full_twice_and_without_intersections() {
    let dat = format!("{CONCRETE}{}", reinforcement("steel", ""));
    let mut model = model_2d(one_bar("steel"), &dat);
    model.init_full(&static_opts()).unwrap();
    assert!(model.init_full(&static_opts()).is_err());
    // nor a second assignment through init_materials
    assert!(model.init_materials().is_err());
    // has_intersections = false: empty interface mesh, reinforcement law
    // without elements, the model still solves
    let mut model = model_2d(one_bar("steel"), &dat);
    let opts = EmbeddedInterfaceModelOptions {
        has_intersections: false,
        ..static_opts()
    };
    model.init_full(&opts).unwrap();
    assert_eq!(
        model
            .interface_mesh()
            .nb_element(ElementType::Segment2, GhostType::NotGhost),
        0
    );
    let law = model.smm.material_by_name("steel").unwrap();
    assert!(
        law.base()
            .element_filter
            .get(ElementType::Segment2, GhostType::NotGhost)
            .is_ok_and(|f| f.is_empty())
    );
    model.assemble_internal_forces().unwrap();
    assert_eq!(model.nb_reinforcement_materials(), 1);
}

#[test]
fn custom_interface_selector() {
    // a user selector replaces the default one and is evaluated with the
    // interface mesh in its context: every interface element to law 2
    #[derive(Debug)]
    struct Always(usize);
    impl akantu_model::constitutive_law::ConstitutiveLawSelector for Always {
        fn select(
            &self,
            ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
            _element: akantu_core::Element,
        ) -> akantu_core::Result<Option<usize>> {
            // the context's mesh is the interface mesh
            assert_eq!(ctx.mesh.id(), "interface_mesh");
            Ok(Some(self.0))
        }
    }
    let dat = format!(
        "{CONCRETE}{}{}",
        reinforcement("steel_a", ""),
        reinforcement("steel_b", "")
    );
    let mut model = model_2d(one_bar("whatever"), &dat);
    model.set_interface_material_selector(Box::new(Always(2)));
    model.init_full(&static_opts()).unwrap();
    let n = |name: &str| {
        model
            .smm
            .material_by_name(name)
            .unwrap()
            .base()
            .element_filter
            .get(ElementType::Segment2, GhostType::NotGhost)
            .map_or(0, |f| f.len())
    };
    assert_eq!(n("steel_a"), 0);
    assert!(n("steel_b") > 0);
    // the default selector reads `physical_names`
    let s = InterfaceMeshDataMaterialSelector::new("physical_names");
    assert_eq!(s.data_name(), "physical_names");
}

#[test]
fn reinforcement_dumper_writes_the_interface_mesh() {
    let dat = format!(
        "{CONCRETE}{}",
        reinforcement("steel", " pre_stress = 2.5\n")
    );
    let mut model = model_2d(one_bar("steel"), &dat);
    model.init_full(&static_opts()).unwrap();
    model.assemble_internal_forces().unwrap();
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reinforcement_dumper");
    let _ = std::fs::remove_dir_all(&out);
    model
        .set_directory_to_dumper(REINFORCEMENT_DUMPER, &out)
        .unwrap();
    model
        .set_base_name_to_dumper(REINFORCEMENT_DUMPER, "reinforcement")
        .unwrap();
    for f in [
        "stress",
        "pre_stress",
        "directing_cosines",
        "material_index",
    ] {
        model
            .add_dump_field_to_dumper(REINFORCEMENT_DUMPER, f)
            .unwrap();
    }
    model
        .add_dump_field_tensor_to_dumper(REINFORCEMENT_DUMPER, "grad_u")
        .unwrap();
    // nodal fields of the background are refused on this dumper
    assert!(
        model
            .add_dump_field_vector_to_dumper(REINFORCEMENT_DUMPER, "displacement")
            .is_err()
    );
    model.dump_named(REINFORCEMENT_DUMPER, None, None).unwrap();
    let mut vtu = Vec::new();
    for entry in walk(&out) {
        if entry.extension().is_some_and(|e| e == "vtu") {
            vtu.push(entry);
        }
    }
    assert_eq!(vtu.len(), 1, "{vtu:?}");
    let text = std::fs::read_to_string(&vtu[0]).unwrap();
    let n = model
        .interface_mesh()
        .nb_element(ElementType::Segment2, GhostType::NotGhost);
    assert!(text.contains(&format!("NumberOfCells=\"{n}\"")), "{text}");
    for f in [
        "stress",
        "pre_stress",
        "directing_cosines",
        "material_index",
        "grad_u",
    ] {
        assert!(text.contains(&format!("Name=\"{f}\"")), "field {f} missing");
    }
    // other dumpers: errors returned by default, swallowed in compat (UD-597)
    assert!(
        model
            .add_dump_field_to_dumper("no_such_dumper", "stress")
            .is_err()
    );
    model.compat.ignore_dump_field_errors = true;
    model
        .add_dump_field_to_dumper("no_such_dumper", "stress")
        .unwrap();
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

#[test]
fn reinforcement_does_not_change_the_mass() {
    // the lumped mass is the background's alone (upstream: the mass
    // assembly visits background elements only)
    let with = format!(
        "material elastic [\n name = concrete\n E = 1e3\n nu = 0.2\n rho = 2.0\n]\n{}",
        reinforcement("steel", " rho = 1e6\n")
    );
    let without = "material elastic [\n name = concrete\n E = 1e3\n nu = 0.2\n rho = 2.0\n]\n";
    let mass = |dat: &str, bar: bool| {
        let mut model = if bar {
            model_2d(one_bar("steel"), dat)
        } else {
            model_2d(one_bar("concrete_bar_unused"), dat)
        };
        let opts = EmbeddedInterfaceModelOptions {
            has_intersections: bar,
            ..EmbeddedInterfaceModelOptions::new(AnalysisMethod::ExplicitLumpedMass)
        };
        model.init_full(&opts).unwrap();
        model.smm.assemble_mass_lumped().unwrap();
        model.smm.mass().unwrap().as_slice().to_vec()
    };
    assert_eq!(mass(&with, true), mass(without, false));
}

#[test]
fn reinforcement_lookup_and_engine() {
    let dat = format!("{CONCRETE}{}", reinforcement("steel", ""));
    let mut model = model_2d(one_bar("steel"), &dat);
    model.init_full(&static_opts()).unwrap();
    assert!(model.reinforcement("steel").is_ok());
    assert!(model.reinforcement("concrete").is_err());
    let fe = model
        .fe_engine(akantu_embedded::INTERFACE_FE_ENGINE)
        .unwrap();
    assert_eq!(fe.element_dimension(), 1);
    assert!(model.fe_engine("").is_ok());
    assert!(matches!(
        model.fe_engine("nope"),
        Err(Error::NotFound { .. }) | Err(_)
    ));
}

/// Decode a ParaView `format="binary"` `DataArray` of `text` (base64 of a
/// 4-byte size word followed by the payload) into its payload bytes.
fn vtu_array(text: &str, name: &str) -> Vec<u8> {
    let start = text
        .find(&format!("Name=\"{name}\""))
        .unwrap_or_else(|| panic!("field {name} missing"));
    let body = &text[start..];
    let open = body.find('>').unwrap() + 1;
    let close = body[open..].find("</DataArray>").unwrap() + open;
    let mut bits = 0u32;
    let mut nb = 0;
    let mut bytes = Vec::new();
    for c in body[open..close]
        .bytes()
        .filter(|c| !c.is_ascii_whitespace())
    {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => continue, // padding
        };
        bits = (bits << 6) | u32::from(v);
        nb += 6;
        if nb >= 8 {
            nb -= 8;
            bytes.push((bits >> nb) as u8);
            bits &= (1 << nb) - 1;
        }
    }
    bytes.split_off(4)
}

fn vtu_reals(text: &str, name: &str) -> Vec<Real> {
    vtu_array(text, name)
        .as_chunks::<8>()
        .0
        .iter()
        .map(|b| Real::from_le_bytes(*b))
        .collect()
}

/// EMB-005, as for the contact dumper: the dumped fields of the
/// `reinforcement` dumper by value. Two bars of two laws (`E`, `pre_stress`
/// per law) in a background under the linear displacement `u = (a x + b y,
/// c x + d y)` (constant strain `ε`): per interface element the dump holds
/// `directing_cosines = (l², m², l m)` of the bar the element lies on,
/// `grad_u = l² ε₁₁ + m² ε₂₂ + 2 l m ε₁₂`, `stress = E_law grad_u +
/// pre_stress_law` and `material_index` = the law's index in the model.
#[test]
fn reinforcement_dump_fields_by_value() {
    let law = |name: &str, e: Real, pre: Real| {
        format!(
            "material reinforcement elastic [\n name = {name}\n E = {e}\n area = 0.01\n \
             pre_stress = {pre}\n]\n"
        )
    };
    let dat = format!(
        "{CONCRETE}{}{}",
        law("steel_a", 2e4, 2.5),
        law("steel_b", 5e3, -1.0)
    );
    let bars = [([0.1, 0.3], [1.7, 0.6]), ([0.2, 0.9], [1.5, 0.1])];
    let primitive = common::reinforcement_mesh(
        2,
        &[&bars[0].0, &bars[0].1, &bars[1].0, &bars[1].1],
        &[[0, 1], [2, 3]],
        &["steel_a", "steel_b"],
    );
    let mut model = model_2d(primitive, &dat);
    model.init_full(&static_opts()).unwrap();
    let (a, b, c, d) = (1e-3, 2e-3, -5e-4, 3e-3);
    let x = common::nodes(&model);
    {
        let mut u = model.smm.displacement_mut().unwrap();
        for (n, p) in x.iter().enumerate() {
            u.as_mut_slice()[2 * n] = a * p[0] + b * p[1];
            u.as_mut_slice()[2 * n + 1] = c * p[0] + d * p[1];
        }
    }
    model.assemble_internal_forces().unwrap();
    let out = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("reinforcement_dumper_values");
    let _ = std::fs::remove_dir_all(&out);
    model
        .set_directory_to_dumper(REINFORCEMENT_DUMPER, &out)
        .unwrap();
    model
        .set_base_name_to_dumper(REINFORCEMENT_DUMPER, "reinforcement")
        .unwrap();
    for f in [
        "stress",
        "pre_stress",
        "directing_cosines",
        "material_index",
    ] {
        model
            .add_dump_field_to_dumper(REINFORCEMENT_DUMPER, f)
            .unwrap();
    }
    model
        .add_dump_field_tensor_to_dumper(REINFORCEMENT_DUMPER, "grad_u")
        .unwrap();
    model.dump_named(REINFORCEMENT_DUMPER, None, None).unwrap();
    let vtu: Vec<_> = walk(&out)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "vtu"))
        .collect();
    assert_eq!(vtu.len(), 1, "{vtu:?}");
    let text = std::fs::read_to_string(&vtu[0]).unwrap();

    let cos = vtu_reals(&text, "directing_cosines");
    let grad_u = vtu_reals(&text, "grad_u");
    let stress = vtu_reals(&text, "stress");
    let pre_stress = vtu_reals(&text, "pre_stress");
    let mat_idx: Vec<i32> = vtu_array(&text, "material_index")
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| i32::from_le_bytes(*b))
        .collect();
    let n = model
        .interface_mesh()
        .nb_element(ElementType::Segment2, GhostType::NotGhost);
    assert!(n >= 2);
    assert_eq!(cos.len(), 3 * n);
    assert_eq!(
        [grad_u.len(), stress.len(), pre_stress.len(), mat_idx.len()],
        [n; 4]
    );
    // (law name, E, pre_stress, [l², m², l m]) of the two bars
    let expected: Vec<(&str, Real, Real, [Real; 3])> = bars
        .iter()
        .zip([("steel_a", 2e4, 2.5), ("steel_b", 5e3, -1.0)])
        .map(|(bar, (name, e, pre))| {
            let t = [bar.1[0] - bar.0[0], bar.1[1] - bar.0[1]];
            let len = (t[0] * t[0] + t[1] * t[1]).sqrt();
            let (l, m) = (t[0] / len, t[1] / len);
            (name, e, pre, [l * l, m * m, l * m])
        })
        .collect();
    let mut per_bar = [0usize; 2];
    for el in 0..n {
        let k = &cos[3 * el..3 * el + 3];
        let bar = expected
            .iter()
            .position(|(.., c3)| c3.iter().zip(k).all(|(x, y)| (x - y).abs() < 1e-12))
            .unwrap_or_else(|| panic!("element {el}: directing cosines {k:?} match no bar"));
        per_bar[bar] += 1;
        let (name, e, pre, c3) = &expected[bar];
        let eps = c3[0] * a + c3[1] * d + 2. * c3[2] * 0.5 * (b + c);
        let tol = |w: Real| 1e-11 * (1. + w.abs());
        assert!(
            (grad_u[el] - eps).abs() < tol(eps),
            "grad_u[{el}] {} vs {eps}",
            grad_u[el]
        );
        assert!(
            (stress[el] - (e * eps + pre)).abs() < tol(e * eps + pre),
            "stress[{el}] {} vs {}",
            stress[el],
            e * eps + pre
        );
        assert_eq!(pre_stress[el], *pre, "pre_stress[{el}]");
        let idx = model.smm.material_index(name).unwrap();
        assert_eq!(
            mat_idx[el],
            i32::try_from(idx).unwrap(),
            "material_index[{el}]"
        );
    }
    assert!(per_bar.iter().all(|&c| c > 0), "{per_bar:?}");
    assert_eq!(per_bar.iter().sum::<usize>(), n);
}
