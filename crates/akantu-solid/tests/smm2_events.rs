// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). SMM-018 (model
// events), the parts other than the handler order (owned):
// where upstream sends `SolidMechanicsModelEvent`s
// (solid_mechanics_model_event_handler.hh, solid_mechanics_model_io.cc,
// solid_mechanics_model.cc):
// - every `dump(...)` overload sends one `BeforeDumpEvent`
//   (solid_mechanics_model_io.cc:175-216);
// - `solveStep` sends none: `SolidMechanicsModel::beforeSolveStep` /
//   `afterSolveStep` only forward to the materials
//   (solid_mechanics_model.cc:320-329); the `BeforeSolveStepEvent` /
//   `AfterSolveStepEvent` sends exist only commented out
//   (solid_mechanics_model_cohesive_inline_impl.hh:40, 264);
// - `sendEvent(event)` (public `EventHandlerManager::sendEvent`) reaches
//   every registered handler once, with the analysis-method payload of the
//   solve-step events.
// The relative order of material and user handlers is not asserted here.

mod model_common;

use std::sync::{Arc, Mutex};

use akantu_core::Result;
use akantu_core::enums::AnalysisMethod;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::{SolidMechanicsModelEvent, SolidMechanicsModelEventHandler};
use model_common::*;

#[derive(Debug)]
struct Recorder {
    tag: &'static str,
    log: Arc<Mutex<Vec<String>>>,
}

impl Recorder {
    fn push(&self, what: String) {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:{what}", self.tag));
    }
}

impl SolidMechanicsModelEventHandler for Recorder {
    fn on_beginning_solve_step(&mut self, method: AnalysisMethod) -> Result<()> {
        self.push(format!("begin({method:?})"));
        Ok(())
    }
    fn on_end_solve_step(&mut self, method: AnalysisMethod) -> Result<()> {
        self.push(format!("end({method:?})"));
        Ok(())
    }
    fn on_dump(&mut self) -> Result<()> {
        self.push("dump".into());
        Ok(())
    }
    fn on_beginning_of_damage_iteration(&mut self) -> Result<()> {
        self.push("damage_iteration".into());
        Ok(())
    }
    fn on_damage(&mut self) -> Result<()> {
        self.push("damage".into());
        Ok(())
    }
}

const DAT: &str = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n]\n";

fn patch_model(method: AnalysisMethod) -> (SolidMechanicsModel, Arc<Mutex<Vec<String>>>) {
    // the upstream patch-test mesh `_triangle_3.msh` (input of the smm
    // fixtures)
    let f = case("smm/patch_explicit_elastic_triangle_3_plane_strain");
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = akantu_parser::Parser::parse_str(DAT, "events.dat").unwrap();
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    m.init_full(&ModelOptions::new(method)).unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    m.register_event_handler(Box::new(Recorder {
        tag: "a",
        log: log.clone(),
    }));
    m.register_event_handler(Box::new(Recorder {
        tag: "b",
        log: log.clone(),
    }));
    (m, log)
}

fn taken(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    let mut v = std::mem::take(&mut *log.lock().unwrap());
    v.sort();
    v
}

/// `solveStep` sends no model event (static, explicit, implicit dynamic).
#[test]
fn solve_step_sends_no_model_event() {
    for method in [
        AnalysisMethod::Static,
        AnalysisMethod::ExplicitLumpedMass,
        AnalysisMethod::ImplicitDynamic,
    ] {
        let (mut m, log) = patch_model(method);
        if method != AnalysisMethod::Static {
            let dt = 0.5 * m.stable_time_step().unwrap();
            m.set_time_step(dt, None).unwrap();
        }
        for _ in 0..3 {
            m.solve_step(None).unwrap();
        }
        assert!(
            taken(&log).is_empty(),
            "{method:?}: events sent by solve_step"
        );
    }
}

/// Every dump overload sends exactly one `BeforeDumpEvent` to every
/// handler.
#[test]
fn every_dump_sends_one_before_dump_event() {
    let (mut m, log) = patch_model(AnalysisMethod::ExplicitLumpedMass);
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("smm2_events");
    let _ = std::fs::remove_dir_all(&dir);
    m.set_directory(&dir).unwrap();
    m.add_dump_field("displacement").unwrap();
    m.dump(None, None).unwrap();
    assert_eq!(taken(&log), ["a:dump", "b:dump"]);
    m.dump(None, Some(4)).unwrap();
    assert_eq!(taken(&log), ["a:dump", "b:dump"]);
    m.dump(Some(0.5), Some(5)).unwrap();
    assert_eq!(taken(&log), ["a:dump", "b:dump"]);
    m.dump_named("solid_mechanics_model", None, None).unwrap();
    assert_eq!(taken(&log), ["a:dump", "b:dump"]);
    m.dump_named("solid_mechanics_model", Some(1.0), Some(6))
        .unwrap();
    assert_eq!(taken(&log), ["a:dump", "b:dump"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `sendEvent` reaches every handler once, with the method payload.
#[test]
fn send_event_reaches_every_handler_once() {
    let (mut m, log) = patch_model(AnalysisMethod::Static);
    for (event, want) in [
        (
            SolidMechanicsModelEvent::BeforeSolveStep(AnalysisMethod::ImplicitDynamic),
            "begin(ImplicitDynamic)",
        ),
        (
            SolidMechanicsModelEvent::AfterSolveStep(AnalysisMethod::Static),
            "end(Static)",
        ),
        (SolidMechanicsModelEvent::BeforeDump, "dump"),
        (
            SolidMechanicsModelEvent::BeginningOfDamageIteration,
            "damage_iteration",
        ),
        (SolidMechanicsModelEvent::AfterDamage, "damage"),
    ] {
        m.send_event(event).unwrap();
        assert_eq!(taken(&log), [format!("a:{want}"), format!("b:{want}")]);
    }
}
