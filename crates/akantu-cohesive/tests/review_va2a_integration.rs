// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test
// `g08_*` on the cohesive init path: upstream
// `SolidMechanicsModelCohesive::initFullImpl`
// (solid_mechanics_model_cohesive.cc:162-203) reads its input section with
// `ModelSolver::getParserSection` (model_solver.cc:58-74) before the
// inserter and `SolidMechanicsModel::initFullImpl`; that lookup converts every
// visited `model` section name with `getOptionToType<ModelType>` and throws
// on a name that is not a model type.

mod common;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::Error;
use akantu_core::enums::AnalysisMethod;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use common::MATERIAL_0;

/// The model is built from a valid file; the input then gains a `model`
/// section with a misspelt type before the cohesive one. `init_full` fails
/// with upstream's enumeration message before the inserter, the solver or
/// any law is initialised (before the fix the inserter section was looked
/// up with the skipping `parser_section`, the default solver was created
/// and only the law instantiation failed).
#[test]
fn g08_invalid_model_type_name_fails_before_any_initialisation() {
    let _g = common::lock();
    let mesh = common::read_mesh(&common::cohesive_mesh("_cohesive_2d_4_triangle_3.msh"), 2);
    let parser = Parser::parse_str(MATERIAL_0, "material.dat").unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh,
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    let typo = format!("model solid_mechanic_model [\n]\n{MATERIAL_0}");
    let mut p = Parser::parse_str(&typo, "typo.dat").unwrap();
    p.last_parsed_file = "typo.dat".into();
    model.smm.physics.base.parser = p;
    let Err(Error::InvalidArgument(m)) = model.init_full(
        &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
        false,
    ) else {
        panic!("a model section with an invalid type name must be an error");
    };
    assert!(
        m.starts_with("The value solid_mechanic_model is not a valid ModelType valid values are "),
        "{m}"
    );
    assert!(
        !model.smm.solver.has_default_solver(),
        "the default solver was created before the section lookup failed"
    );
    assert_eq!(model.smm.nb_materials(), 0, "laws instantiated");
}
