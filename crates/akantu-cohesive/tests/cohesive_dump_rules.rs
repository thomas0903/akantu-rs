// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs registration and event rules of the cohesive dumpers ported from
// Akantu (EPFL/LSMS) src/model/model.cc:255-293 (addDumpGroupFieldToDumper),
// src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc:598-623 and
// src/model/solid_mechanics/solid_mechanics_model_io.cc:176-179
// (BeforeDumpEvent of dump(dumper)).

//! Row COH-013:
//!
//! * `cd04_*` / UD-1080: a field restricted to a group other than `"all"` is
//!   refused at registration on the cohesive and facets dumpers (upstream
//!   registers group fields on the group's own dumper, which has neither:
//!   the registration throws);
//! * `cd05_*` / UD-1081: a name that is neither a field, nor an internal of a
//!   law on the dumper's elements, nor numeric elemental mesh data — a typo,
//!   or a law's `bool` internal — is refused at registration instead of
//!   making every later dump of the dumper fail;
//! * `cd09_*`: `dump_named` of either dumper sends `BeforeDump` to the solid
//!   model's event handlers once.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use akantu_cohesive::{COHESIVE_DUMPER, FACETS_DUMPER, SolidMechanicsModelCohesive};
use akantu_core::Result;
use akantu_core::enums::AnalysisMethod;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::model::SolidMechanicsModelEventHandler;
use common::MATERIAL_0;

const MESH: &str = "_cohesive_2d_4_triangle_3.msh";

/// An extrinsic explicit model of `MATERIAL_0` with the cohesive law `law`.
fn extrinsic(law: &str) -> SolidMechanicsModelCohesive {
    let mesh = common::read_mesh(&common::cohesive_mesh(MESH), 2);
    let dat = MATERIAL_0.replace("material cohesive_linear [", &format!("material {law} ["));
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut m = SolidMechanicsModelCohesive::with_parser(
        mesh,
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
        .unwrap();
    m.register_facets_dumper(None).unwrap();
    m
}

#[test]
fn cd04_group_fields_are_refused_on_the_cohesive_and_facets_dumpers() {
    let _g = common::lock();
    let mut m = extrinsic("cohesive_linear");
    for (dumper, name) in [
        (COHESIVE_DUMPER, "displacement"),
        (COHESIVE_DUMPER, "damage"),
        (FACETS_DUMPER, "displacement"),
        (FACETS_DUMPER, "sigma_c"),
    ] {
        let e = m
            .add_dump_group_field_to_dumper(dumper, name, "insertion", false)
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("UD-1080") && e.contains("'insertion'") && e.contains(dumper),
            "{dumper} {name}: {e}"
        );
        // the whole-mesh registration of the same name is unaffected
        m.add_dump_group_field_to_dumper(dumper, name, "all", false)
            .unwrap();
    }
}

#[test]
fn cd05_unresolvable_names_and_bool_internals_are_refused_at_registration() {
    let _g = common::lock();
    let mut m = extrinsic("cohesive_linear");
    for dumper in [COHESIVE_DUMPER, FACETS_DUMPER] {
        let e = m
            .add_dump_field_to_dumper(dumper, "damgae")
            .unwrap_err()
            .to_string();
        assert!(e.contains("UD-1081") && e.contains("damgae"), "{e}");
    }
    let mut m = extrinsic("cohesive_linear_fatigue");
    let e = m
        .add_dump_field_to_dumper(COHESIVE_DUMPER, "normal_regime")
        .unwrap_err()
        .to_string();
    assert!(e.contains("UD-1081") && e.contains("bool internal"), "{e}");
    // the dumper stays usable: a later valid field and dump succeed
    let out = std::env::temp_dir().join(format!("akantu-cd05-{}", std::process::id()));
    m.set_directory_to_dumper(COHESIVE_DUMPER, &out).unwrap();
    m.add_dump_field_to_dumper(COHESIVE_DUMPER, "damage")
        .unwrap();
    m.dump_named(COHESIVE_DUMPER, None, None).unwrap();
    std::fs::remove_dir_all(&out).ok();
}

#[derive(Debug)]
struct CountDumps(Arc<AtomicUsize>);

impl SolidMechanicsModelEventHandler for CountDumps {
    fn on_dump(&mut self) -> Result<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[test]
fn cd09_before_dump_reaches_the_handlers_for_both_cohesive_dumpers() {
    let _g = common::lock();
    let mut m = extrinsic("cohesive_linear");
    let out = std::env::temp_dir().join(format!("akantu-cd09-{}", std::process::id()));
    for dumper in [COHESIVE_DUMPER, FACETS_DUMPER] {
        m.set_directory_to_dumper(dumper, &out).unwrap();
        m.add_dump_field_vector_to_dumper(dumper, "displacement")
            .unwrap();
    }
    let n = Arc::new(AtomicUsize::new(0));
    m.smm
        .register_event_handler(Box::new(CountDumps(n.clone())));
    m.dump_named(COHESIVE_DUMPER, None, None).unwrap();
    assert_eq!(n.load(Ordering::SeqCst), 1, "cohesive elements dumper");
    m.dump_named(FACETS_DUMPER, None, None).unwrap();
    assert_eq!(n.load(Ordering::SeqCst), 2, "facets dumper");
    std::fs::remove_dir_all(&out).ok();
}
