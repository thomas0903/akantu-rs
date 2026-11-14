// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Named
// tests backing statements of the constitutive-laws catalogue
// (docs/manual/src/constitutive-laws.md, laws/*.md), of the cohesive
// chapter (models/cohesive.md) and of the coupling section
// (models/couplers.md) where they differ from upstream's manual
// (doc/dev-doc/manual/constitutive-laws.rst, solidmechanicsmodel.rst).
// Each test cites the upstream source it was checked against (the code,
// not the upstream manual, is the oracle): src/io/parser/parsable.cc
// (`parseParam`: `params.find(name)`, case-sensitive),
// src/model/solid_mechanics/materials/plane_stress_toolbox_tmpl.hh
// (`Plane_Stress`), material_viscoelastic/material_standard_linear_solid_deviatoric.cc
// (`Eta`), src/model/common/non_local_toolbox/base_weight_function.cc and
// src/model/solid_mechanics/materials/weight_functions/{damaged,
// remove_damaged}_weight_function.cc (`INSTANTIATE_NL_NEIGHBORHOOD`:
// weight functions `base_wf`, `damage_wf`, `remove_wf`),
// material_viscoelastic/material_viscoelastic_maxwell.cc (2-D stiffness),
// src/model/solid_mechanics/solid_mechanics_model_cohesive/materials/
// material_cohesive.cc (`getEnergy`: "reversible", "dissipated",
// "cohesive contact", else 0).

use std::path::{Path, PathBuf};

use akantu::core::enums::{AnalysisMethod, MeshIoType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::Parser;
use akantu::solid::SolidMechanicsModel;

fn manual(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/manual/src")
        .join(rel)
}

fn square() -> SharedMesh {
    let mut m = Mesh::new(2, "mesh").unwrap();
    akantu::io::read_mesh(&mut m, &manual("meshes/square.msh"), MeshIoType::Auto).unwrap();
    SharedMesh::new(m)
}

fn solid(input: &str) -> akantu::core::Result<SolidMechanicsModel> {
    let parser = Parser::parse_str(input, "material.dat")?;
    let mut model = SolidMechanicsModel::with_parser(square(), None, "smm", parser)?;
    model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
    Ok(model)
}

/// laws/viscoelastic.md and constitutive-laws.md "Reading the keyword
/// tables" (UD-1130): parameter names are case-sensitive, in the port as
/// upstream (`parsable.cc`: `params.find(in_param.getName())`), so the
/// spellings of upstream's manual `Plane_stress` (examples of `elastic`,
/// `sls_deviatoric`, `viscoelastic_maxwell`, `marigo`) and `eta` (the text
/// of `sls_deviatoric`) are unknown parameters: an error, a warning with
/// `permissive_parser = true` (then ignored: the law keeps its default).
#[test]
fn doc010_upstream_manual_spellings_are_refused() {
    let sls = |extra: &str| {
        format!(
            "material sls_deviatoric [\n name = a\n rho = 1\n E = 3\n nu = 0.3\n Ev = 1\n{extra}]\n"
        )
    };
    // the registered spellings
    assert!(solid(&sls(" Plane_Stress = true\n Eta = 2\n")).is_ok());
    // upstream's manual
    for wrong in [" Plane_stress = false\n", " eta = 2\n"] {
        let e = solid(&sls(wrong)).err().map(|e| e.to_string());
        let name = wrong.trim().split(' ').next().unwrap();
        assert!(
            e.as_deref().is_some_and(|e| e.contains(name)),
            "{wrong:?} accepted or wrong error: {e:?}"
        );
    }
    // permissive: a warning, and the misspelled value is not applied
    let m = solid(&format!("permissive_parser = true\n{}", sls(" eta = 2\n"))).unwrap();
    assert_eq!(
        akantu::parser::ParameterRegistry::get_param(m.material(0).unwrap(), "Eta").unwrap(),
        akantu::parser::ParamValue::Real(1.0)
    );
}

/// laws/viscoelastic.md (UD-1131): `viscoelastic_maxwell` builds its 2-D
/// stiffness as plane strain whatever `Plane_Stress` says
/// (`material_viscoelastic_maxwell.cc`, `updateInternalParameters`: `C`
/// from `ν` only), where upstream's manual lists `Plane_stress` as the
/// law's plane-stress switch: the stress of the same strain is the same
/// with and without it, and is the plane-strain stress; `elastic` changes.
#[test]
fn doc010_maxwell_ignores_plane_stress() {
    use akantu::core::{ElementType, GhostType};
    let stress = |law: &str, plane_stress: bool| -> Vec<f64> {
        let extra = if law == "viscoelastic_maxwell" {
            " Einf = 1\n Ev = [1]\n Eta = [1]\n"
        } else {
            " E = 2\n"
        };
        let mut m = solid(&format!(
            "material {law} [\n name = a\n rho = 1\n nu = 0.3\n Plane_Stress = {plane_stress}\n{extra}]\n"
        ))
        .unwrap();
        m.set_time_step(1.0, None).unwrap();
        let x: Vec<f64> = m
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        for (u, x) in m.displacement_mut().unwrap().tuples_mut().zip(x.chunks(2)) {
            u[0] = 1e-3 * x[0];
        }
        m.assemble_internal_forces().unwrap();
        let s = m
            .material(0)
            .unwrap()
            .base()
            .stress
            .get(ElementType::Quadrangle4, GhostType::NotGhost)
            .unwrap();
        s.as_slice().to_vec()
    };
    assert_eq!(
        stress("viscoelastic_maxwell", true),
        stress("viscoelastic_maxwell", false)
    );
    // plane strain: σ_yy = ν/(1 − ν) σ_xx ≠ 0
    let s = stress("viscoelastic_maxwell", true);
    assert!((s[3] - 0.3 / 0.7 * s[0]).abs() <= 1e-12 * s[0]);
    assert_ne!(stress("elastic", true), stress("elastic", false));
}

/// laws/non-local.md (UD-1133): the weight functions of a `non_local`
/// section are the options registered by upstream's code and the port,
/// `base_wf`, `damage_wf` and `remove_wf`; the names of upstream's manual
/// (`base_weight_function`, `damaged_weight_function`,
/// `remove_damaged_weight_function`,
/// `remove_damaged_with_damage_rate_weight_function`,
/// `stress_based_weight_function`) are refused.
#[test]
fn doc010_weight_function_options() {
    let input = |wf: &str| {
        format!(
            "material marigo_non_local [\n name = c\n rho = 1\n E = 1\n nu = 0.2\n Yd = 1\n Sd = 1\n]\n\
             non_local c {wf} [\n radius = 0.3\n]\n"
        )
    };
    for wf in ["base_wf", "damage_wf", "remove_wf"] {
        let m = solid(&input(wf)).unwrap_or_else(|e| panic!("{wf}: {e}"));
        assert!(m.non_local_manager().is_some(), "{wf}");
    }
    for wf in [
        "base_weight_function",
        "damaged_weight_function",
        "remove_damaged_weight_function",
        "remove_damaged_with_damage_rate_weight_function",
        "stress_based_weight_function",
    ] {
        assert!(solid(&input(wf)).is_err(), "{wf} accepted");
    }
}

/// models/cohesive.md "Energies" (UD-1134): the cohesive laws know the
/// energies `dissipated`, `reversible` and `cohesive contact` (upstream
/// `MaterialCohesive::getEnergy`); the id `contact` of upstream's manual
/// is unknown to them and gives 0 (here: an intrinsic interface with a
/// contact penalty, compressed for three explicit steps). The values of
/// `cohesive contact` are replayed against the oracle in
/// `crates/akantu-cohesive/tests/cohesive_fixtures.rs`.
#[test]
fn doc023_cohesive_energy_ids() {
    use akantu::cohesive::SolidMechanicsModelCohesive;
    let mesh_file = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
         test_cohesive/_cohesive_2d_4_triangle_3.msh",
    );
    let mut mesh = Mesh::new(2, "mesh").unwrap();
    akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Gmsh).unwrap();
    let input = "model solid_mechanics_model_cohesive [\n cohesive_inserter [\n  cohesive_surfaces = [insertion]\n ]\n\
                 material elastic [\n  name = body\n  rho = 1e3\n  E = 1e9\n  nu = 0.2\n ]\n\
                 material cohesive_bilinear [\n  name = interface\n  beta = 1\n  G_c = 10\n  sigma_c = 1e6\n  delta_0 = 1e-7\n  penalty = 1e12\n ]\n]\n";
    let parser = Parser::parse_str(input, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModelCohesive::with_parser(SharedMesh::new(mesh), None, "c", parser).unwrap();
    model
        .init_full(
            &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
            false,
        )
        .unwrap();
    let dt = 0.1 * model.stable_time_step().unwrap();
    model.set_time_step(dt, None).unwrap();
    // compress the plate across the interface: the upper half moves down
    let x: Vec<f64> = model
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    for (v, x) in model
        .smm
        .velocity_mut()
        .unwrap()
        .tuples_mut()
        .zip(x.chunks(2))
    {
        v[1] = -x[1];
    }
    for _ in 0..3 {
        model.solve_step(None).unwrap();
    }
    // the interface is compressed: the penalty energy is positive under the
    // id of upstream's code, and the id of upstream's manual gives 0
    assert!(model.energy("cohesive contact").unwrap() > 0.0);
    assert_eq!(model.energy("contact").unwrap(), 0.0);
    for id in ["dissipated", "reversible"] {
        assert!(model.energy(id).unwrap().is_finite(), "{id}");
    }
}

/// models/cohesive.md "Distributed meshes" and the availability row
/// `parallel-cohesive-rest`: on a mesh distributed
/// over several processes `FragmentManager` numbers the fragments over all
/// ranks and sums the owned elements' contributions, so every rank holds
/// the fragments of the whole mesh, equal to the serial run (upstream
/// `fragment_manager.cc`: damage synchronised, data all-reduced). The
/// 2 x 1 plate of the cohesive chapter (`rho = 1e3`, mass 2000) is split
/// over two and four ranks; a rank's owned and ghost elements do not cover
/// the plate, so a per-rank clustering would give fewer elements and less
/// mass than the serial run.
#[test]
fn doc023_fragment_manager_on_a_distributed_mesh_is_global() {
    use akantu::cohesive::{FragmentManager, SolidMechanicsModelCohesive};
    use akantu::comm::{Communicator, run_local};
    use akantu::core::{ElementType, GhostType};
    use akantu::mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
    let mesh_file = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
         test_cohesive/_cohesive_2d_4_triangle_3.msh",
    );
    let dat = "material elastic [\n name = steel\n rho = 1e3\n E = 1e3\n nu = 0.2\n]\n\
               material cohesive_linear [\n name = cohesive\n beta = 1\n G_c = 100\n sigma_c = 100\n]\n";
    let run = |np: usize| {
        run_local(
            np,
            std::time::Duration::from_secs(60),
            |comm: Communicator| {
                let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
                if comm.rank() == 0 {
                    akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Gmsh).unwrap();
                }
                if comm.size() > 1 {
                    let mut partitioner = RcbPartitioner;
                    let _events = distribute_with(
                        &mut mesh,
                        Some(&mut partitioner as &mut dyn MeshPartitioner),
                        0,
                    )
                    .unwrap();
                    assert!(mesh.is_distributed());
                }
                let parser = Parser::parse_str(dat, "material.dat").unwrap();
                let mut model = SolidMechanicsModelCohesive::with_parser(
                    SharedMesh::new(mesh),
                    None,
                    "c",
                    parser,
                )
                .unwrap();
                model
                    .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
                    .unwrap();
                let mut fragments = FragmentManager::new(&model).unwrap();
                fragments.compute_all_data(&mut model, 1.0).unwrap();
                let m = model.smm.physics.base.mesh.read().unwrap();
                let t = ElementType::Triangle3;
                let local =
                    m.nb_element(t, GhostType::NotGhost) + m.nb_element(t, GhostType::Ghost);
                (
                    local,
                    fragments.nb_fragment(),
                    fragments.nb_elements_per_fragment.clone(),
                    fragments.mass[0],
                )
            },
        )
    };
    let serial = run(1);
    let (total, nb1, per1, mass1) = serial[0].clone();
    assert_eq!((nb1, per1.clone()), (1, vec![total]));
    // The plate's area is exactly 2 (every boundary node lies exactly on its
    // edge line), so only the rounding of the 44 positive element terms
    // remains (≲ 47 u ≈ 5e-15 relative): bound 1e-12 (tightened from
    // 1e-9).
    eprintln!(
        "serial fragment mass {mass1:e}, relative error {:e}",
        (mass1 - 2000.0).abs() / 2000.0
    );
    assert!((mass1 - 2000.0).abs() <= 1e-12 * 2000.0, "{mass1}");
    for np in [2, 4] {
        for (local, nb, per_fragment, mass) in run(np) {
            assert!(
                local < total,
                "{np} ranks: the rank's part covers the plate"
            );
            assert_eq!(nb, 1);
            assert_eq!(per_fragment, per1, "{np} ranks"); // all owned elements, not the rank's part
            assert!(
                (mass - mass1).abs() <= 1e-12 * mass1,
                "{np} ranks: {mass} vs {mass1}"
            );
        }
    }
}

/// models/cohesive.md "Insertion": the extrinsic
/// insertion check belongs to the linear family. On an extrinsic model with
/// a `cohesive_exponential` law `check_cohesive_stress` returns
/// `Error::NotImplemented` (upstream `MaterialCohesive::checkInsertion`:
/// `AKANTU_TO_IMPLEMENT`; `crates/akantu-cohesive/src/laws/mod.rs`
/// `check_insertion`), whatever the facets, and on an intrinsic model it is
/// an error.
#[test]
fn doc023_extrinsic_check_refuses_cohesive_exponential() {
    use akantu::cohesive::SolidMechanicsModelCohesive;
    use akantu::core::Error;
    let mesh_file = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/\
         test_cohesive/_cohesive_2d_4_triangle_3.msh",
    );
    let build = |law: &str, extrinsic: bool| {
        let mut mesh = Mesh::new(2, "mesh").unwrap();
        akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Gmsh).unwrap();
        let input = format!(
            "model solid_mechanics_model_cohesive [\n cohesive_inserter [\n  cohesive_surfaces = [insertion]\n ]\n\
             material elastic [\n  name = body\n  rho = 1e3\n  E = 1e9\n  nu = 0.2\n ]\n\
             material {law} [\n  name = interface\n  beta = 1\n  sigma_c = 1e6\n  delta_c = 1e-4\n ]\n]\n"
        );
        let parser = Parser::parse_str(&input, "material.dat").unwrap();
        let mut model =
            SolidMechanicsModelCohesive::with_parser(SharedMesh::new(mesh), None, "c", parser)
                .unwrap();
        model
            .init_full(
                &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
                extrinsic,
            )
            .unwrap();
        model
    };
    // extrinsic, exponential law: refused at every check (the bulk is at
    // rest, so no facet would be inserted anyway)
    let mut model = build("cohesive_exponential", true);
    model.smm.assemble_internal_forces().unwrap();
    assert!(
        matches!(model.check_cohesive_stress(), Err(Error::NotImplemented(_))),
        "extrinsic cohesive_exponential"
    );
    // the linear law on the same model: no error, nothing to insert
    let mut linear = build("cohesive_linear", true);
    linear.smm.assemble_internal_forces().unwrap();
    assert_eq!(linear.check_cohesive_stress().unwrap(), 0);
    // intrinsic model: "This function can only be used for extrinsic ..."
    let mut intrinsic = build("cohesive_exponential", false);
    assert!(intrinsic.check_cohesive_stress().is_err());
}
