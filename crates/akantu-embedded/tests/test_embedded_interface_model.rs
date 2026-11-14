// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_embedded_interface/test_embedded_interface_model.cc,
// test_embedded_interface_model.sh (inputs embedded_mesh.msh,
// material.dat, matrix).

//! Port of upstream `test_embedded_interface_model` (TEST-126) and of its
//! `.sh` driver, which `diff`s the saved stiffness against the golden file
//! `matrix` (GOLD-008, Matrix Market, 2015).
//!
//! A unit square of 8 linear triangles (`concrete`, `E = 30e9`, `ν = 0`,
//! plane stress) with a reinforcement bar along `y = 0.5` from `x = 0` to
//! `x = 1` (`E = 210e9`, `A = 0.1`); x blocked on `x = 0`, y blocked on
//! `y = 0`, forces `−250, −500, −250` in x on the nodes of `x = 1`; static
//! solve. Upstream checks: `|E_pot − 7.37343e-06| ≤ 1e-5` (absolute) and
//! the saved `K` equal to `matrix` (text `diff`). The reference energy of
//! 2015 turns out to be the concrete's alone (checked to its six digits);
//! the total (both laws, as the current upstream `getEnergy`) is 1.0568e-5,
//! within upstream's tolerance.
//!
//! The `matrix` comparison is the `.sh`'s: the saved text (15 significant
//! digits, `digits10`, UD-061; profile order = order of first insertion by
//! the assembly) must equal the golden file **byte for byte**. The current
//! upstream, made to compile by the patched oracle variant
//! (`oracle/patches/embedded`, D-060), writes `matrix` byte for byte
//! (the earlier claim that upstream's assembly order
//! changed after 2015 was wrong), and so does the port. The energy is
//! checked at upstream's tolerance and, independently, against `½ f·u`
//! (`PROPOSED` rule, NUMERICAL_VALIDATION).

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_embedded::{EmbeddedInterfaceModelOptions, REINFORCEMENT_DUMPER, ReinforcementDumper};
use akantu_linalg::matrix_market::{self, MatrixMarketOptions};
use akantu_model::constitutive_law::LawPhase;

/// The lines where two texts differ (`diff`-like report for a failure).
fn line_diffs(golden: &str, ours: &str) -> Vec<String> {
    let (g, o): (Vec<&str>, Vec<&str>) = (golden.lines().collect(), ours.lines().collect());
    let mut out = Vec::new();
    for k in 0..g.len().max(o.len()) {
        let (a, b) = (g.get(k), o.get(k));
        if a != b {
            out.push(format!("line {}: golden {a:?}, port {b:?}", k + 1));
        }
    }
    out
}

#[test]
fn test_embedded_interface_model() {
    const DIM: usize = 2;
    let tol = 1e-7; // Math::setTolerance(1e-7)
    // Mesh here is a 1x1 patch
    let mesh = common::read_mesh(&common::data("embedded_mesh.msh"), DIM, "mesh");
    let reinforcement = common::reinforcement_mesh(
        DIM,
        &[&[0., 0.5], &[1., 0.5]],
        &[[0, 1]],
        &["reinforcement"],
    );
    let dat = std::fs::read_to_string(common::data("material.dat")).unwrap();
    let mut model = common::model(mesh, reinforcement, &dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();

    let nodes = common::nodes(&model);
    {
        let mut forces = model.smm.external_force_mut().unwrap();
        forces.as_mut_slice()[2 * DIM] = -250.;
        forces.as_mut_slice()[5 * DIM] = -500.;
        forces.as_mut_slice()[8 * DIM] = -250.;
    }
    {
        let mut bound = model.smm.blocked_dofs_mut().unwrap();
        for (i, x) in nodes.iter().enumerate() {
            if common::are_float_equal(x[0], 0., tol) {
                bound.as_mut_slice()[i * DIM] = true;
            }
            if common::are_float_equal(x[1], 0., tol) {
                bound.as_mut_slice()[i * DIM + 1] = true;
            }
        }
    }

    model.smm.add_dump_field_vector("displacement").unwrap();
    model.smm.add_dump_field_tensor("stress").unwrap();
    model
        .set_base_name_to_dumper(REINFORCEMENT_DUMPER, "reinforcement")
        .unwrap();
    // `stress_embedded` was removed upstream in 2018: upstream's
    // createElementalField returns no field for it, so the reinforcement
    // dumper registers nothing (upstream dumps the default dumper
    // only anyway)
    model
        .add_dump_field_tensor_to_dumper(REINFORCEMENT_DUMPER, "stress_embedded")
        .unwrap();

    model.solve_step(None).unwrap();

    // `.sh`: diff matrix matrix_test (exact text comparison)
    let k = model.smm.dof_manager().matrix("K").unwrap();
    let ours = k.matrix_market_text_with(&MatrixMarketOptions::upstream_precision());
    let golden = std::fs::read_to_string(common::data("matrix")).unwrap();
    let mm = matrix_market::parse(&golden).unwrap();
    assert_eq!((mm.nrows, mm.ncols, mm.values.len()), (18, 18, 91));
    assert!(
        ours == golden,
        "saved K differs from `matrix`:\n{}",
        line_diffs(&golden, &ours).join("\n")
    );

    // upstream writes into `paraview/` (DIRECTORIES_TO_CREATE); here a
    // scratch directory of the target dir
    let out =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("test_embedded_interface_model");
    model.smm.set_directory(&out).unwrap();
    model.dump(None, None).unwrap();

    let pot_energy = model.energy("potential").unwrap();
    assert!(
        (pot_energy - 7.37343e-06).abs() <= 1e-5,
        "potential energy {pot_energy}"
    );

    // independent: E_pot = ½ f·u at equilibrium with homogeneous Dirichlet
    // conditions (PROPOSED rule `emb_energy_work`, relative 1e-10)
    let u = model.smm.displacement().unwrap().as_slice().to_vec();
    let f = model.smm.external_force().unwrap().as_slice().to_vec();
    let work: Real = 0.5 * u.iter().zip(&f).map(|(a, b)| a * b).sum::<Real>();
    assert!(
        ((pot_energy - work) / work).abs() <= 1e-10,
        "E_pot {pot_energy} vs ½ f·u {work}"
    );
    eprintln!("E_pot = {pot_energy:.17e}, ½ f·u = {work:.17e}");

    // The 2015 reference value is the energy of the concrete alone (the
    // reinforcement law did not contribute to `getEnergy` then; the current
    // upstream sums every law, as the port): equal to all six printed
    // digits (half a unit of the last digit), which cross-checks the
    // displacement field against the 2015 run (same K, same f).
    let concrete = model.smm.material_index("concrete").unwrap();
    let e_concrete = model
        .smm
        .with_material(concrete, LawPhase::Energy, |m, ctx| {
            m.energy_of(ctx, "potential")
        })
        .unwrap();
    assert!(
        (e_concrete - 7.37343e-06).abs() <= 0.5e-11,
        "concrete energy {e_concrete:.10e}"
    );
    let reinforcement = model.smm.material_index("reinforcement").unwrap();
    let e_reinforcement = model
        .smm
        .with_material(reinforcement, LawPhase::Energy, |m, ctx| {
            m.energy_of(ctx, "potential")
        })
        .unwrap();
    assert!(
        ((e_concrete + e_reinforcement - pot_energy) / pot_energy).abs() <= 1e-15,
        "law energies {e_concrete} + {e_reinforcement} vs {pot_energy}"
    );

    // the interface mesh: two segments of length 0.5 along the shared edges
    let im = model.interface_mesh();
    assert_eq!(im.nb_element(ElementType::Segment2, GhostType::NotGhost), 2);
}

/// The three outcomes of `ReinforcementDumper::descriptor`
/// (upstream `createElementalField(name, group, padding, 1, _ek_regular)` on
/// the interface mesh, `solid_mechanics_model_io.cc:48-126`, and the
/// `reinforcement` branch of `addDumpGroupFieldToDumper`,
/// `embedded_interface_model.cc:132-153`): an internal of the reinforcement
/// law gives a field (`Some`); a name upstream creates no field for gives
/// `None` and registers nothing (no mesh-data fallback on this dumper); the
/// fields upstream derives from `grad_u` / `stress` through compute functors
/// (`strain`, `Green strain`, `principal strain`, `principal Green strain`,
/// `Von Mises stress`) are refused (`Unsupported`, not ported for this
/// dumper: EMB-005's note); a nodal field is refused.
#[test]
fn reinforcement_dumper_descriptor_outcomes() {
    use akantu_core::Error;
    use akantu_io::dumper::FieldSource;
    const DIM: usize = 2;
    let mesh = common::read_mesh(&common::data("embedded_mesh.msh"), DIM, "mesh");
    let reinforcement = common::reinforcement_mesh(
        DIM,
        &[&[0., 0.5], &[1., 0.5]],
        &[[0, 1]],
        &["reinforcement"],
    );
    let dat = std::fs::read_to_string(common::data("material.dat")).unwrap();
    let mut model = common::model(mesh, reinforcement, &dat);
    model
        .init_full(&EmbeddedInterfaceModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let laws = &model.smm.physics.materials;
    // an internal of the reinforcement law
    let d = ReinforcementDumper::descriptor("stress", "all", true, laws)
        .unwrap()
        .expect("an internal gives a field");
    assert!(
        matches!(&d.source, FieldSource::Internal { name, law: None } if name == "stress"),
        "{:?}",
        d.source
    );
    for (name, source) in [
        ("material_index", "material_index"),
        ("partitions", "partitions"),
    ] {
        assert!(
            ReinforcementDumper::descriptor(name, "all", false, laws)
                .unwrap()
                .is_some(),
            "{source}"
        );
    }
    // a name upstream creates no field for: None, nothing registered
    for name in ["stress_embedded", "physical_names", "no_such_field"] {
        assert!(
            ReinforcementDumper::descriptor(name, "all", true, &model.smm.physics.materials)
                .unwrap()
                .is_none(),
            "{name}"
        );
        model
            .add_dump_field_tensor_to_dumper(REINFORCEMENT_DUMPER, name)
            .unwrap();
        let removed = model
            .reinforcement_dumper_mut()
            .dumpable
            .remove_dump_field_from_dumper(REINFORCEMENT_DUMPER, name);
        assert!(
            matches!(removed, Err(Error::NotFound { .. })),
            "{name} was registered: {removed:?}"
        );
    }
    // the derived fields: refused (not ported for this dumper)
    let laws = &model.smm.physics.materials;
    for name in [
        "strain",
        "Green strain",
        "principal strain",
        "principal Green strain",
        "Von Mises stress",
    ] {
        let r = ReinforcementDumper::descriptor(name, "all", true, laws);
        assert!(matches!(r, Err(Error::Unsupported(_))), "{name}: {r:?}");
    }
    // a nodal field of the background mesh: refused
    assert!(ReinforcementDumper::descriptor("displacement", "all", false, laws).is_err());
}
