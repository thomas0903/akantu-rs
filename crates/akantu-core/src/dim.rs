// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_common.hh (dim_1_t/dim_2_t/
// dim_3_t, AllSpatialDimensions), aka_element_classes_info_inline_impl.hh
// (tuple_dispatch over AllSpatialDimensions).

//! Spatial-dimension handling (decision record D-013).
//!
//! The spatial dimension is a **runtime** value (`usize` in `1..=3`) at the
//! level of meshes, models and Python; numerical kernels that benefit from
//! fixed sizes are **monomorphised** on `const DIM: usize` and selected once
//! per batch with [`with_dim!`](crate::with_dim) (upstream:
//! `tuple_dispatch<AllSpatialDimensions>`).

use crate::error::{Error, Result};

/// Validate a runtime spatial dimension.
pub fn check_dim(dim: usize) -> Result<usize> {
    if (1..=3).contains(&dim) {
        Ok(dim)
    } else {
        Err(Error::invalid(format!(
            "spatial dimension must be 1, 2 or 3 (got {dim})"
        )))
    }
}

/// Size of a symmetric tensor in Voigt notation for `dim`
/// (upstream `VoigtHelper<dim>::size`: 1, 3, 6).
#[must_use]
pub const fn voigt_size(dim: usize) -> usize {
    dim * (dim + 1) / 2
}

/// Dispatch a runtime dimension to a const generic.
///
/// ```
/// use akantu_core::with_dim;
/// fn kernel<const D: usize>() -> usize { D * D }
/// let n = with_dim!(2, D => kernel::<D>()).unwrap();
/// assert_eq!(n, 4);
/// assert!(with_dim!(4, D => kernel::<D>()).is_err());
/// ```
#[macro_export]
macro_rules! with_dim {
    ($dim:expr, $d:ident => $body:expr) => {{
        match $dim {
            1 => {
                const $d: usize = 1;
                Ok($body)
            }
            2 => {
                const $d: usize = 2;
                Ok($body)
            }
            3 => {
                const $d: usize = 3;
                Ok($body)
            }
            other => Err($crate::Error::invalid(format!(
                "spatial dimension must be 1, 2 or 3 (got {other})"
            ))),
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq<const D: usize>() -> usize {
        D * D
    }

    #[test]
    fn dispatch() {
        let r: Result<usize> = with_dim!(3, D => sq::<D>());
        assert_eq!(r.unwrap(), 9);
        // every dimension reaches its own constant
        let r: Result<usize> = with_dim!(2, D => sq::<D>());
        assert_eq!(r.unwrap(), 4);
        let r: Result<usize> = with_dim!(1, D => sq::<D>());
        assert_eq!(r.unwrap(), 1);
        let r: Result<usize> = with_dim!(0, D => sq::<D>());
        assert!(r.is_err());
        assert!(check_dim(2).is_ok() && check_dim(4).is_err());
        assert_eq!([voigt_size(1), voigt_size(2), voigt_size(3)], [1, 3, 6]);
    }
}
