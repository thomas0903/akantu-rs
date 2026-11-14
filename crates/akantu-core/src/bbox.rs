// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_bbox.hh (BBox),
// src/common/aka_math_tmpl.hh (Math::intersects).

//! Axis-aligned bounding box (upstream `BBox`, CORE-028).
//!
//! Moved here from `akantu-mesh` because the mesh, the grid
//! synchroniser, periodicity and contact detection all use it; the mesh
//! crate re-exports it as `akantu_mesh::BBox`.
//!
//! **Emptiness.** Upstream stores an explicit `empty_` flag. Here emptiness
//! is derived: a box is empty iff `lower > upper` in some direction. The two
//! agree on every box built by [`BBox::empty`] / [`BBox::reset`] followed by
//! [`BBox::add_point`] with non-NaN coordinates, and on every result of
//! [`BBox::intersection`]. They differ only on degenerate boxes that upstream
//! marks non-empty although they are inverted — bounds set explicitly with
//! `lower > upper` (`setLowerBounds`/`setUpperBounds` after an empty node
//! list in `ContactDetector::constructBoundingBox`, or of an empty grid in
//! `GridSynchronizer`), `allSum` of boxes that are all empty, or a point
//! made only of NaNs. All of these keep the empty-box bounds `±DBL_MAX` in
//! the inverted directions (a finite margin such as `max_bb` or a grid
//! spacing rounds away against `DBL_MAX`). An inverted interval `[a, b]`
//! (`a > b`) passes `Math::intersects` against `[c, d]` only if
//! `c ≤ b < a ≤ d`, and contains no point; with `a = DBL_MAX`,
//! `b = −DBL_MAX` that never happens for a finite box, so
//! [`BBox::intersects`], [`BBox::intersection`] and [`BBox::contains`] give
//! upstream's answers on every box upstream can produce; only
//! [`BBox::is_empty`] itself (upstream `empty()`/`operator bool`) and the
//! printed text differ. (A box set explicitly with finite inverted bounds is
//! not produced anywhere upstream; there the two can differ.) The comm-free
//! building blocks of upstream's
//! collective operations are [`BBox::gather_record`] /
//! [`BBox::from_gather_records`] (`allGather`) and
//! [`BBox::intersections_with`] (`intersection(other, communicator)`); the
//! `allSum` reduction is a `Min` reduction of `lower` and a `Max` reduction
//! of `upper` (upstream: `Min` of `[lower, -upper]`, the same values), done
//! by the callers through their collective phases (`akantu-mesh`).

use std::collections::BTreeMap;
use std::fmt;

use crate::error::{Error, Result};
use crate::types::Real;

/// Axis-aligned bounding box (upstream `BBox`).
#[derive(Clone, Debug, PartialEq)]
pub struct BBox {
    /// Lower corner (`dim` values).
    pub lower: Vec<Real>,
    /// Upper corner (`dim` values).
    pub upper: Vec<Real>,
}

/// `std::clamp(v, lo, hi)` (libstdc++: `v < lo ? lo : (hi < v ? hi : v)`).
#[inline]
fn std_clamp(v: Real, lo: Real, hi: Real) -> Real {
    if v < lo {
        lo
    } else if hi < v {
        hi
    } else {
        v
    }
}

/// Upstream `Math::intersects(x_min, x_max, y_min, y_max)`: the closed
/// intervals `[x_min, x_max]` and `[y_min, y_max]` overlap.
#[inline]
#[must_use]
pub fn intervals_intersect(x_min: Real, x_max: Real, y_min: Real, y_max: Real) -> bool {
    !((x_max < y_min) || (x_min > y_max))
}

impl BBox {
    /// Empty box of dimension `dim`: lower = +`Real::MAX`, upper =
    /// −`Real::MAX`, exactly as upstream `BBox(dim)` / `BBox::reset()`
    /// (`aka_bbox.hh:45-49,125-126`: `numeric_limits<Real>::max()` and
    /// `lowest()`), so an empty mesh reports the same bounds.
    #[must_use]
    pub fn empty(dim: usize) -> Self {
        Self {
            lower: vec![Real::MAX; dim],
            upper: vec![Real::MIN; dim],
        }
    }

    /// Back to the empty box of the same dimension (upstream `reset`).
    pub fn reset(&mut self) {
        self.lower.fill(Real::MAX);
        self.upper.fill(Real::MIN);
    }

    /// Grow to contain `x` (upstream `operator+=`; `std::min`/`std::max`
    /// keep the current bound when the coordinate is NaN, as `f64::min`/
    /// `f64::max` do). Extra coordinates beyond `dim` are ignored (upstream
    /// asserts equal dimensions).
    pub fn add_point(&mut self, x: &[Real]) {
        for ((l, u), &v) in self.lower.iter_mut().zip(&mut self.upper).zip(x) {
            // std::min(l, v) = (v < l) ? v : l; std::max(u, v) = (u < v) ? v : u
            if v < *l {
                *l = v;
            }
            if *u < v {
                *u = v;
            }
        }
    }

    /// Spatial dimension.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.lower.len()
    }

    /// `true` if no point was added (upstream `empty()`; derived, see the
    /// module documentation). A zero-dimensional box is empty, as
    /// upstream's default `BBox()`, so it intersects
    /// nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lower.is_empty() || self.lower.iter().zip(&self.upper).any(|(l, u)| l > u)
    }

    /// Extent per direction (upstream `size()`).
    #[must_use]
    pub fn size(&self) -> Vec<Real> {
        self.lower
            .iter()
            .zip(&self.upper)
            .map(|(l, u)| u - l)
            .collect()
    }

    /// Extent in one direction (upstream `size(direction)`), `None` if the
    /// direction is out of range.
    #[must_use]
    pub fn size_in(&self, direction: usize) -> Option<Real> {
        Some(self.upper.get(direction)? - self.lower.get(direction)?)
    }

    /// Center (upstream `getCenter`: `(upper + lower) / 2`).
    #[must_use]
    pub fn center(&self) -> Vec<Real> {
        self.lower
            .iter()
            .zip(&self.upper)
            .map(|(l, u)| (u + l) / 2.0)
            .collect()
    }

    /// Closed-box membership (upstream `contains`); a point of another
    /// dimension is not contained.
    #[must_use]
    pub fn contains(&self, x: &[Real]) -> bool {
        x.len() == self.dim()
            && x.iter()
                .zip(self.lower.iter().zip(&self.upper))
                .all(|(v, (l, u))| v >= l && v <= u)
    }

    /// Upstream `intersects(other, direction)`: the closed intervals overlap
    /// in `direction` (`Math::intersects`, emptiness not checked, as
    /// upstream). `false` for an out-of-range direction.
    #[must_use]
    pub fn intersects_in(&self, other: &BBox, direction: usize) -> bool {
        match (
            self.lower.get(direction),
            self.upper.get(direction),
            other.lower.get(direction),
            other.upper.get(direction),
        ) {
            (Some(&a0), Some(&a1), Some(&b0), Some(&b1)) => intervals_intersect(a0, a1, b0, b1),
            _ => false,
        }
    }

    /// Upstream `intersects(other)`: both non-empty and the closed
    /// intervals overlap in every direction (`Math::intersects`). Boxes of
    /// different dimensions do not intersect.
    #[must_use]
    pub fn intersects(&self, other: &BBox) -> bool {
        if self.is_empty() || other.is_empty() || self.dim() != other.dim() {
            return false;
        }
        (0..self.dim()).all(|s| self.intersects_in(other, s))
    }

    /// Upstream `intersection(other)`: an empty box if they do not
    /// intersect, otherwise `other`'s bounds clamped into `self`
    /// (`std::clamp`).
    #[must_use]
    pub fn intersection(&self, other: &BBox) -> BBox {
        let mut out = BBox::empty(self.dim());
        if !self.intersects(other) {
            return out;
        }
        for s in 0..self.dim() {
            out.lower[s] = std_clamp(other.lower[s], self.lower[s], self.upper[s]);
            out.upper[s] = -std_clamp(-other.upper[s], -self.upper[s], -self.lower[s]);
        }
        out
    }

    /// The `3 · dim` values this box contributes to upstream's `allGather`
    /// (`aka_bbox.hh:180-193`): a column-major `dim × 3` block with
    /// `lower`, `upper` and `[empty ? 1 : 0, 0, …]`.
    #[must_use]
    pub fn gather_record(&self) -> Vec<Real> {
        let d = self.dim();
        let mut r = Vec::with_capacity(3 * d);
        r.extend_from_slice(&self.lower);
        r.extend_from_slice(&self.upper);
        if d > 0 {
            r.push(if self.is_empty() { 1.0 } else { 0.0 });
            r.resize(3 * d, 0.0);
        }
        r
    }

    /// Boxes from gathered records (upstream end of `allGather`): one box
    /// per `3 · dim` record of `data`. A record whose flag is `1` is
    /// returned as the empty box (its bounds are those of an empty box
    /// anyway when it was produced by [`BBox::gather_record`]).
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] if `data` is not made of whole records
    /// (a trailing partial record, or any value with `dim = 0`; upstream
    /// always receives `nb_proc · 3 · dim` values).
    pub fn from_gather_records(data: &[Real], dim: usize) -> Result<Vec<BBox>> {
        let record = 3 * dim;
        // `is_multiple_of(0)` is true only for 0: no value without a dimension
        let whole = data.len().is_multiple_of(record);
        if !whole {
            return Err(Error::DimensionMismatch {
                context: "BBox::from_gather_records (whole 3·dim records)",
                expected: if record == 0 {
                    0
                } else {
                    data.len() - data.len() % record
                },
                got: data.len(),
            });
        }
        if dim == 0 {
            return Ok(Vec::new());
        }
        Ok(data
            .chunks_exact(record)
            .map(|rec| {
                if rec[2 * dim] == 1.0 {
                    BBox::empty(dim)
                } else {
                    BBox {
                        lower: rec[..dim].to_vec(),
                        upper: rec[dim..2 * dim].to_vec(),
                    }
                }
            })
            .collect())
    }

    /// Upstream `intersection(other, communicator)` once `other` was
    /// gathered from every rank: the non-empty intersections of `self` with
    /// each rank's box, keyed by rank (ascending, `std::map` order).
    #[must_use]
    pub fn intersections_with(&self, others: &[BBox]) -> BTreeMap<usize, BBox> {
        others
            .iter()
            .enumerate()
            .filter_map(|(r, b)| {
                let i = self.intersection(b);
                (!i.is_empty()).then_some((r, i))
            })
            .collect()
    }
}

/// Upstream `printself`: `BBox[<lower> - <upper>]`, or `BBox[]` when empty;
/// vectors use Akantu's Eigen format (`[[x], [y]]`, `%g` with 6 digits as
/// `std::ostream` by default).
impl fmt::Display for BBox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let vec = |v: &[Real]| {
            let items: Vec<String> = v
                .iter()
                .map(|&x| format!("[{}]", crate::random::fmt_c::format_g(x, 6)))
                .collect();
            format!("[{}]", items.join(", "))
        };
        write!(f, "BBox[")?;
        if !self.is_empty() {
            write!(f, "{} - {}", vec(&self.lower), vec(&self.upper))?;
        }
        write!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Upstream initialises empty boxes to ±`DBL_MAX`
    /// (`aka_bbox.hh:45-49,125-126`), not ±∞.
    #[test]
    fn empty_bbox_matches_upstream_limits() {
        let b = BBox::empty(3);
        assert!(b.lower.iter().all(|&l| l.to_bits() == Real::MAX.to_bits()));
        assert!(
            b.upper
                .iter()
                .all(|&u| u.to_bits() == (-Real::MAX).to_bits())
        );
        assert!(b.lower.iter().chain(&b.upper).all(|v| v.is_finite()));
        let mut c = b.clone();
        c.add_point(&[1.0, 2.0, 3.0]);
        c.reset();
        assert_eq!(c, b);
    }

    #[test]
    fn helpers_follow_upstream() {
        let mut a = BBox::empty(2);
        a.add_point(&[0.0, 0.0]);
        a.add_point(&[2.0, 1.0]);
        assert_eq!(a.size_in(1), Some(1.0));
        assert_eq!(a.size_in(2), None);
        let mut b = BBox::empty(2);
        b.add_point(&[1.0, 5.0]);
        b.add_point(&[3.0, 6.0]);
        // overlap in x only: per-direction test true for 0, false for 1
        assert!(a.intersects_in(&b, 0) && !a.intersects_in(&b, 1));
        assert!(!a.intersects(&b) && a.intersection(&b).is_empty());
        // NaN coordinates keep the current bounds (std::min/std::max)
        let mut n = a.clone();
        n.add_point(&[f64::NAN, 0.5]);
        assert_eq!(n, a);
        // inverted (explicitly set) boxes intersect / contain nothing
        let inv = BBox {
            lower: vec![1.0, 1.0],
            upper: vec![0.0, 0.0],
        };
        assert!(!inv.contains(&[0.5, 0.5]) && !inv.intersects(&a) && !a.intersects(&inv));
    }

    /// Upstream's default `BBox()` (dimension 0) is
    /// empty and intersects nothing; a gathered buffer is whole records.
    #[test]
    fn zero_dimensional_box_is_empty_and_partial_records_are_refused() {
        let z = BBox::empty(0);
        assert!(z.is_empty());
        assert!(!z.intersects(&z) && z.intersection(&z).is_empty());
        assert_eq!(z.to_string(), "BBox[]");
        // upstream `contains` on zero-size arrays: `all()` of nothing
        assert!(z.contains(&[]));
        let mut a = BBox::empty(2);
        a.add_point(&[0.0, 1.0]);
        let mut data = a.gather_record();
        data.push(0.5); // trailing partial record
        assert!(BBox::from_gather_records(&data, 2).is_err());
        assert_eq!(BBox::from_gather_records(&data[..6], 2).unwrap(), vec![a]);
        assert!(BBox::from_gather_records(&[], 0).unwrap().is_empty());
        assert!(BBox::from_gather_records(&[1.0], 0).is_err());
    }

    #[test]
    fn gather_records_roundtrip_and_rank_intersections() {
        let mut a = BBox::empty(2);
        a.add_point(&[0.0, 0.0]);
        a.add_point(&[2.0, 2.0]);
        let mut b = BBox::empty(2);
        b.add_point(&[1.0, 1.0]);
        b.add_point(&[3.0, 3.0]);
        let e = BBox::empty(2);
        assert_eq!(a.gather_record(), vec![0.0, 0.0, 2.0, 2.0, 0.0, 0.0]);
        assert_eq!(e.gather_record()[4..], [1.0, 0.0]);
        let data: Vec<Real> = [&a, &e, &b]
            .iter()
            .flat_map(|x| x.gather_record())
            .collect();
        let boxes = BBox::from_gather_records(&data, 2).unwrap();
        assert_eq!(boxes, vec![a.clone(), e.clone(), b.clone()]);
        let inter = a.intersections_with(&boxes);
        assert_eq!(inter.keys().copied().collect::<Vec<_>>(), vec![0, 2]);
        assert_eq!(inter[&2].lower, vec![1.0, 1.0]);
        assert_eq!(inter[&2].upper, vec![2.0, 2.0]);
    }

    #[test]
    fn display_matches_upstream_printself() {
        let mut a = BBox::empty(2);
        assert_eq!(a.to_string(), "BBox[]");
        a.add_point(&[0.0, -1.5]);
        a.add_point(&[1e-7, 2.0]);
        assert_eq!(a.to_string(), "BBox[[[0], [-1.5]] - [[1e-07], [2]]]");
    }
}
