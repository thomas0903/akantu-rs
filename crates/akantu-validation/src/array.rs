// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it belongs to the validation harness.

//! Small owned n-d array type with named axes, plus index helpers.
//!
//! The harness only needs row-major storage, multi-index decoding for
//! reports and axis permutation/selection for ordering normalisation, so
//! this is intentionally tiny (no linear algebra).

use crate::error::{Error, Result};
use crate::npy::shape_len;

/// Owned row-major n-d array with one name per axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Field<T> {
    /// Dimensions.
    pub shape: Vec<usize>,
    /// Row-major data, `product(shape)` elements.
    pub data: Vec<T>,
    /// Axis names (same length as `shape`), see the axis vocabulary in
    /// `docs/NUMERICAL_VALIDATION.md`.
    pub axes: Vec<String>,
}

impl<T: Copy> Field<T> {
    /// Build a field, validating lengths.
    pub fn new(shape: Vec<usize>, data: Vec<T>, axes: Vec<String>) -> Result<Self> {
        let n = shape_len(&shape)?;
        if n != data.len() {
            return Err(Error::shape(format!(
                "shape {shape:?} needs {n} elements, data has {}",
                data.len()
            )));
        }
        if axes.len() != shape.len() {
            return Err(Error::shape(format!(
                "{} axis names for a {}-d array",
                axes.len(),
                shape.len()
            )));
        }
        Ok(Self { shape, data, axes })
    }

    /// Index of the axis called `name`.
    #[must_use]
    pub fn axis(&self, name: &str) -> Option<usize> {
        self.axes.iter().position(|a| a == name)
    }

    /// Reorder entries along `axis` so that output index `i` holds input
    /// index `perm[i]` (see [`permute_axis`]).
    pub fn permuted(&self, axis: usize, perm: &[usize]) -> Result<Self> {
        Ok(Self {
            shape: self.shape.clone(),
            data: permute_axis(&self.shape, &self.data, axis, perm)?,
            axes: self.axes.clone(),
        })
    }

    /// Keep only the entries along `axis` where `mask` is true.
    pub fn selected(&self, axis: usize, mask: &[bool]) -> Result<Self> {
        let (shape, data) = select_axis(&self.shape, &self.data, axis, mask)?;
        Ok(Self {
            shape,
            data,
            axes: self.axes.clone(),
        })
    }
}

/// Row-major strides (in elements) of a shape.
#[must_use]
pub fn strides(shape: &[usize]) -> Vec<usize> {
    let mut s = vec![1usize; shape.len()];
    for k in (0..shape.len().saturating_sub(1)).rev() {
        s[k] = s[k + 1] * shape[k + 1];
    }
    s
}

/// Decode a flat row-major offset into a multi-index.
#[must_use]
pub fn unravel(shape: &[usize], mut flat: usize) -> Vec<usize> {
    let mut idx = vec![0usize; shape.len()];
    for k in (0..shape.len()).rev() {
        let d = shape[k].max(1);
        idx[k] = flat % d;
        flat /= d;
    }
    idx
}

/// Reorder entries along `axis`: output index `i` along the axis holds the
/// input slice at index `perm[i]`. `perm` must be a permutation of
/// `0..shape[axis]`.
pub fn permute_axis<T: Copy>(
    shape: &[usize],
    data: &[T],
    axis: usize,
    perm: &[usize],
) -> Result<Vec<T>> {
    check_layout(shape, data.len(), axis)?;
    let n = shape[axis];
    if perm.len() != n {
        return Err(Error::ordering(format!(
            "permutation has {} entries, axis {axis} has length {n}",
            perm.len()
        )));
    }
    let mut seen = vec![false; n];
    for &p in perm {
        if p >= n || std::mem::replace(&mut seen[p], true) {
            return Err(Error::ordering(format!(
                "not a permutation of 0..{n} (bad or repeated index {p})"
            )));
        }
    }
    let outer: usize = shape[..axis].iter().product();
    let inner: usize = shape[axis + 1..].iter().product();
    let mut out = Vec::with_capacity(data.len());
    for o in 0..outer {
        for &p in perm {
            let start = (o * n + p) * inner;
            out.extend_from_slice(&data[start..start + inner]);
        }
    }
    Ok(out)
}

/// Keep only the slices along `axis` where `mask` is true. Returns the new
/// shape and data.
pub fn select_axis<T: Copy>(
    shape: &[usize],
    data: &[T],
    axis: usize,
    mask: &[bool],
) -> Result<(Vec<usize>, Vec<T>)> {
    check_layout(shape, data.len(), axis)?;
    let n = shape[axis];
    if mask.len() != n {
        return Err(Error::shape(format!(
            "mask has {} entries, axis {axis} has length {n}",
            mask.len()
        )));
    }
    let outer: usize = shape[..axis].iter().product();
    let inner: usize = shape[axis + 1..].iter().product();
    let mut out = Vec::new();
    for o in 0..outer {
        for (i, _) in mask.iter().enumerate().filter(|(_, m)| **m) {
            let start = (o * n + i) * inner;
            out.extend_from_slice(&data[start..start + inner]);
        }
    }
    let mut new_shape = shape.to_vec();
    new_shape[axis] = mask.iter().filter(|m| **m).count();
    Ok((new_shape, out))
}

fn check_layout(shape: &[usize], len: usize, axis: usize) -> Result<()> {
    if axis >= shape.len() {
        return Err(Error::shape(format!(
            "axis {axis} out of range for a {}-d array",
            shape.len()
        )));
    }
    let n = shape_len(shape)?;
    if n != len {
        return Err(Error::shape(format!(
            "shape {shape:?} needs {n} elements, data has {len}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strides_and_unravel() {
        assert_eq!(strides(&[2, 3, 4]), vec![12, 4, 1]);
        assert_eq!(strides(&[]), Vec::<usize>::new());
        assert_eq!(unravel(&[2, 3, 4], 23), vec![1, 2, 3]);
        assert_eq!(unravel(&[2, 3, 4], 0), vec![0, 0, 0]);
        assert_eq!(unravel(&[], 0), Vec::<usize>::new());
    }

    #[test]
    fn permute_middle_axis() {
        // shape (2, 3, 1): values encode (i, j).
        let data: Vec<i32> = vec![0, 1, 2, 10, 11, 12];
        let out = permute_axis(&[2, 3, 1], &data, 1, &[2, 0, 1]).unwrap();
        assert_eq!(out, vec![2, 0, 1, 12, 10, 11]);
        assert!(permute_axis(&[2, 3, 1], &data, 1, &[0, 0, 1]).is_err());
        assert!(permute_axis(&[2, 3, 1], &data, 1, &[0, 1]).is_err());
        assert!(permute_axis(&[2, 3, 1], &data, 3, &[0]).is_err());
    }

    #[test]
    fn select_first_axis() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let (s, d) = select_axis(&[3, 2], &data, 0, &[true, false, true]).unwrap();
        assert_eq!(s, vec![2, 2]);
        assert_eq!(d, vec![1.0, 2.0, 5.0, 6.0]);
        let f = Field::new(vec![3, 2], data, vec!["node".into(), "component".into()]).unwrap();
        assert_eq!(f.axis("component"), Some(1));
        let g = f.selected(1, &[false, true]).unwrap();
        assert_eq!(g.data, vec![2.0, 4.0, 6.0]);
        assert!(Field::new(vec![2], vec![1.0], vec!["x".into()]).is_err());
        assert!(Field::new(vec![1], vec![1.0], vec![]).is_err());
    }
}
