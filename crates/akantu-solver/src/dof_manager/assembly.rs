// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/dof_manager.cc
// (assembleElementalArrayLocalArray :67-117, assembleElementalArrayToResidual
// :120-142, assembleElementalArrayToLumpedMatrix :145-167,
// assembleMatMulDOFsToResidual :170-179, splitSolutionPerDOFs /
// getLumpedMatrixPerDOFs :182-205, assembleToResidual / assembleToLumpedMatrix
// :208-224, assembleMatMulVectToGlobalArray / assembleMatMulVectToResidual
// :991-1011), dof_manager_inline_impl.hh (assembleMatMulVectToArray_
// :221-230), dof_manager_default.cc (assembleToGlobalArray :78-118,
// getArrayPerDOFs :208-237, assembleLumpedMatMulVectToResidual :240-257).

//! Vector assemblies of the DOF manager (DOF-007), matrix–vector
//! assemblies (DOF-010) and per-family extraction (DOF-011), in upstream
//! operation order (every accumulation a single binary64 operation in the
//! same sequence, so results are bit-identical to the oracle's).

use std::cell::RefCell;

use akantu_comm::phase::Severity;
use akantu_core::array::{ArrayView, ArrayViewMut};
use akantu_core::array_shared::ArrayRef;
use std::collections::BTreeMap;

use akantu_core::exec::{Tuples, TuplesRef};
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::Mesh;

use super::{DofData, DofManager, DofReduction, check_len};

/// A per-DOF array of a family used as the operand of a matrix–vector
/// assembly (the integration schemes multiply `M`/`C` by the family's
/// derivatives without copying them out of the manager).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyArray {
    /// The DOF values (derivative of order 0).
    Dofs,
    /// The derivative of the given order (1 = velocity, 2 = acceleration).
    Derivative(usize),
}

impl DofData {
    fn array(&self, src: FamilyArray) -> Result<ArrayRef<'_, Real>> {
        match src {
            FamilyArray::Dofs | FamilyArray::Derivative(0) => self.dofs.read(),
            FamilyArray::Derivative(o) => self
                .derivatives
                .get(&o)
                .ok_or_else(|| Error::not_found("dof derivative", format!("order {o}")))?
                .read(),
        }
    }
}

/// `r += a ∘ x` of one part (no-alias parameters, not inlined into the
/// part closure).
#[inline(never)]
fn lumped_mul_add_part(r: &mut [Real], a: &[Real], x: &[Real]) {
    for ((r, &ai), &xi) in r.iter_mut().zip(a).zip(x) {
        *r += ai * xi;
    }
}

/// `x = α · (b / a)` of one part (no-alias parameters).
#[inline(never)]
fn lumped_solve_part(alpha: Real, x: &mut [Real], a: &[Real], b: &[Real]) {
    for ((x, &ai), &bi) in x.iter_mut().zip(a).zip(b) {
        *x = alpha * (bi / ai);
    }
}

/// `t[j] += scale · values[inv[j]]` for the equations of one part that have
/// a DOF (no-alias parameters).
#[inline(never)]
fn inverse_scatter_part(inv: &[u32], values: &[Real], scale: Real, target: &mut [Real]) {
    for (t, &k) in target.iter_mut().zip(inv) {
        if k != u32::MAX {
            *t += scale * values[k as usize];
        }
    }
}

/// Equations per part of a threaded per-equation loop (D-123); as the
/// integration-scheme updates (short parts cost more than they save).
const EQUATIONS_PER_PART: usize = 262_144;

/// The inverse of a family's equation numbers (`inv[eq] = dof`), kept for
/// the equation-ordered scatter with the numbering generation it was built
/// for (and, as a further check, the length and address of the numbers);
/// `inv` is `None` when two DOFs share an equation.
#[derive(Debug, Default)]
pub(crate) struct EqInverse {
    generation: u64,
    eqs_len: usize,
    eqs_ptr: usize,
    len: usize,
    inv: Option<Vec<u32>>,
}

impl EqInverse {
    fn valid_for(&self, generation: u64, eqs: &[Idx], len: usize) -> bool {
        self.generation == generation
            && self.eqs_len == eqs.len()
            && self.eqs_ptr == eqs.as_ptr() as usize
            && self.len == len
    }

    fn build(generation: u64, eqs: &[Idx], len: usize) -> Self {
        let mut inv = vec![u32::MAX; len];
        let mut injective = u32::try_from(eqs.len()).is_ok();
        for (k, &eq) in eqs.iter().enumerate() {
            match usize::try_from(eq).ok().and_then(|e| inv.get_mut(e)) {
                Some(slot) if *slot == u32::MAX => *slot = k as u32,
                _ => {
                    injective = false;
                    break;
                }
            }
        }
        Self {
            generation,
            eqs_len: eqs.len(),
            eqs_ptr: eqs.as_ptr() as usize,
            len,
            inv: injective.then_some(inv),
        }
    }
}

impl DofManager {
    /// [`DofManager::assemble_to_global`] with more than one thread when
    /// every DOF of the family has its own equation (non-periodic): each
    /// equation then receives at most one addition, `target[e] += scale ·
    /// v[inv[e]]`, the serial operation, so the equations split over the
    /// threads (D-123). Otherwise, and on any invalid input (the serial
    /// routine reports it), the serial routine.
    #[allow(clippy::too_many_arguments)] // the executor, its cache and the routine's arguments
    pub(crate) fn scatter_to_global(
        exec: &akantu_core::exec::Exec,
        cache: &std::sync::Mutex<BTreeMap<akantu_core::Id, EqInverse>>,
        generation: u64,
        dof_id: &str,
        f: &DofData,
        periodic: bool,
        values: &[Real],
        target: &mut [Real],
        scale: Real,
    ) -> Result<()> {
        // threaded only when the equations really split into several parts
        // (a single part would pay the table lookup for nothing)
        let serial = exec.nb_parts(target.len(), EQUATIONS_PER_PART) <= 1
            || (f.support == akantu_core::enums::DofSupportType::Nodal && periodic)
            || values.len() != f.local_equation_numbers.len();
        if serial {
            return Self::assemble_to_global(f, periodic, values, target, scale);
        }
        let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
        let entry = cache.entry(dof_id.into()).or_default();
        if !entry.valid_for(generation, &f.local_equation_numbers, target.len()) {
            *entry = EqInverse::build(generation, &f.local_equation_numbers, target.len());
        }
        let Some(inv) = &entry.inv else {
            return Self::assemble_to_global(f, periodic, values, target, scale);
        };
        exec.for_each(Tuples::new(target, 1), EQUATIONS_PER_PART, |e0, mut t| {
            inverse_scatter_part(&inv[e0..], values, scale, t.as_mut_slice());
        });
        Ok(())
    }

    /// Upstream `DOFManagerDefault::assembleToGlobalArray`: `target[eq] +=
    /// scale · v` per DOF in DOF order; for a nodal family of a periodic
    /// mesh `target[eq] += scale · v · (not periodic slave)` (slaves alias
    /// their master's equation and contribute a signed zero).
    pub(crate) fn assemble_to_global(
        f: &DofData,
        periodic: bool,
        values: &[Real],
        target: &mut [Real],
        scale: Real,
    ) -> Result<()> {
        check_len(
            "assembleToGlobalArray (array to assemble)",
            values.len(),
            f.local_equation_numbers.len(),
        )?;
        let len = target.len();
        let bad = |eq: Idx| Error::OutOfRange {
            what: "local equation",
            index: usize::try_from(eq).unwrap_or(usize::MAX),
            len,
        };
        if f.support == akantu_core::enums::DofSupportType::Nodal && periodic {
            for (k, (&eq, &v)) in f.local_equation_numbers.iter().zip(values).enumerate() {
                let not_slave = if f.periodic_slave.get(k).copied().unwrap_or(false) {
                    0.0
                } else {
                    1.0
                };
                let t = usize::try_from(eq)
                    .ok()
                    .and_then(|e| target.get_mut(e))
                    .ok_or_else(|| bad(eq))?;
                *t += scale * v * not_slave;
            }
        } else {
            for (&eq, &v) in f.local_equation_numbers.iter().zip(values) {
                let t = usize::try_from(eq)
                    .ok()
                    .and_then(|e| target.get_mut(e))
                    .ok_or_else(|| bad(eq))?;
                *t += scale * v;
            }
        }
        Ok(())
    }

    /// Assemble a per-DOF array into a local-equation vector (upstream
    /// public `assembleToGlobalArray(dof_id, array, global, scale)`).
    pub fn assemble_to_global_array(
        &self,
        dof_id: &str,
        values: &[Real],
        target: &mut [Real],
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        check_len(
            "assembleToGlobalArray (global array)",
            target.len(),
            self.layout.nb_local(),
        )?;
        Self::assemble_to_global(
            self.family(dof_id)?,
            self.mesh_periodic,
            values,
            target,
            scale,
        )
    }

    /// Assemble a per-DOF array into the residual (upstream
    /// `assembleToResidual`).
    pub fn assemble_to_residual(
        &mut self,
        dof_id: &str,
        values: ArrayView<'_, Real>,
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        let f = self
            .families
            .get(dof_id)
            .ok_or_else(|| Error::not_found("dof family", dof_id))?;
        Self::scatter_to_global(
            &self.exec,
            &self.eq_inverse,
            self.numbering_generation,
            dof_id,
            f,
            self.mesh_periodic,
            values.as_slice(),
            self.residual.values_mut().as_mut_slice(),
            scale,
        )
    }

    /// Assemble a per-DOF array into a lumped matrix (upstream
    /// `assembleToLumpedMatrix`).
    pub fn assemble_to_lumped_matrix(
        &mut self,
        dof_id: &str,
        values: ArrayView<'_, Real>,
        lumped_id: &str,
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        let id = self.lumped_matrix_registry_id(lumped_id);
        let f = self
            .families
            .get(dof_id)
            .ok_or_else(|| Error::not_found("dof family", dof_id))?;
        let target = self
            .lumped
            .get_mut(lumped_id)
            .ok_or_else(|| Error::not_found("lumped matrix", id))?;
        Self::assemble_to_global(
            f,
            self.mesh_periodic,
            values.as_slice(),
            &mut target.values,
            scale,
        )
    }

    /// Extract a family's entries of a local-equation vector (upstream
    /// `getArrayPerDOFs`): `local` is resized to the family's DOF count
    /// (keeping its component count, which must divide it) and
    /// `local[k] = global[eq(k)]`. Local: no communication. Periodic slaves
    /// read their master's equation, so on one rank the result is
    /// consistent (there upstream's closing
    /// `makeConsistentForPeriodicity(_id)` only copies masters to slaves).
    ///
    /// On a distributed periodic mesh upstream's `makeConsistentForPeriodicity(_id)`
    /// (`dof_manager_default.cc:222-224`, `synchronizeArrayPBC`) also copies
    /// the owner's value to every periodic master copy, for **every**
    /// extracted vector. This function does not: the port makes the
    /// vectors it extracts consistent beforehand where they need it, i.e.
    /// the solution (`solve`, `set_solution`: owner → copies through the
    /// node and periodic synchronizers) and the distributed
    /// products (`mat_vec`, `assemble_mat_mul_*`: periodic channel of the
    /// reduction). A vector holding rank-local values at
    /// non-owned periodic master copies (a user vector, a lumped matrix
    /// assembled from an inconsistent nodal array) keeps them at those
    /// copies and their slaves, where upstream has the owner's value
    /// (DOF-011 Discrepancies).
    pub fn get_array_per_dofs(
        &self,
        dof_id: &str,
        global: &[Real],
        local: &mut Array<Real>,
    ) -> Result<()> {
        self.check_poison()?;
        check_len(
            "getArrayPerDOFs (global)",
            global.len(),
            self.layout.nb_local(),
        )?;
        let f = self.family(dof_id)?;
        let nbc = local.nb_component().max(1);
        let n = f.local_equation_numbers.len();
        if n % nbc != 0 {
            return Err(Error::DimensionMismatch {
                context: "getArrayPerDOFs (components)",
                expected: f.dofs.nb_component(),
                got: nbc,
            });
        }
        local.try_resize(n / nbc, 0.0)?;
        for (out, &eq) in local
            .as_mut_slice()
            .iter_mut()
            .zip(&f.local_equation_numbers)
        {
            *out = *usize::try_from(eq)
                .ok()
                .and_then(|e| global.get(e))
                .ok_or_else(|| Error::invalid("local equation outside the global array"))?;
        }
        Ok(())
    }

    /// A family's entries of a lumped matrix (upstream
    /// `getLumpedMatrixPerDOFs`).
    pub fn lumped_matrix_per_dofs(
        &self,
        dof_id: &str,
        lumped_id: &str,
        out: &mut Array<Real>,
    ) -> Result<()> {
        let v = &self.lumped_matrix(lumped_id)?.values;
        self.get_array_per_dofs(dof_id, v, out)
    }

    /// Scatter-add element vectors into a nodal array (upstream
    /// `assembleElementalArrayLocalArray`): for each element (of `filter`,
    /// else of the connectivity), for each node `n` and component `d`,
    /// `out[n][d] += scale · elementary[e][n·nb_dof + d]`, with
    /// `nb_dof = elementary.nb_component() / nodes_per_element`. All sizes
    /// and nodes are validated before `out` is touched.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn assemble_elemental_array_local_array(
        mesh: &Mesh,
        elementary: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        scale: Real,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        Self::assemble_elemental_array_local_view(
            mesh,
            elementary,
            out.view_mut(),
            ty,
            ghost,
            scale,
            filter,
        )
    }

    /// [`DofManager::assemble_elemental_array_local_array`] into a borrowed
    /// view (e.g. the guard of an exportable nodal array, D-047).
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn assemble_elemental_array_local_view(
        mesh: &Mesh,
        elementary: ArrayView<'_, Real>,
        mut out: ArrayViewMut<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        scale: Real,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        if mesh.nb_element(ty, ghost) == 0 && elementary.is_empty() {
            return Ok(());
        }
        let conn = mesh.connectivity(ty, ghost)?;
        let nnpe = conn.nb_component();
        let nbc = elementary.nb_component();
        if nnpe == 0 || nbc % nnpe != 0 {
            return Err(Error::DimensionMismatch {
                context: "assembleElementalArrayLocalArray (components per node)",
                expected: nnpe,
                got: nbc,
            });
        }
        let nb_dof = nbc / nnpe;
        check_len(
            "assembleElementalArrayLocalArray (array components)",
            out.nb_component(),
            nb_dof,
        )?;
        // The filter is validated in place (no copy of it), the
        // scatter loops have a compile-time component count for 1…3
        // components; same checks in the same order, same additions
        if let Some(fl) = filter {
            for &e in fl {
                if usize::try_from(e).map_or(true, |u| u >= conn.len()) {
                    return Err(Error::OutOfRange {
                        what: "element",
                        index: usize::try_from(e).unwrap_or(usize::MAX),
                        len: conn.len(),
                    });
                }
            }
        }
        let nb_elements = filter.map_or(conn.len(), <[Idx]>::len);
        check_len(
            "assembleElementalArrayLocalArray (elements)",
            elementary.len(),
            nb_elements,
        )?;
        // element of position k (filter entries validated above)
        let element = |k: usize| match filter {
            Some(fl) => usize::try_from(fl[k]).unwrap_or(0),
            None => k,
        };
        let nb_nodes = out.len();
        for k in 0..nb_elements {
            for &n in conn.tuple(element(k)) {
                if usize::try_from(n).map_or(true, |u| u >= nb_nodes) {
                    return Err(Error::OutOfRange {
                        what: "node",
                        index: usize::try_from(n).unwrap_or(usize::MAX),
                        len: nb_nodes,
                    });
                }
            }
        }
        let (o, ev, cs) = (out.as_mut_slice(), elementary.as_slice(), conn.as_slice());
        match nb_dof {
            1 => scatter_add::<1>(o, ev, cs, nnpe, nb_elements, element, scale),
            2 => scatter_add::<2>(o, ev, cs, nnpe, nb_elements, element, scale),
            3 => scatter_add::<3>(o, ev, cs, nnpe, nb_elements, element, scale),
            _ => {
                for k in 0..nb_elements {
                    let el = elementary.tuple(k);
                    for (i, &n) in conn.tuple(element(k)).iter().enumerate() {
                        let base = usize::try_from(n).unwrap_or(0) * nb_dof;
                        for d in 0..nb_dof {
                            o[base + d] += scale * el[i * nb_dof + d];
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn elemental_to_local(
        mesh: &Mesh,
        elementary: ArrayView<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        scale: Real,
        filter: Option<&[Idx]>,
    ) -> Result<Array<Real>> {
        let nnpe = if mesh.nb_element(ty, ghost) == 0 {
            ty.nb_nodes_per_element()
        } else {
            mesh.connectivity(ty, ghost)?.nb_component()
        };
        let nb_dof = elementary.nb_component().checked_div(nnpe).unwrap_or(0);
        let mut local = Array::try_zeros(mesh.nb_nodes(), nb_dof.max(1))?;
        Self::assemble_elemental_array_local_array(
            mesh, elementary, &mut local, ty, ghost, scale, filter,
        )?;
        Ok(local)
    }

    /// Element vectors → residual (upstream
    /// `assembleElementalArrayToResidual`): assembled into a zeroed nodal
    /// array (`nb_nodes × nb_dof`), then added to the residual with scale 1.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn assemble_elemental_array_to_residual(
        &mut self,
        mesh: &Mesh,
        dof_id: &str,
        elementary: ArrayView<'_, Real>,
        ty: ElementType,
        ghost: GhostType,
        scale: Real,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_poison()?;
        let local = Self::elemental_to_local(mesh, elementary, ty, ghost, scale, filter)?;
        self.assemble_to_residual(dof_id, local.view(), 1.0)
    }

    /// Element vectors → lumped matrix (upstream
    /// `assembleElementalArrayToLumpedMatrix`).
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn assemble_elemental_array_to_lumped_matrix(
        &mut self,
        mesh: &Mesh,
        dof_id: &str,
        elementary: ArrayView<'_, Real>,
        lumped_id: &str,
        ty: ElementType,
        ghost: GhostType,
        scale: Real,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_poison()?;
        let local = Self::elemental_to_local(mesh, elementary, ty, ghost, scale, filter)?;
        self.assemble_to_lumped_matrix(dof_id, local.view(), lumped_id, 1.0)
    }

    /// The upstream `data_cache` of a matrix–vector assembly: a zeroed
    /// local-equation vector with `x` assembled at scale 1.
    fn mat_mul_cache(&self, dof_id: &str, x: &[Real], scale: Real) -> Result<Vec<Real>> {
        let mut cache = vec![0.0; self.layout.nb_local()];
        Self::scatter_to_global(
            &self.exec,
            &self.eq_inverse,
            self.numbering_generation,
            dof_id,
            self.family(dof_id)?,
            self.mesh_periodic,
            x,
            &mut cache,
            scale,
        )?;
        Ok(cache)
    }

    /// `y = beta·y + alpha·A·cache` as one collective operation: the local
    /// preparation (`prepared`: the cache, or the error of building it) and
    /// the matrix are agreed before the distributed reduction (node and,
    /// on a distributed periodic mesh, periodic synchronizers of the last
    /// numbering: UD-003-corrected product, UD-872).
    fn mat_vec_collective(
        &mut self,
        name: &str,
        a_id: &str,
        prepared: Result<Vec<Real>>,
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        self.collective(name, |dm| {
            let pre = prepared.and_then(|c| {
                check_len("mat_vec (y)", y.len(), dm.layout.nb_local())?;
                dm.check_matrix(a_id)?;
                Ok(c)
            });
            dm.lend(|dm, ph| {
                let cache = ph.agreed("prepare", pre, Severity::Recoverable)?;
                let dm: &DofManager = dm;
                let red = DofReduction {
                    dm,
                    sync: dm.node_sync.as_ref(),
                    periodic: dm.periodic_sync.as_ref(),
                    phases: RefCell::new(ph),
                };
                dm.backend
                    .mat_vec(a_id, &dm.layout, &red, &cache, y, alpha, beta)
            })
        })
    }

    /// `residual += scale · A · x` (upstream `assembleMatMulVectToResidual`:
    /// `x` assembled into the zeroed cache at scale 1, then
    /// `A.matVecMul(cache, residual, scale, 1)`). Collective.
    pub fn assemble_mat_mul_vect_to_residual(
        &mut self,
        dof_id: &str,
        a_id: &str,
        x: &[Real],
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        let prepared = self.mat_mul_cache(dof_id, x, 1.0);
        // the values are copied out and back in place (the residual's
        // storage stays where an exported view sees it, PY-045)
        let mut r = self.residual.values().to_vec();
        let out = self.mat_vec_collective(
            &format!("mat_mul_to_residual:{a_id}"),
            a_id,
            prepared,
            &mut r,
            scale,
            1.0,
        );
        self.residual
            .values_mut()
            .as_mut_slice()
            .copy_from_slice(&r);
        out
    }

    /// [`DofManager::assemble_mat_mul_vect_to_residual`] with a family
    /// array of `dof_id` as `x` (used by the integration schemes:
    /// `r -= M·ü`, `r -= C·u̇`).
    pub fn assemble_mat_mul_family_to_residual(
        &mut self,
        dof_id: &str,
        src: FamilyArray,
        a_id: &str,
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        let prepared = self
            .family(dof_id)
            .and_then(|f| f.array(src))
            .and_then(|x| self.mat_mul_cache(dof_id, x.as_slice(), 1.0));
        // the values are copied out and back in place (the residual's
        // storage stays where an exported view sees it, PY-045)
        let mut r = self.residual.values().to_vec();
        let out = self.mat_vec_collective(
            &format!("mat_mul_to_residual:{a_id}"),
            a_id,
            prepared,
            &mut r,
            scale,
            1.0,
        );
        self.residual
            .values_mut()
            .as_mut_slice()
            .copy_from_slice(&r);
        out
    }

    /// `out = (scale · A · x)` restricted to the family (upstream
    /// `assembleMatMulVectToArray`: a zeroed temporary receives
    /// `A.matVecMul(cache, tmp, scale, 1)`, then `getArrayPerDOFs`).
    /// Collective.
    pub fn assemble_mat_mul_vect_to_array(
        &mut self,
        dof_id: &str,
        a_id: &str,
        x: &[Real],
        out: &mut Array<Real>,
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        let prepared = self.mat_mul_cache(dof_id, x, 1.0);
        let mut tmp = vec![0.0; self.layout.nb_local()];
        self.mat_vec_collective(
            &format!("mat_mul_to_array:{a_id}"),
            a_id,
            prepared,
            &mut tmp,
            scale,
            1.0,
        )?;
        self.get_array_per_dofs(dof_id, &tmp, out)
    }

    /// `residual += A ∘ (scale · x)` with a lumped matrix (upstream
    /// `assembleLumpedMatMulVectToResidual`: `x` assembled into the zeroed
    /// cache **at scale `scale`**, then `r += A · cache` over every local
    /// equation). A block-lumped matrix then adds its couplings
    /// ([`super::LumpedBlockCouplings`], D-059). Local.
    pub fn assemble_lumped_mat_mul_vect_to_residual(
        &mut self,
        dof_id: &str,
        a_id: &str,
        x: &[Real],
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        let cache = self.mat_mul_cache(dof_id, x, scale)?;
        self.lumped_times_cache(a_id, &cache)
    }

    /// [`DofManager::assemble_lumped_mat_mul_vect_to_residual`] with a
    /// family array of `dof_id` as `x`.
    pub fn assemble_lumped_mat_mul_family_to_residual(
        &mut self,
        dof_id: &str,
        src: FamilyArray,
        a_id: &str,
        scale: Real,
    ) -> Result<()> {
        self.check_poison()?;
        let f = self.family(dof_id)?;
        let cache = self.mat_mul_cache(dof_id, f.array(src)?.as_slice(), scale)?;
        self.lumped_times_cache(a_id, &cache)
    }

    fn lumped_times_cache(&mut self, a_id: &str, cache: &[Real]) -> Result<()> {
        let id = self.lumped_matrix_registry_id(a_id);
        let a = self
            .lumped
            .get(a_id)
            .ok_or_else(|| Error::not_found("lumped matrix", id))?;
        check_len(
            "assembleLumpedMatMulVectToResidual (lumped)",
            a.values.len(),
            cache.len(),
        )?;
        check_len(
            "assembleLumpedMatMulVectToResidual (residual)",
            self.residual.local_size(),
            cache.len(),
        )?;
        let mut residual = self.residual.values_mut();
        // every equation from its own entries: contiguous parts on the
        // threads (D-123)
        let n = (residual.as_slice().len())
            .min(a.values.len())
            .min(cache.len());
        let items = (
            Tuples::new(&mut residual.as_mut_slice()[..n], 1),
            TuplesRef::new(&a.values[..n], 1),
            TuplesRef::new(&cache[..n], 1),
        );
        self.exec
            .for_each(items, EQUATIONS_PER_PART, |_, (mut r, a, x)| {
                lumped_mul_add_part(r.as_mut_slice(), a.as_slice(), x.as_slice());
            });
        drop(residual);
        // symmetric 3×3 couplings of a block-lumped matrix (D-059)
        self.add_lumped_coupling_product(a_id, cache)
    }

    /// `residual += scale · A · dofs` for every family in lexicographic id
    /// order (upstream `assembleMatMulDOFsToResidual`, iterating its
    /// `std::map`). Collective.
    pub fn assemble_mat_mul_dofs_to_residual(&mut self, a_id: &str, scale: Real) -> Result<()> {
        let ids: Vec<akantu_core::Id> = self.families.keys().cloned().collect();
        for id in ids {
            self.assemble_mat_mul_family_to_residual(&id, FamilyArray::Dofs, a_id, scale)?;
        }
        Ok(())
    }

    /// Lumped solve of the lumped non-linear solver (upstream
    /// `NonLinearSolverLumped::solveLumped`): `x = alpha · (b / A)` over
    /// every local equation, into the manager's solution, then
    /// `splitSolutionPerDOFs`. A block-lumped matrix solves the free
    /// sub-block of every coupled block in between (blocked flags of the
    /// last snapshot; [`super::LumpedBlockCouplings`], D-059). Local.
    pub fn solve_lumped(&mut self, lumped_id: &str, alpha: Real) -> Result<()> {
        self.check_poison()?;
        let n = self.layout.nb_local();
        let id = self.lumped_matrix_registry_id(lumped_id);
        let a = self
            .lumped
            .get(lumped_id)
            .ok_or_else(|| Error::not_found("lumped matrix", id))?;
        check_len("solveLumped (lumped)", a.values.len(), n)?;
        check_len("solveLumped (residual)", self.residual.local_size(), n)?;
        self.solution.clear();
        self.solution.resize(n, 0.0);
        let residual = self.residual.values();
        let m = n.min(a.values.len()).min(residual.as_slice().len());
        let items = (
            Tuples::new(&mut self.solution[..m], 1),
            TuplesRef::new(&a.values[..m], 1),
            TuplesRef::new(&residual.as_slice()[..m], 1),
        );
        self.exec
            .for_each(items, EQUATIONS_PER_PART, |_, (mut x, a, b)| {
                lumped_solve_part(alpha, x.as_mut_slice(), a.as_slice(), b.as_slice());
            });
        drop(residual);
        // free sub-blocks of a block-lumped matrix (D-059)
        self.solve_lumped_free_blocks(lumped_id, alpha)?;
        let sol = std::mem::take(&mut self.solution);
        let r = self.split_solution_per_dofs(&sol);
        self.solution = sol;
        r
    }
}

/// `out[n][d] += scale · elementary[k][i·D + d]` for every node `n` = the
/// `i`-th node of element `element(k)` (validated by the caller), with a
/// compile-time component count `D` (the loop of
/// `assemble_elemental_array_local_array`).
#[inline(always)]
fn scatter_add<const D: usize>(
    out: &mut [Real],
    elementary: &[Real],
    conn: &[Idx],
    nnpe: usize,
    nb_elements: usize,
    element: impl Fn(usize) -> usize,
    scale: Real,
) {
    let nbc = nnpe * D;
    for k in 0..nb_elements {
        let el = &elementary[k * nbc..(k + 1) * nbc];
        let e = element(k);
        for (i, &n) in conn[e * nnpe..(e + 1) * nnpe].iter().enumerate() {
            let base = usize::try_from(n).unwrap_or(0) * D;
            let dst = &mut out[base..base + D];
            for d in 0..D {
                dst[d] += scale * el[i * D + d];
            }
        }
    }
}
