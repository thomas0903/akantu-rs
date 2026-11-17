// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/shape_structural.{hh,cc},
// shape_structural_inline_impl.hh, integrator_gauss_inline_impl.hh
// (IntegratorGauss<_ek_structural>::computeJacobiansOnIntegrationPoints,
// integrate), element_class_structural.hh (computeShapes / computeShapesMass /
// computeShapeDerivatives / computeJMat / computeJacobian),
// fe_engine_template_tmpl_field.hh (assembleFieldMatrix,
// ShapesForMassHelper<_ek_structural>), fe_engine_template_tmpl_struct.hh,
// src/model/structural_mechanics/structural_mechanics_model{.cc,
// _inline_impl.hh} (element-level parts of assembleStiffnessMatrix,
// computeStressOnQuad, computeForcesBy{Local,Global}TractionArray).

//! FE engine for structural elements (FE-016, FE-035, FE-036): upstream
//! `FEEngineTemplate<IntegratorGauss, ShapeStructural, _ek_structural>`.
//!
//! Per `(type, ghost)` block, [`StructuralFeEngine::init_shape_functions`]
//! precomputes (upstream `initShapeFunctions`):
//! * rotations `R` (`ne` tuples of `ndof²`, column-major), from the node
//!   coordinates and, for `_bernoulli_beam_3`, the elemental mesh data
//!   [`EXTRA_NORMAL`] (required for that type);
//! * `det J · w` (`IntegratorGauss<_ek_structural>`): `J = dN_parent/dξ ·
//!   xᵀ` with `x` the first `natural_dim` rows of the rotated coordinates
//!   `R_{sdim×sdim} · X`; negative Jacobians are an error as upstream, and
//!   so are zero / non-finite ones (UD-296);
//! * shapes `N·T` (`ne·nq` tuples of `ndof × ndof·nn`, `T` the
//!   block-diagonal DOF rotation), with `N` evaluated on the rotated local
//!   coordinates (beam length `|x₂ − x₁|` along the local axis);
//! * shape derivatives `B·T` (`ne·nq` tuples of `nb_stress × ndof·nn`):
//!   `dN/dξ` from the **global** coordinates, `J⁻¹ · dN/dξ`,
//!   `arrangeInVoigt`, `· T`.
//!
//! Operand types (evaluation order, `crate::eigen`): `R_{sdim} · X` dynamic
//! × dynamic; `J = dnds · xᵀ` dynamic × fixed-transposed; `J⁻¹` and `det J`
//! dynamic LU; `J⁻¹ · dnds` dynamic × dynamic; `N · T` dynamic × dynamic;
//! `B · T` dynamic × **fixed** `T`; `N(q)·u`, `B(q)·u` dynamic Tensor3
//! slices; `Bᵀσ` dynamic transposed × dynamic vector (GEMV); `Nᵀb` fixed ×
//! fixed; `(Bᵀ D) B`, `(D B) u` dynamic views (model call sites).
//!
//! **Validity of the precomputed data.** Every block
//! `(type, ghost)` records the mesh revision it was built at and, for
//! `_bernoulli_beam_3`, the exact bit pattern of the [`EXTRA_NORMAL`] rows it
//! used (the block's `extra_normal` revision: mesh data mutations do not
//! advance the mesh revision). **Every** public operation that reads a
//! precomputed block takes the mesh and first checks the block with
//! [`StructuralFeEngine::check_up_to_date`], refusing a stale or missing
//! block **before** touching its output: the getters
//! (`rotations`, `shapes`, `shapes_derivatives`, `jacobians`,
//! `precomputed`), `interpolate_on_integration_points`,
//! `gradient_on_integration_points`, `integrate`, `compute_btd`,
//! `compute_btdb`, `compute_ntb`, `compute_ntbn`, `compute_stresses`,
//! `element_stiffness`, `line_load_local`, `tractions_to_local`,
//! `mass_shapes` and `consistent_mass`. Only `quadrature_rule` /
//! `nb_integration_points` (a function of the element type and the
//! integration order, never of the mesh) answer without the mesh. Mesh
//! events ([`akantu_mesh::MeshEventHandler`]) and
//! [`StructuralFeEngine::refresh`] rebuild every initialised block in full
//! (node coordinates are shared between blocks, so any committed mutation
//! can affect all of them); a ghost type whose rebuild fails loses all its
//! blocks (later operations report "not initialised" instead of serving old
//! data). A direct [`StructuralFeEngine::init_shape_functions`] of one ghost
//! type is transactional (nothing changes on error; the old stamps make the
//! old blocks refused once the mesh moved on), removes the blocks whose
//! type vanished from the mesh and never touches (nor re-validates) the
//! blocks of the other ghost type.
//!
//! **Upstream compatibility.** [`StructuralUpstreamCompat`] is fixed at
//! construction ([`StructuralFeEngine::new_with_compat`]); the default is the
//! corrected 3-D beam θy row (UD-293), [`StructuralUpstreamCompat::UPSTREAM`]
//! reproduces the oracle.
//!
//! **Deviations (documented, `docs/UPSTREAM_DEFECTS.md`).**
//! * Upstream `ShapeStructural::interpolateOnIntegrationPoints` /
//!   `gradientOnIntegrationPoints` iterate the element values with a view of
//!   `nb_dof·nn × 1 × nq` per step, so quadrature point `q` of element `e`
//!   reads the values of element `e·nq + q` (out of bounds for the last
//!   elements, UD-291); the gradient also views `B` with
//!   `element_dimension` rows instead of `nb_stress` (UD-292). The port
//!   computes the intended `N(q)·u_e` and `B(q)·u_e` (generalised strains,
//!   `nb_stress` components).
//! * Upstream passes an uninitialised normal to `computeRotationMatrix`
//!   when the mesh has no `extra_normal` data (shapes) and zeros
//!   (integrator); `_bernoulli_beam_3` then gets garbage / NaN rotations
//!   (UD-294). The port requires `extra_normal` for `_bernoulli_beam_3`
//!   (finite, nonzero, not parallel to the beam axis).
//! * `R = Pe·Pg⁻¹` is a rotation only for a unit normal orthogonal to the
//!   axis, which upstream assumes without checking (UD-343): by default a
//!   normal with `|n·n − 1|` or `|n·t̂|` above
//!   [`crate::structural::BEAM3_NORMAL_ADMISSIBILITY`] is refused with
//!   `Error::InvalidArgument` naming the element; admissible normals are
//!   used raw; [`StructuralUpstreamCompat::beam3_normal_unchecked`]
//!   (part of `UPSTREAM`) disables the check.
//! * DKT shape functions are undefined upstream (UD-053, UD-295): `N·T` is
//!   stored as zeros and the operations that use `N` return
//!   `Error::NotImplemented`.
//! * Degenerate geometry (zero / underflowed lengths, collinear triangles,
//!   zero or parallel beam normals, non-finite coordinates, zero or
//!   non-finite Jacobians) is refused with `Error::Numerical` naming the
//!   element instead of publishing `inf`/`NaN` data (UD-296).
//! * `ShapeStructural::computeBtDB` / `computeNtbN` are `AKANTU_TO_IMPLEMENT`
//!   upstream; [`StructuralFeEngine::compute_btdb`] has the semantics and
//!   operand types of `StructuralMechanicsModel::assembleStiffnessMatrix`
//!   (`Bᵀ·D·B`), [`StructuralFeEngine::compute_ntbn`] those of the local
//!   product of `assembleFieldMatrix` (`(Nᵀ·diag b)·N`).

use akantu_core::array::ArrayView;
use akantu_core::array_shared::SliceRef;
use akantu_core::enums::EventPriority;
use akantu_core::{
    Array, ElementKind, ElementType, ElementTypeMap, ElementTypeMapArray, Error, GhostType, Id,
    Idx, Real, Result, TypeFilter,
};
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler};

use crate::eigen::{self, Dims};
use crate::engine::{
    Block, check_input, check_precomputed, element_ids, prepare_out, resolve_filter,
};
use crate::lagrange::Interpolation;
use crate::quadrature::{QuadratureRule, gauss_rule};
use crate::shape::{IntegrationOrders, PrecomputedShapes};
use crate::structural::element::{Coords, StructuralElement, StructuralUpstreamCompat};
use crate::with_structural_element;

/// Name of the elemental mesh data holding the beam normals
/// (`_bernoulli_beam_3`, `spatial_dimension` components per element).
pub const EXTRA_NORMAL: &str = "extra_normal";

/// FE engine for the structural elements of a mesh (upstream
/// `StructuralMechanicsFEEngine`).
#[derive(Debug)]
pub struct StructuralFeEngine {
    id: Id,
    element_dimension: usize,
    orders: IntegrationOrders,
    rules: ElementTypeMap<QuadratureRule>,
    rotations: ElementTypeMapArray<Real>,
    precomputed: PrecomputedShapes,
    mesh_revision: u64,
    compat: StructuralUpstreamCompat,
    /// Validity stamp of every precomputed block.
    stamps: ElementTypeMap<BlockStamp>,
    /// Ghost types initialised so far (`[not ghost, ghost]`), rebuilt by
    /// [`StructuralFeEngine::refresh`].
    initialised: [bool; 2],
    /// Ghost types whose last [`StructuralFeEngine::refresh`] failed (their
    /// blocks were dropped), for the error message.
    rebuild_failed: [bool; 2],
    /// Executor of the per-element loops of the elemental matrices
    /// (D-123; serial until the model sets its own).
    exec: akantu_core::exec::Exec,
}

/// What a precomputed block was built from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct BlockStamp {
    /// Mesh revision at the build.
    mesh_revision: u64,
    /// Elements of the block at the build.
    nb_element: usize,
    /// `_bernoulli_beam_3`: bit patterns of the `extra_normal` rows used
    /// (`nb_element · sdim` values), the block's `extra_normal` revision.
    normals: Option<Vec<u64>>,
}

fn ghost_slot(ghost: GhostType) -> usize {
    usize::from(ghost == GhostType::Ghost)
}

/// Bit patterns of the first `count` normal values (exact comparison).
fn normal_bits(normals: Option<&[Real]>, count: usize) -> Option<Vec<u64>> {
    normals.map(|a| a[..count].iter().map(|v| v.to_bits()).collect())
}

/// Append the element to a geometric (`Error::Numerical`) or input
/// (`Error::InvalidArgument`, UD-343) error of a kernel; other errors are
/// returned unchanged.
fn at_element(err: Error, block: &Block<'_>, e: usize) -> Error {
    let suffix = |m: String| {
        format!(
            "{m} (element {e} of {}:{})",
            block.ty.name(),
            block.ghost.name()
        )
    };
    match err {
        Error::Numerical(m) => Error::Numerical(suffix(m)),
        Error::InvalidArgument(m) => Error::InvalidArgument(suffix(m)),
        other => other,
    }
}

/// Refuse non-finite precomputed values (no `inf`/`NaN` cache is ever
/// published, UD-296): `per` values per element.
fn require_finite_block(values: &[Real], per: usize, what: &str, block: &Block<'_>) -> Result<()> {
    if let Some(i) = values.iter().position(|v| !v.is_finite()) {
        return Err(Error::Numerical(format!(
            "structural FE engine: non-finite {what} {} (element {} of {}:{})",
            values[i],
            i / per.max(1),
            block.ty.name(),
            block.ghost.name()
        )));
    }
    Ok(())
}

fn unsupported(ty: ElementType, context: &'static str) -> Error {
    Error::UnsupportedElementType { ty, context }
}

/// `Error::UnsupportedElementType` unless `ty` is a structural type.
fn require_structural(ty: ElementType, context: &'static str) -> Result<()> {
    with_structural_element!(ty, E => { debug_assert_eq!(<E as crate::element_class::ElementClass>::TYPE, ty); Ok(()) },
        else Err(unsupported(ty, context)))
}

/// `N_T = N * T` of `computeShapes(Xs, x, T, Ns)`: `N` a dynamic
/// `Matrix<Real>` (`rows × nc`), `T` a dynamic `Matrix<Real>` (shape
/// precomputation).
pub(crate) fn shapes_times_t(
    n: &[Real],
    t: &[Real],
    rows: usize,
    nc: usize,
    out: &mut [Real],
) -> Result<()> {
    eigen::product_slices(Dims::dynamic(rows, nc), Dims::dynamic(nc, nc), n, t, out)?;
    Ok(())
}

/// `B_R = B * R` of `computeShapeDerivatives(Js, DNDSs, R, Bs)`: `B` a
/// dynamic `Matrix<Real>` (`rows × nc`), `R` the fixed
/// `Matrix<Real, nc, nc>` DOF rotation `T`.
pub(crate) fn derivatives_times_t(
    b: &[Real],
    t: &[Real],
    rows: usize,
    nc: usize,
    out: &mut [Real],
) -> Result<()> {
    eigen::product_slices(Dims::dynamic(rows, nc), Dims::fixed(nc, nc), b, t, out)?;
    Ok(())
}

/// `T` = block-diagonal `R` (`ndof × ndof`) over `nn` nodes, column-major
/// `nc × nc` with `nc = ndof · nn` (`T.zero(); T.block(...) = R`).
fn block_diag(r: &[Real], ndof: usize, nn: usize, t: &mut [Real]) {
    let nc = ndof * nn;
    t[..nc * nc].fill(0.0);
    for n in 0..nn {
        for j in 0..ndof {
            for i in 0..ndof {
                t[(n * ndof + i) + (n * ndof + j) * nc] = r[i + j * ndof];
            }
        }
    }
}

/// `R.block(0, 0, sdim, sdim) * X` (dynamic block × dynamic view): the
/// node coordinates in the element basis, column-major `sdim × nn`.
fn rotated_coords(
    r: &[Real],
    ndof: usize,
    x: &[Real],
    sdim: usize,
    nn: usize,
    out: &mut [Real],
) -> Result<()> {
    eigen::product(
        Dims::dynamic(sdim, sdim),
        Dims::dynamic(sdim, nn),
        |i, k| r[i + k * ndof],
        |k, j| x[k + j * sdim],
        out,
    )?;
    Ok(())
}

/// `J = dnds · xᵀ` (`computeJMat`): `dnds` the parent Lagrange derivatives
/// in a dynamic `Matrix<Real>`, `x` the fixed `nd × nn` block of the
/// rotated coordinates `rx` (`sdim × nn`). Writes `nd × nd` into `j`.
fn jacobian_matrix<E: StructuralElement>(
    xi: &[Real],
    rx: &[Real],
    sdim: usize,
    dnds: &mut [Real],
    j: &mut [Real],
) -> Result<()> {
    let (nd, nn) = (E::NATURAL_DIM, E::NB_NODES);
    E::ParentItp::dnds(xi, dnds);
    eigen::product(
        Dims::dynamic(nd, nn),
        Dims::fixed(nd, nn).t(),
        |i, k| dnds[i + k * nd],
        |k, c| rx[c + k * sdim],
        j,
    )?;
    Ok(())
}

/// Per-element normal (zeros when the mesh has none, as the integrator).
fn normal_of<'a>(
    normals: Option<&'a [Real]>,
    zeros: &'a [Real],
    e: usize,
    sdim: usize,
) -> &'a [Real] {
    normals.map_or(zeros, |a| &a[e * sdim..(e + 1) * sdim])
}

/// Validated `extra_normal` data of `(ty, ghost)` (`None` if absent).
fn extra_normals(
    mesh: &Mesh,
    ty: ElementType,
    ghost: GhostType,
    nb_element: usize,
    sdim: usize,
) -> Result<Option<SliceRef<'_, Real>>> {
    let Ok(map) = mesh.data().reals(EXTRA_NORMAL) else {
        return Ok(None);
    };
    // absent block → none; a protocol violation is returned
    let Some(a) = akantu_core::error::if_present(map.get(ty, ghost))? else {
        return Ok(None);
    };
    if a.nb_component() != sdim {
        return Err(Error::DimensionMismatch {
            context: "structural FE engine: extra_normal components (spatial dimension)",
            expected: sdim,
            got: a.nb_component(),
        });
    }
    if a.len() < nb_element {
        return Err(Error::DimensionMismatch {
            context: "structural FE engine: extra_normal tuples (elements)",
            expected: nb_element,
            got: a.len(),
        });
    }
    Ok(Some(a.into_slice()))
}

/// Precomputed data of one block: rotations, `N·T`, `B·T`, `det J · w`.
struct TypeData {
    rotations: Array<Real>,
    shapes: Array<Real>,
    dshapes: Array<Real>,
    jacobians: Array<Real>,
}

fn check_block_dimension<E: StructuralElement>(block: &Block<'_>) -> Result<()> {
    if block.sdim != E::SPATIAL_DIM {
        return Err(Error::DimensionMismatch {
            context: "structural FE engine: mesh spatial dimension of the element type",
            expected: E::SPATIAL_DIM,
            got: block.sdim,
        });
    }
    Ok(())
}

fn init_type<E: StructuralElement>(
    block: &Block<'_>,
    rule: &QuadratureRule,
    normals: Option<&[Real]>,
    compat: StructuralUpstreamCompat,
) -> Result<TypeData> {
    check_block_dimension::<E>(block)?;
    let ne = block.nb_element;
    if E::TYPE == ElementType::BernoulliBeam3 && normals.is_none() && ne > 0 {
        return Err(Error::invalid(
            "_bernoulli_beam_3 needs the elemental mesh data \"extra_normal\" (upstream uses an \
             uninitialised / zero normal otherwise)",
        ));
    }
    let (sdim, nn, nd) = (block.sdim, E::NB_NODES, E::NATURAL_DIM);
    let (ndof, ns, nc) = (E::NB_DOF_PER_NODE, E::NB_STRESS, E::NB_ELEMENT_DOFS);
    let nq = rule.nb_points();
    let mut rotations = Array::try_zeros(ne, ndof * ndof)?;
    let mut shapes = Array::try_zeros(ne * nq, ndof * nc)?;
    let mut dshapes = Array::try_zeros(ne * nq, ns * nc)?;
    let mut jacobians = Array::try_zeros(ne * nq, 1)?;
    let zeros = vec![0.0; sdim];
    let mut x = vec![0.0; sdim * nn];
    let mut rx = vec![0.0; sdim * nn];
    let mut xl = vec![0.0; nd * nn];
    let mut t = vec![0.0; nc * nc];
    let mut pdnds = vec![0.0; nd * nn];
    let mut jm = vec![0.0; nd * nd];
    let mut inv = vec![0.0; nd * nd];
    let mut nmat = vec![0.0; ndof * nc];
    let mut dnds = vec![0.0; nd * E::DNDS_COLUMNS];
    let mut dndx = vec![0.0; nd * E::DNDS_COLUMNS];
    let mut bmat = vec![0.0; ns * nc];
    let (rs, ss, ds, js) = (
        rotations.as_mut_slice(),
        shapes.as_mut_slice(),
        dshapes.as_mut_slice(),
        jacobians.as_mut_slice(),
    );
    for e in 0..ne {
        block.coords(e, &mut x)?;
        let r = &mut rs[e * ndof * ndof..(e + 1) * ndof * ndof];
        let xc = Coords::new(&x, sdim, nn)?;
        E::compute_rotation_matrix_with(xc, normal_of(normals, &zeros, e, sdim), compat, r)
            .map_err(|err| at_element(err, block, e))?;
        rotated_coords(r, ndof, &x, sdim, nn, &mut rx)?;
        for c in 0..nn {
            xl[c * nd..(c + 1) * nd].copy_from_slice(&rx[c * sdim..c * sdim + nd]);
        }
        block_diag(r, ndof, nn, &mut t);
        for q in 0..nq {
            let s = e * nq + q;
            let xi = rule.point(q, nd);
            jacobian_matrix::<E>(xi, &rx, sdim, &mut pdnds, &mut jm)?;
            js[s] = eigen::lu_det(nd, &jm);
            if E::HAS_SHAPES {
                E::compute_shapes_with(xi, Coords::new(&xl, nd, nn)?, compat, &mut nmat)
                    .map_err(|err| at_element(err, block, e))?;
                shapes_times_t(
                    &nmat,
                    &t,
                    ndof,
                    nc,
                    &mut ss[s * ndof * nc..(s + 1) * ndof * nc],
                )?;
            }
            E::compute_dnds(xi, xc, &mut dnds).map_err(|err| at_element(err, block, e))?;
            // Upstream does not check for singular Jacobians here (inf/NaN
            // propagate); the port honours the singularity indicator (UD-296).
            if eigen::lu_inverse(nd, &jm, &mut inv) {
                return Err(Error::Numerical(format!(
                    "structural FE engine: singular Jacobian (det J = {:e}) at quadrature point \
                     {q} of element {e} of {}:{}",
                    js[s],
                    block.ty.name(),
                    block.ghost.name()
                )));
            }
            eigen::product_slices(
                Dims::dynamic(nd, nd),
                Dims::dynamic(nd, E::DNDS_COLUMNS),
                &inv,
                &dnds,
                &mut dndx,
            )?;
            E::arrange_in_voigt(&dndx, &mut bmat)?;
            derivatives_times_t(&bmat, &t, ns, nc, &mut ds[s * ns * nc..(s + 1) * ns * nc])?;
        }
    }
    check_raw_jacobians(js, nq, block, true)?;
    require_finite_block(rs, ndof * ndof, "rotation entry", block)?;
    require_finite_block(ss, nq * ndof * nc, "N·T entry", block)?;
    require_finite_block(ds, nq * ns * nc, "B·T entry", block)?;
    multiply_by_weights(js, &rule.weights);
    Ok(TypeData {
        rotations,
        shapes,
        dshapes,
        jacobians,
    })
}

/// Raw (unweighted) Jacobians: negative values are refused with upstream's
/// message (`upstream_negative_check`: precomputation only, the mass path
/// has no sign check upstream), zero and non-finite ones as degenerate
/// (UD-296).
fn check_raw_jacobians(
    js: &[Real],
    nq: usize,
    block: &Block<'_>,
    upstream_negative_check: bool,
) -> Result<()> {
    let nq = nq.max(1);
    if upstream_negative_check {
        for (i, &j) in js.iter().enumerate() {
            if j < 0.0 {
                return Err(Error::Numerical(format!(
                    "Negative jacobian computed, possible problem in the element node ordering \
                     (Quadrature Point {}:{}:{}:{})",
                    i % nq,
                    i / nq,
                    block.ty.name(),
                    block.ghost.name()
                )));
            }
        }
    }
    for (i, &j) in js.iter().enumerate() {
        if !(j > 0.0 && j.is_finite()) {
            return Err(Error::Numerical(format!(
                "structural FE engine: degenerate element (det J = {j:e} at quadrature point {} \
                 of element {} of {}:{})",
                i % nq,
                i / nq,
                block.ty.name(),
                block.ghost.name()
            )));
        }
    }
    Ok(())
}

/// `J.array() *= weights.array()` for every element.
fn multiply_by_weights(jac: &mut [Real], weights: &[Real]) {
    let nq = weights.len();
    if nq == 0 {
        return;
    }
    for je in jac.chunks_mut(nq) {
        for (j, w) in je.iter_mut().zip(weights) {
            *j *= w;
        }
    }
}

/// `int_f = f · J` per element (`IntegratorGauss::integrate`): `f` dynamic
/// `nb_dof × nq`, `J` a dynamic vector — GEMV from `+0`.
fn integrate_rows<'j>(
    f: &[Real],
    nb_dof: usize,
    nq: usize,
    jac: impl Fn(usize) -> Result<&'j [Real]>,
    nb_element: usize,
    out: &mut Array<Real>,
) -> Result<()> {
    prepare_out(out, nb_element, nb_dof)?;
    let o = out.as_mut_slice();
    for pos in 0..nb_element {
        eigen::product_slices(
            Dims::dynamic(nb_dof, nq),
            Dims::new(nq, 1, false, true),
            &f[pos * nq * nb_dof..(pos + 1) * nq * nb_dof],
            jac(pos)?,
            &mut o[pos * nb_dof..(pos + 1) * nb_dof],
        )?;
    }
    Ok(())
}

impl StructuralFeEngine {
    /// New engine on the structural elements of dimension `element_dimension`
    /// (default: the mesh dimension) of `mesh`, without precomputation, with
    /// the corrected default behavior ([`StructuralUpstreamCompat::default`]).
    ///
    /// # Errors
    /// None at present (fallible for symmetry with the precomputing
    /// constructors).
    pub fn new(mesh: &Mesh, element_dimension: Option<usize>, id: impl Into<Id>) -> Result<Self> {
        Self::new_with_compat(
            mesh,
            element_dimension,
            id,
            StructuralUpstreamCompat::default(),
        )
    }

    /// [`StructuralFeEngine::new`] with explicit upstream-compatibility
    /// switches (fixed for the lifetime of the engine, so every precomputed
    /// block always matches them).
    ///
    /// # Errors
    /// None at present.
    pub fn new_with_compat(
        mesh: &Mesh,
        element_dimension: Option<usize>,
        id: impl Into<Id>,
        compat: StructuralUpstreamCompat,
    ) -> Result<Self> {
        let id = id.into();
        Ok(Self {
            precomputed: PrecomputedShapes {
                shapes: ElementTypeMapArray::new(format!("{id}:shapes")),
                shapes_derivatives: ElementTypeMapArray::new(format!("{id}:shapes_derivatives")),
                jacobians: ElementTypeMapArray::new(format!("{id}:jacobians")),
            },
            rotations: ElementTypeMapArray::new(format!("{id}:rotation_matrices")),
            element_dimension: element_dimension.unwrap_or(mesh.spatial_dimension()),
            orders: IntegrationOrders::default(),
            rules: ElementTypeMap::new(),
            mesh_revision: mesh.revision(),
            compat,
            stamps: ElementTypeMap::new(),
            initialised: [false; 2],
            rebuild_failed: [false; 2],
            exec: akantu_core::exec::Exec::serial(),
            id,
        })
    }

    /// New engine with shapes, derivatives, rotations and Jacobians
    /// precomputed for both ghost types (upstream constructor), corrected
    /// default behavior.
    ///
    /// # Errors
    /// See [`StructuralFeEngine::init_shape_functions`].
    pub fn with_precomputation(
        mesh: &Mesh,
        element_dimension: Option<usize>,
        id: impl Into<Id>,
    ) -> Result<Self> {
        Self::with_precomputation_compat(
            mesh,
            element_dimension,
            id,
            StructuralUpstreamCompat::default(),
        )
    }

    /// [`StructuralFeEngine::with_precomputation`] with explicit
    /// upstream-compatibility switches
    /// ([`StructuralUpstreamCompat::UPSTREAM`] for oracle replays).
    ///
    /// # Errors
    /// See [`StructuralFeEngine::init_shape_functions`].
    pub fn with_precomputation_compat(
        mesh: &Mesh,
        element_dimension: Option<usize>,
        id: impl Into<Id>,
        compat: StructuralUpstreamCompat,
    ) -> Result<Self> {
        let mut fe = Self::new_with_compat(mesh, element_dimension, id, compat)?;
        fe.init_shape_functions(mesh, GhostType::NotGhost)?;
        fe.init_shape_functions(mesh, GhostType::Ghost)?;
        Ok(fe)
    }

    /// Upstream-compatibility switches of the engine.
    #[must_use]
    pub fn upstream_compat(&self) -> StructuralUpstreamCompat {
        self.compat
    }

    /// Engine id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Element dimension of the handled types.
    #[must_use]
    pub fn element_dimension(&self) -> usize {
        self.element_dimension
    }

    /// Mesh revision of the most recent (re)initialisation. Validity is
    /// tracked per block: see [`StructuralFeEngine::check_up_to_date`].
    #[must_use]
    pub fn mesh_revision(&self) -> u64 {
        self.mesh_revision
    }

    /// Check that the precomputed `(ty, ghost)` block matches `mesh`: built
    /// at the current mesh revision, for the current number of elements
    /// and (for `_bernoulli_beam_3`) from bit-identical [`EXTRA_NORMAL`]
    /// rows.
    ///
    /// # Errors
    /// `Error::InvalidArgument` if the block was never initialised or is
    /// stale (rebuild with [`StructuralFeEngine::refresh`],
    /// [`StructuralFeEngine::init_shape_functions`] or by forwarding the
    /// mesh event); errors of malformed `extra_normal` data.
    pub fn check_up_to_date(&self, mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<()> {
        let stamp = self.stamps.get(ty, ghost).ok_or_else(|| {
            if self.rebuild_failed[ghost_slot(ghost)] {
                Error::invalid(format!(
                    "structural FE engine '{}': no precomputed data for {} ({}): the last \
                     rebuild failed and dropped the blocks of this ghost type; fix the mesh and \
                     call refresh / init_shape_functions",
                    self.id,
                    ty.name(),
                    ghost.name()
                ))
            } else {
                Error::invalid(format!(
                    "structural FE engine '{}': no precomputed data for {} ({}); call \
                     init_shape_functions first",
                    self.id,
                    ty.name(),
                    ghost.name()
                ))
            }
        })?;
        if stamp.mesh_revision != mesh.revision() {
            return Err(Error::invalid(format!(
                "structural FE engine '{}': the data of {} ({}) were precomputed at mesh revision \
                 {} but the mesh is at revision {} (nodes or elements changed since); forward the \
                 mesh event or call refresh / init_shape_functions",
                self.id,
                ty.name(),
                ghost.name(),
                stamp.mesh_revision,
                mesh.revision()
            )));
        }
        let nb_element = mesh.nb_element(ty, ghost);
        if stamp.nb_element != nb_element {
            return Err(Error::invalid(format!(
                "structural FE engine '{}': the data of {} ({}) were precomputed for {} elements, \
                 the mesh has {nb_element}; call refresh / init_shape_functions",
                self.id,
                ty.name(),
                ghost.name(),
                stamp.nb_element
            )));
        }
        if let Some(bits) = &stamp.normals {
            let sdim = mesh.spatial_dimension();
            let current_g = extra_normals(mesh, ty, ghost, nb_element, sdim)?;
            let current = current_g.as_ref().map(|a| &a[..nb_element * sdim]);
            let same = current.is_some_and(|a| {
                a.len() == bits.len() && a.iter().zip(bits).all(|(v, b)| v.to_bits() == *b)
            });
            if !same {
                return Err(Error::invalid(format!(
                    "structural FE engine '{}': the \"{EXTRA_NORMAL}\" mesh data of {} ({}) \
                     changed since the precomputation (beam frames are stale); call refresh / \
                     init_shape_functions",
                    self.id,
                    ty.name(),
                    ghost.name()
                )));
            }
        }
        Ok(())
    }

    /// Rebuild every block of every ghost type initialised so far, in full
    /// (the structural reaction to mesh events). Every initialised
    /// ghost type is attempted; a ghost type whose rebuild fails loses
    /// **all** its blocks (the engine never keeps data it
    /// failed to rebuild), so every later operation on it reports "not
    /// initialised" until a successful `refresh` / `init_shape_functions`
    /// (the ghost type stays initialised, so the next `refresh` retries it).
    /// The other ghost type is rebuilt independently.
    ///
    /// # Errors
    /// The first error of [`StructuralFeEngine::init_shape_functions`].
    pub fn refresh(&mut self, mesh: &Mesh) -> Result<()> {
        let mut first_error = None;
        for ghost in [GhostType::NotGhost, GhostType::Ghost] {
            let slot = ghost_slot(ghost);
            if !self.initialised[slot] {
                continue;
            }
            if let Err(err) = self.init_shape_functions(mesh, ghost) {
                self.drop_ghost_type(ghost);
                self.rebuild_failed[slot] = true;
                first_error.get_or_insert(err);
            }
        }
        match first_error {
            Some(err) => Err(err),
            None => {
                self.mesh_revision = mesh.revision();
                Ok(())
            }
        }
    }

    /// Drop every block of `ghost` (failed rebuild).
    fn drop_ghost_type(&mut self, ghost: GhostType) {
        let types: Vec<ElementType> = self.stamps.iter(ghost).map(|(t, _)| t).collect();
        for ty in types {
            self.remove_block(ty, ghost);
        }
    }

    /// Integration orders (precomputation and consistent mass).
    #[must_use]
    pub fn integration_orders(&self) -> &IntegrationOrders {
        &self.orders
    }

    /// Replace the integration orders (takes effect at the next
    /// [`StructuralFeEngine::init_shape_functions`]).
    pub fn set_integration_orders(&mut self, orders: IntegrationOrders) {
        self.orders = orders;
    }

    /// Structural element types of `mesh` handled by the engine (upstream
    /// `mesh.elementTypes(element_dimension, ghost, _ek_structural)`).
    pub fn element_types<'m>(
        &self,
        mesh: &'m Mesh,
        ghost: GhostType,
    ) -> impl Iterator<Item = ElementType> + 'm {
        mesh.element_types(TypeFilter {
            spatial_dimension: Some(self.element_dimension),
            ghost_type: ghost,
            kind: Some(ElementKind::Structural),
        })
    }

    /// Precompute rotations, `det J · w`, `N·T` and `B·T` for every handled
    /// type of `ghost` (upstream `initShapeFunctions(ghost_type)`).
    ///
    /// Transactional per ghost type: every block is computed first, then
    /// all are stored together with their stamps, and the blocks of `ghost`
    /// whose type is no longer in the mesh are removed. The blocks of the
    /// other ghost type are left untouched (and keep their own stamps).
    ///
    /// # Errors
    /// Malformed mesh storage or `extra_normal` data
    /// (`Error::DimensionMismatch`, `Error::InvalidArgument`); a missing
    /// `extra_normal` for `_bernoulli_beam_3`; degenerate or non-finite
    /// geometry and negative / zero / non-finite Jacobians
    /// (`Error::Numerical`, naming the element). Nothing is changed on
    /// error.
    pub fn init_shape_functions(&mut self, mesh: &Mesh, ghost: GhostType) -> Result<()> {
        let types: Vec<ElementType> = self.element_types(mesh, ghost).collect();
        let mut built = Vec::with_capacity(types.len());
        for &ty in &types {
            let rule = gauss_rule(ty, self.orders.order(ty))?;
            let block = Block::new(mesh, ty, ghost)?;
            let normals_g = extra_normals(mesh, ty, ghost, block.nb_element, block.sdim)?;
            let normals = normals_g.as_deref();
            let compat = self.compat;
            let data = with_structural_element!(ty, E => init_type::<E>(&block, &rule, normals, compat),
                else Err(unsupported(ty, "structural init_shape_functions")))?;
            let stamp = BlockStamp {
                mesh_revision: mesh.revision(),
                nb_element: block.nb_element,
                normals: if ty == ElementType::BernoulliBeam3 && block.nb_element > 0 {
                    normal_bits(normals, block.nb_element * block.sdim)
                } else {
                    None
                },
            };
            built.push((ty, rule, data, stamp));
        }
        let vanished: Vec<ElementType> = self
            .stamps
            .iter(ghost)
            .map(|(t, _)| t)
            .filter(|t| !types.contains(t))
            .collect();
        for ty in vanished {
            self.remove_block(ty, ghost);
        }
        for (ty, rule, data, stamp) in built {
            self.store(ty, ghost, data);
            self.rules.replace(ty, ghost, rule);
            self.stamps.replace(ty, ghost, stamp);
        }
        self.initialised[ghost_slot(ghost)] = true;
        self.rebuild_failed[ghost_slot(ghost)] = false;
        self.mesh_revision = mesh.revision();
        Ok(())
    }

    /// Drop every precomputed array of `(ty, ghost)`.
    fn remove_block(&mut self, ty: ElementType, ghost: GhostType) {
        self.rotations.remove(ty, ghost);
        self.precomputed.shapes.remove(ty, ghost);
        self.precomputed.shapes_derivatives.remove(ty, ghost);
        self.precomputed.jacobians.remove(ty, ghost);
        self.rules.remove(ty, ghost);
        self.stamps.remove(ty, ghost);
    }

    fn store(&mut self, ty: ElementType, ghost: GhostType, d: TypeData) {
        let suffix = if ghost == GhostType::Ghost {
            ":ghost"
        } else {
            ""
        };
        let named = |mut a: Array<Real>, base: &str| {
            a.set_id(format!("{base}:{ty}{suffix}"));
            a
        };
        let (rid, sid, did, jid) = (
            self.rotations.id().to_owned(),
            self.precomputed.shapes.id().to_owned(),
            self.precomputed.shapes_derivatives.id().to_owned(),
            self.precomputed.jacobians.id().to_owned(),
        );
        self.rotations
            .as_map_mut()
            .replace(ty, ghost, named(d.rotations, &rid));
        let p = &mut self.precomputed;
        p.shapes
            .as_map_mut()
            .replace(ty, ghost, named(d.shapes, &sid));
        p.shapes_derivatives
            .as_map_mut()
            .replace(ty, ghost, named(d.dshapes, &did));
        p.jacobians
            .as_map_mut()
            .replace(ty, ghost, named(d.jacobians, &jid));
    }

    /// Quadrature rule of `(ty, ghost)` (after initialisation).
    pub fn quadrature_rule(&self, ty: ElementType, ghost: GhostType) -> Result<&QuadratureRule> {
        self.rules.get(ty, ghost).ok_or_else(|| {
            Error::invalid(format!(
                "no integration points for {} ({}); call init_shape_functions first",
                ty.name(),
                ghost.name()
            ))
        })
    }

    /// Quadrature points per element.
    pub fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> Result<usize> {
        Ok(self.quadrature_rule(ty, ghost)?.nb_points())
    }

    /// Rotation matrices (`ne` tuples of `ndof²`, column-major; upstream
    /// `ShapeStructural::getRotations`), after
    /// [`StructuralFeEngine::check_up_to_date`].
    ///
    /// # Errors
    /// `Error::InvalidArgument` for a stale or missing block.
    pub fn rotations(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        self.check_up_to_date(mesh, ty, ghost)?;
        self.block_rotations(ty, ghost)
    }

    /// `N·T` per quadrature point (`ndof × ndof·nn`, column-major), after
    /// [`StructuralFeEngine::check_up_to_date`].
    ///
    /// # Errors
    /// `Error::InvalidArgument` for a stale or missing block.
    pub fn shapes(&self, mesh: &Mesh, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.check_up_to_date(mesh, ty, ghost)?;
        self.block_shapes(ty, ghost)
    }

    /// `B·T` per quadrature point (`nb_stress × ndof·nn`, column-major),
    /// after [`StructuralFeEngine::check_up_to_date`].
    ///
    /// # Errors
    /// `Error::InvalidArgument` for a stale or missing block.
    pub fn shapes_derivatives(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        self.check_up_to_date(mesh, ty, ghost)?;
        self.block_dshapes(ty, ghost)
    }

    /// `det J · w` per quadrature point, after
    /// [`StructuralFeEngine::check_up_to_date`].
    ///
    /// # Errors
    /// `Error::InvalidArgument` for a stale or missing block.
    pub fn jacobians(
        &self,
        mesh: &Mesh,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<&Array<Real>> {
        self.check_up_to_date(mesh, ty, ghost)?;
        self.block_jacobians(ty, ghost)
    }

    /// All precomputed shape data, after checking **every** stored block of
    /// both ghost types against `mesh` ([`StructuralFeEngine::check_up_to_date`]).
    ///
    /// # Errors
    /// `Error::InvalidArgument` if any stored block is stale.
    pub fn precomputed(&self, mesh: &Mesh) -> Result<&PrecomputedShapes> {
        for ghost in [GhostType::NotGhost, GhostType::Ghost] {
            for (ty, _) in self.stamps.iter(ghost) {
                self.check_up_to_date(mesh, ty, ghost)?;
            }
        }
        Ok(&self.precomputed)
    }

    // Unchecked block accessors (callers have run `check_up_to_date`).
    fn block_rotations(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.rotations.get(ty, ghost)
    }

    fn block_shapes(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.precomputed.shapes.get(ty, ghost)
    }

    fn block_dshapes(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.precomputed.shapes_derivatives.get(ty, ghost)
    }

    fn block_jacobians(&self, ty: ElementType, ghost: GhostType) -> Result<&Array<Real>> {
        self.precomputed.jacobians.get(ty, ghost)
    }

    /// `(ids, nq, array)` for a per-quadrature-point precomputed array.
    fn per_point<'s>(
        &self,
        a: &'s Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<(Vec<usize>, usize, &'s Array<Real>)> {
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = a.len().checked_div(nq).unwrap_or(0);
        let filter = resolve_filter(filter, ne)?;
        Ok((element_ids(filter, ne), nq, a))
    }

    /// Interpolate a nodal field (`ndof` values per node) at the quadrature
    /// points: `u_q = (N·T)(q) · u_e` (intended semantics of upstream
    /// `ShapeStructural::interpolateOnIntegrationPoints`, see the module
    /// notes). `nb_dof` must equal the DOFs per node of `ty`.
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn interpolate_on_integration_points(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let (ndof, nc, has) = with_structural_element!(ty, E => (E::NB_DOF_PER_NODE, E::NB_ELEMENT_DOFS, E::HAS_SHAPES),
            else return Err(unsupported(ty, "structural interpolate_on_integration_points")));
        if !has {
            return Err(Error::NotImplemented(
                "interpolation on _discrete_kirchhoff_triangle_18 (no shape functions upstream, UD-053)",
            ));
        }
        self.apply_per_point(mesh, u, out, nb_dof, ty, ghost, filter, ndof, nc, true)
    }

    /// Generalised strains `ε_q = (B·T)(q) · u_e` (`nb_stress` components;
    /// intended semantics of upstream
    /// `ShapeStructural::gradientOnIntegrationPoints`, see the module notes).
    #[allow(clippy::too_many_arguments)] // mirrors the upstream signature
    pub fn gradient_on_integration_points(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let (ns, nc) = with_structural_element!(ty, E => (E::NB_STRESS, E::NB_ELEMENT_DOFS),
            else return Err(unsupported(ty, "structural gradient_on_integration_points")));
        self.apply_per_point(mesh, u, out, nb_dof, ty, ghost, filter, ns, nc, false)
    }

    /// `M(q) · u_e` for `M` = `N·T` (`shapes`) or `B·T`, all operands
    /// dynamic (`Tensor3` slices).
    #[allow(clippy::too_many_arguments)]
    fn apply_per_point(
        &self,
        mesh: &Mesh,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
        rows: usize,
        nc: usize,
        shapes: bool,
    ) -> Result<()> {
        self.check_up_to_date(mesh, ty, ghost)?;
        let block = Block::new(mesh, ty, ghost)?;
        let ndof = nc / ty.nb_nodes_per_element();
        if nb_dof != ndof || u.nb_component() != nb_dof {
            return Err(Error::DimensionMismatch {
                context: "structural interpolation: components of the nodal field (DOFs per node)",
                expected: ndof,
                got: if nb_dof == ndof {
                    u.nb_component()
                } else {
                    nb_dof
                },
            });
        }
        // exactly one tuple per mesh node, before the output is touched
        // (as `FeEngine`)
        check_input(
            "structural interpolation: nodal field (mesh nodes)",
            u.len(),
            u.nb_component(),
            mesh.nb_nodes(),
            nb_dof,
        )?;
        let m = if shapes {
            self.block_shapes(ty, ghost)?
        } else {
            self.block_dshapes(ty, ghost)?
        };
        let (ids, nq, m) = self.per_point(m, ty, ghost, filter)?;
        check_precomputed(
            "structural interpolation: precomputed data vs mesh block",
            m.len(),
            block.nb_element,
            nq,
        )?;
        prepare_out(out, ids.len() * nq, rows)?;
        let mut ue = vec![0.0; nc];
        let (ms, o) = (m.as_slice(), out.as_mut_slice());
        let sz = rows * nc;
        let p_mu = eigen::prepare(Dims::dynamic(rows, nc), Dims::dynamic(nc, 1))?;
        for (pos, &e) in ids.iter().enumerate() {
            block.gather(e, u.as_slice(), nb_dof, &mut ue)?;
            for q in 0..nq {
                let s = e * nq + q;
                let t = pos * nq + q;
                p_mu.eval(
                    &ms[s * sz..(s + 1) * sz],
                    &ue,
                    &mut o[t * rows..(t + 1) * rows],
                )?;
            }
        }
        Ok(())
    }

    /// Integrate a quadrature field per element: `∫ f = Σ_q f_q (det J·w)_q`
    /// (upstream `integrate`, GEMV from `+0`). The block is checked against
    /// `mesh` first ([`StructuralFeEngine::check_up_to_date`]).
    #[allow(clippy::too_many_arguments)] // upstream signature plus the mesh
    pub fn integrate(
        &self,
        mesh: &Mesh,
        f: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        nb_dof: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        self.check_up_to_date(mesh, ty, ghost)?;
        let jac = self.block_jacobians(ty, ghost)?;
        let (ids, nq, jac) = self.per_point(jac, ty, ghost, filter)?;
        check_input(
            "structural integrate: quadrature field",
            f.len(),
            f.nb_component(),
            ids.len() * nq,
            nb_dof,
        )?;
        let js = jac.as_slice();
        integrate_rows(
            f.as_slice(),
            nb_dof,
            nq,
            |pos| Ok(&js[ids[pos] * nq..(ids[pos] + 1) * nq]),
            ids.len(),
            out,
        )
    }

    /// `(B·T)ᵀ σ` per quadrature point (upstream
    /// `ShapeStructural::computeBtD`; `σ` has `nb_stress` components, the
    /// output `ndof·nn`): dynamic transposed `B` times a dynamic vector.
    /// The block is checked against `mesh` first.
    pub fn compute_btd(
        &self,
        mesh: &Mesh,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let (ns, nc) = with_structural_element!(ty, E => (E::NB_STRESS, E::NB_ELEMENT_DOFS),
            else return Err(unsupported(ty, "structural compute_btd")));
        self.check_up_to_date(mesh, ty, ghost)?;
        let (ids, nq, b) = self.per_point(self.block_dshapes(ty, ghost)?, ty, ghost, filter)?;
        check_input(
            "structural compute_btd: D",
            d.len(),
            d.nb_component(),
            ids.len() * nq,
            ns,
        )?;
        prepare_out(out, d.len(), nc)?;
        let (bs, ds, os) = (b.as_slice(), d.as_slice(), out.as_mut_slice());
        let p_btd = eigen::prepare(Dims::dynamic(ns, nc).t(), Dims::new(ns, 1, false, true))?;
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let (s, t) = (e * nq + q, pos * nq + q);
                p_btd.eval(
                    &bs[s * ns * nc..(s + 1) * ns * nc],
                    &ds[t * ns..(t + 1) * ns],
                    &mut os[t * nc..(t + 1) * nc],
                )?;
            }
        }
        Ok(())
    }

    /// `(B·T)ᵀ D (B·T)` per quadrature point with `D` the `nb_stress²`
    /// tangent (column-major); output `(ndof·nn)²`. Operand types of
    /// `StructuralMechanicsModel::assembleStiffnessMatrix<type>`
    /// (`B.transpose() * D * B` on dynamic views: `(Bᵀ·D)·B`). The block is
    /// checked against `mesh` first.
    pub fn compute_btdb(
        &self,
        mesh: &Mesh,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let (ns, nc) = with_structural_element!(ty, E => (E::NB_STRESS, E::NB_ELEMENT_DOFS),
            else return Err(unsupported(ty, "structural compute_btdb")));
        self.check_up_to_date(mesh, ty, ghost)?;
        let (ids, nq, b) = self.per_point(self.block_dshapes(ty, ghost)?, ty, ghost, filter)?;
        check_input(
            "structural compute_btdb: D",
            d.len(),
            d.nb_component(),
            ids.len() * nq,
            ns * ns,
        )?;
        prepare_out(out, d.len(), nc * nc)?;
        let mut btd = vec![0.0; nc * ns];
        let p_btd = eigen::prepare(Dims::dynamic(ns, nc).t(), Dims::dynamic(ns, ns))?;
        let p_btdb = eigen::prepare(Dims::dynamic(nc, ns), Dims::dynamic(ns, nc))?;
        let (bs, ds, os) = (b.as_slice(), d.as_slice(), out.as_mut_slice());
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let (s, t) = (e * nq + q, pos * nq + q);
                let bq = &bs[s * ns * nc..(s + 1) * ns * nc];
                p_btd.eval(bq, &ds[t * ns * ns..(t + 1) * ns * ns], &mut btd)?;
                p_btdb.eval(&btd, bq, &mut os[t * nc * nc..(t + 1) * nc * nc])?;
            }
        }
        Ok(())
    }

    /// `(N·T)ᵀ b` per quadrature point (upstream `ShapeStructural::computeNtb`;
    /// `b` has `ndof` components, output `ndof·nn`): fixed `N` view
    /// transposed times a fixed vector. The block is checked against `mesh`
    /// first.
    pub fn compute_ntb(
        &self,
        mesh: &Mesh,
        b: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let (ndof, nc, has) = with_structural_element!(ty, E => (E::NB_DOF_PER_NODE, E::NB_ELEMENT_DOFS, E::HAS_SHAPES),
            else return Err(unsupported(ty, "structural compute_ntb")));
        if !has {
            return Err(Error::NotImplemented(
                "computeNtb on _discrete_kirchhoff_triangle_18 (no shape functions upstream, UD-053)",
            ));
        }
        self.check_up_to_date(mesh, ty, ghost)?;
        let (ids, nq, n) = self.per_point(self.block_shapes(ty, ghost)?, ty, ghost, filter)?;
        check_input(
            "structural compute_ntb: b",
            b.len(),
            b.nb_component(),
            ids.len() * nq,
            ndof,
        )?;
        prepare_out(out, b.len(), nc)?;
        let (ns_, bs, os) = (n.as_slice(), b.as_slice(), out.as_mut_slice());
        let p_ntb = eigen::prepare(Dims::fixed(ndof, nc).t(), Dims::fixed(ndof, 1))?;
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let (s, t) = (e * nq + q, pos * nq + q);
                p_ntb.eval(
                    &ns_[s * ndof * nc..(s + 1) * ndof * nc],
                    &bs[t * ndof..(t + 1) * ndof],
                    &mut os[t * nc..(t + 1) * nc],
                )?;
            }
        }
        Ok(())
    }

    /// `(N·T)ᵀ diag(b) (N·T)` per quadrature point (`b`: `ndof` weights per
    /// point; output `(ndof·nn)²`), with the operand types of the local
    /// product of `assembleFieldMatrix` (`Nt = N.transpose()` copied into a
    /// dynamic matrix, `Nt(d) *= b(d)`, `Nt * N`). Upstream
    /// `ShapeStructural::computeNtbN` is `AKANTU_TO_IMPLEMENT`. The block is
    /// checked against `mesh` first.
    pub fn compute_ntbn(
        &self,
        mesh: &Mesh,
        b: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
        filter: Option<&[Idx]>,
    ) -> Result<()> {
        let (ndof, nc, has) = with_structural_element!(ty, E => (E::NB_DOF_PER_NODE, E::NB_ELEMENT_DOFS, E::HAS_SHAPES),
            else return Err(unsupported(ty, "structural compute_ntbn")));
        if !has {
            return Err(Error::NotImplemented(
                "computeNtbN on _discrete_kirchhoff_triangle_18 (no shape functions upstream, UD-053)",
            ));
        }
        self.check_up_to_date(mesh, ty, ghost)?;
        let (ids, nq, n) = self.per_point(self.block_shapes(ty, ghost)?, ty, ghost, filter)?;
        check_input(
            "structural compute_ntbn: b",
            b.len(),
            b.nb_component(),
            ids.len() * nq,
            ndof,
        )?;
        prepare_out(out, b.len(), nc * nc)?;
        let mut nt = vec![0.0; nc * ndof];
        let (nss, bs, os) = (n.as_slice(), b.as_slice(), out.as_mut_slice());
        for (pos, &e) in ids.iter().enumerate() {
            for q in 0..nq {
                let (s, t) = (e * nq + q, pos * nq + q);
                let nq_ = &nss[s * ndof * nc..(s + 1) * ndof * nc];
                ntbn_local(
                    nq_,
                    ndof,
                    nc,
                    &bs[t * ndof..(t + 1) * ndof],
                    &mut nt,
                    &mut os[t * nc * nc..(t + 1) * nc * nc],
                )?;
            }
        }
        Ok(())
    }

    /// Generalised stresses `σ_q = (D_q · (B·T)(q)) · u_e` (element part of
    /// `StructuralMechanicsModel::computeStressOnQuad`; `D` per quadrature
    /// point `nb_stress²`, `u` nodal with `ndof` components), all elements of
    /// `(ty, ghost)`.
    pub fn compute_stresses(
        &self,
        mesh: &Mesh,
        d: ArrayView<'_, Real>,
        u: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let (ndof, ns, nc) = with_structural_element!(ty, E => (E::NB_DOF_PER_NODE, E::NB_STRESS, E::NB_ELEMENT_DOFS),
            else return Err(unsupported(ty, "structural compute_stresses")));
        self.check_up_to_date(mesh, ty, ghost)?;
        let block = Block::new(mesh, ty, ghost)?;
        let (ids, nq, b) = self.per_point(self.block_dshapes(ty, ghost)?, ty, ghost, None)?;
        check_precomputed(
            "structural stresses: precomputed B",
            b.len(),
            block.nb_element,
            nq,
        )?;
        check_input(
            "structural stresses: D",
            d.len(),
            d.nb_component(),
            ids.len() * nq,
            ns * ns,
        )?;
        if u.nb_component() != ndof {
            return Err(Error::DimensionMismatch {
                context: "structural stresses: components of the displacement-rotation field",
                expected: ndof,
                got: u.nb_component(),
            });
        }
        check_input(
            "structural stresses: displacement-rotation field (mesh nodes)",
            u.len(),
            u.nb_component(),
            mesh.nb_nodes(),
            ndof,
        )?;
        prepare_out(out, ids.len() * nq, ns)?;
        let mut ue = vec![0.0; nc];
        let mut db = vec![0.0; ns * nc];
        let (bs, ds, os) = (b.as_slice(), d.as_slice(), out.as_mut_slice());
        let p_db = eigen::prepare(Dims::dynamic(ns, ns), Dims::dynamic(ns, nc))?;
        let p_dbu = eigen::prepare(Dims::dynamic(ns, nc), Dims::new(nc, 1, false, true))?;
        for &e in &ids {
            block.gather(e, u.as_slice(), ndof, &mut ue)?;
            for q in 0..nq {
                let s = e * nq + q;
                p_db.eval(
                    &ds[s * ns * ns..(s + 1) * ns * ns],
                    &bs[s * ns * nc..(s + 1) * ns * nc],
                    &mut db,
                )?;
                p_dbu.eval(&db, &ue, &mut os[s * ns..(s + 1) * ns])?;
            }
        }
        Ok(())
    }

    /// Element stiffness `k_e = ∫ (B·T)ᵀ D (B·T)` (element part of
    /// `StructuralMechanicsModel::assembleStiffnessMatrix<type>`): `D` per
    /// quadrature point (`nb_stress²`), output `ne` tuples of `(ndof·nn)²`
    /// (column-major). The block is checked against `mesh` first.
    pub fn element_stiffness(
        &self,
        mesh: &Mesh,
        d: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        // `compute_btdb` then `integrate`, fused per element: their checks
        // in their order, then per element the point matrices with the
        // products of `compute_btdb` and their integral with the product
        // of `integrate` (the same bits), elements on the engine's threads
        let (ns, nc) = with_structural_element!(ty, E => (E::NB_STRESS, E::NB_ELEMENT_DOFS),
            else return Err(unsupported(ty, "structural compute_btdb")));
        self.check_up_to_date(mesh, ty, ghost)?;
        let (ids, nq, b) = self.per_point(self.block_dshapes(ty, ghost)?, ty, ghost, None)?;
        check_input(
            "structural compute_btdb: D",
            d.len(),
            d.nb_component(),
            ids.len() * nq,
            ns * ns,
        )?;
        let jac = self.block_jacobians(ty, ghost)?;
        let (ids_j, nq_j, jac) = self.per_point(jac, ty, ghost, None)?;
        if ids_j != ids || nq_j != nq {
            let mut btdb = Array::new(1);
            self.compute_btdb(mesh, d, &mut btdb, ty, ghost, None)?;
            let nc2 = btdb.nb_component();
            return self.integrate(mesh, btdb.view(), out, nc2, ty, ghost, None);
        }
        let nc2 = nc * nc;
        prepare_out(out, ids.len(), nc2)?;
        let p_btd = eigen::prepare(Dims::dynamic(ns, nc).t(), Dims::dynamic(ns, ns))?;
        let p_btdb = eigen::prepare(Dims::dynamic(nc, ns), Dims::dynamic(ns, nc))?;
        let p_fj = eigen::prepare(Dims::dynamic(nc2, nq), Dims::new(nq, 1, false, true))?;
        let kernel = StiffnessParts {
            ns,
            nc,
            nq,
            bs: b.as_slice(),
            ds: d.as_slice(),
            js: jac.as_slice(),
            p_btd: &p_btd,
            p_btdb: &p_btdb,
            p_fj: &p_fj,
        };
        let min = (65_536 / (nq * nc2).max(1)).max(1);
        self.exec.try_for_each(
            (
                akantu_core::exec::Tuples::new(out.as_mut_slice(), nc2),
                akantu_core::exec::TuplesRef::new(&ids, 1),
            ),
            min,
            |first, (mut o, ids)| kernel.part(first, ids.as_slice(), o.as_mut_slice()),
        )
    }

    /// Executor of the per-element loops (D-123).
    #[must_use]
    pub fn exec(&self) -> &akantu_core::exec::Exec {
        &self.exec
    }

    /// Set the executor of the per-element loops of the elemental matrices
    /// (every result is the same for every thread count in
    /// [`akantu_core::exec::Precision::Bitwise`]).
    pub fn set_exec(&mut self, exec: akantu_core::exec::Exec) {
        self.exec = exec;
    }

    /// Element forces of a line load given in the **local** element basis
    /// (`ndof` components per quadrature point): `∫ (N·T)ᵀ t` (element part
    /// of `computeForcesByLocalTractionArray`), output `ne` tuples of
    /// `ndof·nn`. The block is checked against `mesh` first.
    pub fn line_load_local(
        &self,
        mesh: &Mesh,
        tractions: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let mut ntb = Array::new(1);
        self.compute_ntb(mesh, tractions, &mut ntb, ty, ghost, None)?;
        let nc = ntb.nb_component();
        self.integrate(mesh, ntb.view(), out, nc, ty, ghost, None)
    }

    /// Rotate global tractions to the element basis, `t_q = R_e · T_q`
    /// (`computeForcesByGlobalTractionArray`: dynamic `R` view times a
    /// dynamic vector, GEMV). The block is checked against `mesh` first.
    pub fn tractions_to_local(
        &self,
        mesh: &Mesh,
        global: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let ndof = with_structural_element!(ty, E => E::NB_DOF_PER_NODE,
            else return Err(unsupported(ty, "structural tractions_to_local")));
        self.check_up_to_date(mesh, ty, ghost)?;
        let r = self.block_rotations(ty, ghost)?;
        let nq = self.nb_integration_points(ty, ghost)?;
        let ne = r.len();
        check_input(
            "structural tractions: global",
            global.len(),
            global.nb_component(),
            ne * nq,
            ndof,
        )?;
        prepare_out(out, ne * nq, ndof)?;
        let (rs, gs, os) = (r.as_slice(), global.as_slice(), out.as_mut_slice());
        let p_rg = eigen::prepare(Dims::dynamic(ndof, ndof), Dims::new(ndof, 1, false, true))?;
        for e in 0..ne {
            for q in 0..nq {
                let s = e * nq + q;
                p_rg.eval(
                    &rs[e * ndof * ndof..(e + 1) * ndof * ndof],
                    &gs[s * ndof..(s + 1) * ndof],
                    &mut os[s * ndof..(s + 1) * ndof],
                )?;
            }
        }
        Ok(())
    }

    /// Mass shapes `N_m·T` at the points of the consistent-mass rule
    /// (`nb_stress × ndof·nn` per point, column-major; upstream
    /// `ShapeStructural::computeShapesMassOnIntegrationPoints` with the
    /// rule of order [`IntegrationOrders::mass_order`]).
    pub fn mass_shapes(
        &self,
        mesh: &Mesh,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        require_structural(ty, "structural mass_shapes")?;
        self.check_up_to_date(mesh, ty, ghost)?;
        let rule = gauss_rule(ty, self.orders.mass_order(ty))?;
        let block = Block::new(mesh, ty, ghost)?;
        let r = self.block_rotations(ty, ghost)?;
        if r.len() != block.nb_element {
            return Err(Error::DimensionMismatch {
                context: "structural mass shapes: rotations vs mesh block",
                expected: block.nb_element,
                got: r.len(),
            });
        }
        let compat = self.compat;
        let (shapes, ncomp) = with_structural_element!(ty, E => (mass_shapes_type::<E>(&block, &rule, r.as_slice(), compat)?.0, E::NB_STRESS * E::NB_ELEMENT_DOFS),
            else return Err(unsupported(ty, "structural mass_shapes")));
        *out = Array::from_vec(shapes, ncomp)?;
        Ok(())
    }

    /// Element consistent mass `∫ (N_m·T)ᵀ diag(ρ) (N_m·T)` (element part of
    /// `FEEngineTemplate::assembleFieldMatrix` with
    /// `ShapesForMassHelper<_ek_structural>`): `N_m` are the first
    /// `nb_stress` rows of `N` evaluated on the **global** coordinates, the
    /// rule has order [`IntegrationOrders::mass_order`] (`2 · degree`) and the
    /// Jacobians are recomputed on it (structural formula, no sign check).
    /// `field` holds `nb_stress` values per mass quadrature point (upstream
    /// `ComputeRhoFunctorStruct`: `ρ·A` in every component); output `ne`
    /// tuples of `(ndof·nn)²`.
    pub fn consistent_mass(
        &self,
        mesh: &Mesh,
        field: ArrayView<'_, Real>,
        out: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        require_structural(ty, "structural consistent_mass")?;
        self.check_up_to_date(mesh, ty, ghost)?;
        let rule = gauss_rule(ty, self.orders.mass_order(ty))?;
        let block = Block::new(mesh, ty, ghost)?;
        let r = self.block_rotations(ty, ghost)?;
        if r.len() != block.nb_element {
            return Err(Error::DimensionMismatch {
                context: "structural consistent mass: rotations vs mesh block",
                expected: block.nb_element,
                got: r.len(),
            });
        }
        let compat = self.compat;
        let exec = &self.exec;
        with_structural_element!(ty, E => consistent_mass_type::<E>(&block, &rule, r.as_slice(), compat, field, out, exec),
            else Err(unsupported(ty, "structural consistent_mass")))
    }
}

/// `(Nᵀ diag b) N` with `Nt` a dynamic copy of `Nᵀ` (`nc × rows`) and `N`
/// a dynamic `rows × nc` view.
fn ntbn_local(
    n: &[Real],
    rows: usize,
    nc: usize,
    b: &[Real],
    nt: &mut [Real],
    out: &mut [Real],
) -> Result<()> {
    for d in 0..rows {
        for i in 0..nc {
            nt[i + d * nc] = n[d + i * rows] * b[d];
        }
    }
    eigen::product_slices(Dims::dynamic(nc, rows), Dims::dynamic(rows, nc), nt, n, out)?;
    Ok(())
}

/// Mass shapes `N_m·T` (`nb_stress × ndof·nn` per mass quadrature point,
/// upstream `computeShapesMassOnIntegrationPoints`) and the structural
/// `det J` (raw, before weights) at the rule points, for every element.
fn mass_shapes_type<E: StructuralElement>(
    block: &Block<'_>,
    rule: &QuadratureRule,
    rotations: &[Real],
    compat: StructuralUpstreamCompat,
) -> Result<(Vec<Real>, Vec<Real>)> {
    if !E::HAS_SHAPES {
        return Err(Error::NotImplemented(
            "consistent mass of _discrete_kirchhoff_triangle_18 (no shape functions upstream, UD-053)",
        ));
    }
    check_block_dimension::<E>(block)?;
    let (sdim, nn, nd) = (block.sdim, E::NB_NODES, E::NATURAL_DIM);
    let (ndof, ns, nc) = (E::NB_DOF_PER_NODE, E::NB_STRESS, E::NB_ELEMENT_DOFS);
    let ne = block.nb_element;
    let nq = rule.nb_points();
    let mut x = vec![0.0; sdim * nn];
    let mut rx = vec![0.0; sdim * nn];
    let mut t = vec![0.0; nc * nc];
    let mut nmat = vec![0.0; ndof * nc];
    let mut pdnds = vec![0.0; nd * nn];
    let mut jm = vec![0.0; nd * nd];
    let mut shapes = vec![0.0; ne * nq * ns * nc];
    let mut jac = vec![0.0; ne * nq];
    for e in 0..ne {
        block.coords(e, &mut x)?;
        let r = &rotations[e * ndof * ndof..(e + 1) * ndof * ndof];
        block_diag(r, ndof, nn, &mut t);
        rotated_coords(r, ndof, &x, sdim, nn, &mut rx)?;
        let xc = Coords::new(&x, sdim, nn)?;
        for q in 0..nq {
            let s = e * nq + q;
            let xi = rule.point(q, nd);
            jacobian_matrix::<E>(xi, &rx, sdim, &mut pdnds, &mut jm)?;
            jac[s] = eigen::lu_det(nd, &jm);
            // N_T = N.block(0, 0, nb_stress, cols) * T  (dynamic block × fixed T)
            E::compute_shapes_with(xi, xc, compat, &mut nmat)
                .map_err(|err| at_element(err, block, e))?;
            eigen::product(
                Dims::dynamic(ns, nc),
                Dims::fixed(nc, nc),
                |i, k| nmat[i + k * ndof],
                |k, j| t[k + j * nc],
                &mut shapes[s * ns * nc..(s + 1) * ns * nc],
            )?;
        }
    }
    // upstream has no sign check on this path; zero / non-finite Jacobians
    // and non-finite shapes are refused (UD-296)
    check_raw_jacobians(&jac, nq, block, false)?;
    require_finite_block(&shapes, nq * ns * nc, "mass N·T entry", block)?;
    Ok((shapes, jac))
}

fn consistent_mass_type<E: StructuralElement>(
    block: &Block<'_>,
    rule: &QuadratureRule,
    rotations: &[Real],
    compat: StructuralUpstreamCompat,
    field: ArrayView<'_, Real>,
    out: &mut Array<Real>,
    exec: &akantu_core::exec::Exec,
) -> Result<()> {
    if !E::HAS_SHAPES {
        return Err(Error::NotImplemented(
            "consistent mass of _discrete_kirchhoff_triangle_18 (no shape functions upstream, UD-053)",
        ));
    }
    let (ns, nc) = (E::NB_STRESS, E::NB_ELEMENT_DOFS);
    let ne = block.nb_element;
    let nq = rule.nb_points();
    check_input(
        "structural consistent mass: field",
        field.len(),
        field.nb_component(),
        ne * nq,
        ns,
    )?;
    let (shapes, mut jac) = mass_shapes_type::<E>(block, rule, rotations, compat)?;
    multiply_by_weights(&mut jac, &rule.weights);
    let fs = field.as_slice();
    let nc2 = nc * nc;
    prepare_out(out, ne, nc2)?;
    // per element: the point matrices (`ntbn_local`), then their integral
    // with the product of `integrate_rows` (the same bits); elements on
    // the executor's threads
    let p_fj = eigen::prepare(Dims::dynamic(nc2, nq), Dims::new(nq, 1, false, true))?;
    let min = (65_536 / (nq * nc2).max(1)).max(1);
    exec.try_for_each(
        akantu_core::exec::Tuples::new(out.as_mut_slice(), nc2),
        min,
        |first, mut o| {
            mass_part(
                &shapes,
                fs,
                &jac,
                ns,
                nc,
                nq,
                &p_fj,
                first,
                o.as_mut_slice(),
            )
        },
    )
}

/// [`consistent_mass_type`] for the elements `first..` (one `nc²` tuple of
/// `o` each).
#[allow(clippy::too_many_arguments)] // one part of the loop
#[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
fn mass_part(
    shapes: &[Real],
    fs: &[Real],
    jac: &[Real],
    ns: usize,
    nc: usize,
    nq: usize,
    p_fj: &eigen::SliceProduct,
    first: usize,
    o: &mut [Real],
) -> Result<()> {
    let nc2 = nc * nc;
    let mut nt = vec![0.0; nc * ns];
    let mut local = vec![0.0; nq * nc2];
    for (p, oe) in o.chunks_exact_mut(nc2).enumerate() {
        let e = first + p;
        for q in 0..nq {
            let s = e * nq + q;
            ntbn_local(
                &shapes[s * ns * nc..(s + 1) * ns * nc],
                ns,
                nc,
                &fs[s * ns..(s + 1) * ns],
                &mut nt,
                &mut local[q * nc2..(q + 1) * nc2],
            )?;
        }
        p_fj.eval(&local, &jac[e * nq..(e + 1) * nq], oe)?;
    }
    Ok(())
}

/// The operands of the fused [`StructuralFeEngine::element_stiffness`].
struct StiffnessParts<'a> {
    ns: usize,
    nc: usize,
    nq: usize,
    bs: &'a [Real],
    ds: &'a [Real],
    js: &'a [Real],
    p_btd: &'a eigen::SliceProduct,
    p_btdb: &'a eigen::SliceProduct,
    p_fj: &'a eigen::SliceProduct,
}

impl StiffnessParts<'_> {
    /// The elemental matrices of the elements `ids` (positions `first..`).
    #[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
    fn part(&self, first: usize, ids: &[usize], o: &mut [Real]) -> Result<()> {
        let (ns, nc, nq) = (self.ns, self.nc, self.nq);
        let nc2 = nc * nc;
        let mut btd = vec![0.0; nc * ns];
        let mut local = vec![0.0; nq * nc2];
        for (p, (&e, oe)) in ids.iter().zip(o.chunks_exact_mut(nc2)).enumerate() {
            let pos = first + p;
            for q in 0..nq {
                let (s, t) = (e * nq + q, pos * nq + q);
                let bq = &self.bs[s * ns * nc..(s + 1) * ns * nc];
                self.p_btd
                    .eval(bq, &self.ds[t * ns * ns..(t + 1) * ns * ns], &mut btd)?;
                self.p_btdb
                    .eval(&btd, bq, &mut local[q * nc2..(q + 1) * nc2])?;
            }
            self.p_fj.eval(&local, &self.js[e * nq..(e + 1) * nq], oe)?;
        }
        Ok(())
    }
}

impl MeshEventHandler for StructuralFeEngine {
    fn event_priority(&self) -> EventPriority {
        EventPriority::FE_ENGINE
    }

    /// Accepts every event: the engine has no
    /// event it cannot handle (every event triggers the same full rebuild
    /// of the initialised blocks). The rebuild can still fail on the new
    /// geometry (degenerate or non-finite beam/shell data);
    /// deciding that means doing the rebuild, which a side-effect
    /// free check cannot keep, so such failures stay dispatch failures.
    fn check_mesh_event(&self, _mesh: &Mesh, _event: &MeshEvent) -> Result<()> {
        Ok(())
    }

    /// Every committed mesh event triggers a full rebuild of the
    /// initialised blocks ([`StructuralFeEngine::refresh`]): node
    /// coordinates are shared, so
    /// no block can be proven unaffected, and new / vanished types of the
    /// initialised ghost types are added / removed.
    fn on_mesh_event(&mut self, mesh: &Mesh, _event: &MeshEvent) -> Result<()> {
        self.refresh(mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-element `_bernoulli_beam_2` block (for the guard messages).
    fn beam2_block_mesh() -> Mesh {
        let mut mesh = Mesh::new(2, "guards").unwrap();
        {
            let mut acc = akantu_mesh::MeshAccessor::new(&mut mesh);
            acc.push_node(&[0., 0.]).unwrap();
            acc.push_node(&[1., 0.]).unwrap();
            acc.connectivity_mut(ElementType::BernoulliBeam2, GhostType::Ghost)
                .push_tuple(&[0, 1]);
        }
        mesh
    }

    /// The second-line guards refuse the
    /// conditions they exist for, injected directly (no geometry reaches
    /// them through the kernels): non-finite cache entries are refused
    /// naming the element of the offending entry; raw Jacobians that are
    /// negative (upstream message, precomputation only), `±0`, infinite or
    /// NaN are refused.
    #[test]
    fn second_line_guards_refuse_injected_conditions() {
        let mesh = beam2_block_mesh();
        let block = Block::new(&mesh, ElementType::BernoulliBeam2, GhostType::Ghost).unwrap();
        require_finite_block(&[1.0, 2.0, 3.0, 4.0], 2, "B·T entry", &block).unwrap();
        for bad in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            let err =
                require_finite_block(&[1.0, 2.0, 3.0, bad], 2, "B·T entry", &block).unwrap_err();
            let msg = format!("{err}");
            assert!(matches!(err, Error::Numerical(_)), "{msg}");
            assert!(msg.contains("non-finite B·T entry"), "{msg}");
            assert!(
                msg.contains("element 1 of _bernoulli_beam_2:_ghost"),
                "{msg}"
            );
        }
        // positive finite values pass, subnormal ones included (the guard is
        // "positive and finite"; a subnormal det J makes J⁻¹ overflow, which
        // the singular-inverse and finite-cache guards refuse)
        check_raw_jacobians(&[0.5, 1e-300, 1e-320], 3, &block, true).unwrap();
        let negative = check_raw_jacobians(&[0.5, -1.0], 2, &block, true).unwrap_err();
        assert!(format!("{negative}").contains("Negative jacobian"));
        for bad in [0.0, -0.0, Real::INFINITY, Real::NAN, -1.0] {
            let err = check_raw_jacobians(&[0.5, bad], 2, &block, false).unwrap_err();
            let msg = format!("{err}");
            assert!(matches!(err, Error::Numerical(_)), "{msg}");
            assert!(msg.contains("degenerate element"), "{bad}: {msg}");
            assert!(
                msg.contains("quadrature point 1 of element 0"),
                "{bad}: {msg}"
            );
        }
    }

    fn hex(t: &str) -> Real {
        Real::from_bits(u64::from_str_radix(t, 16).expect("hex double"))
    }

    /// The corpus records "structural N * T and B * R" (oracle:
    /// strict Eigen 3.4.0 products of the upstream sizes) evaluated through
    /// the engine's own `N·T` (dynamic × dynamic) and `B·T` (dynamic ×
    /// fixed) products: bit-identical. DKT `N·T` and `B·T` share the size
    /// `6 × 18 · 18 × 18`, so every record of a size is pinned through both
    /// paths that use it.
    #[test]
    fn products_match_the_oracle_structural_records() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../akantu-linalg/tests/fixtures/dense/dense_gemm_strict.txt"
        );
        let text = std::fs::read_to_string(path).expect("dense product corpus");
        let mut in_section = false;
        let mut seen = 0;
        for line in text.lines() {
            if line.starts_with('#') {
                in_section = line.starts_with("# structural N * T and B * R")
                    || (in_section && line.starts_with("# bernoulli_beam_3"));
                continue;
            }
            if !in_section {
                continue;
            }
            let fields: Vec<&str> = line.split(" | ").collect();
            let head: Vec<&str> = fields[0].split_whitespace().collect();
            let ["matmul", "D", r, c, k] = head.as_slice() else {
                continue;
            };
            let (r, c, k): (usize, usize, usize) =
                (r.parse().unwrap(), c.parse().unwrap(), k.parse().unwrap());
            assert_eq!(c, k, "{line}");
            let input: Vec<Real> = fields[1].split_whitespace().map(hex).collect();
            let expected: Vec<u64> = fields[2]
                .split_whitespace()
                .map(|t| u64::from_str_radix(t, 16).unwrap())
                .collect();
            let (a, t) = input.split_at(r * c);
            // (rows, nc) of N·T: (3, 6), (6, 12), (6, 18); of B·T: (2, 6), (4, 12), (6, 18)
            let n_path = matches!((r, c), (3, 6) | (6, 12) | (6, 18));
            let b_path = matches!((r, c), (2, 6) | (4, 12) | (6, 18));
            assert!(
                n_path || b_path,
                "unexpected structural record size {r} {c}"
            );
            let mut out = vec![0.0; r * c];
            if n_path {
                shapes_times_t(a, t, r, c, &mut out).unwrap();
                let got: Vec<u64> = out.iter().map(|v| v.to_bits()).collect();
                assert_eq!(got, expected, "N*T {r}x{c}");
            }
            if b_path {
                derivatives_times_t(a, t, r, c, &mut out).unwrap();
                let got: Vec<u64> = out.iter().map(|v| v.to_bits()).collect();
                assert_eq!(got, expected, "B*T {r}x{c}");
            }
            seen += 1;
        }
        assert_eq!(seen, 12, "structural records of the corpus");
    }
}
