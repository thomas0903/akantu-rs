// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/boundary_condition/
// {boundary_condition.hh, boundary_condition_tmpl.hh,
// boundary_condition_functor.hh, boundary_condition_functor_inline_impl.hh}.

//! Boundary-condition functors (upstream `BC::Dirichlet::*`,
//! `BC::Neumann::*`, BC-002/003, BC-006…012) and their application on
//! element groups (upstream `BoundaryCondition<Model>::applyBC`, BC-001,
//! BC-004, BC-005).
//!
//! Upstream `BoundaryCondition<ModelType>` is a mixin bound by `initBC` to a
//! model's primal (`displacement`), primal increment and dual
//! (`external_force`) arrays. In the port a model calls
//! [`apply_dirichlet`] / [`apply_neumann`] with its arrays (the DOF arrays
//! live in the DOF manager, D-021) and its boundary FE engine; a model's
//! `apply_bc` method resolves the group by name and wraps errors like
//! upstream ([`group_error`]). Functors are trait objects, so Python-defined
//! functors implement the same traits.

use akantu_core::array_shared::{ArrayMut, SharedArray};
use akantu_core::enums::SpatialDirection;
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, Error, GhostType, Idx, Real, Result};
use akantu_fe::eigen::{self, Dims};
use akantu_fe::{FeEngine, IntegrationPoint};
use akantu_mesh::Mesh;
use akantu_mesh::group::ElementGroupRef;
use akantu_solver::DofManager;

use crate::model::{
    ReduceOperation, make_consistent_for_periodicity, make_consistent_for_periodicity_values,
};
use crate::neumann_store::{BlockContribution, NeumannStore};

/// Dirichlet functor: called once per node of the group (upstream
/// `DirichletFunctor::operator()(node, flags, primal, coord)`). `flags` and
/// `primal` are the node's blocked flags and primal values (one entry per
/// DOF component), `coord` its coordinates.
pub trait DirichletFunctor {
    /// Set blocked `flags` and prescribed `primal` values of `node`.
    fn apply(
        &self,
        node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        coord: &[Real],
    ) -> Result<()>;

    /// `true` if the functor calls back into foreign code (a Python
    /// functor): [`apply_dirichlet`] then calls
    /// [`DirichletFunctor::apply_shared`] once per node with no element
    /// guard alive. `false` (the default, every native functor): the guards
    /// of the nodal arrays are taken once for the whole group and
    /// [`DirichletFunctor::apply`] runs on the live rows in place
    /// (bitwise the same values).
    fn calls_foreign(&self) -> bool {
        false
    }

    /// Apply the functor to tuple `row` (= `node`) of the live nodal arrays
    /// (upstream passes row views of `primal` and `blocked_dofs`); used by
    /// [`apply_dirichlet`] only when [`DirichletFunctor::calls_foreign`] is
    /// `true`.
    ///
    /// The arrays are exportable ([`SharedArray`], D-047); no element guard
    /// of any exportable array is held by [`apply_dirichlet`] during this
    /// call, so an implementation may call back into foreign code
    /// (`ForeignRuntime::callback`) and alias the rows there, as upstream's
    /// Python trampoline does (`akantu-python`). The default copies the rows
    /// out through guards, calls [`DirichletFunctor::apply`] and copies them
    /// back (bitwise what `apply` on the rows themselves computes).
    ///
    /// With the default, a write that the functor's callback makes to its
    /// **own** rows through the foreign side is overwritten by the copy-back:
    /// a functor whose callback writes the rows must override
    /// `apply_shared` and pass live row views, as the Python adapter does.
    fn apply_shared(
        &self,
        node: Idx,
        row: usize,
        blocked: &mut SharedArray<bool>,
        primal: &mut SharedArray<Real>,
        coord: &[Real],
    ) -> Result<()> {
        apply_rows_by_copy(self, node, row, blocked, primal, coord)
    }
}

/// The default [`DirichletFunctor::apply_shared`]: copy tuple `row` of
/// `blocked` and `primal` out through guards, call
/// [`DirichletFunctor::apply`] on the copies, copy them back.
pub fn apply_rows_by_copy<F: DirichletFunctor + ?Sized>(
    functor: &F,
    node: Idx,
    row: usize,
    blocked: &mut SharedArray<bool>,
    primal: &mut SharedArray<Real>,
    coord: &[Real],
) -> Result<()> {
    let mut f = blocked.read()?.get_tuple(row)?.to_vec();
    let mut p = primal.read()?.get_tuple(row)?.to_vec();
    functor.apply(node, &mut f, &mut p, coord)?;
    blocked.write()?.get_tuple_mut(row)?.copy_from_slice(&f);
    primal.write()?.get_tuple_mut(row)?.copy_from_slice(&p);
    Ok(())
}

/// Neumann functor: called per quadrature point of the boundary elements
/// (upstream `NeumannFunctor::operator()(quad_point, dual, coord, normals)`).
pub trait NeumannFunctor {
    /// Compute the dual (traction/flux) value at `quad_point`.
    fn apply(
        &self,
        quad_point: &IntegrationPoint,
        dual: &mut [Real],
        coord: &[Real],
        normals: &[Real],
    ) -> Result<()>;

    /// `true` if [`NeumannFunctor::apply`] calls back into foreign code (a
    /// Python functor): [`apply_neumann`] then holds no element guard of
    /// an exportable array (the group's element lists are copied once per
    /// type). `false` (the default, every native functor): the lists are
    /// read through their guards.
    fn calls_foreign(&self) -> bool {
        false
    }
}

fn axis_index(axis: SpatialDirection) -> usize {
    match axis {
        SpatialDirection::X => 0,
        SpatialDirection::Y => 1,
        SpatialDirection::Z => 2,
    }
}

/// Upstream `DIRICHLET_SANITY_CHECK` (`primal.size() <= flags.size()`) plus
/// the bounds check of the axis access (unchecked upstream).
fn sanity(flags: &[bool], primal: &[Real], axis: Option<SpatialDirection>) -> Result<usize> {
    if primal.len() > flags.len() {
        return Err(Error::invalid(
            "The primal vector and flags vectors given to the boundary condition functor have different sizes!",
        ));
    }
    match axis {
        None => Ok(0),
        Some(a) => {
            let i = axis_index(a);
            let len = flags.len().min(primal.len());
            if i < len {
                Ok(i)
            } else {
                Err(Error::OutOfRange {
                    what: "Dirichlet axis",
                    index: i,
                    len,
                })
            }
        }
    }
}

/// `BC::Dirichlet::FlagOnly`: block `axis` without changing the value.
#[derive(Clone, Copy, Debug)]
pub struct FlagOnly {
    /// Axis.
    pub axis: SpatialDirection,
}

impl DirichletFunctor for FlagOnly {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        _coord: &[Real],
    ) -> Result<()> {
        // upstream only checks the sizes; the flag axis must exist
        sanity(flags, primal, None)?;
        let i = axis_index(self.axis);
        let len = flags.len();
        *flags.get_mut(i).ok_or(Error::OutOfRange {
            what: "Dirichlet axis",
            index: i,
            len,
        })? = true;
        Ok(())
    }
}

/// `BC::Dirichlet::FixedValue`: block `axis` and set it to `value`.
#[derive(Clone, Copy, Debug)]
pub struct FixedValue {
    /// Prescribed value.
    pub value: Real,
    /// Axis.
    pub axis: SpatialDirection,
}

impl DirichletFunctor for FixedValue {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        _coord: &[Real],
    ) -> Result<()> {
        let i = sanity(flags, primal, Some(self.axis))?;
        flags[i] = true;
        primal[i] = self.value;
        Ok(())
    }
}

/// `BC::Dirichlet::IncrementValue`: block `axis` and add `value`
/// (`set_increment` = upstream `setIncrement`).
#[derive(Clone, Copy, Debug)]
pub struct IncrementValue {
    /// Increment.
    pub value: Real,
    /// Axis.
    pub axis: SpatialDirection,
}

impl IncrementValue {
    /// Upstream `setIncrement(val)`.
    pub fn set_increment(&mut self, value: Real) {
        self.value = value;
    }
}

impl DirichletFunctor for IncrementValue {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        _coord: &[Real],
    ) -> Result<()> {
        let i = sanity(flags, primal, Some(self.axis))?;
        flags[i] = true;
        primal[i] += self.value;
        Ok(())
    }
}

/// `BC::Dirichlet::Increment`: block every component and add the vector
/// `value` (`set_increment` = upstream `setIncrement`).
#[derive(Clone, Debug)]
pub struct Increment {
    /// Increment (one entry per component).
    pub value: Vec<Real>,
}

impl Increment {
    /// Upstream `setIncrement(val)`.
    pub fn set_increment(&mut self, value: Vec<Real>) {
        self.value = value;
    }
}

impl DirichletFunctor for Increment {
    fn apply(
        &self,
        _node: Idx,
        flags: &mut [bool],
        primal: &mut [Real],
        _coord: &[Real],
    ) -> Result<()> {
        sanity(flags, primal, None)?;
        // Upstream `primal += value` (an Eigen size assertion, compiled out
        // with NDEBUG): sizes must agree.
        if primal.len() != self.value.len() {
            return Err(Error::DimensionMismatch {
                context: "BC::Dirichlet::Increment",
                expected: primal.len(),
                got: self.value.len(),
            });
        }
        flags.fill(true);
        for (p, v) in primal.iter_mut().zip(&self.value) {
            *p += v;
        }
        Ok(())
    }
}

/// `BC::Neumann::FreeBoundary`: zero dual value.
#[derive(Clone, Copy, Debug, Default)]
pub struct FreeBoundary;

impl NeumannFunctor for FreeBoundary {
    fn apply(
        &self,
        _q: &IntegrationPoint,
        dual: &mut [Real],
        _coord: &[Real],
        _normals: &[Real],
    ) -> Result<()> {
        dual.fill(0.0);
        Ok(())
    }
}

/// `BC::Neumann::FromHigherDim` (alias `FromStress`): `dual = σ · n` with a
/// constant matrix `σ` (`dual.len() × normals.len()`), evaluated as
/// upstream's dynamic Eigen product `Matrix<Real> * VectorProxy`.
#[derive(Clone, Debug)]
pub struct FromHigherDim {
    /// The matrix (upstream `bc_data`).
    pub bc_data: MatrixX,
}

/// Upstream alias `BC::Neumann::FromStress`.
pub type FromStress = FromHigherDim;

impl FromHigherDim {
    /// Functor for the constant matrix `bc_data`.
    #[must_use]
    pub fn new(bc_data: MatrixX) -> Self {
        Self { bc_data }
    }
}

impl NeumannFunctor for FromHigherDim {
    fn apply(
        &self,
        _q: &IntegrationPoint,
        dual: &mut [Real],
        _coord: &[Real],
        normals: &[Real],
    ) -> Result<()> {
        let (r, c) = (self.bc_data.rows(), self.bc_data.cols());
        if c != normals.len() || r != dual.len() {
            return Err(Error::DimensionMismatch {
                context: "BC::Neumann::FromHigherDim: sigma * n",
                expected: dual.len() * normals.len(),
                got: r * c,
            });
        }
        let m = self.bc_data.as_slice();
        // written in place (`dual.len() == r` checked above;
        // no allocation per quadrature point)
        eigen::product(
            Dims::dynamic(r, c),
            Dims::dynamic(c, 1),
            |i, k| m[i + k * r],
            |k, _| normals[k],
            dual,
        )?;
        Ok(())
    }
}

/// `BC::Neumann::FromSameDim` (alias `FromTraction`): a constant dual
/// vector, independent of position and normal.
#[derive(Clone, Debug)]
pub struct FromSameDim {
    /// The vector (upstream `bc_data`).
    pub bc_data: Vec<Real>,
}

/// Upstream alias `BC::Neumann::FromTraction`.
pub type FromTraction = FromSameDim;

impl FromSameDim {
    /// Functor for the constant vector `bc_data`.
    #[must_use]
    pub fn new(bc_data: Vec<Real>) -> Self {
        Self { bc_data }
    }
}

impl NeumannFunctor for FromSameDim {
    fn apply(
        &self,
        _q: &IntegrationPoint,
        dual: &mut [Real],
        _coord: &[Real],
        _normals: &[Real],
    ) -> Result<()> {
        if dual.len() != self.bc_data.len() {
            return Err(Error::DimensionMismatch {
                context: "BC::Neumann::FromSameDim",
                expected: dual.len(),
                got: self.bc_data.len(),
            });
        }
        dual.copy_from_slice(&self.bc_data);
        Ok(())
    }
}

/// Upstream wrapping of an error raised while applying a functor on a named
/// group (`applyBC(func, group_name)`).
#[must_use]
pub fn group_error(group: &str, e: &Error) -> Error {
    Error::invalid(format!(
        "Error applying a boundary condition onto \"{group}\"! [{e}]"
    ))
}

/// Upstream Dirichlet `applyBC(func, group)` (BC-004): for every node of the
/// group's node group (in its order), call `functor` on the node's `primal`
/// values, its `blocked` flags and its coordinates; then make `blocked`
/// (`or`) and `primal` (copy) consistent for periodicity. `primal` and
/// `blocked` are nodal arrays with the same number of components.
pub fn apply_dirichlet(
    functor: &dyn DirichletFunctor,
    mesh: &Mesh,
    group: ElementGroupRef<'_>,
    primal: &mut SharedArray<Real>,
    blocked: &mut SharedArray<bool>,
) -> Result<()> {
    let nn = mesh.nb_nodes();
    if primal.len() != nn || blocked.len() != nn {
        return Err(Error::DimensionMismatch {
            context: "applyBC (Dirichlet): nodal arrays",
            expected: nn,
            got: primal.len().min(blocked.len()),
        });
    }
    let dim = mesh.spatial_dimension();
    // (the test-only export-all mode drives every functor through the
    // foreign protocol below; `ENABLED` is `false` in product builds)
    if !functor.calls_foreign() && !akantu_core::array_shared::export_all::ENABLED {
        return apply_dirichlet_native(functor, mesh, group, primal, blocked);
    }
    // a copy: the node list is exportable and no guard may be held across
    // `apply_shared` (below)
    let nodes = group.nodes().as_slice().to_vec();
    // validate every node first (no partial application on a bad group)
    for &n in &nodes {
        let u = akantu_core::try_to_usize(n)?;
        if u >= nn {
            return Err(Error::OutOfRange {
                what: "group node",
                index: u,
                len: nn,
            });
        }
    }
    // No element guard is held across `apply_shared`, which may call back
    // into foreign code (ARCHITECTURE.md, "Shared storage and its aliasing rules"): the coordinates are copied per node.
    let mut coord = vec![0.0; dim];
    for &n in &nodes {
        let u = akantu_core::try_to_usize(n)?;
        coord.copy_from_slice(&mesh.nodes().as_slice()[u * dim..(u + 1) * dim]);
        akantu_core::array_shared::export_all::callback_site("DirichletFunctor::apply_shared");
        functor.apply_shared(n, u, blocked, primal, &coord)?;
    }
    {
        let mut b = blocked.write()?;
        let (len, nbc) = (b.len(), b.nb_component());
        make_consistent_for_periodicity_values(
            mesh,
            b.as_mut_slice(),
            len,
            nbc,
            ReduceOperation::Or,
            |a, b| a || b,
        )?;
    }
    let mut p = primal.write()?;
    let (len, nbc) = (p.len(), p.nb_component());
    make_consistent_for_periodicity_values(
        mesh,
        p.as_mut_slice(),
        len,
        nbc,
        ReduceOperation::Id,
        |a, _| a,
    )?;
    Ok(())
}

/// Upstream's `makeConsistentForPeriodicity` of the Dirichlet `applyBC`:
/// blocked flags (or), then primal values (master's).
fn dirichlet_periodicity(
    mesh: &Mesh,
    p: &mut ArrayMut<'_, Real>,
    b: &mut ArrayMut<'_, bool>,
) -> Result<()> {
    let (len, nbc) = (b.len(), b.nb_component());
    make_consistent_for_periodicity_values(
        mesh,
        b.as_mut_slice(),
        len,
        nbc,
        ReduceOperation::Or,
        |a, b| a || b,
    )?;
    let (len, nbc) = (p.len(), p.nb_component());
    make_consistent_for_periodicity_values(
        mesh,
        p.as_mut_slice(),
        len,
        nbc,
        ReduceOperation::Id,
        |a, _| a,
    )?;
    Ok(())
}

/// The native path of [`apply_dirichlet`] (functors that never call back
/// into foreign code): one guard per array for the whole group and
/// [`DirichletFunctor::apply`] on the live rows in place, as before the
/// arrays became exportable (same values bitwise).
fn apply_dirichlet_native(
    functor: &dyn DirichletFunctor,
    mesh: &Mesh,
    group: ElementGroupRef<'_>,
    primal: &mut SharedArray<Real>,
    blocked: &mut SharedArray<bool>,
) -> Result<()> {
    let nn = mesh.nb_nodes();
    let dim = mesh.spatial_dimension();
    let nodes = group.node_group.nodes_shared().read()?;
    let coords = mesh.nodes_shared().read()?;
    let coords = coords.as_slice();
    // validate every node first (no partial application on a bad group)
    for &n in nodes.as_slice() {
        let u = akantu_core::try_to_usize(n)?;
        if u >= nn {
            return Err(Error::OutOfRange {
                what: "group node",
                index: u,
                len: nn,
            });
        }
    }
    let mut p = primal.write()?;
    let mut b = blocked.write()?;
    let (pc, bc) = (p.nb_component(), b.nb_component());
    for &n in nodes.as_slice() {
        let u = akantu_core::try_to_usize(n)?;
        let pr = &mut p.as_mut_slice()[u * pc..(u + 1) * pc];
        let f = &mut b.as_mut_slice()[u * bc..(u + 1) * bc];
        functor.apply(n, f, pr, &coords[u * dim..(u + 1) * dim])?;
    }
    dirichlet_periodicity(mesh, &mut p, &mut b)
}

/// Upstream Neumann `applyBC(func, group)` (BC-005): for both ghost types
/// and every element type of dimension `dim − 1` of the group, interpolate
/// the coordinates at the boundary quadrature points, call `functor` per
/// point with the stored normal, integrate `Nᵀ·dual` over the elements and
/// assemble into a zeroed local array, which is made consistent for
/// periodicity (sum) and **added** to `dual` (upstream
/// `boundary_condition_tmpl.hh:91-190`). `fe_boundary` is the model's
/// boundary FE engine (normals computed). 1-D models: upstream
/// `AKANTU_TO_IMPLEMENT`.
pub fn apply_neumann(
    functor: &dyn NeumannFunctor,
    mesh: &Mesh,
    group: ElementGroupRef<'_>,
    fe_boundary: &FeEngine,
    spatial_dimension: usize,
    dual: &mut SharedArray<Real>,
) -> Result<()> {
    apply_neumann_recorded(
        functor,
        mesh,
        group,
        fe_boundary,
        spatial_dimension,
        dual,
        None,
    )
}

/// [`apply_neumann`], and with `store` the elemental contributions of the
/// application recorded per boundary element (D-105, UD-1280,
/// [`NeumannStore`]): after every functor call and before the dual array is
/// updated, the store forgets the records of nodes written by another path
/// since its last update, adds this application's contributions and takes
/// the updated dual array as its view of it. Nothing is changed on error.
///
/// # Errors
/// See [`apply_neumann`]; a contribution that does not fit the store.
pub fn apply_neumann_recorded(
    functor: &dyn NeumannFunctor,
    mesh: &Mesh,
    group: ElementGroupRef<'_>,
    fe_boundary: &FeEngine,
    spatial_dimension: usize,
    dual: &mut SharedArray<Real>,
    mut store: Option<&mut NeumannStore>,
) -> Result<()> {
    if spatial_dimension == 1 {
        return Err(Error::NotImplemented(
            "Neumann boundary conditions in 1-D (AKANTU_TO_IMPLEMENT upstream)",
        ));
    }
    if dual.len() != mesh.nb_nodes() {
        return Err(Error::DimensionMismatch {
            context: "applyBC (Neumann): nodal dual array",
            expected: mesh.nb_nodes(),
            got: dual.len(),
        });
    }
    let mut local = Array::try_zeros(dual.len(), dual.nb_component())?;
    let mut contributions = store.as_ref().map(|_| Vec::new());
    for ghost in GhostType::ALL {
        neumann_one_ghost(
            functor,
            mesh,
            group,
            fe_boundary,
            spatial_dimension,
            &mut local,
            ghost,
            contributions.as_mut(),
        )?;
    }
    make_consistent_for_periodicity(mesh, &mut local, ReduceOperation::Sum, |a, b| a + b)?;
    // `dual` is read only now, after every functor call (a foreign functor
    // may have written into it through a view, upstream semantics).
    let mut dual = dual.write()?;
    if let (Some(st), Some(c)) = (store.as_deref_mut(), contributions) {
        st.check_contributions(mesh, &c)?;
        st.forget_foreign_writes(mesh, dual.as_slice())?;
        st.add_contributions(mesh, c)?;
    }
    for (d, l) in dual.as_mut_slice().iter_mut().zip(local.as_slice()) {
        *d += l;
    }
    if let Some(st) = store {
        st.snapshot(dual.as_slice());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // the application's inputs plus the record
fn neumann_one_ghost(
    functor: &dyn NeumannFunctor,
    mesh: &Mesh,
    group: ElementGroupRef<'_>,
    fe: &FeEngine,
    dim: usize,
    dual: &mut Array<Real>,
    ghost: GhostType,
    mut record: Option<&mut Vec<BlockContribution>>,
) -> Result<()> {
    let nb_dof = dual.nb_component();
    let types: Vec<_> = group
        .element_types(ghost)
        .into_iter()
        .filter(|ty| ty.spatial_dimension() == dim - 1)
        .collect();
    for ty in types {
        // A foreign functor is called below: no guard of the (exportable)
        // element list may be alive then, so it gets a copy (ARCHITECTURE.md, "Shared storage and its aliasing rules").
        let ids_guard;
        let ids_copy;
        // (the test-only export-all mode copies for every functor)
        let foreign = functor.calls_foreign() || akantu_core::array_shared::export_all::ENABLED;
        let ids: &[Idx] = if foreign {
            ids_copy = group.elements_of(ty, ghost).to_vec();
            &ids_copy
        } else {
            ids_guard = group.elements_of(ty, ghost);
            &ids_guard
        };
        let nq = fe.nb_integration_points(ty, ghost)?;
        let nn = ty.nb_nodes_per_element();
        let nb_elements = ids.len();
        let mut dual_before = Array::try_zeros(nb_elements * nq, nb_dof)?;
        let mut quad_coords = Array::new(dim);
        let normals = fe.normals_on_integration_points(ty, ghost)?;
        if normals.nb_component() != dim {
            return Err(Error::DimensionMismatch {
                context: "applyBC (Neumann): normals",
                expected: dim,
                got: normals.nb_component(),
            });
        }
        fe.interpolate_on_integration_points(
            mesh,
            mesh.nodes().view(),
            &mut quad_coords,
            dim,
            ty,
            ghost,
            Some(ids),
        )?;
        let nslice = normals.as_slice();
        let qc = quad_coords.as_slice();
        for (pos, &el) in ids.iter().enumerate() {
            let e = akantu_core::try_to_usize(el)?;
            let element = Element::try_new(ty, e, ghost)?;
            for q in 0..nq {
                let ip = IntegrationPoint::new(
                    element,
                    akantu_core::try_idx(q)?,
                    akantu_core::try_idx(nq)?,
                );
                let t = pos * nq + q;
                let nt = e * nq + q;
                let n = nslice
                    .get(nt * dim..(nt + 1) * dim)
                    .ok_or(Error::OutOfRange {
                        what: "normal of boundary quadrature point",
                        index: nt,
                        len: normals.len(),
                    })?;
                if foreign {
                    akantu_core::array_shared::export_all::callback_site("NeumannFunctor::apply");
                }
                functor.apply(
                    &ip,
                    &mut dual_before.as_mut_slice()[t * nb_dof..(t + 1) * nb_dof],
                    &qc[t * dim..(t + 1) * dim],
                    n,
                )?;
            }
        }
        let mut by_shapes = Array::new(nb_dof * nn);
        fe.compute_ntb(dual_before.view(), &mut by_shapes, ty, ghost, Some(ids))?;
        let mut integ = Array::new(nb_dof * nn);
        fe.integrate(
            by_shapes.view(),
            &mut integ,
            nb_dof * nn,
            ty,
            ghost,
            Some(ids),
        )?;
        DofManager::assemble_elemental_array_local_array(
            mesh,
            integ.view(),
            dual,
            ty,
            ghost,
            1.0,
            Some(ids),
        )?;
        if let Some(r) = record.as_deref_mut() {
            r.push(BlockContribution {
                ty,
                ghost,
                ids: ids.to_vec(),
                values: integ,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirichlet_functors_match_upstream() {
        let mut flags = [false; 2];
        let mut primal = [1.0, 2.0];
        FixedValue {
            value: 5.0,
            axis: SpatialDirection::Y,
        }
        .apply(0, &mut flags, &mut primal, &[0.0, 0.0])
        .unwrap();
        assert_eq!((flags, primal), ([false, true], [1.0, 5.0]));
        IncrementValue {
            value: 0.5,
            axis: SpatialDirection::X,
        }
        .apply(0, &mut flags, &mut primal, &[0.0, 0.0])
        .unwrap();
        assert_eq!((flags, primal), ([true, true], [1.5, 5.0]));
        let mut f = [false; 2];
        FlagOnly {
            axis: SpatialDirection::X,
        }
        .apply(0, &mut f, &mut primal, &[0.0, 0.0])
        .unwrap();
        assert_eq!(f, [true, false]);
        assert!(
            FixedValue {
                value: 0.0,
                axis: SpatialDirection::Z
            }
            .apply(0, &mut flags, &mut primal, &[0.0, 0.0])
            .is_err()
        );
        assert!(
            FlagOnly {
                axis: SpatialDirection::Z
            }
            .apply(0, &mut f, &mut primal, &[0.0, 0.0])
            .is_err()
        );
    }

    /// BC-006…009: repeated application accumulates exactly as upstream.
    #[test]
    fn repeated_application_and_increment() {
        let mut flags = [false; 3];
        let mut primal = [1.0, 2.0, 3.0];
        let mut inc = IncrementValue {
            value: 0.25,
            axis: SpatialDirection::Z,
        };
        for _ in 0..3 {
            inc.apply(0, &mut flags, &mut primal, &[0.0; 3]).unwrap();
        }
        assert_eq!(primal, [1.0, 2.0, 3.75]);
        inc.set_increment(-1.0);
        inc.apply(0, &mut flags, &mut primal, &[0.0; 3]).unwrap();
        assert_eq!(primal[2], 2.75);
        let mut vinc = Increment {
            value: vec![0.5, -0.5, 1.0],
        };
        let mut f2 = [false; 3];
        vinc.apply(0, &mut f2, &mut primal, &[0.0; 3]).unwrap();
        assert_eq!(f2, [true; 3]);
        assert_eq!(primal, [1.5, 1.5, 3.75]);
        vinc.set_increment(vec![1.0]);
        assert!(vinc.apply(0, &mut f2, &mut primal, &[0.0; 3]).is_err());
        // primal longer than flags: sanity check
        assert!(
            FlagOnly {
                axis: SpatialDirection::X
            }
            .apply(0, &mut [false; 1], &mut [0.0; 2], &[0.0; 2])
            .is_err()
        );
    }

    /// BC-010…012 (FromHigherDim with a non-symmetric matrix detects a
    /// transpose).
    #[test]
    fn neumann_functors() {
        let e = Element::new(akantu_core::ElementType::Segment2, 0, GhostType::NotGhost);
        let ip = IntegrationPoint::new(e, 0, 1);
        let sigma = MatrixX::from_col_major(2, 2, vec![1.0, 3.0, 2.0, 4.0]).unwrap(); // [[1,2],[3,4]]
        let mut dual = [9.0, 9.0];
        FromStress::new(sigma)
            .apply(&ip, &mut dual, &[0.0, 0.0], &[1.0, 0.0])
            .unwrap();
        assert_eq!(dual, [1.0, 3.0]); // first column, not first row
        let mut dual = [0.0, 0.0];
        FromTraction::new(vec![5.0, -1.0])
            .apply(&ip, &mut dual, &[0.0; 2], &[0.0, 1.0])
            .unwrap();
        assert_eq!(dual, [5.0, -1.0]);
        FreeBoundary
            .apply(&ip, &mut dual, &[0.0; 2], &[0.0, 1.0])
            .unwrap();
        assert_eq!(dual, [0.0, 0.0]);
        assert!(
            FromTraction::new(vec![1.0])
                .apply(&ip, &mut dual, &[0.0; 2], &[0.0, 1.0])
                .is_err()
        );
    }
}
