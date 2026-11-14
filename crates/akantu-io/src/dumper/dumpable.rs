// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/dumper/{dumpable.hh, dumpable.cc,
// dumpable_iohelper.hh, dumpable_inline_impl.hh} (Dumpable), the field
// selection of src/model/model.cc (addDumpGroupFieldToDumper) and
// src/model/solid_mechanics/solid_mechanics_model_io.cc
// (createNodalFieldReal/Bool, createElementalField).

//! Named-dumper registry (upstream `Dumpable`, DUMP-001) and the
//! solid-mechanics field catalogue.
//!
//! Upstream meshes, element groups, node groups and models are
//! `Dumpable`: they own named dumpers, one of which is the default. Here
//! [`Dumpable`] is a plain registry owned by whoever upstream would make
//! `Dumpable` (mesh wrapper, group, model); dumping passes the mesh and the
//! [`FieldResolver`] explicitly.

use std::any::Any;
use std::collections::BTreeMap;
use std::path::Path;

use akantu_core::{Error, Real, Result};
use akantu_mesh::Mesh;

use super::{
    DerivedQuantity, Dumper, DumperParaview, DumperText, FieldDescriptor, FieldResolver,
    FieldSource, FieldSupport, FieldValueType, MeshSelection, Padding, TextDumpMode,
};

/// Registry of named dumpers with a default (upstream `Dumpable`).
#[derive(Debug, Default)]
pub struct Dumpable {
    dumpers: BTreeMap<String, Box<dyn Dumper>>,
    default_dumper: Option<String>,
}

impl Dumpable {
    /// Empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `registerExternalDumper(dumper, name, is_default)`; an existing
    /// name is replaced (upstream: info message).
    pub fn register_dumper(&mut self, name: &str, dumper: Box<dyn Dumper>, is_default: bool) {
        self.dumpers.insert(name.to_owned(), dumper);
        if is_default {
            self.default_dumper = Some(name.to_owned());
        }
    }

    /// `registerDumper<DumperParaview>(name, file_name, is_default)`: the
    /// file name defaults to the dumper name.
    pub fn register_paraview(&mut self, name: &str, file_name: Option<&str>, is_default: bool) {
        let f = file_name.filter(|f| !f.is_empty()).unwrap_or(name);
        self.register_dumper(name, Box::new(DumperParaview::with_name(f)), is_default);
    }

    /// `registerDumper<DumperText>(name, file_name, is_default)`.
    pub fn register_text(&mut self, name: &str, file_name: Option<&str>, is_default: bool) {
        let f = file_name.filter(|f| !f.is_empty()).unwrap_or(name);
        self.register_dumper(
            name,
            Box::new(DumperText::new(f, TextDumpMode::Space, true)),
            is_default,
        );
    }

    /// `getDefaultDumperName`.
    #[must_use]
    pub fn default_dumper_name(&self) -> Option<&str> {
        self.default_dumper.as_deref()
    }

    /// Registered dumper names (sorted).
    pub fn dumper_names(&self) -> impl Iterator<Item = &str> {
        self.dumpers.keys().map(String::as_str)
    }

    fn default_name(&self) -> Result<String> {
        self.default_dumper
            .clone()
            .ok_or_else(|| Error::not_found("default dumper", ""))
    }

    /// `getDumper(name)` (error if unknown, as upstream's exception).
    pub fn dumper(&self, name: &str) -> Result<&dyn Dumper> {
        self.dumpers
            .get(name)
            .map(AsRef::as_ref)
            .ok_or_else(|| Error::not_found("dumper", name))
    }

    /// Mutable `getDumper(name)`.
    pub fn dumper_mut(&mut self, name: &str) -> Result<&mut dyn Dumper> {
        match self.dumpers.get_mut(name) {
            Some(d) => Ok(d.as_mut()),
            None => Err(Error::not_found("dumper", name)),
        }
    }

    /// `getDumper<T>(name)` (error if the dumper has another type).
    pub fn dumper_as<T: Any>(&mut self, name: &str) -> Result<&mut T> {
        self.dumper_mut(name)?
            .as_any_mut()
            .downcast_mut::<T>()
            .ok_or_else(|| {
                Error::InvalidArgument(format!("dumper '{name}' is not of the requested type"))
            })
    }

    /// `getDumper()` (default dumper).
    pub fn default_dumper_mut(&mut self) -> Result<&mut dyn Dumper> {
        let n = self.default_name()?;
        self.dumper_mut(&n)
    }

    /// `addDumpMeshToDumper` / `addDumpFilteredMeshToDumper`.
    pub fn add_dump_mesh_to_dumper(
        &mut self,
        dumper: &str,
        selection: MeshSelection,
    ) -> Result<()> {
        self.dumper_mut(dumper)?.register_mesh_selection(selection)
    }

    /// `addDumpMesh` / `addDumpFilteredMesh` (default dumper).
    pub fn add_dump_mesh(&mut self, selection: MeshSelection) -> Result<()> {
        self.default_dumper_mut()?
            .register_mesh_selection(selection)
    }

    /// `addDumpFieldExternalToDumper` / `internalAddDumpFieldToDumper`.
    pub fn add_dump_field_to_dumper(&mut self, dumper: &str, field: FieldDescriptor) -> Result<()> {
        self.dumper_mut(dumper)?.register_field(field)
    }

    /// `addDumpFieldExternal` (default dumper).
    pub fn add_dump_field(&mut self, field: FieldDescriptor) -> Result<()> {
        self.default_dumper_mut()?.register_field(field)
    }

    /// `removeDumpFieldFromDumper`.
    pub fn remove_dump_field_from_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.dumper_mut(dumper)?.unregister_field(name)
    }

    /// `removeDumpField` (default dumper).
    pub fn remove_dump_field(&mut self, name: &str) -> Result<()> {
        self.default_dumper_mut()?.unregister_field(name)
    }

    /// `registerVariable` on a dumper (written by text dumpers, accepted
    /// and ignored by ParaView/LAMMPS).
    pub fn add_dump_variable_to_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.dumper_mut(dumper)?.register_variable(name)
    }

    /// `unRegisterVariable` on a dumper.
    pub fn remove_dump_variable_from_dumper(&mut self, dumper: &str, name: &str) -> Result<()> {
        self.dumper_mut(dumper)?.unregister_variable(name)
    }

    /// `setDirectoryToDumper`.
    pub fn set_directory_to_dumper(&mut self, dumper: &str, directory: &Path) -> Result<()> {
        self.dumper_mut(dumper)?.set_directory(directory)
    }

    /// `setDirectory` (default dumper).
    pub fn set_directory(&mut self, directory: &Path) -> Result<()> {
        self.default_dumper_mut()?.set_directory(directory)
    }

    /// `setBaseNameToDumper`.
    pub fn set_base_name_to_dumper(&mut self, dumper: &str, base_name: &str) -> Result<()> {
        self.dumper_mut(dumper)?.set_base_name(base_name)
    }

    /// `setBaseName` (default dumper).
    pub fn set_base_name(&mut self, base_name: &str) -> Result<()> {
        self.default_dumper_mut()?.set_base_name(base_name)
    }

    /// `setTimeStepToDumper(name, dt)`.
    pub fn set_time_step_to_dumper(&mut self, dumper: &str, time_step: Real) -> Result<()> {
        self.dumper_mut(dumper)?.set_time_step(time_step)
    }

    /// `setTimeStepToDumper(dt)` (default dumper).
    pub fn set_time_step(&mut self, time_step: Real) -> Result<()> {
        self.default_dumper_mut()?.set_time_step(time_step)
    }

    /// `setTextModeToDumper(name)`.
    pub fn set_text_mode_to_dumper(&mut self, dumper: &str) -> Result<()> {
        self.dumper_mut(dumper)?.set_text_mode()
    }

    /// `setTextModeToDumper()` (default dumper).
    pub fn set_text_mode(&mut self) -> Result<()> {
        self.default_dumper_mut()?.set_text_mode()
    }

    /// `dump(name)` / `dump(name, step)` / `dump(name, time, step)`.
    pub fn dump_named(
        &mut self,
        dumper: &str,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.dumper_mut(dumper)?.dump(mesh, resolver, time, step)
    }

    /// `dump()` / `dump(step)` / `dump(time, step)` (default dumper).
    pub fn dump(
        &mut self,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        self.default_dumper_mut()?.dump(mesh, resolver, time, step)
    }
}

/// Nodal arrays of `SolidMechanicsModel::createNodalFieldReal`.
const SMM_NODAL_REAL: [&str; 7] = [
    "displacement",
    "mass",
    "velocity",
    "acceleration",
    "external_force",
    "internal_force",
    "increment",
];

/// Field descriptor for a `SolidMechanicsModel` dump field name, as
/// `Model::addDumpGroupFieldToDumper` chooses it (DUMP-024…030): nodal
/// arrays (padded to 3 components with `padding`), `blocked_dofs`,
/// `partitions`, `material_index`, the derived `strain`, `Green strain`,
/// `principal strain`, `principal Green strain`, `Von Mises stress`, law
/// internals (`is_internal(name)`, homogenised per element) and otherwise
/// mesh data (`createFieldFromAttachedData`). `group` restricts the field
/// to an element group; upstream's `"all"` sentinel (the whole mesh,
/// `group_manager_inline_impl.hh`) gives `None`.
pub fn solid_mechanics_field(
    name: &str,
    group: Option<&str>,
    padding: bool,
    is_internal: impl Fn(&str) -> bool,
) -> Result<FieldDescriptor> {
    let pad = if padding {
        Padding::To3D
    } else {
        Padding::None
    };
    let d = |source, support, value_type, homogenize| FieldDescriptor {
        name: name.to_owned(),
        source,
        support,
        value_type,
        group: group.filter(|g| *g != "all").map(str::to_owned),
        padding: pad,
        homogenize,
    };
    match name {
        "force" => Err(Error::InvalidArgument(
            "The 'force' field has been renamed in 'external_force'".into(),
        )),
        "residual" => Err(Error::InvalidArgument(
            "The 'residual' field has been replaced by 'internal_force'".into(),
        )),
        "displacement" => Ok(d(
            FieldSource::Dof {
                family: "displacement".into(),
                order: 0,
            },
            FieldSupport::Nodal,
            FieldValueType::Real,
            false,
        )),
        "velocity" | "acceleration" => Ok(d(
            FieldSource::Dof {
                family: "displacement".into(),
                order: if name == "velocity" { 1 } else { 2 },
            },
            FieldSupport::Nodal,
            FieldValueType::Real,
            false,
        )),
        n if SMM_NODAL_REAL.contains(&n) => Ok(d(
            FieldSource::ModelNodal(n.to_owned()),
            FieldSupport::Nodal,
            FieldValueType::Real,
            false,
        )),
        "blocked_dofs" => Ok(FieldDescriptor {
            padding: Padding::None,
            ..d(
                FieldSource::Blocked {
                    family: "displacement".into(),
                },
                FieldSupport::Nodal,
                FieldValueType::Bool,
                false,
            )
        }),
        "partitions" => Ok(d(
            FieldSource::Partition,
            FieldSupport::Elemental,
            FieldValueType::Int,
            false,
        )),
        "material_index" => Ok(d(
            FieldSource::ModelElemental("material_index".into()),
            FieldSupport::Elemental,
            FieldValueType::Int,
            false,
        )),
        _ => {
            let derived = match name {
                "strain" => Some(DerivedQuantity::SmallStrain),
                "Green strain" => Some(DerivedQuantity::GreenStrain),
                "principal strain" => Some(DerivedQuantity::PrincipalSmallStrain),
                "principal Green strain" => Some(DerivedQuantity::PrincipalGreenStrain),
                "Von Mises stress" => Some(DerivedQuantity::VonMisesStress),
                _ => None,
            };
            let input = match derived {
                Some(DerivedQuantity::VonMisesStress) => "stress",
                Some(_) => "grad_u",
                None => name,
            };
            if is_internal(input) {
                let source = match derived {
                    Some(quantity) => FieldSource::Derived {
                        quantity,
                        law: None,
                    },
                    None => FieldSource::Internal {
                        law: None,
                        name: name.to_owned(),
                    },
                };
                Ok(d(
                    source,
                    FieldSupport::QuadraturePoint,
                    FieldValueType::Real,
                    true,
                ))
            } else if derived.is_some() {
                Err(Error::not_found("internal field for dump field", name))
            } else {
                Ok(FieldDescriptor {
                    padding: Padding::None,
                    ..d(
                        FieldSource::Mesh(name.to_owned()),
                        FieldSupport::QuadraturePoint,
                        FieldValueType::Int,
                        false,
                    )
                })
            }
        }
    }
}
