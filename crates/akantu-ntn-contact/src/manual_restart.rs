// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/common/{manual_restart.hh, manual_restart.cc}.

//! Manual displacement restart of the package (upstream `dumpArray`,
//! `loadArray`, `dumpRestart`, `loadRestart`; TASN-040).
//!
//! File format: the number of tuples, the number of components (one per
//! line), then one tuple per line, components separated by one space, in C++
//! `std::scientific` with precision 9 (`%.9e`). Only the displacement is
//! saved and restored (no velocity, acceleration or material history).
//!
//! **UD-402 (A):** upstream `loadArray` only checks the header against the
//! array with debug assertions and then resizes the array to the file's size;
//! a short file leaves the remaining values untouched (failed extractions).
//! The port refuses a header that differs from the array and a file with
//! fewer values than announced.
//!
//! The rank-0 gather/scatter variants (`dumpRestart(model, file, prank)`,
//! `loadRestart(model, file, prank)`) are serial here: on one rank they are
//! the plain variants; a mesh distributed over several ranks is refused.

use std::path::Path;

use akantu_core::{Array, Error, Real, Result};
use akantu_parser::stream;
use akantu_solid::SolidMechanicsModel;

use crate::sync_array::io_error;

/// Upstream `dumpArray(array, fname)` content.
#[must_use]
pub fn array_text(array: &Array<Real>) -> String {
    let nbc = array.nb_component();
    let mut out = format!("{}\n{}\n", array.len(), nbc);
    for t in array.tuples() {
        for (c, v) in t.iter().enumerate() {
            if c != 0 {
                out.push(' ');
            }
            out.push_str(&stream::format_scientific(*v, 9));
        }
        out.push('\n');
    }
    out
}

/// Upstream `dumpArray(array, fname)`.
///
/// # Errors
/// I/O failure.
pub fn dump_array(array: &Array<Real>, fname: impl AsRef<Path>) -> Result<()> {
    let path = fname.as_ref();
    std::fs::write(path, array_text(array)).map_err(|e| io_error(path, &e))
}

/// Parse [`array_text`] content into `array` (see [`load_array`]).
///
/// # Errors
/// Header different from the array, or missing/malformed values (UD-402).
pub fn load_array_text(array: &mut Array<Real>, text: &[u8]) -> Result<()> {
    let size = stream::extract_int::<i64>(text, 0);
    let mut pos = size.consumed;
    let comp = stream::extract_int::<i64>(text, pos);
    pos += comp.consumed;
    let (Some(size), Some(comp)) = (size.value, comp.value) else {
        return Err(Error::invalid(
            "loadArray: cannot read the size and the number of components",
        ));
    };
    if usize::try_from(comp).ok() != Some(array.nb_component()) {
        return Err(Error::invalid(format!(
            "loadArray: BAD NUM OF COMPONENTS ({comp} in the file, {} in the array)",
            array.nb_component()
        )));
    }
    if usize::try_from(size).ok() != Some(array.len()) {
        return Err(Error::invalid(format!(
            "loadArray: number of data points in file ({size}) does not correspond to array size ({})!!",
            array.len()
        )));
    }
    for v in array.as_mut_slice() {
        let r = stream::extract_double(text, pos);
        pos += r.consumed;
        match (r.fail, r.value) {
            (false, Some(x)) => *v = x,
            _ => {
                return Err(Error::invalid(
                    "loadArray: the file holds fewer values than announced (or a malformed value)",
                ));
            }
        }
    }
    Ok(())
}

/// Upstream `loadArray(array, fname)` (UD-402).
///
/// # Errors
/// Unreadable file, or see [`load_array_text`].
pub fn load_array(array: &mut Array<Real>, fname: impl AsRef<Path>) -> Result<()> {
    let path = fname.as_ref();
    let text = std::fs::read(path).map_err(|e| io_error(path, &e))?;
    load_array_text(array, &text)
}

fn check_serial(model: &SolidMechanicsModel, what: &str) -> Result<()> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = mesh_h.read()?;
    if mesh.communicator().size() > 1 {
        return Err(Error::Unsupported(format!(
            "{what} with the rank-0 gather/scatter on several ranks (serial port)"
        )));
    }
    Ok(())
}

/// Upstream `dumpRestart(model, fname)`: the displacement.
///
/// # Errors
/// Displacement not allocated or I/O failure.
pub fn dump_restart(model: &SolidMechanicsModel, fname: impl AsRef<Path>) -> Result<()> {
    dump_array(&*model.displacement()?, fname)
}

/// Upstream `loadRestart(model, fname)`: the displacement.
///
/// # Errors
/// Displacement not allocated, or see [`load_array`].
pub fn load_restart(model: &mut SolidMechanicsModel, fname: impl AsRef<Path>) -> Result<()> {
    // `load_array` fills a plain array in place: through a copy of the
    // (exportable) displacement, copied back also on error (the same
    // partial writes as loading into the array itself)
    let mut u = model.displacement_mut()?;
    let mut plain = Array::clone(&u);
    let r = load_array(&mut plain, fname);
    u.copy_from(&plain)?;
    r
}

/// Upstream `dumpRestart(model, fname, prank)` (gather to rank 0; serial).
///
/// # Errors
/// Several ranks, or see [`dump_restart`].
pub fn dump_restart_gathered(
    model: &SolidMechanicsModel,
    fname: impl AsRef<Path>,
    prank: usize,
) -> Result<()> {
    check_serial(model, "dumpRestart")?;
    if prank == 0 {
        dump_restart(model, fname)
    } else {
        Ok(())
    }
}

/// Upstream `loadRestart(model, fname, prank)` (scatter from rank 0; serial).
///
/// # Errors
/// Several ranks, or see [`load_restart`].
pub fn load_restart_scattered(
    model: &mut SolidMechanicsModel,
    fname: impl AsRef<Path>,
    prank: usize,
) -> Result<()> {
    check_serial(model, "loadRestart")?;
    if prank == 0 {
        load_restart(model, fname)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_format_and_round_trip() {
        let a = Array::from_vec(vec![1.0, -2.5e-12, 1.0 / 3.0, 6.02214076e23], 2).unwrap();
        let t = array_text(&a);
        assert_eq!(
            t,
            "2\n2\n1.000000000e+00 -2.500000000e-12\n3.333333333e-01 6.022140760e+23\n"
        );
        let mut b = Array::zeros(2, 2);
        load_array_text(&mut b, t.as_bytes()).unwrap();
        assert_eq!(b.as_slice(), &[1.0, -2.5e-12, 0.3333333333, 6.02214076e23]);
        let mut c = Array::zeros(3, 2);
        assert!(load_array_text(&mut c, t.as_bytes()).is_err());
        let mut d = Array::zeros(2, 3);
        assert!(load_array_text(&mut d, t.as_bytes()).is_err());
        assert!(load_array_text(&mut b, b"2\n2\n1 2 3\n").is_err());
    }
}
