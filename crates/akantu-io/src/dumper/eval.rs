// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the iteration semantics of
// src/io/dumper/{dumper_nodal_field.hh, dumper_elemental_field.hh,
// dumper_generic_elemental_field.hh, dumper_generic_elemental_field_tmpl.hh,
// dumper_element_iterator.hh, dumper_internal_material_field.hh,
// dumper_quadrature_point_iterator.hh, dumper_element_partition.hh,
// dumper_filtered_connectivity.hh, dumper_compute.hh,
// dumper_homogenizing_field.hh}, src/mesh/group_manager_inline_impl.hh
// (createNodalField, createStridedNodalField, createElementalField),
// src/mesh/mesh.cc (getNbDataPerElem, createFieldFromAttachedData) and
// src/model/solid_mechanics/solid_mechanics_model_io.cc
// (createElementalField pipeline: compute → padder → homogeniser).

//! Evaluation of field descriptors into the tuples a writer emits.
//!
//! Upstream composes `dumpers::Field` objects (nodal/elemental/quadrature
//! fields, filters, compute functors, padders, homogenisers) that iterate
//! over model arrays referenced at registration. Here the same pipeline is
//! applied at dump time to the transient buffers returned by the
//! [`FieldResolver`]: select the mesh part (group filters), extract the
//! tuples, compute derived quantities, pad, homogenise.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use akantu_core::{
    ElementKind, ElementType, Error, GhostType, Idx, Int, NodeFlag, Rank, Real, Result, TypeFilter,
    UInt,
};
use akantu_mesh::Mesh;
use akantu_mesh::data::{MeshDataValues, NodalDataValues};

use super::derived::{self, StrainMeasure};
use super::{
    DerivedQuantity, DumpBuffer, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshFilter, MeshSelection, Padding,
};

/// Scalar type of evaluated values (and of the VTK/text data type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scalar {
    /// `double` → `Float64` / `float64`.
    F64,
    /// `int` → `Int32` / `int32`.
    I32,
    /// `unsigned int` → `UInt32` / `uint32`.
    U32,
    /// `bool` → `UInt8` / `bool`.
    Bool,
}

impl Scalar {
    /// ParaView `dataTypeToStr`.
    pub(crate) fn vtk_name(self) -> &'static str {
        match self {
            Scalar::F64 => "Float64",
            Scalar::I32 => "Int32",
            Scalar::U32 => "UInt32",
            Scalar::Bool => "UInt8",
        }
    }

    /// IOHelper `operator<<(DataType)` (text dumper `.fields`).
    pub(crate) fn text_name(self) -> &'static str {
        match self {
            Scalar::F64 => "float64",
            Scalar::I32 => "int32",
            Scalar::U32 => "uint32",
            Scalar::Bool => "bool",
        }
    }
}

/// Flat values of one block.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Values {
    F64(Vec<Real>),
    I32(Vec<Int>),
    U32(Vec<UInt>),
    Bool(Vec<bool>),
}

impl Values {
    fn empty(s: Scalar) -> Self {
        match s {
            Scalar::F64 => Values::F64(Vec::new()),
            Scalar::I32 => Values::I32(Vec::new()),
            Scalar::U32 => Values::U32(Vec::new()),
            Scalar::Bool => Values::Bool(Vec::new()),
        }
    }

    pub(crate) fn scalar(&self) -> Scalar {
        match self {
            Values::F64(_) => Scalar::F64,
            Values::I32(_) => Scalar::I32,
            Values::U32(_) => Scalar::U32,
            Values::Bool(_) => Scalar::Bool,
        }
    }

    fn len(&self) -> usize {
        match self {
            Values::F64(v) => v.len(),
            Values::I32(v) => v.len(),
            Values::U32(v) => v.len(),
            Values::Bool(v) => v.len(),
        }
    }

    /// Append `src[range]` (same scalar type).
    fn extend_from(&mut self, src: &Values, range: std::ops::Range<usize>) {
        match (self, src) {
            (Values::F64(d), Values::F64(s)) => d.extend_from_slice(&s[range]),
            (Values::I32(d), Values::I32(s)) => d.extend_from_slice(&s[range]),
            (Values::U32(d), Values::U32(s)) => d.extend_from_slice(&s[range]),
            (Values::Bool(d), Values::Bool(s)) => d.extend_from_slice(&s[range]),
            _ => debug_assert!(false, "scalar mismatch"),
        }
    }
}

/// One element type (or the nodes) of an evaluated field.
#[derive(Clone, Debug)]
pub(crate) struct Block {
    /// Element type of the tuples (`None` for nodal data).
    pub(crate) ty: Option<ElementType>,
    /// Number of tuples.
    pub(crate) nb_tuples: usize,
    /// Values per tuple.
    pub(crate) tuple_len: usize,
    /// `0`: vector tuples; otherwise matrix tuples of `rows` rows,
    /// column-major (upstream `Matrix<Real>` return types).
    pub(crate) rows: usize,
    /// `nb_tuples × tuple_len` values.
    pub(crate) values: Values,
}

/// A field ready to be written.
#[derive(Clone, Debug)]
pub(crate) struct Evaluated {
    pub(crate) scalar: Scalar,
    /// Upstream `getDim()` (`NumberOfComponents`, text `nb_components`).
    pub(crate) dim: usize,
    /// Upstream `isHomogeneous()`.
    pub(crate) homogeneous: bool,
    /// Vector tuples shorter than `dim` are zero-padded (nodal padding).
    pub(crate) padded: bool,
    pub(crate) blocks: Vec<Block>,
}

impl Evaluated {
    /// Total number of tuples.
    pub(crate) fn nb_tuples(&self) -> usize {
        self.blocks.iter().map(|b| b.nb_tuples).sum()
    }
}

// ---------------------------------------------------------------- selection

/// Node set of a mesh selection.
#[derive(Clone, Debug)]
pub(crate) enum NodeSet<'a> {
    /// All nodes `0..n`.
    All(usize),
    /// Listed nodes (filter order).
    List(Cow<'a, [Idx]>),
}

impl NodeSet<'_> {
    pub(crate) fn len(&self) -> usize {
        match self {
            NodeSet::All(n) => *n,
            NodeSet::List(l) => l.len(),
        }
    }

    /// Mesh node of output position `i` (validated at construction).
    fn node(&self, i: usize) -> usize {
        match self {
            NodeSet::All(_) => i,
            NodeSet::List(l) => usize::try_from(l[i]).unwrap_or(0),
        }
    }
}

/// Elements of one type in a mesh selection.
#[derive(Clone, Debug)]
pub(crate) struct ElemSet<'a> {
    pub(crate) ty: ElementType,
    pub(crate) ghost: GhostType,
    /// `None`: all elements `0..nb`; otherwise listed elements.
    ids: Option<Cow<'a, [Idx]>>,
    /// Number of selected elements.
    pub(crate) nb: usize,
}

impl ElemSet<'_> {
    /// Mesh element of output position `i` (validated at construction).
    pub(crate) fn element(&self, i: usize) -> usize {
        match &self.ids {
            None => i,
            Some(l) => usize::try_from(l[i]).unwrap_or(0),
        }
    }
}

/// A resolved mesh part: nodes and per-type elements in output order.
#[derive(Clone, Debug)]
pub(crate) struct Selection<'a> {
    pub(crate) nodes: NodeSet<'a>,
    pub(crate) elems: Vec<ElemSet<'a>>,
}

fn check_nodes<'a>(mesh: &Mesh, list: Cow<'a, [Idx]>) -> Result<NodeSet<'a>> {
    let n = mesh.nb_nodes();
    for &v in list.iter() {
        if usize::try_from(v).map_or(true, |u| u >= n) {
            return Err(Error::InvalidIndex {
                what: "node in a dump filter",
                value: i64::from(v),
            });
        }
    }
    Ok(NodeSet::List(list))
}

fn check_elems(mesh: &Mesh, ty: ElementType, ghost: GhostType, ids: &[Idx]) -> Result<()> {
    let n = mesh.nb_element(ty, ghost);
    for &v in ids {
        if usize::try_from(v).map_or(true, |u| u >= n) {
            return Err(Error::InvalidIndex {
                what: "element in a dump filter",
                value: i64::from(v),
            });
        }
    }
    Ok(())
}

impl MeshSelection {
    fn type_filter(&self) -> TypeFilter {
        TypeFilter {
            spatial_dimension: self.spatial_dimension,
            ghost_type: self.ghost,
            kind: match self.kind {
                None | Some(ElementKind::NotDefined) => None,
                Some(k) => Some(k),
            },
        }
    }

    /// Resolve against the current mesh; `group` (a field's element group)
    /// overrides the registered filter.
    pub(crate) fn resolve<'a>(
        &'a self,
        mesh: &'a Mesh,
        group: Option<&'a str>,
    ) -> Result<Selection<'a>> {
        let tf = self.type_filter();
        let ghost = self.ghost;
        let element_group = |name: &'a str| -> Result<Selection<'a>> {
            let g = mesh.groups().element_group(name)?;
            let nodes = check_nodes(mesh, Cow::Owned(g.nodes().as_slice().to_vec()))?;
            let mut elems = Vec::new();
            for ty in g.elements.element_types(tf) {
                let ids = g.elements_of(ty, ghost);
                check_elems(mesh, ty, ghost, &ids)?;
                elems.push(ElemSet {
                    ty,
                    ghost,
                    nb: ids.len(),
                    ids: Some(Cow::Owned(ids.to_vec())),
                });
            }
            Ok(Selection { nodes, elems })
        };
        if let Some(name) = group {
            return element_group(name);
        }
        match &self.filter {
            MeshFilter::All => Ok(Selection {
                nodes: NodeSet::All(mesh.nb_nodes()),
                elems: mesh
                    .element_types(tf)
                    .map(|ty| ElemSet {
                        ty,
                        ghost,
                        ids: None,
                        nb: mesh.nb_element(ty, ghost),
                    })
                    .collect(),
            }),
            MeshFilter::ElementGroup(name) => element_group(name),
            MeshFilter::NodeGroup(name) => {
                let g = mesh.groups().node_group(name)?;
                Ok(Selection {
                    nodes: check_nodes(mesh, Cow::Owned(g.node_list()))?,
                    elems: Vec::new(),
                })
            }
            MeshFilter::Explicit { elements, nodes } => {
                let nodes = check_nodes(mesh, Cow::Borrowed(nodes))?;
                let mut elems = Vec::new();
                for ty in elements.element_types(tf) {
                    let ids = elements.get(ty, ghost)?.as_slice();
                    check_elems(mesh, ty, ghost, ids)?;
                    elems.push(ElemSet {
                        ty,
                        ghost,
                        nb: ids.len(),
                        ids: Some(Cow::Borrowed(ids)),
                    });
                }
                Ok(Selection { nodes, elems })
            }
        }
    }
}

/// A field restricted to an element group is written over the
/// dumper's mesh part (`written`), taking each value by node/element
/// identity in the written order. Every written node (nodal support) or
/// element (elemental and quadrature support) must therefore belong to
/// the field's group (`field`); otherwise the field is rejected before
/// anything is written. Identical selections take the fast path.
pub(crate) fn check_group_covers(
    name: &str,
    group: &str,
    support: FieldSupport,
    written: &Selection<'_>,
    field: &Selection<'_>,
) -> Result<()> {
    let missing = |what: String| {
        Error::InvalidArgument(format!(
            "dump field '{name}' is restricted to group '{group}', which does not contain {what} written by the dumper (register the field without a group, or on a group containing the dumper's mesh part)"
        ))
    };
    if support == FieldSupport::Nodal {
        let NodeSet::List(have) = &field.nodes else {
            return Ok(()); // a group never selects all nodes implicitly
        };
        if let NodeSet::List(want) = &written.nodes
            && want == have
        {
            return Ok(());
        }
        let have: HashSet<Idx> = have.iter().copied().collect();
        for i in 0..written.nodes.len() {
            let node = written.nodes.node(i);
            let id = akantu_core::try_idx(node)?;
            if !have.contains(&id) {
                return Err(missing(format!("node {node}")));
            }
        }
        return Ok(());
    }
    for w in &written.elems {
        if w.nb == 0 {
            continue;
        }
        let Some(f) = field
            .elems
            .iter()
            .find(|f| f.ty == w.ty && f.ghost == w.ghost)
        else {
            return Err(missing(format!("the {:?} elements", w.ty)));
        };
        let same = match (&w.ids, &f.ids) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        };
        if same {
            continue;
        }
        let have: HashSet<usize> = (0..f.nb).map(|i| f.element(i)).collect();
        for i in 0..w.nb {
            let e = w.element(i);
            if !have.contains(&e) {
                return Err(missing(format!("{:?} element {e}", w.ty)));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- context

/// What a dump needs to evaluate fields.
pub(crate) struct EvalContext<'a> {
    pub(crate) mesh: &'a Mesh,
    pub(crate) resolver: &'a dyn FieldResolver,
    /// Writing rank (value of the `partitions` field, errata E1).
    pub(crate) rank: Rank,
}

fn values_of(buf: DumpBuffer) -> (usize, Values) {
    match buf {
        DumpBuffer::Real {
            nb_component,
            values,
        } => (nb_component, Values::F64(values)),
        DumpBuffer::Int {
            nb_component,
            values,
        } => (nb_component, Values::I32(values)),
        DumpBuffer::UInt {
            nb_component,
            values,
        } => (nb_component, Values::U32(values)),
        DumpBuffer::Bool {
            nb_component,
            values,
        } => (nb_component, Values::Bool(values)),
    }
}

/// Largest component count a dump writes: VTK reads `NumberOfComponents`
/// as an `int`.
pub(crate) const MAX_DUMP_COMPONENTS: usize = i32::MAX as usize;

/// Every array a dump reads has `1 ≤ nb_component ≤ i32::MAX`,
/// checked for every block, empty ones included. On a 0-node mesh or an
/// empty element block the length check (`n × nc == len`) holds for any
/// `nc`, so without this a resolver's `nc = 0` or `usize::MAX` would be
/// written as is (`NumberOfComponents` omitted, or
/// `18446744073709551615`).
fn check_nb_components(desc: &FieldDescriptor, nc: usize) -> Result<()> {
    if nc == 0 || nc > MAX_DUMP_COMPONENTS {
        return Err(Error::InvalidArgument(format!(
            "dump field '{}': {nc} components per tuple (a dumped array has 1 to {MAX_DUMP_COMPONENTS})",
            desc.name
        )));
    }
    Ok(())
}

fn expected_scalar(t: FieldValueType) -> Scalar {
    match t {
        FieldValueType::Real => Scalar::F64,
        FieldValueType::Int => Scalar::I32,
        FieldValueType::UInt => Scalar::U32,
        FieldValueType::Bool => Scalar::Bool,
    }
}

fn check_scalar(desc: &FieldDescriptor, got: Scalar) -> Result<()> {
    let want = expected_scalar(desc.value_type);
    if want != got {
        return Err(Error::InvalidArgument(format!(
            "dump field '{}': declared {:?} values but the resolver returned {got:?}",
            desc.name, desc.value_type
        )));
    }
    Ok(())
}

fn with_source(
    desc: &FieldDescriptor,
    source: FieldSource,
    support: FieldSupport,
) -> FieldDescriptor {
    FieldDescriptor {
        source,
        support,
        ..desc.clone()
    }
}

/// Full nodal array of a source: `(nb_component, values)`, `len = nb_nodes`.
fn nodal_full(
    ctx: &EvalContext<'_>,
    desc: &FieldDescriptor,
    source: &FieldSource,
) -> Result<(usize, Values)> {
    let mesh = ctx.mesh;
    let (nc, values) = match source {
        FieldSource::Mesh(name) => match name.as_str() {
            "positions" | "position" | "coordinates" | "nodes" => (
                mesh.nodes().nb_component(),
                Values::F64(mesh.nodes().as_slice().to_vec()),
            ),
            "nodes_flags" | "nodes_type" => (
                1,
                Values::I32(
                    mesh.nodes_flags()
                        .as_slice()
                        .iter()
                        .map(|f: &NodeFlag| Int::from(f.bits()))
                        .collect(),
                ),
            ),
            other => match mesh.data().nodal(other)? {
                NodalDataValues::Int(a) => (a.nb_component(), Values::I32(a.as_slice().to_vec())),
                NodalDataValues::UInt(a) => (a.nb_component(), Values::U32(a.as_slice().to_vec())),
                NodalDataValues::Real(a) => (a.nb_component(), Values::F64(a.as_slice().to_vec())),
            },
        },
        FieldSource::Components { .. } | FieldSource::Partition | FieldSource::Derived { .. } => {
            return Err(Error::InvalidArgument(format!(
                "dump field '{}': source {source:?} is not a nodal array",
                desc.name
            )));
        }
        _ => {
            let d = with_source(desc, source.clone(), FieldSupport::Nodal);
            let (nc, v) = values_of(ctx.resolver.resolve(mesh, &d, None, GhostType::NotGhost)?);
            check_scalar(desc, v.scalar())?;
            (nc, v)
        }
    };
    let n = mesh.nb_nodes();
    check_nb_components(desc, nc)?;
    if n.checked_mul(nc) != Some(values.len()) {
        return Err(Error::DimensionMismatch {
            context: "nodal dump field (values vs nodes × components)",
            expected: n.saturating_mul(nc),
            got: values.len(),
        });
    }
    Ok((nc, values))
}

/// Evaluate a nodal field over `sel.nodes` (upstream `NodalField`).
fn evaluate_nodal(
    ctx: &EvalContext<'_>,
    desc: &FieldDescriptor,
    sel: &Selection<'_>,
) -> Result<Evaluated> {
    let (source, first, count) = match &desc.source {
        FieldSource::Components {
            source,
            first,
            count,
        } => (source.as_ref(), *first, Some(*count)),
        s => (s, 0, None),
    };
    if count == Some(0) {
        return Err(Error::InvalidArgument(format!(
            "dump field '{}': a component selection needs at least one component",
            desc.name
        )));
    }
    let (nc, full) = nodal_full(ctx, desc, source)?;
    let n = count.unwrap_or(nc.saturating_sub(first));
    if first.checked_add(n).is_none_or(|end| end > nc) {
        return Err(Error::InvalidArgument(format!(
            "dump field '{}': {n} components from component {first} of a {nc}-component array",
            desc.name
        )));
    }
    let nb = sel.nodes.len();
    let mut values = Values::empty(full.scalar());
    for i in 0..nb {
        let node = sel.nodes.node(i);
        values.extend_from(&full, node * nc + first..node * nc + first + n);
    }
    let (dim, padded) = match desc.padding {
        Padding::To3D if n < 3 => (3, true),
        _ => (n, false),
    };
    Ok(Evaluated {
        scalar: full.scalar(),
        dim,
        homogeneous: true,
        padded,
        blocks: vec![Block {
            ty: None,
            nb_tuples: nb,
            tuple_len: n,
            rows: 0,
            values,
        }],
    })
}

/// Full block of an elemental/quadrature source for `(ty, ghost)`:
/// `(nb_component, values)` with `values.len() = nb_elements × data/elem`.
fn elemental_full(
    ctx: &EvalContext<'_>,
    desc: &FieldDescriptor,
    ty: ElementType,
    ghost: GhostType,
) -> Result<(usize, Values)> {
    let mesh = ctx.mesh;
    let nel = mesh.nb_element(ty, ghost);
    Ok(match &desc.source {
        FieldSource::Partition => (1, Values::I32(vec![ctx.rank; nel])),
        FieldSource::Mesh(name) if name == "connectivities" || name == "connectivity" => {
            let c = mesh.connectivity(ty, ghost)?;
            (c.nb_component(), Values::I32(c.as_slice().to_vec()))
        }
        FieldSource::Mesh(name) => {
            let missing = || Error::NotFound {
                kind: "mesh data block",
                id: format!("{name}:{ty:?}"),
            };
            match mesh.data().elemental(name)? {
                MeshDataValues::Int(m) => {
                    let a = m.get(ty, ghost).map_err(|_| missing())?;
                    (a.nb_component(), Values::I32(a.as_slice().to_vec()))
                }
                MeshDataValues::UInt(m) => {
                    let a = m.get(ty, ghost).map_err(|_| missing())?;
                    (a.nb_component(), Values::U32(a.as_slice().to_vec()))
                }
                MeshDataValues::Real(m) => {
                    let a = m.get(ty, ghost).map_err(|_| missing())?;
                    (a.nb_component(), Values::F64(a.as_slice().to_vec()))
                }
                MeshDataValues::Bool(m) => {
                    let a = m.get(ty, ghost).map_err(|_| missing())?;
                    (a.nb_component(), Values::Bool(a.as_slice().to_vec()))
                }
                MeshDataValues::String(_)
                | MeshDataValues::Element(_)
                | MeshDataValues::ElementList(_) => {
                    return Err(Error::Unsupported(format!(
                        "dumping mesh data '{name}' (strings and element references have no VTK representation)"
                    )));
                }
            }
        }
        FieldSource::Derived { quantity, law } => {
            let input = match quantity {
                DerivedQuantity::VonMisesStress => "stress",
                _ => "grad_u",
            };
            let d = with_source(
                desc,
                FieldSource::Internal {
                    law: law.clone(),
                    name: input.into(),
                },
                FieldSupport::QuadraturePoint,
            );
            let (nc, v) = values_of(ctx.resolver.resolve(mesh, &d, Some(ty), ghost)?);
            if v.scalar() != Scalar::F64 {
                return Err(Error::InvalidArgument(format!(
                    "dump field '{}': internal '{input}' must be real-valued",
                    desc.name
                )));
            }
            (nc, v)
        }
        FieldSource::Components { .. } => {
            return Err(Error::InvalidArgument(format!(
                "dump field '{}': component selection applies to nodal fields only",
                desc.name
            )));
        }
        _ => {
            let (nc, v) = values_of(ctx.resolver.resolve(mesh, desc, Some(ty), ghost)?);
            check_scalar(desc, v.scalar())?;
            (nc, v)
        }
    })
}

/// Which 2-D padder applies (upstream: field name `stress` or
/// `strain`/`Green strain` with the padding flag, spatial dimension 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Padder {
    Stress,
    Strain,
}

fn padder_of(desc: &FieldDescriptor, dim: usize) -> Option<Padder> {
    if desc.padding != Padding::To3D || dim != 2 {
        return None;
    }
    match &desc.source {
        FieldSource::Internal { name, .. } if name == "stress" => Some(Padder::Stress),
        FieldSource::Derived {
            quantity: DerivedQuantity::SmallStrain | DerivedQuantity::GreenStrain,
            ..
        } => Some(Padder::Strain),
        _ => None,
    }
}

/// Per-element law parameters needed by a padder (`nu`, `Plane_Stress`).
fn law_parameters(
    ctx: &EvalContext<'_>,
    ty: ElementType,
    ghost: GhostType,
    nel: usize,
) -> Result<(Vec<Real>, Vec<Real>)> {
    let get = |name: &str| -> Result<Vec<Real>> {
        let v = ctx.resolver.law_parameter(ctx.mesh, ty, ghost, name)?;
        if v.len() != nel {
            return Err(Error::DimensionMismatch {
                context: "law parameter per element (dump padder)",
                expected: nel,
                got: v.len(),
            });
        }
        Ok(v)
    };
    Ok((get("nu")?, get("Plane_Stress")?))
}

/// Evaluate an elemental / quadrature-point field over `sel.elems`.
fn evaluate_elemental(
    ctx: &EvalContext<'_>,
    desc: &FieldDescriptor,
    sel: &Selection<'_>,
) -> Result<Evaluated> {
    let dim = ctx.mesh.spatial_dimension();
    let padder = padder_of(desc, dim);
    let derived = match &desc.source {
        FieldSource::Derived { quantity, .. } => Some(*quantity),
        _ => None,
    };
    let computed = derived.is_some() || padder.is_some() || desc.homogenize;
    let mut blocks = Vec::with_capacity(sel.elems.len());
    let mut raw_nc: Option<usize> = None;
    let mut homogeneous = true;
    // nb_data per point of the homogeniser input, per type.
    let mut point_comps: Option<usize> = None;
    let mut scalar = None;
    for set in &sel.elems {
        let nel = ctx.mesh.nb_element(set.ty, set.ghost);
        let (nc, full) = elemental_full(ctx, desc, set.ty, set.ghost)?;
        check_nb_components(desc, nc)?;
        // Upstream checkHomogeneity compares the raw arrays' components.
        match raw_nc {
            None => raw_nc = Some(nc),
            Some(c) if c != nc => homogeneous = false,
            Some(_) => {}
        }
        let per_elem = if let Some(per) = full.len().checked_div(nel) {
            // at least one component; whole elements, whole points (nc
            // values each, at least one point), and exactly one tuple per
            // element for elemental support (no products: nc is resolver
            // input)
            if nc == 0
                || !full.len().is_multiple_of(nel)
                || per == 0
                || !per.is_multiple_of(nc)
                || (desc.support == FieldSupport::Elemental && per != nc)
            {
                return Err(Error::DimensionMismatch {
                    context: "elemental dump field (values vs elements × components)",
                    expected: nel.saturating_mul(nc),
                    got: full.len(),
                });
            }
            per
        } else {
            nc // no elements of this type
        };
        let mut values = Values::empty(full.scalar());
        for i in 0..set.nb {
            let e = set.element(i);
            values.extend_from(&full, e * per_elem..(e + 1) * per_elem);
        }
        scalar.get_or_insert(full.scalar());
        let mut block = Block {
            ty: Some(set.ty),
            nb_tuples: set.nb,
            tuple_len: per_elem,
            rows: 0,
            values,
        };
        if !computed {
            blocks.push(block);
            continue;
        }
        let Values::F64(input) = &block.values else {
            return Err(Error::InvalidArgument(format!(
                "dump field '{}': derived quantities, padding and homogenisation need real values",
                desc.name
            )));
        };
        // 1. derived quantity (FieldCompute with a ComputeFunctor)
        let mut pc = nc;
        let mut data: Vec<Real>;
        let mut rows = 0;
        let mut len = per_elem;
        if let Some(q) = derived {
            if nc != dim * dim {
                return Err(Error::DimensionMismatch {
                    context: "derived dump field (components per point vs dim²)",
                    expected: dim * dim,
                    got: nc,
                });
            }
            let nq = if dim == 0 { 0 } else { per_elem / (dim * dim) };
            let per: Vec<Vec<Real>> = input
                .chunks(per_elem.max(1))
                .take(set.nb)
                .map(|t| -> Result<Vec<Real>> {
                    Ok(match q {
                        DerivedQuantity::SmallStrain => {
                            derived::compute_strain(t, dim, StrainMeasure::Small)
                        }
                        DerivedQuantity::GreenStrain => {
                            derived::compute_strain(t, dim, StrainMeasure::Green)
                        }
                        DerivedQuantity::PrincipalSmallStrain => {
                            derived::compute_principal_strain(t, dim, StrainMeasure::Small)?
                        }
                        DerivedQuantity::PrincipalGreenStrain => {
                            derived::compute_principal_strain(t, dim, StrainMeasure::Green)?
                        }
                        DerivedQuantity::VonMisesStress => derived::compute_von_mises(t, dim),
                    })
                })
                .collect::<Result<_>>()?;
            (pc, rows, len) = match q {
                DerivedQuantity::SmallStrain | DerivedQuantity::GreenStrain => {
                    (dim * dim, dim, dim * dim * nq)
                }
                DerivedQuantity::PrincipalSmallStrain | DerivedQuantity::PrincipalGreenStrain => {
                    (dim, dim, dim * nq)
                }
                DerivedQuantity::VonMisesStress => (1, 0, nq),
            };
            data = per.concat();
        } else {
            data = input.clone();
        }
        // 2. material-aware 2-D padder (StressPadder / StrainPadder)
        if let Some(p) = padder {
            if pc != 4 {
                return Err(Error::DimensionMismatch {
                    context: "2-D tensor padding (components per point)",
                    expected: 4,
                    got: pc,
                });
            }
            let (nu, plane_stress) = law_parameters(ctx, set.ty, set.ghost, nel)?;
            let mut out = Vec::with_capacity(data.len() / 4 * 9);
            for (i, t) in data.chunks(len.max(1)).take(set.nb).enumerate() {
                let e = set.element(i);
                let ps = plane_stress[e] != 0.0;
                out.extend(match p {
                    Padder::Stress => derived::pad_stress_2d(t, nu[e], ps),
                    Padder::Strain => derived::pad_strain_2d(t, nu[e], ps),
                });
            }
            data = out;
            len = len / 4 * 9;
            pc = 9;
            rows = 3;
        }
        // 3. homogeniser (AvgHomogenizingFunctor, one value set per element)
        if desc.homogenize {
            match point_comps {
                None => point_comps = Some(pc),
                Some(c) if c != pc => {
                    return Err(Error::InvalidArgument(format!(
                        "dump field '{}': cannot homogenise, {c} vs {pc} values per point across element types",
                        desc.name
                    )));
                }
                Some(_) => {}
            }
            let mut out = Vec::with_capacity(set.nb.saturating_mul(pc).min(data.len()));
            for t in data.chunks(len.max(1)).take(set.nb) {
                out.extend(derived::homogenize(t, pc)?);
            }
            data = out;
            len = pc;
            if rows > 0 && pc % rows != 0 {
                rows = 0;
            }
        }
        block.values = Values::F64(data);
        block.tuple_len = len;
        block.rows = rows;
        blocks.push(block);
    }
    let scalar = if computed {
        Scalar::F64
    } else {
        scalar.unwrap_or_else(|| expected_scalar(desc.value_type))
    };
    let dim_out = match desc.source {
        FieldSource::Partition => 1,
        _ => blocks.first().map_or(0, |b| b.tuple_len),
    };
    Ok(Evaluated {
        scalar,
        dim: dim_out,
        homogeneous: computed || matches!(desc.source, FieldSource::Partition) || homogeneous,
        padded: false,
        blocks,
    })
}

/// Evaluate `desc` over `sel` (the dumper's mesh part, or the field's
/// element group).
pub(crate) fn evaluate(
    ctx: &EvalContext<'_>,
    desc: &FieldDescriptor,
    sel: &Selection<'_>,
) -> Result<Evaluated> {
    let f = match desc.support {
        FieldSupport::Nodal => evaluate_nodal(ctx, desc, sel)?,
        FieldSupport::Elemental | FieldSupport::QuadraturePoint => {
            evaluate_elemental(ctx, desc, sel)?
        }
    };
    // values per tuple (nb_quad × nb_component for quadrature fields)
    // are written as `NumberOfComponents` too
    if f.dim > MAX_DUMP_COMPONENTS {
        return Err(Error::InvalidArgument(format!(
            "dump field '{}': {} values per tuple (at most {MAX_DUMP_COMPONENTS})",
            desc.name, f.dim
        )));
    }
    Ok(f)
}

/// Mesh positions over the selected nodes (`positions`, always written
/// with 3 components by ParaView).
pub(crate) fn positions(mesh: &Mesh, sel: &Selection<'_>, pad_to_3d: bool) -> Evaluated {
    let nodes = mesh.nodes();
    let nc = nodes.nb_component();
    let full = nodes.as_slice();
    let nb = sel.nodes.len();
    let mut v = Vec::with_capacity(nb * nc);
    for i in 0..nb {
        let n = sel.nodes.node(i);
        v.extend_from_slice(&full[n * nc..(n + 1) * nc]);
    }
    Evaluated {
        scalar: Scalar::F64,
        dim: if pad_to_3d { 3 } else { nc },
        homogeneous: true,
        padded: pad_to_3d,
        blocks: vec![Block {
            ty: None,
            nb_tuples: nb,
            tuple_len: nc,
            rows: 0,
            values: Values::F64(v),
        }],
    }
}

/// Connectivity of the selected elements; filtered selections are
/// renumbered to positions in the node list (upstream
/// `FilteredConnectivityField`: first occurrence, error if absent).
pub(crate) fn connectivity(mesh: &Mesh, sel: &Selection<'_>) -> Result<Vec<Block>> {
    let lookup: Option<HashMap<Idx, Int>> = match &sel.nodes {
        NodeSet::All(_) => None,
        NodeSet::List(l) => {
            let mut m = HashMap::with_capacity(l.len());
            for (i, &n) in l.iter().enumerate() {
                if let std::collections::hash_map::Entry::Vacant(v) = m.entry(n) {
                    v.insert(akantu_core::try_idx(i)?);
                }
            }
            Some(m)
        }
    };
    let mut blocks = Vec::with_capacity(sel.elems.len());
    for set in &sel.elems {
        let conn = mesh.connectivity(set.ty, set.ghost)?;
        let nn = conn.nb_component();
        let mut v = Vec::with_capacity(set.nb * nn);
        for i in 0..set.nb {
            let e = set.element(i);
            for &n in conn.tuple(e) {
                match &lookup {
                    None => v.push(n),
                    Some(m) => v.push(*m.get(&n).ok_or_else(|| {
                        Error::Mesh(format!(
                            "node {n} of a filtered dump element is not in the node filter"
                        ))
                    })?),
                }
            }
        }
        blocks.push(Block {
            ty: Some(set.ty),
            nb_tuples: set.nb,
            tuple_len: nn,
            rows: 0,
            values: Values::I32(v),
        });
    }
    Ok(blocks)
}

/// Global variable (text dumper): `(scalar, values)`.
pub(crate) fn variable(resolver: &dyn FieldResolver, name: &str) -> Result<Values> {
    let (_, v) = values_of(resolver.variable(name)?);
    Ok(v)
}
