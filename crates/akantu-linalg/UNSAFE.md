# `unsafe` sites of `akantu-linalg`

Every `unsafe` site of this crate is listed below, one row per site
(`scripts/unsafe_inventory.py --check`, run by `cargo test -p
akantu-validation --test unsafe_inventory`, fails when a site is missing,
moved or gone; `--update-lines` rewrites the Line column after an edit
that only moves code).
Each row is also pinned to a fingerprint of its site's code (comments and
whitespace ignored) in `UNSAFE.fingerprints`: a site
whose code changed fails the check, and `--update-lines` does not move its
row, until the row is reviewed and re-pinned with `--refresh <Id>`.
A fingerprint covers the Rust site's own tokens only: not the code around
it that sets up its invariants, and not C sources (here `src/mumps/shim.c`, `src/petsc/shim.c`), which the guard
does not read; a change there shows only in the commit's diff and its
review.

**Where `unsafe` is allowed.** `src/lib.rs`: `#![deny(unsafe_code)]`.
The opt-outs: the module `eigen_sparse_lu::simd`
(`#[allow(unsafe_code)] mod simd;` in `src/eigen_sparse_lu/mod.rs`,
every build: the run-time instruction-set dispatch of the SparseLU dense
kernels, class L6), `src/mumps/mod.rs` (`#![allow(unsafe_code)]`, feature
`mumps`) and the module `petsc` (`#[allow(unsafe_code)] pub mod petsc;` in `lib.rs`,
feature `petsc`). The C sides of the FFI edges are `src/mumps/shim.c`
(the opaque `DMUMPS_STRUC_C`; `_Static_assert` of 32-bit `MUMPS_INT` and
64-bit `MUMPS_INT8`) and `src/petsc/shim.c` (plain C types only;
`#error` for complex scalars, 64-bit `PetscInt`, PETSc < 3.17; every
`int64_t` → `PetscInt` narrowing range-checked by `aka_int`). Both shims are
compiled by `build.rs`. This audit compared every `extern` declaration of
the two blocks below with its shim prototype (names, argument order, C
types): no mismatch.

**Counts** (library / tests): block 74 / 4, `unsafe fn` 4 / 4, `unsafe impl`
6 / 2, `extern` block 2 / 0 — **86 library sites, 10 test sites** (6
library blocks in the default build: `src/eigen_sparse_lu/simd.rs`; the
others need `mumps` or `petsc`).

## Invariant classes

* **L1 — MUMPS instance accessor.** `Instance::ptr` comes from
  `akantu_mumps_alloc` (zeroed `calloc`, null checked), is owned by exactly
  one `Instance` (private type, no `Clone`), freed exactly once (`Drop`:
  `akantu_mumps_free` directly when never initialised, else after the
  collective `JOB = -2` queued through `Communicator::release_native`) or
  leaked (`abandon`, broken communicator). Index arguments are 1-based and
  must lie in `1..=60` (ICNTL), `1..=15` (CNTL), `1..=80` (INFOG),
  `1..=40` (RINFOG): constants at every internal call, and the user's
  ICNTL/CNTL keys are checked by `MumpsSolver::validate_options` in
  `preflight`, inside the same `&mut self` borrow as `apply_controls`
  (`options_mut` cannot run in between). The
  accessors also check the range themselves with an always-on `assert!`
  (a panic on an internal bug instead of an out-of-bounds write; never
  reachable from user input, which `validate_options` refuses with an
  error). `validate_options` also refuses every user ICNTL value that would
  make MUMPS read or write a user array the binding never provides
  (`array_backed_icntl`: ICNTL(7) = 1, (8) = −1, (15) > 0, (19),
  (26), (30), (32) ≠ 0, (25) < 0; MUMPS 5.7.3 user's guide §6.1).
* **L2 — MUMPS job inside an agreed native collective.** The job runs in
  `Communicator::native_collective` (job identity agreed in the native
  header; thread admission of the process registry); the agreed
  plan checked that every rank's instance is the same native instance bound
  to the solve's communicator (instance token, `Communicator::is_same`);
  the matrix pointers are set only after the agreed local validation
  (`Coordinates::check`: `nnz` coordinates and values, order within
  `MUMPS_INT`) and cleared right after the job on every path; the host's
  right-hand side has exactly `n` entries (checked, agreed) and its pointer
  is cleared after the job.
* **L3 — PETSc object method.** The handle comes from a shim constructor,
  non-null checked (`Native::new`), owned by one `Native` (no `Clone`;
  `Matrix::new_reference` takes a PETSc reference that its own `Native`
  releases), destroyed exactly once through `release_native` (collective,
  SPMD order, under the PETSc lock, before `PetscFinalize`, which waits for
  the last `Session` clone held by every object). Every call holds the
  process-wide PETSc lock (`PetscLock::try_enter` for local calls,
  `enter_native` inside an agreed native entry; lock order MPI → PETSc);
  collective methods check that the `NativeComm` capability is
  the object's own communicator (`check_capability`) and the
  capability cannot outlive its entry (`for<'n>` closure). Buffers are Rust
  slices valid for the call; lengths are checked on the Rust side and again
  by the shim against the PETSc local size (`vec_set_owned`,
  `vec_get_owned`, `mat_mult`). **Indices** passed to `MatSetValues` /
  `MatGetValues` are range-checked in Rust (`check_index`: `0 ≤ k < size`);
  the application indices of `ao_map` are
  range-checked by the shim on every rank and the verdict agreed before
  `AOCreateMemoryScalable` (which writes out of bounds for an out-of-range
  one); the layouts of the operands of `Ksp::solve`, `Snes::solve`,
  `Tao::solve` and `Vector::dot` (owned and global sizes) are checked in
  Rust (`check_layout`); `create` checks `n_owned ≤ size`, the
  cross-rank sum is refused by PETSc (`PetscLayoutSetUp`, always on).
* **L4 — callback trampoline.** C (the shim's static PETSc callbacks)
  calls these during `SNESSolve` / `TaoSolve`: `user` points to the
  `UserData` fat reference on the solving Rust frame, alive for the whole
  solve (the shim detaches its routines and the caller's history buffers
  before returning) and used by one callback at a time on that
  thread; `x`, `f`, `grad` are `VecGetArray(Read)` arrays of distinct PETSc
  vectors whose local size is `n` (the owned rows Rust passed; the solution
  and residual vectors were created by Rust with that local size and PETSc's
  work vectors duplicate them; the shim checks the local size of every
  vector it passes, `aka_check_local`);
  panics are caught (`catch_unwind`), never unwinding into C.
* **L5 — argument-free shim query.** `sizeof` constants, version macros,
  `dlsym` lookups of OpenBLAS's getters returning a static string or null.
* **L6 — call of a `#[target_feature]` kernel after the CPU check.** The
  callee is safe Rust (bounds-checked slices, no raw pointers) compiled
  with AVX or AVX-512F enabled; its only precondition is that the CPU
  supports that feature, which `is_x86_feature_detected!` checked in the
  guard of the same `match` arm (or on the lines just before). The `fma`
  feature is enabled only for the explicit fast mode
  (`gemm_into_zero_fma`, off by default); the default kernels keep
  separate multiplies and adds (bitwise the portable instance; `dense::tests` compare every instance the machine runs with
  the reference kernels on random inputs with signed zeros, infinities,
  NaNs and subnormals).

## Site table

### src/eigen_sparse_lu/simd.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-81 | 115 | `fn gemm_into_zero_with` block 1 | calls `x86::gemm_avx512` (`sparselu_gemm`, 16-row tiles) | the CPU supports AVX-512F | L6 (`is_x86_feature_detected!("avx512f")` in the arm's guard) | `dense::tests::gemm_matches_reference_bitwise`, `eigen_sparse_lu_oracle` |
| LA-82 | 122 | `fn gemm_into_zero_with` block 2 | calls `x86::gemm_avx` (8-row tiles) | the CPU supports AVX | L6 (`is_x86_feature_detected!("avx")`) | `dense::tests::gemm_matches_reference_bitwise` |
| LA-85 | 156 | `fn gemm_into_zero_fma` block 1 | calls `x86::gemm_avx512_fma` (fast mode, opt-in) | the CPU supports AVX-512F and FMA | L6 (both detected on the lines before) | `tests::fma_mode_solution_within_tolerance` |
| LA-86 | 162 | `fn gemm_into_zero_fma` block 2 | calls `x86::gemm_avx2_fma` (fast mode, opt-in) | the CPU supports AVX2 and FMA | L6 | `tests::fma_mode_solution_within_tolerance` |
| LA-83 | 232 | `fn gemv_with` block 1 | calls `x86::gemv_avx512` (column-major GEMV, 32-row tiles) | the CPU supports AVX-512F | L6 | `dense::tests::gemv_matches_reference_bitwise`, `eigen_sparse_lu_oracle` |
| LA-84 | 237 | `fn gemv_with` block 2 | calls `x86::gemv_avx` (16-row tiles) | the CPU supports AVX | L6 | `dense::tests::gemv_matches_reference_bitwise` |

### src/mumps/mod.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-01 | 172 | `(crate root)` extern 1 [akantu_mumps_alloc, akantu_mumps_free, akantu_mumps_set_init, akantu_mumps_job, akantu_mumps_set_icntl, akantu_mumps_get_icntl, akantu_mumps_set_cntl, akantu_mumps_get_cntl, akantu_mumps_infog, akantu_mumps_rinfog, akantu_mumps_set_matrix, akantu_mumps_clear_matrix, akantu_mumps_set_rhs, akantu_mumps_version, akantu_mumps_int_bytes, akantu_mumps_blas_corename, akantu_mumps_blas_threads] | declares the MUMPS shim | `akantu_mumps *` as `*mut c_void`, `int` = `c_int`, `int64_t` = `i64`, `double` = `f64`; matches `src/mumps/shim.c` (checked by this audit) | declaration; the shim's `_Static_assert`s pin `MUMPS_INT` = `int`, `MUMPS_INT8` = `int64_t` | MUMPS tier (`mumps_mpi`, `mumps_mpi_solver`, `review_v54_regressions` / `review_v66_regressions`) |
| LA-02 | 218 | `fn blas_info` block 1 | OpenBLAS core name and thread count (D-050 provenance) | shim functions without preconditions; the core name is a static NUL-terminated string (or null), copied out | L5 | `mumps_mpi` `blas_pin` |
| LA-03 | 435 | `impl Send for Instance` impl 1 | a MUMPS instance may move to another thread with its solver | plain heap memory owned by one `MumpsSolver`; MUMPS is not thread-bound; which thread enters MPI through it is decided at every job (`native_collective`) and destruction (lifecycle); `Communicator: Send` | L1; not `Sync` | `review_v66_regressions` `mu2_05_concurrent_solvers` |
| LA-04 | 445 | `impl Instance › fn alloc` block 1 | `akantu_mumps_alloc()` | `calloc`, no precondition; null checked | L1 | every MUMPS solve |
| LA-05 | 483 | `impl Instance › fn job` block 1 | `akantu_mumps_job(ptr, job)` (`dmumps_c`) | live instance; fields read by the job set by the caller (init fields; matrix pointers valid during JOB 1/2; RHS during JOB 3); every rank runs the same job (agreed plan, native header) | L1 + L2 | `mumps_mpi` (all scenarios), `review_v54_regressions`, `review_v66_regressions` |
| LA-06 | 496 | `impl Instance › fn set_icntl` block 1 | `icntl[i-1] = v` | live instance; `i` in `1..=60` | L1: `assert!` in the accessor (always on, UA-06) + `validate_options` (user keys) / constants | `mumps_mpi` (options scenarios), unit test `validate_options` cases |
| LA-07 | 505 | `impl Instance › fn icntl` block 1 | reads `icntl[i-1]` | as LA-06 | L1: always-on `assert!` (UA-06); constant indices | `mumps_mpi` (report) |
| LA-08 | 515 | `impl Instance › fn set_cntl` block 1 | `cntl[i-1] = v` | live instance; `i` in `1..=15` (validated) | L1: always-on `assert!` (UA-06) + `validate_options` | unit tests of `validate_options` |
| LA-09 | 525 | `impl Instance › fn cntl` block 1 | reads `cntl[i-1]` (diagnostics, unused) | as LA-08 | L1: always-on `assert!` (UA-06); constant indices | — (dead code, `#[allow(dead_code)]`) |
| LA-10 | 534 | `impl Instance › fn infog` block 1 | reads `infog[i-1]` | live instance; `i` in `1..=80` | L1: always-on `assert!` (UA-06); constant indices | every MUMPS solve (agreed outcomes) |
| LA-11 | 543 | `impl Instance › fn rinfog` block 1 | reads `rinfog[i-1]` | live instance; `i` in `1..=40` | L1: always-on `assert!` (UA-06); constant indices | `mumps_mpi` (report) |
| LA-12 | 581 | `impl Instance › fn set_matrix` block 1 | stores `n`, `nnz`, `irn`, `jcn`, `a` pointers in the structure | `irn`/`jcn` point to the `nnz` 1-based coordinates, `av` to `nnz` values (or null when empty/unused); lengths equal by the caller's agreed check (asserted); buffers kept alive and unmodified until `clear_matrix` right after the one job that reads them | L2 (`matrix_job` sets them only after `Coordinates::check` succeeded, clears them after the job on every path); the equal lengths are `assert!`ed here, always on (UA-06) | `mumps_mpi`, `review_v66_regressions` `mu2_01_*` |
| LA-13 | 597 | `impl Instance › fn clear_matrix` block 1 | resets the matrix pointers to null | live instance | L1 | as LA-12 |
| LA-14 | 605 | `impl Instance › fn version` block 1 | copies `version_number` out | live instance; the array is NUL-terminated (zeroed by `calloc`, filled by JOB −1) | L1 + MUMPS's structure layout (shim) | `mumps_mpi` (report) |
| LA-15 | 635 | `impl Drop for Instance › fn drop` block 1 | `akantu_mumps_free(ptr)` of a never-initialised (or destroyed) instance | allocated by `akantu_mumps_alloc`, freed exactly once (`abandon` forgets instead) | L1 | every MUMPS solve (re-initialisation), unit tests |
| LA-16 | 662 | `impl Drop for Instance › fn drop` block 2 | the queued destruction closure: JOB −2, then free | initialised, uniquely owned by the closure; run by the lifecycle on a thread allowed to call MPI, before `MPI_Finalize`; every rank queues at the same program point (documented obligation of dropping a solver) | L1 + `Communicator::release_native` (lifecycle queue); the SPMD drop order is the **caller's** obligation | `review_v54_regressions` (drop scenarios), `review_v66_regressions` `mu2_04_*` |
| LA-17 | 684 | `impl Send for QueuedInstance` impl 1 | the raw pointer moved into the destruction closure | as `Instance`: unique ownership moves into the closure; run on an MPI-capable thread before finalisation | L1 | as LA-16 |
| LA-18 | 909 | `impl MumpsSolver › fn mumps_int_bytes` block 1 | `sizeof(MUMPS_INT)` | shim constant | L5 | `mumps_mpi` `int_width` |
| LA-19 | 1293 | `impl MumpsSolver › fn initialize` block 1 | `akantu_mumps_set_init(ptr, comm_fortran, sym, par)` | live instance; plain integers (the Fortran handle of the native entry's communicator) | L1 + L2 (inside `native_collective` `mumps:init`) | every MUMPS solve |
| LA-20 | 1563 | `impl MumpsSolver › fn solve_job` block 1 | `akantu_mumps_set_rhs(ptr, p, n)` | `p` null (non-host ranks, or a failed check: the job is then refused on every rank) or the host's `n` writable doubles (length checked, alive and not otherwise borrowed until cleared) | L2 | `mumps_mpi`, `review_v66_regressions` `mu2_01_*` |
| LA-21 | 1568 | `impl MumpsSolver › fn solve_job` block 2 | clears the RHS pointer after the job | live instance; whatever the outcome | L1 | as LA-20 |

### src/petsc/callbacks.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-22 | 65 | `fn with_user` fn 1 | private `unsafe fn`: rebuilds `&mut dyn NativeCallbacks` from `user` | `# Safety`: `user` points to a live `UserData` used by no one else during the call; `x`/`out` point to `n` valid `f64`s for the call | the three trampolines (L4) | `petsc_mpi` `partitioned_bar_native`, `petsc_snes_rank_specific_failure`, `petsc_pftao` |
| LA-23 | 76 | `fn with_user` block 1 | `&mut *user.cast::<UserData>()` | `user` is a unique pointer to a live `UserData` (contract) | L4; null checked first | as LA-22 |
| LA-24 | 81 | `fn residual` fn 1 | `unsafe extern "C"` SNES residual trampoline | callers: the shim's `aka_snes_function` only | L4; the shim checks the local size of every vector it passes (`aka_check_local`, UA-08) | `partitioned_bar_native`, `petsc_snes_rank_specific_failure` |
| LA-25 | 92 | `fn residual` block 1 | slices `x` (`n`, read) and `f` (`n`, write) of PETSc's arrays, then the callback | the shim passes the `user` of `akantu_petsc_snes_solve` and PETSc's owned arrays of distinct vectors (length `n`) for the call | L4; nulls checked | as LA-24 |
| LA-26 | 108 | `fn jacobian` fn 1 | SNES Jacobian / TAO Hessian trampoline | as LA-24 | L4; the shim checks the local size of every vector it passes (`aka_check_local`, UA-08) | as LA-24, `petsc_pftao` |
| LA-27 | 112 | `fn jacobian` block 1 | slice `x` (`n`), then the callback | as LA-25 | L4 | as LA-26 |
| LA-28 | 125 | `fn objective` fn 1 | TAO objective / gradient trampoline | as LA-24 | L4; the shim checks the local size of every vector it passes (`aka_check_local`, UA-08) | `petsc_pftao` |
| LA-29 | 136 | `fn objective` block 1 | slices `x`, `grad` (`n`), writes `*obj` | the solve's `user`, PETSc's arrays of distinct vectors, the shim's local `obj` | L4; nulls checked | `petsc_pftao` |

### src/petsc/ffi.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-30 | 121 | `(crate root)` extern 1 [akantu_petsc_last_error, akantu_petsc_clear_error, akantu_petsc_error_message, akantu_petsc_int_size, akantu_petsc_scalar_size, akantu_petsc_real_size, akantu_petsc_version, akantu_petsc_is_initialized, akantu_petsc_world_compare, akantu_petsc_blas_corename, akantu_petsc_blas_threads, akantu_petsc_initialize, akantu_petsc_finalize, akantu_petsc_options_create, akantu_petsc_options_destroy, akantu_petsc_options_set, akantu_petsc_ao_map, akantu_petsc_mat_create, akantu_petsc_mat_set_unsymmetric, akantu_petsc_mat_destroy, akantu_petsc_mat_reference, akantu_petsc_mat_zero_entries, akantu_petsc_mat_reset_preallocation, akantu_petsc_mat_add_row, akantu_petsc_mat_assemble, akantu_petsc_mat_ownership, akantu_petsc_mat_get_values, akantu_petsc_mat_nonzeros, akantu_petsc_mat_mult, akantu_petsc_mat_save_matrix_market, akantu_petsc_vec_create, akantu_petsc_vec_destroy, akantu_petsc_vec_set_owned, akantu_petsc_vec_get_owned, akantu_petsc_vec_zero, akantu_petsc_vec_dot, akantu_petsc_ksp_create, akantu_petsc_ksp_destroy, akantu_petsc_ksp_reset, akantu_petsc_ksp_set_from_options, akantu_petsc_ksp_solve, akantu_petsc_ksp_reason_string, akantu_petsc_ksp_tolerances, akantu_petsc_ksp_types, akantu_petsc_snes_create, akantu_petsc_snes_destroy, akantu_petsc_snes_set_from_options, akantu_petsc_snes_solve, akantu_petsc_tao_create, akantu_petsc_tao_destroy, akantu_petsc_tao_set_from_options, akantu_petsc_tao_solve] | declares the PETSc shim (`pub mod ffi`: public, callable only through `unsafe`) | plain C types only; `PetscInt` = `i32` (checked at session start by `check_types`), report structs `#[repr(C)]` with the shim's field order; matches `src/petsc/shim.c` (checked by this audit, including `SnesReport` / `TaoReport` and the three callback typedefs; `akantu_petsc_last_error` takes the failing code first, prototype and declaration changed together) | declaration + `check_types` (runtime) + the shim's `#error`s | PETSc tier (`petsc_mpi` at 1/2/4 ranks × 2 levels, regression cases) |

### src/petsc/objects.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-31 | 50 | `impl Send for Ptr` impl 1 | a PETSc handle may move between threads | not thread-affine; every use serialised by the PETSc lock and, for MPI, the thread policy; moved into exactly one destruction closure | L3 | `petsc_mpi` (SERIALIZED level), `pe02_*` cases |
| LA-32 | 56 | `fn destroy` block 1 | `akantu_petsc_*_destroy(p)` by kind | `p` is the unique owned handle of that kind (or a taken reference), destroyed exactly once | L3 (`Native::drop` → `release_native` closure, under `PetscLock::enter_waiting`) | `petsc_session_restart`, `pe09_finalize_order` |
| LA-33 | 79 | `impl Send for Native` impl 1 | an owned PETSc object may move | as LA-31; `Communicator` and `Session` are `Send` | L3 | as LA-31 |
| LA-34 | 226 | `impl Options › fn new` block 1 | `PetscOptionsCreate(&p)` | one valid out-pointer; local | L3 | `petsc_options` |
| LA-35 | 242 | `impl Options › fn set` block 1 | `PetscOptionsSetValue(db, name, value)` | live database; NUL-terminated strings (PETSc copies) | L3 (`c_string` refuses interior NULs) | `petsc_options` |
| LA-36 | 267 | `fn ao_map` block 1 | `AOCreateMemoryScalable` + `AOApplicationToPetsc` (collective) | live communicator of the agreed entry; `owned_app` `no` readable, `idx` `n` writable `PetscInt`s | L3; **checked:** the shim checks every `owned_app` entry against `[0, N)` (N = Σ `n_owned`, `MPI_Allreduce`) and agrees the verdict (`MPI_MAX`) before `AOCreateMemoryScalable`, which leaves an out-of-range entry's owner uninitialised and writes through it (aomemscalable.c, PETSc 3.22.5); out-of-range `idx` entries are mapped to −1 by PETSc (defined) | `petsc_mpi` (every DOF manager), `ao_map_range` (refused on every rank when one rank's index is out of range; natural map afterwards), reproducer `ao-big` |
| LA-37 | 304 | `impl Matrix › fn create` block 1 | `MatCreate` sequence | live communicator, valid name and out-pointer | L3; `check_sizes` (`n_owned ≤ size`, UA-01 sweep); sizes range-checked by the shim (`aka_int`); the cross-rank sum by PETSc (`PetscLayoutSetUp`) | every PETSc test |
| LA-38 | 342 | `impl Matrix › fn set_unsymmetric` block 1 | `MatSetOption(MAT_SYMMETRIC, false)` | live handle | L3 | `petsc_matrix_two_families` |
| LA-39 | 353 | `impl Matrix › fn zero_entries` block 1 | `MatZeroEntries` | live handle | L3 | `petsc_profile_change` |
| LA-40 | 365 | `impl Matrix › fn reset_preallocation` block 1 | `MatResetPreallocation` | live handle | L3 | `petsc_profile_change` |
| LA-41 | 392 | `impl Matrix › fn add_row` block 1 | `MatSetValues(1 row, n cols, ADD_VALUES)` | live handle; `cols`/`vals` hold `cols.len()` entries (checked equal); `row` and every column in `0..size` (checked above), so PETSc's unchecked hash insertion of an unpreallocated matrix stays in bounds | L3 + **`check_index`:** `row` and every column in `0..size` in Rust, before any PETSc call (`Error::OutOfRange`, `Error::InvalidArgument` for a negative index). Before the fix a row ≥ the local row count of a 1-rank unpreallocated matrix made PETSc 3.22.5 write out of bounds (`MatSetValues_Seq_Hash` / `MatAssemblyEnd_Seq_Hash`) | `petsc_mpi` (assembly), `ua01_add_row_range` (rows `size`, `size + 1` — the reproducer's `row = 3` at one rank —, `size + 64`, 10⁶, −1, and the same columns: refused in Rust at 1 / 2 / 4 ranks, matrix intact), reproducer `ua_petsc_indices` under memcheck |
| LA-42 | 413 | `impl Matrix › fn assemble` block 1 | `MatAssemblyBegin/End` (collective) | live handle; every rank in the same agreed entry | L3 | every PETSc test |
| LA-43 | 424 | `impl Matrix › fn ownership` block 1 | `MatGetOwnershipRange` | live handle; two out-pointers | L3 | `petsc_matrix_two_families` |
| LA-44 | 448 | `impl Matrix › fn get_values` block 1 | `MatGetValues(rows × cols)` | live assembled matrix; `v` holds `rows × cols` doubles | L3; **`check_index` (fixed, UA-01):** rows and columns in `0..size` in Rust; the count `rows × columns` with `checked_mul`; a row of another rank is refused by PETSc (`MatGetValues_MPIAIJ`, always-on `PetscCheck`) | `test_sparse_solver_petsc`, `oracle_serial`, `ua01_add_row_range` (out-of-range indices refused in Rust; another rank's row refused by PETSc at ≥ 2 ranks) |
| LA-45 | 472 | `impl Matrix › fn nonzeros` block 1 | `MatGetInfo(MAT_GLOBAL_SUM)` (collective) | live handle; one out-pointer | L3 | `test_sparse_solver_petsc` |
| LA-46 | 493 | `impl Matrix › fn mult` block 1 | `y = A x` through the shim's work vectors | `x`/`y` hold `n_owned` = the matrix's local size (Rust check; the shim re-checks `MatGetLocalSize`) | L3 (both checks) | `petsc_pftao` (TAO objective) |
| LA-47 | 513 | `impl Matrix › fn save_matrix_market` block 1 | `MatView` to a Matrix Market file | live matrix; NUL-terminated path | L3 | `petsc_mpi` (saveMatrix scenario) |
| LA-48 | 525 | `impl Matrix › fn new_reference` block 1 | `PetscObjectReference(mat)` | live handle; the extra reference is released by the returned value's destruction | L3 | `partitioned_bar_native` (SNES holds the matrix) |
| LA-49 | 575 | `impl Vector › fn create` block 1 | `VecCreate` sequence | live communicator, name, out-pointer | L3; `check_sizes` (`n_owned ≤ size`); `aka_int`; `PetscLayoutSetUp` | every PETSc solve |
| LA-50 | 618 | `impl Vector › fn set_owned` block 1 | copies `vals` into the owned rows | local size checked by Rust and by the shim against `vals.len()` | L3 | every PETSc solve |
| LA-51 | 642 | `impl Vector › fn get_owned` block 1 | copies the owned rows out | as LA-50 | L3 | every PETSc solve |
| LA-52 | 673 | `impl Vector › fn dot` block 1 | `VecDot` (collective) | two live vectors of the same communicator (capability checked for both) and layout (checked) | L3; owned and global sizes of both vectors checked (global size added by UA-03) | `petsc_pftao` |
| LA-53 | 723 | `impl Ksp › fn create` block 1 | `KSPCreate` | live communicator, name, out-pointer | L3 | every KSP solve |
| LA-54 | 744 | `impl Ksp › fn set_from_options` block 1 | options pushed around `KSPSetFromOptions` | live KSP; `o` null or a live database | L3 | `petsc_options` |
| LA-55 | 768 | `impl Ksp › fn solve` block 1 | `KSPSetOperators`, `KSPSetUp`, `KSPSolve` | live KSP, matrix and vectors of the same communicator (capabilities checked) and layout (local and global sizes checked, UA-03); four out-pointers | L3 + **`check_layout` (fixed, UA-03):** `b` and `x` have the matrix's owned and global sizes, checked in Rust before any PETSc call | every KSP solve, `ud423_ksp_divergence`, `ua03_ksp_layout` (a layout of `n + 1` / `size + p` refused, the matching one solves) |
| LA-56 | 795 | `impl Ksp › fn solve` block 2 | `KSPGetConvergedReasonString` into a 128-byte buffer | live KSP; `buf.len()` writable characters (the shim writes at most `len − 1` + NUL) | L3 | every KSP solve |
| LA-57 | 817 | `impl Ksp › fn reset` block 1 | `KSPReset` (collective) | live KSP; the reset decision is reduced | L3 | `petsc_mpi` re-partitioning case (PE-04) |
| LA-58 | 828 | `impl Ksp › fn tolerances` block 1 | `KSPGetTolerances` | live KSP; four out-pointers | L3 | `petsc_options` |
| LA-59 | 849 | `impl Ksp › fn types` block 1 | KSP / PC type names into two 64-byte buffers | live KSP; two buffers of `len` characters | L3 | `petsc_options` |
| LA-60 | 924 | `impl Snes › fn create` block 1 | `SNESCreate` sequence | live communicator, name, out-pointer | L3 | `partitioned_bar_native` |
| LA-61 | 944 | `impl Snes › fn set_from_options` block 1 | options pushed around `SNESSetFromOptions` | live SNES; `o` null or a live database | L3 | `petsc_options` |
| LA-62 | 985 | `impl Snes › fn solve` block 1 | `akantu_petsc_snes_solve` (callbacks, history, `SNESSolve`) | live SNES, matrix and vectors of one communicator and layout (both checked: capabilities and `n_owned`); `hist`/`hist_its` `HISTORY` entries outliving the solve (detached by the shim before it returns); `user_ptr` points to `user` on this frame for the whole solve, used only by the trampolines on this thread; `rep` valid | L3 + L4; `check_layout` (owned and global sizes of `x`, `r` against the Jacobian; global sizes added by UA-03) | `partitioned_bar_native`, `petsc_snes_rank_specific_failure` |
| LA-63 | 1061 | `impl Tao › fn create` block 1 | `TaoCreate` sequence | live communicator, name, out-pointer | L3 | `petsc_pftao` |
| LA-64 | 1081 | `impl Tao › fn set_from_options` block 1 | options pushed around `TaoSetFromOptions` | live TAO; `o` null or a live database | L3 | `petsc_pftao` |
| LA-65 | 1127 | `impl Tao › fn solve` block 1 | `akantu_petsc_tao_solve` (bounds, callbacks, `TaoSolve`) | live TAO, matrix and vector of one communicator and layout (checked); `lo`/`hi` null or `n` doubles each (checked), copied into PETSc vectors; `user_ptr` as LA-62 | L3 + L4; `check_layout` (`x` against the Hessian, owned and global sizes); bounds lengths checked | `petsc_pftao` (bounded and unbounded) |

### src/petsc/session.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-66 | 203 | `fn take_message` block 1 | copies the shim's static error message into a 4096-byte buffer when it was recorded for `code` (the shim keeps the code with the message; a message of another code, left by an unread earlier failure, yields an empty string and PETSc's generic description) | `buf` has `len` characters (the shim writes `len − 1` + NUL, or only a NUL); `code` is a plain `int`; the PETSc lock is held while the shim's static error buffer is read | the buffer size; **the PETSc lock, taken by `take_message` itself with `try_enter` (fixed, UA-02):** re-entrant for the callers that hold it, so the two callers that ran without it (`set_global_option`, `Session::acquire`) now read under it too; when another thread holds the lock the static buffer is not touched and PETSc's generic description is returned | `petsc_mpi` error scenarios (`ud423_ksp_divergence`, `petsc_snes_rank_specific_failure`) |
| LA-67 | 206 | `fn take_message` block 2 | clears the shim's static error buffer | no pointer passed; the reset runs under the PETSc lock | as LA-66 (UA-02) | as LA-66 |
| LA-68 | 215 | `fn take_message` block 3 | PETSc's generic description of `code` into a 512-byte buffer | `desc` has `len` characters; `PetscErrorMessage` reads only PETSc's constant message table (no lock needed) | the buffer size | as LA-66 |
| LA-69 | 318 | `fn set_global_option` block 1 | `PetscInitialized(&flag)` | one out-pointer; local query under the (tried) PETSc lock | L3 | `petsc_options` (global options) |
| LA-70 | 359 | `fn set_global_option` block 2 | `PetscOptionsSetValue(NULL, name, value)` (global database) | null selects the global database (PETSc initialised on every rank: agreed); NUL-terminated strings; under the PETSc lock inside the agreed entry | L3 | `petsc_options` |
| LA-71 | 384 | `impl Send for Argv` impl 1 | the C `argv` of `PetscInitialize` may move | the pointers point into `_strings`, owned by the same value and never mutated by Rust; PETSc reads them only under the PETSc lock | the type (private, fields set once in `Argv::new`; `CString` heap buffers do not move) | `petsc_session_restart` |
| LA-72 | 386 | `impl Sync for Argv` impl 1 | shared `&Argv` across threads | no interior mutation through `&Argv` | as LA-71 | as LA-71 |
| LA-73 | 507 | `fn take_finalize` block 1 | `PetscFinalize` (collective over `PETSC_COMM_WORLD`) | PETSc initialised by akantu; every PETSc object destroyed before (objects hold a `Session` clone); `PETSC_COMM_WORLD` is akantu's duplicate, freed only after; under the thread policy (lifecycle) and the PETSc lock; argv alive until it returned | `Session` reference counting (`Arc`), `FINALIZE_PENDING`, the lifecycle queue (finalisation after the queued object destructions) | `petsc_session_restart`, `pe09_finalize_order` (valgrind clean) |
| LA-74 | 580 | `fn version` block 1 | PETSc version macros | three out-pointers; compile-time constants | L5 | `petsc_mpi` provenance |
| LA-75 | 612 | `fn world_relation` block 1 | `MPI_Comm_compare(PETSC_COMM_WORLD, comm)` | live communicator of the agreed entry (thread policy held); one out-pointer | L3 | `pe09_*` cases |
| LA-76 | 640 | `fn blas_info` block 1 | OpenBLAS core name / threads | as LA-02 | L5 | `petsc_mpi` `blas_pin` |
| LA-77 | 660 | `fn check_types` block 1 | `sizeof(PetscInt/PetscScalar/PetscReal)` | argument-free shim functions | L5 | every session (refuses other builds) |
| LA-78 | 746 | `impl Session › fn acquire` block 1 | `PetscInitialized(&flag)` | one out-pointer; local query under the PETSc lock | L3 | every PETSc test |
| LA-79 | 851 | `impl Session › fn acquire` block 2 | global options set before `PetscInitialize` | null = global database (writable before initialisation); NUL-terminated strings; under the PETSc lock | L3 | `petsc_options` |
| LA-80 | 869 | `impl Session › fn acquire` block 3 | `PetscInitialize(world, argc, argv)` | `nc.raw()` is akantu's duplicate `world`, kept in the library state until `PetscFinalize` returned; `ptrs` points to `argc` C strings plus a NULL terminator, kept alive in the session until finalisation | `Library` state (`world`, `argv` taken only by `take_finalize`); the agreed command-line digest (PE-13) | every PETSc test, `petsc_session_restart` |

### tests/eig3_fixed.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-T01 | 26 | `impl GlobalAlloc for Counting` impl 1 | counting global allocator of the test binary | forwards every call unchanged to `System`; adds a const-initialised thread-local counter (no allocation) | the forwarding (same layout, same pointer) | `eig3_fixed` (allocation-free kernel checks) |
| LA-T02 | 27 | `impl GlobalAlloc for Counting › fn alloc` fn 1 | `GlobalAlloc::alloc` | trait contract | std | as LA-T01 |
| LA-T03 | 30 | `impl GlobalAlloc for Counting › fn alloc` block 1 | `System.alloc(layout)` | the caller's layout (valid, non-zero size) | forwarding | as LA-T01 |
| LA-T04 | 32 | `impl GlobalAlloc for Counting › fn dealloc` fn 1 | `GlobalAlloc::dealloc` | trait contract | std | as LA-T01 |
| LA-T05 | 34 | `impl GlobalAlloc for Counting › fn dealloc` block 1 | `System.dealloc(ptr, layout)` | `ptr` from `System.alloc` with this layout | forwarding | as LA-T01 |

### tests/eig_fixed.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| LA-T06 | 40 | `impl GlobalAlloc for Counting` impl 1 | counting global allocator of the test binary | as LA-T01 | forwarding | `eig_fixed` |
| LA-T07 | 41 | `impl GlobalAlloc for Counting › fn alloc` fn 1 | `GlobalAlloc::alloc` | trait contract | std | as LA-T06 |
| LA-T08 | 44 | `impl GlobalAlloc for Counting › fn alloc` block 1 | `System.alloc(layout)` | the caller's layout | forwarding | as LA-T06 |
| LA-T09 | 46 | `impl GlobalAlloc for Counting › fn dealloc` fn 1 | `GlobalAlloc::dealloc` | trait contract | std | as LA-T06 |
| LA-T10 | 48 | `impl GlobalAlloc for Counting › fn dealloc` block 1 | `System.dealloc(ptr, layout)` | `ptr` from `System.alloc` with this layout | forwarding | as LA-T06 |

## Dynamic checks

* **Miri** cannot run this code (MUMPS and PETSc through FFI; the test
  allocators wrap long numerical tests).
* **valgrind memcheck** (`docs/TESTING.md`): `mumps_mpi` at 1 and 2
  ranks and `petsc_mpi` (akantu-solver, the PETSc backend over this module)
  at 1 and 2 ranks: 0 errors. The reproducers: before the index checks
  were added, an out-of-range row (1 rank) produced invalid reads / writes
  inside libpetsc; now `add_row` refuses the row in Rust and every case
  (`row=3`, `row=66`, `row-big`, `col-big`, `get-row`, `get-col`,
  `ao-big`, `ksp-layout`) runs with 0 errors; `petsc_mpi` and `mumps_mpi`
  under memcheck include the regression scenarios `ua01_add_row_range`,
  `ua03_ksp_layout`, `ao_map_range` and `ua04_array_backed_icntl`
  (0 errors). Earlier runs:
  the MUMPS regression scenarios under valgrind
  (`scripts/mumps/run-v66-regressions.sh --valgrind`), and the PETSc
  scenarios `oracle_serial`, `petsc_session_restart` and
  `pe09_finalize_order` (0 errors). `ci/tiers/memcheck.sh` is the
  repeatable job.
