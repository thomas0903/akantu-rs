// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs build script. Upstream counterparts: cmake/Modules/FindPETSc.cmake
// and packages/petsc.cmake of Akantu (EPFL/LSMS) for the PETSc part; the
// MUMPS part has none. One script, one native shim per feature:
//
// * feature `mumps`: compiles the C shim `src/mumps/shim.c`
//   against the installed MUMPS headers (`dmumps_c.h`) and links the
//   parallel double-precision MUMPS library (`-ldmumps`; its closure —
//   mumps_common, ScaLAPACK, BLAS/LAPACK, orderings, MPI — comes from the
//   shared object's own dependencies). Overrides: AKANTU_MUMPS_INCLUDE_DIR,
//   AKANTU_MUMPS_LIB_DIR (non-Debian layouts), AKANTU_MUMPS_LIB (library
//   name, default `dmumps`), MPICC (C compiler, default `mpicc`).
// * feature `petsc`: locates PETSc with pkg-config (`PETSc`,
//   then `petsc`) and compiles the C shim `src/petsc/shim.c` with the MPI
//   compiler wrapper (`$MPICC`, default `mpicc`).
//
// Without either feature it does nothing: the default build needs no native
// library and no C compiler.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "mumps")]
    mumps_shim();
    #[cfg(feature = "petsc")]
    petsc_shim();
}

#[cfg(feature = "mumps")]
fn mumps_shim() {
    println!("cargo:rerun-if-changed=src/mumps/shim.c");
    for v in [
        "AKANTU_MUMPS_INCLUDE_DIR",
        "AKANTU_MUMPS_LIB_DIR",
        "AKANTU_MUMPS_LIB",
        "MPICC",
    ] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    let cc = std::env::var("MPICC").unwrap_or_else(|_| "mpicc".to_owned());
    let mut build = cc::Build::new();
    build.compiler(cc).file("src/mumps/shim.c").warnings(true);
    if let Ok(dir) = std::env::var("AKANTU_MUMPS_INCLUDE_DIR") {
        build.include(dir);
    }
    build.compile("akantu_linalg_mumps_shim");
    if let Ok(dir) = std::env::var("AKANTU_MUMPS_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
    let lib = std::env::var("AKANTU_MUMPS_LIB").unwrap_or_else(|_| "dmumps".to_owned());
    println!("cargo:rustc-link-lib=dylib={lib}");
}

#[cfg(feature = "petsc")]
fn petsc_shim() {
    println!("cargo:rerun-if-changed=src/petsc/shim.c");
    println!("cargo:rerun-if-env-changed=MPICC");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");
    let lib = pkg_config::Config::new()
        .probe("PETSc")
        .or_else(|_| pkg_config::Config::new().probe("petsc"))
        .unwrap_or_else(|e| {
            panic!(
                "feature `petsc`: PETSc not found with pkg-config (PETSc.pc / petsc.pc): {e}; \
                 build inside the akantu-rs-petsc image (docker/petsc) or set PKG_CONFIG_PATH"
            )
        });
    let mpicc = std::env::var("MPICC").unwrap_or_else(|_| "mpicc".to_owned());
    let mut build = cc::Build::new();
    build
        .compiler(mpicc)
        .file("src/petsc/shim.c")
        .warnings(true)
        // No fast-math or contraction in the shim (AGENTS.md, "Rules for numerical code"); the shim
        // only copies values, but keep the contract explicit.
        .flag_if_supported("-ffp-contract=off");
    for inc in &lib.include_paths {
        build.include(inc);
    }
    build.compile("akantu_linalg_petsc_shim");
    println!("cargo:rustc-env=AKANTU_PETSC_PKG_VERSION={}", lib.version);
}
