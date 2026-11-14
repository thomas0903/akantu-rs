// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the PETSc calls of
// src/model/common/dof_manager/dof_manager_petsc.cc, src/solver/
// {sparse_matrix_petsc.cc, solver_vector_petsc.cc, sparse_solver_petsc.cc},
// src/model/common/non_linear_solver/{non_linear_solver_petsc.cc,
// non_linear_solver_tao.cc}, through the C shim `shim.c`.

//! Raw declarations of the C shim (`src/petsc/shim.c`). Every function
//! returns `0` or a PETSc error code; the shim's own types are plain C
//! types (`int`, `int64_t`, `double`, `size_t`, opaque `void *`), so these
//! declarations never depend on PETSc's struct layouts. `PetscInt` arrays
//! are `i32` (checked at session start with `akantu_petsc_int_size`), the
//! scalar is `double` (checked with `akantu_petsc_scalar_size`).
//!
//! Only the safe wrappers of this module (`session`, `objects`) call these
//! functions; each call site states its `SAFETY` invariants.

use std::ffi::{c_char, c_int, c_void};

/// PETSc's `PetscInt` in the supported builds (32-bit indices).
pub type PetscInt = i32;

/// `MPI_Comm` as seen by the shim (rsmpi's raw handle type).
pub type MpiComm = mpi::ffi::MPI_Comm;

/// Residual callback: owned `x` (n values) in, owned `f` out.
pub type ResidualFn =
    unsafe extern "C" fn(user: *mut c_void, n: i64, x: *const f64, f: *mut f64) -> c_int;
/// Jacobian / Hessian callback at the owned trial point `x`.
pub type JacobianFn = unsafe extern "C" fn(user: *mut c_void, n: i64, x: *const f64) -> c_int;
/// TAO objective + gradient callback.
pub type ObjectiveFn = unsafe extern "C" fn(
    user: *mut c_void,
    n: i64,
    x: *const f64,
    obj: *mut f64,
    grad: *mut f64,
) -> c_int;

/// `struct akantu_snes_report` of the shim (same field order and C types).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct SnesReport {
    /// `SNESGetIterationNumber`.
    pub iterations: i64,
    /// Entries of the convergence history (`SNESGetConvergenceHistory`).
    pub history_len: i64,
    /// `SNESGetTolerances` maximum iterations.
    pub max_it: i64,
    /// `SNESGetTolerances` maximum function evaluations.
    pub max_f: i64,
    /// `SNESGetFunctionNorm`.
    pub function_norm: f64,
    /// `SNESGetSolutionNorm`.
    pub solution_norm: f64,
    /// `SNESGetUpdateNorm`.
    pub update_norm: f64,
    /// Absolute tolerance.
    pub atol: f64,
    /// Relative tolerance.
    pub rtol: f64,
    /// Step tolerance.
    pub stol: f64,
    /// Divergence tolerance.
    pub divtol: f64,
    /// `SNESConvergedReason` (`>= 0`: converged).
    pub reason: c_int,
    /// `SNESGetConvergedReasonString` (NUL-terminated).
    pub reason_string: [c_char; 128],
}

impl Default for SnesReport {
    fn default() -> Self {
        Self {
            iterations: 0,
            history_len: 0,
            max_it: 0,
            max_f: 0,
            function_norm: 0.0,
            solution_norm: 0.0,
            update_norm: 0.0,
            atol: 0.0,
            rtol: 0.0,
            stol: 0.0,
            divtol: 0.0,
            reason: 0,
            reason_string: [0; 128],
        }
    }
}

/// `struct akantu_tao_report` of the shim.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TaoReport {
    /// `TaoGetIterationNumber`.
    pub iterations: i64,
    /// `TaoGetMaximumIterations`.
    pub max_it: i64,
    /// Gradient absolute tolerance.
    pub gatol: f64,
    /// Gradient relative tolerance.
    pub grtol: f64,
    /// Gradient step tolerance.
    pub gttol: f64,
    /// Final objective (`TaoGetSolutionStatus`).
    pub fval: f64,
    /// Final gradient norm (`TaoGetSolutionStatus`; projected for a bounded
    /// solve).
    pub gnorm: f64,
    /// `TaoConvergedReason` (`>= 0`: converged).
    pub reason: c_int,
}

// The shim's signatures (src/petsc/shim.c): `int` ↔ `c_int`, `int64_t` ↔
// `i64`, `double` ↔ `f64`, `size_t` ↔ `usize`, `PetscInt` ↔ `i32` (checked
// at session start), `MPI_Comm` ↔ rsmpi's `MPI_Comm`, opaque objects as
// `*mut c_void`, report structs `#[repr(C)]` with the same field order.
// Allowed: raw declarations whose common contract is in the module docs above; a doc comment per name would only repeat the C function name.
#[allow(missing_docs)]
unsafe extern "C" {
    pub fn akantu_petsc_last_error(code: c_int, buf: *mut c_char, len: usize) -> c_int;
    pub fn akantu_petsc_clear_error();
    pub fn akantu_petsc_error_message(code: c_int, buf: *mut c_char, len: usize) -> c_int;

    pub fn akantu_petsc_int_size() -> c_int;
    pub fn akantu_petsc_scalar_size() -> c_int;
    pub fn akantu_petsc_real_size() -> c_int;
    pub fn akantu_petsc_version(major: *mut c_int, minor: *mut c_int, subminor: *mut c_int);
    pub fn akantu_petsc_is_initialized(flag: *mut c_int) -> c_int;
    pub fn akantu_petsc_world_compare(comm: MpiComm, relation: *mut c_int) -> c_int;
    pub fn akantu_petsc_blas_corename() -> *const c_char;
    pub fn akantu_petsc_blas_threads() -> c_int;
    pub fn akantu_petsc_initialize(comm: MpiComm, argc: c_int, argv: *mut *mut c_char) -> c_int;
    pub fn akantu_petsc_finalize() -> c_int;

    pub fn akantu_petsc_options_create(out: *mut *mut c_void) -> c_int;
    pub fn akantu_petsc_options_destroy(opts: *mut c_void) -> c_int;
    pub fn akantu_petsc_options_set(
        opts: *mut c_void,
        name: *const c_char,
        value: *const c_char,
    ) -> c_int;

    pub fn akantu_petsc_ao_map(
        comm: MpiComm,
        n_owned: i64,
        app: *const PetscInt,
        n: i64,
        idx: *mut PetscInt,
    ) -> c_int;

    pub fn akantu_petsc_mat_create(
        comm: MpiComm,
        name: *const c_char,
        m: i64,
        big_n: i64,
        symmetric: c_int,
        out: *mut *mut c_void,
    ) -> c_int;
    pub fn akantu_petsc_mat_set_unsymmetric(mat: *mut c_void) -> c_int;
    pub fn akantu_petsc_mat_destroy(mat: *mut c_void) -> c_int;
    pub fn akantu_petsc_mat_reference(mat: *mut c_void) -> c_int;
    pub fn akantu_petsc_mat_zero_entries(mat: *mut c_void) -> c_int;
    pub fn akantu_petsc_mat_reset_preallocation(mat: *mut c_void) -> c_int;
    pub fn akantu_petsc_mat_add_row(
        mat: *mut c_void,
        row: PetscInt,
        n: i64,
        cols: *const PetscInt,
        vals: *const f64,
    ) -> c_int;
    pub fn akantu_petsc_mat_assemble(mat: *mut c_void) -> c_int;
    pub fn akantu_petsc_mat_ownership(mat: *mut c_void, lo: *mut i64, hi: *mut i64) -> c_int;
    pub fn akantu_petsc_mat_get_values(
        mat: *mut c_void,
        m: i64,
        rows: *const PetscInt,
        n: i64,
        cols: *const PetscInt,
        vals: *mut f64,
    ) -> c_int;
    pub fn akantu_petsc_mat_nonzeros(mat: *mut c_void, nz_used: *mut f64) -> c_int;
    pub fn akantu_petsc_mat_mult(
        mat: *mut c_void,
        n_owned: i64,
        x: *const f64,
        y: *mut f64,
    ) -> c_int;
    pub fn akantu_petsc_mat_save_matrix_market(mat: *mut c_void, filename: *const c_char) -> c_int;

    pub fn akantu_petsc_vec_create(
        comm: MpiComm,
        name: *const c_char,
        n: i64,
        big_n: i64,
        out: *mut *mut c_void,
    ) -> c_int;
    pub fn akantu_petsc_vec_destroy(vec: *mut c_void) -> c_int;
    pub fn akantu_petsc_vec_set_owned(vec: *mut c_void, n: i64, vals: *const f64) -> c_int;
    pub fn akantu_petsc_vec_get_owned(vec: *mut c_void, n: i64, vals: *mut f64) -> c_int;
    pub fn akantu_petsc_vec_zero(vec: *mut c_void) -> c_int;
    pub fn akantu_petsc_vec_dot(a: *mut c_void, b: *mut c_void, out: *mut f64) -> c_int;

    pub fn akantu_petsc_ksp_create(
        comm: MpiComm,
        name: *const c_char,
        out: *mut *mut c_void,
    ) -> c_int;
    pub fn akantu_petsc_ksp_destroy(ksp: *mut c_void) -> c_int;
    pub fn akantu_petsc_ksp_reset(ksp: *mut c_void) -> c_int;
    pub fn akantu_petsc_ksp_set_from_options(ksp: *mut c_void, opts: *mut c_void) -> c_int;
    pub fn akantu_petsc_ksp_solve(
        ksp: *mut c_void,
        mat: *mut c_void,
        b: *mut c_void,
        x: *mut c_void,
        setup_failed: *mut c_int,
        reason: *mut c_int,
        its: *mut i64,
        rnorm: *mut f64,
    ) -> c_int;
    pub fn akantu_petsc_ksp_reason_string(ksp: *mut c_void, buf: *mut c_char, len: usize) -> c_int;
    pub fn akantu_petsc_ksp_tolerances(
        ksp: *mut c_void,
        rtol: *mut f64,
        atol: *mut f64,
        dtol: *mut f64,
        maxits: *mut i64,
    ) -> c_int;
    pub fn akantu_petsc_ksp_types(
        ksp: *mut c_void,
        ksp_type: *mut c_char,
        pc_type: *mut c_char,
        len: usize,
    ) -> c_int;

    pub fn akantu_petsc_snes_create(
        comm: MpiComm,
        name: *const c_char,
        out: *mut *mut c_void,
    ) -> c_int;
    pub fn akantu_petsc_snes_destroy(snes: *mut c_void) -> c_int;
    pub fn akantu_petsc_snes_set_from_options(snes: *mut c_void, opts: *mut c_void) -> c_int;
    pub fn akantu_petsc_snes_solve(
        snes: *mut c_void,
        mat: *mut c_void,
        x: *mut c_void,
        r: *mut c_void,
        hist: *mut f64,
        hist_its: *mut PetscInt,
        hist_len: i64,
        residual: ResidualFn,
        jacobian: JacobianFn,
        user: *mut c_void,
        n: i64,
        alias: c_int,
        rep: *mut SnesReport,
    ) -> c_int;

    pub fn akantu_petsc_tao_create(
        comm: MpiComm,
        name: *const c_char,
        out: *mut *mut c_void,
    ) -> c_int;
    pub fn akantu_petsc_tao_destroy(tao: *mut c_void) -> c_int;
    pub fn akantu_petsc_tao_set_from_options(tao: *mut c_void, opts: *mut c_void) -> c_int;
    pub fn akantu_petsc_tao_solve(
        tao: *mut c_void,
        mat: *mut c_void,
        x: *mut c_void,
        lower: *const f64,
        upper: *const f64,
        objective: ObjectiveFn,
        hessian: JacobianFn,
        user: *mut c_void,
        n: i64,
        alias: c_int,
        rep: *mut TaoReport,
    ) -> c_int;
}
