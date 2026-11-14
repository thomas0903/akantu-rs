// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/model_solver.cc:58-121
// (getParserSection: getOptionToType<ModelType> of every visited section;
// initDOFManager: dof_manager_type allocated from the DOFManagerFactory),
// src/common/aka_factory.hh:63-67 (factory message),
// src/model/common/constitutive_laws/constitutive_laws_handler.hh:102.

//! `g08_*` (MODEL-005): the model-level error branches
//! of the DOF-manager initialisation, with upstream's messages.

use akantu_core::enums::{AnalysisMethod, ModelType};
use akantu_core::{ElementType, Error, GhostType};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;

const MATERIAL: &str =
    "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n]\n";

fn square() -> SharedMesh {
    let mut mesh = Mesh::new(2, "m").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for p in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
            acc.push_node(&p).unwrap();
        }
        let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[0, 2, 3]);
    }
    SharedMesh::new(mesh)
}

fn construct(dat: &str) -> Result<SolidMechanicsModel, Error> {
    let mut parser = Parser::parse_str(dat, "input.dat").unwrap();
    parser.last_parsed_file = "input.dat".into();
    SolidMechanicsModel::with_parser(square(), None, "solid_mechanics_model", parser)
}

/// `dof_manager_type` other than `default`: the factory has no such DOF
/// manager; upstream's message appends the factory's text.
#[test]
fn g08_dof_manager_type_other_than_default_is_refused() {
    let dat =
        format!("{MATERIAL}model solid_mechanics_model [\n  dof_manager_type = mumps_like\n]\n");
    let Err(Error::InvalidArgument(m)) = construct(&dat) else {
        panic!("an unknown dof_manager_type must be refused");
    };
    assert_eq!(
        m,
        "To use the solver mumps_like you will have to code it. This is an unknown solver \
         type.The id \"mumps_like\" is not registered in the akantu::DOFManager factory."
    );
    // with a model section the laws come from it (upstream
    // constitutive_laws_handler.hh:102)
    let dat = format!("model solid_mechanics_model [\n  dof_manager_type = default\n{MATERIAL}]\n");
    let mut model = construct(&dat).unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
}

/// A `model` section whose name is not a model type, before the model's
/// own section: upstream's `getParserSection` throws from
/// `getOptionToType<ModelType>`; so does the model construction here.
#[test]
fn g08_model_section_with_an_invalid_type_name_is_an_error() {
    let dat =
        format!("{MATERIAL}model solid_mechanic_model [\n]\nmodel solid_mechanics_model [\n]\n");
    let Err(Error::InvalidArgument(m)) = construct(&dat) else {
        panic!("a model section with an invalid type name must be an error");
    };
    let mut want =
        String::from("The value solid_mechanic_model is not a valid ModelType valid values are ");
    let names: Vec<String> = ModelType::ALL.iter().map(|t| format!("\"{t}\"")).collect();
    want.push_str(&names.join(", "));
    assert_eq!(m, want);
    // the model's own section first: the invalid one is never visited
    let dat = format!("{MATERIAL}model solid_mechanics_model [\n]\nmodel nonsense [\n]\n");
    construct(&dat).unwrap();
}
