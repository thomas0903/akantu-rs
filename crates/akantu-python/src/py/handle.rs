// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_model.cc (Model, ModelSolver as
// the common base of every model class), python/py_solid_mechanics_model.cc,
// py_solid_mechanics_model_cohesive.cc, py_heat_transfer_model.cc,
// py_structural_mechanics_model.cc, py_phase_field_model.cc,
// py_contact_mechanics_model.cc, py_model_couplers.cc (the model objects the
// bindings hold). The handle/ownership design is akantu-rs (D-024, D-047;
// docs/ARCHITECTURE.md).

//! The binding-side owner of every model object.
//!
//! A Python model object (`SolidMechanicsModel`, `HeatTransferModel`, a
//! coupler, …) is a [`ModelRef`]: an `Arc` of one [`ModelHandle`] plus the
//! [`Part`] it designates (a coupler's `getSolidMechanicsModel()` is the
//! coupler's handle with `Part::Solid`). The handle owns the Rust model
//! ([`AnyModel`]) behind a borrow flag ([`SharedOwner`]), keeps the Python
//! mesh's domain alive (upstream `keep_alive<1, 2>`), caches the export
//! leases of the arrays Python fetched (zero-copy views, D-047) and keeps the
//! stack of **solve accesses** lent to Python solver callbacks
//! ([`ScopedMut`]`<dyn SolverAccess>`): while a solve step calls back into a
//! Python `SolverCallback` / `InterceptSolverCallback` of this model, the
//! model is held by the running solve, and the model's solver-callback
//! methods and its DOF manager reach the running solve through that access
//! (upstream: the callback keeps a reference to the model the solver is
//! driving).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use akantu::cohesive::SolidMechanicsModelCohesive;
use akantu::contact::ContactMechanicsModel;
use akantu::core::{ElementType, Error, GhostType, Result};
use akantu::couplers::{CouplerSolidCohesiveContact, CouplerSolidContact, CouplerSolidPhaseField};
use akantu::diffusion::HeatTransferModel;
use akantu::model::constitutive_law::ModelServices;
use akantu::phase_field::PhaseFieldModel;
use akantu::solid::SolidMechanicsModel;
use akantu::structural::StructuralMechanicsModel;
use pyo3::prelude::*;

use super::exports::{ExportTable, into_numpy};
use super::mesh::MeshRef;
use super::model_ops::ModelOps;
use super::runtime::{IntoPy, LendContext, in_callback, native, runtime};
use akantu::core::array_shared::Interpreter;

use crate::foreign_view::numpy_export::{Exportable, Layout};
use crate::foreign_view::{ForeignView, SharedOwner};
use crate::scoped::{ContextKey, Scoped, ScopedMut, SolverAccess};

/// The Rust model behind a Python model object.
#[derive(Debug)]
pub(crate) enum AnyModel {
    /// `SolidMechanicsModel`.
    Solid(Box<SolidMechanicsModel>),
    /// `SolidMechanicsModelCohesive`.
    Cohesive(Box<SolidMechanicsModelCohesive>),
    /// `HeatTransferModel` (a `DiffusionModel`).
    Heat(Box<HeatTransferModel>),
    /// `StructuralMechanicsModel`.
    Structural(Box<StructuralMechanicsModel>),
    /// `PhaseFieldModel`.
    PhaseField(Box<PhaseFieldModel>),
    /// `ContactMechanicsModel`.
    Contact(Box<ContactMechanicsModel>),
    /// `CouplerSolidContact`.
    SolidContact(Box<CouplerSolidContact>),
    /// `CouplerSolidCohesiveContact`.
    SolidCohesiveContact(Box<CouplerSolidCohesiveContact>),
    /// `CouplerSolidPhaseField`.
    SolidPhaseField(Box<CouplerSolidPhaseField>),
}

/// Which model of a handle a Python object designates (couplers own
/// several).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Part {
    /// The handle's model itself.
    Root,
    /// A coupler's solid (or cohesive solid) model.
    Solid,
    /// A coupler's contact model.
    Contact,
    /// A coupler's phase-field model.
    PhaseField,
}

fn no_part(part: Part, what: &str) -> Error {
    Error::invalid(format!("internal error: {what} has no part {part:?}"))
}

impl AnyModel {
    /// The parts this model owns (the root first).
    pub(crate) fn parts(&self) -> &'static [Part] {
        match self {
            Self::SolidContact(_) | Self::SolidCohesiveContact(_) => {
                &[Part::Root, Part::Solid, Part::Contact]
            }
            Self::SolidPhaseField(_) => &[Part::Root, Part::Solid, Part::PhaseField],
            _ => &[Part::Root],
        }
    }

    /// The generic model operations of `part`.
    pub(crate) fn ops(&mut self, part: Part) -> Result<&mut dyn ModelOps> {
        Ok(match (self, part) {
            (Self::Solid(m), Part::Root) => m.as_mut(),
            (Self::Cohesive(m), Part::Root) => m.as_mut(),
            (Self::Heat(m), Part::Root) => &mut m.0,
            (Self::Structural(m), Part::Root) => m.as_mut(),
            (Self::PhaseField(m), Part::Root) => m.as_mut(),
            (Self::Contact(m), Part::Root) => m.as_mut(),
            (Self::SolidContact(m), Part::Root) => m.as_mut(),
            (Self::SolidContact(m), Part::Solid) => &mut m.solid,
            (Self::SolidContact(m), Part::Contact) => &mut m.contact,
            (Self::SolidCohesiveContact(m), Part::Root) => m.as_mut(),
            (Self::SolidCohesiveContact(m), Part::Solid) => &mut m.solid,
            (Self::SolidCohesiveContact(m), Part::Contact) => &mut m.contact,
            (Self::SolidPhaseField(m), Part::Root) => m.as_mut(),
            (Self::SolidPhaseField(m), Part::Solid) => &mut m.solid,
            (Self::SolidPhaseField(m), Part::PhaseField) => &mut m.phase,
            (_, p) => return Err(no_part(p, "this model")),
        })
    }

    /// The solid mechanics model of `part` (also the solid part of a
    /// cohesive model or of a coupler).
    pub(crate) fn solid(&mut self, part: Part) -> Result<&mut SolidMechanicsModel> {
        Ok(match (self, part) {
            (Self::Solid(m), Part::Root) => m.as_mut(),
            (Self::Cohesive(m), Part::Root) => &mut m.smm,
            (Self::SolidContact(m), Part::Root | Part::Solid) => &mut m.solid,
            (Self::SolidCohesiveContact(m), Part::Root | Part::Solid) => &mut m.solid.smm,
            (Self::SolidPhaseField(m), Part::Root | Part::Solid) => &mut m.solid,
            (_, p) => return Err(no_part(p, "a solid mechanics model")),
        })
    }

    /// The cohesive model of `part`.
    pub(crate) fn cohesive(&mut self, part: Part) -> Result<&mut SolidMechanicsModelCohesive> {
        Ok(match (self, part) {
            (Self::Cohesive(m), Part::Root) => m.as_mut(),
            (Self::SolidCohesiveContact(m), Part::Root | Part::Solid) => &mut m.solid,
            (_, p) => return Err(no_part(p, "a cohesive model")),
        })
    }

    /// The heat transfer model.
    pub(crate) fn heat(&mut self, part: Part) -> Result<&mut HeatTransferModel> {
        match (self, part) {
            (Self::Heat(m), Part::Root) => Ok(m.as_mut()),
            (_, p) => Err(no_part(p, "a heat transfer model")),
        }
    }

    /// The structural mechanics model.
    pub(crate) fn structural(&mut self, part: Part) -> Result<&mut StructuralMechanicsModel> {
        match (self, part) {
            (Self::Structural(m), Part::Root) => Ok(m.as_mut()),
            (_, p) => Err(no_part(p, "a structural mechanics model")),
        }
    }

    /// The phase-field model of `part`.
    pub(crate) fn phase_field(&mut self, part: Part) -> Result<&mut PhaseFieldModel> {
        match (self, part) {
            (Self::PhaseField(m), Part::Root) => Ok(m.as_mut()),
            (Self::SolidPhaseField(m), Part::PhaseField) => Ok(&mut m.phase),
            (_, p) => Err(no_part(p, "a phase-field model")),
        }
    }

    /// The contact mechanics model of `part`.
    pub(crate) fn contact(&mut self, part: Part) -> Result<&mut ContactMechanicsModel> {
        match (self, part) {
            (Self::Contact(m), Part::Root) => Ok(m.as_mut()),
            (Self::SolidContact(m), Part::Contact) => Ok(&mut m.contact),
            (Self::SolidCohesiveContact(m), Part::Contact) => Ok(&mut m.contact),
            (_, p) => Err(no_part(p, "a contact mechanics model")),
        }
    }

    /// The solid–contact coupler.
    pub(crate) fn solid_contact(&mut self) -> Result<&mut CouplerSolidContact> {
        match self {
            Self::SolidContact(m) => Ok(m.as_mut()),
            _ => Err(no_part(Part::Root, "a solid-contact coupler")),
        }
    }

    /// The solid–cohesive–contact coupler.
    pub(crate) fn solid_cohesive_contact(&mut self) -> Result<&mut CouplerSolidCohesiveContact> {
        match self {
            Self::SolidCohesiveContact(m) => Ok(m.as_mut()),
            _ => Err(no_part(Part::Root, "a solid-cohesive-contact coupler")),
        }
    }

    /// The solid–phase-field coupler.
    pub(crate) fn solid_phase_field(&mut self) -> Result<&mut CouplerSolidPhaseField> {
        match self {
            Self::SolidPhaseField(m) => Ok(m.as_mut()),
            _ => Err(no_part(Part::Root, "a solid-phase-field coupler")),
        }
    }
}

/// Which constitutive-law handler a law index refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Handler {
    /// Solid materials (`Material`).
    Material,
    /// Diffusion laws (`DiffusionLaw`).
    Diffusion,
    /// Phase fields (`PhaseField`).
    PhaseField,
}

/// Arrays of a model exported to Python (zero-copy views, D-047).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ModelKey {
    /// A real nodal field by its upstream name
    /// ([`ModelOps::nodal_names`]).
    Nodal(&'static str),
    /// `getBlockedDOFs()`.
    BlockedDofs,
    /// `getConstitutiveLawByElement()(type, ghost)` of a handler.
    LawByElement(Handler, ElementType, GhostType),
    /// `getElementMaterial(type, ghost)` of the structural model
    /// (D-078).
    ElementMaterial(ElementType, GhostType),
    /// `getInsertionFacets(type, ghost)` of the cohesive inserter while the
    /// Python laws check the insertion (staged flags).
    InsertionFacets(ElementType, GhostType),
    /// The residual of the model's DOF manager (`getDOFManager()
    /// .getResidual().getValues()`, PY-045).
    Residual,
    /// `getInternalReal(name)(type, ghost)` of law `law` of a handler.
    Internal {
        handler: Handler,
        law: usize,
        name: String,
        ty: ElementType,
        ghost: GhostType,
    },
}

/// Element types of model arrays: export the array of a key into the
/// owner's table (native section, model held mutably).
pub(crate) trait ModelValue: Exportable {
    fn export_into(
        m: &mut dyn ModelOps,
        key: &ModelKey,
        table: &mut ExportTable<(Part, ModelKey)>,
        part: Part,
    ) -> Result<ForeignView<Self>>;
}

fn wrong_key(key: &ModelKey) -> Error {
    Error::invalid(format!(
        "internal error: {key:?} accessed with another dtype"
    ))
}

impl ModelValue for f64 {
    fn export_into(
        m: &mut dyn ModelOps,
        key: &ModelKey,
        table: &mut ExportTable<(Part, ModelKey)>,
        part: Part,
    ) -> Result<ForeignView<Self>> {
        match key {
            ModelKey::Nodal(name) => table.view((part, key.clone()), m.nodal_shared_mut(name)?),
            ModelKey::Residual => table.view(
                (part, key.clone()),
                m.model_solver()?.dof_manager.residual_shared_mut(),
            ),
            ModelKey::Internal {
                handler,
                law,
                name,
                ty,
                ghost,
            } => {
                let mut out = None;
                m.with_law_internal(*handler, *law, name, *ty, *ghost, &mut |a| {
                    out = Some(table.view((part, key.clone()), a)?);
                    Ok(())
                })?;
                out.ok_or_else(|| {
                    Error::not_found("real internal field", format!("{name} ({ty}, {ghost})"))
                })
            }
            _ => Err(wrong_key(key)),
        }
    }
}

impl ModelValue for bool {
    fn export_into(
        m: &mut dyn ModelOps,
        key: &ModelKey,
        table: &mut ExportTable<(Part, ModelKey)>,
        part: Part,
    ) -> Result<ForeignView<Self>> {
        match key {
            ModelKey::BlockedDofs => table.view((part, key.clone()), m.blocked_shared_mut()?),
            ModelKey::InsertionFacets(t, g) => {
                table.view((part, key.clone()), m.insertion_facets_shared_mut(*t, *g)?)
            }
            _ => Err(wrong_key(key)),
        }
    }
}

impl ModelValue for u32 {
    fn export_into(
        m: &mut dyn ModelOps,
        key: &ModelKey,
        table: &mut ExportTable<(Part, ModelKey)>,
        part: Part,
    ) -> Result<ForeignView<Self>> {
        match key {
            ModelKey::ElementMaterial(t, g) => {
                table.view((part, key.clone()), m.element_material_shared_mut(*t, *g)?)
            }
            _ => Err(wrong_key(key)),
        }
    }
}

impl ModelValue for i32 {
    fn export_into(
        m: &mut dyn ModelOps,
        key: &ModelKey,
        table: &mut ExportTable<(Part, ModelKey)>,
        part: Part,
    ) -> Result<ForeignView<Self>> {
        match key {
            ModelKey::LawByElement(h, t, g) => table.view(
                (part, key.clone()),
                m.law_by_element_shared_mut(*h, *t, *g)?,
            ),
            _ => Err(wrong_key(key)),
        }
    }
}

/// The Python objects of runtime laws, by part and law id.
type LawObjects = BTreeMap<(Part, String), Arc<Py<PyAny>>>;

/// Upstream name of the current position nodal field.
const CURRENT_POSITION: &str = "current_position";

/// What [`ModelHandle::export_current_position`] produced.
#[derive(Debug)]
pub(crate) enum CurrentPosition {
    /// The view of the updated current position.
    Ready(ForeignView<f64>),
    /// The model is held by a running call: the cached views of the current
    /// position and of the displacement and a copy of the mesh nodes; the
    /// current position must be set to `X + u` in the foreign phase.
    Held {
        cp: ForeignView<f64>,
        u: ForeignView<f64>,
        nodes: Vec<f64>,
    },
}

/// `cp ← X + u` through the views, value by value as
/// `SolidMechanicsModel::update_current_position` computes it (a copy of
/// the nodes, then `+= u`: the same IEEE sums). Foreign phase (`interp`).
fn set_current_position(
    interp: &Interpreter,
    cp: &ForeignView<f64>,
    u: &ForeignView<f64>,
    nodes: &[f64],
) -> Result<()> {
    let (n, d) = cp.shape();
    if u.shape() != (n, d) || nodes.len() != n * d {
        return Err(Error::invalid(format!(
            "getCurrentPosition: current position {:?}, displacement {:?} and {} node \
             coordinates do not match inside this callback",
            cp.shape(),
            u.shape(),
            nodes.len()
        )));
    }
    let mut disp = vec![0.0; n * d];
    u.copy_to(interp, &mut disp)?;
    for i in 0..n {
        for j in 0..d {
            let k = i * d + j;
            let mut x = nodes[k];
            x += disp[k];
            cp.write(interp, i, j, x)?;
        }
    }
    Ok(())
}

/// A handle lent to a running Python callback: the part whose call lent
/// it, the handle, and the execution context of the lend.
#[derive(Debug)]
pub(crate) struct Lend<H> {
    part: Part,
    handle: H,
    cx: Arc<LendContext>,
}

/// The innermost valid lend of `part` in `stack`, admitted for the calling
/// Python code: `Ok(None)` when no callback of a call of `part` is running,
/// an error when one is running and the caller is not inside it (another
/// thread, greenlet, or a generator / coroutine resumed outside it).
fn admit_top<H: Clone>(
    py: Python<'_>,
    stack: &Mutex<Vec<Lend<H>>>,
    part: Part,
    valid: impl Fn(&H) -> bool,
) -> Result<Option<(H, ContextKey)>> {
    // clone out and release the lock before looking at Python frames
    // (creating frame objects may run the garbage collector and so
    // finalizers that call back into the bindings)
    let top = stack
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .rev()
        .find(|l| l.part == part && valid(&l.handle))
        .map(|l| (l.handle.clone(), Arc::clone(&l.cx)));
    match top {
        None => Ok(None),
        Some((h, cx)) => Ok(Some((h, cx.admit(py)?))),
    }
}

/// Binding-side owner of a model.
#[derive(Debug)]
pub(crate) struct ModelHandle {
    pub(crate) mesh: MeshRef,
    model: SharedOwner<AnyModel>,
    /// Export leases of the arrays Python fetched (bookkeeping, no values).
    exports: Mutex<ExportTable<(Part, ModelKey)>>,
    /// Solve accesses lent to running Python solver callbacks (innermost
    /// last), with the part whose solve lent them.
    access: Mutex<Vec<Lend<ScopedMut<dyn SolverAccess>>>>,
    /// Model services lent to running Python law callbacks (innermost
    /// last): `getModel().getFEEngine()` inside a law callback.
    services: Mutex<Vec<Lend<Scoped<dyn ModelServices>>>>,
    /// The Python objects of the runtime laws (by part and law id):
    /// `getMaterial(i)` returns them, as pybind11 returns the registered
    /// instance.
    laws: Mutex<LawObjects>,
    /// The selector objects set from Python (`getMaterialSelector`).
    selectors: Mutex<BTreeMap<(Part, Handler), Py<PyAny>>>,
    /// The `Parsable` base of each model object (upstream `ModelSolver`
    /// derives from `Parsable(_model, id)`), created
    /// on first use.
    parsables: Mutex<BTreeMap<Part, Py<PyAny>>>,
    /// The class of the root model.
    root_kind: ModelKind,
}

impl ModelHandle {
    /// A new handle owning `model`.
    pub(crate) fn new(mesh: MeshRef, model: AnyModel) -> Arc<Self> {
        let root_kind = match &model {
            AnyModel::Solid(_) => ModelKind::Solid,
            AnyModel::Cohesive(_) => ModelKind::Cohesive,
            AnyModel::Heat(_) => ModelKind::Heat,
            AnyModel::Structural(_) => ModelKind::Structural,
            AnyModel::PhaseField(_) => ModelKind::PhaseField,
            AnyModel::Contact(_) => ModelKind::Contact,
            AnyModel::SolidContact(_) => ModelKind::SolidContact,
            AnyModel::SolidCohesiveContact(_) => ModelKind::SolidCohesiveContact,
            AnyModel::SolidPhaseField(_) => ModelKind::SolidPhaseField,
        };
        Arc::new(Self {
            root_kind,
            mesh,
            model: SharedOwner::new(model),
            exports: Mutex::new(ExportTable::default()),
            access: Mutex::new(Vec::new()),
            services: Mutex::new(Vec::new()),
            laws: Mutex::new(BTreeMap::new()),
            selectors: Mutex::new(BTreeMap::new()),
            parsables: Mutex::new(BTreeMap::new()),
        })
    }

    /// The `Parsable` object of `part` (created on first use with the
    /// model id; upstream `ModelSolver`'s `Parsable` base).
    pub(crate) fn parsable(
        &self,
        py: Python<'_>,
        part: Part,
        id: impl FnOnce() -> PyResult<String>,
    ) -> PyResult<Py<PyAny>> {
        let mut map = self
            .parsables
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(p) = map.get(&part) {
            return Ok(p.clone_ref(py));
        }
        let obj = super::parser::model_parsable(py, id()?)?;
        map.insert(part, obj.clone_ref(py));
        Ok(obj)
    }

    fn table(&self) -> MutexGuard<'_, ExportTable<(Part, ModelKey)>> {
        // only short, non-panicking critical sections
        self.exports.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn accesses(&self) -> MutexGuard<'_, Vec<Lend<ScopedMut<dyn SolverAccess>>>> {
        self.access.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The error of a model call refused because the model is held.
    fn busy(e: Error) -> Error {
        if in_callback() {
            Error::invalid(
                "the model is in use by the running call that called this Python callback: \
                 inside it only array getters and the solver-callback interface of a solve \
                 that called it are available (views of the live arrays)",
            )
        } else {
            e
        }
    }

    /// Run `f` on the model (native section). No values are copied or
    /// synchronised — Python views alias the model's storage; afterwards the
    /// export leases are refreshed (bookkeeping only).
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut AnyModel) -> Result<R>) -> Result<R> {
        let mut model = self.model.try_borrow_mut().map_err(Self::busy)?;
        let r = f(&mut model);
        let s = self.refresh(&mut model);
        drop(model);
        let s2 = self.mesh.h.refresh_exports();
        let r = r?;
        s?;
        s2?;
        Ok(r)
    }

    /// Re-lease the nodal fields of every part and every cached array whose
    /// lease no longer covers the live values; drop entries of arrays that
    /// no longer exist.
    fn refresh(&self, m: &mut AnyModel) -> Result<()> {
        let mut t = self.table();
        for &part in m.parts() {
            let Ok(ops) = m.ops(part) else { continue };
            for &name in ops.nodal_names() {
                let key = (part, ModelKey::Nodal(name));
                match ops.nodal_shared_mut(name) {
                    Ok(a) => {
                        t.view(key, a)?;
                    }
                    Err(_) => t.remove(&key),
                }
            }
            let key = (part, ModelKey::BlockedDofs);
            match ops.blocked_shared_mut() {
                Ok(a) => {
                    t.view(key, a)?;
                }
                Err(_) => t.remove(&key),
            }
        }
        for key in t.keys() {
            let (part, k) = &key;
            let Ok(ops) = m.ops(*part) else {
                t.remove(&key);
                continue;
            };
            let r = match k {
                ModelKey::Nodal(_) | ModelKey::BlockedDofs => continue,
                ModelKey::Internal { .. } | ModelKey::Residual => {
                    f64::export_into(ops, k, &mut t, *part).map(drop)
                }
                ModelKey::LawByElement(..) => i32::export_into(ops, k, &mut t, *part).map(drop),
                ModelKey::ElementMaterial(..) => u32::export_into(ops, k, &mut t, *part).map(drop),
                ModelKey::InsertionFacets(..) => bool::export_into(ops, k, &mut t, *part).map(drop),
            };
            if r.is_err() {
                t.remove(&key);
            }
        }
        Ok(())
    }

    /// A view of the array `key` of `part` (native section). While the
    /// model is held by a running call that called back into Python, the
    /// cached lease is returned (it aliases the live array; refused if the
    /// call retired it). `current_position` goes through
    /// [`Self::export_current_position`] (it must be brought up to date).
    pub(crate) fn export<T: ModelValue>(
        &self,
        part: Part,
        key: ModelKey,
    ) -> Result<ForeignView<T>> {
        if key == ModelKey::Nodal(CURRENT_POSITION) {
            return Err(Error::invalid(
                "internal error: current_position is exported by export_current_position",
            ));
        }
        match self.model.try_borrow_mut() {
            Ok(mut m) => {
                let ops = m.ops(part)?;
                let mut t = self.table();
                T::export_into(ops, &key, &mut t, part)
            }
            Err(e) => {
                if !in_callback() {
                    return Err(e);
                }
                self.table().cached::<T>(&(part, key))
            }
        }
    }

    /// `getCurrentPosition()` of `part` (native section): upstream updates
    /// the current position before returning it
    /// (`solid_mechanics_model.cc:411-429`). Outside running calls the
    /// model updates it natively; while the model is held by a running call
    /// that called back into Python (solver callbacks, Python `Material`
    /// methods, Dirichlet functors) the update needs the foreign phase:
    /// the cached views of the current position and the displacement are
    /// returned with a copy of the mesh nodes, and [`ModelRef`] writes
    /// `X + u` into the view before handing it out
    /// (never a stale array).
    pub(crate) fn export_current_position(&self, part: Part) -> Result<CurrentPosition> {
        let key = ModelKey::Nodal(CURRENT_POSITION);
        match self.model.try_borrow_mut() {
            Ok(mut m) => {
                let ops = m.ops(part)?;
                ops.update_current_position()?;
                let mut t = self.table();
                f64::export_into(ops, &key, &mut t, part).map(CurrentPosition::Ready)
            }
            Err(e) => {
                if !in_callback() {
                    return Err(e);
                }
                let (cp, u) = {
                    let t = self.table();
                    (
                        t.cached::<f64>(&(part, key))?,
                        t.cached::<f64>(&(part, ModelKey::Nodal("displacement")))?,
                    )
                };
                // the running call may hold the mesh for reading: a shared
                // read of the node coordinates (native guard, this section)
                let nodes = self
                    .mesh
                    .h
                    .read(self.mesh.part, |mesh| Ok(mesh.nodes().as_slice().to_vec()))?;
                Ok(CurrentPosition::Held { cp, u, nodes })
            }
        }
    }

    /// A view of the residual of `dofs`, the DOF manager of `part` reached
    /// through the solve access lent to a running Python callback (native
    /// section; `&mut` proves no native guard of the residual is alive: the
    /// suspended solve lent its DOF manager exclusively).
    pub(crate) fn export_residual_of(
        &self,
        part: Part,
        dofs: &mut akantu::solver::DofManager,
    ) -> Result<ForeignView<f64>> {
        self.table()
            .view((part, ModelKey::Residual), dofs.residual_shared_mut())
    }

    /// The innermost solve access lent to a running Python callback of a
    /// solve of `part` and the key of its execution context (`None` outside
    /// such callbacks; an error for a caller outside the callback).
    pub(crate) fn active_access(
        &self,
        py: Python<'_>,
        part: Part,
    ) -> Result<Option<(ScopedMut<dyn SolverAccess>, ContextKey)>> {
        admit_top(py, &self.access, part, ScopedMut::is_valid)
    }

    /// Register `access`, lent in the execution context `cx`, for the
    /// duration of the returned guard.
    pub(crate) fn push_access(
        self: &Arc<Self>,
        part: Part,
        access: ScopedMut<dyn SolverAccess>,
        cx: &Arc<LendContext>,
    ) -> AccessGuard {
        self.accesses().push(Lend {
            part,
            handle: access,
            cx: Arc::clone(cx),
        });
        AccessGuard(Arc::clone(self))
    }
}

impl ModelHandle {
    fn lock_services(&self) -> MutexGuard<'_, Vec<Lend<Scoped<dyn ModelServices>>>> {
        self.services.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The innermost model services lent to a running law callback of
    /// `part` and the key of its execution context (`None` outside such
    /// callbacks; an error for a caller outside the callback).
    pub(crate) fn active_services(
        &self,
        py: Python<'_>,
        part: Part,
    ) -> Result<Option<(Scoped<dyn ModelServices>, ContextKey)>> {
        admit_top(py, &self.services, part, Scoped::is_valid)
    }

    /// Register `services`, lent in the execution context `cx`, for the
    /// duration of the returned guard.
    pub(crate) fn push_services(
        self: &Arc<Self>,
        part: Part,
        services: Scoped<dyn ModelServices>,
        cx: &Arc<LendContext>,
    ) -> ServicesGuard {
        self.lock_services().push(Lend {
            part,
            handle: services,
            cx: Arc::clone(cx),
        });
        ServicesGuard(Arc::clone(self))
    }

    /// Remember the Python object of the runtime law `id`.
    pub(crate) fn register_law_object(&self, part: Part, id: String, obj: Arc<Py<PyAny>>) {
        self.laws
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((part, id), obj);
    }

    /// The Python object of the runtime law `id`, if any.
    pub(crate) fn law_object(&self, part: Part, id: &str) -> Option<Arc<Py<PyAny>>> {
        self.laws
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(part, id.to_owned()))
            .cloned()
    }

    /// Remember the selector object set from Python.
    pub(crate) fn set_selector_object(&self, part: Part, handler: Handler, obj: Py<PyAny>) {
        self.selectors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((part, handler), obj);
    }

    /// The selector object set from Python, if any.
    pub(crate) fn selector_object(
        &self,
        py: Python<'_>,
        part: Part,
        handler: Handler,
    ) -> Option<Py<PyAny>> {
        self.selectors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(part, handler))
            .map(|o| o.clone_ref(py))
    }
}

/// The Python model object of `model` (the class of the model it
/// designates).
pub(crate) fn model_object<'py>(py: Python<'py>, model: &ModelRef) -> PyResult<Bound<'py, PyAny>> {
    match model.h.kind(model.part) {
        ModelKind::Cohesive => {
            Ok(super::cohesive::PySolidMechanicsModelCohesive::wrap(py, model.clone())?.into_any())
        }
        ModelKind::Solid => {
            Ok(super::solid::PySolidMechanicsModel::wrap(py, model.clone())?.into_any())
        }
        other => super::physics::model_object(py, model, other),
    }
}

/// The class of a model part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelKind {
    Solid,
    Cohesive,
    Heat,
    Structural,
    PhaseField,
    Contact,
    SolidContact,
    SolidCohesiveContact,
    SolidPhaseField,
}

impl ModelHandle {
    /// The class of `part` of this handle's model.
    pub(crate) fn kind(&self, part: Part) -> ModelKind {
        match (self.root_kind, part) {
            (k, Part::Root) => k,
            (ModelKind::SolidCohesiveContact, Part::Solid) => ModelKind::Cohesive,
            (_, Part::Solid) => ModelKind::Solid,
            (_, Part::Contact) => ModelKind::Contact,
            (_, Part::PhaseField) => ModelKind::PhaseField,
        }
    }
}

/// Pops the innermost lent services on drop.
pub(crate) struct ServicesGuard(Arc<ModelHandle>);

impl Drop for ServicesGuard {
    fn drop(&mut self) {
        self.0.lock_services().pop();
    }
}

/// Pops the innermost solve access on drop.
pub(crate) struct AccessGuard(Arc<ModelHandle>);

impl Drop for AccessGuard {
    fn drop(&mut self) {
        self.0.accesses().pop();
    }
}

/// A model object as used by bindings: a handle and the part it designates.
#[derive(Clone, Debug)]
pub(crate) struct ModelRef {
    pub(crate) h: Arc<ModelHandle>,
    pub(crate) part: Part,
}

impl ModelRef {
    /// The root model of a new handle.
    pub(crate) fn root(h: Arc<ModelHandle>) -> Self {
        Self {
            h,
            part: Part::Root,
        }
    }

    /// The same handle, another part.
    pub(crate) fn with_part(&self, part: Part) -> Self {
        Self {
            h: Arc::clone(&self.h),
            part,
        }
    }

    /// Run `f` on the whole model (native section).
    pub(crate) fn with_any<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut AnyModel, Part) -> Result<R> + Send,
    ) -> PyResult<R> {
        let h = Arc::clone(&self.h);
        let part = self.part;
        native(py, move || h.with(|m| f(m, part)))
    }

    /// Run `f` on the generic operations of this model (native section).
    pub(crate) fn with_ops<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut dyn ModelOps) -> Result<R> + Send,
    ) -> PyResult<R> {
        self.with_any(py, move |m, part| f(m.ops(part)?))
    }

    /// Run `f` on the solid mechanics model of this object (native
    /// section).
    pub(crate) fn with<R: Send>(
        &self,
        py: Python<'_>,
        f: impl FnOnce(&mut SolidMechanicsModel) -> Result<R> + Send,
    ) -> PyResult<R> {
        self.with_any(py, move |m, part| f(m.solid(part)?))
    }

    /// `getCurrentPosition()`: a NumPy view of the current position,
    /// updated first as upstream does (also inside callbacks of a running
    /// call).
    pub(crate) fn current_position_view<'py>(
        &self,
        py: Python<'py>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = Arc::clone(&self.h);
        let part = self.part;
        let v = match native(py, move || h.export_current_position(part))? {
            CurrentPosition::Ready(v) => v,
            CurrentPosition::Held { cp, u, nodes } => {
                let interp = runtime().enter().py_err(py)?;
                set_current_position(&interp, &cp, &u, &nodes).py_err(py)?;
                drop(interp);
                cp
            }
        };
        into_numpy(py, v, Layout::Rows)
    }

    /// A NumPy view of the array `key` of this model.
    pub(crate) fn view<'py, T: ModelValue>(
        &self,
        py: Python<'py>,
        key: ModelKey,
    ) -> PyResult<Bound<'py, PyAny>> {
        let h = Arc::clone(&self.h);
        let part = self.part;
        let v = native(py, move || h.export::<T>(part, key))?;
        into_numpy(py, v, Layout::Rows)
    }

    /// The innermost solve access of a running callback of this model and
    /// the key of its execution context (an error for a caller outside the
    /// callback).
    pub(crate) fn active_access(
        &self,
        py: Python<'_>,
    ) -> Result<Option<(ScopedMut<dyn SolverAccess>, ContextKey)>> {
        self.h.active_access(py, self.part)
    }
}
