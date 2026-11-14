// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs C ABI stub (no upstream counterpart: upstream has no C API).

//! # akantu-capi
//!
//! Placeholder for a C ABI of akantu-rs. It exports one function,
//! [`akantu_version`], which returns the library version as a static,
//! NUL-terminated string. A C API (handles to meshes and models, error
//! codes, a C header) is not part of this release: upstream Akantu has no C
//! API either, so no parity row depends on it.
//!
//! The crate builds as `cdylib`, `staticlib` and `rlib`.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::ffi::{CStr, c_char};

static VERSION: &CStr = c"0.1.0";

/// Version string of the library (static, never freed).
// Exporting an unmangled symbol is flagged by `unsafe_code`; the function
// itself performs no unsafe operation and returns a pointer to a 'static,
// NUL-terminated string.
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
pub extern "C" fn akantu_version() -> *const c_char {
    VERSION.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_crate() {
        assert_eq!(VERSION.to_str().unwrap(), env!("CARGO_PKG_VERSION"));
        assert!(!akantu_version().is_null());
    }
}
