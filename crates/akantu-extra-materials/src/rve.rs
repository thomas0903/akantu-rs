// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_FE2/solid_mechanics_model_RVE.hh, solid_mechanics_model_RVE.cc.

//! `SolidMechanicsModelRVE` and `GelMaterialSelector` (XMAT-015): a 2-D
//! representative volume element for FE² and alkali–silica-reaction (ASR)
//! studies, a `SolidMechanicsModel` with:
//!
//! * **corner nodes** (Leonardo's thesis numbering, `corner_nodes()`): the
//!   nodes at the bounding-box corners `(x_min, y_min)`, `(x_max, y_min)`,
//!   `(x_max, y_max)`, `(x_min, y_max)` (`Math::are_float_equal`, distance
//!   to the lower corner for the first), an error if one is missing; they
//!   are removed from the node groups of the element groups `top`, `left`,
//!   `bottom`, `right` (corners are not periodic). The node sets of
//!   `bottom`/`left` are kept for the (commented-out upstream) periodic
//!   conditions;
//! * always a **static** analysis; the volume `V = Σ_types ∫ 1` (UD-585);
//! * `apply_boundary_conditions(H)`: the four corners blocked at `u = H·x`
//!   (corner 3, then 1, 2, 4); `apply_homogeneous_temperature(T)`:
//!   `delta_T = T` at every point of every law with elements;
//! * `average_tensor_field(i, j, field)`: `Σ_types Σ_laws Σ_elements ∫ f /
//!   V` with `f` the component of flat index `i·dim + j` of the
//!   column-major tuple of `stress`, `grad_u` (symmetrised: `½(f_{i·d+j} +
//!   f_{j·d+i})`, "strain") or `eigen_grad_u` (upstream's index order kept:
//!   the `(i, j)` request reads the column-major entry `(j, i)`);
//! * `homogenize_stiffness()`: the gel eigenstrain cleared, three virtual
//!   tests `H = 0.01·e₁⊗e₁`, `0.01·e₂⊗e₂`, `0.01·e₁⊗e₂` (Newton–Raphson,
//!   at most 2 iterations, threshold 1e-6 on the solution increment), Voigt
//!   averages `(σ̄₁₁, σ̄₂₂, σ̄₂₁)` and `(ε̄₁₁, ε̄₂₂, 2ε̄₂₁)` as columns,
//!   `C = Σ̄·Ē⁻¹` (the commented-out crack fill/drain is not applied);
//! * `homogenize_eigen_grad_u()`: the four averages of `eigen_grad_u`;
//! * `advance_asr(ε*)`: gel eigenstrain `ε*`, then solve and damage until no
//!   point is damaged: the law with the larger maximum equivalent stress
//!   among `materials[0]` (aggregates) and `materials[1]` (paste) is
//!   damaged (both when equal). Both must be `MaterialDamageIterative<2>`
//!   or derived from it (upstream `dynamic_cast<MaterialDamageIterative<2>
//!   &>`: `damage_iterative`, `iterative_stiffness_reduction`,
//!   `damage_iterative_non_local`, whose own `updateDamage` runs); other
//!   laws (e.g. `orthotropic_damage_iterative`, upstream `std::bad_cast`)
//!   are refused;
//! * the RVE laws do not all-reduce (upstream `dynamic_cast` to the RVE);
//!   `material_FE2` sections of the input create inert laws (upstream skips
//!   their `initMaterial` in the RVE).
//!
//! [`GelMaterialSelector`]: the law of an element by its `physical_names`
//! (paste = law 1 kept), `nb_gel_pockets` pseudo-random `_triangle_3`
//! elements outside the paste get the `gel` law, every other element law 0.
//! The pockets are drawn with the C library generator (`srand(0)`,
//! `rand() % nb_element`, [`crate::c_rand`]).
//!
//! # Upstream defects
//!
//! * **UD-585** (B): the volume loop assigns per type
//!   ([`ExtraLawOptions::rve_volume_sum_over_types`]); `homogenizeStiffness`
//!   fails without a `gel` law ([`ExtraLawOptions::rve_optional_gel`]).
//! * **UD-586** (C): `rand()` (platform dependent; the port reproduces
//!   glibc, the oracle platform's), the selector's barycenters and box size
//!   are computed but unused (not computed here), the per-pocket counter
//!   print is not reproduced, an RVE with fewer non-paste elements than
//!   pockets is an error (upstream loops forever), dumps (`initFull`,
//!   every 10th `advanceASR`) are not written.

use std::collections::BTreeSet;
use std::sync::Arc;

use akantu_core::array::ArrayView;
use akantu_core::enums::AnalysisMethod;
use akantu_core::math::{DEFAULT_TOLERANCE, are_float_equal};
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt};
use akantu_core::{
    Array, Element, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter, UInt,
};
use akantu_mesh::{Mesh, MeshDataValues, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, InternalFieldMut, LawPhase, LawSpec, ModelServices, SelectorContext,
};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solid::{Material, SolidMechanicsModel};

use crate::c_rand::CRand;
use crate::damage_iterative::MaterialDamageIterative;
use crate::iterative_stiffness_reduction::MaterialIterativeStiffnessReduction;
use crate::non_local::MaterialDamageIterativeNonLocal;
use crate::options::ExtraLawOptions;

const NG: GhostType = GhostType::NotGhost;
const DIM: usize = 2;

/// Upstream `GelMaterialSelector` (module docs).
#[derive(Debug, Clone)]
pub struct GelMaterialSelector {
    gel_index: usize,
    gel_pockets: BTreeSet<Idx>,
}

impl GelMaterialSelector {
    /// Draw the gel pockets: `srand(0)`, then `rand() % nb_element` until
    /// `nb_gel_pockets` distinct `_triangle_3` elements whose
    /// `physical_names` law is not 1 (paste) are found.
    ///
    /// # Errors
    /// No `gel` law, or fewer candidate elements than pockets.
    pub fn new(
        mesh: &Mesh,
        law_by_name: &dyn Fn(&str) -> Option<usize>,
        nb_gel_pockets: UInt,
    ) -> Result<Self> {
        let gel_index = law_by_name("gel").ok_or_else(|| Error::not_found("material", "gel"))?;
        let ty = ElementType::Triangle3;
        let nb_element = mesh.nb_element(ty, NG);
        let mut rng = CRand::new(0);
        let mut checked = BTreeSet::new();
        let mut gel_pockets = BTreeSet::new();
        let mut placed: UInt = 0;
        while placed != nb_gel_pockets {
            if checked.len() >= nb_element {
                return Err(Error::invalid(format!(
                    "GelMaterialSelector: only {placed} of {nb_gel_pockets} gel pockets fit outside \
                     the paste (UD-586: upstream loops forever)"
                )));
            }
            let id = rng.next_value() as usize % nb_element;
            if !checked.insert(id) {
                continue;
            }
            let el = Element::new(
                ty,
                Idx::try_from(id).map_err(|_| Error::invalid("element index"))?,
                NG,
            );
            if physical_law(mesh, law_by_name, el) == 1 {
                continue; // element belongs to the paste
            }
            gel_pockets.insert(el.index());
            placed += 1;
        }
        Ok(Self {
            gel_index,
            gel_pockets,
        })
    }

    /// The pocket elements (`_triangle_3`, not ghost).
    #[must_use]
    pub fn gel_pockets(&self) -> Vec<Idx> {
        self.gel_pockets.iter().copied().collect()
    }
}

/// `MeshDataMaterialSelector<std::string>("physical_names")` with the
/// default fallback (law 0).
fn physical_law(mesh: &Mesh, law_by_name: &dyn Fn(&str) -> Option<usize>, el: Element) -> usize {
    if let Ok(MeshDataValues::String(m)) = mesh.data().elemental("physical_names")
        && let Some(name) = m.element(el).ok().and_then(|v| v.first())
        && let Some(i) = law_by_name(name)
    {
        return i;
    }
    0
}

impl ConstitutiveLawSelector for GelMaterialSelector {
    fn select(&self, ctx: &SelectorContext<'_>, element: Element) -> Result<Option<usize>> {
        let temp = physical_law(ctx.mesh, ctx.law_by_name, element);
        if temp == 1 {
            return Ok(Some(1));
        }
        if element.ty() == ElementType::Triangle3
            && element.ghost_type() == NG
            && self.gel_pockets.contains(&element.index())
        {
            return Ok(Some(self.gel_index));
        }
        Ok(Some(0))
    }
}

/// Upstream `SolidMechanicsModelRVE` (module docs).
#[derive(Debug)]
pub struct SolidMechanicsModelRve {
    /// The underlying solid model.
    pub model: SolidMechanicsModel,
    /// Defect switches (UD-585).
    pub extra_options: ExtraLawOptions,
    volume: Real,
    corner_nodes: [Idx; 4],
    bottom_nodes: BTreeSet<Idx>,
    left_nodes: BTreeSet<Idx>,
    use_rve_mat_selector: bool,
    nb_gel_pockets: UInt,
}

impl SolidMechanicsModelRve {
    /// Upstream constructor: corner nodes, group edits, then the model on
    /// `mesh` with the input `parser` (the extra materials registered with
    /// the corrected defaults; `material_FE2` sections allocate inert
    /// laws).
    ///
    /// # Errors
    /// A mesh that is not 2-D, a missing corner or group, model errors.
    pub fn new(
        mesh: Mesh,
        use_rve_mat_selector: bool,
        nb_gel_pockets: UInt,
        parser: Parser,
        id: &str,
    ) -> Result<Self> {
        Self::new_with_options(
            mesh,
            use_rve_mat_selector,
            nb_gel_pockets,
            parser,
            id,
            ExtraLawOptions::default(),
        )
    }

    /// [`SolidMechanicsModelRve::new`] with the upstream-defect switches
    /// `options`, used by the RVE itself (UD-585) and by every law its
    /// model creates (registered with
    /// [`crate::register_extra_materials_with`]).
    ///
    /// # Errors
    /// As [`SolidMechanicsModelRve::new`].
    pub fn new_with_options(
        mut mesh: Mesh,
        use_rve_mat_selector: bool,
        nb_gel_pockets: UInt,
        parser: Parser,
        id: &str,
        options: ExtraLawOptions,
    ) -> Result<Self> {
        if mesh.spatial_dimension() != DIM {
            return Err(Error::Unsupported(
                "SolidMechanicsModelRVE: This is 2D only!".into(),
            ));
        }
        let corner_nodes = find_corner_nodes(&mesh, DEFAULT_TOLERANCE)?;
        {
            let groups = mesh.groups_mut();
            for (group, corners) in [
                ("top", [2, 3]),
                ("left", [3, 0]),
                ("bottom", [1, 0]),
                ("right", [2, 1]),
            ] {
                let mut g = groups.element_group_mut(group)?;
                for c in corners {
                    g.remove_node(corner_nodes[c]);
                }
            }
        }
        let bottom_nodes = mesh
            .groups()
            .element_group("bottom")?
            .nodes()
            .iter()
            .copied()
            .collect();
        let left_nodes = mesh
            .groups()
            .element_group("left")?
            .nodes()
            .iter()
            .copied()
            .collect();
        let mut model = SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, id, parser)?;
        crate::register_extra_materials_with(&mut model.physics.factory, options)?;
        model.physics.factory.register(
            "material_FE2",
            Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
                let mesh = Some(services.mesh());
                akantu_core::with_dim!(spec.spatial_dimension, D => {
                    Box::new(crate::fe2::MaterialFE2::<D>::nested(spec, mesh)?) as Box<dyn Material>
                })
            }),
        )?;
        Ok(Self {
            model,
            extra_options: options,
            volume: 0.,
            corner_nodes,
            bottom_nodes,
            left_nodes,
            use_rve_mat_selector,
            nb_gel_pockets,
        })
    }

    /// The corner nodes 1…4.
    #[must_use]
    pub fn corner_nodes(&self) -> [Idx; 4] {
        self.corner_nodes
    }

    /// The RVE volume.
    #[must_use]
    pub fn volume(&self) -> Real {
        self.volume
    }

    /// `bottom` / `left` nodes without the corners.
    #[must_use]
    pub fn boundary_node_sets(&self) -> (&BTreeSet<Idx>, &BTreeSet<Idx>) {
        (&self.bottom_nodes, &self.left_nodes)
    }

    /// Upstream `initFull` (always static) with the overridden
    /// `initMaterials` (gel selector when requested), then the volume.
    ///
    /// # Errors
    /// Solver, law or selector errors.
    pub fn init_full(&mut self) -> Result<()> {
        let options = ModelOptions::new(AnalysisMethod::Static);
        self.model.init_method_and_solver(&options)?;
        let pre = self.model.instantiate_materials()?.unwrap_or(Ok(()));
        if pre.is_ok() && self.use_rve_mat_selector {
            let names: Vec<String> = (0..self.model.nb_materials())
                .map(|i| self.model.material(i).map(|m| m.base().name.clone()))
                .collect::<Result<_>>()?;
            let by_name = |n: &str| names.iter().position(|x| x == n);
            let selector = {
                let mesh = self.model.physics.base.mesh.read()?;
                GelMaterialSelector::new(&mesh, &by_name, self.nb_gel_pockets)?
            };
            self.model.set_material_selector(Box::new(selector));
        }
        self.model.init_materials_after(pre)?;
        for i in 0..self.model.nb_materials() {
            if let Some(it) = crate::as_iterative_damage_mut(self.model.material_mut(i)?) {
                it.set_rve_mode(true);
            }
        }
        self.volume = self.compute_volume()?;
        Ok(())
    }

    fn compute_volume(&self) -> Result<Real> {
        let mesh = self.model.physics.base.mesh.read()?;
        let fe = self.model.physics.base.fems.get("")?;
        let mut volume = 0.;
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: Some(DIM),
            ghost_type: NG,
            kind: None,
        }) {
            let n = mesh.nb_element(ty, NG) * fe.nb_integration_points(ty, NG)?;
            let ones = vec![1.; n];
            let v = fe.integrate_scalar(ArrayView::new(&ones, 1)?, ty, NG, None)?;
            if self.extra_options.rve_volume_sum_over_types {
                volume += v;
            } else {
                volume = v; // UD-585: upstream assigns per type
            }
        }
        Ok(volume)
    }

    /// Upstream `applyBoundaryConditions(H)`.
    ///
    /// # Errors
    /// Missing nodal arrays (model not initialised).
    pub fn apply_boundary_conditions(&mut self, h: &Matrix<2, 2>) -> Result<()> {
        let pos: Vec<Real> = self
            .model
            .physics
            .base
            .mesh
            .read()?
            .nodes()
            .as_slice()
            .to_vec();
        for c in [2, 0, 1, 3] {
            let node =
                usize::try_from(self.corner_nodes[c]).map_err(|_| Error::invalid("corner"))?;
            let x = [pos[2 * node], pos[2 * node + 1]];
            let u = [
                h[0][0] * x[0] + h[1][0] * x[1],
                h[0][1] * x[0] + h[1][1] * x[1],
            ];
            {
                let mut b = self.model.blocked_dofs_mut()?;
                b.as_mut_slice()[2 * node] = true;
                b.as_mut_slice()[2 * node + 1] = true;
            }
            let mut d = self.model.displacement_mut()?;
            d.as_mut_slice()[2 * node] = u[0];
            d.as_mut_slice()[2 * node + 1] = u[1];
        }
        Ok(())
    }

    /// Upstream `applyHomogeneousTemperature(T)`.
    ///
    /// # Errors
    /// Laws without a `delta_T` internal on a type with elements.
    pub fn apply_homogeneous_temperature(&mut self, temperature: Real) -> Result<()> {
        for m in 0..self.model.nb_materials() {
            let law = self.model.material_mut(m)?;
            let types: Vec<ElementType> = law
                .base()
                .filter_types(NG)
                .into_iter()
                .filter(|&t| {
                    law.base()
                        .element_filter
                        .get(t, NG)
                        .is_ok_and(|f| !f.is_empty())
                })
                .collect();
            if types.is_empty() {
                continue;
            }
            let mut found = false;
            law.for_each_internal_mut(&mut |n, f| {
                if n == "delta_T"
                    && let InternalFieldMut::Real(f) = f
                {
                    found = true;
                    for &t in &types {
                        f.get_mut(t, NG)?.as_mut_slice().fill(temperature);
                    }
                }
                Ok(())
            })?;
            if !found {
                return Err(Error::not_found("internal", "delta_T"));
            }
        }
        Ok(())
    }

    /// Set the Newton–Raphson parameters of the RVE solves (upstream:
    /// `max_iterations = 2`, `threshold = 1e-6`, `_solution`).
    fn set_rve_solver(&mut self) -> Result<()> {
        let nls = self.model.solver.non_linear_solver_mut(None)?;
        nls.set_param("max_iterations", ParamValue::Int(2))?;
        nls.set_param("threshold", ParamValue::Real(1e-6))?;
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
    }

    /// Set the `eigen_grad_u` of every point of the `gel` law.
    fn set_gel_eigen_grad_u(&mut self, m: &Matrix<2, 2>, optional: bool) -> Result<()> {
        let Ok(gel) = self.model.material_index("gel") else {
            if optional {
                return Ok(());
            }
            return Err(Error::not_found("material", "gel"));
        };
        let law = self.model.material_mut(gel)?;
        for ty in law.base().filter_types(NG) {
            let mut a = law.base_mut().eigen_grad_u.get_mut(ty, NG)?;
            for t in a.as_mut_slice().as_chunks_mut::<4>().0 {
                t.copy_from_slice(m.as_flattened());
            }
        }
        Ok(())
    }

    /// Upstream `advanceASR(prestrain)`.
    ///
    /// # Errors
    /// No `gel` law, laws 0/1 not of the `MaterialDamageIterative<2>`
    /// family, solver errors.
    pub fn advance_asr(&mut self, prestrain: &Matrix<2, 2>) -> Result<()> {
        self.set_gel_eigen_grad_u(prestrain, false)?;
        for i in [0, 1] {
            let law = self.model.material(i)?;
            if !is_damage_iterative_2d(law) {
                return Err(Error::invalid(format!(
                    "SolidMechanicsModelRVE::advanceASR: material {i} ('{}') is not a \
                     MaterialDamageIterative<2> (damage_iterative, iterative_stiffness_reduction \
                     or damage_iterative_non_local; upstream dynamic_cast throws std::bad_cast)",
                    law.base().law_type
                )));
            }
        }
        self.set_rve_solver()?;
        loop {
            self.model.solve_step(None)?;
            let max = |s: &Self, i: usize| -> Result<Real> {
                Ok(crate::as_iterative_damage(s.model.material(i)?)
                    .map(crate::IterativeDamage::norm_max_equivalent_stress)
                    .unwrap_or(0.))
            };
            let (agg, paste) = (max(self, 0)?, max(self, 1)?);
            let update = |s: &mut Self, i: usize| -> Result<UInt> {
                s.model
                    .with_material(i, LawPhase::AfterSolveStep, |law, ctx| {
                        let it = crate::as_iterative_damage_mut(law)
                            .ok_or_else(|| Error::invalid("not an iterative damage law"))?;
                        it.update_damage(ctx)
                    })
            };
            let nb = if agg > paste {
                update(self, 0)?
            } else if agg < paste {
                update(self, 1)?
            } else {
                update(self, 1)? + update(self, 0)?
            };
            if nb == 0 {
                break;
            }
        }
        Ok(())
    }

    /// Upstream `averageTensorField(row, col, field)`.
    ///
    /// # Errors
    /// An unknown field (upstream: "Averaging not implemented for this
    /// field!!!") or integration errors.
    pub fn average_tensor_field(&self, row: usize, col: usize, field: &str) -> Result<Real> {
        if !matches!(field, "stress" | "strain" | "eigen_grad_u") || row >= DIM || col >= DIM {
            return Err(Error::invalid(format!(
                "Averaging not implemented for this field!!! ({field}, {row}, {col})"
            )));
        }
        let mesh = self.model.physics.base.mesh.read()?;
        let fe = self.model.physics.base.fems.get("")?;
        let n = DIM * DIM;
        let mut average = 0.;
        for ty in mesh.element_types(TypeFilter {
            spatial_dimension: Some(DIM),
            ghost_type: NG,
            kind: None,
        }) {
            for m in 0..self.model.nb_materials() {
                let law = self.model.material(m)?;
                let base = law.base();
                let Ok(filter) = base.element_filter.get(ty, NG) else {
                    continue;
                };
                if filter.is_empty() {
                    continue;
                }
                let values = match field {
                    "stress" => base.stress.get(ty, NG)?,
                    "strain" => base.grad_u.get(ty, NG)?,
                    _ => base.eigen_grad_u.get(ty, NG)?,
                };
                let mut int = Array::new(n);
                fe.integrate(values.view(), &mut int, n, ty, NG, Some(filter.as_slice()))?;
                for k in 0..filter.len() {
                    let t = &int.as_slice()[k * n..(k + 1) * n];
                    average += if field == "strain" {
                        0.5 * (t[row * DIM + col] + t[col * DIM + row])
                    } else {
                        t[row * DIM + col]
                    };
                }
            }
        }
        Ok(average / self.volume)
    }

    /// Upstream `performVirtualTesting(H, stresses, strains, test_no)`.
    fn perform_virtual_testing(
        &mut self,
        h: &Matrix<2, 2>,
        stresses: &mut Matrix<3, 3>,
        strains: &mut Matrix<3, 3>,
        test_no: usize,
    ) -> Result<()> {
        self.apply_boundary_conditions(h)?;
        self.set_rve_solver()?;
        self.model.solve_step(None)?;
        stresses[test_no][0] = self.average_tensor_field(0, 0, "stress")?;
        strains[test_no][0] = self.average_tensor_field(0, 0, "strain")?;
        stresses[test_no][1] = self.average_tensor_field(1, 1, "stress")?;
        strains[test_no][1] = self.average_tensor_field(1, 1, "strain")?;
        stresses[test_no][2] = self.average_tensor_field(1, 0, "stress")?;
        strains[test_no][2] = 2. * self.average_tensor_field(1, 0, "strain")?;
        Ok(())
    }

    /// Upstream `homogenizeStiffness(C)`: the effective Voigt stiffness.
    ///
    /// # Errors
    /// No `gel` law in compat mode (UD-585), solver errors, a singular
    /// strain matrix.
    pub fn homogenize_stiffness(&mut self) -> Result<Matrix<3, 3>> {
        let optional = self.extra_options.rve_optional_gel;
        self.set_gel_eigen_grad_u(&[[0.; 2]; 2], optional)?;
        let mut stresses = [[0.; 3]; 3];
        let mut strains = [[0.; 3]; 3];
        let mut h = [[0.; 2]; 2];
        h[0][0] = 0.01;
        self.perform_virtual_testing(&h, &mut stresses, &mut strains, 0)?;
        let mut h = [[0.; 2]; 2];
        h[1][1] = 0.01;
        self.perform_virtual_testing(&h, &mut stresses, &mut strains, 1)?;
        let mut h = [[0.; 2]; 2];
        h[1][0] = 0.01; // H(0, 1) (column-major [col][row])
        self.perform_virtual_testing(&h, &mut stresses, &mut strains, 2)?;
        let eps_inverse = strains.inverse()?;
        Ok(stresses.matmul(&eps_inverse))
    }

    /// Upstream `homogenizeEigenGradU(eigen_gradu_macro)`.
    ///
    /// # Errors
    /// Integration errors.
    pub fn homogenize_eigen_grad_u(&self) -> Result<Matrix<2, 2>> {
        let mut m = [[0.; 2]; 2];
        m[0][0] = self.average_tensor_field(0, 0, "eigen_grad_u")?;
        m[1][1] = self.average_tensor_field(1, 1, "eigen_grad_u")?;
        m[1][0] = self.average_tensor_field(0, 1, "eigen_grad_u")?;
        m[0][1] = self.average_tensor_field(1, 0, "eigen_grad_u")?;
        Ok(m)
    }

    /// Symmetric positive definite check of a homogenised stiffness (a
    /// helper for tests and FE² diagnostics).
    #[must_use]
    pub fn is_positive_definite(c: &Matrix<3, 3>) -> bool {
        let at = |i: usize, j: usize| c[j][i];
        let d1 = at(0, 0);
        let d2 = at(0, 0) * at(1, 1) - at(0, 1) * at(1, 0);
        d1 > 0. && d2 > 0. && c.det() > 0.
    }
}

/// Whether `law` is a `MaterialDamageIterative<2>` or derives from it
/// upstream (the `dynamic_cast` of `advanceASR`).
fn is_damage_iterative_2d(law: &dyn Material) -> bool {
    law.as_any().is_some_and(|a| {
        a.is::<MaterialDamageIterative<2>>()
            || a.is::<MaterialIterativeStiffnessReduction<2>>()
            || a.is::<MaterialDamageIterativeNonLocal<2>>()
    })
}

/// Upstream `findCornerNodes` (`Math::are_float_equal` with `tolerance`).
///
/// # Errors
/// "The corner node i wasn't found".
pub fn find_corner_nodes(mesh: &Mesh, tolerance: Real) -> Result<[Idx; 4]> {
    let lower = mesh.lower_bounds().to_vec();
    let upper = mesh.upper_bounds().to_vec();
    let mut corners: [Option<Idx>; 4] = [None; 4];
    for (node, x) in mesh
        .nodes()
        .as_slice()
        .as_chunks::<DIM>()
        .0
        .iter()
        .enumerate()
    {
        let node = Idx::try_from(node).map_err(|_| Error::invalid("node index"))?;
        let dx = [x[0] - lower[0], x[1] - lower[1]];
        let distance = (dx[0] * dx[0] + dx[1] * dx[1]).sqrt();
        let eq = |a: Real, b: Real| are_float_equal(a, b, tolerance);
        if eq(distance, 0.) {
            corners[0] = Some(node);
        } else if eq(x[0], upper[0]) && eq(x[1], lower[1]) {
            corners[1] = Some(node);
        } else if eq(x[0], upper[0]) && eq(x[1], upper[1]) {
            corners[2] = Some(node);
        } else if eq(x[0], lower[0]) && eq(x[1], upper[1]) {
            corners[3] = Some(node);
        }
    }
    let mut out = [0; 4];
    for (i, c) in corners.iter().enumerate() {
        out[i] =
            c.ok_or_else(|| Error::invalid(format!("The corner node {} wasn't found", i + 1)))?;
    }
    Ok(out)
}
