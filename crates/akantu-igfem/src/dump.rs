// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/dumper_igfem_connectivity.hh,
// dumper_igfem_element_iterator.hh, dumper_igfem_elemental_field.hh,
// dumper_igfem_generic_elemental_field.hh, dumper_igfem_quadrature_points_field.hh,
// dumper_igfem_element_partition.hh, dumper_igfem_material_internal_field.hh
// (the splitting of IGFEM elements into their sub-elements for output), and
// solid_mechanics_model_igfem.cc (the "igfem elements" dumper:
// constructor, addDumpGroupFieldToDumper, onDump, createElementalField,
// createNodalFieldReal).

//! Output of IGFEM elements (IGFEM-010): upstream's IOHelper adaptors
//! write every IGFEM element as its two sub-elements (`_triangle_3` /
//! `_quadrangle_4` / `_segment_2` cells) and every elemental or
//! quadrature-point field per sub-element. This module builds that split
//! mesh ([`sub_element_mesh`]) and the per-sub-element values
//! ([`SubElementMesh::expand_elemental`], [`SubElementMesh::average_on_sub_elements`]),
//! and the model-level dumper [`IGFEM_DUMPER`] (`"igfem elements"`).
//!
//! # The `"igfem elements"` dumper
//!
//! Registered by [`crate::SolidMechanicsModelIgfem::new`] as upstream (a
//! ParaView dumper whose file base name is the model id, beside the solid
//! model's default dumper). A dump of it writes the **sub-element mesh** of
//! the IGFEM elements of the model dimension (not ghost; the model's nodes)
//! with, per field (upstream `createElementalField` with `_ek_igfem`,
//! `createNodalFieldReal`):
//!
//! * nodal fields of the solid model (`displacement`, `blocked_dofs`,
//!   `external_force`, …) and `real_displacement` (the IGFEM
//!   interpolation at the enriched nodes, computed by upstream's `onDump`,
//!   `computeValuesOnEnrichedNodes`);
//! * `material_index` and `partitions`: the IGFEM element's value on both
//!   sub-elements (`IGFEMElementalField`, `IGFEMElementPartitionField`);
//! * IGFEM law internals (`lambda`, `stress`, `grad_u`, `damage`, …) and
//!   the quantities derived from them (`strain`, `Von Mises stress`, …):
//!   the points of each sub-element (`IGFEMInternalMaterialField`, the
//!   sub-element-0 points first in the element), derived per point, then
//!   averaged per sub-element (upstream's homogeniser).
//!
//! Through the IGFEM model ([`crate::SolidMechanicsModelIgfem::dump_named`])
//! every field is available; a dump through the solid model alone
//! (`model.smm.dump_named("igfem elements", …)`, the route installed by the
//! IGFEM model) writes the same mesh and fields but cannot evaluate
//! `real_displacement` (it lives in the IGFEM model) and refuses it.
//!
//! Upstream defects (D-003): UD-1207 (C): upstream interleaves the cells
//! (element by element, sub-element 0 then 1, across the IGFEM types); the
//! port writes one block per sub-element type (VTK cells of each type in
//! the elements' order) — the same cells and values per cell, another cell
//! order. UD-1208 (A): the 2-D stress / strain padders of upstream read
//! the Poisson ratio of the element's law (`getParam("nu")`), which the
//! IGFEM laws do not have (upstream throws at the dump); padded `stress` /
//! `strain` are refused on this dumper at registration; names that are no
//! IGFEM internal fall through to mesh data upstream (an empty array of the
//! IGFEM kind, the next dump raises) and are refused at registration too.

use akantu_core::{
    Array, Element, ElementKind, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter,
};
use akantu_io::dumper::{
    DumpBuffer, FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType,
    MeshSelection, Padding,
};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::{DOF_ID, SolidMechanicsPhysics};
use akantu_solver::DofManager;

use crate::element;
use crate::material::as_igfem;

/// Name of the IGFEM-element dumper (upstream `"igfem elements"`).
pub const IGFEM_DUMPER: &str = "igfem elements";

/// The nodal field upstream's `createNodalFieldReal` override adds on
/// every dumper.
pub const REAL_DISPLACEMENT: &str = "real_displacement";

/// The IGFEM elements of a mesh split into sub-elements.
#[derive(Debug)]
pub struct SubElementMesh {
    /// Mesh with the same nodes and one cell per sub-element.
    pub mesh: Mesh,
    /// For every block `(sub-element type)` of `mesh`, the IGFEM element and
    /// sub-element index of each cell, in cell order.
    pub origin: Vec<(ElementType, Vec<(Element, usize)>)>,
}

/// Split the IGFEM elements of `(mesh, ghost)` (mesh dimension) into
/// sub-elements (upstream `IGFEMConnectivityField` iteration order: element
/// types in enum order, elements in order, sub-element 0 then 1; cells of
/// each sub-element type are appended to their block in that order).
///
/// # Errors
/// Malformed mesh.
pub fn sub_element_mesh(mesh: &Mesh, ghost: GhostType) -> Result<SubElementMesh> {
    let dim = mesh.spatial_dimension();
    let mut out = Mesh::new(dim, format!("{}:igfem_sub_elements", mesh.id()))?;
    let mut origin: Vec<(ElementType, Vec<(Element, usize)>)> = Vec::new();
    let mut cells: Vec<(ElementType, Vec<Idx>)> = Vec::new();
    let types: Vec<ElementType> = mesh
        .element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: ghost,
            kind: Some(ElementKind::Igfem),
        })
        .collect();
    for ty in types {
        let conn = mesh.connectivity(ty, ghost)?;
        for (e, row) in conn.tuples().enumerate() {
            let el = Element::try_new(ty, e, ghost)?;
            for s in 0..element::NB_SUB_ELEMENTS {
                let sub_ty = element::sub_element_type(ty, s)?;
                let local = element::sub_element_connectivity(ty, s)?;
                let c: Vec<Idx> = local
                    .iter()
                    .map(|&k| {
                        row.get(usize::from(k)).copied().ok_or(Error::OutOfRange {
                            what: "IGFEM sub-element connectivity",
                            index: usize::from(k),
                            len: row.len(),
                        })
                    })
                    .collect::<Result<_>>()?;
                cells.push((sub_ty, c));
                match origin.iter_mut().find(|(t, _)| *t == sub_ty) {
                    Some((_, v)) => v.push((el, s)),
                    None => origin.push((sub_ty, vec![(el, s)])),
                }
            }
        }
    }
    {
        let mut acc = MeshAccessor::new(&mut out);
        for x in mesh.nodes().tuples() {
            acc.push_node(x)?;
        }
        for (ty, c) in &cells {
            acc.connectivity_mut(*ty, GhostType::NotGhost)
                .try_push_tuple(c)?;
        }
    }
    origin.sort_by_key(|(t, _)| t.index());
    Ok(SubElementMesh { mesh: out, origin })
}

impl SubElementMesh {
    /// Cells of the sub-element type `sub_ty`, in order.
    #[must_use]
    pub fn cells(&self, sub_ty: ElementType) -> &[(Element, usize)] {
        self.origin
            .iter()
            .find(|(t, _)| *t == sub_ty)
            .map_or(&[], |(_, v)| v.as_slice())
    }

    /// An elemental field of the IGFEM elements (one tuple of
    /// `nb_component` per element of `value(element)`) repeated on both
    /// sub-elements (upstream `IGFEMElementalField`, e.g. `material_index`).
    ///
    /// # Errors
    /// `value` errors or a wrong tuple size.
    pub fn expand_elemental(
        &self,
        sub_ty: ElementType,
        nb_component: usize,
        value: &dyn Fn(Element) -> Result<Vec<Real>>,
    ) -> Result<Array<Real>> {
        let cells = self.cells(sub_ty);
        let mut out = Array::try_zeros(cells.len(), nb_component)?;
        for (i, (el, _)) in cells.iter().enumerate() {
            let v = value(*el)?;
            if v.len() != nb_component {
                return Err(Error::DimensionMismatch {
                    context: "IGFEM expand_elemental: tuple size",
                    expected: nb_component,
                    got: v.len(),
                });
            }
            out.tuple_mut(i).copy_from_slice(&v);
        }
        Ok(out)
    }

    /// The average over each sub-element of a quadrature-point field of the
    /// IGFEM elements (`values(element)`: the element's `nq` tuples, the
    /// sub-element-0 points first; upstream homogenisation of
    /// `IGFEMInternalMaterialField`/`IGFEMQuadraturePointsField` per
    /// sub-element).
    ///
    /// # Errors
    /// `values` errors or wrong sizes.
    pub fn average_on_sub_elements(
        &self,
        sub_ty: ElementType,
        nb_component: usize,
        values: &dyn Fn(Element) -> Result<Vec<Real>>,
    ) -> Result<Array<Real>> {
        let cells = self.cells(sub_ty);
        let mut out = Array::try_zeros(cells.len(), nb_component)?;
        for (i, (el, s)) in cells.iter().enumerate() {
            let ty = el.ty();
            let n0 =
                akantu_fe::quadrature::default_rule(element::sub_element_type(ty, 0)?)?.nb_points();
            let n1 =
                akantu_fe::quadrature::default_rule(element::sub_element_type(ty, 1)?)?.nb_points();
            let v = values(*el)?;
            if v.len() != (n0 + n1) * nb_component {
                return Err(Error::DimensionMismatch {
                    context: "IGFEM average_on_sub_elements: values per element",
                    expected: (n0 + n1) * nb_component,
                    got: v.len(),
                });
            }
            let range = if *s == 0 { 0..n0 } else { n0..n0 + n1 };
            #[allow(clippy::cast_precision_loss)] // at most 4 points
            let n = range.len() as Real;
            let t = out.tuple_mut(i);
            for q in range {
                for c in 0..nb_component {
                    t[c] += v[q * nb_component + c];
                }
            }
            for x in t.iter_mut() {
                *x /= n;
            }
        }
        Ok(out)
    }
}

/// Register [`IGFEM_DUMPER`] on the solid model (upstream constructor:
/// `registerDumper<DumperParaview>("igfem elements", id)`,
/// `addDumpMeshToDumper(…, mesh, dim, _not_ghost, _ek_igfem)`; the port
/// writes the sub-element mesh, whose cells are regular types).
///
/// # Errors
/// Registry errors.
pub(crate) fn register_igfem_dumper(
    smm: &mut SolidMechanicsModel,
    id: &str,
    dim: usize,
) -> Result<()> {
    let d = &mut smm.dumpers.dumpable;
    d.register_paraview(IGFEM_DUMPER, Some(id), false);
    d.add_dump_mesh_to_dumper(
        IGFEM_DUMPER,
        MeshSelection::new(Some(dim), GhostType::NotGhost, ElementKind::Regular),
    )
}

/// `true` if an IGFEM law of `physics` has the internal `name` (upstream
/// `isInternal(name, _ek_igfem)`).
fn is_igfem_internal(physics: &SolidMechanicsPhysics, name: &str) -> bool {
    physics.materials.laws().iter().any(|l| {
        as_igfem(l.as_ref()).is_some() && {
            let mut found = false;
            l.for_each_internal(&mut |n, _| found |= n == name);
            found
        }
    })
}

/// The descriptor of `real_displacement`.
fn real_displacement_descriptor(group: Option<&str>, padding: bool) -> FieldDescriptor {
    FieldDescriptor {
        name: REAL_DISPLACEMENT.into(),
        source: FieldSource::ModelNodal(REAL_DISPLACEMENT.into()),
        support: FieldSupport::Nodal,
        value_type: FieldValueType::Real,
        group: group.filter(|g| *g != "all").map(str::to_owned),
        padding: if padding {
            Padding::To3D
        } else {
            Padding::None
        },
        homogenize: false,
    }
}

/// Upstream `SolidMechanicsModelIGFEM::addDumpGroupFieldToDumper` for the
/// IGFEM dumper (element kind `_ek_igfem`): the field descriptor of `name`.
///
/// # Errors
/// An element group other than `"all"`, a name that is neither a nodal
/// field, `material_index`, `partitions` nor an IGFEM internal (or a
/// quantity derived from one), a padded stress / strain (UD-1208).
pub(crate) fn igfem_descriptor(
    physics: &SolidMechanicsPhysics,
    name: &str,
    group: &str,
    padding: bool,
) -> Result<FieldDescriptor> {
    if group != "all" {
        return Err(Error::Unsupported(format!(
            "the '{IGFEM_DUMPER}' dumper writes every IGFEM element: element group '{group}' is \
             not supported"
        )));
    }
    if name == REAL_DISPLACEMENT {
        return Ok(real_displacement_descriptor(None, padding));
    }
    let d = akantu_io::dumper::solid_mechanics_field(name, None, padding, |n| {
        is_igfem_internal(physics, n)
    })?;
    if matches!(d.source, FieldSource::Mesh(_)) {
        return Err(Error::InvalidArgument(format!(
            "'{name}' is not an internal of an IGFEM law, so it is not a field of the \
             '{IGFEM_DUMPER}' dumper (upstream falls through to mesh data of the IGFEM kind and \
             its next dump raises, UD-1208)"
        )));
    }
    let padded_tensor = padding
        && match &d.source {
            FieldSource::Internal { name, .. } => name == "stress",
            FieldSource::Derived { quantity, .. } => matches!(
                quantity,
                akantu_io::dumper::DerivedQuantity::SmallStrain
                    | akantu_io::dumper::DerivedQuantity::GreenStrain
            ),
            _ => false,
        };
    if padded_tensor && physics.base.spatial_dimension == 2 {
        return Err(Error::InvalidArgument(format!(
            "padded '{name}' on the '{IGFEM_DUMPER}' dumper: upstream's 2-D padder reads the \
             Poisson ratio of the element's law, which the IGFEM laws do not have (UD-1208)"
        )));
    }
    Ok(d)
}

/// The descriptor of `name` on a dumper of the solid model other than
/// [`IGFEM_DUMPER`] (upstream `createNodalFieldReal` override: also
/// `real_displacement`).
///
/// # Errors
/// See [`SolidMechanicsModel::dump_field_descriptor`].
pub(crate) fn regular_descriptor(
    smm: &SolidMechanicsModel,
    name: &str,
    group: &str,
    padding: bool,
) -> Result<FieldDescriptor> {
    let g = Some(group).filter(|g| *g != "all");
    if name == REAL_DISPLACEMENT {
        return Ok(real_displacement_descriptor(g, padding));
    }
    smm.dump_field_descriptor(name, g, padding)
}

/// `(nb_component, values at the points of one sub-element)`.
type CellValues = (usize, Vec<Real>);

/// Live state for one dump of [`IGFEM_DUMPER`] (the sub-element mesh).
struct IgfemResolver<'a> {
    physics: &'a SolidMechanicsPhysics,
    dofs: &'a DofManager,
    sub: &'a SubElementMesh,
    real_displacement: Option<&'a Array<Real>>,
}

/// The `real_displacement` nodal field alone (for the regular dumpers).
pub(crate) struct RealDisplacementResolver<'a>(pub(crate) &'a Array<Real>);

impl FieldResolver for RealDisplacementResolver<'_> {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        match (&field.source, ty) {
            (FieldSource::ModelNodal(n), None) if n == REAL_DISPLACEMENT => Ok(DumpBuffer::Real {
                nb_component: self.0.nb_component(),
                values: self.0.as_slice().to_vec(),
            }),
            _ => Err(Error::not_found("IGFEM dump field", field.name.clone())),
        }
    }
}

/// `true` for the fields [`RealDisplacementResolver`] resolves.
pub(crate) fn is_real_displacement(d: &FieldDescriptor) -> bool {
    matches!(&d.source, FieldSource::ModelNodal(n) if n == REAL_DISPLACEMENT)
}

/// Points of sub-element `s` of an IGFEM element of type `ty` in the
/// element's point order (sub-element 0 first).
fn sub_element_points(ty: ElementType, s: usize) -> Result<std::ops::Range<usize>> {
    let n0 = akantu_fe::quadrature::default_rule(element::sub_element_type(ty, 0)?)?.nb_points();
    let n1 = akantu_fe::quadrature::default_rule(element::sub_element_type(ty, 1)?)?.nb_points();
    Ok(if s == 0 { 0..n0 } else { n0..n0 + n1 })
}

impl IgfemResolver<'_> {
    fn nodal_real(&self, name: &str) -> Result<DumpBuffer> {
        let p = self.physics;
        let a = match name {
            REAL_DISPLACEMENT => {
                let r = self.real_displacement.ok_or_else(|| {
                    Error::Unsupported(format!(
                        "'{REAL_DISPLACEMENT}' lives in the IGFEM model: dump the \
                         '{IGFEM_DUMPER}' dumper through SolidMechanicsModelIgfem::dump_named"
                    ))
                })?;
                return Ok(DumpBuffer::Real {
                    nb_component: r.nb_component(),
                    values: r.as_slice().to_vec(),
                });
            }
            "external_force" => p.external_force()?,
            "internal_force" => p.internal_force()?,
            "mass" => p.mass()?,
            "increment" => self
                .dofs
                .family(DOF_ID)?
                .increment
                .as_ref()
                .ok_or_else(|| Error::not_found("nodal dump field (not allocated)", name))?
                .read()?,
            _ => return Err(Error::not_found("nodal dump field", name)),
        };
        Ok(DumpBuffer::Real {
            nb_component: a.nb_component(),
            values: a.as_slice().to_vec(),
        })
    }

    /// Upstream `IGFEMInternalMaterialField` of `name` on the cells of the
    /// sub-element type `sub_ty`: per cell, the values at the points of its
    /// sub-element from the law holding the IGFEM element (zeros when the
    /// law has no such internal, upstream `flattenInternal`).
    fn internal(
        &self,
        law_name: Option<&str>,
        name: &str,
        sub_ty: ElementType,
    ) -> Result<DumpBuffer> {
        let cells = self.sub.cells(sub_ty);
        let law_map = self.physics.materials.law_index_map();
        let local_map = self.physics.materials.local_numbering_map();
        // per cell: (points of its sub-element, the law's values there)
        let mut per_cell: Vec<(usize, Option<CellValues>)> = Vec::with_capacity(cells.len());
        for &(el, s) in cells {
            let pts = sub_element_points(el.ty(), s)?;
            let nq = sub_element_points(el.ty(), 1)?.end;
            let law = usize::try_from(law_map.element_ref(el)?[0])
                .map_err(|_| Error::invalid(format!("{el}: no law")))?;
            let local = usize::try_from(local_map.element(el)?[0])
                .map_err(|_| Error::invalid(format!("{el}: no law-local index")))?;
            let l = self.physics.materials.law(law)?;
            let mut got: Option<Result<CellValues>> = None;
            if law_name.is_none_or(|n| n == l.base().name) {
                l.for_each_internal(&mut |n, f| {
                    if n != name || got.is_some() {
                        return;
                    }
                    let InternalFieldRef::Real(f) = f else {
                        return;
                    };
                    let Ok(b) = f.get(el.ty(), el.ghost_type()) else {
                        return;
                    };
                    let c = f.nb_component();
                    let range = (local * nq + pts.start) * c..(local * nq + pts.end) * c;
                    got = Some(
                        b.as_slice()
                            .get(range)
                            .map(|v| (c, v.to_vec()))
                            .ok_or_else(|| Error::invalid(format!("{el}: {name} block too short"))),
                    );
                });
            }
            per_cell.push((pts.len(), got.transpose()?));
        }
        let c = match per_cell
            .iter()
            .find_map(|(_, g)| g.as_ref().map(|(c, _)| *c))
        {
            Some(c) => c,
            None if cells.is_empty() => 1,
            None => return Err(Error::not_found("IGFEM internal field", name)),
        };
        let mut values = Vec::new();
        for (n, got) in per_cell {
            match got {
                Some((cc, v)) if cc == c => values.extend(v),
                Some((cc, _)) => {
                    return Err(Error::DimensionMismatch {
                        context: "IGFEM dump: components of an internal across laws",
                        expected: c,
                        got: cc,
                    });
                }
                // upstream flattenInternal: zeros where the law lacks it
                None => values.extend(std::iter::repeat_n(0.0, n * c)),
            }
        }
        Ok(DumpBuffer::Real {
            nb_component: c,
            values,
        })
    }
}

impl FieldResolver for IgfemResolver<'_> {
    fn resolve(
        &self,
        _mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        _ghost: GhostType,
    ) -> Result<DumpBuffer> {
        match (&field.source, ty) {
            (FieldSource::Dof { family, order }, None) => {
                let a = if *order == 0 {
                    self.dofs.dofs(family)?
                } else {
                    self.dofs.dofs_derivative(family, *order)?
                };
                Ok(DumpBuffer::Real {
                    nb_component: a.nb_component(),
                    values: a.as_slice().to_vec(),
                })
            }
            (FieldSource::Blocked { family }, None) => {
                let b = self.dofs.blocked_dofs(family)?;
                Ok(DumpBuffer::Bool {
                    nb_component: b.nb_component(),
                    values: b.as_slice().to_vec(),
                })
            }
            (FieldSource::ModelNodal(n), None) => self.nodal_real(n),
            (FieldSource::ModelElemental(n), Some(t)) if n == "material_index" => {
                let law_map = self.physics.materials.law_index_map();
                let mut values = Vec::new();
                for &(el, _) in self.sub.cells(t) {
                    values.push(law_map.element_ref(el)?[0]);
                }
                Ok(DumpBuffer::Int {
                    nb_component: 1,
                    values,
                })
            }
            (FieldSource::Internal { law, name }, Some(t)) => {
                self.internal(law.as_deref(), name, t)
            }
            _ => Err(Error::not_found("IGFEM dump field", field.name.clone())),
        }
    }
}

/// Dump [`IGFEM_DUMPER`]: upstream `onDump` (the solid model's
/// `BeforeDumpEvent`; the IGFEM model computed `real_displacement` before
/// calling), then the sub-element mesh with its fields.
///
/// # Errors
/// Event-handler, mesh and resolver errors; `real_displacement` without
/// the IGFEM model.
pub fn dump_igfem_elements(
    smm: &mut SolidMechanicsModel,
    real_displacement: Option<&Array<Real>>,
    time: Option<Real>,
    step: Option<usize>,
) -> Result<()> {
    smm.on_dump()?;
    let mesh_h = smm.physics.base.mesh.clone();
    let sub = {
        let m = mesh_h.read()?;
        sub_element_mesh(&m, GhostType::NotGhost)?
    };
    let r = IgfemResolver {
        physics: &smm.physics,
        dofs: &smm.solver.dof_manager,
        sub: &sub,
        real_displacement,
    };
    smm.dumpers
        .dumpable
        .dump_named(IGFEM_DUMPER, &sub.mesh, &r, time, step)
}

/// The IGFEM model's [`akantu_solid::model::DumpRoute`]: a dump of
/// [`IGFEM_DUMPER`] through the solid model writes the sub-element mesh
/// (without `real_displacement`); other dumpers go back to the solid model.
pub(crate) fn route(
    smm: &mut SolidMechanicsModel,
    dumper: &str,
    time: Option<Real>,
    step: Option<usize>,
) -> Option<Result<()>> {
    (dumper == IGFEM_DUMPER).then(|| dump_igfem_elements(smm, None, time, step))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_triangle_5() {
        let mut m = Mesh::new(2, "m").unwrap();
        {
            let mut acc = MeshAccessor::new(&mut m);
            for p in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.5, 0.0], [0.0, 0.5]] {
                acc.push_node(&p).unwrap();
            }
            acc.connectivity_mut(ElementType::IgfemTriangle5, GhostType::NotGhost)
                .push_tuple(&[0, 1, 2, 3, 4]);
        }
        let s = sub_element_mesh(&m, GhostType::NotGhost).unwrap();
        assert_eq!(
            s.mesh
                .connectivity(ElementType::Triangle3, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
            &[0, 3, 4]
        );
        assert_eq!(
            s.mesh
                .connectivity(ElementType::Quadrangle4, GhostType::NotGhost)
                .unwrap()
                .as_slice(),
            &[3, 1, 2, 4]
        );
        let el = Element::try_new(ElementType::IgfemTriangle5, 0, GhostType::NotGhost).unwrap();
        assert_eq!(s.cells(ElementType::Quadrangle4), &[(el, 1)]);
        let avg = s
            .average_on_sub_elements(ElementType::Quadrangle4, 1, &|_| {
                Ok(vec![10.0, 1.0, 2.0, 3.0, 6.0])
            })
            .unwrap();
        assert_eq!(avg.as_slice(), &[3.0]);
        let e = s
            .expand_elemental(ElementType::Triangle3, 1, &|_| Ok(vec![7.0]))
            .unwrap();
        assert_eq!(e.as_slice(), &[7.0]);
    }
}
