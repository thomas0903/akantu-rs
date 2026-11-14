// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests of the solid mechanics model
// ported from Akantu (EPFL/LSMS): src/model/common/model_solver.cc
// (initDOFManager), src/io/parser/parsable.cc (parseParam).

//! Regressions of the solid mechanics model:
//!
//! * item 1: the model's `non_linear_solver` section is
//!   parsed with the parser's `permissive_parser` flag;
//! * item 3: the UD-038 registry entry states what the code
//!   does — the 3-D thermal eigenstrain is corrected by default, upstream's
//!   value behind `ThermalStrain::Upstream`, the 2-D reductions unchanged;
//! * item 4 (SMM-018): model events reach materials and user handlers in
//!   upstream's order — one list, newest first;
//! * UD-793: a material registered with
//!   `register_new_material` reports its registered name (upstream leaves
//!   the `name` parameter empty; category C).

use std::sync::{Arc, Mutex};

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Array, Element, ElementType, ElementTypeMapArray, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext,
};
use akantu_parser::param::{ParamInfo, ParamValue, ParameterRegistry, Parsable};
use akantu_parser::{Parser, ParserType};
use akantu_solid::material::LawOptions;
use akantu_solid::material::MaterialBase;
use akantu_solid::materials::thermal::{ThermalParams, ThermalStrain, thermal_on_quad};
use akantu_solid::model::{SolidMechanicsModelEvent, SolidMechanicsModelEventHandler};
use akantu_solid::{Material, SolidMechanicsModel};

/// Two triangles on the unit square.
fn square() -> SharedMesh {
    let mut mesh = Mesh::new(2, "square").unwrap();
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

// ------------------------------------------------------------- item 1

fn dat(permissive: bool) -> String {
    format!(
        "permissive_parser = {permissive}
model solid_mechanics_model [
  material elastic [
    name = steel
    rho = 1
    E = 1
    nu = 0.3
  ]
  time_step_solver static [
    sparse_solver_type = eigen
    non_linear_solver newton_raphson [
      bogus = 3
      max_iterations = 4
    ]
  ]
]
"
    )
}

/// `with_parser` + `init_full
/// (Static)` failed with "No parameter named bogus registered in static."
/// in both modes; upstream builds the model when `permissive_parser = true`.
#[test]
fn dc08_model_section_honours_permissive_parser() {
    let strict = Parser::parse_str(&dat(false), "strict.dat").unwrap();
    let err = SolidMechanicsModel::with_parser(square(), None, "solid_mechanics_model", strict)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("No parameter named bogus registered in"),
        "{err}"
    );

    let permissive = Parser::parse_str(&dat(true), "permissive.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(square(), None, "solid_mechanics_model", permissive)
            .unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let nls = model.solver.non_linear_solver(Some("static")).unwrap();
    assert_eq!(
        nls.get_param("max_iterations").unwrap().as_int().unwrap(),
        4
    );
}

// ------------------------------------------------------------- item 3

/// Pins the claims of the corrected UD-038 Decision cell (the defect was in
/// the registry text, "Reproduce by default", not in the code, so this test
/// passes on the old code too): `LawOptions::default()` corrects the 3-D
/// eigenstrain to `αΔT`, `LawOptions::upstream()` reproduces `(1 + ν)αΔT`,
/// and the plane-strain / plane-stress / 1-D values do not depend on the
/// switch.
#[test]
fn item3_ud038_decision_matches_the_code() {
    assert_eq!(
        LawOptions::default().thermal_strain,
        ThermalStrain::Corrected
    );
    assert_eq!(
        LawOptions::upstream().thermal_strain,
        ThermalStrain::Upstream
    );
    let p = ThermalParams {
        e: 2.0,
        nu: 0.25,
        alpha: 0.5,
    };
    let dt = 4.0;
    let corrected = ThermalStrain::Corrected;
    let upstream = ThermalStrain::Upstream;
    assert_eq!(thermal_on_quad::<3>(&p, false, corrected, dt).0, 2.0);
    assert_eq!(thermal_on_quad::<3>(&p, false, upstream, dt).0, 2.5);
    for c in [corrected, upstream] {
        assert_eq!(thermal_on_quad::<2>(&p, false, c, dt).0, 2.5); // plane strain
        assert_eq!(thermal_on_quad::<2>(&p, true, c, dt).0, 2.0); // plane stress
        assert_eq!(thermal_on_quad::<1>(&p, false, c, dt).0, 2.0);
    }
}

// ------------------------------------------------------------- item 4

type Log = Arc<Mutex<Vec<String>>>;

fn push(log: &Log, entry: String) {
    log.lock().unwrap().push(entry);
}

/// A user event handler recording its callbacks.
#[derive(Debug)]
struct Recorder {
    tag: &'static str,
    log: Log,
}

impl SolidMechanicsModelEventHandler for Recorder {
    fn on_beginning_solve_step(&mut self, _: AnalysisMethod) -> Result<()> {
        push(&self.log, format!("{}:beginning_solve_step", self.tag));
        Ok(())
    }
    fn on_end_solve_step(&mut self, _: AnalysisMethod) -> Result<()> {
        push(&self.log, format!("{}:end_solve_step", self.tag));
        Ok(())
    }
    fn on_dump(&mut self) -> Result<()> {
        push(&self.log, format!("{}:dump", self.tag));
        Ok(())
    }
    fn on_beginning_of_damage_iteration(&mut self) -> Result<()> {
        push(&self.log, format!("{}:damage_iteration", self.tag));
        Ok(())
    }
    fn on_damage(&mut self) -> Result<()> {
        push(&self.log, format!("{}:damage", self.tag));
        Ok(())
    }
}

/// The built-in `elastic` law, recording its event callbacks (upstream
/// `Material::onDump`, `onDamageIteration`, `onDamageUpdate`) before
/// running them. Everything else is delegated.
#[derive(Debug)]
struct Logged {
    inner: Box<dyn Material>,
    log: Log,
}

impl Logged {
    fn tag(&self) -> String {
        format!("law {}", self.inner.name())
    }
}

impl ParameterRegistry for Logged {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.inner.get_param(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.inner.set_param(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        self.inner.param_names()
    }
    fn update_internal_parameters(&mut self) -> Result<()> {
        self.inner.update_internal_parameters()
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.inner.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.inner.set_param_from_input(name, value)
    }
}

impl Parsable for Logged {
    fn section_type(&self) -> ParserType {
        self.inner.section_type()
    }
    fn parsable_id(&self) -> &str {
        self.inner.parsable_id()
    }
}

impl ConstitutiveLaw for Logged {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn law_id(&self) -> &str {
        self.inner.law_id()
    }
    fn law_type(&self) -> &str {
        self.inner.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.inner.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.inner.element_filter_mut()
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.inner.for_each_internal(f);
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.inner.for_each_internal_mut(f)
    }
    fn save_previous_state(&mut self) -> Result<()> {
        self.inner.save_previous_state()
    }
    fn restore_previous_state(&mut self) -> Result<()> {
        self.inner.restore_previous_state()
    }
    fn parameter_names(&self) -> Vec<String> {
        self.inner.parameter_names()
    }
    fn energy(&self, energy_id: &str) -> Result<Real> {
        self.inner.energy(energy_id)
    }
}

impl Material for Logged {
    fn base(&self) -> &MaterialBase {
        self.inner.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.inner.base_mut()
    }
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.inner.init_material(ctx)
    }
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        g: GhostType,
    ) -> Result<()> {
        self.inner.compute_stress(ctx, ty, g)
    }
    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        g: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        self.inner.compute_tangent_moduli(ctx, ty, g, tangent)
    }
    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.inner.compute_potential_energy(ctx, ty)
    }
    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.inner.push_wave_speed(element)
    }
    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.inner.shear_wave_speed(element)
    }
    fn on_dump(&mut self) -> Result<()> {
        push(&self.log, format!("{}:dump", self.tag()));
        self.inner.on_dump()
    }
    fn on_damage_iteration(&mut self) -> Result<()> {
        push(&self.log, format!("{}:damage_iteration", self.tag()));
        self.inner.on_damage_iteration()
    }
    fn on_damage_update(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        push(&self.log, format!("{}:damage", self.tag()));
        self.inner.on_damage_update(ctx)
    }
}

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.lock().unwrap())
}

/// Upstream (`material.cc:53`, `aka_event_handler_manager.hh:58-80`): every
/// material registers itself with the model when it is constructed and
/// every user handler is registered at the same priority `_ehp_highest`;
/// insertion at the `lower_bound` of the priority puts each new handler
/// first, so `sendEvent` dispatches materials and user handlers in one
/// list, newest first. Registration here: handler A, law `a`, handler B,
/// law `b`, handler C → dispatch C, b, B, a, A. Before the fix the port
/// dispatched the solve-step and damage events to the user handlers only
/// (oldest first: A, B, C; the materials' `onDamageIteration` /
/// `onDamageUpdate` never ran) and `BeforeDump` to the materials first
/// (a, b, A, B, C).
#[test]
fn item4_events_reach_materials_and_handlers_newest_first() {
    let log: Log = Arc::default();
    let mut model = SolidMechanicsModel::new(square(), None, "solid_mechanics_model").unwrap();
    let law_log = log.clone();
    model
        .physics
        .factory
        .register(
            "logged_elastic",
            Arc::new(move |spec, services| {
                let mut f = akantu_solid::material::MaterialFactory::default();
                akantu_solid::material::register_builtin_materials(&mut f)?;
                let inner = f.create("elastic", spec, services)?;
                Ok(Box::new(Logged {
                    inner,
                    log: law_log.clone(),
                }) as Box<dyn Material>)
            }),
        )
        .unwrap();
    let recorder = |tag| {
        Box::new(Recorder {
            tag,
            log: log.clone(),
        })
    };
    model.register_event_handler(recorder("A"));
    let a = model
        .register_new_material("a", "logged_elastic", "")
        .unwrap();
    model.register_event_handler(recorder("B"));
    let b = model
        .register_new_material("b", "logged_elastic", "")
        .unwrap();
    model.register_event_handler(recorder("C"));
    for i in [a, b] {
        let law = model.physics.materials.law_mut(i).unwrap();
        law.set_param("rho", ParamValue::Real(1.0)).unwrap();
        law.set_param("E", ParamValue::Real(1.0)).unwrap();
    }
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    take(&log);

    let order = |what: &str, laws: bool| -> Vec<String> {
        let mut v = vec![format!("C:{what}")];
        if laws {
            v.push(format!("law b:{what}"));
        }
        v.push(format!("B:{what}"));
        if laws {
            v.push(format!("law a:{what}"));
        }
        v.push(format!("A:{what}"));
        v
    };
    let m = AnalysisMethod::ExplicitLumpedMass;
    model
        .send_event(SolidMechanicsModelEvent::BeforeSolveStep(m))
        .unwrap();
    assert_eq!(take(&log), order("beginning_solve_step", false));
    model
        .send_event(SolidMechanicsModelEvent::AfterSolveStep(m))
        .unwrap();
    assert_eq!(take(&log), order("end_solve_step", false));
    model
        .send_event(SolidMechanicsModelEvent::BeforeDump)
        .unwrap();
    assert_eq!(take(&log), order("dump", true));
    model
        .send_event(SolidMechanicsModelEvent::BeginningOfDamageIteration)
        .unwrap();
    assert_eq!(take(&log), order("damage_iteration", true));
    model
        .send_event(SolidMechanicsModelEvent::AfterDamage)
        .unwrap();
    assert_eq!(take(&log), order("damage", true));

    // every `dump` sends BeforeDumpEvent first (solid_mechanics_model_io.cc)
    let dir = std::env::temp_dir().join(format!("akantu-followups2-item4-{}", std::process::id()));
    model
        .set_directory_to_dumper("solid_mechanics_model", &dir)
        .unwrap();
    let dumped = model.dump(None, None);
    let _ = std::fs::remove_dir_all(&dir);
    dumped.unwrap();
    assert_eq!(take(&log), order("dump", true));
}

// ------------------------------------- D-077

/// A model with the logging law `logged_elastic` registered, laws `a` and
/// `b` (`register_new_material`) and, when `with_c`, a law `c` that
/// `init_full` instantiates from the parsed file; user handlers are
/// registered through `register` at their place in the sequence
/// (`A`, law `a`, `L`, `B`, law `b`, `M`, `C`). Returns the model, the log
/// and the handler ids by tag.
#[allow(clippy::type_complexity)] // test helper
fn d077_model(
    register: &dyn Fn(&mut SolidMechanicsModel, &'static str, Log) -> akantu_solid::EventHandlerId,
) -> (
    SolidMechanicsModel,
    Log,
    std::collections::BTreeMap<&'static str, akantu_solid::EventHandlerId>,
) {
    let log: Log = Arc::default();
    let parser = Parser::parse_str(
        "material logged_elastic [\n  name = c\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n",
        "event_order.dat",
    )
    .unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(square(), None, "solid_mechanics_model", parser).unwrap();
    let law_log = log.clone();
    model
        .physics
        .factory
        .register(
            "logged_elastic",
            Arc::new(move |spec, services| {
                let mut f = akantu_solid::material::MaterialFactory::default();
                akantu_solid::material::register_builtin_materials(&mut f)?;
                let inner = f.create("elastic", spec, services)?;
                Ok(Box::new(Logged {
                    inner,
                    log: law_log.clone(),
                }) as Box<dyn Material>)
            }),
        )
        .unwrap();
    let mut ids = std::collections::BTreeMap::new();
    ids.insert("A", register(&mut model, "A", log.clone()));
    let a = model
        .register_new_material("a", "logged_elastic", "")
        .unwrap();
    ids.insert("L", register(&mut model, "L", log.clone()));
    ids.insert("B", register(&mut model, "B", log.clone()));
    let b = model
        .register_new_material("b", "logged_elastic", "")
        .unwrap();
    ids.insert("M", register(&mut model, "M", log.clone()));
    ids.insert("C", register(&mut model, "C", log.clone()));
    for i in [a, b] {
        let law = model.physics.materials.law_mut(i).unwrap();
        law.set_param("rho", ParamValue::Real(1.0)).unwrap();
        law.set_param("E", ParamValue::Real(1.0)).unwrap();
        law.set_param("nu", ParamValue::Real(0.3)).unwrap();
    }
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    take(&log);
    (model, log, ids)
}

/// The priorities of the D-077 sequence: `L` at `_ehp_lowest`, `M` at
/// `_ehp_model`, the others at the default.
fn d077_priority(tag: &str) -> akantu_core::enums::EventPriority {
    use akantu_core::enums::EventPriority;
    match tag {
        "L" => EventPriority::LOWEST,
        "M" => EventPriority::MODEL,
        _ => EventPriority::HIGHEST,
    }
}

fn d077_register(
    model: &mut SolidMechanicsModel,
    tag: &'static str,
    log: Log,
) -> akantu_solid::EventHandlerId {
    let h = Box::new(Recorder { tag, log });
    if d077_priority(tag) == akantu_core::enums::EventPriority::HIGHEST {
        model.register_event_handler(h)
    } else {
        model
            .register_event_handler_with_priority(h, d077_priority(tag))
            .unwrap()
    }
}

/// D-077 (upstream `registerEventHandler(handler, priority)`,
/// `aka_event_handler_manager.hh:58-73`): a user handler registered with a
/// lower priority (`_ehp_lowest`, `_ehp_model`) runs after every material
/// (all at `_ehp_highest`, `material.cc:53`), whenever it was registered;
/// the default-priority entries keep their newest-first order; among the
/// lower priorities the smaller value first. With the materials `a`, `b`
/// and the parsed `c`: `c, C, b, B, a, A, M, L` for every event.
#[test]
fn d077_low_priority_handler_runs_after_the_materials() {
    let (mut model, log, _) = d077_model(&d077_register);
    for (event, what, laws) in [
        (
            SolidMechanicsModelEvent::BeforeSolveStep(AnalysisMethod::ExplicitLumpedMass),
            "beginning_solve_step",
            false,
        ),
        (SolidMechanicsModelEvent::BeforeDump, "dump", true),
        (
            SolidMechanicsModelEvent::BeginningOfDamageIteration,
            "damage_iteration",
            true,
        ),
        (SolidMechanicsModelEvent::AfterDamage, "damage", true),
    ] {
        model.send_event(event).unwrap();
        let mut want = Vec::new();
        for entry in ["law c", "C", "law b", "B", "law a", "A", "M", "L"] {
            if laws || !entry.starts_with("law") {
                want.push(format!("{entry}:{what}"));
            }
        }
        assert_eq!(take(&log), want, "{what}");
    }
}

/// D-077: `unregister_event_handler` removes exactly that handler (the
/// order of the others is kept, a handler registered afterwards takes its
/// place by priority) and gives it back; an id that is not registered
/// (any more) is an error (upstream throws "This event handler is not
/// registered").
#[test]
fn d077_unregister_event_handler() {
    use akantu_core::enums::EventPriority;
    let (mut model, log, ids) = d077_model(&d077_register);
    let b = model.unregister_event_handler(ids["B"]).unwrap();
    let _m = model.unregister_event_handler(ids["M"]).unwrap();
    model
        .register_event_handler_with_priority(
            Box::new(Recorder {
                tag: "D",
                log: log.clone(),
            }),
            EventPriority::LOWEST,
        )
        .unwrap();
    model
        .send_event(SolidMechanicsModelEvent::BeforeDump)
        .unwrap();
    assert_eq!(
        take(&log),
        ["law c", "C", "law b", "law a", "A", "D", "L"]
            .map(|e| format!("{e}:dump"))
            .to_vec()
    );
    let e = model.unregister_event_handler(ids["B"]).unwrap_err();
    assert!(e.to_string().contains("not registered"), "{e}");
    // the handler given back is the registered object
    let mut b = b;
    b.on_dump().unwrap();
    assert_eq!(take(&log), vec!["B:dump".to_owned()]);
}

/// D-077 evidence: the committed oracle order fixture
/// `fixtures/model/smm2/event_order` (smm2 probe v2: upstream's
/// `EventHandlerManager` with the same registration sequence, priorities
/// `_ehp_lowest` / `_ehp_model`, the five events and one `model.dump()`,
/// then `B` and `M` unregistered and `D` registered at `_ehp_lowest`): the
/// port's dispatch log equals upstream's for every recorded event. UD-793
/// (C): upstream leaves the name of a material registered with
/// `registerNewMaterial` empty, the port sets it (`a`, `b`), so the port's
/// `law a:` / `law b:` entries are compared as upstream's `law :` (the two
/// laws are then told apart by their position only, which is what the
/// order evidence needs).
#[test]
fn d077_event_order_matches_the_oracle() {
    let f = model_common_case("smm2/event_order");
    let ud793 = |entries: Vec<String>| -> Vec<String> {
        entries
            .into_iter()
            .map(|e| {
                e.replacen("law a:", "law :", 1)
                    .replacen("law b:", "law :", 1)
            })
            .collect()
    };
    let want = |key: &str| -> Vec<String> {
        f.manifest()
            .parameters
            .get(key)
            .unwrap_or_else(|| panic!("{key}"))
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    let (mut model, log, ids) = d077_model(&d077_register);
    let names: Vec<String> = model
        .physics
        .materials
        .laws()
        .iter()
        .map(|l| match l.name() {
            "a" | "b" => String::new(), // UD-793
            n => n.to_owned(),
        })
        .collect();
    assert_eq!(names, want("materials"));
    let m = AnalysisMethod::ExplicitLumpedMass;
    for (event, key) in [
        (
            SolidMechanicsModelEvent::BeforeSolveStep(m),
            "events/1/beginning_solve_step",
        ),
        (
            SolidMechanicsModelEvent::AfterSolveStep(m),
            "events/1/end_solve_step",
        ),
        (SolidMechanicsModelEvent::BeforeDump, "events/1/dump"),
        (
            SolidMechanicsModelEvent::BeginningOfDamageIteration,
            "events/1/damage_iteration",
        ),
        (SolidMechanicsModelEvent::AfterDamage, "events/1/damage"),
    ] {
        model.send_event(event).unwrap();
        assert_eq!(ud793(take(&log)), want(key), "{key}");
    }
    let dir = std::env::temp_dir().join(format!("akantu-d077-order-{}", std::process::id()));
    model
        .set_directory_to_dumper("solid_mechanics_model", &dir)
        .unwrap();
    let dumped = model.dump(None, None);
    let _ = std::fs::remove_dir_all(&dir);
    dumped.unwrap();
    assert_eq!(ud793(take(&log)), want("events/1/model_dump"));
    model.unregister_event_handler(ids["B"]).unwrap();
    model.unregister_event_handler(ids["M"]).unwrap();
    model
        .register_event_handler_with_priority(
            Box::new(Recorder {
                tag: "D",
                log: log.clone(),
            }),
            akantu_core::enums::EventPriority::LOWEST,
        )
        .unwrap();
    model
        .send_event(SolidMechanicsModelEvent::BeforeDump)
        .unwrap();
    assert_eq!(ud793(take(&log)), want("events/2/dump"));
    model
        .send_event(SolidMechanicsModelEvent::BeginningOfDamageIteration)
        .unwrap();
    assert_eq!(ud793(take(&log)), want("events/2/damage_iteration"));
    let again = model.unregister_event_handler(ids["B"]).is_err();
    assert_eq!(
        Some(again),
        f.manifest()
            .parameters
            .get("unregister_again_throws")
            .and_then(serde_json::Value::as_bool)
    );
}

fn model_common_case(name: &str) -> akantu_validation::Fixture {
    akantu_validation::Fixture::open_by_id(&format!("model/{name}")).unwrap()
}

// ------------------------------------------------- UD-793 (O-2)

/// UD-793 (category C): upstream's
/// `registerNewMaterial(name, type, option)` (`registerNewConstitutiveLaw`,
/// `constitutive_laws_handler_tmpl.hh:204-228`) records `name` only in the
/// handler's name → index map; the law's `name` parameter stays empty
/// (oracle: `getName()` is `""` for both registered materials of the
/// event probe). The port also sets the law's `name` parameter to the
/// registered name; lookups by name (`getMaterialIndex`, `getMaterial(name)`)
/// are the same in both.
#[test]
fn ud793_registered_material_reports_its_registered_name() {
    let mut model = SolidMechanicsModel::new(square(), None, "solid_mechanics_model").unwrap();
    let a = model.register_new_material("a", "elastic", "").unwrap();
    let b = model.register_new_material("b", "elastic", "").unwrap();
    assert_eq!((a, b), (0, 1));
    assert_eq!(model.material_index("a").unwrap(), a);
    assert_eq!(model.material_index("b").unwrap(), b);
    assert_eq!(model.material(a).unwrap().base().name, "a");
    assert_eq!(model.material(b).unwrap().base().name, "b");
}
