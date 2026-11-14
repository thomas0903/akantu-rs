// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): python/py_model.cc (the `Model` /
// `ModelSolver` / `SolverCallback` methods every model class inherits:
// solveStep, getDOFManager, assembleMatrix, dumpers, solvers, integration
// schemes), src/model/model.hh, src/model/common/model_solver.hh.

//! Generic model operations: what the Python base classes
//! `SolverCallback` ← `ModelSolver` ← `Model` need of any Rust model.
//!
//! Every Rust model type implements [`ModelOps`] by delegating to its own
//! methods; an operation a model does not have returns an
//! `akantu.Exception` naming the model class (upstream: the method of the
//! C++ base class, e.g. `AKANTU_TO_IMPLEMENT()`).

use std::path::Path;

use akantu::core::array_shared::SharedArray;
use akantu::core::enums::{AnalysisMethod, IntegrationSchemeType};
use akantu::core::{ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result};
use akantu::fe::engine::FeEngine;
use akantu::mesh::Mesh;
use akantu::model::boundary_condition::{DirichletFunctor, NeumannFunctor};
use akantu::model::constitutive_law::{ConstitutiveLaw, InternalFieldMut, SelectorChain};
use akantu::solid::SolidMechanicsModel;
use akantu::solid::material::Material;
use akantu::solver::callback::InterceptCallback;
use akantu::solver::integration_scheme::SolutionType;
use akantu::solver::model_solver::{ModelSolver, ModelSolverOptions};
use akantu::solver::{DofManager, SolverCallback};

use super::handle::Handler;

/// `Error` of an operation a model does not provide.
pub(crate) fn unsupported(class: &str, what: &str) -> Error {
    Error::invalid(format!("{class} does not provide {what}"))
}

/// The real nodal fields of a solid mechanics model (upstream names). They
/// are exported after every model call, so their getters also work inside
/// Python callbacks of a later call (the leases are bookkeeping only).
pub(crate) const SOLID_NODAL_FIELDS: [&str; 9] = [
    "displacement",
    "velocity",
    "acceleration",
    "displacement_increment",
    "previous_displacement",
    "external_force",
    "internal_force",
    "mass",
    "current_position",
];

/// The body of a solve with a replacing callback: `(model callback,
/// solver, solver id)`.
pub(crate) type ReplacedRun<'a> =
    dyn FnMut(&mut dyn SolverCallback, &mut ModelSolver, Option<&str>) -> Result<()> + 'a;

/// Generic model operations.
#[allow(unused_variables)] // default bodies ignore their arguments
pub(crate) trait ModelOps {
    /// Upstream Python class name (error messages).
    fn class_name(&self) -> &'static str;

    /// Model id.
    fn id(&self) -> String;

    /// Spatial dimension.
    fn spatial_dimension(&self) -> usize;

    /// The model's solver (DOF manager + time-step solvers).
    fn model_solver(&mut self) -> Result<&mut ModelSolver> {
        Err(unsupported(self.class_name(), "a model solver"))
    }

    /// Run `f` on the model's own solver callback and its DOF manager
    /// (upstream: the model *is* its `SolverCallback`).
    fn with_callback(
        &mut self,
        f: &mut dyn FnMut(&mut dyn SolverCallback, &mut DofManager) -> Result<()>,
    ) -> Result<()> {
        Err(unsupported(
            self.class_name(),
            "the solver-callback interface",
        ))
    }

    /// `solveStep(solver_id)`.
    fn solve_step(&mut self, id: Option<&str>) -> Result<()> {
        Err(unsupported(self.class_name(), "solveStep"))
    }

    /// `solveStep(callback, solver_id)` with an `InterceptSolverCallback`.
    fn solve_step_intercepted(
        &mut self,
        user: &mut dyn InterceptCallback,
        id: Option<&str>,
    ) -> Result<()> {
        Err(unsupported(
            self.class_name(),
            "solveStep with an InterceptSolverCallback",
        ))
    }

    /// `solveStep(callback, solver_id)` with a `SolverCallback` replacing
    /// the model's own callback: the model's pre-step checks, then `run`
    /// with the model's own callback (not driven by the solve) and its
    /// solver.
    fn solve_step_replaced(&mut self, id: Option<&str>, run: &mut ReplacedRun<'_>) -> Result<()> {
        Err(unsupported(
            self.class_name(),
            "solveStep with a SolverCallback",
        ))
    }

    /// Names of the exportable real nodal fields (exported after every
    /// call).
    fn nodal_names(&self) -> &'static [&'static str] {
        &[]
    }

    /// An exportable real nodal field by upstream name.
    fn nodal_shared_mut(&mut self, name: &str) -> Result<&mut SharedArray<Real>> {
        Err(Error::not_found("nodal field", name))
    }

    /// The blocked DOFs (exportable).
    fn blocked_shared_mut(&mut self) -> Result<&mut SharedArray<bool>> {
        Err(unsupported(self.class_name(), "blocked DOFs"))
    }

    /// Refresh `current_position` before it is fetched.
    fn update_current_position(&mut self) -> Result<()> {
        Ok(())
    }

    /// Run `f` on the block `(ty, ghost)` of the real internal `name` of law
    /// `law` of a handler.
    fn with_law_internal(
        &mut self,
        handler: Handler,
        law: usize,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
        f: &mut dyn FnMut(&mut SharedArray<Real>) -> Result<()>,
    ) -> Result<()> {
        Err(unsupported(self.class_name(), "constitutive laws"))
    }

    /// Number of laws of a handler.
    fn law_count(&mut self, handler: Handler) -> Result<usize> {
        Err(unsupported(
            self.class_name(),
            "this constitutive-law handler",
        ))
    }

    /// Law index of a handler by name.
    fn law_index_by_name(&mut self, handler: Handler, name: &str) -> Result<usize> {
        Err(unsupported(
            self.class_name(),
            "this constitutive-law handler",
        ))
    }

    /// Run `f` on law `index` of a handler.
    fn with_handler_law(
        &mut self,
        handler: Handler,
        index: usize,
        f: &mut dyn FnMut(&mut dyn ConstitutiveLaw) -> Result<()>,
    ) -> Result<()> {
        Err(unsupported(
            self.class_name(),
            "this constitutive-law handler",
        ))
    }

    /// `registerNewConstitutiveLaw(name, type, option)` of a handler.
    fn register_handler_law(
        &mut self,
        handler: Handler,
        name: &str,
        ty: &str,
        option: &str,
    ) -> Result<usize> {
        Err(unsupported(
            self.class_name(),
            "this constitutive-law handler",
        ))
    }

    /// Replace the selector chain of a handler.
    fn set_handler_selector(&mut self, handler: Handler, chain: SelectorChain) -> Result<()> {
        Err(unsupported(
            self.class_name(),
            "this constitutive-law handler",
        ))
    }

    /// `initConstitutiveLaws()` (akantu-rs helper): the model's own law
    /// initialisation (the cohesive model's includes the facet laws and the
    /// insertion).
    fn init_constitutive_laws(&mut self) -> Result<()> {
        Err(unsupported(self.class_name(), "initConstitutiveLaws"))
    }

    /// `reassignConstitutiveLaw()` of a handler (D-078).
    fn reassign_handler_laws(&mut self, handler: Handler) -> Result<()> {
        Err(unsupported(self.class_name(), "reassignConstitutiveLaw"))
    }

    /// A copy of the law-local numbering of a handler (upstream
    /// `getConstitutiveLawLocalNumbering()`; not exportable storage).
    fn law_local_numbering(&mut self, handler: Handler) -> Result<ElementTypeMapArray<Idx>> {
        Err(unsupported(self.class_name(), "constitutive laws"))
    }

    /// The cohesive inserter's staged insertion flags (exportable, while
    /// the Python laws check the insertion).
    fn insertion_facets_shared_mut(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&mut SharedArray<bool>> {
        Err(unsupported(self.class_name(), "getInsertionFacets views"))
    }

    /// The structural model's material index per element (exportable
    /// block, upstream `getElementMaterial(type, ghost)`).
    fn element_material_shared_mut(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&mut SharedArray<u32>> {
        Err(unsupported(self.class_name(), "getElementMaterial"))
    }

    /// The law index per element of a handler (exportable block).
    fn law_by_element_shared_mut(
        &mut self,
        handler: Handler,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&mut SharedArray<i32>> {
        Err(unsupported(self.class_name(), "constitutive laws"))
    }

    /// Run `f` on the FE engine `name` (`""` = default; the boundary engine,
    /// created on first use, if `boundary`) and the mesh.
    fn with_fe_engine(
        &mut self,
        name: &str,
        boundary: bool,
        f: &mut dyn FnMut(&FeEngine, &Mesh) -> Result<()>,
    ) -> Result<()> {
        Err(unsupported(self.class_name(), "FE engines"))
    }

    // ---- dumpers (upstream `Model` dump interface) ----------------------

    fn set_base_name(&mut self, name: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "setBaseName"))
    }
    fn set_directory(&mut self, dir: &Path) -> Result<()> {
        Err(unsupported(self.class_name(), "setDirectory"))
    }
    fn add_dump_field(&mut self, field: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "addDumpField"))
    }
    fn add_dump_field_vector(&mut self, field: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "addDumpFieldVector"))
    }
    fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "setBaseNameToDumper"))
    }
    fn add_dump_field_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "addDumpFieldToDumper"))
    }
    fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "addDumpFieldVectorToDumper"))
    }
    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        Err(unsupported(self.class_name(), "dump"))
    }
    fn dump_named(&mut self, dumper: &str, time: Option<Real>, step: Option<usize>) -> Result<()> {
        Err(unsupported(self.class_name(), "dump(dumper_name)"))
    }
    fn dump_group(&mut self, group: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "dumpGroup"))
    }
    fn set_group_directory(&mut self, dir: &Path, group: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "setGroupDirectory"))
    }
    fn set_group_base_name(&mut self, name: &str, group: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "setGroupBaseName"))
    }
    fn add_dump_group_field(&mut self, field: &str, group: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "addDumpGroupField"))
    }
    fn add_dump_group_field_vector(&mut self, field: &str, group: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "addDumpGroupFieldVector"))
    }

    // ---- solvers ------------------------------------------------------------

    /// `getNewSolver(id, options)`.
    fn get_new_solver(&mut self, id: &str, options: &ModelSolverOptions) -> Result<()> {
        Err(unsupported(self.class_name(), "getNewSolver"))
    }
    /// `setIntegrationScheme(id, primal, type, solution_type)`.
    fn set_integration_scheme(
        &mut self,
        id: &str,
        primal: &str,
        ty: IntegrationSchemeType,
        solution_type: SolutionType,
    ) -> Result<()> {
        Err(unsupported(self.class_name(), "setIntegrationScheme"))
    }
    /// `initNewSolver(method)`.
    fn init_new_solver(&mut self, method: AnalysisMethod) -> Result<()> {
        Err(unsupported(self.class_name(), "initNewSolver"))
    }
    /// `setTimeStep(time_step, solver_id)`.
    fn set_time_step(&mut self, time_step: Real, id: Option<&str>) -> Result<()> {
        self.model_solver()?.set_time_step(time_step, id)
    }
    /// `getTimeStep(solver_id)`.
    fn time_step(&mut self, id: Option<&str>) -> Result<Real> {
        self.model_solver()?.time_step(id)
    }

    // ---- boundary conditions ---------------------------------------------------

    /// `applyBC(DirichletFunctor, group)`.
    fn apply_dirichlet(&mut self, functor: &dyn DirichletFunctor, group: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "applyBC(DirichletFunctor)"))
    }
    /// `applyBC(NeumannFunctor, group)`.
    fn apply_neumann(&mut self, functor: &dyn NeumannFunctor, group: &str) -> Result<()> {
        Err(unsupported(self.class_name(), "applyBC(NeumannFunctor)"))
    }
}

/// `with_law_internal` on top of `with_handler_law`.
fn handler_law_internal(
    ops: &mut dyn ModelOps,
    handler: Handler,
    law: usize,
    name: &str,
    ty: ElementType,
    ghost: GhostType,
    f: &mut dyn FnMut(&mut SharedArray<Real>) -> Result<()>,
) -> Result<()> {
    let mut found = false;
    ops.with_handler_law(handler, law, &mut |l| {
        l.for_each_internal_mut(&mut |n, field| {
            if n == name
                && !found
                && let InternalFieldMut::Real(field) = field
            {
                found = true;
                f(field.values_mut().shared_mut(ty, ghost)?)?;
            }
            Ok(())
        })
    })?;
    if found {
        Ok(())
    } else {
        Err(Error::not_found(
            "real internal field",
            format!("{name} ({ty}, {ghost})"),
        ))
    }
}

/// A DOF family array (`order` 0: values, 1, 2: derivatives).
fn dof_shared<'a>(
    ms: &'a mut ModelSolver,
    family: &str,
    order: usize,
) -> Result<&'a mut SharedArray<Real>> {
    let f = ms.dof_manager.family_mut(family)?;
    if order == 0 {
        Ok(&mut f.dofs)
    } else {
        f.derivatives
            .get_mut(&order)
            .ok_or_else(|| Error::not_found("DOF derivative", format!("{family}:{order}")))
    }
}

/// The blocked flags of a DOF family.
fn dof_blocked<'a>(ms: &'a mut ModelSolver, family: &str) -> Result<&'a mut SharedArray<bool>> {
    ms.dof_manager
        .family_mut(family)?
        .blocked
        .as_mut()
        .ok_or_else(|| Error::not_found("blocked DOFs", family))
}

/// FE engine of a [`akantu::model::ModelBase`] registry.
fn base_fe_engine(
    base: &mut akantu::model::model::ModelBase,
    name: &str,
    boundary: bool,
    f: &mut dyn FnMut(&FeEngine, &Mesh) -> Result<()>,
) -> Result<()> {
    let mesh_h = base.mesh.clone();
    let mesh = mesh_h.read()?;
    let fe = if boundary {
        base.fems.boundary(name, &mesh)?
    } else {
        base.fems.get(name)?
    };
    f(fe, &mesh)
}

fn sid(id: Option<&str>) -> Option<&str> {
    id.filter(|s| !s.is_empty())
}

// ---- SolidMechanicsModel -------------------------------------------------------

impl ModelOps for SolidMechanicsModel {
    fn init_constitutive_laws(&mut self) -> Result<()> {
        self.init_materials()
    }
    fn class_name(&self) -> &'static str {
        "SolidMechanicsModel"
    }
    fn id(&self) -> String {
        SolidMechanicsModel::id(self).to_owned()
    }
    fn spatial_dimension(&self) -> usize {
        SolidMechanicsModel::spatial_dimension(self)
    }
    fn model_solver(&mut self) -> Result<&mut ModelSolver> {
        Ok(&mut self.solver)
    }
    fn with_callback(
        &mut self,
        f: &mut dyn FnMut(&mut dyn SolverCallback, &mut DofManager) -> Result<()>,
    ) -> Result<()> {
        f(&mut self.physics, &mut self.solver.dof_manager)
    }
    fn solve_step(&mut self, id: Option<&str>) -> Result<()> {
        SolidMechanicsModel::solve_step(self, sid(id))
    }
    fn solve_step_intercepted(
        &mut self,
        user: &mut dyn InterceptCallback,
        id: Option<&str>,
    ) -> Result<()> {
        SolidMechanicsModel::solve_step_intercepted(self, user, sid(id))
    }
    fn solve_step_replaced(&mut self, id: Option<&str>, run: &mut ReplacedRun<'_>) -> Result<()> {
        self.solve_step_with(sid(id), |physics, solver, id| run(physics, solver, id))
    }
    fn nodal_names(&self) -> &'static [&'static str] {
        &SOLID_NODAL_FIELDS
    }
    fn nodal_shared_mut(&mut self, name: &str) -> Result<&mut SharedArray<Real>> {
        SolidMechanicsModel::nodal_field_shared_mut(self, name)
    }
    fn blocked_shared_mut(&mut self) -> Result<&mut SharedArray<bool>> {
        SolidMechanicsModel::blocked_dofs_shared_mut(self)
    }
    fn update_current_position(&mut self) -> Result<()> {
        SolidMechanicsModel::update_current_position(self)
    }
    fn with_law_internal(
        &mut self,
        handler: Handler,
        law: usize,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
        f: &mut dyn FnMut(&mut SharedArray<Real>) -> Result<()>,
    ) -> Result<()> {
        if handler != Handler::Material {
            return Err(unsupported(
                self.class_name(),
                "this constitutive-law handler",
            ));
        }
        let law_ref: &mut dyn Material = self.material_mut(law)?;
        let mut found = false;
        law_ref.for_each_internal_mut(&mut |n, field| {
            if n == name
                && !found
                && let InternalFieldMut::Real(field) = field
            {
                found = true;
                f(field.values_mut().shared_mut(ty, ghost)?)?;
            }
            Ok(())
        })?;
        if found {
            Ok(())
        } else {
            Err(Error::not_found(
                "real internal field",
                format!("{name} ({ty}, {ghost})"),
            ))
        }
    }
    fn law_by_element_shared_mut(
        &mut self,
        handler: Handler,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&mut SharedArray<i32>> {
        if handler != Handler::Material {
            return Err(unsupported(
                self.class_name(),
                "this constitutive-law handler",
            ));
        }
        self.material_by_element_mut().shared_mut(ty, ghost)
    }
    fn with_fe_engine(
        &mut self,
        name: &str,
        boundary: bool,
        f: &mut dyn FnMut(&FeEngine, &Mesh) -> Result<()>,
    ) -> Result<()> {
        base_fe_engine(&mut self.physics.base, name, boundary, f)
    }
    fn set_base_name(&mut self, name: &str) -> Result<()> {
        SolidMechanicsModel::set_base_name(self, name)
    }
    fn set_directory(&mut self, dir: &Path) -> Result<()> {
        SolidMechanicsModel::set_directory(self, dir)
    }
    fn add_dump_field(&mut self, field: &str) -> Result<()> {
        SolidMechanicsModel::add_dump_field(self, field)
    }
    fn add_dump_field_vector(&mut self, field: &str) -> Result<()> {
        SolidMechanicsModel::add_dump_field_vector(self, field)
    }
    fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        SolidMechanicsModel::set_base_name_to_dumper(self, dumper, name)
    }
    fn add_dump_field_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
        SolidMechanicsModel::add_dump_field_to_dumper(self, dumper, field)
    }
    fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
        SolidMechanicsModel::add_dump_field_vector_to_dumper(self, dumper, field)
    }
    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        SolidMechanicsModel::dump(self, time, step)
    }
    fn dump_named(&mut self, dumper: &str, time: Option<Real>, step: Option<usize>) -> Result<()> {
        SolidMechanicsModel::dump_named(self, dumper, time, step)
    }
    fn get_new_solver(&mut self, id: &str, options: &ModelSolverOptions) -> Result<()> {
        SolidMechanicsModel::get_new_solver(self, id, options)
    }
    fn set_integration_scheme(
        &mut self,
        id: &str,
        primal: &str,
        ty: IntegrationSchemeType,
        solution_type: SolutionType,
    ) -> Result<()> {
        SolidMechanicsModel::set_integration_scheme(self, id, primal, ty, solution_type)
    }
    fn init_new_solver(&mut self, method: AnalysisMethod) -> Result<()> {
        SolidMechanicsModel::init_new_solver(self, method)
    }
    fn set_time_step(&mut self, time_step: Real, id: Option<&str>) -> Result<()> {
        SolidMechanicsModel::set_time_step(self, time_step, sid(id))
    }
    fn time_step(&mut self, id: Option<&str>) -> Result<Real> {
        SolidMechanicsModel::time_step(self, sid(id))
    }
    fn apply_dirichlet(&mut self, functor: &dyn DirichletFunctor, group: &str) -> Result<()> {
        self.apply_dirichlet_bc(functor, group)
    }
    fn apply_neumann(&mut self, functor: &dyn NeumannFunctor, group: &str) -> Result<()> {
        self.apply_neumann_bc(functor, group)
    }
}

// ---- other models (completed by their groups) -----------------------------------

/// `ModelOps` for a model with `physics` (its callback, with a `base`) and
/// `solver` fields, inherent `id`, `spatial_dimension`, `solve_step`,
/// `set_time_step`, the dump methods, and its nodal arrays in the DOF family
/// `$family` (`(name, derivative order)`).
macro_rules! physics_solver_ops {
    ($t:ty, $name:literal, $family:literal, [$(($n:literal, $o:literal)),*], { $($extra:tt)* }) => {
        physics_solver_ops!($t, $name, $family, [$(($n, $o)),*], [], { $($extra)* });
    };
    ($t:ty, $name:literal, $family:literal, [$(($n:literal, $o:literal)),*], [$(($xn:literal, $xget:ident)),*], { $($extra:tt)* }) => {
        impl ModelOps for $t {
            fn class_name(&self) -> &'static str {
                $name
            }
            fn id(&self) -> String {
                <$t>::id(self).to_owned()
            }
            fn spatial_dimension(&self) -> usize {
                <$t>::spatial_dimension(self)
            }
            fn model_solver(&mut self) -> Result<&mut ModelSolver> {
                Ok(&mut self.solver)
            }
            fn with_callback(
                &mut self,
                f: &mut dyn FnMut(&mut dyn SolverCallback, &mut DofManager) -> Result<()>,
            ) -> Result<()> {
                f(&mut self.physics, &mut self.solver.dof_manager)
            }
            fn solve_step(&mut self, id: Option<&str>) -> Result<()> {
                <$t>::solve_step(self, sid(id))
            }
            fn solve_step_replaced(
                &mut self,
                id: Option<&str>,
                run: &mut ReplacedRun<'_>,
            ) -> Result<()> {
                self.physics.base.mesh.check_current(&self.token)?;
                run(&mut self.physics, &mut self.solver, sid(id))
            }
            fn nodal_names(&self) -> &'static [&'static str] {
                &[$($n,)* $($xn),*]
            }
            fn nodal_shared_mut(&mut self, name: &str) -> Result<&mut SharedArray<Real>> {
                match name {
                    $($n => dof_shared(&mut self.solver, $family, $o),)*
                    $($xn => self.$xget(),)*
                    _ => Err(Error::not_found("nodal field", name)),
                }
            }
            fn blocked_shared_mut(&mut self) -> Result<&mut SharedArray<bool>> {
                dof_blocked(&mut self.solver, $family)
            }
            fn with_fe_engine(
                &mut self,
                name: &str,
                boundary: bool,
                f: &mut dyn FnMut(&FeEngine, &Mesh) -> Result<()>,
            ) -> Result<()> {
                base_fe_engine(&mut self.physics.base, name, boundary, f)
            }
            fn set_base_name(&mut self, name: &str) -> Result<()> {
                <$t>::set_base_name(self, name)
            }
            fn set_directory(&mut self, dir: &Path) -> Result<()> {
                <$t>::set_directory(self, dir)
            }
            fn add_dump_field(&mut self, field: &str) -> Result<()> {
                <$t>::add_dump_field(self, field)
            }
            fn add_dump_field_vector(&mut self, field: &str) -> Result<()> {
                <$t>::add_dump_field_vector(self, field)
            }
            fn add_dump_field_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
                <$t>::add_dump_field_to_dumper(self, dumper, field)
            }
            fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
                <$t>::dump(self, time, step)
            }
            fn dump_named(
                &mut self,
                dumper: &str,
                time: Option<Real>,
                step: Option<usize>,
            ) -> Result<()> {
                <$t>::dump_named(self, dumper, time, step)
            }
            fn set_time_step(&mut self, time_step: Real, id: Option<&str>) -> Result<()> {
                <$t>::set_time_step(self, time_step, sid(id))
            }
            fn get_new_solver(&mut self, id: &str, options: &ModelSolverOptions) -> Result<()> {
                <$t>::get_new_solver(self, id, options)
            }
            fn init_new_solver(&mut self, method: AnalysisMethod) -> Result<()> {
                <$t>::init_new_solver(self, method)
            }
            $($extra)*
        }
    };
}

/// The law-handler methods of a model whose laws are `$laws` (a
/// `ConstitutiveLawsHandler`) of handler `$h`.
macro_rules! handler_ops {
    ($h:path, $m:ident, $count:expr, $index:expr, $laws:expr, $register:expr, $selector:expr, $by_el:expr, $numbering:expr) => {
        fn law_count(&mut self, handler: Handler) -> Result<usize> {
            check_handler(handler, $h)?;
            let $m = &mut *self;
            Ok($count)
        }
        fn law_index_by_name(&mut self, handler: Handler, name: &str) -> Result<usize> {
            check_handler(handler, $h)?;
            let $m = &mut *self;
            $index(name)
        }
        fn with_handler_law(
            &mut self,
            handler: Handler,
            index: usize,
            f: &mut dyn FnMut(&mut dyn ConstitutiveLaw) -> Result<()>,
        ) -> Result<()> {
            check_handler(handler, $h)?;
            let $m = &mut *self;
            let law = $laws.law_mut(index)?;
            f(law as &mut dyn ConstitutiveLaw)
        }
        fn register_handler_law(
            &mut self,
            handler: Handler,
            name: &str,
            ty: &str,
            option: &str,
        ) -> Result<usize> {
            check_handler(handler, $h)?;
            let $m = &mut *self;
            $register(name, ty, option)
        }
        fn set_handler_selector(&mut self, handler: Handler, chain: SelectorChain) -> Result<()> {
            check_handler(handler, $h)?;
            let $m = &mut *self;
            $selector(chain);
            Ok(())
        }
        fn with_law_internal(
            &mut self,
            handler: Handler,
            law: usize,
            name: &str,
            ty: ElementType,
            ghost: GhostType,
            f: &mut dyn FnMut(&mut SharedArray<Real>) -> Result<()>,
        ) -> Result<()> {
            handler_law_internal(self, handler, law, name, ty, ghost, f)
        }
        fn law_by_element_shared_mut(
            &mut self,
            handler: Handler,
            ty: ElementType,
            ghost: GhostType,
        ) -> Result<&mut SharedArray<i32>> {
            check_handler(handler, $h)?;
            let $m = &mut *self;
            $by_el.shared_mut(ty, ghost)
        }
        fn law_local_numbering(&mut self, handler: Handler) -> Result<ElementTypeMapArray<Idx>> {
            check_handler(handler, $h)?;
            let $m = &mut *self;
            Ok($numbering.clone())
        }
    };
}

fn check_handler(handler: Handler, expected: Handler) -> Result<()> {
    if handler == expected {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "internal error: handler {handler:?} on a model of handler {expected:?}"
        )))
    }
}

type Pf = akantu::phase_field::PhaseFieldModel;
type Dm = akantu::diffusion::DiffusionModel;

physics_solver_ops!(
    Pf,
    "PhaseFieldModel",
    "damage",
    [("damage", 0)],
    [("internal_force", internal_force_shared_mut)],
    {
        handler_ops!(
            Handler::PhaseField,
            m,
            m.nb_phase_fields(),
            |n: &str| m.phase_field_index(n),
            m.physics.phasefields,
            |n: &str, t: &str, o: &str| m.register_new_phase_field(n, t, o),
            |c| m.set_phase_field_selector_chain(c),
            m.phase_field_by_element_mut(),
            m.phase_field_local_numbering()
        );
        fn reassign_handler_laws(&mut self, handler: Handler) -> Result<()> {
            check_handler(handler, Handler::PhaseField)?;
            self.reassign_phase_field()
        }
        fn apply_dirichlet(&mut self, functor: &dyn DirichletFunctor, group: &str) -> Result<()> {
            self.apply_dirichlet_bc(functor, group)
        }
        fn apply_neumann(&mut self, functor: &dyn NeumannFunctor, group: &str) -> Result<()> {
            self.apply_neumann_bc(functor, group)
        }
    }
);

physics_solver_ops!(
    akantu::structural::StructuralMechanicsModel,
    "StructuralMechanicsModel",
    "displacement",
    [("displacement", 0), ("velocity", 1), ("acceleration", 2)],
    [
        ("external_force", external_force_shared_mut),
        ("internal_force", internal_force_shared_mut)
    ],
    {
        fn element_material_shared_mut(
            &mut self,
            ty: ElementType,
            ghost: GhostType,
        ) -> Result<&mut SharedArray<u32>> {
            self.element_material_map_mut().shared_mut(ty, ghost)
        }
        fn solve_step_intercepted(
            &mut self,
            user: &mut dyn InterceptCallback,
            id: Option<&str>,
        ) -> Result<()> {
            akantu::structural::StructuralMechanicsModel::solve_step_intercepted(
                self,
                user,
                sid(id),
            )
        }
        fn set_integration_scheme(
            &mut self,
            id: &str,
            primal: &str,
            ty: IntegrationSchemeType,
            solution_type: SolutionType,
        ) -> Result<()> {
            akantu::structural::StructuralMechanicsModel::set_integration_scheme(
                self,
                id,
                primal,
                ty,
                solution_type,
            )
        }
    }
);

physics_solver_ops!(
    Dm,
    "DiffusionModel",
    "temperature",
    [("temperature", 0), ("temperature_rate", 1)],
    {
        handler_ops!(
            Handler::Diffusion,
            m,
            m.nb_laws(),
            |n: &str| m.law_index(n),
            m.physics.laws,
            |n: &str, t: &str, o: &str| m.register_new_law(n, t, o),
            |c| m.set_law_selector_chain(c),
            m.law_by_element_mut(),
            Dm::law_local_numbering(m)
        );
        fn reassign_handler_laws(&mut self, handler: Handler) -> Result<()> {
            check_handler(handler, Handler::Diffusion)?;
            self.reassign_law()
        }
        fn solve_step_intercepted(
            &mut self,
            user: &mut dyn InterceptCallback,
            id: Option<&str>,
        ) -> Result<()> {
            Dm::solve_step_intercepted(self, user, sid(id))
        }
        fn set_integration_scheme(
            &mut self,
            id: &str,
            primal: &str,
            ty: IntegrationSchemeType,
            solution_type: SolutionType,
        ) -> Result<()> {
            Dm::set_integration_scheme(self, id, primal, ty, solution_type)
        }
        fn apply_dirichlet(&mut self, functor: &dyn DirichletFunctor, group: &str) -> Result<()> {
            self.apply_dirichlet_bc(functor, group)
        }
    }
);

impl ModelOps for akantu::cohesive::SolidMechanicsModelCohesive {
    fn class_name(&self) -> &'static str {
        "SolidMechanicsModelCohesive"
    }
    fn init_constitutive_laws(&mut self) -> Result<()> {
        akantu::cohesive::SolidMechanicsModelCohesive::init_constitutive_laws(self)
    }
    fn insertion_facets_shared_mut(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&mut SharedArray<bool>> {
        self.staged_insertion_facets_mut(ty, ghost)
    }
    fn id(&self) -> String {
        self.smm.id().to_owned()
    }
    fn spatial_dimension(&self) -> usize {
        self.smm.spatial_dimension()
    }
    fn model_solver(&mut self) -> Result<&mut ModelSolver> {
        Ok(&mut self.smm.solver)
    }
    fn with_callback(
        &mut self,
        f: &mut dyn FnMut(&mut dyn SolverCallback, &mut DofManager) -> Result<()>,
    ) -> Result<()> {
        let mut cb = akantu::cohesive::model::CohesiveCallback::new(&mut self.smm.physics);
        f(&mut cb, &mut self.smm.solver.dof_manager)
    }
    fn solve_step(&mut self, id: Option<&str>) -> Result<()> {
        akantu::cohesive::SolidMechanicsModelCohesive::solve_step(self, sid(id))
    }
    fn solve_step_intercepted(
        &mut self,
        user: &mut dyn InterceptCallback,
        id: Option<&str>,
    ) -> Result<()> {
        // the cohesive callback around the solid physics, as `solve_step`
        // (the UD-010 legacy-hook fallback commit is not applied here)
        self.smm.solve_step_with(sid(id), |physics, solver, id| {
            let mut cb = akantu::cohesive::model::CohesiveCallback::new(physics);
            solver.solve_step_intercepted(&mut cb, user, id)
        })
    }
    fn solve_step_replaced(&mut self, id: Option<&str>, run: &mut ReplacedRun<'_>) -> Result<()> {
        self.smm.solve_step_with(sid(id), |physics, solver, id| {
            let mut cb = akantu::cohesive::model::CohesiveCallback::new(physics);
            run(&mut cb, solver, id)
        })
    }
    fn nodal_names(&self) -> &'static [&'static str] {
        &SOLID_NODAL_FIELDS
    }
    fn nodal_shared_mut(&mut self, name: &str) -> Result<&mut SharedArray<Real>> {
        ModelOps::nodal_shared_mut(&mut self.smm, name)
    }
    fn blocked_shared_mut(&mut self) -> Result<&mut SharedArray<bool>> {
        ModelOps::blocked_shared_mut(&mut self.smm)
    }
    fn update_current_position(&mut self) -> Result<()> {
        ModelOps::update_current_position(&mut self.smm)
    }
    fn with_law_internal(
        &mut self,
        handler: Handler,
        law: usize,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
        f: &mut dyn FnMut(&mut SharedArray<Real>) -> Result<()>,
    ) -> Result<()> {
        ModelOps::with_law_internal(&mut self.smm, handler, law, name, ty, ghost, f)
    }
    fn law_by_element_shared_mut(
        &mut self,
        handler: Handler,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&mut SharedArray<i32>> {
        ModelOps::law_by_element_shared_mut(&mut self.smm, handler, ty, ghost)
    }
    fn with_fe_engine(
        &mut self,
        name: &str,
        boundary: bool,
        f: &mut dyn FnMut(&FeEngine, &Mesh) -> Result<()>,
    ) -> Result<()> {
        // upstream registers `FacetsFEEngine` (the facet mesh) on the model
        if name == akantu::cohesive::model::FACETS_FE_ENGINE && !boundary {
            let mesh_h = self.smm.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            let fm = mesh
                .mesh_facets()
                .ok_or_else(|| Error::Mesh("no facet mesh".into()))?;
            return f(&self.ext.facet_fe, fm);
        }
        ModelOps::with_fe_engine(&mut self.smm, name, boundary, f)
    }
    fn set_base_name(&mut self, name: &str) -> Result<()> {
        ModelOps::set_base_name(&mut self.smm, name)
    }
    fn set_directory(&mut self, dir: &Path) -> Result<()> {
        ModelOps::set_directory(&mut self.smm, dir)
    }
    fn add_dump_field(&mut self, field: &str) -> Result<()> {
        ModelOps::add_dump_field(&mut self.smm, field)
    }
    fn add_dump_field_vector(&mut self, field: &str) -> Result<()> {
        ModelOps::add_dump_field_vector(&mut self.smm, field)
    }
    fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        ModelOps::set_base_name_to_dumper(&mut self.smm, dumper, name)
    }
    fn add_dump_field_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
        ModelOps::add_dump_field_to_dumper(&mut self.smm, dumper, field)
    }
    fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
        ModelOps::add_dump_field_vector_to_dumper(&mut self.smm, dumper, field)
    }
    fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
        ModelOps::dump(&mut self.smm, time, step)
    }
    fn dump_named(&mut self, dumper: &str, time: Option<Real>, step: Option<usize>) -> Result<()> {
        ModelOps::dump_named(&mut self.smm, dumper, time, step)
    }
    fn get_new_solver(&mut self, id: &str, options: &ModelSolverOptions) -> Result<()> {
        ModelOps::get_new_solver(&mut self.smm, id, options)
    }
    fn set_integration_scheme(
        &mut self,
        id: &str,
        primal: &str,
        ty: IntegrationSchemeType,
        solution_type: SolutionType,
    ) -> Result<()> {
        ModelOps::set_integration_scheme(&mut self.smm, id, primal, ty, solution_type)
    }
    fn init_new_solver(&mut self, method: AnalysisMethod) -> Result<()> {
        ModelOps::init_new_solver(&mut self.smm, method)
    }
    fn set_time_step(&mut self, time_step: Real, id: Option<&str>) -> Result<()> {
        akantu::cohesive::SolidMechanicsModelCohesive::set_time_step(self, time_step, sid(id))
    }
    fn time_step(&mut self, id: Option<&str>) -> Result<Real> {
        ModelOps::time_step(&mut self.smm, id)
    }
    fn apply_dirichlet(&mut self, functor: &dyn DirichletFunctor, group: &str) -> Result<()> {
        ModelOps::apply_dirichlet(&mut self.smm, functor, group)
    }
    fn apply_neumann(&mut self, functor: &dyn NeumannFunctor, group: &str) -> Result<()> {
        ModelOps::apply_neumann(&mut self.smm, functor, group)
    }
}

/// The model-level dump methods of the contact model and the couplers
/// (D-078: `akantu_contact::dump`, `akantu_couplers::dump`).
macro_rules! model_dump_ops {
    ($T:ty) => {
        fn set_base_name(&mut self, name: &str) -> Result<()> {
            <$T>::set_base_name(self, name)
        }
        fn set_directory(&mut self, dir: &Path) -> Result<()> {
            <$T>::set_directory(self, dir)
        }
        fn add_dump_field(&mut self, field: &str) -> Result<()> {
            <$T>::add_dump_field(self, field)
        }
        fn add_dump_field_vector(&mut self, field: &str) -> Result<()> {
            <$T>::add_dump_field_vector(self, field)
        }
        fn set_base_name_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
            <$T>::set_base_name_to_dumper(self, dumper, name)
        }
        fn add_dump_field_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
            <$T>::add_dump_field_to_dumper(self, dumper, field)
        }
        fn add_dump_field_vector_to_dumper(&mut self, dumper: &str, field: &str) -> Result<()> {
            <$T>::add_dump_field_vector_to_dumper(self, dumper, field)
        }
        fn dump(&mut self, time: Option<Real>, step: Option<usize>) -> Result<()> {
            <$T>::dump(self, time, step)
        }
        fn dump_named(
            &mut self,
            dumper: &str,
            time: Option<Real>,
            step: Option<usize>,
        ) -> Result<()> {
            <$T>::dump_named(self, dumper, time, step)
        }
    };
}

/// The contact nodal fields upstream's module returns by reference
/// (`py_contact_mechanics_model.cc:150-156`), exportable.
const CONTACT_NODAL_FIELDS: [&str; 7] = [
    "external_force",
    "normal_force",
    "tangential_force",
    "internal_force",
    "gaps",
    "normals",
    "nodal_area",
];

impl ModelOps for akantu::contact::ContactMechanicsModel {
    fn class_name(&self) -> &'static str {
        "ContactMechanicsModel"
    }
    fn nodal_names(&self) -> &'static [&'static str] {
        &CONTACT_NODAL_FIELDS
    }
    fn nodal_shared_mut(&mut self, name: &str) -> Result<&mut SharedArray<Real>> {
        let f = &mut self.fields;
        Ok(match name {
            "external_force" => &mut f.external_force,
            "normal_force" => &mut f.normal_force,
            "tangential_force" => &mut f.tangential_force,
            "internal_force" => &mut f.internal_force,
            "gaps" => &mut f.gaps,
            "normals" => &mut f.normals,
            "nodal_area" => &mut f.nodal_area,
            _ => return Err(Error::not_found("nodal field", name)),
        })
    }
    fn id(&self) -> String {
        akantu::contact::ContactMechanicsModel::id(self).to_owned()
    }
    fn spatial_dimension(&self) -> usize {
        akantu::contact::ContactMechanicsModel::spatial_dimension(self)
    }
    fn with_fe_engine(
        &mut self,
        name: &str,
        boundary: bool,
        f: &mut dyn FnMut(&FeEngine, &Mesh) -> Result<()>,
    ) -> Result<()> {
        base_fe_engine(&mut self.base, name, boundary, f)
    }
    model_dump_ops!(akantu::contact::ContactMechanicsModel);
}

impl ModelOps for akantu::couplers::CouplerSolidContact {
    fn class_name(&self) -> &'static str {
        "CouplerSolidContact"
    }
    fn id(&self) -> String {
        akantu::couplers::CouplerSolidContact::id(self).to_owned()
    }
    fn spatial_dimension(&self) -> usize {
        self.solid.spatial_dimension()
    }
    fn model_solver(&mut self) -> Result<&mut ModelSolver> {
        Ok(&mut self.solid.solver)
    }
    fn solve_step(&mut self, id: Option<&str>) -> Result<()> {
        akantu::couplers::CouplerSolidContact::solve_step(self, sid(id))
    }
    model_dump_ops!(akantu::couplers::CouplerSolidContact);
    fn dump_group(&mut self, group: &str) -> Result<()> {
        akantu::couplers::CouplerSolidContact::dump_group(self, group)
    }
    fn set_time_step(&mut self, time_step: Real, id: Option<&str>) -> Result<()> {
        akantu::couplers::CouplerSolidContact::set_time_step(self, time_step, sid(id))
    }
}

impl ModelOps for akantu::couplers::CouplerSolidCohesiveContact {
    fn class_name(&self) -> &'static str {
        "CouplerSolidCohesiveContact"
    }
    fn id(&self) -> String {
        akantu::couplers::CouplerSolidCohesiveContact::id(self).to_owned()
    }
    fn spatial_dimension(&self) -> usize {
        self.solid.smm.spatial_dimension()
    }
    fn model_solver(&mut self) -> Result<&mut ModelSolver> {
        Ok(&mut self.solid.smm.solver)
    }
    fn solve_step(&mut self, id: Option<&str>) -> Result<()> {
        akantu::couplers::CouplerSolidCohesiveContact::solve_step(self, sid(id))
    }
    model_dump_ops!(akantu::couplers::CouplerSolidCohesiveContact);
    fn dump_group(&mut self, group: &str) -> Result<()> {
        akantu::couplers::CouplerSolidCohesiveContact::dump_group(self, group)
    }
    fn set_time_step(&mut self, time_step: Real, id: Option<&str>) -> Result<()> {
        akantu::couplers::CouplerSolidCohesiveContact::set_time_step(self, time_step, sid(id))
    }
}

impl ModelOps for akantu::couplers::CouplerSolidPhaseField {
    fn class_name(&self) -> &'static str {
        "CouplerSolidPhaseField"
    }
    fn id(&self) -> String {
        self.id.to_string()
    }
    fn spatial_dimension(&self) -> usize {
        akantu::couplers::CouplerSolidPhaseField::spatial_dimension(self)
    }
    model_dump_ops!(akantu::couplers::CouplerSolidPhaseField);
    fn dump_group(&mut self, group: &str) -> Result<()> {
        akantu::couplers::CouplerSolidPhaseField::dump_group(self, group)
    }
}
