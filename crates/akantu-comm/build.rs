// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs build script (no upstream counterpart): with the `mpi` feature,
// compiles the small C shim `src/mpi_shim.c` (predefined MPI handles that
// are macros in mpi.h: MPI_DOUBLE_INT, MPI_MINLOC, MPI_MAXLOC) with the MPI
// compiler wrapper (`$MPICC`, default `mpicc`). Without the feature it does
// nothing (the default build needs no MPI and no C compiler).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "mpi")]
    mpi_shim();
}

#[cfg(feature = "mpi")]
fn mpi_shim() {
    println!("cargo:rerun-if-changed=src/mpi_shim.c");
    println!("cargo:rerun-if-env-changed=MPICC");
    let mpicc = std::env::var("MPICC").unwrap_or_else(|_| "mpicc".to_owned());
    cc::Build::new()
        .compiler(mpicc)
        .file("src/mpi_shim.c")
        .warnings(true)
        .compile("akantu_comm_mpi_shim");
}
