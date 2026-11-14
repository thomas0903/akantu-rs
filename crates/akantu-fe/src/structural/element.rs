// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element_class_structural.hh,
// element_classes/element_class_hermite_inline_impl.hh,
// element_classes/element_class_bernoulli_beam_inline_impl.hh,
// element_classes/element_class_kirchhoff_shell_inline_impl.hh.

//! Structural element classes (ELEM-021/022/023/027): the cubic Hermite
//! helper `_itp_hermite_2`, the Euler–Bernoulli beams `_bernoulli_beam_2` /
//! `_bernoulli_beam_3` and the discrete Kirchhoff triangle
//! `_discrete_kirchhoff_triangle_18`.
//!
//! Every formula is transcribed verbatim from the upstream
//! `*_inline_impl.hh` files (same constants, parenthesisation and
//! left-to-right evaluation, no `mul_add`); reductions and products follow
//! the Eigen operand types of the upstream expressions (`crate::eigen`):
//!
//! | Upstream expression | Operands | Order |
//! |---|---|---|
//! | Hermite `computeLength`: `x1.distance(x2)` | columns of a dynamic coordinate matrix | `sqrt` of the sequential sum of squares |
//! | beam 2-D `(x2 − x1) / (x2 − x1).norm()` | dynamic columns | sequential |
//! | beam 3-D `x.normalize()`, DKT `a1.normalize()`, `….normalized()` | `Matrix<Real, 1, 3>` | halving tree `a₀² + (a₁² + a₂²)` |
//! | beam 3-D `Pe *= Pg.inverse()` | dynamic `3 × 3` | `PartialPivLU` inverse, then coefficient-based sequential product |
//! | DKT `P * real_coordinates` | fixed `3 × 3` times dynamic | coefficient-based, halving tree over 3 |
//! | DKT edge lengths `a.norm()` | `Vector<Real>` (dynamic) | sequential |
//!
//! Matrices are column-major slices: `N` is `NB_DOF × (NB_DOF · NB_NODES)`,
//! `dN/dξ` is `NATURAL_DIM × DNDS_COLUMNS`, `B` is
//! `NB_STRESS × (NB_DOF · NB_NODES)`, a rotation is `NB_DOF × NB_DOF`.
//!
//! **DKT shape functions.** Upstream's
//! `InterpolationElement<_itp_discrete_kirchhoff_triangle_18>::computeShapes`
//! is empty: the shape matrix is left uninitialised and `N·T` is undefined
//! (UD-053, UD-295). [`StructuralElement::compute_shapes`] returns
//! `Error::NotImplemented` for that type; the engine stores zeros and refuses
//! the operations that would use them.
//!
//! **3-D beam θy row (UD-293, D-003 B).** Upstream's θy row of `N` is
//! `+dw/ds` (`element_class_bernoulli_beam_inline_impl.hh:75–80`) although
//! the nodal rotations, the `w` row and `B` use `θy = −dw/ds`; at `ξ = −1`
//! the row does not even interpolate its own nodal `θy`. The default is the
//! corrected row `−dw/ds` (the four nonzero coefficients negated, an exact
//! sign change); [`StructuralUpstreamCompat::beam3_theta_y_legacy`]
//! reproduces upstream bit for bit (oracle replays).
//!
//! **3-D beam normal admissibility (UD-343, D-003 A with a legacy switch).
//! ** Upstream builds `R = Pe·Pg⁻¹` from any finite
//! normal; a non-unit or non-orthogonal normal silently gives a
//! non-orthogonal frame. By default a normal with `|n·n − 1|` or `|n·t̂|`
//! above [`BEAM3_NORMAL_ADMISSIBILITY`] (input validation, `t̂` the unit
//! axis) is refused with `Error::InvalidArgument`; admissible normals are
//! used raw, exactly as upstream (their results keep their bits);
//! [`StructuralUpstreamCompat::beam3_normal_unchecked`] reproduces upstream.
//!
//! **Degenerate geometry (UD-296).** Every kernel refuses
//! non-finite coordinates / normals and degenerate geometry with
//! `Error::Numerical` naming the element type, instead of dividing by zero:
//! the squared beam length, the squared first DKT edge, the squared DKT
//! normal `a₁ × (X₃ − X₁)`, the squared beam binormal `x × n` and the squared
//! DKT in-plane edges must be **normal** positive numbers (zero, subnormal —
//! i.e. underflowed —, infinite or NaN values are refused; an exact
//! floating-point criterion, not a tolerance), and the beam frame
//! `[x, x × n, n]` must have no zero LU pivot. Thin but finite elements are
//! accepted (no conditioning policy).

use akantu_core::{Error, Real, Result};

use crate::eigen::{self, Dims};
use crate::element_class::{ElementClass, types};
use crate::lagrange::{Interpolation, ItpSegment2, ItpTriangle3};

/// Column-major coordinate matrix (`rows × cols`) whose sizes are run-time
/// values for Eigen (every upstream call site passes a dynamic view or a
/// dynamic block of a product).
///
/// The fields are private: a `Coords` always satisfies
/// `data.len() ≥ rows · cols` (checked by [`Coords::new`]),
/// so a literal cannot bypass the check:
///
/// ```compile_fail,E0451
/// use akantu_fe::structural::Coords;
/// let x = Coords { data: &[], rows: 2, cols: 2 };
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Coords<'a> {
    data: &'a [Real],
    rows: usize,
    cols: usize,
}

impl<'a> Coords<'a> {
    /// View `data` as a `rows × cols` column-major matrix (the first
    /// `rows · cols` values; any further values are ignored).
    ///
    /// # Errors
    /// `Error::DimensionMismatch` if `data` is shorter than `rows · cols`.
    pub fn new(data: &'a [Real], rows: usize, cols: usize) -> Result<Self> {
        let need = rows.checked_mul(cols).ok_or(Error::DimensionMismatch {
            context: "structural coordinates: size overflow",
            expected: usize::MAX,
            got: data.len(),
        })?;
        if data.len() < need {
            return Err(Error::DimensionMismatch {
                context: "structural coordinates",
                expected: need,
                got: data.len(),
            });
        }
        Ok(Self { data, rows, cols })
    }

    /// The `rows · cols` values, column-major.
    #[must_use]
    pub fn data(&self) -> &'a [Real] {
        &self.data[..self.rows * self.cols]
    }

    /// Rows (spatial or local dimension).
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Columns (nodes).
    #[must_use]
    pub fn cols(&self) -> usize {
        self.cols
    }

    #[inline]
    fn at(&self, i: usize, j: usize) -> Real {
        self.data[i + j * self.rows]
    }

    /// `rows` exactly, at least `cols` columns.
    fn require(&self, rows: usize, cols: usize, context: &'static str) -> Result<()> {
        if self.rows != rows {
            return Err(Error::DimensionMismatch {
                context,
                expected: rows,
                got: self.rows,
            });
        }
        if self.cols < cols {
            return Err(Error::DimensionMismatch {
                context,
                expected: cols,
                got: self.cols,
            });
        }
        Ok(())
    }

    /// Refuse non-finite values in the first `cols` columns.
    fn require_finite(&self, cols: usize, element: &str) -> Result<()> {
        for j in 0..cols {
            for i in 0..self.rows {
                let v = self.at(i, j);
                if !v.is_finite() {
                    return Err(Error::Numerical(format!(
                        "{element}: non-finite node coordinate {v} (component {i} of node {j})"
                    )));
                }
            }
        }
        Ok(())
    }

    /// akantu `MatrixBase::distance` squared: `(col(a) − col(b)).squaredNorm()`
    /// of dynamic columns (sequential sum of squares); `distance` is its
    /// square root, bit for bit.
    fn squared_distance(&self, a: usize, b: usize) -> Real {
        eigen::squared_norm(self.rows, false, |i| self.at(i, a) - self.at(i, b))
    }
}

/// Upstream-compatibility switches of the structural kernels and of
/// [`crate::structural::StructuralFeEngine`] (`docs/DECISIONS.md` D-003
/// category B: corrected behavior by default, upstream behavior on request
/// for differential tests against the unpatched oracle).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StructuralUpstreamCompat {
    /// **UD-293**: upstream's θy row of the
    /// `_bernoulli_beam_3` shape matrix is `+dw/ds` while the DOF
    /// convention (nodal rotations, `w` row, `B`) is `θy = −dw/ds`, so the
    /// interpolated θy and the moment part of line loads have the wrong
    /// sign. Default (`false`): the corrected row `−dw/ds`; `true`: the
    /// upstream row (consistent mass is unaffected: it uses the first
    /// `nb_stress` rows only).
    pub beam3_theta_y_legacy: bool,
    /// **UD-343**: upstream builds the
    /// `_bernoulli_beam_3` frame `R = Pe·Pg⁻¹` from any finite normal, so a
    /// non-unit or non-orthogonal `extra_normal` silently gives a
    /// non-orthogonal `R` (wrong frames and stiffness). Default (`false`):
    /// such a normal is refused (see [`BEAM3_NORMAL_ADMISSIBILITY`]);
    /// `true`: no admissibility check, upstream's frames for any finite
    /// normal (zero, parallel and non-finite normals stay refused, UD-296).
    pub beam3_normal_unchecked: bool,
}

impl StructuralUpstreamCompat {
    /// Every upstream behavior (oracle replays).
    pub const UPSTREAM: Self = Self {
        beam3_theta_y_legacy: true,
        beam3_normal_unchecked: true,
    };
}

/// Admissibility bound of a `_bernoulli_beam_3` normal `n` (UD-343):
/// by default `n` is refused when `|n·n − 1| >` this bound or
/// `|n·t̂| >` this bound, `t̂` the unit beam axis. **Input validation**, not
/// a tolerance on results: it only separates normals that are clearly not
/// unit vectors orthogonal to the axis (user data errors such as a
/// non-normalised or skew normal) from admissible ones; admissible normals
/// are used raw, exactly as upstream (no normalisation or
/// orthogonalisation), so their results keep their bits. `1e-10` is
/// ~`4.5e5` ulps of 1: far above the rounding of normals computed in
/// double precision (a few ulps), far below any physically meaningful
/// deviation (`docs/NUMERICAL_VALIDATION.md`).
pub const BEAM3_NORMAL_ADMISSIBILITY: Real = 1e-10;

/// `z` is a positive normal floating-point number (not zero, subnormal,
/// infinite or NaN): the exact admissibility criterion of a squared length
/// that is later inverted (UD-296).
#[inline]
fn positive_normal(z: Real) -> bool {
    z.is_normal() && z > 0.0
}

/// Message of a refused squared length `z` (not a positive normal number):
/// an overflow (`inf`, finite coordinates too large for double precision)
/// is reported as such, otherwise `degenerate` (zero or underflowed).
fn length_refusal(z: Real, degenerate: &str, what: &str) -> String {
    if z.is_infinite() {
        format!("element too large ({what} = {z:e} overflows double precision)")
    } else {
        format!(
            "{degenerate} ({what} = {z:e} is not a positive normal number: zero or underflowed)"
        )
    }
}

/// Refuse non-finite values of a vector (beam normal).
fn require_finite_vector(v: &[Real], element: &str, what: &str) -> Result<()> {
    if let Some((i, x)) = v.iter().enumerate().find(|(_, x)| !x.is_finite()) {
        return Err(Error::Numerical(format!(
            "{element}: non-finite {what} {x} (component {i})"
        )));
    }
    Ok(())
}

/// Structural interpolation kernels (upstream
/// `InterpolationElement<itp, _itk_structural>` and
/// `ElementClass<type, _ek_structural>`), implemented by
/// [`types::BernoulliBeam2`], [`types::BernoulliBeam3`] and
/// [`types::DiscreteKirchhoffTriangle18`].
pub trait StructuralElement: ElementClass {
    /// Lagrange interpolation of the parent geometry (`parent_element_type`:
    /// `_segment_2` for beams, `_triangle_3` for DKT), used for the Jacobian
    /// (`computeJMat`).
    type ParentItp: Interpolation;
    /// DOFs per node (`getNbDegreeOfFreedom`).
    const NB_DOF_PER_NODE: usize;
    /// Generalised stress / strain components (`getNbStressComponents`).
    const NB_STRESS: usize;
    /// Columns of `dN/dξ` (`interpolation_property::dnds_columns`).
    const DNDS_COLUMNS: usize;
    /// `false` where upstream's `computeShapes` is empty (DKT, UD-053).
    const HAS_SHAPES: bool;
    /// Columns of `N`, `B` and of the DOF rotation `T`
    /// (`NB_DOF_PER_NODE · NB_NODES`).
    const NB_ELEMENT_DOFS: usize = Self::NB_DOF_PER_NODE * Self::NB_NODES;

    /// Global-to-local rotation of the nodal DOFs (`computeRotationMatrix`,
    /// `NB_DOF × NB_DOF` column-major). `x` holds the node coordinates
    /// (spatial dimension × nodes); `extra_normal` the beam normal (3-D
    /// beams only, `spatial_dimension` values; ignored otherwise).
    /// `compat.beam3_normal_unchecked` disables the admissibility check of
    /// the 3-D beam normal (UD-343); the other types ignore `compat`.
    ///
    /// # Errors
    /// `Error::DimensionMismatch` for coordinates / normals / outputs of the
    /// wrong size (beam 2-D: exactly 2 coordinate rows; beam 3-D and DKT:
    /// exactly 3; beam 3-D normal: exactly 3 values); `Error::Numerical` for
    /// non-finite coordinates / normals and degenerate geometry (zero or
    /// underflowed length, collinear triangle, normal parallel to the beam
    /// axis or zero; UD-296); `Error::InvalidArgument` for a 3-D beam normal
    /// that is not a unit vector orthogonal to the axis within
    /// [`BEAM3_NORMAL_ADMISSIBILITY`] (UD-343, unless
    /// `compat.beam3_normal_unchecked`).
    fn compute_rotation_matrix_with(
        x: Coords<'_>,
        extra_normal: &[Real],
        compat: StructuralUpstreamCompat,
        r: &mut [Real],
    ) -> Result<()>;

    /// [`StructuralElement::compute_rotation_matrix_with`] with the
    /// corrected default behavior (admissibility check on).
    ///
    /// # Errors
    /// See [`StructuralElement::compute_rotation_matrix_with`].
    fn compute_rotation_matrix(x: Coords<'_>, extra_normal: &[Real], r: &mut [Real]) -> Result<()> {
        Self::compute_rotation_matrix_with(x, extra_normal, StructuralUpstreamCompat::default(), r)
    }

    /// Shape matrix `N` at `xi` (`computeShapes(natural_coords,
    /// real_coord, N)`); `real_coord` provides the element length (beams).
    /// `compat` selects the upstream θy row of `_bernoulli_beam_3`
    /// (UD-293); the other types ignore it.
    ///
    /// # Errors
    /// `Error::NotImplemented` for DKT (empty upstream, UD-053);
    /// `Error::DimensionMismatch` for malformed inputs; `Error::Numerical`
    /// for a degenerate or non-finite element length (UD-296).
    fn compute_shapes_with(
        xi: &[Real],
        real_coord: Coords<'_>,
        compat: StructuralUpstreamCompat,
        n: &mut [Real],
    ) -> Result<()>;

    /// [`StructuralElement::compute_shapes_with`] with the corrected
    /// default behavior.
    ///
    /// # Errors
    /// See [`StructuralElement::compute_shapes_with`].
    fn compute_shapes(xi: &[Real], real_coord: Coords<'_>, n: &mut [Real]) -> Result<()> {
        Self::compute_shapes_with(xi, real_coord, StructuralUpstreamCompat::default(), n)
    }

    /// `dN/dξ` at `xi` (`computeDNDS(natural_coords, real_coord, dnds)`),
    /// `NATURAL_DIM × DNDS_COLUMNS`.
    ///
    /// # Errors
    /// `Error::DimensionMismatch` for malformed inputs; `Error::Numerical`
    /// for degenerate or non-finite geometry (UD-296).
    fn compute_dnds(xi: &[Real], real_coord: Coords<'_>, dnds: &mut [Real]) -> Result<()>;

    /// Strain–displacement matrix `B` (`NB_STRESS × NB_ELEMENT_DOFS`) from
    /// the physical derivatives `dndx` (`NATURAL_DIM × DNDS_COLUMNS`),
    /// upstream `arrangeInVoigt`.
    ///
    /// # Errors
    /// `Error::DimensionMismatch` if `dndx` holds fewer than
    /// `NATURAL_DIM · DNDS_COLUMNS` values or `b` fewer than
    /// `NB_STRESS · NB_ELEMENT_DOFS`.
    fn arrange_in_voigt(dndx: &[Real], b: &mut [Real]) -> Result<()>;
}

fn check_len(context: &'static str, got: usize, expected: usize) -> Result<()> {
    if got < expected {
        return Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// _itp_hermite_2 (ELEM-027)
// ---------------------------------------------------------------------------

/// Hermite shape values for the half length `a` (`details::computeShapes`):
/// column-major `2 × 4`, row 0 the cubic displacement functions
/// `(M1, L1, M2, L2)`, row 1 their `x`-derivatives (rotations). Unchecked:
/// `a` comes from [`hermite_length`] (public entry: [`hermite_shapes`]).
#[must_use]
fn hermite_shapes_half_length(xi: Real, a: Real) -> [Real; 8] {
    let xi2 = xi * xi;
    let xi3 = xi * xi * xi;
    let m1 = 1. / 4. * (2. - 3. * xi + xi3);
    let m2 = 1. / 4. * (2. + 3. * xi - xi3);
    let l1 = a / 4. * (1. - xi - xi2 + xi3);
    let l2 = a / 4. * (-1. - xi + xi2 + xi3);
    let m1_ = 3. / (4. * a) * (xi2 - 1.);
    let m2_ = 3. / (4. * a) * (1. - xi2);
    let l1_ = 1. / 4. * (3. * xi2 - 2. * xi - 1.);
    let l2_ = 1. / 4. * (3. * xi2 + 2. * xi - 1.);
    // N << M1, L1, M2, L2,  M1_, L1_, M2_, L2_  (row-wise), stored column-major
    [m1, m1_, l1, l1_, m2, m2_, l2, l2_]
}

/// Hermite curvature row for the half length `a` (`details::computeDNDS`):
/// `(M1, L1, M2, L2) / a`. Unchecked: `a` comes from [`hermite_length`]
/// (public entry: [`hermite_dnds`]).
#[must_use]
fn hermite_dnds_half_length(xi: Real, a: Real) -> [Real; 4] {
    let m1 = 3. / 2. * xi;
    let m2 = 3. / 2. * (-xi);
    let l1 = 1. * a / 2. * (3. * xi - 1.);
    let l2 = 1. * a / 2. * (3. * xi + 1.);
    [m1 / a, l1 / a, m2 / a, l2 / a]
}

/// Element length used by the Hermite functions
/// (`details::computeLength(real_coord)` = `x1.distance(x2)`).
///
/// # Errors
/// `Error::DimensionMismatch` if `x` has fewer than two columns;
/// `Error::Numerical` for non-finite coordinates or a squared length that
/// is not a positive normal number (zero / underflowed length: the Hermite
/// functions divide by it, UD-296).
pub fn hermite_length(x: Coords<'_>) -> Result<Real> {
    check_len("hermite length: nodes", x.cols, 2)?;
    x.require_finite(2, "_itp_hermite_2")?;
    let z = x.squared_distance(0, 1);
    if !positive_normal(z) {
        return Err(Error::Numerical(format!(
            "_itp_hermite_2: {}",
            length_refusal(z, "degenerate element length", "squared length")
        )));
    }
    Ok(z.sqrt())
}

/// `InterpolationElement<_itp_hermite_2>::computeShapes`: `L = length`,
/// `a = L / 2`.
///
/// # Errors
/// See [`hermite_length`].
pub fn hermite_shapes(xi: Real, x: Coords<'_>) -> Result<[Real; 8]> {
    let l = hermite_length(x)?;
    Ok(hermite_shapes_half_length(xi, l / 2.))
}

/// `InterpolationElement<_itp_hermite_2>::computeDNDS`.
///
/// # Errors
/// See [`hermite_length`].
pub fn hermite_dnds(xi: Real, x: Coords<'_>) -> Result<[Real; 4]> {
    let l = hermite_length(x)?;
    Ok(hermite_dnds_half_length(xi, l / 2.))
}

fn segment2_shapes(xi: &[Real]) -> [Real; 2] {
    let mut l = [0.0; 2];
    ItpSegment2::shapes(xi, &mut l);
    l
}

fn segment2_dnds(xi: &[Real]) -> [Real; 2] {
    let mut l = [0.0; 2];
    ItpSegment2::dnds(xi, &mut l);
    l
}

// ---------------------------------------------------------------------------
// _bernoulli_beam_2 (ELEM-021)
// ---------------------------------------------------------------------------

/// Beam `computeDNDS` (2-D and 3-D): `[dL/dξ (2), Hermite curvature (4)]`.
fn beam_dnds(xi: &[Real], real_coord: Coords<'_>, dnds: &mut [Real]) -> Result<()> {
    check_len("beam dnds: natural coordinates", xi.len(), 1)?;
    check_len("beam dnds: output", dnds.len(), 6)?;
    let l = segment2_dnds(xi);
    let h = hermite_dnds(xi[0], real_coord)?;
    dnds[..2].copy_from_slice(&l);
    dnds[2..6].copy_from_slice(&h);
    Ok(())
}

impl StructuralElement for types::BernoulliBeam2 {
    type ParentItp = ItpSegment2;
    const NB_DOF_PER_NODE: usize = 3;
    const NB_STRESS: usize = 2;
    const DNDS_COLUMNS: usize = 6;
    const HAS_SHAPES: bool = true;

    fn compute_rotation_matrix_with(
        x: Coords<'_>,
        _n: &[Real],
        _compat: StructuralUpstreamCompat,
        r: &mut [Real],
    ) -> Result<()> {
        x.require(2, 2, "beam 2 rotation: coordinates (2 × 2)")?;
        check_len("beam 2 rotation: output", r.len(), 9)?;
        x.require_finite(2, "_bernoulli_beam_2")?;
        // cs = (x2 - x1) / (x2 - x1).norm()  (dynamic columns: sequential)
        let z = x.squared_distance(1, 0);
        if !positive_normal(z) {
            return Err(Error::Numerical(format!(
                "_bernoulli_beam_2: {}",
                length_refusal(z, "degenerate element", "squared length")
            )));
        }
        let nrm = z.sqrt();
        let c = (x.at(0, 1) - x.at(0, 0)) / nrm;
        let s = (x.at(1, 1) - x.at(1, 0)) / nrm;
        // R = {{c, s, 0}, {-s, c, 0}, {0, 0, 1}}, column-major
        r[..9].copy_from_slice(&[c, -s, 0., s, c, 0., 0., 0., 1.]);
        Ok(())
    }

    fn compute_shapes_with(
        xi: &[Real],
        real_coord: Coords<'_>,
        _compat: StructuralUpstreamCompat,
        n: &mut [Real],
    ) -> Result<()> {
        check_len("beam 2 shapes: natural coordinates", xi.len(), 1)?;
        check_len("beam 2 shapes: output", n.len(), 18)?;
        let l = segment2_shapes(xi);
        let h = hermite_shapes(xi[0], real_coord)?;
        let hh = |r: usize, c: usize| h[r + 2 * c];
        n[..18].fill(0.0);
        let mut set = |r: usize, c: usize, v: Real| n[r + 3 * c] = v;
        //       u1      v1        t1        u2      v2        t2
        set(0, 0, l[0]);
        set(0, 3, l[1]);
        set(1, 1, hh(0, 0));
        set(1, 2, hh(0, 1));
        set(1, 4, hh(0, 2));
        set(1, 5, hh(0, 3));
        set(2, 1, hh(1, 0));
        set(2, 2, hh(1, 1));
        set(2, 4, hh(1, 2));
        set(2, 5, hh(1, 3));
        Ok(())
    }

    fn compute_dnds(xi: &[Real], real_coord: Coords<'_>, dnds: &mut [Real]) -> Result<()> {
        beam_dnds(xi, real_coord, dnds)
    }

    fn arrange_in_voigt(dndx: &[Real], b: &mut [Real]) -> Result<()> {
        check_len("beam 2 arrange_in_voigt: dN/dx", dndx.len(), 6)?;
        check_len("beam 2 arrange_in_voigt: output", b.len(), 12)?;
        let (l, h) = (&dndx[..2], &dndx[2..6]);
        b[..12].fill(0.0);
        let mut set = |r: usize, c: usize, v: Real| b[r + 2 * c] = v;
        set(0, 0, l[0]);
        set(0, 3, l[1]);
        set(1, 1, -h[0]);
        set(1, 2, -h[1]);
        set(1, 4, -h[2]);
        set(1, 5, -h[3]);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// _bernoulli_beam_3 (ELEM-022)
// ---------------------------------------------------------------------------

/// `n` as a 3-vector (upstream assigns it to `Eigen::Matrix<Real, 1, 3>`):
/// exactly 3 values.
fn vec3(v: &[Real], context: &'static str) -> Result<[Real; 3]> {
    match v {
        [a, b, c] => Ok([*a, *b, *c]),
        _ => Err(Error::DimensionMismatch {
            context,
            expected: 3,
            got: v.len(),
        }),
    }
}

impl StructuralElement for types::BernoulliBeam3 {
    type ParentItp = ItpSegment2;
    const NB_DOF_PER_NODE: usize = 6;
    const NB_STRESS: usize = 4;
    const DNDS_COLUMNS: usize = 6;
    const HAS_SHAPES: bool = true;

    fn compute_rotation_matrix_with(
        x: Coords<'_>,
        n: &[Real],
        compat: StructuralUpstreamCompat,
        r: &mut [Real],
    ) -> Result<()> {
        const BEAM3: &str = "_bernoulli_beam_3";
        x.require(3, 2, "beam 3 rotation: coordinates (3 × 2)")?;
        check_len("beam 3 rotation: output", r.len(), 36)?;
        let nv = vec3(n, "beam 3 rotation: extra normal (3 values)")?;
        x.require_finite(2, BEAM3)?;
        require_finite_vector(&nv, BEAM3, "extra_normal")?;
        // Eigen::Matrix<Real, 1, 3> x = X(1) - X(0); x.normalize();
        let mut xv = [0.0; 3];
        for (i, v) in xv.iter_mut().enumerate() {
            *v = x.at(i, 1) - x.at(i, 0);
        }
        let z = eigen::squared_norm(3, true, |i| xv[i]);
        if !positive_normal(z) {
            return Err(Error::Numerical(format!(
                "{BEAM3}: {}",
                length_refusal(z, "degenerate element", "squared length")
            )));
        }
        eigen::normalize(&mut xv, true);
        let x_n = eigen::cross(&xv, &nv);
        let zb = eigen::squared_norm(3, true, |i| x_n[i]);
        if zb.is_infinite() {
            return Err(Error::Numerical(format!(
                "{BEAM3}: extra_normal {nv:?} too large (squared |x × n| = {zb:e} overflows \
                 double precision)"
            )));
        }
        if !positive_normal(zb) {
            return Err(Error::Numerical(format!(
                "{BEAM3}: extra_normal {nv:?} is zero or parallel to the beam axis (squared \
                 |x × n| = {zb:e} is not a positive normal number)"
            )));
        }
        if !compat.beam3_normal_unchecked {
            // UD-343: input validation of the normal (used raw below, as
            // upstream; the check computes on copies and changes no value)
            let nn = eigen::squared_norm(3, true, |i| nv[i]);
            let nt = nv[0] * xv[0] + nv[1] * xv[1] + nv[2] * xv[2];
            let (dev_unit, dev_orth) = ((nn - 1.0).abs(), nt.abs());
            if dev_unit > BEAM3_NORMAL_ADMISSIBILITY || dev_orth > BEAM3_NORMAL_ADMISSIBILITY {
                return Err(Error::InvalidArgument(format!(
                    "{BEAM3}: inadmissible extra_normal {nv:?}: not a unit vector orthogonal to \
                     the beam axis (|n·n − 1| = {dev_unit:e}, |n·t| = {dev_orth:e}, bound \
                     {BEAM3_NORMAL_ADMISSIBILITY:e}; UD-343: upstream builds a non-orthogonal \
                     frame; StructuralUpstreamCompat::beam3_normal_unchecked reproduces it)"
                )));
            }
        }
        // Pe = diag(1, -1, 1) (comma initialiser), Pg columns x, x × n, n
        let pe = [1., 0., 0., 0., -1., 0., 0., 0., 1.];
        let mut pg = [0.0; 9];
        pg[..3].copy_from_slice(&xv);
        pg[3..6].copy_from_slice(&x_n);
        pg[6..9].copy_from_slice(&nv);
        // Pe *= Pg.inverse(): dynamic inverse, then Pe = Pe * inv (dynamic,
        // 3 + 3 + 3 < 20: coefficient-based, sequential)
        let mut inv = [0.0; 9];
        if eigen::lu_inverse(3, &pg, &mut inv) {
            return Err(Error::Numerical(format!(
                "{BEAM3}: singular frame [x, x × n, n] (zero LU pivot) for extra_normal {nv:?}"
            )));
        }
        let mut p = [0.0; 9];
        eigen::product_slices(Dims::dynamic(3, 3), Dims::dynamic(3, 3), &pe, &inv, &mut p)?;
        if let Some(v) = p.iter().find(|v| !v.is_finite()) {
            return Err(Error::Numerical(format!(
                "{BEAM3}: non-finite rotation entry {v} (ill-conditioned frame for extra_normal \
                 {nv:?})"
            )));
        }
        r[..36].fill(0.0);
        for i in 0..3 {
            for j in 0..3 {
                let v = p[i + 3 * j];
                r[i + 6 * j] = v;
                r[(i + 3) + 6 * (j + 3)] = v;
            }
        }
        Ok(())
    }

    fn compute_shapes_with(
        xi: &[Real],
        real_coord: Coords<'_>,
        compat: StructuralUpstreamCompat,
        n: &mut [Real],
    ) -> Result<()> {
        check_len("beam 3 shapes: natural coordinates", xi.len(), 1)?;
        check_len("beam 3 shapes: output", n.len(), 72)?;
        let l = segment2_shapes(xi);
        let h = hermite_shapes(xi[0], real_coord)?;
        let hh = |r: usize, c: usize| h[r + 2 * c];
        n[..72].fill(0.0);
        let mut set = |r: usize, c: usize, v: Real| n[r + 6 * c] = v;
        // u1 v1 w1 tx1 ty1 tz1 u2 v2 w2 tx2 ty2 tz2
        set(0, 0, l[0]);
        set(0, 6, l[1]);
        set(1, 1, hh(0, 0));
        set(1, 5, hh(0, 1));
        set(1, 7, hh(0, 2));
        set(1, 11, hh(0, 3));
        set(2, 2, hh(0, 0));
        set(2, 4, -hh(0, 1));
        set(2, 8, hh(0, 2));
        set(2, 10, -hh(0, 3));
        set(3, 3, l[0]);
        set(3, 9, l[1]);
        if compat.beam3_theta_y_legacy {
            // upstream θy row: +dw/ds (UD-293)
            set(4, 2, hh(1, 0));
            set(4, 4, -hh(1, 1));
            set(4, 8, hh(1, 2));
            set(4, 10, -hh(1, 3));
        } else {
            // θy = −dw/ds, the convention of the w row and of B
            set(4, 2, -hh(1, 0));
            set(4, 4, hh(1, 1));
            set(4, 8, -hh(1, 2));
            set(4, 10, hh(1, 3));
        }
        set(5, 1, hh(1, 0));
        set(5, 5, hh(1, 1));
        set(5, 7, hh(1, 2));
        set(5, 11, hh(1, 3));
        Ok(())
    }

    fn compute_dnds(xi: &[Real], real_coord: Coords<'_>, dnds: &mut [Real]) -> Result<()> {
        beam_dnds(xi, real_coord, dnds)
    }

    fn arrange_in_voigt(dndx: &[Real], b: &mut [Real]) -> Result<()> {
        check_len("beam 3 arrange_in_voigt: dN/dx", dndx.len(), 6)?;
        check_len("beam 3 arrange_in_voigt: output", b.len(), 48)?;
        let (l, h) = (&dndx[..2], &dndx[2..6]);
        b[..48].fill(0.0);
        let mut set = |r: usize, c: usize, v: Real| b[r + 4 * c] = v;
        // eps
        set(0, 0, l[0]);
        set(0, 6, l[1]);
        // chi strong axis
        set(1, 1, -h[0]);
        set(1, 5, -h[1]);
        set(1, 7, -h[2]);
        set(1, 11, -h[3]);
        // chi weak axis
        set(2, 2, -h[0]);
        set(2, 4, h[1]);
        set(2, 8, -h[2]);
        set(2, 10, h[3]);
        // chi torsion
        set(3, 3, l[0]);
        set(3, 9, l[1]);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// _discrete_kirchhoff_triangle_18 (ELEM-023)
// ---------------------------------------------------------------------------

/// `detail::computeBasisChangeMatrix(X)` of the Kirchhoff shell: rows
/// `a1 = (X2 − X1)/|·|`, `e2 = e3 × a1`, `e3 = (a1 × (X3 − X1))/|·|`
/// (column-major `3 × 3`, the transpose of `P` with columns `a1, e2, e3`).
///
/// # Errors
/// `Error::DimensionMismatch` unless `x` has 3 rows and at least 3 columns;
/// `Error::Numerical` for non-finite coordinates, a zero / underflowed first
/// edge or a collinear (or underflowed) triangle: the squared norms of
/// `X₂ − X₁` and of `a₁ × (X₃ − X₁)` must be positive normal numbers
/// (UD-296).
pub fn dkt_basis_change(x: Coords<'_>) -> Result<[Real; 9]> {
    const DKT: &str = "_discrete_kirchhoff_triangle_18";
    x.require(3, 3, "DKT basis: coordinates (3 × 3)")?;
    x.require_finite(3, DKT)?;
    let mut a1 = [0.0; 3];
    let mut a2 = [0.0; 3];
    for i in 0..3 {
        a1[i] = x.at(i, 1) - x.at(i, 0);
        a2[i] = x.at(i, 2) - x.at(i, 0);
    }
    let z1 = eigen::squared_norm(3, true, |i| a1[i]);
    if !positive_normal(z1) {
        return Err(Error::Numerical(format!(
            "{DKT}: {}",
            length_refusal(z1, "degenerate element", "squared edge X2 − X1")
        )));
    }
    eigen::normalize(&mut a1, true);
    let mut e3 = eigen::cross(&a1, &a2);
    let z3 = eigen::squared_norm(3, true, |i| e3[i]);
    if !positive_normal(z3) {
        return Err(Error::Numerical(format!(
            "{DKT}: {}",
            length_refusal(
                z3,
                "degenerate (collinear or underflowed) triangle",
                "squared |a1 × (X3 − X1)|"
            )
        )));
    }
    eigen::normalize(&mut e3, true);
    let e2 = eigen::cross(&e3, &a1);
    // P.transpose()(i, j) = row i of [a1; e2; e3]
    let mut pt = [0.0; 9];
    for j in 0..3 {
        pt[3 * j] = a1[j];
        pt[1 + 3 * j] = e2[j];
        pt[2 + 3 * j] = e3[j];
    }
    Ok(pt)
}

impl StructuralElement for types::DiscreteKirchhoffTriangle18 {
    type ParentItp = ItpTriangle3;
    const NB_DOF_PER_NODE: usize = 6;
    const NB_STRESS: usize = 6;
    const DNDS_COLUMNS: usize = 21;
    const HAS_SHAPES: bool = false;

    fn compute_rotation_matrix_with(
        x: Coords<'_>,
        _n: &[Real],
        _compat: StructuralUpstreamCompat,
        r: &mut [Real],
    ) -> Result<()> {
        check_len("DKT rotation: output", r.len(), 36)?;
        let p = dkt_basis_change(x)?;
        r[..36].fill(0.0);
        for i in 0..3 {
            for j in 0..3 {
                let v = p[i + 3 * j];
                r[i + 6 * j] = v;
                r[(i + 3) + 6 * (j + 3)] = v;
            }
        }
        Ok(())
    }

    fn compute_shapes_with(
        _xi: &[Real],
        _x: Coords<'_>,
        _compat: StructuralUpstreamCompat,
        _n: &mut [Real],
    ) -> Result<()> {
        Err(Error::NotImplemented(
            "shape functions of _discrete_kirchhoff_triangle_18 (empty upstream, UD-053)",
        ))
    }

    #[allow(clippy::many_single_char_names)] // upstream names (P, X, A, L, C, S)
    fn compute_dnds(xi: &[Real], real_coord: Coords<'_>, b: &mut [Real]) -> Result<()> {
        check_len("DKT dnds: natural coordinates", xi.len(), 2)?;
        check_len("DKT dnds: output", b.len(), 42)?;
        let p = dkt_basis_change(real_coord)?;
        // X = P * real_coordinates (fixed 3 × 3 times dynamic: coefficient-based)
        let nn = 3;
        let mut xl = [0.0; 9];
        eigen::product(
            Dims::fixed(3, 3),
            Dims::dynamic(3, nn),
            |i, k| p[i + 3 * k],
            |k, j| real_coord.at(k, j),
            &mut xl,
        )?;
        let col = |j: usize| [xl[3 * j], xl[1 + 3 * j], xl[2 + 3 * j]];
        let (x1, x2, x3) = (col(0), col(1), col(2));
        let diff = |a: [Real; 3], b: [Real; 3]| [a[0] - b[0], a[1] - b[1], 0.0];
        let a = [diff(x2, x1), diff(x3, x2), diff(x1, x3)];
        let mut l = [0.0; 3];
        let mut c = [0.0; 3];
        let mut s = [0.0; 3];
        for k in 0..3 {
            // a.norm() = sqrt(a.squaredNorm()) (dynamic: sequential)
            let z = eigen::squared_norm(3, false, |i| a[k][i]);
            if !positive_normal(z) {
                return Err(Error::Numerical(format!(
                    "_discrete_kirchhoff_triangle_18: {}",
                    length_refusal(
                        z,
                        &format!("degenerate in-plane edge {k}"),
                        "squared length"
                    )
                )));
            }
            l[k] = z.sqrt();
        }
        for k in 0..3 {
            c[k] = a[k][0] / l[k];
        }
        for k in 0..3 {
            s[k] = a[k][1] / l[k];
        }
        let xi_ = xi[0];
        let eta = xi[1];
        // dP (2 × 3, row-wise initialiser)
        let dp = [
            [4. * (1. - 2. * xi_ - eta), 4. * eta, -4. * eta],
            [-4. * xi_, 4. * xi_, 4. * (1. - xi_ - 2. * eta)],
        ];
        // (a, b) index pairs of the three columns: (0,2), (1,0), (2,1)
        const PAIRS: [(usize, usize); 3] = [(0, 2), (1, 0), (2, 1)];
        let mut dnx1 = [[0.0; 3]; 2];
        let mut dnx2 = [[0.0; 3]; 2];
        let mut dnx3 = [[0.0; 3]; 2];
        let mut dny1 = [[0.0; 3]; 2];
        let mut dny3 = [[0.0; 3]; 2];
        for r in 0..2 {
            for (col_i, &(ia, ib)) in PAIRS.iter().enumerate() {
                let d = dp[r];
                dnx1[r][col_i] = 3. / 2. * (d[ia] * c[ia] / l[ia] - d[ib] * c[ib] / l[ib]);
                dny1[r][col_i] = 3. / 2. * (d[ia] * s[ia] / l[ia] - d[ib] * s[ib] / l[ib]);
                dnx3[r][col_i] = -3. / 4. * (d[ia] * c[ia] * s[ia] + d[ib] * c[ib] * s[ib]);
                let sxx = d[ia] * c[ia] * c[ia] + d[ib] * c[ib] * c[ib];
                let syy = d[ia] * s[ia] * s[ia] + d[ib] * s[ib] * s[ib];
                // constants of dNx2 / dNy3: row 0: (-1, 1, none), row 1: (-1, none, 1)
                let k: Option<Real> = match (r, col_i) {
                    (_, 0) => Some(-1.),
                    (0, 1) | (1, 2) => Some(1.),
                    _ => None,
                };
                (dnx2[r][col_i], dny3[r][col_i]) = match k {
                    Some(k) => (k - 3. / 4. * sxx, k - 3. / 4. * syy),
                    None => (-3. / 4. * sxx, -3. / 4. * syy),
                };
            }
        }
        let dny2 = dnx3;
        // linear membrane functions
        let mut dnm = [0.0; 6];
        ItpTriangle3::dnds(xi, &mut dnm);
        let as_rows = |m: [[Real; 3]; 2]| {
            let mut o = [0.0; 6];
            for (r, row) in m.iter().enumerate() {
                for (cc, v) in row.iter().enumerate() {
                    o[r + 2 * cc] = *v;
                }
            }
            o
        };
        let blocks = [
            dnm,
            as_rows(dnx1),
            as_rows(dnx2),
            as_rows(dnx3),
            as_rows(dny1),
            as_rows(dny2),
            as_rows(dny3),
        ];
        for (k, m) in blocks.iter().enumerate() {
            b[6 * k..6 * k + 6].copy_from_slice(m);
        }
        Ok(())
    }

    fn arrange_in_voigt(dndx: &[Real], b: &mut [Real]) -> Result<()> {
        check_len("DKT arrange_in_voigt: dN/dx", dndx.len(), 42)?;
        check_len("DKT arrange_in_voigt: output", b.len(), 108)?;
        // block k occupies the columns 3k..3k+3 of the 2 × 21 matrix
        let m = |k: usize, r: usize, i: usize| dndx[r + 2 * (3 * k + i)];
        let (dnm, dnx1, dnx2, dnx3, dny1, dny2, dny3) = (0, 1, 2, 3, 4, 5, 6);
        b[..108].fill(0.0);
        let mut set = |r: usize, c: usize, v: Real| b[r + 6 * c] = v;
        for i in 0..3 {
            let c0 = 6 * i;
            // Bm
            set(0, c0, m(dnm, 0, i));
            set(1, c0 + 1, m(dnm, 1, i));
            set(2, c0, m(dnm, 1, i));
            set(2, c0 + 1, m(dnm, 0, i));
            // Bf
            set(3, c0 + 2, m(dnx1, 0, i));
            set(3, c0 + 3, -m(dnx3, 0, i));
            set(3, c0 + 4, m(dnx2, 0, i));
            set(4, c0 + 2, m(dny1, 1, i));
            set(4, c0 + 3, -m(dny3, 1, i));
            set(4, c0 + 4, m(dny2, 1, i));
            set(5, c0 + 2, m(dnx1, 1, i) + m(dny1, 0, i));
            set(5, c0 + 3, -m(dnx3, 1, i) - m(dny3, 0, i));
            set(5, c0 + 4, m(dnx2, 1, i) + m(dny2, 0, i));
        }
        Ok(())
    }
}

/// Run `$body` with `$E` bound to the structural element class of `$ty`
/// (`_bernoulli_beam_2`, `_bernoulli_beam_3`,
/// `_discrete_kirchhoff_triangle_18`), else evaluate `$fallback`.
#[macro_export]
macro_rules! with_structural_element {
    ($ty:expr, $E:ident => $body:expr, else $fallback:expr) => {{
        match $ty {
            akantu_core::ElementType::BernoulliBeam2 => {
                type $E = $crate::element_class::types::BernoulliBeam2;
                $body
            }
            akantu_core::ElementType::BernoulliBeam3 => {
                type $E = $crate::element_class::types::BernoulliBeam3;
                $body
            }
            akantu_core::ElementType::DiscreteKirchhoffTriangle18 => {
                type $E = $crate::element_class::types::DiscreteKirchhoffTriangle18;
                $body
            }
            _ => $fallback,
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::ElementType;

    #[test]
    fn constants_match_the_core_tables() {
        fn check<E: StructuralElement>() {
            let s = E::TYPE.interpolation_type().info().structural.unwrap();
            assert_eq!(s.nb_degree_of_freedom, E::NB_DOF_PER_NODE, "{:?}", E::TYPE);
            assert_eq!(s.nb_stress_components, E::NB_STRESS, "{:?}", E::TYPE);
            assert_eq!(s.dnds_columns, E::DNDS_COLUMNS, "{:?}", E::TYPE);
        }
        check::<types::BernoulliBeam2>();
        check::<types::BernoulliBeam3>();
        check::<types::DiscreteKirchhoffTriangle18>();
        let n =
            with_structural_element!(ElementType::BernoulliBeam3, E => E::NB_ELEMENT_DOFS, else 0);
        assert_eq!(n, 12);
        let n = with_structural_element!(ElementType::Triangle3, E => E::NB_ELEMENT_DOFS, else 0);
        assert_eq!(n, 0);
    }

    #[test]
    fn hermite_interpolates_cubics_exactly() {
        // v(x) = c0 + c1 x + c2 x² + c3 x³ on [x0, x0 + L]; nodal values v, v'
        let (x0, len) = (0.75, 2.5);
        let cf = [0.3, -1.25, 0.5, 0.125];
        let v = |x: Real| cf[0] + cf[1] * x + cf[2] * x * x + cf[3] * x * x * x;
        let dv = |x: Real| cf[1] + 2. * cf[2] * x + 3. * cf[3] * x * x;
        let d2v = |x: Real| 2. * cf[2] + 6. * cf[3] * x;
        let coords = [x0, x0 + len];
        let x = Coords::new(&coords, 1, 2).unwrap();
        let nodal = [v(x0), dv(x0), v(x0 + len), dv(x0 + len)];
        for k in 0..=10 {
            let xi = -1. + 0.2 * Real::from(k);
            let h = hermite_shapes(xi, x).unwrap();
            let xp = x0 + (xi + 1.) * len / 2.;
            let val: Real = (0..4).map(|c| h[2 * c] * nodal[c]).sum();
            let rot: Real = (0..4).map(|c| h[1 + 2 * c] * nodal[c]).sum();
            assert!((val - v(xp)).abs() < 1e-14, "value at {xi}");
            assert!((rot - dv(xp)).abs() < 1e-13, "slope at {xi}");
            // computeDNDS gives d²N/(dx dξ); · (2/L) = d²/dx²
            let b = hermite_dnds(xi, x).unwrap();
            let curv: Real = (0..4).map(|c| b[c] * nodal[c]).sum::<Real>() * 2. / len;
            assert!((curv - d2v(xp)).abs() < 1e-13, "curvature at {xi}");
        }
        assert!(hermite_length(Coords::new(&coords, 1, 1).unwrap()).is_err());
        assert!(Coords::new(&coords, 2, 2).is_err());
    }

    #[test]
    fn beam_2_rotation_of_the_upstream_test_mesh() {
        // test_fe_engine_precomputation_bernoulli_2.cc: angles atan(4/3), -atan(4/3), 0
        let nodes = [[0., 0.], [0.6, 0.8], [1.2, 0.]];
        let conn = [(0, 1), (1, 2), (0, 2)];
        let a: Real = (4.0_f64 / 3.).atan();
        for (&(i, j), angle) in conn.iter().zip([a, -a, 0.]) {
            let x = [nodes[i][0], nodes[i][1], nodes[j][0], nodes[j][1]];
            let mut r = [0.0; 9];
            types::BernoulliBeam2::compute_rotation_matrix(
                Coords::new(&x, 2, 2).unwrap(),
                &[],
                &mut r,
            )
            .unwrap();
            let (c, s) = (angle.cos(), angle.sin());
            let expect = [c, -s, 0., s, c, 0., 0., 0., 1.];
            let err: Real = r.iter().zip(expect).map(|(p, q)| (p - q) * (p - q)).sum();
            assert!(err.sqrt() < 1e-15, "{r:?}");
        }
    }

    #[test]
    fn beam_3_rotation_of_the_upstream_test_mesh() {
        // test_fe_engine_precomputation_structural.cc, TestBernoulliB3
        let x = [0., 0., 0., 3., 4., 12.];
        let n = [-36. / 65., -48. / 65., 5. / 13.];
        let mut r = [0.0; 36];
        types::BernoulliBeam3::compute_rotation_matrix(Coords::new(&x, 3, 2).unwrap(), &n, &mut r)
            .unwrap();
        let reference = [
            [3. / 13., 4. / 13., 12. / 13.],
            [-4. / 5., 3. / 5., 0.],
            [-36. / 65., -48. / 65., 5. / 13.],
        ];
        let mut err = 0.0;
        for i in 0..6 {
            for j in 0..6 {
                let e = if (i < 3) == (j < 3) {
                    reference[i % 3][j % 3]
                } else {
                    0.
                };
                err += (r[i + 6 * j] - e) * (r[i + 6 * j] - e);
            }
        }
        assert!(err.sqrt() < 1e-15, "{r:?}");
        assert!(
            types::BernoulliBeam3::compute_rotation_matrix(
                Coords::new(&x, 3, 2).unwrap(),
                &n[..2],
                &mut r
            )
            .is_err()
        );
    }

    #[test]
    fn beam_2_shapes_match_the_reference_polynomials() {
        // test_fe_engine_precomputation_bernoulli_2.cc references (unit half length)
        let coords = [0., 2.];
        let x = Coords::new(&coords, 1, 2).unwrap();
        for xq in [-1. / 3.0_f64.sqrt(), 1. / 3.0_f64.sqrt(), 0.3] {
            let mut n = [0.0; 18];
            types::BernoulliBeam2::compute_shapes(&[xq], x, &mut n).unwrap();
            let row = |r: usize| -> Vec<Real> { (0..6).map(|c| n[r + 3 * c]).collect() };
            let axial = [(1. - xq) / 2., 0., 0., (1. + xq) / 2., 0., 0.];
            let bending = [
                0.,
                0.25 * (xq - 1.).powi(2) * (xq + 2.),
                0.25 * (xq - 1.).powi(2) * (xq + 1.),
                0.,
                0.25 * (xq + 1.).powi(2) * (2. - xq),
                0.25 * (xq + 1.).powi(2) * (xq - 1.),
            ];
            let rotation = [
                0.,
                0.75 * (xq * xq - 1.),
                0.25 * (3. * xq * xq - 2. * xq - 1.),
                0.,
                0.75 * (1. - xq * xq),
                0.25 * (3. * xq * xq + 2. * xq - 1.),
            ];
            for (got, want) in [(row(0), axial), (row(1), bending), (row(2), rotation)] {
                for (g, w) in got.iter().zip(want) {
                    assert!((g - w).abs() < 1e-15, "{got:?} vs {want:?}");
                }
            }
        }
    }

    #[test]
    fn dkt_basis_is_orthonormal_and_in_plane() {
        let x = [0.1, 0.2, 0.3, 1.4, 0.1, -0.2, 0.3, 1.1, 0.9];
        let p = dkt_basis_change(Coords::new(&x, 3, 3).unwrap()).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                let d: Real = (0..3).map(|k| p[i + 3 * k] * p[j + 3 * k]).sum();
                let e = if i == j { 1. } else { 0. };
                assert!((d - e).abs() < 1e-15);
            }
        }
        // local z of the three nodes is constant (in-plane basis)
        let z: Vec<Real> = (0..3)
            .map(|n| (0..3).map(|k| p[2 + 3 * k] * x[k + 3 * n]).sum())
            .collect();
        assert!((z[0] - z[1]).abs() < 1e-14 && (z[0] - z[2]).abs() < 1e-14);
        let mut n = [0.0; 108];
        assert!(matches!(
            types::DiscreteKirchhoffTriangle18::compute_shapes(
                &[0.2, 0.2],
                Coords::new(&x, 3, 3).unwrap(),
                &mut n
            ),
            Err(Error::NotImplemented(_))
        ));
    }
}
