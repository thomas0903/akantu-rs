// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Named
// tests backing statements of the user manual chapters written for the
// features merged on 2026-11-11 (phase field, split-node contact, new
// constitutive laws, geometry/cgal, MUMPS, coding conventions). Each test
// cites the upstream source it was checked against (the code, not the
// upstream manual, is the oracle):
// src/model/phase_field/{phase_field_model.cc, phasefield.cc,
// phasefields/phasefield_linear.cc, phasefields/phasefield_quadratic.cc},
// src/model/common/model_solver.cc (sparse solver selection),
// src/model/common/constitutive_laws/constitutive_law_tmpl.hh (factory),
// extra_packages/traction-at-split-node-contact/src/ntn_contact/
// ntn_initiation_function.cc, friction_laws/ntn_friclaw_rate_and_state.hh,
// doc/coding_convention/akantu-input-mode.el, packages/cgal.cmake.
// Src/model/diffusion_model/diffusion_model.cc (default solvers;
// models/diffusion.md).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use akantu::core::enums::{
    AnalysisMethod, IntegrationSchemeType, MeshIoType, NonLinearSolverType, TimeStepSolverType,
};
use akantu::core::{Element, ElementType, GhostType, Real, Result};
use akantu::mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu::model::ModelOptions;
use akantu::model::constitutive_law::{LawSpec, ModelServices};
use akantu::parser::param::ParameterAccess;
use akantu::parser::{Parser, ParserType};
use akantu::phase_field::PhaseFieldModel;
use akantu::solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
    SolidMechanicsModel,
};

fn manual(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/manual/src")
        .join(rel)
}

fn read_mesh(rel: &str, dim: usize) -> Mesh {
    let mut m = Mesh::new(dim, "mesh").unwrap();
    akantu::io::read_mesh(&mut m, &manual(rel), MeshIoType::Auto).unwrap();
    m
}

/// One bilinear quadrangle on the unit square (bounding box computed).
fn unit_quad() -> SharedMesh {
    let mut mesh = Mesh::new(2, "quad").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost)
            .push_tuple(&[0, 1, 2, 3]);
    }
    mesh.compute_bounding_box().unwrap();
    SharedMesh::new(mesh)
}

fn phase_field_model(input: &str) -> Result<PhaseFieldModel> {
    let parser = Parser::parse_str(input, "phase_field.dat")?;
    let mut m = PhaseFieldModel::with_parser(unit_quad(), None, "phase_field_model", parser)?;
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
    Ok(m)
}

/* ---------------------------------------------------------------------- */
/* DOC-014: phase field                                                    */
/* ---------------------------------------------------------------------- */

/// `phasefield_quadratic.cc:222`, `phasefield_linear.cc:220`: the
/// registered laws are `quadratic` and `linear`; the `exponential` law of
/// `phasefieldmodel.rst` does not exist (models/phase-field.md, "Theory"
/// and the errata table of differences.md).
#[test]
fn doc014_upstream_law_names() {
    let m = PhaseFieldModel::new(unit_quad(), None, "pfm").unwrap();
    let names: Vec<&str> = m.physics.factory.names().collect();
    assert_eq!(names, ["linear", "quadratic"]);
    let err = phase_field_model(
        "phasefield exponential [\n name = plate\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.1\n]\n",
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("exponential"), "{err}");
    for law in ["quadratic", "linear"] {
        phase_field_model(&format!(
            "phasefield {law} [\n name = plate\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.1\n]\n"
        ))
        .unwrap();
    }
}

/// `phase_field_model.cc:133-191`: the default solver of `_static` is
/// `static`, Newton–Raphson with the pseudo-time scheme on `damage`
/// (models/phase-field.md: the upstream manual's "linear type" is an
/// erratum).
#[test]
fn doc014_default_solver() {
    let m = phase_field_model(
        "phasefield quadratic [\n name = plate\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.1\n]\n",
    )
    .unwrap();
    let s = m.solver.solver(None).unwrap();
    assert_eq!(s.id, "static");
    assert_eq!(s.kind, TimeStepSolverType::Static);
    assert_eq!(
        s.non_linear_solver.kind(),
        NonLinearSolverType::NewtonRaphson
    );
    assert_eq!(
        s.integration_scheme("damage").unwrap().kind(),
        IntegrationSchemeType::PseudoTime
    );
}

/// `phasefield.cc:74-95` (`updateInternalParameters`): `history`,
/// `penalization` and `tao` are accepted, anything else is "Unknown
/// irreversibility type" (models/phase-field.md, "Irreversibility").
#[test]
fn doc014_irreversibility_types() {
    for (value, ok) in [
        ("history", true),
        ("penalization", true),
        ("tao", true),
        ("healing", false),
    ] {
        let r = phase_field_model(&format!(
            "phasefield quadratic [\n name = p\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.1\n irreversibility_type = {value}\n]\n"
        ));
        match (r, ok) {
            (Ok(_), true) => {}
            (Err(e), false) => {
                assert!(
                    e.to_string().contains("Unknown irreversibility type"),
                    "{e}"
                );
            }
            (Ok(_), false) => panic!("{value} accepted"),
            (Err(e), true) => panic!("{value}: {e}"),
        }
    }
}

/* ---------------------------------------------------------------------- */
/* DOC-013: split-node contact                                             */
/* ---------------------------------------------------------------------- */

/// `ntn_initiation_function.cc` (`initializeNTNFriction`): the first
/// `friction` section's name is the law and its option the regularisation
/// (`no_regularisation` by default); later sections are ignored. UD-039:
/// an unknown regularisation is refused (upstream: Rubin–Ampuero). The
/// legacy keys alias `rate_and_state:n_shaped:*` (models/ntn-contact.md,
/// "Friction laws").
#[test]
fn doc013_friction_sections() {
    use akantu_ntn_contact::{
        NtnBaseContact, Regularisation, TasnUpstreamCompat, friction_law_from_name,
        initialize_ntn_friction,
    };
    let mesh = read_mesh("meshes/ntn_2d.msh", 2);
    let contact =
        NtnBaseContact::new_ntn(&mesh, 2, "contact", TasnUpstreamCompat::default()).unwrap();
    let friction = |input: &str| {
        let parser = Parser::parse_str(input, "friction.dat").unwrap();
        initialize_ntn_friction(&contact, mesh.nb_nodes(), &parser)
    };
    let f = friction("friction coulomb [\n mu = 0.3\n]\nfriction linear_cohesive [\n]\n").unwrap();
    assert_eq!(f.law(), friction_law_from_name("coulomb").unwrap());
    assert_eq!(f.regularisation(), Regularisation::NoRegularisation);
    let f = friction("friction coulomb rubin_ampuero [\n mu = 0.3\n t_star = 1e-6\n]\n").unwrap();
    assert_eq!(f.regularisation(), Regularisation::RubinAmpuero);
    let err = friction("friction coulomb rubin [\n mu = 0.3\n]\n").unwrap_err();
    assert!(err.to_string().contains("UD-039"), "{err}");
    assert!(friction("friction rate_and_state:dieterich [\n]\n").is_err());
    for state in ["ageing", "slip", "ageing_regularized"] {
        assert_eq!(
            friction_law_from_name(&format!("rate_and_state_n_shaped_{state}")),
            friction_law_from_name(&format!("rate_and_state:n_shaped:{state}"))
        );
    }
}

/// `ntn_friclaw_rate_and_state.hh:283-324`: `n_shaped` uses
/// `√(1 + (V/V*)²)`, `revised_n_shaped` `√(1 + (V*/V)²)`
/// (models/ntn-contact.md, erratum of the upstream manual's formula).
#[test]
fn doc013_n_shaped_coefficients() {
    use akantu_ntn_contact::RsCoefficient;
    use akantu_ntn_contact::friction::RsArgs;
    let a = RsArgs {
        mu_0: 0.6,
        a: 0.0,
        b: 0.0,
        v_star: 1.0,
        phi_star: 1.0,
        slip_velocity: 3.0,
        state: 1.0,
        ..RsArgs::default()
    };
    // with a = b = 0 the coefficient is μ₀/√(1 + r²)
    let n_shaped = RsCoefficient::NShaped.evaluate(&a);
    let revised = RsCoefficient::RevisedNShaped.evaluate(&a);
    assert_eq!(n_shaped, 0.6 / (1.0 + 3.0_f64 * 3.0).sqrt()); // r = V/V*
    assert_eq!(revised, 0.6 / (1.0 + (1.0_f64 / 3.0) * (1.0 / 3.0)).sqrt()); // r = V*/V
}

/* ---------------------------------------------------------------------- */
/* DOC-011: new constitutive laws                                          */
/* ---------------------------------------------------------------------- */

#[derive(Debug, Default)]
struct Dummy;

impl ForeignMaterialMethods for Dummy {
    fn compute_stress(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
    ) -> Result<()> {
        Ok(())
    }
    fn push_wave_speed(&self, _: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(1.0)
    }
}

fn dummy_allocator() -> akantu::model::constitutive_law::Allocator<dyn Material> {
    Arc::new(
        |spec: &LawSpec, _: &dyn ModelServices| -> Result<Box<dyn Material>> {
            let mut law = ForeignMaterial::new(spec, "dummy", Box::new(Dummy))?;
            law.register_param_real(
                "E",
                ParameterAccess::READABLE | ParameterAccess::PARSMOD,
                "E",
            )?;
            Ok(Box::new(law))
        },
    )
}

/// `constitutive_law_tmpl.hh` / `Factory::registerAllocator`: a name can
/// be registered once (a user law cannot replace `elastic`); the port's
/// factory belongs to the model, so a law registered on one model is
/// unknown to another (new-constitutive-laws.md, "Differences from
/// upstream's procedure").
#[test]
fn doc011_factories_are_per_model_and_names_unique() {
    let square = || SharedMesh::new(read_mesh("meshes/square.msh", 2));
    let input = "material dummy [\n name = d\n rho = 1\n E = 1\n]\n";
    let new_model = || {
        SolidMechanicsModel::with_parser(
            square(),
            None,
            "solid_mechanics_model",
            Parser::parse_str(input, "m.dat").unwrap(),
        )
        .unwrap()
    };
    let mut a = new_model();
    assert!(
        a.physics
            .factory
            .register("elastic", dummy_allocator())
            .is_err()
    );
    a.physics
        .factory
        .register("dummy", dummy_allocator())
        .unwrap();
    assert!(
        a.physics
            .factory
            .register("dummy", dummy_allocator())
            .is_err()
    );
    a.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    assert_eq!(a.material(0).unwrap().law_type(), "dummy");
    // another model: the law is not registered there
    let mut b = new_model();
    let err = b
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap_err()
        .to_string();
    assert!(err.contains("dummy"), "{err}");
}

/* ---------------------------------------------------------------------- */
/* MUMPS page                                                              */
/* ---------------------------------------------------------------------- */

/// `model_solver.cc` (sparse solver selection): in a build without the
/// feature `mumps` (the default build of this test suite), an explicit
/// `sparse_solver_type = mumps` is an error when the solver is created
/// (mumps.md, "Selecting it"); `eigen` in the same place works.
#[test]
fn doc_mumps_explicit_selection_without_the_feature() {
    use akantu::linalg::direct::SparseSolverFactory;
    if SparseSolverFactory::default().is_registered("mumps") {
        return; // a build with the feature: the MUMPS tier tests it
    }
    let build = |sparse: &str| -> Result<SolidMechanicsModel> {
        let input = format!(
            "model solid_mechanics_model [\n time_step_solver static [\n  sparse_solver_type = {sparse}\n ]\n\
             material elastic [\n name = steel\n rho = 1\n E = 1\n nu = 0.3\n ]\n]\n"
        );
        let parser = Parser::parse_str(&input, "solver.dat")?;
        let mut m = SolidMechanicsModel::with_parser(
            SharedMesh::new(read_mesh("meshes/square.msh", 2)),
            None,
            "solid_mechanics_model",
            parser,
        )?;
        m.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
        Ok(m)
    };
    build("eigen").unwrap();
    let err = build("mumps").unwrap_err().to_string();
    assert!(err.contains("not available in this build"), "{err}");
    assert!(err.contains("mumps"), "{err}");
}

/* ---------------------------------------------------------------------- */
/* Geometry page: the cgal feature                                         */
/* ---------------------------------------------------------------------- */

/// The facade's `[features]` (crates/akantu/Cargo.toml): only `cgal` and
/// `igfem` reach the GPL crate, `igfem` implies `cgal`, `default` does not
/// (geometry.md, "The `cgal` feature"; upstream `packages/cgal.cmake`:
/// `CGAL` is an external package, off unless enabled). The resolved
/// dependency graph is checked by
/// `akantu-geometry/tests/license_separation.rs`.
#[test]
fn doc_geometry_cgal_feature_policy() {
    use std::collections::{BTreeMap, BTreeSet};
    let toml =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let section = toml
        .split("[features]")
        .nth(1)
        .unwrap()
        .split("\n[")
        .next()
        .unwrap();
    let mut features: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in section.lines() {
        let line = line.split('#').next().unwrap().trim();
        let Some((name, list)) = line.split_once('=') else {
            continue;
        };
        let items = list
            .trim()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        features.insert(name.trim().to_string(), items);
    }
    fn closure(f: &BTreeMap<String, Vec<String>>, start: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut todo = vec![start.to_string()];
        while let Some(x) = todo.pop() {
            if seen.insert(x.clone())
                && let Some(items) = f.get(&x)
            {
                todo.extend(items.iter().cloned());
            }
        }
        seen
    }
    let gpl = "dep:akantu-geometry-cgal";
    let reaching: BTreeSet<&str> = features
        .keys()
        .filter(|k| closure(&features, k).contains(gpl))
        .map(String::as_str)
        .collect();
    assert_eq!(reaching, BTreeSet::from(["cgal", "igfem"]));
    assert!(closure(&features, "igfem").contains("cgal"));
    assert!(!closure(&features, "default").contains(gpl));
    assert!(!closure(&features, "ntn-contact").contains(gpl));
}

/* ---------------------------------------------------------------------- */
/* DOC-024: coding conventions appendix                                    */
/* ---------------------------------------------------------------------- */

/// `doc/coding_convention/akantu-input-mode.el` (`akantu-input-font-lock-
/// buffer`, first entry): the eleven section keywords the mode highlights
/// are section types of the port, and the section types the appendix names
/// as newer (unhighlighted) exist in the port (appendix/conventions.md,
/// "Why the Emacs mode is not ported"). What is checked is the port's side
/// only: the keyword list below is transcribed from the `.el` file, which
/// is not copied into the repository (by design) and not read here, and
/// that the newer types are absent from it is a fact of the transcription,
/// not of a test.
#[test]
fn doc024_emacs_mode_keywords() {
    const EMACS_KEYWORDS: [&str; 11] = [
        "global",
        "material",
        "model",
        "mesh",
        "heat",
        "contact",
        "friction",
        "embedded_interface",
        "rules",
        "non_local",
        "user",
    ];
    let types: Vec<String> = ParserType::ALL
        .iter()
        .map(|t| t.name().to_string())
        .collect();
    for k in EMACS_KEYWORDS {
        assert!(types.iter().any(|t| t == k), "{k} is not a section type");
    }
    for newer in [
        "phasefield",
        "solver",
        "time_step_solver",
        "non_linear_solver",
        "integration_scheme",
    ] {
        assert!(
            types.iter().any(|t| t == newer),
            "{newer} is not a section type"
        );
    }
}

/* ---------------------------------------------------------------------- */
/* Solver types, contact, distributed meshes                               */
/* ---------------------------------------------------------------------- */

/// A static traction model on the manual's square mesh whose static time
/// step solver uses the non-linear solver `nls` and the sparse solver
/// `sparse`.
fn traction_model(nls: &str, sparse: &str) -> Result<SolidMechanicsModel> {
    let input = format!(
        "model solid_mechanics_model [\n time_step_solver static [\n  sparse_solver_type = {sparse}\n  \
         non_linear_solver {nls} [\n  ]\n ]\n\
         material elastic [\n name = steel\n rho = 1\n E = 1\n nu = 0.3\n ]\n]\n"
    );
    let parser = Parser::parse_str(&input, "solver.dat")?;
    let mesh = SharedMesh::new(read_mesh("meshes/square.msh", 2));
    let mut m = SolidMechanicsModel::with_parser(mesh, None, "solid_mechanics_model", parser)?;
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
    Ok(m)
}

/// solver-configuration.md: `newton_raphson_contact` is
/// upstream's contact name of the one Newton–Raphson implementation
/// (`non_linear_solver_newton_raphson.cc:61-64`, `supported_type`
/// includes `_newton_raphson_contact`): selecting it solves the static
/// traction test to its closed form.
#[test]
fn doc_solver_newton_raphson_contact_is_selectable() {
    use akantu::core::enums::SpatialDirection::{X, Y};
    use akantu::model::boundary_condition::{FixedValue, FlagOnly, FromSameDim};
    let mut m = traction_model("newton_raphson_contact", "eigen").unwrap();
    assert_eq!(
        m.solver.solver(None).unwrap().non_linear_solver.kind(),
        NonLinearSolverType::NewtonRaphsonContact
    );
    m.apply_dirichlet_bc(&FlagOnly { axis: X }, "left").unwrap();
    m.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: Y,
        },
        "bottom",
    )
    .unwrap();
    let t = 1e-3;
    m.apply_neumann_bc(&FromSameDim::new(vec![t, 0.0]), "right")
        .unwrap();
    m.solve_step(None).unwrap();
    let ux = m.displacement().unwrap().tuple(8)[0]; // node (1, 1)
    let exact = (1.0 - 0.3 * 0.3) * t / 1.0;
    assert!((ux - exact).abs() <= 1e-12 * exact, "{ux} vs {exact}");
}

/// solver-configuration.md and the availability table (row
/// `petsc`): the PETSc types are accepted by the parser but selecting
/// them is an error: `petsc_snes` and `petsc_tao` as non-linear solvers,
/// `petsc` as sparse solver.
#[test]
fn doc_solver_petsc_types_are_refused() {
    use akantu::core::enums::SpatialDirection::{X, Y};
    use akantu::model::boundary_condition::{FixedValue, FlagOnly, FromSameDim};
    for nls in ["petsc_snes", "petsc_tao"] {
        assert!(
            traction_model(nls, "eigen").is_err(),
            "non_linear_solver {nls} accepted"
        );
    }
    // the sparse solver is created at the first solve
    let r = traction_model("newton_raphson", "petsc").and_then(|mut m| {
        m.apply_dirichlet_bc(&FlagOnly { axis: X }, "left")?;
        m.apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: Y,
            },
            "bottom",
        )?;
        m.apply_neumann_bc(&FromSameDim::new(vec![1e-3, 0.0]), "right")?;
        m.solve_step(None)
    });
    assert!(r.is_err(), "sparse_solver_type = petsc accepted");
}

/// Two segments `master` (top edge of the lower block) and `slave`
/// (bottom edge of the upper block) of the contact chapter's mesh.
fn contact_blocks() -> Mesh {
    use akantu::mesh::utils::fill_element_to_subelements_data;
    let (q, s, g) = (
        ElementType::Quadrangle4,
        ElementType::Segment2,
        GhostType::NotGhost,
    );
    let mut mesh = Mesh::new(2, "blocks").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for x in [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 0.0],
            [-1.0, 0.0],
            [-0.5, 0.0],
            [0.5, 0.0],
            [0.5, 1.0],
            [-0.5, 1.0],
        ] {
            a.push_node(&x).unwrap();
        }
        let mut quads = a.connectivity_mut(q, g);
        quads.push_tuple(&[0, 1, 2, 3]);
        quads.push_tuple(&[4, 5, 6, 7]);
        drop(quads);
        let mut segments = a.connectivity_mut(s, g);
        segments.push_tuple(&[2, 3]);
        segments.push_tuple(&[4, 5]);
    }
    for (name, el) in [("master", 0), ("slave", 1)] {
        mesh.groups_mut().create_element_group(name, 1).unwrap();
        mesh.element_group_add(name, Element::new(s, el, g), true, true)
            .unwrap();
    }
    fill_element_to_subelements_data(&mut mesh).unwrap();
    mesh
}

const CONTACT_INPUT: &str = "contact_detector [\n type = explicit\n master = master\n slave = slave\n \
    projection_tolerance = 1e-10\n max_iterations = 100\n extension_tolerance = 1e-5\n]\n\
    contact_resolution penalty_quadratic [\n name = c\n mu = 0.2\n epsilon_n = 1e3\n epsilon_t = 1e3\n]\n";

/// models/contact.md, materials.md and the availability table (row
/// `contact`): the contact mechanics model is in the default build
/// (facade module `akantu::contact`), its factory registers upstream's
/// two resolution types, and `init_full` instantiates the
/// `contact_resolution` sections of the input file (upstream
/// `resolution.hh` `INSTANTIATE_RESOLUTION`, `contact_mechanics_model.cc`
/// `instantiateResolutions`).
#[test]
fn doc012_contact_model_is_available() {
    use akantu::contact::ContactMechanicsModel;
    let parser = Parser::parse_str(CONTACT_INPUT, "contact.dat").unwrap();
    let mut model =
        ContactMechanicsModel::with_parser(SharedMesh::new(contact_blocks()), None, "cm", parser)
            .unwrap();
    assert_eq!(
        model.factory.names().collect::<Vec<_>>(),
        ["penalty_linear", "penalty_quadratic"]
    );
    // upstream `getDefaultSolverOptions(_static)`: `_newton_raphson_contact`
    let options =
        ContactMechanicsModel::default_solver_options_for(TimeStepSolverType::Static).unwrap();
    assert_eq!(
        options.non_linear_solver_type,
        Some(NonLinearSolverType::NewtonRaphsonContact)
    );
    model.init_full(AnalysisMethod::Static).unwrap();
    assert!(model.are_resolutions_instantiated());
    assert_eq!(model.resolutions().len(), 1);
    assert_eq!(model.resolutions()[0].id(), "cm:0:penalty_quadratic");
}

/// Run `f` on each rank of a two-rank in-process world with the manual's
/// square mesh distributed over the two ranks.
fn on_distributed_square<T: Send>(f: impl Fn(Mesh) -> T + Sync) -> Vec<T> {
    use akantu::comm::{Communicator, run_local};
    use akantu::mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
    run_local(
        2,
        std::time::Duration::from_secs(60),
        |comm: Communicator| {
            let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
            if comm.rank() == 0 {
                akantu::io::read_mesh(&mut mesh, &manual("meshes/square.msh"), MeshIoType::Auto)
                    .unwrap();
            }
            let mut partitioner = RcbPartitioner;
            let _events = distribute_with(
                &mut mesh,
                Some(&mut partitioner as &mut dyn MeshPartitioner),
                0,
            )
            .unwrap();
            assert!(mesh.is_distributed());
            f(mesh)
        },
    )
}

/// models/cohesive.md and the availability table (row `parallel-cohesive`):
/// the cohesive model runs on a mesh distributed over several
/// processes — built, initialised (extrinsic) and stepped once with a
/// stress check on every rank.
#[test]
fn doc023_cohesive_model_runs_on_a_distributed_mesh() {
    use akantu::cohesive::SolidMechanicsModelCohesive;
    let dat = "material elastic [\n name = steel\n rho = 1e3\n E = 1e3\n nu = 0.2\n]\n\
               material cohesive_linear [\n name = cohesive\n beta = 1\n G_c = 100\n sigma_c = 100\n]\n";
    for r in on_distributed_square(|mesh| {
        let parser = Parser::parse_str(dat, "material.dat").unwrap();
        let mut model =
            SolidMechanicsModelCohesive::with_parser(SharedMesh::new(mesh), None, "c", parser)?;
        model.init_full(
            &akantu::model::ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
            true,
        )?;
        let dt = model.stable_time_step()? * 0.1;
        model.set_time_step(dt, None)?;
        let inserted = model.check_cohesive_stress()?;
        model.solve_step(None)?;
        Ok::<usize, akantu::core::Error>(inserted)
    }) {
        assert!(matches!(r, Ok(0)), "{r:?}");
    }
}

/// models/contact.md and the availability table (row `parallel-contact`):
/// `init_full` of the contact mechanics model refuses a mesh distributed
/// over several processes with `Error::Unsupported` (parity row CONT-019).
#[test]
fn doc012_contact_model_refuses_a_distributed_mesh() {
    use akantu::contact::ContactMechanicsModel;
    let detector = "contact_detector [\n type = explicit\n master = master\n slave = slave\n \
        projection_tolerance = 1e-10\n max_iterations = 100\n extension_tolerance = 1e-5\n]\n";
    for r in on_distributed_square(|mesh| {
        let parser = Parser::parse_str(detector, "contact.dat").unwrap();
        let mut model =
            ContactMechanicsModel::with_parser(SharedMesh::new(mesh), None, "cm", parser)?;
        model.init_full(AnalysisMethod::ExplicitLumpedMass)
    }) {
        assert!(
            matches!(r, Err(akantu::core::Error::Unsupported(_))),
            "{r:?}"
        );
    }
}

/// models/ntn-contact.md, parallel.md and the availability table
/// (row `parallel-ntn-contact`): on a mesh distributed over several
/// processes the split-node contact objects are built and
/// `init_parallel` builds the element synchronizer filtered on the contact
/// nodes (parity rows MPI-051/052; a contact step without it is refused,
/// UD-904: `crates/akantu-ntn-contact/tests/ntn_parallel.rs`).
#[test]
fn doc013_split_node_contact_on_a_distributed_mesh() {
    use akantu_ntn_contact::{NtnBaseContact, TasnUpstreamCompat};
    let serial = read_mesh("meshes/ntn_2d.msh", 2);
    let mut contact =
        NtnBaseContact::new_ntn(&serial, 2, "contact", TasnUpstreamCompat::default()).unwrap();
    assert!(contact.init_parallel(&serial).is_ok());
    assert!(
        contact.synchronizer().is_none(),
        "one process: nothing to exchange"
    );
    for r in on_distributed_square(|mesh| {
        let mut c = NtnBaseContact::new_ntn(&mesh, 2, "contact", TasnUpstreamCompat::default())?;
        c.init_parallel(&mesh)?;
        Ok::<bool, akantu::core::Error>(c.synchronizer().is_some())
    }) {
        assert!(matches!(r, Ok(true)), "{r:?}");
    }
}

/// models/phase-field.md: the defaults of `E` and `nu`
/// of both phase-field laws are 0, as `l0` and `gc` (`phasefield.cc:58-69`
/// registers all four with a default of 0).
#[test]
fn doc014_phase_field_defaults_of_e_and_nu() {
    for law in ["quadratic", "linear"] {
        let m = phase_field_model(&format!("phasefield {law} [\n name = p\n]\n")).unwrap();
        let p = m.phase_field(0).unwrap();
        for name in ["E", "nu", "l0", "gc"] {
            assert_eq!(
                p.get_param(name).unwrap().as_real().unwrap(),
                0.0,
                "{law}: {name}"
            );
        }
    }
}

/// models/phase-field.md and the availability table (row
/// `parallel-phase-field`, parity rows MPI-044/045): the
/// phase-field model runs on a mesh distributed over several processes —
/// built, initialised (static, faer), one solve with a prescribed damage on
/// part of the boundary, and the dissipated energy reduced over the ranks
/// (equal on every rank).
#[test]
fn doc014_phase_field_model_runs_on_a_distributed_mesh() {
    let out = on_distributed_square(|mesh| {
        let parser = Parser::parse_str(
            "phasefield quadratic [\n name = p\n E = 210\n nu = 0.3\n gc = 1\n l0 = 0.1\n]\n",
            "pf.dat",
        )
        .unwrap();
        let shared = SharedMesh::new(mesh);
        let mut model = PhaseFieldModel::with_parser(shared.clone(), None, "pf", parser)?;
        let mut options = akantu::model::ModelOptions::new(AnalysisMethod::Static);
        options.solver_options.sparse_solver_type =
            Some(akantu::core::enums::SparseSolverType::Faer);
        model.init_full(&options)?;
        let x = shared.read()?.nodes().as_slice().to_vec();
        let on_left: Vec<usize> = (0..x.len() / 2).filter(|&n| x[2 * n] == 0.0).collect();
        {
            let mut b = model.blocked_dofs_mut()?;
            for &n in &on_left {
                b.as_mut_slice()[n] = true;
            }
        }
        {
            let mut d = model.damage_mut()?;
            for &n in &on_left {
                d.as_mut_slice()[n] = 0.5;
            }
        }
        model.solve_step(None)?;
        model.energy("dissipated")
    });
    let e: Vec<Real> = out.into_iter().map(|r| r.unwrap()).collect();
    assert!(e[0] > 0.0, "{e:?}");
    assert_eq!(e[0].to_bits(), e[1].to_bits(), "{e:?}");
}

/// models/diffusion.md (parity row DOC-015): the diffusion model is
/// available through the facade (`akantu::diffusion`), with upstream's
/// default solvers (`diffusion_model.cc:126-207`: `explicit_lumped` =
/// lumped forward Euler on the rate, `static` = Newton–Raphson with the
/// pseudo-time scheme, `implicit` = backward Euler on the temperature) and
/// the UD-287 correction of the predictor on by default (compat switch off).
#[test]
fn doc015_diffusion_model_is_available() {
    use akantu::diffusion::HeatTransferModel;
    let model = |method: AnalysisMethod| {
        let mut mesh = Mesh::new(1, "bar").unwrap();
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for i in 0..3 {
                a.push_node(&[i as Real]).unwrap();
            }
            let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
            c.push_tuple(&[0, 1]);
            c.push_tuple(&[1, 2]);
        }
        mesh.compute_bounding_box().unwrap();
        let parser = Parser::parse_str(
            "model heat_transfer_model [\n constitutive_law heat_diffusion [\n name = m\n \
             density = 1\n capacity = 1\n conductivity = [[1]]\n ]\n]\n",
            "heat.dat",
        )
        .unwrap();
        let mut m =
            HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
                .unwrap();
        m.init_full(&ModelOptions::new(method)).unwrap();
        m
    };
    for (method, id, tss, nls, scheme) in [
        (
            AnalysisMethod::ExplicitLumpedMass,
            "explicit_lumped",
            TimeStepSolverType::DynamicLumped,
            NonLinearSolverType::Lumped,
            IntegrationSchemeType::ForwardEuler,
        ),
        (
            AnalysisMethod::Static,
            "static",
            TimeStepSolverType::Static,
            NonLinearSolverType::NewtonRaphson,
            IntegrationSchemeType::PseudoTime,
        ),
        (
            AnalysisMethod::ImplicitDynamic,
            "implicit",
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
            IntegrationSchemeType::BackwardEuler,
        ),
    ] {
        let m = model(method);
        let s = m.solver.solver(None).unwrap();
        assert_eq!((s.id.as_str(), s.kind), (id, tss), "{method:?}");
        assert_eq!(s.non_linear_solver.kind(), nls, "{method:?}");
        assert_eq!(
            s.integration_scheme("temperature").unwrap().kind(),
            scheme,
            "{method:?}"
        );
        assert!(
            !m.dof_manager()
                .upstream_compat()
                .generalized_trapezoidal_keeps_rate_in_predictor,
            "UD-287 corrected by default"
        );
    }
}

/// Availability row `non-local` (D-074): the non-local laws of
/// upstream's package `damage_non_local` are registered in the default
/// build, and a model with such a law and a `non_local` section creates its
/// non-local manager at `init_full` (upstream
/// `ConstitutiveLawsHandler::initNonLocal`,
/// `src/model/common/non_local_toolbox/non_local_manager.cc`).
#[test]
fn doc_non_local_laws_are_available() {
    let input = "material mazars_non_local [\n  name = body\n  rho = 1\n  E = 1e9\n  nu = 0.2\n  \
                 K0 = 1e-4\n]\nnon_local body base_wf [\n  radius = 0.6\n]\n";
    let parser = Parser::parse_str(input, "non_local.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(unit_quad(), None, "solid_mechanics_model", parser)
            .unwrap();
    let names: Vec<String> = m.physics.factory.names().map(String::from).collect();
    for law in [
        "marigo_non_local",
        "mazars_non_local",
        "plastic_mazars_non_local",
    ] {
        assert!(names.iter().any(|n| n == law), "{law} is not registered");
    }
    assert!(!m.is_non_local(), "no manager before init_full");
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    assert!(m.is_non_local());
    assert!(m.non_local_manager().is_some());
}
