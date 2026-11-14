// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element_classes/
// element_class_{point_1,segment_2,segment_3,triangle_3,triangle_6,
// quadrangle_4,quadrangle_8,tetrahedron_4,tetrahedron_10,pentahedron_6,
// pentahedron_15,hexahedron_8,hexahedron_20}_inline_impl.hh
// (InterpolationElement<_itp_*>::computeShapes / computeDNDS /
// computeD2NDS2), interpolation_element_tmpl.hh.

//! Lagrange / serendipity interpolation kernels, one per upstream
//! `InterpolationType` (ELEM-001…013; cohesive types reuse the
//! interpolation of their facet).
//!
//! Every expression is transcribed **verbatim** from upstream (same
//! constants, same parenthesisation, C++ left-to-right evaluation of `*`,
//! `/`, `+`, `-`; integer literals are exact doubles), so the values are
//! bit-identical to the oracle (`strict` build, no FMA). The `dnds` output is
//! the column-major `natural_dim × nb_nodes` matrix `dnds(i, j) = ∂N_j/∂ξ_i`
//! (index `i + j·natural_dim`); `d2nds2` is the column-major
//! `natural_dim² × nb_nodes` matrix of upstream `computeD2NDS2`.

use akantu_core::element_type::InterpolationType;
use akantu_core::{Error, Real, Result};

/// One interpolation family (upstream `InterpolationElement<itp>`).
pub trait Interpolation: Copy + Default + Send + Sync + 'static {
    /// Upstream interpolation type.
    const ITP: InterpolationType;
    /// Number of shape functions.
    const NB_NODES: usize;
    /// Natural-space dimension.
    const NATURAL_DIM: usize;
    /// `N(ξ)`.
    fn shapes(xi: &[Real], n: &mut [Real]);
    /// `∂N/∂ξ` (column-major `NATURAL_DIM × NB_NODES`).
    fn dnds(xi: &[Real], dnds: &mut [Real]);
    /// `∂²N/∂ξ²` (column-major `NATURAL_DIM² × NB_NODES`, the output must
    /// hold exactly that many values: `Error::DimensionMismatch` otherwise),
    /// only where upstream implements it (`_segment_2`, `_triangle_3`,
    /// `_quadrangle_4`); `AKANTU_TO_IMPLEMENT` elsewhere.
    fn d2nds2(_xi: &[Real], _d2nds2: &mut [Real]) -> Result<()> {
        Err(Error::NotImplemented(
            "computeD2NDS2 for this interpolation (AKANTU_TO_IMPLEMENT upstream)",
        ))
    }
}

/// Size check of a `d2nds2` output (`nd² × nn` values).
fn check_d2nds2_len(d2: &[Real], nd: usize, nn: usize) -> Result<()> {
    if d2.len() != nd * nd * nn {
        return Err(Error::DimensionMismatch {
            context: "computeD2NDS2: output (natural_dim² × nb_nodes)",
            expected: nd * nd * nn,
            got: d2.len(),
        });
    }
    Ok(())
}

macro_rules! itp {
    ($name:ident, $itp:ident) => {
        #[doc = concat!("Upstream `_itp_` interpolation `", stringify!($itp), "`.")]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
        pub struct $name;
    };
}

itp!(ItpPoint1, LagrangePoint1);
itp!(ItpSegment2, LagrangeSegment2);
itp!(ItpSegment3, LagrangeSegment3);
itp!(ItpTriangle3, LagrangeTriangle3);
itp!(ItpTriangle6, LagrangeTriangle6);
itp!(ItpQuadrangle4, LagrangeQuadrangle4);
itp!(ItpQuadrangle8, SerendipQuadrangle8);
itp!(ItpTetrahedron4, LagrangeTetrahedron4);
itp!(ItpTetrahedron10, LagrangeTetrahedron10);
itp!(ItpPentahedron6, LagrangePentahedron6);
itp!(ItpPentahedron15, LagrangePentahedron15);
itp!(ItpHexahedron8, LagrangeHexahedron8);
itp!(ItpHexahedron20, SerendipHexahedron20);

/// Column-major setter for `dnds(i, j)` with `nd` rows.
macro_rules! set {
    ($d:ident, $nd:expr; $(($i:expr, $j:expr) = $v:expr;)*) => {
        $( $d[$i + $j * $nd] = $v; )*
    };
}

impl Interpolation for ItpPoint1 {
    const ITP: InterpolationType = InterpolationType::LagrangePoint1;
    const NB_NODES: usize = 1;
    const NATURAL_DIM: usize = 0;
    fn shapes(_xi: &[Real], n: &mut [Real]) {
        n[0] = 1.;
    }
    fn dnds(_xi: &[Real], _dnds: &mut [Real]) {}
}

impl Interpolation for ItpSegment2 {
    const ITP: InterpolationType = InterpolationType::LagrangeSegment2;
    const NB_NODES: usize = 2;
    const NATURAL_DIM: usize = 1;
    fn shapes(xi: &[Real], n: &mut [Real]) {
        let c = xi[0];
        n[0] = 0.5 * (1. - c);
        n[1] = 0.5 * (1. + c);
    }
    fn dnds(_xi: &[Real], d: &mut [Real]) {
        set!(d, 1; (0, 0) = -0.5; (0, 1) = 0.5;);
    }
    fn d2nds2(_xi: &[Real], d2: &mut [Real]) -> Result<()> {
        check_d2nds2_len(d2, Self::NATURAL_DIM, Self::NB_NODES)?;
        d2.fill(0.0);
        Ok(())
    }
}

impl Interpolation for ItpSegment3 {
    const ITP: InterpolationType = InterpolationType::LagrangeSegment3;
    const NB_NODES: usize = 3;
    const NATURAL_DIM: usize = 1;
    fn shapes(xi: &[Real], n: &mut [Real]) {
        let c = xi[0];
        n[0] = (c - 1.) * c / 2.;
        n[1] = (c + 1.) * c / 2.;
        n[2] = 1. - c * c;
    }
    fn dnds(xi: &[Real], d: &mut [Real]) {
        let c = xi[0];
        set!(d, 1; (0, 0) = c - 0.5; (0, 1) = c + 0.5; (0, 2) = -2. * c;);
    }
}

impl Interpolation for ItpTriangle3 {
    const ITP: InterpolationType = InterpolationType::LagrangeTriangle3;
    const NB_NODES: usize = 3;
    const NATURAL_DIM: usize = 2;
    fn shapes(x: &[Real], n: &mut [Real]) {
        n[0] = 1. - x[0] - x[1];
        n[1] = x[0];
        n[2] = x[1];
    }
    fn dnds(_xi: &[Real], d: &mut [Real]) {
        set!(d, 2;
            (0, 0) = -1.; (0, 1) = 1.; (0, 2) = 0.;
            (1, 0) = -1.; (1, 1) = 0.; (1, 2) = 1.;);
    }
    fn d2nds2(_xi: &[Real], d2: &mut [Real]) -> Result<()> {
        check_d2nds2_len(d2, Self::NATURAL_DIM, Self::NB_NODES)?;
        d2.fill(0.0);
        Ok(())
    }
}

impl Interpolation for ItpTriangle6 {
    const ITP: InterpolationType = InterpolationType::LagrangeTriangle6;
    const NB_NODES: usize = 6;
    const NATURAL_DIM: usize = 2;
    fn shapes(x: &[Real], n: &mut [Real]) {
        let c0 = 1. - x[0] - x[1];
        let c1 = x[0];
        let c2 = x[1];
        n[0] = c0 * (2. * c0 - 1.);
        n[1] = c1 * (2. * c1 - 1.);
        n[2] = c2 * (2. * c2 - 1.);
        n[3] = 4. * c0 * c1;
        n[4] = 4. * c1 * c2;
        n[5] = 4. * c2 * c0;
    }
    fn dnds(x: &[Real], d: &mut [Real]) {
        let c0 = 1. - x[0] - x[1];
        let c1 = x[0];
        let c2 = x[1];
        set!(d, 2;
            (0, 0) = 1. - 4. * c0;
            (0, 1) = 4. * c1 - 1.;
            (0, 2) = 0.;
            (0, 3) = 4. * (c0 - c1);
            (0, 4) = 4. * c2;
            (0, 5) = -4. * c2;
            (1, 0) = 1. - 4. * c0;
            (1, 1) = 0.;
            (1, 2) = 4. * c2 - 1.;
            (1, 3) = -4. * c1;
            (1, 4) = 4. * c1;
            (1, 5) = 4. * (c0 - c2););
    }
}

impl Interpolation for ItpQuadrangle4 {
    const ITP: InterpolationType = InterpolationType::LagrangeQuadrangle4;
    const NB_NODES: usize = 4;
    const NATURAL_DIM: usize = 2;
    fn shapes(c: &[Real], n: &mut [Real]) {
        n[0] = 1. / 4. * (1. - c[0]) * (1. - c[1]);
        n[1] = 1. / 4. * (1. + c[0]) * (1. - c[1]);
        n[2] = 1. / 4. * (1. + c[0]) * (1. + c[1]);
        n[3] = 1. / 4. * (1. - c[0]) * (1. + c[1]);
    }
    fn dnds(c: &[Real], d: &mut [Real]) {
        set!(d, 2;
            (0, 0) = -1. / 4. * (1. - c[1]);
            (0, 1) = 1. / 4. * (1. - c[1]);
            (0, 2) = 1. / 4. * (1. + c[1]);
            (0, 3) = -1. / 4. * (1. + c[1]);
            (1, 0) = -1. / 4. * (1. - c[0]);
            (1, 1) = -1. / 4. * (1. + c[0]);
            (1, 2) = 1. / 4. * (1. + c[0]);
            (1, 3) = 1. / 4. * (1. - c[0]););
    }
    fn d2nds2(_xi: &[Real], d2: &mut [Real]) -> Result<()> {
        check_d2nds2_len(d2, Self::NATURAL_DIM, Self::NB_NODES)?;
        d2.fill(0.0);
        set!(d2, 4;
            (1, 0) = 1. / 4.; (1, 1) = -1. / 4.; (1, 2) = 1. / 4.; (1, 3) = -1. / 4.;
            (2, 0) = 1. / 4.; (2, 1) = -1. / 4.; (2, 2) = 1. / 4.; (2, 3) = -1. / 4.;);
        Ok(())
    }
}

impl Interpolation for ItpQuadrangle8 {
    const ITP: InterpolationType = InterpolationType::SerendipQuadrangle8;
    const NB_NODES: usize = 8;
    const NATURAL_DIM: usize = 2;
    fn shapes(c: &[Real], n: &mut [Real]) {
        let xi = c[0];
        let eta = c[1];
        n[0] = 0.25 * (1. - xi) * (1. - eta) * (-1. - xi - eta);
        n[1] = 0.25 * (1. + xi) * (1. - eta) * (-1. + xi - eta);
        n[2] = 0.25 * (1. + xi) * (1. + eta) * (-1. + xi + eta);
        n[3] = 0.25 * (1. - xi) * (1. + eta) * (-1. - xi + eta);
        n[4] = 0.5 * (1. - xi * xi) * (1. - eta);
        n[5] = 0.5 * (1. + xi) * (1. - eta * eta);
        n[6] = 0.5 * (1. - xi * xi) * (1. + eta);
        n[7] = 0.5 * (1. - xi) * (1. - eta * eta);
    }
    fn dnds(c: &[Real], d: &mut [Real]) {
        let xi = c[0];
        let eta = c[1];
        set!(d, 2;
            (0, 0) = 0.25 * (1. - eta) * (2. * xi + eta);
            (0, 1) = 0.25 * (1. - eta) * (2. * xi - eta);
            (0, 2) = 0.25 * (1. + eta) * (2. * xi + eta);
            (0, 3) = 0.25 * (1. + eta) * (2. * xi - eta);
            (0, 4) = -xi * (1. - eta);
            (0, 5) = 0.5 * (1. - eta * eta);
            (0, 6) = -xi * (1. + eta);
            (0, 7) = -0.5 * (1. - eta * eta);
            (1, 0) = 0.25 * (1. - xi) * (2. * eta + xi);
            (1, 1) = 0.25 * (1. + xi) * (2. * eta - xi);
            (1, 2) = 0.25 * (1. + xi) * (2. * eta + xi);
            (1, 3) = 0.25 * (1. - xi) * (2. * eta - xi);
            (1, 4) = -0.5 * (1. - xi * xi);
            (1, 5) = -eta * (1. + xi);
            (1, 6) = 0.5 * (1. - xi * xi);
            (1, 7) = -eta * (1. - xi););
    }
}

impl Interpolation for ItpTetrahedron4 {
    const ITP: InterpolationType = InterpolationType::LagrangeTetrahedron4;
    const NB_NODES: usize = 4;
    const NATURAL_DIM: usize = 3;
    /// Upstream's (non-standard, UD-041) node mapping is kept:
    /// `N = (1 − ξ − η − ζ, η, ζ, ξ)`.
    fn shapes(x: &[Real], n: &mut [Real]) {
        let c0 = 1. - x[0] - x[1] - x[2];
        let c1 = x[1];
        let c2 = x[2];
        let c3 = x[0];
        n[0] = c0;
        n[1] = c1;
        n[2] = c2;
        n[3] = c3;
    }
    fn dnds(_xi: &[Real], d: &mut [Real]) {
        set!(d, 3;
            (0, 0) = -1.; (1, 0) = -1.; (2, 0) = -1.;
            (0, 1) = 0.; (1, 1) = 1.; (2, 1) = 0.;
            (0, 2) = 0.; (1, 2) = 0.; (2, 2) = 1.;
            (0, 3) = 1.; (1, 3) = 0.; (2, 3) = 0.;);
    }
}

impl Interpolation for ItpTetrahedron10 {
    const ITP: InterpolationType = InterpolationType::LagrangeTetrahedron10;
    const NB_NODES: usize = 10;
    const NATURAL_DIM: usize = 3;
    fn shapes(x: &[Real], n: &mut [Real]) {
        let xi = x[0];
        let eta = x[1];
        let zeta = x[2];
        let sum = xi + eta + zeta;
        let c0 = 1. - sum;
        let c1 = 1. - 2. * sum;
        let c2 = 2. * xi - 1.;
        let c3 = 2. * eta - 1.;
        let c4 = 2. * zeta - 1.;
        n[0] = c0 * c1;
        n[1] = xi * c2;
        n[2] = eta * c3;
        n[3] = zeta * c4;
        n[4] = 4. * xi * c0;
        n[5] = 4. * xi * eta;
        n[6] = 4. * eta * c0;
        n[7] = 4. * zeta * c0;
        n[8] = 4. * xi * zeta;
        n[9] = 4. * eta * zeta;
    }
    fn dnds(x: &[Real], d: &mut [Real]) {
        let xi = x[0];
        let eta = x[1];
        let zeta = x[2];
        let sum = xi + eta + zeta;
        set!(d, 3;
            (0, 0) = 4. * sum - 3.;
            (0, 1) = 4. * xi - 1.;
            (0, 2) = 0.;
            (0, 3) = 0.;
            (0, 4) = 4. * (1. - sum - xi);
            (0, 5) = 4. * eta;
            (0, 6) = -4. * eta;
            (0, 7) = -4. * zeta;
            (0, 8) = 4. * zeta;
            (0, 9) = 0.;
            (1, 0) = 4. * sum - 3.;
            (1, 1) = 0.;
            (1, 2) = 4. * eta - 1.;
            (1, 3) = 0.;
            (1, 4) = -4. * xi;
            (1, 5) = 4. * xi;
            (1, 6) = 4. * (1. - sum - eta);
            (1, 7) = -4. * zeta;
            (1, 8) = 0.;
            (1, 9) = 4. * zeta;
            (2, 0) = 4. * sum - 3.;
            (2, 1) = 0.;
            (2, 2) = 0.;
            (2, 3) = 4. * zeta - 1.;
            (2, 4) = -4. * xi;
            (2, 5) = 0.;
            (2, 6) = -4. * eta;
            (2, 7) = 4. * (1. - sum - zeta);
            (2, 8) = 4. * xi;
            (2, 9) = 4. * eta;);
    }
}

impl Interpolation for ItpPentahedron6 {
    const ITP: InterpolationType = InterpolationType::LagrangePentahedron6;
    const NB_NODES: usize = 6;
    const NATURAL_DIM: usize = 3;
    fn shapes(c: &[Real], n: &mut [Real]) {
        n[0] = 0.5 * c[1] * (1. - c[0]);
        n[1] = 0.5 * c[2] * (1. - c[0]);
        n[2] = 0.5 * (1. - c[1] - c[2]) * (1. - c[0]);
        n[3] = 0.5 * c[1] * (1. + c[0]);
        n[4] = 0.5 * c[2] * (1. + c[0]);
        n[5] = 0.5 * (1. - c[1] - c[2]) * (1. + c[0]);
    }
    fn dnds(c: &[Real], d: &mut [Real]) {
        set!(d, 3;
            (0, 0) = -0.5 * c[1];
            (0, 1) = -0.5 * c[2];
            (0, 2) = -0.5 * (1. - c[1] - c[2]);
            (0, 3) = 0.5 * c[1];
            (0, 4) = 0.5 * c[2];
            (0, 5) = 0.5 * (1. - c[1] - c[2]);
            (1, 0) = 0.5 * (1. - c[0]);
            (1, 1) = 0.0;
            (1, 2) = -0.5 * (1. - c[0]);
            (1, 3) = 0.5 * (1. + c[0]);
            (1, 4) = 0.0;
            (1, 5) = -0.5 * (1. + c[0]);
            (2, 0) = 0.0;
            (2, 1) = 0.5 * (1. - c[0]);
            (2, 2) = -0.5 * (1. - c[0]);
            (2, 3) = 0.0;
            (2, 4) = 0.5 * (1. + c[0]);
            (2, 5) = -0.5 * (1. + c[0]););
    }
}

impl Interpolation for ItpPentahedron15 {
    const ITP: InterpolationType = InterpolationType::LagrangePentahedron15;
    const NB_NODES: usize = 15;
    const NATURAL_DIM: usize = 3;
    fn shapes(c: &[Real], n: &mut [Real]) {
        let x = c[0];
        let y = c[1];
        let z = c[2];
        n[0] = 0.5 * y * (1. - x) * (2. * y - 2. - x);
        n[1] = 0.5 * z * (1. - x) * (2. * z - 2. - x);
        n[2] = 0.5 * (x - 1.) * (1. - y - z) * (x + 2. * y + 2. * z);
        n[3] = 0.5 * y * (1. + x) * (2. * y - 2. + x);
        n[4] = 0.5 * z * (1. + x) * (2. * z - 2. + x);
        n[5] = 0.5 * (-x - 1.) * (1. - y - z) * (-x + 2. * y + 2. * z);
        n[6] = 2.0 * y * z * (1. - x);
        n[7] = 2.0 * z * (1. - y - z) * (1. - x);
        n[8] = 2.0 * y * (1. - x) * (1. - y - z);
        n[9] = y * (1. - x * x);
        n[10] = z * (1. - x * x);
        n[11] = (1. - y - z) * (1. - x * x);
        n[12] = 2.0 * y * z * (1. + x);
        n[13] = 2.0 * z * (1. - y - z) * (1. + x);
        n[14] = 2.0 * y * (1. - y - z) * (1. + x);
    }
    fn dnds(c: &[Real], d: &mut [Real]) {
        let x = c[0];
        let y = c[1];
        let z = c[2];
        set!(d, 3;
            // ddx
            (0, 0) = 0.5 * y * (2. * x - 2. * y + 1.);
            (0, 1) = 0.5 * z * (2. * x - 2. * z + 1.);
            (0, 2) = -0.5 * (2. * x + 2. * y + 2. * z - 1.) * (y + z - 1.);
            (0, 3) = 0.5 * y * (2. * x + 2. * y - 1.);
            (0, 4) = 0.5 * z * (2. * x + 2. * z - 1.);
            (0, 5) = -0.5 * (y + z - 1.) * (2. * x - 2. * y - 2. * z + 1.);
            (0, 6) = -2.0 * y * z;
            (0, 7) = 2.0 * z * (y + z - 1.);
            (0, 8) = 2.0 * y * (y + z - 1.);
            (0, 9) = -2.0 * x * y;
            (0, 10) = -2.0 * x * z;
            (0, 11) = 2.0 * x * (y + z - 1.);
            (0, 12) = 2.0 * y * z;
            (0, 13) = -2.0 * z * (y + z - 1.);
            (0, 14) = -2.0 * y * (y + z - 1.);
            // ddy
            (1, 0) = -0.5 * (x - 1.) * (4. * y - x - 2.);
            (1, 1) = 0.0;
            (1, 2) = -0.5 * (x - 1.) * (4. * y + x + 2. * (2. * z - 1.));
            (1, 3) = 0.5 * (x + 1.) * (4. * y + x - 2.);
            (1, 4) = 0.0;
            (1, 5) = 0.5 * (x + 1.) * (4. * y - x + 2. * (2. * z - 1.));
            (1, 6) = -2.0 * (x - 1.) * z;
            (1, 7) = 2.0 * z * (x - 1.);
            (1, 8) = 2.0 * (2. * y + z - 1.) * (x - 1.);
            (1, 9) = -(x * x - 1.);
            (1, 10) = 0.0;
            (1, 11) = x * x - 1.;
            (1, 12) = 2.0 * z * (x + 1.);
            (1, 13) = -2.0 * z * (x + 1.);
            (1, 14) = -2.0 * (2. * y + z - 1.) * (x + 1.);
            // ddz
            (2, 0) = 0.0;
            (2, 1) = -0.5 * (x - 1.) * (4. * z - x - 2.);
            (2, 2) = -0.5 * (x - 1.) * (4. * z + x + 2. * (2. * y - 1.));
            (2, 3) = 0.0;
            (2, 4) = 0.5 * (x + 1.) * (4. * z + x - 2.);
            (2, 5) = 0.5 * (x + 1.) * (4. * z - x + 2. * (2. * y - 1.));
            (2, 6) = -2.0 * (x - 1.) * y;
            (2, 7) = 2.0 * (x - 1.) * (2. * z + y - 1.);
            (2, 8) = 2.0 * y * (x - 1.);
            (2, 9) = 0.0;
            (2, 10) = -(x * x - 1.);
            (2, 11) = x * x - 1.;
            (2, 12) = 2.0 * (x + 1.) * y;
            (2, 13) = -2.0 * (x + 1.) * (2. * z + y - 1.);
            (2, 14) = -2.0 * (x + 1.) * y;);
    }
}

impl Interpolation for ItpHexahedron8 {
    const ITP: InterpolationType = InterpolationType::LagrangeHexahedron8;
    const NB_NODES: usize = 8;
    const NATURAL_DIM: usize = 3;
    fn shapes(c: &[Real], n: &mut [Real]) {
        n[0] = 0.125 * (1. - c[0]) * (1. - c[1]) * (1. - c[2]);
        n[1] = 0.125 * (1. + c[0]) * (1. - c[1]) * (1. - c[2]);
        n[2] = 0.125 * (1. + c[0]) * (1. + c[1]) * (1. - c[2]);
        n[3] = 0.125 * (1. - c[0]) * (1. + c[1]) * (1. - c[2]);
        n[4] = 0.125 * (1. - c[0]) * (1. - c[1]) * (1. + c[2]);
        n[5] = 0.125 * (1. + c[0]) * (1. - c[1]) * (1. + c[2]);
        n[6] = 0.125 * (1. + c[0]) * (1. + c[1]) * (1. + c[2]);
        n[7] = 0.125 * (1. - c[0]) * (1. + c[1]) * (1. + c[2]);
    }
    fn dnds(c: &[Real], d: &mut [Real]) {
        set!(d, 3;
            (0, 0) = -0.125 * (1. - c[1]) * (1. - c[2]);
            (0, 1) = 0.125 * (1. - c[1]) * (1. - c[2]);
            (0, 2) = 0.125 * (1. + c[1]) * (1. - c[2]);
            (0, 3) = -0.125 * (1. + c[1]) * (1. - c[2]);
            (0, 4) = -0.125 * (1. - c[1]) * (1. + c[2]);
            (0, 5) = 0.125 * (1. - c[1]) * (1. + c[2]);
            (0, 6) = 0.125 * (1. + c[1]) * (1. + c[2]);
            (0, 7) = -0.125 * (1. + c[1]) * (1. + c[2]);
            (1, 0) = -0.125 * (1. - c[0]) * (1. - c[2]);
            (1, 1) = -0.125 * (1. + c[0]) * (1. - c[2]);
            (1, 2) = 0.125 * (1. + c[0]) * (1. - c[2]);
            (1, 3) = 0.125 * (1. - c[0]) * (1. - c[2]);
            (1, 4) = -0.125 * (1. - c[0]) * (1. + c[2]);
            (1, 5) = -0.125 * (1. + c[0]) * (1. + c[2]);
            (1, 6) = 0.125 * (1. + c[0]) * (1. + c[2]);
            (1, 7) = 0.125 * (1. - c[0]) * (1. + c[2]);
            (2, 0) = -0.125 * (1. - c[0]) * (1. - c[1]);
            (2, 1) = -0.125 * (1. + c[0]) * (1. - c[1]);
            (2, 2) = -0.125 * (1. + c[0]) * (1. + c[1]);
            (2, 3) = -0.125 * (1. - c[0]) * (1. + c[1]);
            (2, 4) = 0.125 * (1. - c[0]) * (1. - c[1]);
            (2, 5) = 0.125 * (1. + c[0]) * (1. - c[1]);
            (2, 6) = 0.125 * (1. + c[0]) * (1. + c[1]);
            (2, 7) = 0.125 * (1. - c[0]) * (1. + c[1]););
    }
}

impl Interpolation for ItpHexahedron20 {
    const ITP: InterpolationType = InterpolationType::SerendipHexahedron20;
    const NB_NODES: usize = 20;
    const NATURAL_DIM: usize = 3;
    fn shapes(c: &[Real], n: &mut [Real]) {
        n[0] = 0.125 * (1. - c[0]) * (1. - c[1]) * (1. - c[2]) * (-2. - c[0] - c[1] - c[2]);
        n[1] = 0.125 * (1. + c[0]) * (1. - c[1]) * (1. - c[2]) * (-2. + c[0] - c[1] - c[2]);
        n[2] = 0.125 * (1. + c[0]) * (1. + c[1]) * (1. - c[2]) * (-2. + c[0] + c[1] - c[2]);
        n[3] = 0.125 * (1. - c[0]) * (1. + c[1]) * (1. - c[2]) * (-2. - c[0] + c[1] - c[2]);
        n[4] = 0.125 * (1. - c[0]) * (1. - c[1]) * (1. + c[2]) * (-2. - c[0] - c[1] + c[2]);
        n[5] = 0.125 * (1. + c[0]) * (1. - c[1]) * (1. + c[2]) * (-2. + c[0] - c[1] + c[2]);
        n[6] = 0.125 * (1. + c[0]) * (1. + c[1]) * (1. + c[2]) * (-2. + c[0] + c[1] + c[2]);
        n[7] = 0.125 * (1. - c[0]) * (1. + c[1]) * (1. + c[2]) * (-2. - c[0] + c[1] + c[2]);
        n[8] = 0.25 * (1. - c[0] * c[0]) * (1. - c[1]) * (1. - c[2]);
        n[9] = 0.25 * (1. - c[1] * c[1]) * (1. + c[0]) * (1. - c[2]);
        n[10] = 0.25 * (1. - c[0] * c[0]) * (1. + c[1]) * (1. - c[2]);
        n[11] = 0.25 * (1. - c[1] * c[1]) * (1. - c[0]) * (1. - c[2]);
        n[12] = 0.25 * (1. - c[2] * c[2]) * (1. - c[0]) * (1. - c[1]);
        n[13] = 0.25 * (1. - c[2] * c[2]) * (1. + c[0]) * (1. - c[1]);
        n[14] = 0.25 * (1. - c[2] * c[2]) * (1. + c[0]) * (1. + c[1]);
        n[15] = 0.25 * (1. - c[2] * c[2]) * (1. - c[0]) * (1. + c[1]);
        n[16] = 0.25 * (1. - c[0] * c[0]) * (1. - c[1]) * (1. + c[2]);
        n[17] = 0.25 * (1. - c[1] * c[1]) * (1. + c[0]) * (1. + c[2]);
        n[18] = 0.25 * (1. - c[0] * c[0]) * (1. + c[1]) * (1. + c[2]);
        n[19] = 0.25 * (1. - c[1] * c[1]) * (1. - c[0]) * (1. + c[2]);
    }
    fn dnds(c: &[Real], d: &mut [Real]) {
        set!(d, 3;
            // ddx
            (0, 0) = 0.25 * (c[0] + 0.5 * (c[1] + c[2] + 1.)) * (c[1] - 1.) * (c[2] - 1.);
            (0, 1) = 0.25 * (c[0] - 0.5 * (c[1] + c[2] + 1.)) * (c[1] - 1.) * (c[2] - 1.);
            (0, 2) = -0.25 * (c[0] + 0.5 * (c[1] - c[2] - 1.)) * (c[1] + 1.) * (c[2] - 1.);
            (0, 3) = -0.25 * (c[0] - 0.5 * (c[1] - c[2] - 1.)) * (c[1] + 1.) * (c[2] - 1.);
            (0, 4) = -0.25 * (c[0] + 0.5 * (c[1] - c[2] + 1.)) * (c[1] - 1.) * (c[2] + 1.);
            (0, 5) = -0.25 * (c[0] - 0.5 * (c[1] - c[2] + 1.)) * (c[1] - 1.) * (c[2] + 1.);
            (0, 6) = 0.25 * (c[0] + 0.5 * (c[1] + c[2] - 1.)) * (c[1] + 1.) * (c[2] + 1.);
            (0, 7) = 0.25 * (c[0] - 0.5 * (c[1] + c[2] - 1.)) * (c[1] + 1.) * (c[2] + 1.);
            (0, 8) = -0.5 * c[0] * (c[1] - 1.) * (c[2] - 1.);
            (0, 9) = 0.25 * (c[1] * c[1] - 1.) * (c[2] - 1.);
            (0, 10) = 0.5 * c[0] * (c[1] + 1.) * (c[2] - 1.);
            (0, 11) = -0.25 * (c[1] * c[1] - 1.) * (c[2] - 1.);
            (0, 12) = -0.25 * (c[2] * c[2] - 1.) * (c[1] - 1.);
            (0, 13) = 0.25 * (c[1] - 1.) * (c[2] * c[2] - 1.);
            (0, 14) = -0.25 * (c[1] + 1.) * (c[2] * c[2] - 1.);
            (0, 15) = 0.25 * (c[1] + 1.) * (c[2] * c[2] - 1.);
            (0, 16) = 0.5 * c[0] * (c[1] - 1.) * (c[2] + 1.);
            (0, 17) = -0.25 * (c[2] + 1.) * (c[1] * c[1] - 1.);
            (0, 18) = -0.5 * c[0] * (c[1] + 1.) * (c[2] + 1.);
            (0, 19) = 0.25 * (c[2] + 1.) * (c[1] * c[1] - 1.);
            // ddy
            (1, 0) = 0.25 * (c[1] + 0.5 * (c[0] + c[2] + 1.)) * (c[0] - 1.) * (c[2] - 1.);
            (1, 1) = -0.25 * (c[1] - 0.5 * (c[0] - c[2] - 1.)) * (c[0] + 1.) * (c[2] - 1.);
            (1, 2) = -0.25 * (c[1] + 0.5 * (c[0] - c[2] - 1.)) * (c[0] + 1.) * (c[2] - 1.);
            (1, 3) = 0.25 * (c[1] - 0.5 * (c[0] + c[2] + 1.)) * (c[0] - 1.) * (c[2] - 1.);
            (1, 4) = -0.25 * (c[1] + 0.5 * (c[0] - c[2] + 1.)) * (c[0] - 1.) * (c[2] + 1.);
            (1, 5) = 0.25 * (c[1] - 0.5 * (c[0] + c[2] - 1.)) * (c[0] + 1.) * (c[2] + 1.);
            (1, 6) = 0.25 * (c[1] + 0.5 * (c[0] + c[2] - 1.)) * (c[0] + 1.) * (c[2] + 1.);
            (1, 7) = -0.25 * (c[1] - 0.5 * (c[0] - c[2] + 1.)) * (c[0] - 1.) * (c[2] + 1.);
            (1, 8) = -0.25 * (c[0] * c[0] - 1.) * (c[2] - 1.);
            (1, 9) = 0.5 * c[1] * (c[0] + 1.) * (c[2] - 1.);
            (1, 10) = 0.25 * (c[0] * c[0] - 1.) * (c[2] - 1.);
            (1, 11) = -0.5 * c[1] * (c[0] - 1.) * (c[2] - 1.);
            (1, 12) = -0.25 * (c[2] * c[2] - 1.) * (c[0] - 1.);
            (1, 13) = 0.25 * (c[0] + 1.) * (c[2] * c[2] - 1.);
            (1, 14) = -0.25 * (c[0] + 1.) * (c[2] * c[2] - 1.);
            (1, 15) = 0.25 * (c[0] - 1.) * (c[2] * c[2] - 1.);
            (1, 16) = 0.25 * (c[2] + 1.) * (c[0] * c[0] - 1.);
            (1, 17) = -0.5 * c[1] * (c[0] + 1.) * (c[2] + 1.);
            (1, 18) = -0.25 * (c[2] + 1.) * (c[0] * c[0] - 1.);
            (1, 19) = 0.5 * c[1] * (c[0] - 1.) * (c[2] + 1.);
            // ddz
            (2, 0) = 0.25 * (c[2] + 0.5 * (c[0] + c[1] + 1.)) * (c[0] - 1.) * (c[1] - 1.);
            (2, 1) = -0.25 * (c[2] - 0.5 * (c[0] - c[1] - 1.)) * (c[0] + 1.) * (c[1] - 1.);
            (2, 2) = 0.25 * (c[2] - 0.5 * (c[0] + c[1] - 1.)) * (c[0] + 1.) * (c[1] + 1.);
            (2, 3) = -0.25 * (c[2] + 0.5 * (c[0] - c[1] + 1.)) * (c[0] - 1.) * (c[1] + 1.);
            (2, 4) = 0.25 * (c[2] - 0.5 * (c[0] + c[1] + 1.)) * (c[0] - 1.) * (c[1] - 1.);
            (2, 5) = -0.25 * (c[2] + 0.5 * (c[0] - c[1] - 1.)) * (c[0] + 1.) * (c[1] - 1.);
            (2, 6) = 0.25 * (c[2] + 0.5 * (c[0] + c[1] - 1.)) * (c[0] + 1.) * (c[1] + 1.);
            (2, 7) = -0.25 * (c[2] - 0.5 * (c[0] - c[1] + 1.)) * (c[0] - 1.) * (c[1] + 1.);
            (2, 8) = -0.25 * (c[0] * c[0] - 1.) * (c[1] - 1.);
            (2, 9) = 0.25 * (c[1] * c[1] - 1.) * (c[0] + 1.);
            (2, 10) = 0.25 * (c[0] * c[0] - 1.) * (c[1] + 1.);
            (2, 11) = -0.25 * (c[1] * c[1] - 1.) * (c[0] - 1.);
            (2, 12) = -0.5 * c[2] * (c[1] - 1.) * (c[0] - 1.);
            (2, 13) = 0.5 * c[2] * (c[0] + 1.) * (c[1] - 1.);
            (2, 14) = -0.5 * c[2] * (c[0] + 1.) * (c[1] + 1.);
            (2, 15) = 0.5 * c[2] * (c[0] - 1.) * (c[1] + 1.);
            (2, 16) = 0.25 * (c[1] - 1.) * (c[0] * c[0] - 1.);
            (2, 17) = -0.25 * (c[0] + 1.) * (c[1] * c[1] - 1.);
            (2, 18) = -0.25 * (c[1] + 1.) * (c[0] * c[0] - 1.);
            (2, 19) = 0.25 * (c[0] - 1.) * (c[1] * c[1] - 1.););
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partition_of_unity<I: Interpolation>(xi: &[Real]) {
        assert_eq!(I::NB_NODES, I::ITP.info().nb_nodes_per_element);
        assert_eq!(I::NATURAL_DIM, I::ITP.info().natural_space_dimension);
        let mut n = vec![0.0; I::NB_NODES];
        I::shapes(xi, &mut n);
        let s: Real = n.iter().sum();
        assert!((s - 1.0).abs() < 1e-14, "{:?}: Σ N = {s}", I::ITP);
        let mut d = vec![0.0; I::NB_NODES * I::NATURAL_DIM];
        I::dnds(xi, &mut d);
        for i in 0..I::NATURAL_DIM {
            let s: Real = (0..I::NB_NODES).map(|j| d[i + j * I::NATURAL_DIM]).sum();
            assert!(s.abs() < 1e-14, "{:?}: Σ dN/ds_{i} = {s}", I::ITP);
        }
    }

    #[test]
    fn partition_of_unity_all() {
        partition_of_unity::<ItpPoint1>(&[]);
        partition_of_unity::<ItpSegment2>(&[0.3]);
        partition_of_unity::<ItpSegment3>(&[0.3]);
        partition_of_unity::<ItpTriangle3>(&[0.2, 0.3]);
        partition_of_unity::<ItpTriangle6>(&[0.2, 0.3]);
        partition_of_unity::<ItpQuadrangle4>(&[0.2, -0.3]);
        partition_of_unity::<ItpQuadrangle8>(&[0.2, -0.3]);
        partition_of_unity::<ItpTetrahedron4>(&[0.2, 0.3, 0.1]);
        partition_of_unity::<ItpTetrahedron10>(&[0.2, 0.3, 0.1]);
        partition_of_unity::<ItpPentahedron6>(&[-0.4, 0.3, 0.1]);
        partition_of_unity::<ItpPentahedron15>(&[-0.4, 0.3, 0.1]);
        partition_of_unity::<ItpHexahedron8>(&[0.2, -0.3, 0.7]);
        partition_of_unity::<ItpHexahedron20>(&[0.2, -0.3, 0.7]);
    }

    /// Analytical second derivatives of the three interpolations upstream
    /// implements (linear segment/triangle: zero; bilinear quadrangle: only
    /// the mixed derivative `∂²N/∂ξ∂η = ±1/4`, rows 1 and 2 of the
    /// `nd² × nn` matrix), every other interpolation `NotImplemented`.
    #[test]
    fn d2nds2_where_upstream_has_it() {
        fn d2<I: Interpolation>(xi: &[Real]) -> Result<Vec<Real>> {
            let nd = I::NATURAL_DIM;
            let mut m = vec![1.0; nd * nd * I::NB_NODES];
            I::d2nds2(xi, &mut m).map(|()| m)
        }
        assert_eq!(d2::<ItpSegment2>(&[0.3]).unwrap(), vec![0.0; 2]);
        assert_eq!(d2::<ItpTriangle3>(&[0.1, 0.2]).unwrap(), vec![0.0; 12]);
        let q = 0.25;
        for xi in [[0.1, 0.2], [-1.0, 1.0], [0.0, 0.0]] {
            assert_eq!(
                d2::<ItpQuadrangle4>(&xi).unwrap(),
                vec![
                    0.0, q, q, 0.0, // node 0
                    0.0, -q, -q, 0.0, // node 1
                    0.0, q, q, 0.0, // node 2
                    0.0, -q, -q, 0.0, // node 3
                ]
            );
        }
        let not_implemented = |r: Result<Vec<Real>>| matches!(r, Err(Error::NotImplemented(_)));
        assert!(not_implemented(d2::<ItpPoint1>(&[])));
        assert!(not_implemented(d2::<ItpSegment3>(&[0.1])));
        assert!(not_implemented(d2::<ItpTriangle6>(&[0.1, 0.2])));
        assert!(not_implemented(d2::<ItpQuadrangle8>(&[0.1, 0.2])));
        assert!(not_implemented(d2::<ItpTetrahedron4>(&[0.1, 0.2, 0.3])));
        assert!(not_implemented(d2::<ItpTetrahedron10>(&[0.1, 0.2, 0.3])));
        assert!(not_implemented(d2::<ItpPentahedron6>(&[0.1, 0.2, 0.3])));
        assert!(not_implemented(d2::<ItpPentahedron15>(&[0.1, 0.2, 0.3])));
        assert!(not_implemented(d2::<ItpHexahedron8>(&[0.1, 0.2, 0.3])));
        assert!(not_implemented(d2::<ItpHexahedron20>(&[0.1, 0.2, 0.3])));
    }
}
