// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs value-level tests on upstream input files
// (tests/fixtures/…, copied from the reference tree at 35a85178).

//! Hand-derived expectations for 28 upstream input files, covering every
//! value kind and grammar feature. Each assertion cites the upstream rule it
//! relies on (`input_file_parser.hh` = IFG, `algebraic_parser_x3.cc` = X3,
//! `algebraic_parser.hh` = Qi, `parser_tmpl.hh` = conversions,
//! `aka_random_generator.hh` = random parameters). Number literals with at
//! most 15 significant digits and a decimal exponent of magnitude ≤ 22 are
//! computed by Spirit as one exactly-rounded multiplication/division of two
//! exact doubles, i.e. they equal the Rust literal (correct rounding). The
//! exact numerics of longer literals are covered by `tests/oracle.rs`.

mod common;

use akantu_core::tensor::MatrixX;
use akantu_parser::{
    Parser, ParserSection, ParserType, RandomDistribution, RandomParameter, SearchScope,
};
use std::f64::consts::PI;

use ParserType as T;

fn parse(rel: &str) -> Parser {
    common::parse_fixture(rel).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// (type, name, option) of the sub-sections, in file order.
fn subs(s: &ParserSection) -> Vec<(ParserType, &str, &str)> {
    s.sub_sections
        .iter()
        .map(|c| (c.section_type, c.name.as_str(), c.option.as_str()))
        .collect()
}

fn raw<'a>(s: &'a ParserSection, name: &str) -> &'a str {
    s.parameter(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .value
        .to_str()
        .expect("upstream fixtures are ASCII")
}

fn real(s: &ParserSection, name: &str) -> f64 {
    s.get_parameter_value(name, SearchScope::Current)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn vector(s: &ParserSection, name: &str) -> Vec<f64> {
    s.get_parameter_value(name, SearchScope::Current)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn matrix(s: &ParserSection, name: &str) -> Vec<Vec<f64>> {
    let m: MatrixX = s
        .get_parameter_value(name, SearchScope::Current)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    (0..m.rows())
        .map(|i| (0..m.cols()).map(|j| m.at(i, j)).collect())
        .collect()
}

fn random(s: &ParserSection, name: &str) -> RandomParameter {
    s.get_parameter_value(name, SearchScope::Current)
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn pos(s: &ParserSection, name: &str) -> (usize, usize) {
    let p = s.parameter(name).unwrap();
    (p.line, p.column)
}

#[test]
fn f01_test_parser_input_file() {
    let p = parse("test/test_io/test_parser/input_file.dat");
    let g = &p.root;
    // IFG section: `material elastic opt1 [` → name + option (+space >> name)
    assert_eq!(subs(g), vec![(T::Material, "elastic", "opt1")]);
    let m = &g.sub_sections[0];
    assert_eq!(subs(m), vec![(T::Rules, "material", "")]);
    // IFG mono_line_value keeps the space before the comment.
    assert_eq!(raw(m, "X135"), "1 + 3* debug_level ");
    assert_eq!(pos(m, "X135"), (7, 9));
    // X3 variable lookup climbs to the global scope.
    assert_eq!(real(m, "X135"), 4.0);
    assert_eq!(real(&m.sub_sections[0], "X135"), 2.0);
    // `a = c`: `c` is not defined anywhere → Real conversion fails softly.
    assert!(
        !m.get_parameter("a", SearchScope::Current)
            .unwrap()
            .try_get::<f64>()
            .unwrap_err()
            .is_hard()
    );
    // String parameters keep the raw value.
    assert_eq!(raw(m, "yop"), "yop");
    // X3: 2*pi + max(2, general), general = 50
    assert_eq!(real(g, "toto"), 2.0 * PI + 50.0);
    // IFG value: `\` continuation concatenates the two lines without the
    // newline and leading whitespace.
    assert_eq!(
        raw(g, "mat"),
        "[[ 1, 23+2, 5, toto ],[ 0, 10, general, 5+8] ]"
    );
    assert!(p.is_permissive());
}

#[test]
fn f02_static_example_constants_and_power() {
    let p = parse("examples/c++/solid_mechanics_model/static/material.dat");
    let g = &p.root;
    // X3: factor = number ("**" number)*, term left-assoc: (pi * 0.05**2) / 4
    let a = (PI * 0.05f64.powf(2.0)) / 4.0;
    assert_eq!(real(g, "A"), a);
    let m = &g.sub_sections[0];
    assert_eq!(subs(g), vec![(T::Material, "elastic", "")]);
    // Variables of the parent scope inside the material section.
    assert_eq!(real(m, "rho"), (7750.0 * a) * 2.0);
    assert_eq!(real(m, "E"), 200e9 * a);
    assert_eq!(real(m, "nu"), 0.0);
    // IFG positions: a tab advances to the next multiple-of-4 stop (col 5),
    // then one space.
    assert_eq!(pos(m, "name"), (6, 6));
}

#[test]
fn f03_diffusion_example_continued_matrix() {
    let p = parse("examples/c++/diffusion_model/material.dat");
    assert_eq!(subs(&p.root), vec![(T::Model, "heat_transfer_model", "")]);
    let model = &p.root.sub_sections[0];
    assert_eq!(
        subs(model),
        vec![(T::ConstitutiveLaw, "heat_diffusion", "")]
    );
    let law = &model.sub_sections[0];
    // Qi MatrixGrammar over a value continued on three lines.
    assert_eq!(
        matrix(law, "conductivity"),
        vec![
            vec![401.0, 0.0, 0.0],
            vec![0.0, 401.0, 0.0],
            vec![0.0, 0.0, 401.0]
        ]
    );
    assert_eq!(real(law, "density"), 8940.0);
}

#[test]
fn f04_fe2_weibull_and_user_matrix() {
    let p = parse("extra_packages/extra-materials/test/test_material_FE2/material.dat");
    let g = &p.root;
    assert_eq!(
        subs(g),
        vec![
            (T::Material, "damage_iterative", ""),
            (T::Material, "damage_iterative", ""),
            (T::Material, "elastic", ""),
            (T::Material, "material_FE2", ""),
            (T::User, "parameter", ""),
        ]
    );
    // Random: `26.4e6 weibull [6.6e6, 5.]` (user order a, b; upstream builds
    // std::weibull_distribution(b, a)).
    let r = random(&g.sub_sections[0], "Sc");
    assert_eq!(r.base_value, 26.4e6);
    assert_eq!(
        r.distribution,
        Some(RandomDistribution::Weibull(6.6e6, 5.0))
    );
    assert_eq!(
        r.to_string(),
        "2.64e+07 + weibull [ 5.00000000000000000e+00 6.60000000000000000e+06 ]"
    );
    assert_eq!(raw(&g.sub_sections[3], "element_type"), "_triangle_3");
    // Qi matrix entries are expressions with variables (p = 0.00065).
    let user = p.user_section().unwrap();
    assert_eq!(
        matrix(user, "prestrain_increment"),
        vec![vec![0.00065, 0.0], vec![0.0, 0.00065]]
    );
    assert_eq!(
        user.get_parameter("total_steps", SearchScope::Current)
            .unwrap()
            .to_int::<i32>(),
        10
    );
}

#[test]
fn f05_orthotropic_vectors_and_products() {
    let p = parse("test/test_model/test_solid_mechanics_model/test_materials/orthotropic.dat");
    let m2 = &p.root.sub_sections[1];
    assert_eq!(raw(m2, "name"), "test_mat_2");
    // Qi VectorGrammar with signed entries.
    assert_eq!(vector(m2, "n1"), vec![0.0, 1.0, 0.0]);
    assert_eq!(vector(m2, "n2"), vec![-1.0, 0.0, 0.0]);
    // X3 term: E1 = 10 * E2 (same literal, so exactly 10 times the value).
    assert_eq!(real(m2, "E1"), 10.0 * real(m2, "E2"));
    // Upstream map order of parameters (byte-wise: uppercase first).
    let names: Vec<&str> = m2
        .parameters_sorted()
        .iter()
        .map(|q| q.name.to_str().unwrap())
        .collect();
    assert_eq!(
        names,
        vec!["E1", "E2", "G12", "n1", "n2", "name", "nu12", "rho"]
    );
}

#[test]
fn f06_parallel_insertion_string_list_no_newline() {
    let p = parse(
        "test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/input_file.dat",
    );
    let g = &p.root;
    assert_eq!(g.sub_sections.len(), 8);
    assert_eq!(g.sub_sections[7].section_type, T::Mesh);
    assert_eq!(g.sub_sections[7].name, "parameters");
    // operator std::vector<std::string>(): split on commas.
    let mesh = &g.sub_sections[7];
    let list = mesh
        .parameter("cohesive_surfaces")
        .unwrap()
        .to_string_list();
    assert_eq!(
        list,
        vec!["coh1", "coh2", "coh3", "coh4", "coh5", "interface"]
    );
    // `#\t finite_deformation = 1` is a comment.
    assert!(g.sub_sections[0].parameter("finite_deformation").is_none());
    // bool: "true" via boolalpha.
    assert!(
        g.sub_sections[1]
            .parameter("exponential_penalty")
            .unwrap()
            .to_bool()
    );
}

#[test]
fn f07_scalability_nested_solver_sections() {
    let p = parse("examples/c++/scalability_test/material-cohesive.dat");
    let g = &p.root;
    assert_eq!(
        subs(g),
        vec![
            (T::User, "section", ""),
            (T::Model, "solid_mechanics_model_cohesive", "")
        ]
    );
    let user = p.user_section().unwrap();
    assert_eq!(real(user, "compression"), -1e5);
    assert!(!user.parameter("output_energy").unwrap().to_bool());
    assert!(user.parameter("cohesive_insertion").unwrap().to_bool());
    assert_eq!(user.parameter("mesh").unwrap().to_word(), "cube.msh");
    let model = &g.sub_sections[1];
    // IFG create_subsection: the counter (starting at 12) is incremented for
    // every section in pre-order: user 12, model 13, elastic 14,
    // cohesive_linear 15, cohesive_inserter 16.
    assert_eq!(
        subs(model),
        vec![
            (T::Material, "elastic", ""),
            (T::Material, "cohesive_linear", ""),
            (T::CohesiveInserter, "anonymous_16", ""),
            (T::TimeStepSolver, "static", ""),
        ]
    );
    // multimap order: by ParserType declaration order.
    let sorted: Vec<ParserType> = model
        .sub_sections_sorted()
        .iter()
        .map(|s| s.section_type)
        .collect();
    assert_eq!(
        sorted,
        vec![
            T::CohesiveInserter,
            T::Material,
            T::Material,
            T::TimeStepSolver
        ]
    );
    assert_eq!(
        matrix(&model.sub_sections[2], "bounding_box"),
        vec![vec![-1.0, 1.0], vec![-1.0, 1.0], vec![-1.0, 1.0]]
    );
    let tss = &model.sub_sections[3];
    assert_eq!(subs(tss), vec![(T::NonLinearSolver, "linear", "")]);
    assert_eq!(raw(&tss.sub_sections[0], "type"), "linear");
}

#[test]
fn f08_non_local_random_with_variables() {
    let p = parse(
        "test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/material_damage_non_local.dat",
    );
    let g = &p.root;
    assert_eq!(
        subs(g),
        vec![
            (T::Material, "marigo_non_local", ""),
            (T::NonLocal, "concrete", "remove_wf")
        ]
    );
    // Qi RandomGeneratorGrammar: `Yd0 uniform [ 0, 0.1 * Yd0 ]` with Yd0 =
    // 94e3 from the global scope.
    let r = random(&g.sub_sections[0], "Yd");
    assert_eq!(r.base_value, 94e3);
    assert_eq!(
        r.distribution,
        Some(RandomDistribution::Uniform(0.0, 0.1 * 94e3))
    );
    let nl = &g.sub_sections[1];
    assert_eq!(subs(nl), vec![(T::WeightFunction, "weight_parameter", "")]);
    // A value keeps trailing tabs/spaces before the end of line.
    assert_eq!(raw(&nl.sub_sections[0], "update_rate"), "10\t       ");
    assert_eq!(real(&nl.sub_sections[0], "update_rate"), 10.0);
}

#[test]
fn f09_options_prefix_section_type() {
    let p = parse("test/test_model/test_contact_mechanics_model/test_resolution/options.dat");
    // IFG: no_case[section_type] is a longest-prefix symbol match without
    // word boundary: `contact_resolution` + name `_penalty`.
    assert_eq!(subs(&p.root), vec![(T::ContactResolution, "_penalty", "")]);
    assert_eq!(real(&p.root.sub_sections[0], "epsilon"), 10.0);
}

#[test]
fn f10_io_parser_example_user_section() {
    let p = parse("examples/c++/solid_mechanics_model/io/parser/input_file.dat");
    let user = p.user_section().unwrap();
    assert_eq!(user.name, "parameters");
    // IFG key: raw text trimmed, so `mesh_file \t  =` defines `mesh_file`.
    assert_eq!(raw(user, "mesh_file"), "swiss_cheese.msh");
    assert_eq!(
        user.parameter("spatial_dimension").unwrap().to_int::<i32>(),
        2
    );
    assert_eq!(
        matrix(user, "stress"),
        vec![vec![30e3, 0.0], vec![0.0, 30e3]]
    );
    assert_eq!(real(user, "precision"), 1e-9);
    let mat = &p.root.sub_sections[0];
    assert_eq!(real(mat, "E"), 1e5);
}

#[test]
fn f11_ntn_friction_colon_names() {
    let p = parse(
        "extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_n_shaped_slip.dat",
    );
    let g = &p.root;
    // IFG section_name allows ':'.
    assert_eq!(g.sub_sections[1].section_type, T::Friction);
    assert_eq!(
        g.sub_sections[1].name,
        "rate_and_state:revised_n_shaped:slip"
    );
    let user = p.user_section().unwrap();
    assert_eq!(raw(user, "top_traction"), "[ 0.0e+06 , -1e6 ] ");
    assert_eq!(vector(user, "top_traction"), vec![0.0, -1e6]);
    // Qi real parser: the sign is applied with copysign → -0.0.
    let bot = vector(user, "bot_traction");
    assert!(bot[0] == 0.0 && bot[0].is_sign_negative());
    assert_eq!(real(&g.sub_sections[1], "v_star"), 1e-7);
    assert_eq!(
        g.sub_sections[2]
            .parameter("Plane_Stress")
            .unwrap()
            .to_int::<i32>(),
        1
    );
}

#[test]
fn f12_ras_ss_in_quoted_string() {
    let p = parse("examples/c++/node_to_node/ras_ss.in");
    let user = p.user_section().unwrap();
    // Quotes have no meaning in the grammar: they are part of the value.
    assert_eq!(raw(user, "mesh"), "\"ntn_test_ras.msh\"");
    assert_eq!(raw(user, "noise_file"), "noise_field_200.dat");
    assert_eq!(user.parameter("nb_it_nodes").unwrap().to_int::<u32>(), 200);
    assert_eq!(
        p.root.sub_sections[1].name,
        "rate_and_state:revised_dieterich:ageing_regularized"
    );
}

#[test]
fn f13_compression_contact_sections() {
    let p = parse("examples/python/contact_mechanics_model/compression/compression.dat");
    let g = &p.root;
    assert_eq!(
        subs(g),
        vec![
            (T::Material, "elastic", ""),
            (T::Material, "elastic", ""),
            // materials 12, 13; contact_detector 14
            (T::ContactDetector, "anonymous_14", ""),
            (T::ContactResolution, "penalty_linear", ""),
        ]
    );
    let det = &g.sub_sections[2];
    assert_eq!(
        det.get_parameter_value::<String>("type", SearchScope::Current)
            .unwrap(),
        "explicit"
    );
    assert_eq!(
        det.parameter("max_iterations").unwrap().to_int::<i32>(),
        1000
    );
    // trailing tabs and spaces are part of the raw value, not of the number
    assert_eq!(raw(det, "extension_tolerance"), "1e-3\t\t \t\t\t");
    assert_eq!(real(det, "extension_tolerance"), 1e-3);
    assert_eq!(real(&g.sub_sections[3], "epsilon_n"), 1050e7);
}

#[test]
fn f14_phase_field_models() {
    let p = parse("examples/c++/phase_field_model/material_notch.dat");
    let g = &p.root;
    assert_eq!(
        subs(g),
        vec![
            (T::Model, "solid_mechanics_model", ""),
            (T::Model, "phase_field_model", "")
        ]
    );
    assert_eq!(
        subs(&g.sub_sections[0]),
        vec![(T::Material, "phasefield", "")]
    );
    assert_eq!(
        subs(&g.sub_sections[1]),
        vec![(T::Phasefield, "quadratic", "")]
    );
    assert!(
        !g.sub_sections[0].sub_sections[0]
            .parameter("Plane_Stress")
            .unwrap()
            .to_bool()
    );
    assert_eq!(real(&g.sub_sections[1].sub_sections[0], "gc"), 2.7e-3);
}

#[test]
fn f15_anisotropic_non_ascii_comments() {
    let p = parse("test/test_model/patch_tests/data/material_anisotropic_3.dat");
    let m = &p.root.sub_sections[0].sub_sections[0];
    assert_eq!(m.name, "elastic_anisotropic");
    // UTF-8 bytes inside comments are skipped.
    assert_eq!(raw(m, "rho"), "1.6465129043044597 ");
    assert_eq!(real(m, "C14"), -9.47874286e-11);
    assert_eq!(vector(m, "n3"), vec![1.0, 1.0, -2.0]);
    // `#    alpha = 1e-3 …` is a comment.
    assert!(m.parameter("alpha").is_none());
    // name, rho, 21 C_ij (upper triangle of 6x6), n1..n3
    assert_eq!(m.parameters.len(), 26);
}

#[test]
fn f16_cohesive_insertion_continued_list() {
    let p = parse(
        "test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/input_file.dat",
    );
    let model = &p.root.sub_sections[0];
    let ins = &model.sub_sections[0];
    assert_eq!(ins.section_type, T::CohesiveInserter);
    assert_eq!(
        raw(ins, "cohesive_surfaces"),
        "[coh1, coh2, coh3, coh4, coh5, interface]"
    );
    let set = ins.parameter("cohesive_surfaces").unwrap().to_string_set();
    assert_eq!(set.len(), 6);
    assert!(set.contains(&"interface".into()));
}

#[test]
fn f17_heat_transfer_matrix() {
    let p = parse("test/test_model/patch_tests/data/heat_transfer_input.dat");
    let law = &p.root.sub_sections[0].sub_sections[0];
    assert_eq!(law.section_type, T::ConstitutiveLaw);
    assert_eq!(
        matrix(law, "conductivity"),
        vec![
            vec![401.0, 0.0, 0.0],
            vec![0.0, 401.0, 0.0],
            vec![0.0, 0.0, 401.0]
        ]
    );
}

#[test]
fn f18_cohesive_extrinsic_uniform_zero() {
    let p = parse("examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/material.dat");
    let r = random(&p.root.sub_sections[1], "sigma_c");
    assert_eq!(r.base_value, 100.0);
    assert_eq!(r.distribution, Some(RandomDistribution::Uniform(0.0, 0.0)));
    // A plain number is a random parameter without distribution.
    let r = random(&p.root.sub_sections[1], "G_c");
    assert_eq!((r.base_value, r.distribution), (100.0, None));
}

#[test]
fn f19_cohesive_insertion_material_random_with_comment() {
    let p = parse(
        "test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/material.dat",
    );
    // `seed = 1.0` read as an integer (stream extraction stops at '.').
    assert_eq!(p.root.parameter("seed").unwrap().to_int::<i64>(), 1);
    let coh = &p.root.sub_sections[0].sub_sections[1];
    let r = random(coh, "sigma_c");
    assert_eq!(r.base_value, 300.0e6);
    assert_eq!(r.distribution, Some(RandomDistribution::Uniform(-1e6, 1e6)));
    assert_eq!(real(coh, "beta"), 0.0);
}

#[test]
fn f20_parallel_insertion_iii_seed_and_list() {
    let p = parse(
        "test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/input_file_iii.dat",
    );
    assert_eq!(p.root.parameter("seed").unwrap().to_int::<i64>(), 5);
    let mesh = &p.root.sub_sections[3];
    assert_eq!(raw(mesh, "cohesive_surfaces"), "coh_1,coh_2 ");
    assert_eq!(
        mesh.parameter("cohesive_surfaces")
            .unwrap()
            .to_string_list(),
        vec!["coh_1", "coh_2"]
    );
}

#[test]
fn f21_remove_damage_two_non_local_sections() {
    let p = parse("test/test_model/test_common/test_non_local_toolbox/material_remove_damage.dat");
    assert_eq!(
        subs(&p.root),
        vec![
            (T::Material, "test_material", ""),
            (T::Material, "test_material", ""),
            (T::NonLocal, "mat_1", "remove_wf"),
            (T::NonLocal, "mat_2", "remove_wf"),
        ]
    );
    let wf = &p.root.sub_sections[3].sub_sections[0];
    assert_eq!(real(wf, "damage_limit"), 0.9);
    assert_eq!(raw(&p.root.sub_sections[3], "radius"), "0.5\t\t  ");
    // radius of the parent is visible from the weight function.
    let r: f64 = wf
        .get_parameter_value("radius", SearchScope::CurrentAndParent)
        .unwrap();
    assert_eq!(r, 0.5);
}

#[test]
fn f22_weight_computation_base_wf() {
    let p =
        parse("test/test_model/test_common/test_non_local_toolbox/material_weight_computation.dat");
    assert_eq!(p.root.sub_sections[1].option, "base_wf");
    assert_eq!(p.root.sub_sections[1].option_or("x"), "base_wf");
    assert_eq!(p.root.sub_sections[0].option_or("x"), "x");
}

#[test]
fn f23_damage_non_local_extra_package() {
    let p =
        parse("extra_packages/extra-materials/test/test_material_damage/material_non_local.dat");
    let m = &p.root.sub_sections[0];
    assert_eq!(real(m, "rho"), 2.7e-9);
    assert_eq!(real(m, "max_damage"), 0.99);
    assert_eq!(p.root.sub_sections[1].name, "test_material");
}

#[test]
fn f24_ntn_fixture_vectors_with_spaces() {
    let p = parse(
        "extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_n_shaped_slip.dat",
    );
    // A matrix row cannot be a vector parameter written with spaces: the
    // Qi `vector_eval` uses only the first word ("[" here).
    let user = p.user_section().unwrap();
    let mut g = user.clone();
    g.add_parameter(akantu_parser::ParserParameter::new("m", "[top_traction]"))
        .unwrap();
    assert!(
        g.get_parameter_value::<MatrixX>("m", SearchScope::Current)
            .is_err()
    );
}

#[test]
fn f25_phase_field_test_files_parse_and_convert() {
    for f in [
        "test/test_model/test_phase_field_model/material_coupling.dat",
        "test/test_model/test_phase_field_model/material_linear.dat",
        "examples/python/phase_field_model/material_static.dat",
    ] {
        let p = parse(f);
        assert!(!p.root.sub_sections.is_empty(), "{f}");
        for s in &p.root.sub_sections {
            for c in s.sub_sections.iter().chain(std::iter::once(s)) {
                if let Some(e) = c.parameter("E") {
                    let v: f64 = c
                        .get_parameter_value(&e.name, SearchScope::Current)
                        .unwrap();
                    assert!(v > 0.0, "{f}");
                }
            }
        }
    }
}

#[test]
fn f26_scalability_elastic_bools() {
    let p = parse("examples/c++/scalability_test/material-elastic.dat");
    let user = p.user_section().unwrap();
    assert_eq!(user.parameter("max_steps").unwrap().to_int::<i32>(), 5000);
    let model = &p.root.sub_sections[1];
    assert!(model.sub_sections_of(T::TimeStepSolver).count() == 1);
}

#[test]
fn f27_anisotropic_1_short_vector() {
    let p = parse("test/test_model/patch_tests/data/material_anisotropic_1.dat");
    let m = &p.root.sub_sections[0].sub_sections[0];
    assert_eq!(vector(m, "n1"), vec![-1.0]);
    assert_eq!(real(m, "C11"), 105.092023);
}

#[test]
fn f28_input_file_global_search_scopes() {
    let p = parse("test/test_io/test_parser/input_file.dat");
    let rules = &p.root.sub_sections[0].sub_sections[0];
    assert!(!rules.has_parameter("general", SearchScope::Current));
    assert!(rules.has_parameter("general", SearchScope::CurrentAndParent));
    assert!(rules.has_parameter("general", SearchScope::Parent));
    // getParameter(name, default): missing → default
    assert_eq!(
        rules
            .get_parameter_or("missing", 7.5, SearchScope::Current)
            .unwrap(),
        7.5
    );
    // soft conversion error (`yop` → undefined variable) → default
    let mat = &p.root.sub_sections[0];
    assert_eq!(
        mat.get_parameter_or("a", 1.25, SearchScope::Current)
            .unwrap(),
        1.25
    );
}
