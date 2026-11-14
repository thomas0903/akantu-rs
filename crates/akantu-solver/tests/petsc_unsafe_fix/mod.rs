// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: regression tests (`ua03_*` and the
// AO sweep)
// over the safe wrappers of `akantu_linalg::petsc` (whose calls follow
// src/solver/sparse_matrix_petsc.cc and src/model/common/dof_manager/
// dof_manager_petsc.cc).

//! Scenarios of the PETSc tier (`petsc_mpi`, real MPI at 1 / 2 / 4 ranks)
//! for the indices, lengths and layouts the safe PETSc wrappers hand to
//! PETSc:
//!
//! * `ua01_add_row_range`: on an unpreallocated matrix (at one rank the
//!   `MATSEQAIJ` hash insertion that wrote out of bounds)
//!   `Matrix::add_row` refuses a row or column outside `0..size` — the
//!   reproducer's `row = 3` (first rows past a 2-row-per-rank matrix:
//!   `size`, `size + 1`), `row = 10⁶`, negative indices — before any PETSc
//!   call; the matrix assembles intact afterwards (diagonal 2, nothing
//!   else); `Matrix::get_values` refuses out-of-range indices in Rust and a
//!   row of another rank through PETSc;
//! * `ua03_ksp_layout`: `Ksp::solve` refuses vectors whose layout differs
//!   from the matrix's, before any PETSc call;
//! * `ao_map_range`: `ao_map` with an application index outside `0..N` on
//!   one rank is refused on every rank (the shim's agreed check before
//!   `AOCreateMemoryScalable`), and a valid map still works afterwards.

use akantu_comm::Communicator;
use akantu_core::Error;
use akantu_linalg::petsc;

type Outcome = std::result::Result<(), String>;

fn e2s(e: Error) -> String {
    e.to_string()
}

/// `true` for the refusal of an index by the Rust wrappers (never a PETSc
/// `Error::Backend`).
fn refused_in_rust(r: &akantu_core::Result<impl std::fmt::Debug>) -> bool {
    matches!(
        r,
        Err(Error::OutOfRange { .. } | Error::InvalidArgument(_) | Error::DimensionMismatch { .. })
    )
}

/// Out-of-range rows and columns are refused by
/// `Matrix::add_row` in Rust; the unpreallocated matrix assembles intact.
pub fn ua01_add_row_range(c: &Communicator) -> Outcome {
    let session = petsc::Session::acquire(c).map_err(e2s)?;
    let (r, p) = (c.rank() as usize, c.size());
    let n = 2usize;
    let size = n * p;
    let first = i32::try_from(n * r).map_err(|e| e.to_string())?;
    let big = i32::try_from(size).map_err(|e| e.to_string())?;
    let mut m = c
        .native_collective("ua01:create", Ok(()), |nc| {
            petsc::Matrix::create(&nc, c, &session, "ua01", n, size, false)
        })
        .map_err(e2s)?
        .map_err(e2s)?;
    for k in 0..2 {
        m.add_row(first + k, &[first + k], &[2.0]).map_err(e2s)?;
    }
    // rows past the end (the reproducer's row = 3 at one rank is
    // `size + 1`; `size` is the first index past the matrix), far past it,
    // negative; then the same for columns
    let rows = [big, big + 1, big + 64, 1_000_000, -1];
    for row in rows {
        let res = m.add_row(row, &[first], &[1.0]);
        if !refused_in_rust(&res) {
            return Err(format!("add_row(row = {row}) not refused in Rust: {res:?}"));
        }
    }
    for col in rows {
        let res = m.add_row(first, &[first, col], &[1.0, 1.0]);
        if !refused_in_rust(&res) {
            return Err(format!("add_row(col = {col}) not refused in Rust: {res:?}"));
        }
    }
    c.native_collective("ua01:assemble", Ok(()), |nc| m.assemble(&nc))
        .map_err(e2s)?
        .map_err(e2s)?;
    // intact: diagonal 2, every other entry of the owned rows 0
    let owned: Vec<i32> = (first..first + 2).collect();
    let all: Vec<i32> = (0..big).collect();
    let v = m.get_values(&owned, &all).map_err(e2s)?;
    for (k, &row) in owned.iter().enumerate() {
        for (j, &col) in all.iter().enumerate() {
            let want = if row == col { 2.0 } else { 0.0 };
            let got = v[k * all.len() + j];
            if got.to_bits() != f64::to_bits(want) {
                return Err(format!("entry ({row}, {col}) = {got}, expected {want}"));
            }
        }
    }
    let x = vec![1.0; n];
    let mut y = vec![0.0; n];
    c.native_collective("ua01:mult", Ok(()), |nc| m.mult(&nc, &x, &mut y))
        .map_err(e2s)?
        .map_err(e2s)?;
    if y.iter().any(|v| v.to_bits() != 2.0_f64.to_bits()) {
        return Err(format!("A·1 = {y:?} after the refused insertions"));
    }
    // get_values: out-of-range indices refused in Rust, a row of another
    // rank by PETSc (`MatGetValues_MPIAIJ`, "Only local values")
    for row in rows {
        let res = m.get_values(&[row], &[first]);
        if !refused_in_rust(&res) {
            return Err(format!(
                "get_values(row = {row}) not refused in Rust: {res:?}"
            ));
        }
        let res = m.get_values(&[first], &[row]);
        if !refused_in_rust(&res) {
            return Err(format!(
                "get_values(col = {row}) not refused in Rust: {res:?}"
            ));
        }
    }
    if p > 1 {
        let other = i32::try_from(n * ((r + 1) % p)).map_err(|e| e.to_string())?;
        let res = m.get_values(&[other], &[other]);
        if !matches!(&res, Err(Error::Backend { message, .. }) if message.contains("Only local values"))
        {
            return Err(format!("get_values of another rank's row: {res:?}"));
        }
    }
    println!(
        "INFO ua01 rank {r}: {} rows and {} columns refused in Rust; matrix intact",
        rows.len(),
        rows.len()
    );
    drop(m);
    drop(session);
    Ok(())
}

/// `Ksp::solve` refuses vectors of another layout than the matrix's
/// before PETSc runs; a matching layout still solves.
pub fn ua03_ksp_layout(c: &Communicator) -> Outcome {
    let session = petsc::Session::acquire(c).map_err(e2s)?;
    let (r, p) = (c.rank() as usize, c.size());
    let n = 2usize;
    let size = n * p;
    let first = i32::try_from(n * r).map_err(|e| e.to_string())?;
    let (a, mut ksp, b_wide, mut x_wide, mut b, mut x) = c
        .native_collective("ua03:create", Ok(()), |nc| {
            let mut a = petsc::Matrix::create(&nc, c, &session, "ua03-a", n, size, true)?;
            for k in 0..2 {
                a.add_row(first + k, &[first + k], &[2.0])?;
            }
            a.assemble(&nc)?;
            let ksp = petsc::Ksp::create(&nc, c, &session, "ua03-ksp")?;
            let wide = |name| petsc::Vector::create(&nc, c, &session, name, n + 1, size + p);
            let (b_wide, x_wide) = (wide("ua03-bw")?, wide("ua03-xw")?);
            let b = petsc::Vector::create(&nc, c, &session, "ua03-b", n, size)?;
            let x = petsc::Vector::create(&nc, c, &session, "ua03-x", n, size)?;
            akantu_core::Result::Ok((a, ksp, b_wide, x_wide, b, x))
        })
        .map_err(e2s)?
        .map_err(e2s)?;
    let refused = c
        .native_collective("ua03:refused", Ok(()), |nc| {
            ksp.solve(&nc, &a, &b_wide, &mut x_wide)
        })
        .map_err(e2s)?;
    if !matches!(refused, Err(Error::DimensionMismatch { .. })) {
        return Err(format!(
            "a vector layout of {} owned rows / {} accepted for a matrix of {n} / {size}: {refused:?}",
            n + 1,
            size + p
        ));
    }
    b.set_owned(&vec![2.0; n]).map_err(e2s)?;
    let o = c
        .native_collective("ua03:solve", Ok(()), |nc| ksp.solve(&nc, &a, &b, &mut x))
        .map_err(e2s)?
        .map_err(e2s)?;
    let mut xs = vec![0.0; n];
    x.get_owned(&mut xs).map_err(e2s)?;
    if xs.iter().any(|v| (v - 1.0).abs() > 1e-12) {
        return Err(format!("matching layout: x = {xs:?} (reason {})", o.reason));
    }
    println!(
        "INFO ua03 rank {r}: mismatched layout refused; matching layout solved (reason {})",
        o.reason
    );
    drop((x, b, x_wide, b_wide, ksp, a));
    drop(session);
    Ok(())
}

/// The AO sweep: an application index outside `0..N` on the last rank only
/// is refused on every rank before `AOCreateMemoryScalable` runs; a valid
/// map (the natural numbering) works afterwards.
pub fn ao_map_range(c: &Communicator) -> Outcome {
    let session = petsc::Session::acquire(c).map_err(e2s)?;
    let (r, p) = (c.rank() as usize, c.size());
    let n = 2usize;
    let first = i32::try_from(n * r).map_err(|e| e.to_string())?;
    let total = i32::try_from(n * p).map_err(|e| e.to_string())?;
    let mut owned: Vec<i32> = (first..first + 2).collect();
    if r == p - 1 {
        owned[1] = total + 5;
    }
    let mut idx = owned.clone();
    let bad = c
        .native_collective("ao:bad", Ok(()), |nc| petsc::ao_map(&nc, &owned, &mut idx))
        .map_err(e2s)?;
    if !matches!(&bad, Err(Error::Backend { message, .. }) if message.contains("application index"))
    {
        return Err(format!(
            "out-of-range application index on rank {}: {bad:?}",
            p - 1
        ));
    }
    let owned: Vec<i32> = (first..first + 2).collect();
    let mut idx = vec![first + 1, first, total, -1];
    c.native_collective("ao:good", Ok(()), |nc| petsc::ao_map(&nc, &owned, &mut idx))
        .map_err(e2s)?
        .map_err(e2s)?;
    if idx != [first + 1, first, -1, -1] {
        return Err(format!("natural AO mapped to {idx:?}"));
    }
    println!("INFO ao_map rank {r}: out-of-range application index refused on every rank");
    drop(session);
    Ok(())
}
