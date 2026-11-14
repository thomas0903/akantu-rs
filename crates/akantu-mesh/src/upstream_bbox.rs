// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_bbox.hh (BBox with its
// `empty_` flag: operator+=, setLowerBounds/setUpperBounds, intersects,
// intersection, allGather packing).

//! Upstream `BBox` with its explicit `empty_` flag, as the grid
//! synchronizer and the distributed `makePeriodic` use it (CORE-028
//! subset). [`crate::mesh::BBox`] derives emptiness from `lower > upper`;
//! here a box whose bounds were *set* is non-empty whatever their values
//! (upstream `setLowerBounds`/`setUpperBounds`), which is what the
//! all-gathered flag transmits.

use akantu_core::{Error, Real, Result};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FlaggedBox {
    pub(crate) lower: Vec<Real>,
    pub(crate) upper: Vec<Real>,
    pub(crate) empty: bool,
}

impl FlaggedBox {
    /// Upstream `BBox(dim)`: `lower = +max`, `upper = −max`, empty.
    pub(crate) fn new(dim: usize) -> Self {
        Self {
            lower: vec![Real::MAX; dim],
            upper: vec![-Real::MAX; dim],
            empty: true,
        }
    }

    /// Upstream `operator+=` (`std::min` / `std::max`).
    pub(crate) fn add_point(&mut self, x: &[Real]) {
        self.empty = false;
        for ((l, u), &v) in self.lower.iter_mut().zip(&mut self.upper).zip(x) {
            if v < *l {
                *l = v;
            }
            if *u < v {
                *u = v;
            }
        }
    }

    /// Upstream `contains` (closed).
    pub(crate) fn contains(&self, x: &[Real]) -> bool {
        x.iter()
            .zip(self.lower.iter().zip(&self.upper))
            .all(|(v, (l, u))| v >= l && v <= u)
    }

    /// Upstream `intersects` (`Math::intersects` per direction).
    pub(crate) fn intersects(&self, other: &FlaggedBox) -> bool {
        if self.empty || other.empty {
            return false;
        }
        (0..self.lower.len())
            .all(|s| !(self.upper[s] < other.lower[s] || self.lower[s] > other.upper[s]))
    }

    /// Upstream `intersection(other)`: `other` clamped into `self`
    /// (`std::clamp`); `None` when empty.
    pub(crate) fn intersection(&self, other: &FlaggedBox) -> Option<FlaggedBox> {
        if !self.intersects(other) {
            return None;
        }
        // std::clamp(v, lo, hi) = v < lo ? lo : (hi < v ? hi : v)
        let clamp = |v: Real, lo: Real, hi: Real| {
            if v < lo {
                lo
            } else if hi < v {
                hi
            } else {
                v
            }
        };
        let d = self.lower.len();
        let mut out = FlaggedBox {
            lower: vec![0.0; d],
            upper: vec![0.0; d],
            empty: false,
        };
        for s in 0..d {
            out.lower[s] = clamp(other.lower[s], self.lower[s], self.upper[s]);
            out.upper[s] = -clamp(-other.upper[s], -self.upper[s], -self.lower[s]);
        }
        Some(out)
    }

    /// Upstream `allGather` packing: `dim · (2 + 1)` values — lower, upper,
    /// then the empty flag (1/0) followed by zeros.
    pub(crate) fn pack(&self, out: &mut Vec<Real>) {
        let d = self.lower.len();
        out.extend_from_slice(&self.lower);
        out.extend_from_slice(&self.upper);
        out.push(if self.empty { 1.0 } else { 0.0 });
        out.extend(std::iter::repeat_n(0.0, d.saturating_sub(1)));
    }

    /// Inverse of [`FlaggedBox::pack`].
    pub(crate) fn unpack(values: &[Real], dim: usize) -> Result<Self> {
        if values.len() != 3 * dim || dim == 0 {
            return Err(Error::Communication(format!(
                "bounding box: {} values for dimension {dim}",
                values.len()
            )));
        }
        Ok(Self {
            lower: values[..dim].to_vec(),
            upper: values[dim..2 * dim].to_vec(),
            empty: values[2 * dim] == 1.0,
        })
    }
}
