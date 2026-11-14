// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements ordering normalisation for the
// differential-testing harness (docs/NUMERICAL_VALIDATION.md).

//! Ordering normalisation: build bijections between the entity numbering of
//! the oracle and of akantu-rs when they legitimately differ (mesh
//! distribution, facet/cohesive insertion, renumbering, hash-map iteration
//! order), then permute arrays into the reference order.
//!
//! All functions return a permutation `perm` with the convention
//! `actual[perm[i]]` corresponds to `expected[i]`; apply it with
//! [`Field::permuted`] / [`crate::array::permute_axis`] on the *actual*
//! array to bring it into reference order.

use std::collections::HashMap;

use crate::array::Field;
use crate::error::{Error, Result};

/// Match entities by integer ids (e.g. global node ids).
pub fn perm_from_ids(expected: &[i64], actual: &[i64]) -> Result<Vec<usize>> {
    if expected.len() != actual.len() {
        return Err(Error::ordering(format!(
            "{} expected ids vs {} actual ids",
            expected.len(),
            actual.len()
        )));
    }
    let mut pos: HashMap<i64, usize> = HashMap::with_capacity(actual.len());
    for (i, &id) in actual.iter().enumerate() {
        if pos.insert(id, i).is_some() {
            return Err(Error::ordering(format!("duplicate actual id {id}")));
        }
    }
    let mut used = vec![false; actual.len()];
    expected
        .iter()
        .map(|id| {
            let p = *pos.get(id).ok_or_else(|| {
                Error::ordering(format!("expected id {id} not found in actual ids"))
            })?;
            if std::mem::replace(&mut used[p], true) {
                return Err(Error::ordering(format!("duplicate expected id {id}")));
            }
            Ok(p)
        })
        .collect()
}

/// Largest number of coordinate directions supported by the grid search
/// (`3^dim` neighbour cells are visited per point).
pub const MAX_POINT_DIM: usize = 8;

/// Match points by coordinates within `tol` (max-norm).
///
/// `expected` and `actual` are row-major `[n, dim]`. Uses a uniform grid of
/// cell size `tol` so the cost is linear; fails if a point has no partner
/// or more than one partner within `tol` (ambiguous meshes need ids).
/// Finite inputs never panic: a coordinate/tolerance ratio the integer grid
/// cannot represent (`|x / tol| >= 2^62`), or more than [`MAX_POINT_DIM`]
/// dimensions, is an [`Error::Ordering`].
pub fn perm_from_points(
    expected: &[f64],
    actual: &[f64],
    dim: usize,
    tol: f64,
) -> Result<Vec<usize>> {
    if dim == 0 || !expected.len().is_multiple_of(dim) || actual.len() != expected.len() {
        return Err(Error::ordering(format!(
            "point arrays of {} and {} values are not both [n, {dim}]",
            expected.len(),
            actual.len()
        )));
    }
    if dim > MAX_POINT_DIM {
        return Err(Error::ordering(format!(
            "{dim} coordinate directions exceed the supported maximum {MAX_POINT_DIM}"
        )));
    }
    if !(tol.is_finite() && tol > 0.0) {
        return Err(Error::ordering(
            "coordinate tolerance must be finite and > 0",
        ));
    }
    if expected.iter().chain(actual).any(|x| !x.is_finite()) {
        return Err(Error::ordering("non-finite coordinate"));
    }
    // Grid cell of a coordinate; the cell index must leave room for the
    // +-1 neighbour offsets.
    const CELL_LIMIT: f64 = 4_611_686_018_427_387_904.0; // 2^62
    let cell = |x: f64| -> Result<i64> {
        let c = (x / tol).floor();
        if !(c.is_finite() && c.abs() < CELL_LIMIT) {
            return Err(Error::ordering(format!(
                "coordinate {x:e} with tolerance {tol:e} does not fit the matching grid \
                 (|x / tol| must stay below 2^62); use a larger tolerance or ids"
            )));
        }
        #[allow(clippy::cast_possible_truncation)] // range checked above
        Ok(c as i64)
    };
    let mut grid: HashMap<Vec<i64>, Vec<usize>> = HashMap::new();
    for (j, p) in actual.chunks_exact(dim).enumerate() {
        let key: Vec<i64> = p.iter().map(|&x| cell(x)).collect::<Result<_>>()?;
        grid.entry(key).or_default().push(j);
    }
    let mut used = vec![false; actual.len() / dim];
    let mut perm = Vec::with_capacity(used.len());
    let mut key = vec![0i64; dim];
    let n_neighbours = 3usize.pow(u32::try_from(dim).unwrap_or(u32::MAX));
    for (i, p) in expected.chunks_exact(dim).enumerate() {
        let base: Vec<i64> = p.iter().map(|&x| cell(x)).collect::<Result<_>>()?;
        let mut found: Option<usize> = None;
        for code in 0..n_neighbours {
            let mut c = code;
            for d in 0..dim {
                // offset in {-1, 0, 1}
                #[allow(clippy::cast_possible_wrap)] // c % 3 < 3
                let off = (c % 3) as i64 - 1;
                key[d] = base[d]
                    .checked_add(off)
                    .ok_or_else(|| Error::ordering("grid cell index overflow"))?;
                c /= 3;
            }
            if let Some(cands) = grid.get(&key) {
                for &j in cands {
                    let q = &actual[j * dim..(j + 1) * dim];
                    if p.iter().zip(q).all(|(a, b)| (a - b).abs() <= tol) {
                        if found.is_some_and(|f| f != j) {
                            return Err(Error::ordering(format!(
                                "expected point {i} {p:?} matches several actual points within {tol:e}"
                            )));
                        }
                        found = Some(j);
                    }
                }
            }
        }
        let j = found.ok_or_else(|| {
            Error::ordering(format!(
                "expected point {i} {p:?} has no actual partner within {tol:e}"
            ))
        })?;
        if std::mem::replace(&mut used[j], true) {
            return Err(Error::ordering(format!(
                "actual point {j} matched twice (points closer than {tol:e})"
            )));
        }
        perm.push(j);
    }
    Ok(perm)
}

/// Match elements by connectivity (node sets, independent of the local
/// node order).
///
/// This only *identifies* elements: a reversed or rotated local node order
/// (a flipped orientation) matches as well. Tests that care about the
/// local numbering must compare the permuted, renumbered connectivity rows
/// separately (e.g. with [`crate::compare_i64`]).
///
/// `expected` / `actual` are row-major `[n_el, nnpe]` connectivities.
/// `actual_to_expected_node` maps actual node ids into the expected
/// numbering (from [`perm_from_ids`] or [`perm_from_points`] inverted with
/// [`invert`]); pass `None` when both share the node numbering.
pub fn perm_from_connectivity(
    expected: &[i64],
    actual: &[i64],
    nnpe: usize,
    actual_to_expected_node: Option<&[usize]>,
) -> Result<Vec<usize>> {
    if nnpe == 0 || !expected.len().is_multiple_of(nnpe) || actual.len() != expected.len() {
        return Err(Error::ordering(format!(
            "connectivities of {} and {} values are not both [n, {nnpe}]",
            expected.len(),
            actual.len()
        )));
    }
    let map_node = |n: i64| -> Result<i64> {
        match actual_to_expected_node {
            None => Ok(n),
            Some(m) => {
                let idx = usize::try_from(n)
                    .map_err(|_| Error::ordering(format!("negative node id {n}")))?;
                let mapped = m
                    .get(idx)
                    .ok_or_else(|| Error::ordering(format!("node {n} outside the node map")))?;
                i64::try_from(*mapped).map_err(|_| Error::ordering("node id overflow"))
            }
        }
    };
    let key = |row: &[i64], remap: bool| -> Result<Vec<i64>> {
        let mut k: Vec<i64> = if remap {
            row.iter().map(|&n| map_node(n)).collect::<Result<_>>()?
        } else {
            row.to_vec()
        };
        k.sort_unstable();
        Ok(k)
    };
    let mut pos: HashMap<Vec<i64>, usize> = HashMap::new();
    for (j, row) in actual.chunks_exact(nnpe).enumerate() {
        if pos.insert(key(row, true)?, j).is_some() {
            return Err(Error::ordering(format!(
                "actual element {j} duplicates another node set"
            )));
        }
    }
    let mut used = vec![false; actual.len() / nnpe];
    expected
        .chunks_exact(nnpe)
        .enumerate()
        .map(|(i, row)| {
            let k = key(row, false)?;
            let j = *pos.get(&k).ok_or_else(|| {
                Error::ordering(format!("expected element {i} {row:?} not found"))
            })?;
            if std::mem::replace(&mut used[j], true) {
                return Err(Error::ordering(format!(
                    "expected element {i} duplicates another node set"
                )));
            }
            Ok(j)
        })
        .collect()
}

/// Inverse permutation.
pub fn invert(perm: &[usize]) -> Result<Vec<usize>> {
    let mut inv = vec![usize::MAX; perm.len()];
    for (i, &p) in perm.iter().enumerate() {
        if p >= perm.len() || inv[p] != usize::MAX {
            return Err(Error::ordering(format!("not a permutation (index {p})")));
        }
        inv[p] = i;
    }
    Ok(inv)
}

/// Renumber ids through a map (`out[i] = map[ids[i]]`), e.g. to express an
/// actual connectivity in the expected node numbering.
pub fn renumber(ids: &[i64], map: &[usize]) -> Result<Vec<i64>> {
    ids.iter()
        .map(|&n| {
            let idx =
                usize::try_from(n).map_err(|_| Error::ordering(format!("negative id {n}")))?;
            let m = *map
                .get(idx)
                .ok_or_else(|| Error::ordering(format!("id {n} outside the map")))?;
            i64::try_from(m).map_err(|_| Error::ordering("id overflow"))
        })
        .collect()
}

/// Lexicographic sort of the slices along `axis` (set semantics), for
/// quantities whose order is meaningless (e.g. group member lists).
/// Returns the sorted field and the permutation applied.
pub fn sort_along_axis<T: Copy + PartialOrd>(
    field: &Field<T>,
    axis: usize,
) -> Result<(Field<T>, Vec<usize>)> {
    if axis >= field.shape.len() {
        return Err(Error::shape(format!("axis {axis} out of range")));
    }
    let n = field.shape[axis];
    let outer: usize = field.shape[..axis].iter().product();
    let inner: usize = field.shape[axis + 1..].iter().product();
    if outer > 1 {
        return Err(Error::ordering(
            "sort_along_axis only supports a leading axis (no outer dimensions)",
        ));
    }
    let row = |i: usize| &field.data[i * inner..(i + 1) * inner];
    let mut perm: Vec<usize> = (0..n).collect();
    let mut incomparable = false;
    perm.sort_by(|&a, &b| {
        for (x, y) in row(a).iter().zip(row(b)) {
            match x.partial_cmp(y) {
                Some(std::cmp::Ordering::Equal) => {}
                Some(o) => return o,
                None => {
                    incomparable = true;
                    return std::cmp::Ordering::Equal;
                }
            }
        }
        std::cmp::Ordering::Equal
    });
    if incomparable {
        return Err(Error::ordering("cannot sort rows containing NaN"));
    }
    Ok((field.permuted(axis, &perm)?, perm))
}

/// Drop ghost entries along `axis` (`is_ghost[i]` true for ghosts).
pub fn strip_ghosts<T: Copy>(field: &Field<T>, axis: usize, is_ghost: &[bool]) -> Result<Field<T>> {
    let keep: Vec<bool> = is_ghost.iter().map(|g| !g).collect();
    field.selected(axis, &keep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        let p = perm_from_ids(&[10, 20, 30], &[30, 10, 20]).unwrap();
        assert_eq!(p, vec![1, 2, 0]);
        assert!(perm_from_ids(&[1, 2], &[1, 1]).is_err());
        assert!(perm_from_ids(&[1, 1], &[1, 2]).is_err());
        assert!(perm_from_ids(&[1, 3], &[1, 2]).is_err());
        assert!(perm_from_ids(&[1], &[1, 2]).is_err());
    }

    #[test]
    fn points_with_noise_and_cell_boundaries() {
        // 2D grid points; actual is shuffled and perturbed, some points sit
        // exactly on grid-cell boundaries.
        let expected = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.5, 0.5];
        let actual = [
            1.0 + 1e-13,
            1.0,
            0.5,
            0.5 - 1e-13,
            0.0,
            0.0,
            0.0,
            1.0,
            1.0,
            -1e-13,
        ];
        let p = perm_from_points(&expected, &actual, 2, 1e-10).unwrap();
        assert_eq!(p, vec![2, 4, 3, 0, 1]);
        // Apply and verify.
        let f = Field::new(
            vec![5, 2],
            actual.to_vec(),
            vec!["node".into(), "spatial_dim".into()],
        )
        .unwrap();
        let g = f.permuted(0, &p).unwrap();
        for (a, e) in g.data.iter().zip(expected) {
            assert!((a - e).abs() < 1e-12);
        }
    }

    #[test]
    fn points_errors() {
        assert!(perm_from_points(&[0.0, 0.0], &[1.0, 1.0], 2, 1e-6).is_err());
        // Ambiguous: two actual points within tol.
        assert!(perm_from_points(&[0.0, 1.0], &[0.0, 1e-9], 1, 1e-6).is_err());
        assert!(perm_from_points(&[0.0], &[f64::NAN], 1, 1e-6).is_err());
        assert!(perm_from_points(&[0.0], &[0.0], 1, 0.0).is_err());
        assert!(perm_from_points(&[0.0, 1.0, 2.0], &[0.0, 1.0, 2.0], 2, 1e-3).is_err());
        assert_eq!(
            perm_from_points(&[], &[], 3, 1e-3).unwrap(),
            Vec::<usize>::new()
        );
    }

    /// Extreme but finite inputs return errors instead of panicking.
    #[test]
    fn points_extreme_inputs_do_not_panic() {
        // The cell index saturates, the neighbour offset
        // overflowed in debug builds.
        let e = perm_from_points(&[1.0], &[1.0], 1, 1e-30).unwrap_err();
        assert!(e.to_string().contains("2^62"), "{e}");
        assert!(perm_from_points(&[1e300], &[1e300], 1, 1e-3).is_err());
        assert!(perm_from_points(&[f64::MAX, 0.0], &[f64::MAX, 0.0], 1, 1.0).is_err());
        // Ratios just inside the grid work.
        let p = perm_from_points(&[1e18, 0.0], &[0.0, 1e18], 1, 1.0).unwrap();
        assert_eq!(p, vec![1, 0]);
        // Too many dimensions is an error, not a 3^dim search.
        let big = vec![0.0; MAX_POINT_DIM + 1];
        assert!(perm_from_points(&big, &big, MAX_POINT_DIM + 1, 1e-3).is_err());
        let ok = vec![0.5; MAX_POINT_DIM];
        assert_eq!(
            perm_from_points(&ok, &ok, MAX_POINT_DIM, 1e-3).unwrap(),
            vec![0]
        );
        // Points on cell boundaries with a tiny tolerance still match.
        let x = [0.0, 1e-12, 2e-12];
        let y = [2e-12, 0.0, 1e-12];
        assert_eq!(perm_from_points(&x, &y, 1, 1e-13).unwrap(), vec![1, 2, 0]);
        // Coincident nodes are ambiguous.
        assert!(perm_from_points(&[0.0, 0.0], &[0.0, 0.0], 1, 1e-6).is_err());
    }

    #[test]
    fn connectivity_with_node_renumbering() {
        // Two triangles; actual uses a different node numbering, a different
        // element order, and rotated local node order.
        let expected = [0, 1, 2, 1, 3, 2];
        // actual node j corresponds to expected node a2e[j]
        let a2e = [3, 2, 1, 0];
        // expected tri (1,3,2) -> actual nodes (2,0,1); expected (0,1,2) -> actual (3,2,1)
        let actual = [0, 1, 2, 2, 1, 3];
        let p = perm_from_connectivity(&expected, &actual, 3, Some(&a2e)).unwrap();
        assert_eq!(p, vec![1, 0]);
        // Without the node map the node sets do not all exist.
        assert!(perm_from_connectivity(&expected, &[0, 1, 2, 2, 1, 4], 3, None).is_err());
        assert!(perm_from_connectivity(&[0, 1, 0, 1], &[0, 1, 1, 0], 2, None).is_err());
        let renum = renumber(&actual, &a2e).unwrap();
        assert_eq!(renum, vec![3, 2, 1, 1, 2, 0]);
        assert!(renumber(&[-1], &a2e).is_err());
        assert!(renumber(&[7], &a2e).is_err());
    }

    #[test]
    fn invert_and_sort() {
        assert_eq!(invert(&[2, 0, 1]).unwrap(), vec![1, 2, 0]);
        assert!(invert(&[0, 0]).is_err());
        let f = Field::new(
            vec![3, 2],
            vec![2, 1, 0, 5, 2, 0],
            vec!["group_member".into(), "component".into()],
        )
        .unwrap();
        let (s, p) = sort_along_axis(&f, 0).unwrap();
        assert_eq!(s.data, vec![0, 5, 2, 0, 2, 1]);
        assert_eq!(p, vec![1, 2, 0]);
        let g = Field::new(vec![2], vec![f64::NAN, 1.0], vec!["entry".into()]).unwrap();
        assert!(sort_along_axis(&g, 0).is_err());
        let h = Field::new(
            vec![2, 2],
            vec![1, 2, 3, 4],
            vec!["step".into(), "node".into()],
        )
        .unwrap();
        assert!(sort_along_axis(&h, 1).is_err());
    }

    /// Duplicate ids are ambiguous, missing ids are errors.
    #[test]
    fn ids_duplicates_and_missing() {
        assert!(perm_from_ids(&[1, 1], &[1, 2]).is_err());
        assert!(perm_from_ids(&[1, 2], &[2, 2]).is_err());
        assert!(perm_from_ids(&[1, 2], &[1, 3]).is_err());
        assert!(perm_from_ids(&[1], &[1, 2]).is_err());
        assert_eq!(
            perm_from_ids(&[5, 7, 9], &[9, 5, 7]).unwrap(),
            vec![1, 2, 0]
        );
    }

    /// Matching node sets identifies an element but does not
    /// verify its local orientation; a separate row comparison does.
    #[test]
    fn connectivity_matching_does_not_check_orientation() {
        let expected = [0, 1, 2, 2, 1, 3];
        let reversed = [2, 1, 0, 3, 1, 2]; // both elements flipped
        let p = perm_from_connectivity(&expected, &reversed, 3, None).unwrap();
        assert_eq!(p, vec![0, 1], "identification succeeds");
        let e = Field::new(
            vec![2, 3],
            expected.to_vec(),
            vec!["element".into(), "node_local".into()],
        )
        .unwrap();
        let a = Field::new(
            vec![2, 3],
            reversed.to_vec(),
            vec!["element".into(), "node_local".into()],
        )
        .unwrap()
        .permuted(0, &p)
        .unwrap();
        let r = crate::compare_i64("c", "connectivity", &e, &a.shape, &a.data);
        assert!(
            !r.passed(),
            "the orientation difference is caught by the row comparison"
        );
        // Duplicate node sets on the expected side are ambiguous.
        assert!(perm_from_connectivity(&[0, 1, 1, 0], &[0, 1, 2, 3], 2, None).is_err());
    }

    #[test]
    fn ghosts() {
        let f = Field::new(
            vec![3, 1],
            vec![1.0, 2.0, 3.0],
            vec!["node".into(), "component".into()],
        )
        .unwrap();
        let g = strip_ghosts(&f, 0, &[false, true, false]).unwrap();
        assert_eq!(g.data, vec![1.0, 3.0]);
        assert_eq!(g.shape, vec![2, 1]);
        // Ghosts along a non-leading axis ([step, node, c]).
        let h = Field::new(
            vec![2, 3, 2],
            (0..12).map(f64::from).collect(),
            vec!["step".into(), "node".into(), "component".into()],
        )
        .unwrap();
        let s = strip_ghosts(&h, 1, &[false, true, false]).unwrap();
        assert_eq!(s.shape, vec![2, 2, 2]);
        assert_eq!(s.data, vec![0.0, 1.0, 4.0, 5.0, 6.0, 7.0, 10.0, 11.0]);
        // A mask of the wrong length is an error, not a silent truncation.
        assert!(strip_ghosts(&h, 1, &[false, true]).is_err());
        assert!(strip_ghosts(&h, 3, &[false]).is_err());
    }
}
