// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model_embedded_interface/embedded_interface_model.cc
// (initFullImpl: the "reinforcement" dumper; addDumpGroupFieldToDumper);
// src/model/solid_mechanics/solid_mechanics_model_io.cc
// (createElementalField); src/model/common/constitutive_laws/
// constitutive_laws_handler_tmpl.hh (flattenInternal).

//! The `reinforcement` dumper (EMB-005): a ParaView dumper on the interface
//! mesh (elements of dimension 1, not ghost, regular; upstream
//! `mesh.registerDumper<DumperParaview>("reinforcement", id)` +
//! `addDumpMeshToDumper("reinforcement", interface_mesh, 1, _not_ghost,
//! _ek_regular)`), whose fields are 1-D elemental fields
//! (`createElementalField(field, group, padding, 1, kind)`): the internals
//! of the reinforcement laws flattened **over the interface mesh** (UD-598:
//! upstream flattens over the background mesh, whose element count differs),
//! `material_index` (law of every interface element, `-1` if none) and
//! `partitions`; any other name gives no field (upstream's
//! `createElementalField` returns none and the `reinforcement` branch adds
//! nothing: no mesh-data fallback), and the fields upstream
//! derives from `grad_u` / `stress` (`strain`, `Von Mises stress`, …) are
//! refused (not ported for this dumper; EMB-005).

use akantu_core::{ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu_io::dumper::{
    DumpBuffer, Dumpable, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshSelection, Padding,
};
use akantu_mesh::Mesh;
use akantu_model::ConstitutiveLawsHandler;
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_solid::Material;

use crate::material::{InterfaceData, is_reinforcement};

/// Name of the dumper on the interface mesh.
pub const REINFORCEMENT_DUMPER: &str = "reinforcement";

/// The `reinforcement` dumper registry (not the default dumper: upstream
/// `dump()` writes the solid model's default dumper only).
#[derive(Debug)]
pub struct ReinforcementDumper {
    /// The dumper registry (one ParaView dumper named `reinforcement`,
    /// file base name = model id).
    pub dumpable: Dumpable,
}

fn is_internal(materials: &ConstitutiveLawsHandler<dyn Material>, name: &str) -> bool {
    materials.laws().iter().any(|l| {
        let mut found = false;
        if is_reinforcement(l.as_ref()) {
            l.for_each_internal(&mut |n, _| found |= n == name);
        }
        found
    })
}

impl ReinforcementDumper {
    /// Upstream `registerDumper<DumperParaview>("reinforcement", id)` and
    /// the interface-mesh registration.
    ///
    /// # Errors
    /// Dumper registration errors.
    pub fn new(id: &str) -> Result<Self> {
        let mut dumpable = Dumpable::new();
        dumpable.register_paraview(REINFORCEMENT_DUMPER, Some(id), false);
        dumpable.add_dump_mesh_to_dumper(
            REINFORCEMENT_DUMPER,
            MeshSelection::new(Some(1), GhostType::NotGhost, ElementKind::Regular),
        )?;
        Ok(Self { dumpable })
    }

    /// Field descriptor of `name` for the reinforcement dumper (upstream
    /// `createElementalField(name, group, padding, 1, _ek_regular)`);
    /// `None` for a name that is neither `partitions`, `material_index`, an
    /// internal nor one of the fields upstream derives from `grad_u` /
    /// `stress`: upstream's `SolidMechanicsModel::createElementalField`
    /// returns no field for it, and the override
    /// (`embedded_interface_model.cc:131-148`) then adds nothing (there is
    /// no mesh-data fallback on this dumper, unlike the generic
    /// `Model::addDumpGroupFieldToDumper`), e.g. for the example's
    /// `stress_embedded`.
    ///
    /// # Errors
    /// `"force"` / `"residual"` renamed fields; nodal fields (upstream
    /// creates no field for them on this dumper); the derived fields
    /// (`strain`, `Green strain`, `principal strain`, `principal Green
    /// strain`, `Von Mises stress`: upstream computes them from the law's
    /// `grad_u` / `stress`), not ported for this dumper.
    pub fn descriptor(
        name: &str,
        group: &str,
        padding: bool,
        materials: &ConstitutiveLawsHandler<dyn Material>,
    ) -> Result<Option<FieldDescriptor>> {
        let group = (group != "all").then(|| group.to_owned());
        // upstream createElementalField(…, spatial_dimension = 1, …) applies
        // its stress/strain padders only for spatial_dimension == 2, and
        // internals have no generic padder: the flag has no effect here
        let _ = padding;
        let d = |source, support, value_type, homogenize, padding| FieldDescriptor {
            name: name.to_owned(),
            source,
            support,
            value_type,
            group: group.clone(),
            padding,
            homogenize,
        };
        match name {
            "force" | "residual" | "displacement" | "velocity" | "acceleration" | "mass"
            | "external_force" | "internal_force" | "increment" | "blocked_dofs" => {
                Err(Error::invalid(format!(
                    "'{name}' is a nodal field of the background mesh: the reinforcement dumper \
                     only takes elemental fields of the interface mesh"
                )))
            }
            "strain"
            | "Green strain"
            | "principal strain"
            | "principal Green strain"
            | "Von Mises stress" => Err(Error::Unsupported(format!(
                "'{name}' (computed from the reinforcement law's grad_u / stress) is not \
                 available on the reinforcement dumper"
            ))),
            "partitions" => Ok(Some(d(
                FieldSource::Partition,
                FieldSupport::Elemental,
                FieldValueType::Int,
                false,
                Padding::None,
            ))),
            "material_index" => Ok(Some(d(
                FieldSource::ModelElemental("material_index".into()),
                FieldSupport::Elemental,
                FieldValueType::Int,
                false,
                Padding::None,
            ))),
            n if is_internal(materials, n) => Ok(Some(d(
                FieldSource::Internal {
                    law: None,
                    name: n.to_owned(),
                },
                FieldSupport::QuadraturePoint,
                FieldValueType::Real,
                true,
                Padding::None,
            ))),
            _ => Ok(None),
        }
    }

    /// Register the field `name` (see [`ReinforcementDumper::descriptor`]):
    /// nothing is registered for a name upstream creates no field for.
    ///
    /// # Errors
    /// See [`ReinforcementDumper::descriptor`]; duplicate field.
    pub fn add_field(
        &mut self,
        name: &str,
        group: &str,
        padding: bool,
        materials: &ConstitutiveLawsHandler<dyn Material>,
    ) -> Result<()> {
        match Self::descriptor(name, group, padding, materials)? {
            Some(d) => self
                .dumpable
                .add_dump_field_to_dumper(REINFORCEMENT_DUMPER, d),
            None => Ok(()),
        }
    }

    /// Write one step of the interface mesh.
    ///
    /// # Errors
    /// Dump errors.
    pub fn dump(
        &mut self,
        iface: &InterfaceData,
        materials: &ConstitutiveLawsHandler<dyn Material>,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        let r = Resolver { materials, iface };
        self.dumpable
            .dump_named(REINFORCEMENT_DUMPER, &iface.mesh, &r, time, step)
    }
}

/// Live values of the reinforcement laws on the interface mesh.
struct Resolver<'a> {
    materials: &'a ConstitutiveLawsHandler<dyn Material>,
    iface: &'a InterfaceData,
}

impl Resolver<'_> {
    /// Upstream `flattenInternal(name)` restricted to the interface mesh:
    /// every element gets its law's values (`nb_quad × nb_component`),
    /// zeros when its law has no such internal or it has no law.
    fn internal(
        &self,
        mesh: &Mesh,
        law_name: Option<&str>,
        name: &str,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        let nq = self.iface.fe.nb_integration_points(ty, ghost).unwrap_or(0);
        let ne = mesh.nb_element(ty, ghost);
        let mut nbc = None;
        let mut blocks = Vec::new();
        for law in self.materials.laws() {
            if !is_reinforcement(law.as_ref()) || law_name.is_some_and(|n| n != law.base().name) {
                continue;
            }
            let Ok(filter) = law.base().element_filter.get(ty, ghost) else {
                continue;
            };
            let mut found = None;
            law.for_each_internal(&mut |n, f| {
                if n == name
                    && found.is_none()
                    && let InternalFieldRef::Real(f) = f
                    && let Ok(a) = f.get(ty, ghost)
                {
                    found = Some((f.nb_component(), a.as_slice().to_vec()));
                }
            });
            if let Some((c, v)) = found {
                nbc = Some(c);
                blocks.push((filter.as_slice().to_vec(), v));
            }
        }
        let Some(c) = nbc else {
            return Err(Error::not_found("reinforcement internal field", name));
        };
        let mut values = vec![0.0; ne * nq * c];
        for (filter, v) in blocks {
            for (local, &el) in filter.iter().enumerate() {
                let e = akantu_core::try_to_usize(el)?;
                let src = v
                    .get(local * nq * c..(local + 1) * nq * c)
                    .ok_or(Error::OutOfRange {
                        what: "reinforcement internal block",
                        index: local,
                        len: v.len() / (nq * c).max(1),
                    })?;
                let dst =
                    values
                        .get_mut(e * nq * c..(e + 1) * nq * c)
                        .ok_or(Error::OutOfRange {
                            what: "interface element",
                            index: e,
                            len: ne,
                        })?;
                dst.copy_from_slice(src);
            }
        }
        Ok(DumpBuffer::Real {
            nb_component: c,
            values,
        })
    }

    /// Law index of every interface element of `(ty, ghost)` (`-1` if
    /// none).
    fn material_index(&self, mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<DumpBuffer> {
        let ne = mesh.nb_element(ty, ghost);
        let mut values: Vec<akantu_core::Int> = vec![-1; ne];
        for (i, law) in self.materials.laws().iter().enumerate() {
            if !is_reinforcement(law.as_ref()) {
                continue;
            }
            let Ok(filter) = law.base().element_filter.get(ty, ghost) else {
                continue;
            };
            for &el in filter.as_slice() {
                let e = akantu_core::try_to_usize(el)?;
                if let Some(v) = values.get_mut(e) {
                    *v = akantu_core::try_idx(i)?;
                }
            }
        }
        Ok(DumpBuffer::Int {
            nb_component: 1,
            values,
        })
    }
}

impl FieldResolver for Resolver<'_> {
    fn resolve(
        &self,
        mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer> {
        match (&field.source, ty) {
            (FieldSource::Internal { law, name }, Some(t)) => {
                self.internal(mesh, law.as_deref(), name, t, ghost)
            }
            (FieldSource::ModelElemental(n), Some(t)) if n == "material_index" => {
                self.material_index(mesh, t, ghost)
            }
            _ => Err(Error::not_found(
                "reinforcement dump field",
                field.name.clone(),
            )),
        }
    }
}
