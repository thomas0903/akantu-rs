// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs build script (no upstream counterpart): with the `scotch`
// feature, compiles the C shim `src/scotch_shim.c` and links Scotch
// (`SCOTCH_INCLUDE_DIR`, default /usr/include/scotch; Docker image
// docker/scotch). Without the feature it does nothing.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "scotch")]
    scotch_shim();
}

#[cfg(feature = "scotch")]
fn scotch_shim() {
    println!("cargo:rerun-if-changed=src/scotch_shim.c");
    println!("cargo:rerun-if-env-changed=SCOTCH_INCLUDE_DIR");
    let inc =
        std::env::var("SCOTCH_INCLUDE_DIR").unwrap_or_else(|_| "/usr/include/scotch".to_owned());
    cc::Build::new()
        .file("src/scotch_shim.c")
        .include(inc)
        .warnings(true)
        .compile("akantu_mesh_scotch_shim");
    println!("cargo:rustc-link-lib=scotch");
    println!("cargo:rustc-link-lib=scotcherr");
}
