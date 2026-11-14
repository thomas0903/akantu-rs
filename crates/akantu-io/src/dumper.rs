// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/dumper/{dumpable.hh, dumper_field.hh,
// dumper_iohelper.hh, dumper_iohelper_paraview.*, dumper_text.*,
// dumper_nodal_field.hh, dumper_elemental_field.hh,
// dumper_internal_material_field.hh, dumper_homogenizing_field.hh,
// dumper_padding_helper.hh, dumper_material_padders.hh, dumper_compute.hh},
// src/model/solid_mechanics/solid_mechanics_model_io.cc (createNodalFieldReal/
// Bool, createElementalField: field names and sources), third-party/iohelper.

//! Dumpers.
//!
//! * [`DumperParaview`] — `.pvtu`/`.vtu`/`.pvd` (IOHelper ParaView writer,
//!   base64 or ASCII), [`DumperText`] — text/CSV data files,
//!   [`DumperLammps`] — LAMMPS atomic/bond data files;
//! * [`Dumpable`] — named dumpers with a default (upstream `Dumpable`),
//!   [`solid_mechanics_field`] — upstream solid-mechanics field names;
//! * [`FieldDescriptor`] + [`FieldResolver`] — the descriptor contract
//!   below; [`TextDumper`] — the in-memory demonstration dumper of the
//!   architecture (not an upstream format).
//!
//! Upstream dumpers keep **references** to model arrays (`dumpers::Field`
//! wrapping `Array<T> &`) and read them at `dump()` time; after a cohesive
//! insertion those references may point to resized or replaced storage.
//!
//! akantu-rs: a dumper stores **field descriptors** ([`FieldDescriptor`]:
//! output name, source, support, value type, group, padding,
//! homogenisation), never data. At dump time the owner of the data — the
//! model, coupler or Python simulation — passes a [`FieldResolver`] that
//! fills a transient [`DumpBuffer`] from its **live** storage (DOF manager
//! arrays, law internals, mesh data). Sizes are taken from the mesh at dump
//! time, so a dump after a topology change sees the new nodes and elements
//! without any refresh step; no permanent shadow arrays exist.
//!
//! Division of work at dump time: the resolver returns **full** blocks
//! (all nodes, or all elements of `(type, ghost)`, in mesh order); the
//! dumper applies group filters, component selection, derived
//! quantities, padding and homogenisation (`dumper/eval.rs`), so a
//! resolver never needs to know about groups or output formats.

use std::any::Any;
use std::collections::BTreeMap;
use std::path::Path;

use akantu_core::{
    ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Int, Real, Result, UInt,
};
use akantu_mesh::Mesh;

mod base;
mod base64;
pub(crate) mod cfmt;
mod derived;
mod dumpable;
mod eval;
pub mod group;
mod lammps;
mod paraview;
mod text;
pub mod vtk;

pub use base::MAX_EXPLICIT_PARALLEL_SIZE;
pub use derived::stress_to_von_mises;
pub use dumpable::{Dumpable, solid_mechanics_field};
pub use lammps::{DumperLammps, LammpsAtomStyle};
pub use paraview::{DumperParaview, FileStorageMode};
pub use text::{DumperText, MAX_TEXT_PRECISION, TextDumpMode};

/// Where a field lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldSupport {
    /// One tuple per node.
    Nodal,
    /// One tuple per element.
    Elemental,
    /// One tuple per quadrature point (dumped homogenised or per point).
    QuadraturePoint,
}

/// Scalar type of a dumped field (upstream `createNodalFieldReal/Bool/
/// UInt`, `ElementTypeMapArray<Int>` material index).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldValueType {
    /// `Real` (`Float64`).
    Real,
    /// `Int`/`Idx` (`Int32`).
    Int,
    /// `bool` (`UInt8`).
    Bool,
    /// `UInt` (`UInt32`).
    UInt,
}

/// A solid-mechanics quantity derived from a law internal by the dumper
/// (DUMP-024…028; upstream `ComputeStrain`, `ComputePrincipalStrain`,
/// `ComputeVonMisesStress`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivedQuantity {
    /// `strain`: `½(∇uᵀ + ∇u)` from `grad_u`.
    SmallStrain,
    /// `Green strain`: `½(∇uᵀ∇u + ∇uᵀ + ∇u)` from `grad_u`.
    GreenStrain,
    /// `principal strain`: unsorted eigenvalues of the small strain.
    PrincipalSmallStrain,
    /// `principal Green strain`: unsorted eigenvalues of the Green strain.
    PrincipalGreenStrain,
    /// `Von Mises stress`: `sqrt(3/2 s:s)`, `s = σ − tr(σ)/3 I`, from `stress`.
    VonMisesStress,
}

/// What a field is computed from (resolved by the model).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldSource {
    /// Values of a DOF family (`displacement`, `temperature`, …).
    Dof {
        /// Family.
        family: String,
        /// Time-derivative order (0 = values, 1 = velocity, …).
        order: usize,
    },
    /// Blocked flags of a DOF family (`blocked_dofs`).
    Blocked {
        /// Family.
        family: String,
    },
    /// A model nodal array not owned by the DOF manager (`external_force`,
    /// `internal_force`, `mass`, …).
    ModelNodal(String),
    /// A law internal (`stress`, `strain`, `damage`, …), from every law that
    /// has it or from one named law (upstream `material_internal_field`).
    Internal {
        /// Restrict to one law (`None`: all laws, padded where absent).
        law: Option<String>,
        /// Internal name.
        name: String,
    },
    /// Mesh-owned data, resolved by the dumper itself: `positions`
    /// (`coordinates`), `connectivities`, `nodes_flags` (integer value of
    /// each `NodeFlag`), otherwise mesh data (`tag_0`, …; elemental data
    /// with any number of values per element, upstream
    /// `createFieldFromAttachedData`).
    Mesh(String),
    /// A model elemental array not owned by a law (`material_index`).
    ModelElemental(String),
    /// Writing rank of every element (`partitions`, DUMP-011; errata E1).
    Partition,
    /// A quantity derived from a law internal (`grad_u` or `stress`, asked
    /// from the resolver as `Internal { law, name }`).
    Derived {
        /// Quantity.
        quantity: DerivedQuantity,
        /// Restrict to one law (`None`: all laws).
        law: Option<String>,
    },
    /// Components `first..first + count` of a nodal source (upstream
    /// `createStridedNodalField(field, group, size, stride)`).
    Components {
        /// Underlying nodal source.
        source: Box<FieldSource>,
        /// First component (`stride`).
        first: usize,
        /// Number of components (`size`).
        count: usize,
    },
}

/// Padding applied by the dumper (upstream padding flag of
/// `addDumpFieldVector/Tensor`): nodal fields get 3 components (zeros);
/// in 2-D the `stress` internal and the `strain`/`Green strain` derived
/// fields become 3×3 tensors with the law-dependent out-of-plane normal
/// component (`StressPadder`, `StrainPadder`, DUMP-029/030); other
/// elemental fields are not padded (as upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Padding {
    /// As stored.
    None,
    /// Pad vectors/tensors to 3D (ParaView).
    To3D,
}

/// Restriction of the dumped mesh part (upstream `registerMesh` vs
/// `registerFilteredMesh`).
#[derive(Clone, Debug)]
pub enum MeshFilter {
    /// Every element/node.
    All,
    /// Elements and nodes of an element group (read live at dump time).
    ElementGroup(String),
    /// Nodes of a node group, no elements (point set).
    NodeGroup(String),
    /// Explicit element and node lists (upstream filter arrays).
    Explicit {
        /// Elements per `(type, ghost)`.
        elements: Box<ElementTypeMapArray<Idx>>,
        /// Nodes (output order; connectivities are renumbered to positions
        /// in this list).
        nodes: Vec<Idx>,
    },
}

/// The mesh part a dumper writes (upstream `registerMesh(mesh, dim,
/// ghost, kind)` / `registerFilteredMesh`).
#[derive(Clone, Debug)]
pub struct MeshSelection {
    /// Element types of this spatial dimension (`None`: `_all_dimensions`).
    pub spatial_dimension: Option<usize>,
    /// Ghost block.
    pub ghost: GhostType,
    /// Element kind (`None` or `NotDefined`: every kind).
    pub kind: Option<ElementKind>,
    /// Element/node filter.
    pub filter: MeshFilter,
}

impl MeshSelection {
    /// Unfiltered selection (`kind = NotDefined` means every kind).
    #[must_use]
    pub fn new(spatial_dimension: Option<usize>, ghost: GhostType, kind: ElementKind) -> Self {
        Self {
            spatial_dimension,
            ghost,
            kind: (kind != ElementKind::NotDefined).then_some(kind),
            filter: MeshFilter::All,
        }
    }

    /// Same selection restricted by `filter`.
    #[must_use]
    pub fn filtered(mut self, filter: MeshFilter) -> Self {
        self.filter = filter;
        self
    }
}

/// Rank and size used for file names and the `partitions` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ParallelContext {
    /// Rank/size of the mesh communicator at dump time (upstream
    /// `parallel = true`).
    #[default]
    FromMesh,
    /// Rank 0 of 1 (upstream `parallel = false`).
    Serial,
    /// Explicit context (upstream `getDumper().setParallelContext(rank,
    /// size, root)`).
    Explicit {
        /// Rank.
        rank: usize,
        /// Number of ranks (at most [`MAX_EXPLICIT_PARALLEL_SIZE`]).
        size: usize,
        /// Rank writing the `.pvtu`/`.pvd`/variables/time files.
        root: usize,
    },
    /// A raw IOHelper dumper never given a context (rank 0 of 1, rank
    /// width 3 in the text description).
    IoHelperDefault,
}

/// A registered field (descriptor only, no data).
#[derive(Clone, Debug, PartialEq)]
pub struct FieldDescriptor {
    /// Output name.
    pub name: String,
    /// Source.
    pub source: FieldSource,
    /// Support.
    pub support: FieldSupport,
    /// Scalar type.
    pub value_type: FieldValueType,
    /// Restrict to an element group (upstream `addDumpFieldToDumper(group)`).
    pub group: Option<String>,
    /// Padding.
    pub padding: Padding,
    /// Average quadrature-point values per element (upstream
    /// `HomogenizingField`).
    pub homogenize: bool,
}

/// Transient values produced by a resolver for one block.
#[derive(Clone, Debug, PartialEq)]
pub enum DumpBuffer {
    /// Real values.
    Real {
        /// Components per tuple.
        nb_component: usize,
        /// Values, tuple-major.
        values: Vec<Real>,
    },
    /// Integer values.
    Int {
        /// Components per tuple.
        nb_component: usize,
        /// Values.
        values: Vec<Int>,
    },
    /// Unsigned integer values.
    UInt {
        /// Components per tuple.
        nb_component: usize,
        /// Values.
        values: Vec<UInt>,
    },
    /// Boolean values.
    Bool {
        /// Components per tuple.
        nb_component: usize,
        /// Values.
        values: Vec<bool>,
    },
}

impl DumpBuffer {
    /// Number of tuples.
    #[must_use]
    pub fn len(&self) -> usize {
        let (n, c) = match self {
            Self::Real {
                values,
                nb_component,
            } => (values.len(), *nb_component),
            Self::Int {
                values,
                nb_component,
            } => (values.len(), *nb_component),
            Self::UInt {
                values,
                nb_component,
            } => (values.len(), *nb_component),
            Self::Bool {
                values,
                nb_component,
            } => (values.len(), *nb_component),
        };
        n.checked_div(c).unwrap_or(0)
    }

    /// `true` if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Supplies live field values at dump time (implemented by models,
/// couplers, the Python simulation object).
pub trait FieldResolver {
    /// Values of `field` for **all** nodes (`ty = None`, `len = nb_nodes`)
    /// or for **all** elements of block `(ty, ghost)` in mesh order:
    /// one tuple per element (`Elemental`) or per quadrature point
    /// (`QuadraturePoint`: `nb_component` = values per point, laws without
    /// the internal padded with zeros, upstream `flattenInternal`). Group
    /// filters are applied by the dumper. Sources resolved by the dumper
    /// itself (`Mesh`, `Partition`, the input of `Derived`, which is asked
    /// as `Internal`) never reach this method.
    fn resolve(
        &self,
        mesh: &Mesh,
        field: &FieldDescriptor,
        ty: Option<ElementType>,
        ghost: GhostType,
    ) -> Result<DumpBuffer>;

    /// One value per element of `(ty, ghost)`: the parameter `name` of the
    /// element's law (`nu`, `Plane_Stress` as 0/1), needed by the 2-D
    /// stress/strain padders (DUMP-029/030).
    fn law_parameter(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        let _ = (mesh, ty, ghost);
        Err(Error::Unsupported(format!(
            "law parameter '{name}' requested by a dump padder but the resolver provides none"
        )))
    }

    /// Value of a global variable (upstream `registerVariable`: a scalar or
    /// a vector, written by the text dumper).
    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        Err(Error::not_found("dump variable", name))
    }
}

/// A dumper (upstream `DumperIOHelper`). `Send`, so that a mesh domain can
/// own the dumpers of its groups (`group::SharedMeshDumpers`);
/// dumpers are also owned by models, which Python bindings share between
/// threads under a borrow flag (contract change CC-PY-2).
pub trait Dumper: std::fmt::Debug + Send {
    /// Register the mesh part to dump (upstream `registerMesh(mesh, dim,
    /// ghost, kind)`; `kind = NotDefined`: every kind).
    fn register_mesh(
        &mut self,
        spatial_dimension: usize,
        ghost: GhostType,
        kind: ElementKind,
    ) -> Result<()>;
    /// Register a mesh part with any dimension filter and/or element/node
    /// filter (`registerMesh` with `_all_dimensions`,
    /// `registerFilteredMesh`).
    fn register_mesh_selection(&mut self, selection: MeshSelection) -> Result<()> {
        let _ = selection;
        Err(Error::Unsupported("mesh selections on this dumper".into()))
    }
    /// Register a field descriptor (error on a duplicate name; upstream
    /// warns and ignores the second registration).
    fn register_field(&mut self, field: FieldDescriptor) -> Result<()>;
    /// `unRegisterField` (also `"connectivities"`/`"positions"`).
    fn unregister_field(&mut self, name: &str) -> Result<()> {
        Err(Error::not_found("dump field", name))
    }
    /// `registerVariable`: the text dumper writes the values given by
    /// [`FieldResolver::variable`]; the ParaView and LAMMPS writers accept
    /// and ignore variables, as upstream (error on a duplicate name, which
    /// is shared with the fields).
    fn register_variable(&mut self, name: &str) -> Result<()> {
        Err(Error::Unsupported(format!(
            "dump variable '{name}': this dumper does not take variables"
        )))
    }
    /// `unRegisterVariable` (error if unknown; upstream warns).
    fn unregister_variable(&mut self, name: &str) -> Result<()> {
        Err(Error::not_found("dump variable", name))
    }
    /// `setDirectory`.
    fn set_directory(&mut self, directory: &Path) -> Result<()> {
        let _ = directory;
        Ok(())
    }
    /// `setBaseName`.
    fn set_base_name(&mut self, base_name: &str) -> Result<()>;
    /// `setTimeStep`: activates the time description (`.pvd`, `.time`)
    /// and restarts its clock at 0 (upstream re-activates on every call).
    fn set_time_step(&mut self, time_step: Real) -> Result<()> {
        let _ = time_step;
        Ok(())
    }
    /// `setTextModeToDumper` (ASCII ParaView output; no-op elsewhere, as
    /// upstream).
    fn set_text_mode(&mut self) -> Result<()> {
        Ok(())
    }
    /// `getDumper().setParallelContext(rank, size, root)`.
    fn set_parallel_context(&mut self, ctx: ParallelContext) -> Result<()> {
        let _ = ctx;
        Ok(())
    }
    /// Write one time step, reading live values through `resolver`
    /// (upstream `dump()` / `dump(step)` / `dump(time, step)`; `step`
    /// sets the counter, `time` the current time, then the counter is
    /// incremented). Everything is validated and rendered before the
    /// first file is written; on any error (including directory and write
    /// errors) the dumper state — count, clock, time series, open
    /// variable files — is left as before the call.
    fn dump(
        &mut self,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        time: Option<Real>,
        step: Option<usize>,
    ) -> Result<()>;
    /// Follow renamed groups (upstream dumpers refer to the group object):
    /// for every `old → new` of `renames`, applied simultaneously (a swap of
    /// two names is one call), a selection on group `old` (an element group
    /// if `element`, else a node group) and fields restricted to element
    /// group `old` name `new` afterwards ([`group::MeshDumpers`],
    /// which calls it when it sees a group under a new name). Default:
    /// nothing refers to groups.
    fn rename_groups(&mut self, element: bool, renames: &BTreeMap<String, String>) {
        let _ = (element, renames);
    }
    /// Downcasting support (upstream `getDumper<T>(name)`).
    fn as_any(&self) -> &dyn Any;
    /// Mutable downcasting support.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// Pad a tuple-major real buffer of `dim`-vectors or `dim×dim` tensors to
/// 3D (upstream `PadderGeneric`): vectors get zeros, tensors are embedded
/// in the upper-left block of a 3×3 matrix.
#[must_use]
pub fn pad_to_3d(values: &[Real], nb_component: usize, dim: usize) -> (usize, Vec<Real>) {
    let n = values.len().checked_div(nb_component).unwrap_or(0);
    if dim >= 3 {
        (nb_component, values.to_vec())
    } else if nb_component == dim {
        let mut out = Vec::with_capacity(n * 3);
        for t in values.chunks_exact(nb_component) {
            out.extend_from_slice(t);
            out.extend(std::iter::repeat_n(0.0, 3 - dim));
        }
        (3, out)
    } else if nb_component == dim * dim {
        let mut out = Vec::with_capacity(n * 9);
        for t in values.chunks_exact(nb_component) {
            let mut m = [0.0; 9];
            for c in 0..dim {
                for r in 0..dim {
                    m[r + 3 * c] = t[r + dim * c];
                }
            }
            out.extend_from_slice(&m);
        }
        (9, out)
    } else {
        (nb_component, values.to_vec())
    }
}

/// In-memory debug dumper (one summary line per field and block),
/// demonstrating the resolver contract (`docs/ARCHITECTURE.md`). Not an
/// upstream format: the upstream text output is [`DumperText`].
#[derive(Debug, Default)]
pub struct TextDumper {
    fields: Vec<FieldDescriptor>,
    dim: usize,
    /// Accumulated output.
    pub output: String,
}

impl Dumper for TextDumper {
    fn register_mesh(
        &mut self,
        spatial_dimension: usize,
        _: GhostType,
        _: ElementKind,
    ) -> Result<()> {
        self.dim = spatial_dimension;
        Ok(())
    }
    fn register_field(&mut self, field: FieldDescriptor) -> Result<()> {
        if self.fields.iter().any(|f| f.name == field.name) {
            return Err(Error::AlreadyExists {
                kind: "dump field",
                id: field.name,
            });
        }
        self.fields.push(field);
        Ok(())
    }
    fn set_base_name(&mut self, _: &str) -> Result<()> {
        Ok(())
    }
    fn rename_groups(&mut self, element: bool, renames: &BTreeMap<String, String>) {
        if element {
            for f in &mut self.fields {
                if let Some(new) = f.group.as_ref().and_then(|g| renames.get(g)) {
                    f.group = Some(new.clone());
                }
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn dump(
        &mut self,
        mesh: &Mesh,
        resolver: &dyn FieldResolver,
        _: Option<Real>,
        step: Option<usize>,
    ) -> Result<()> {
        use std::fmt::Write;
        let _ = writeln!(self.output, "step {}", step.unwrap_or(0));
        for f in &self.fields {
            let blocks: Vec<(Option<ElementType>, GhostType)> = match f.support {
                FieldSupport::Nodal => vec![(None, GhostType::NotGhost)],
                _ => mesh
                    .element_types(akantu_core::TypeFilter::default())
                    .map(|t| (Some(t), GhostType::NotGhost))
                    .collect(),
            };
            for (ty, g) in blocks {
                let b = resolver.resolve(mesh, f, ty, g)?;
                let text = match &b {
                    DumpBuffer::Real {
                        nb_component,
                        values,
                    } => {
                        let (c, v) = match f.padding {
                            Padding::To3D => pad_to_3d(values, *nb_component, self.dim),
                            Padding::None => (*nb_component, values.clone()),
                        };
                        v.chunks(c.max(1))
                            .map(|t| {
                                t.iter()
                                    .map(|x| format!("{x}"))
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .collect::<Vec<_>>()
                            .join(" | ")
                    }
                    DumpBuffer::Int { values, .. } => values
                        .iter()
                        .map(Int::to_string)
                        .collect::<Vec<_>>()
                        .join(" "),
                    DumpBuffer::UInt { values, .. } => values
                        .iter()
                        .map(UInt::to_string)
                        .collect::<Vec<_>>()
                        .join(" "),
                    DumpBuffer::Bool { values, .. } => values
                        .iter()
                        .map(|b| if *b { "1" } else { "0" })
                        .collect::<Vec<_>>()
                        .join(" "),
                };
                let _ = writeln!(self.output, "{} [{}]: {text}", f.name, b.len());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_follows_upstream_layout() {
        assert_eq!(pad_to_3d(&[1.0, 2.0], 2, 2), (3, vec![1.0, 2.0, 0.0]));
        let (c, m) = pad_to_3d(&[1.0, 2.0, 3.0, 4.0], 4, 2);
        assert_eq!(c, 9);
        assert_eq!(m, vec![1.0, 2.0, 0.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
    }
}
