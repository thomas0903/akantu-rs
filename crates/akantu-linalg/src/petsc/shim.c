/* SPDX-License-Identifier: LGPL-3.0-or-later
 *
 * akantu-rs C shim over PETSc (Cargo feature `petsc`).
 * Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/
 * dof_manager_petsc.cc (PETScSingleton, petscErrorHandler, AO numbering),
 * src/solver/sparse_matrix_petsc.cc (MatCreate sequence, options, assembly),
 * src/solver/solver_vector_petsc.cc, src/solver/sparse_solver_petsc.cc (KSP),
 * src/model/common/non_linear_solver/non_linear_solver_petsc.cc (SNES) and
 * non_linear_solver_tao.cc (TAO), src/io/parser/parsable_petsc.hh (private
 * options databases pushed around XSetFromOptions).
 *
 * Copyright (C) 2011-2023 EPFL (Ecole Polytechnique Federale de Lausanne),
 * Laboratory (LSMS - Laboratoire de Simulation en Mecanique des Solides).
 *
 * The shim exposes PETSc through plain C types (int, int64_t, double,
 * opaque void pointers) so the Rust side never duplicates PETSc's struct
 * layouts or macro values; every function returns 0 or a PETSc error code
 * (the first error message is kept, see akantu_petsc_last_error). PETSc
 * objects (Mat, Vec, KSP, SNES, Tao, AO, PetscOptions) are pointers to
 * opaque structs. Indices are PetscInt (32-bit in the pinned Debian build;
 * the Rust side checks akantu_petsc_int_size() before passing arrays).
 */
#define _GNU_SOURCE /* RTLD_DEFAULT */
#include <dlfcn.h>
#include <petscao.h>
#include <petscksp.h>
#include <petscsnes.h>
#include <petsctao.h>
#include <petscviewer.h>

#include <stdint.h>
#include <stdio.h>
#include <string.h>

/* Supported PETSc builds: real double scalars, 32-bit
 * PetscInt (the oracle's Debian build; also checked at run time and agreed
 * by the session) and the PETSc >= 3.17 API used below (bitwise claims hold
 * for the pinned 3.22.5 only). */
#if defined(PETSC_USE_COMPLEX)
#error "akantu-rs PETSc shim: complex PETSc builds are not supported (real double scalars needed)"
#endif
#if defined(PETSC_USE_64BIT_INDICES)
#error "akantu-rs PETSc shim: 64-bit PetscInt builds are not supported (32-bit indices needed)"
#endif
#if PETSC_VERSION_LT(3, 17, 0)
#error "akantu-rs PETSc shim: PETSc >= 3.17 is required"
#endif

#define AKA_TRY(x)                                                             \
  do {                                                                         \
    PetscErrorCode aka_ierr_ = (x);                                            \
    if (aka_ierr_ != PETSC_SUCCESS)                                            \
      return (int)aka_ierr_;                                                   \
  } while (0)

/* Like AKA_TRY inside functions with cleanup: keep the first error code in
 * the local `ierr` and jump to `fail`. */
#define AKA_CHECK(x)                                                           \
  do {                                                                         \
    ierr = (x);                                                                \
    if (ierr != PETSC_SUCCESS)                                                 \
      goto fail;                                                               \
  } while (0)

/* A size or index from Rust (`int64_t`) as a PetscInt, range-checked
 * (no unchecked narrowing casts). */
static PetscErrorCode aka_int(int64_t v, PetscInt *out) {
  if (v < 0 || v > (int64_t)PETSC_MAX_INT)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ARG_OUTOFRANGE,
            "akantu-rs: size %lld outside the PetscInt range", (long long)v);
  *out = (PetscInt)v;
  return PETSC_SUCCESS;
}

/* ------------------------------------------------------------------------ */
/* Errors                                                                    */
/* ------------------------------------------------------------------------ */

/* Calls are serialised by the Rust side (process-wide PETSc lock), so one
 * static buffer is enough. `aka_last_code` is the PETSc error code of the
 * recorded message: akantu_petsc_last_error returns the
 * message only for that code. */
static char aka_last_error[4096];
static PetscErrorCode aka_last_code = PETSC_SUCCESS;

/* Upstream `petscErrorHandler` throws an akantu exception with
 * "file:line: Error(p) in PETSc call to 'fun': mess"; the shim records the
 * initial error instead (no exception may cross into Rust) and returns the
 * code, which propagates up through PETSc's call stack. */
static PetscErrorCode aka_error_handler(MPI_Comm comm, int line,
                                        const char *fun, const char *file,
                                        PetscErrorCode n, PetscErrorType p,
                                        const char *mess, void *ctx) {
  (void)comm;
  (void)ctx;
  if (p == PETSC_ERROR_INITIAL || aka_last_error[0] == '\0') {
    const char *desc = NULL;
    PetscErrorMessage(n, &desc, NULL);
    snprintf(aka_last_error, sizeof aka_last_error,
             "%s:%d: Error(%d) in PETSc call to '%s': %s%s%s", file ? file : "?",
             line, (int)p, fun ? fun : "?", mess ? mess : "",
             desc ? " | " : "", desc ? desc : "");
    aka_last_code = n;
  }
  return n;
}

/* The recorded message if it was recorded for `code`, the code the failing
 * shim function returned; otherwise an empty string (0). A message left in
 * the buffer by an earlier failure that was not read (the Rust side's
 * fallback when another thread holds the PETSc lock)
 * is therefore not reported for a later failure whose code did not pass
 * PETSc's error handler, e.g. the PETSC_ERR_ARG_SIZ of vec_set_owned or
 * the PETSC_ERR_MPI of ao_map, unless the two codes
 * coincide: a stale message recorded for code X is still returned for a
 * later failure with code X that did not pass the handler
 * (only the code is compared, there is no sequence number). */
int akantu_petsc_last_error(int code, char *buf, size_t len) {
  if (len == 0)
    return 0;
  if (aka_last_error[0] == '\0' || (int)aka_last_code != code) {
    buf[0] = '\0';
    return 0;
  }
  strncpy(buf, aka_last_error, len - 1);
  buf[len - 1] = '\0';
  return (int)strlen(buf);
}

void akantu_petsc_clear_error(void) {
  aka_last_error[0] = '\0';
  aka_last_code = PETSC_SUCCESS;
}

int akantu_petsc_error_message(int code, char *buf, size_t len) {
  const char *desc = NULL;
  if (len == 0)
    return 0;
  if (PetscErrorMessage((PetscErrorCode)code, &desc, NULL) != PETSC_SUCCESS ||
      desc == NULL)
    desc = "unknown PETSc error";
  strncpy(buf, desc, len - 1);
  buf[len - 1] = '\0';
  return (int)strlen(buf);
}

/* ------------------------------------------------------------------------ */
/* Library                                                                   */
/* ------------------------------------------------------------------------ */

int akantu_petsc_int_size(void) { return (int)sizeof(PetscInt); }
int akantu_petsc_scalar_size(void) { return (int)sizeof(PetscScalar); }
int akantu_petsc_real_size(void) { return (int)sizeof(PetscReal); }

void akantu_petsc_version(int *major, int *minor, int *subminor) {
  *major = PETSC_VERSION_MAJOR;
  *minor = PETSC_VERSION_MINOR;
  *subminor = PETSC_VERSION_SUBMINOR;
}

int akantu_petsc_is_initialized(int *flag) {
  PetscBool b = PETSC_FALSE;
  AKA_TRY(PetscInitialized(&b));
  *flag = b ? 1 : 0;
  return 0;
}

/* Upstream PETScSingleton: `-no_signal_handler true`, PetscInitialize with
 * the program arguments, then the default error handler replaced. PETSc's
 * world is the communicator of the first PETSc user (an akantu duplicate);
 * MPI is already initialised by akantu, so PetscFinalize never finalises
 * MPI and PETSc may be initialised again after a finalisation. `argv` must
 * stay valid until akantu_petsc_finalize (PETSc keeps the pointers). */
int akantu_petsc_initialize(MPI_Comm comm, int argc, char **argv) {
  PetscBool init = PETSC_FALSE;
  int c = argc;
  char **v = argv;
  AKA_TRY(PetscInitialized(&init));
  if (init)
    return 0;
  PETSC_COMM_WORLD = comm;
  AKA_TRY(PetscOptionsSetValue(NULL, "-no_signal_handler", "true"));
  AKA_TRY(PetscInitialize(&c, &v, NULL, NULL));
  AKA_TRY(PetscPopErrorHandler());
  AKA_TRY(PetscPushErrorHandler(aka_error_handler, NULL));
  return 0;
}

/* Diagnostics: relation of PETSC_COMM_WORLD to `comm`
 * (MPI_Comm_compare: 0 = identical, 1 = congruent, 2 = similar,
 * 3 = unequal; -1 if PETSc is not initialised). A local MPI call: the Rust
 * side runs it inside an agreed native entry (thread policy held). */
int akantu_petsc_world_compare(MPI_Comm comm, int *relation) {
  PetscBool init = PETSC_FALSE;
  int r = MPI_UNEQUAL;
  AKA_TRY(PetscInitialized(&init));
  if (!init) {
    *relation = -1;
    return 0;
  }
  if (MPI_Comm_compare(PETSC_COMM_WORLD, comm, &r) != MPI_SUCCESS)
    return (int)PETSC_ERR_MPI;
  *relation = r == MPI_IDENT ? 0 : r == MPI_CONGRUENT ? 1 : r == MPI_SIMILAR ? 2 : 3;
  return 0;
}

/* BLAS provenance (D-050): OpenBLAS's core type and
 * thread count, looked up in the global scope (PETSc links OpenBLAS through
 * the BLAS/LAPACK alternatives); NULL / -1 if the BLAS is not OpenBLAS. */
const char *akantu_petsc_blas_corename(void) {
  char *(*f)(void) = (char *(*)(void))dlsym(RTLD_DEFAULT, "openblas_get_corename");
  return f ? f() : NULL;
}
int akantu_petsc_blas_threads(void) {
  int (*f)(void) = (int (*)(void))dlsym(RTLD_DEFAULT, "openblas_get_num_threads");
  return f ? f() : -1;
}

int akantu_petsc_finalize(void) {
  PetscBool init = PETSC_FALSE;
  AKA_TRY(PetscInitialized(&init));
  if (!init)
    return 0;
  return (int)PetscFinalize();
}

/* ------------------------------------------------------------------------ */
/* Options databases (ParsablePETSc)                                          */
/* ------------------------------------------------------------------------ */

int akantu_petsc_options_create(void **out) {
  PetscOptions o = NULL;
  AKA_TRY(PetscOptionsCreate(&o));
  *out = (void *)o;
  return 0;
}

int akantu_petsc_options_destroy(void *opts) {
  PetscOptions o = (PetscOptions)opts;
  if (!o)
    return 0;
  return (int)PetscOptionsDestroy(&o);
}

int akantu_petsc_options_set(void *opts, const char *name, const char *value) {
  return (int)PetscOptionsSetValue((PetscOptions)opts, name, value);
}

/* ------------------------------------------------------------------------ */
/* Application ordering (DOFManagerPETSc::setISLocalToGlobalMapping)          */
/* ------------------------------------------------------------------------ */

/* `app` are the akantu global equations owned by this rank in local order;
 * PETSc ordering is the natural one (owned rows of rank r contiguous, in
 * rank order). `idx` (n entries, akantu globals or negative) is mapped in
 * place to PETSc indices (negative entries unchanged, out-of-range ones
 * mapped to -1 by PETSc). Collective.
 *
 * Every `app` entry must lie in [0, N), N the sum of the
 * ranks' `n_owned`. AOCreateMemoryScalable does not check it in a
 * non-debug PETSc: for an out-of-range entry it finds no owner, leaves the
 * owner slot of its (uninitialised) work array unset and then indexes send
 * buffers with it (aomemscalable.c, PETSc 3.22). The range is checked here
 * on every rank and the verdict agreed (MPI_MAX) before the AO exists, so
 * every rank refuses together. Duplicates or holes are not memory errors
 * there (every entry has an owner) and remain the caller's contract.
 *
 * Both reductions run on every rank whatever the local
 * outcome, so no rank leaves the collective sequence early: a local size
 * outside the PetscInt range (`n_owned` or `n` above 2^31 - 1) is folded
 * into the verdict instead of returning before the first MPI_Allreduce,
 * and N is summed in 64 bits (MPI_INT64_T) and refused when it exceeds
 * PETSC_MAX_INT (the 32-bit sum could overflow). Verdict codes, the
 * largest wins: 1 an application index outside [0, N), 2 N above the
 * PetscInt range, 3 a local size above it. */
int akantu_petsc_ao_map(MPI_Comm comm, int64_t n_owned, const PetscInt *app,
                        int64_t n, PetscInt *idx) {
  AO ao = NULL;
  PetscErrorCode ierr;
  PetscInt no = 0, ni = 0;
  int64_t mine = 0, total = 0;
  int bad = 0, any_bad = 0;
  if (n_owned < 0 || n_owned > (int64_t)PETSC_MAX_INT || n < 0 ||
      n > (int64_t)PETSC_MAX_INT) {
    bad = 3;
  } else {
    no = (PetscInt)n_owned;
    ni = (PetscInt)n;
    mine = n_owned;
  }
  if (MPI_Allreduce(&mine, &total, 1, MPI_INT64_T, MPI_SUM, comm) != MPI_SUCCESS)
    return (int)PETSC_ERR_MPI;
  if (!bad && total > (int64_t)PETSC_MAX_INT)
    bad = 2;
  if (!bad)
    for (PetscInt i = 0; i < no; ++i)
      if (app[i] < 0 || (int64_t)app[i] >= total) {
        bad = 1;
        break;
      }
  if (MPI_Allreduce(&bad, &any_bad, 1, MPI_INT, MPI_MAX, comm) != MPI_SUCCESS)
    return (int)PETSC_ERR_MPI;
  if (any_bad == 3)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ARG_OUTOFRANGE,
            "akantu-rs: a size outside the PetscInt range on %s rank (AO map refused on "
            "every rank)",
            bad == 3 ? "this" : "another");
  if (any_bad == 2)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ARG_OUTOFRANGE,
            "akantu-rs: %lld owned equations in total exceed the PetscInt range (AO map "
            "refused on every rank)",
            (long long)total);
  if (any_bad)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ARG_OUTOFRANGE,
            "akantu-rs: an application index outside [0, %lld) on %s rank "
            "(AOCreateMemoryScalable would write out of bounds)",
            (long long)total, bad ? "this" : "another");
  AKA_TRY(AOCreateMemoryScalable(comm, no, app, NULL, &ao));
  ierr = AOApplicationToPetsc(ao, ni, idx);
  if (ierr != PETSC_SUCCESS) {
    AODestroy(&ao);
    return (int)ierr;
  }
  return (int)AODestroy(&ao);
}

/* ------------------------------------------------------------------------ */
/* Matrices (SparseMatrixPETSc)                                              */
/* ------------------------------------------------------------------------ */

/* Constructor sequence of SparseMatrixPETSc: MatCreate, name,
 * MatSetFromOptions, sizes (pure local × pure local, N × N), MatSetUp,
 * row-oriented values, new nonzero locations allowed, MAT_SYMMETRIC for a
 * symmetric matrix. */
int akantu_petsc_mat_create(MPI_Comm comm, const char *name, int64_t m,
                            int64_t big_n, int symmetric, void **out) {
  Mat A = NULL;
  PetscInt lm = 0, bn = 0;
  PetscErrorCode ierr = PETSC_SUCCESS;
  AKA_TRY(aka_int(m, &lm));
  AKA_TRY(aka_int(big_n, &bn));
  AKA_CHECK(MatCreate(comm, &A));
  AKA_CHECK(PetscObjectSetName((PetscObject)A, name));
  AKA_CHECK(MatSetFromOptions(A));
  AKA_CHECK(MatSetSizes(A, lm, lm, bn, bn));
  AKA_CHECK(MatSetUp(A));
  AKA_CHECK(MatSetOption(A, MAT_ROW_ORIENTED, PETSC_TRUE));
  AKA_CHECK(MatSetOption(A, MAT_NEW_NONZERO_LOCATIONS, PETSC_TRUE));
  if (symmetric)
    AKA_CHECK(MatSetOption(A, MAT_SYMMETRIC, PETSC_TRUE));
  *out = (void *)A;
  return 0;
fail: /* no partial object leaks */
  MatDestroy(&A);
  return (int)ierr;
}

/* SparseMatrixPETSc::addValues with an unsymmetric block: the matrix stops
 * being (structurally) symmetric. */
int akantu_petsc_mat_set_unsymmetric(void *mat) {
  Mat A = (Mat)mat;
  AKA_TRY(MatSetOption(A, MAT_SYMMETRIC, PETSC_FALSE));
  AKA_TRY(MatSetOption(A, MAT_STRUCTURALLY_SYMMETRIC, PETSC_FALSE));
  return 0;
}

int akantu_petsc_mat_destroy(void *mat) {
  Mat A = (Mat)mat;
  if (!A)
    return 0;
  return (int)MatDestroy(&A);
}

int akantu_petsc_mat_reference(void *mat) {
  return (int)PetscObjectReference((PetscObject)mat);
}

int akantu_petsc_mat_zero_entries(void *mat) {
  return (int)MatZeroEntries((Mat)mat);
}

/* SparseMatrixPETSc::clearProfile: forget the assembled nonzero pattern. */
int akantu_petsc_mat_reset_preallocation(void *mat) {
  Mat A = (Mat)mat;
  AKA_TRY(MatResetPreallocation(A));
  AKA_TRY(MatSetOption(A, MAT_NEW_NONZERO_LOCATIONS, PETSC_TRUE));
  return 0;
}

/* One row of values (row-oriented, ADD_VALUES, PETSc indices). */
int akantu_petsc_mat_add_row(void *mat, PetscInt row, int64_t n,
                             const PetscInt *cols, const double *vals) {
  PetscInt nc = 0;
  AKA_TRY(aka_int(n, &nc));
  return (int)MatSetValues((Mat)mat, 1, &row, nc, cols,
                           (const PetscScalar *)vals, ADD_VALUES);
}

/* SparseMatrixPETSc::applyModifications (begin/end assembly, new nonzero
 * locations allowed again). Collective. */
int akantu_petsc_mat_assemble(void *mat) {
  Mat A = (Mat)mat;
  AKA_TRY(MatAssemblyBegin(A, MAT_FINAL_ASSEMBLY));
  AKA_TRY(MatAssemblyEnd(A, MAT_FINAL_ASSEMBLY));
  AKA_TRY(MatSetOption(A, MAT_NEW_NONZERO_LOCATIONS, PETSC_TRUE));
  return 0;
}

int akantu_petsc_mat_ownership(void *mat, int64_t *lo, int64_t *hi) {
  PetscInt a = 0, b = 0;
  AKA_TRY(MatGetOwnershipRange((Mat)mat, &a, &b));
  *lo = a;
  *hi = b;
  return 0;
}

/* Values of owned rows (tests and diagnostics); `vals` is row-major m × n. */
int akantu_petsc_mat_get_values(void *mat, int64_t m, const PetscInt *rows,
                                int64_t n, const PetscInt *cols,
                                double *vals) {
  PetscInt mr = 0, nc = 0;
  AKA_TRY(aka_int(m, &mr));
  AKA_TRY(aka_int(n, &nc));
  return (int)MatGetValues((Mat)mat, mr, rows, nc, cols, (PetscScalar *)vals);
}

int akantu_petsc_mat_nonzeros(void *mat, double *nz_used) {
  MatInfo info;
  AKA_TRY(MatGetInfo((Mat)mat, MAT_GLOBAL_SUM, &info));
  *nz_used = (double)info.nz_used;
  return 0;
}

/* `y = A x` with vectors given as owned arrays (the Vecs are created and
 * destroyed here). Collective. */
int akantu_petsc_mat_mult(void *mat, int64_t n_owned, const double *x,
                          double *y) {
  Mat A = (Mat)mat;
  Vec vx = NULL, vy = NULL;
  const PetscScalar *ry = NULL;
  PetscScalar *wx = NULL;
  PetscInt nl = 0;
  PetscErrorCode ierr = PETSC_SUCCESS;
  AKA_TRY(MatGetLocalSize(A, &nl, NULL));
  if ((int64_t)nl != n_owned)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ARG_SIZ, "akantu-rs: MatMult local size mismatch");
  AKA_CHECK(MatCreateVecs(A, &vx, &vy));
  AKA_CHECK(VecGetArray(vx, &wx));
  for (int64_t i = 0; i < n_owned; ++i)
    wx[i] = x[i];
  AKA_CHECK(VecRestoreArray(vx, &wx));
  AKA_CHECK(MatMult(A, vx, vy));
  AKA_CHECK(VecGetArrayRead(vy, &ry));
  for (int64_t i = 0; i < n_owned; ++i)
    y[i] = ry[i];
  AKA_CHECK(VecRestoreArrayRead(vy, &ry));
fail: /* the work vectors are destroyed on every path */
  VecDestroy(&vx);
  VecDestroy(&vy);
  return (int)ierr;
}

/* SparseMatrixPETSc::saveMatrix: Matrix Market through a PETSc viewer.
 * Collective. */
int akantu_petsc_mat_save_matrix_market(void *mat, const char *filename) {
  Mat A = (Mat)mat;
  MPI_Comm comm;
  PetscViewer viewer = NULL;
  AKA_TRY(PetscObjectGetComm((PetscObject)A, &comm));
  AKA_TRY(PetscViewerASCIIOpen(comm, filename, &viewer));
  AKA_TRY(PetscViewerPushFormat(viewer, PETSC_VIEWER_ASCII_MATRIXMARKET));
  AKA_TRY(MatView(A, viewer));
  AKA_TRY(PetscViewerPopFormat(viewer));
  AKA_TRY(PetscViewerDestroy(&viewer));
  return 0;
}

/* ------------------------------------------------------------------------ */
/* Vectors (SolverVectorPETSc)                                               */
/* ------------------------------------------------------------------------ */

int akantu_petsc_vec_create(MPI_Comm comm, const char *name, int64_t n,
                            int64_t big_n, void **out) {
  Vec x = NULL;
  PetscInt ln = 0, bn = 0;
  PetscErrorCode ierr = PETSC_SUCCESS;
  AKA_TRY(aka_int(n, &ln));
  AKA_TRY(aka_int(big_n, &bn));
  AKA_CHECK(VecCreate(comm, &x));
  AKA_CHECK(PetscObjectSetName((PetscObject)x, name));
  AKA_CHECK(VecSetSizes(x, ln, bn));
  AKA_CHECK(VecSetFromOptions(x));
  *out = (void *)x;
  return 0;
fail:
  VecDestroy(&x);
  return (int)ierr;
}

int akantu_petsc_vec_destroy(void *vec) {
  Vec x = (Vec)vec;
  if (!x)
    return 0;
  return (int)VecDestroy(&x);
}

int akantu_petsc_vec_set_owned(void *vec, int64_t n, const double *vals) {
  Vec x = (Vec)vec;
  PetscScalar *w = NULL;
  PetscInt nl = 0;
  AKA_TRY(VecGetLocalSize(x, &nl));
  if ((int64_t)nl != n)
    return (int)PETSC_ERR_ARG_SIZ;
  AKA_TRY(VecGetArray(x, &w));
  for (int64_t i = 0; i < n; ++i)
    w[i] = vals[i];
  AKA_TRY(VecRestoreArray(x, &w));
  return 0;
}

int akantu_petsc_vec_get_owned(void *vec, int64_t n, double *vals) {
  Vec x = (Vec)vec;
  const PetscScalar *r = NULL;
  PetscInt nl = 0;
  AKA_TRY(VecGetLocalSize(x, &nl));
  if ((int64_t)nl != n)
    return (int)PETSC_ERR_ARG_SIZ;
  AKA_TRY(VecGetArrayRead(x, &r));
  for (int64_t i = 0; i < n; ++i)
    vals[i] = r[i];
  AKA_TRY(VecRestoreArrayRead(x, &r));
  return 0;
}

int akantu_petsc_vec_zero(void *vec) { return (int)VecZeroEntries((Vec)vec); }

/* VecDot(a, b) (NonLinearSolverTAO::computeObjectiveGradient's dot
 * products: BLAS ddot on the owned rows, then a reduction over the
 * vector's communicator). Collective. */
int akantu_petsc_vec_dot(void *a, void *b, double *out) {
  PetscScalar d = 0;
  AKA_TRY(VecDot((Vec)a, (Vec)b, &d));
  *out = (double)d;
  return 0;
}

/* ------------------------------------------------------------------------ */
/* KSP (SparseSolverPETSc)                                                   */
/* ------------------------------------------------------------------------ */

int akantu_petsc_ksp_create(MPI_Comm comm, const char *name, void **out) {
  KSP ksp = NULL;
  PetscErrorCode ierr = PETSC_SUCCESS;
  AKA_CHECK(KSPCreate(comm, &ksp));
  AKA_CHECK(PetscObjectSetName((PetscObject)ksp, name));
  *out = (void *)ksp;
  return 0;
fail:
  KSPDestroy(&ksp);
  return (int)ierr;
}

int akantu_petsc_ksp_destroy(void *ksp) {
  KSP k = (KSP)ksp;
  if (!k)
    return 0;
  return (int)KSPDestroy(&k);
}

/* KSPReset: drop the operators, preconditioner and work vectors (type and
 * options are kept), so the next KSPSetOperators may use a matrix with
 * other local sizes (re-partitioning). Collective. */
int akantu_petsc_ksp_reset(void *ksp) { return (int)KSPReset((KSP)ksp); }

/* ParsablePETSc::updateInternalParameters: the private database pushed
 * around KSPSetFromOptions, then the global database (command line,
 * environment) applied on top. */
int akantu_petsc_ksp_set_from_options(void *ksp, void *opts) {
  KSP k = (KSP)ksp;
  if (opts) {
    PetscErrorCode ierr;
    AKA_TRY(PetscOptionsPush((PetscOptions)opts));
    ierr = KSPSetFromOptions(k);
    AKA_TRY(PetscOptionsPop());
    AKA_TRY(ierr);
  }
  return (int)KSPSetFromOptions(k);
}

/* SparseSolverPETSc::setOperators + solve: KSPSetOperators(A, A), KSPSetUp
 * (`*setup_failed` set when it fails: upstream's "is your matrix
 * singular?"), KSPSolve with a zero initial guess. Reports the converged
 * reason, the iteration count and the final residual norm. Collective. */
int akantu_petsc_ksp_solve(void *ksp, void *mat, void *b, void *x,
                           int *setup_failed, int *reason, int64_t *its,
                           double *rnorm) {
  KSP k = (KSP)ksp;
  KSPConvergedReason r = KSP_CONVERGED_ITERATING;
  PetscInt n = 0;
  PetscReal nrm = 0;
  PetscErrorCode ierr;
  *setup_failed = 0;
  AKA_TRY(KSPSetOperators(k, (Mat)mat, (Mat)mat));
  ierr = KSPSetUp(k);
  if (ierr != PETSC_SUCCESS) {
    *setup_failed = 1;
    return (int)ierr;
  }
  AKA_TRY(KSPSolve(k, (Vec)b, (Vec)x));
  AKA_TRY(KSPGetConvergedReason(k, &r));
  AKA_TRY(KSPGetIterationNumber(k, &n));
  AKA_TRY(KSPGetResidualNorm(k, &nrm));
  *reason = (int)r;
  *its = n;
  *rnorm = (double)nrm;
  return 0;
}

int akantu_petsc_ksp_reason_string(void *ksp, char *buf, size_t len) {
  const char *s = NULL;
  AKA_TRY(KSPGetConvergedReasonString((KSP)ksp, &s));
  if (len) {
    strncpy(buf, s ? s : "", len - 1);
    buf[len - 1] = '\0';
  }
  return 0;
}

int akantu_petsc_ksp_tolerances(void *ksp, double *rtol, double *atol,
                                double *dtol, int64_t *maxits) {
  PetscReal r, a, d;
  PetscInt m;
  AKA_TRY(KSPGetTolerances((KSP)ksp, &r, &a, &d, &m));
  *rtol = r;
  *atol = a;
  *dtol = d;
  *maxits = m;
  return 0;
}

/* Type names of the KSP and its PC (diagnostics, tests). */
int akantu_petsc_ksp_types(void *ksp, char *ksp_type, char *pc_type,
                           size_t len) {
  KSPType kt = NULL;
  PCType pt = NULL;
  PC pc = NULL;
  AKA_TRY(KSPGetType((KSP)ksp, &kt));
  AKA_TRY(KSPGetPC((KSP)ksp, &pc));
  AKA_TRY(PCGetType(pc, &pt));
  if (len) {
    strncpy(ksp_type, kt ? kt : "", len - 1);
    ksp_type[len - 1] = '\0';
    strncpy(pc_type, pt ? pt : "", len - 1);
    pc_type[len - 1] = '\0';
  }
  return 0;
}

/* ------------------------------------------------------------------------ */
/* Callbacks of the native non-linear solvers                                */
/* ------------------------------------------------------------------------ */

/* Implemented in Rust (`akantu_linalg::petsc::callbacks`): owned arrays in,
 * owned arrays out, status 0 or nonzero (the Rust error is stored on the
 * Rust side; the status is agreed on every rank before it is returned). */
typedef int (*aka_residual_fn)(void *user, int64_t n, const double *x,
                               double *f);
typedef int (*aka_jacobian_fn)(void *user, int64_t n, const double *x);
typedef int (*aka_objective_fn)(void *user, int64_t n, const double *x,
                                double *obj, double *grad);

struct aka_native_user {
  void *user;
  aka_residual_fn residual;
  aka_jacobian_fn jacobian;
  aka_objective_fn objective;
  int64_t n;
  /* UD-428 compat: upstream hands the DOF manager's solution vector to the
   * solver as its iterate and its residual vector as SNES's function
   * vector, and its corrector VecCopy's every trial point into the former
   * (the residual assembly overwrites the latter). With `alias`, the
   * callbacks reproduce those copies into `xsol` / `rvec`. */
  Vec xsol;
  Vec rvec;
  int alias;
};

static PetscErrorCode aka_alias_trial(struct aka_native_user *u, Vec x) {
  if (u->alias && u->xsol && x != u->xsol)
    PetscCall(VecCopy(x, u->xsol));
  return PETSC_SUCCESS;
}

/* The Rust trampolines slice PETSc's arrays with the
 * owned count `u->n` the solve passed; the vectors PETSc hands to a callback
 * are the solve's own or duplicates of them, so their local size is `u->n`
 * by construction. Checked once per callback (two integer compares) so that
 * the trampolines never depend on it. */
static PetscErrorCode aka_check_local(struct aka_native_user *u, Vec v) {
  PetscInt nl = 0;
  PetscCall(VecGetLocalSize(v, &nl));
  PetscCheck((int64_t)nl == u->n, PETSC_COMM_SELF, PETSC_ERR_ARG_SIZ,
             "akantu-rs: callback vector of local size %" PetscInt_FMT
             ", the solve's owned rows are %lld",
             nl, (long long)u->n);
  return PETSC_SUCCESS;
}

static PetscErrorCode aka_snes_function(SNES snes, Vec x, Vec f, void *ctx) {
  struct aka_native_user *u = (struct aka_native_user *)ctx;
  const PetscScalar *rx = NULL;
  PetscScalar *wf = NULL;
  int status;
  (void)snes;
  PetscCall(aka_check_local(u, x));
  PetscCall(aka_check_local(u, f));
  PetscCall(aka_alias_trial(u, x));
  PetscCall(VecGetArrayRead(x, &rx));
  PetscCall(VecGetArray(f, &wf));
  status = u->residual(u->user, u->n, (const double *)rx, (double *)wf);
  PetscCall(VecRestoreArray(f, &wf));
  PetscCall(VecRestoreArrayRead(x, &rx));
  if (status != 0)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_USER,
            "akantu residual callback failed (error stored by akantu)");
  if (u->alias && u->rvec && f != u->rvec)
    PetscCall(VecCopy(f, u->rvec));
  return PETSC_SUCCESS;
}

static PetscErrorCode aka_snes_jacobian(SNES snes, Vec x, Mat J, Mat P,
                                        void *ctx) {
  struct aka_native_user *u = (struct aka_native_user *)ctx;
  const PetscScalar *rx = NULL;
  int status;
  (void)snes;
  (void)J;
  (void)P;
  PetscCall(aka_check_local(u, x));
  PetscCall(aka_alias_trial(u, x));
  PetscCall(VecGetArrayRead(x, &rx));
  status = u->jacobian(u->user, u->n, (const double *)rx);
  PetscCall(VecRestoreArrayRead(x, &rx));
  if (status != 0)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_USER,
            "akantu Jacobian callback failed (error stored by akantu)");
  return PETSC_SUCCESS;
}

int akantu_petsc_snes_create(MPI_Comm comm, const char *name, void **out) {
  SNES snes = NULL;
  PetscErrorCode ierr = PETSC_SUCCESS;
  AKA_CHECK(SNESCreate(comm, &snes));
  AKA_CHECK(PetscObjectSetName((PetscObject)snes, name));
  AKA_CHECK(SNESSetType(snes, SNESNEWTONLS));
  AKA_CHECK(SNESSetFromOptions(snes));
  *out = (void *)snes;
  return 0;
fail:
  SNESDestroy(&snes);
  return (int)ierr;
}

int akantu_petsc_snes_destroy(void *snes) {
  SNES s = (SNES)snes;
  if (!s)
    return 0;
  return (int)SNESDestroy(&s);
}

int akantu_petsc_snes_set_from_options(void *snes, void *opts) {
  SNES s = (SNES)snes;
  if (opts) {
    PetscErrorCode ierr;
    AKA_TRY(PetscOptionsPush((PetscOptions)opts));
    ierr = SNESSetFromOptions(s);
    AKA_TRY(PetscOptionsPop());
    AKA_TRY(ierr);
  }
  return (int)SNESSetFromOptions(s);
}

/* Mirrored by `akantu_linalg::petsc::ffi::SnesReport` (#[repr(C)]). */
struct akantu_snes_report {
  int64_t iterations;
  int64_t history_len;
  int64_t max_it;
  int64_t max_f;
  double function_norm;
  double solution_norm;
  double update_norm;
  double atol;
  double rtol;
  double stol;
  double divtol;
  int reason;
  char reason_string[128];
};

/* NonLinearSolverPETSc::solve, native part: function, Jacobian and
 * convergence history set (`hist`/`hist_its` hold `hist_len` entries,
 * upstream 1000; SNES keeps the pointers until the next solve resets them),
 * x and r zeroed, SNESSolve(snes, NULL, x), then iteration count, norms,
 * reason and tolerances read back. Collective. */
/* Routines registered after a solve: the callback
 * context lives on the solving C frame, so it is detached before returning;
 * a later call by PETSc fails cleanly instead of reading a dead frame. */
static PetscErrorCode aka_snes_detached_function(SNES snes, Vec x, Vec f, void *ctx) {
  (void)snes;
  (void)x;
  (void)f;
  (void)ctx;
  SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ORDER,
          "akantu-rs: SNES function called outside akantu_petsc_snes_solve");
}

static PetscErrorCode aka_snes_detached_jacobian(SNES snes, Vec x, Mat J, Mat P,
                                                 void *ctx) {
  (void)snes;
  (void)x;
  (void)J;
  (void)P;
  (void)ctx;
  SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ORDER,
          "akantu-rs: SNES Jacobian called outside akantu_petsc_snes_solve");
}

/* Detach the frame's context and the caller's history buffers (PETSc
 * allocates its own history from now on; freed by SNESDestroy). */
static PetscErrorCode aka_snes_detach(SNES s) {
  PetscCall(SNESSetFunction(s, NULL, aka_snes_detached_function, NULL));
  PetscCall(SNESSetJacobian(s, NULL, NULL, aka_snes_detached_jacobian, NULL));
  PetscCall(SNESSetConvergenceHistory(s, NULL, NULL, PETSC_DECIDE, PETSC_TRUE));
  return PETSC_SUCCESS;
}

int akantu_petsc_snes_solve(void *snes, void *mat, void *x, void *r,
                            double *hist, PetscInt *hist_its, int64_t hist_len,
                            aka_residual_fn residual, aka_jacobian_fn jacobian,
                            void *user, int64_t n, int alias,
                            struct akantu_snes_report *rep) {
  SNES s = (SNES)snes;
  struct aka_native_user u;
  PetscReal *a = NULL;
  PetscInt *its = NULL, na = 0, nit = 0, hl = 0;
  SNESConvergedReason reason = SNES_CONVERGED_ITERATING;
  const char *rs = NULL;
  PetscReal fnorm = 0, xnorm = 0, ynorm = 0;
  PetscReal atol = 0, rtol = 0, stol = 0, divtol = 0;
  PetscInt maxit = 0, maxf = 0;
  PetscErrorCode ierr = PETSC_SUCCESS, detach;
  u.user = user;
  u.residual = residual;
  u.jacobian = jacobian;
  u.objective = NULL;
  u.n = n;
  u.xsol = (Vec)x;
  u.rvec = (Vec)r;
  u.alias = alias;
  AKA_TRY(aka_int(hist_len, &hl));
  AKA_TRY(VecZeroEntries((Vec)x));
  AKA_TRY(VecZeroEntries((Vec)r));
  AKA_CHECK(SNESSetFunction(s, (Vec)r, aka_snes_function, &u));
  AKA_CHECK(SNESSetJacobian(s, (Mat)mat, (Mat)mat, aka_snes_jacobian, &u));
  AKA_CHECK(SNESSetConvergenceHistory(s, (PetscReal *)hist, hist_its, hl, PETSC_TRUE));
  AKA_CHECK(SNESSolve(s, NULL, (Vec)x));
  AKA_CHECK(SNESGetIterationNumber(s, &nit));
  AKA_CHECK(SNESGetFunctionNorm(s, &fnorm));
  AKA_CHECK(SNESGetSolutionNorm(s, &xnorm));
  AKA_CHECK(SNESGetUpdateNorm(s, &ynorm));
  AKA_CHECK(SNESGetConvergedReason(s, &reason));
  AKA_CHECK(SNESGetConvergedReasonString(s, &rs));
  AKA_CHECK(SNESGetConvergenceHistory(s, &a, &its, &na));
  AKA_CHECK(SNESGetTolerances(s, &atol, &rtol, &stol, &maxit, &maxf));
  AKA_CHECK(SNESGetDivergenceTolerance(s, &divtol));
  rep->iterations = nit;
  rep->history_len = na;
  rep->reason = (int)reason;
  rep->function_norm = fnorm;
  rep->solution_norm = xnorm;
  rep->update_norm = ynorm;
  rep->atol = atol;
  rep->rtol = rtol;
  rep->stol = stol;
  rep->divtol = divtol;
  rep->max_it = maxit;
  rep->max_f = maxf;
  strncpy(rep->reason_string, rs ? rs : "", sizeof rep->reason_string - 1);
  rep->reason_string[sizeof rep->reason_string - 1] = '\0';
fail:
  detach = aka_snes_detach(s);
  return (int)(ierr != PETSC_SUCCESS ? ierr : detach);
}

/* ------------------------------------------------------------------------ */
/* TAO (NonLinearSolverTAO)                                                  */
/* ------------------------------------------------------------------------ */

static PetscErrorCode aka_tao_objective(Tao tao, Vec x, PetscReal *obj,
                                        Vec g, void *ctx) {
  struct aka_native_user *u = (struct aka_native_user *)ctx;
  const PetscScalar *rx = NULL;
  PetscScalar *wg = NULL;
  double o = 0;
  int status;
  (void)tao;
  PetscCall(aka_check_local(u, x));
  PetscCall(aka_check_local(u, g));
  PetscCall(aka_alias_trial(u, x));
  PetscCall(VecGetArrayRead(x, &rx));
  PetscCall(VecGetArray(g, &wg));
  status = u->objective(u->user, u->n, (const double *)rx, &o, (double *)wg);
  PetscCall(VecRestoreArray(g, &wg));
  PetscCall(VecRestoreArrayRead(x, &rx));
  *obj = (PetscReal)o;
  if (status != 0)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_USER,
            "akantu objective callback failed (error stored by akantu)");
  return PETSC_SUCCESS;
}

static PetscErrorCode aka_tao_hessian(Tao tao, Vec x, Mat H, Mat P,
                                      void *ctx) {
  struct aka_native_user *u = (struct aka_native_user *)ctx;
  const PetscScalar *rx = NULL;
  int status;
  (void)tao;
  (void)H;
  (void)P;
  PetscCall(aka_check_local(u, x));
  PetscCall(aka_alias_trial(u, x));
  PetscCall(VecGetArrayRead(x, &rx));
  status = u->jacobian(u->user, u->n, (const double *)rx);
  PetscCall(VecRestoreArrayRead(x, &rx));
  if (status != 0)
    SETERRQ(PETSC_COMM_SELF, PETSC_ERR_USER,
            "akantu Hessian callback failed (error stored by akantu)");
  return PETSC_SUCCESS;
}

int akantu_petsc_tao_create(MPI_Comm comm, const char *name, void **out) {
  Tao tao = NULL;
  PetscErrorCode ierr = PETSC_SUCCESS;
  AKA_CHECK(TaoCreate(comm, &tao));
  AKA_CHECK(PetscObjectSetName((PetscObject)tao, name));
  AKA_CHECK(TaoSetType(tao, TAOGPCG));
  AKA_CHECK(TaoSetFromOptions(tao));
  *out = (void *)tao;
  return 0;
fail:
  TaoDestroy(&tao);
  return (int)ierr;
}

int akantu_petsc_tao_destroy(void *tao) {
  Tao t = (Tao)tao;
  if (!t)
    return 0;
  return (int)TaoDestroy(&t);
}

int akantu_petsc_tao_set_from_options(void *tao, void *opts) {
  Tao t = (Tao)tao;
  if (opts) {
    PetscErrorCode ierr;
    AKA_TRY(PetscOptionsPush((PetscOptions)opts));
    ierr = TaoSetFromOptions(t);
    AKA_TRY(PetscOptionsPop());
    AKA_TRY(ierr);
  }
  return (int)TaoSetFromOptions(t);
}

/* Mirrored by `akantu_linalg::petsc::ffi::TaoReport` (#[repr(C)]). */
struct akantu_tao_report {
  int64_t iterations;
  int64_t max_it;
  double gatol;
  double grtol;
  double gttol;
  double fval;  /* TaoGetSolutionStatus: final objective */
  double gnorm; /* TaoGetSolutionStatus: final (projected) gradient norm */
  int reason;
};

/* NonLinearSolverTAO::solve, native part: x zeroed, solution, objective /
 * gradient and Hessian routines set, optional variable bounds (owned arrays
 * of the increment bounds), TaoSolve, reason and iteration count read
 * back. Collective. */
static PetscErrorCode aka_tao_detached_objective(Tao tao, Vec x, PetscReal *obj,
                                                 Vec g, void *ctx) {
  (void)tao;
  (void)x;
  (void)obj;
  (void)g;
  (void)ctx;
  SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ORDER,
          "akantu-rs: TAO objective called outside akantu_petsc_tao_solve");
}

static PetscErrorCode aka_tao_detached_hessian(Tao tao, Vec x, Mat H, Mat P,
                                               void *ctx) {
  (void)tao;
  (void)x;
  (void)H;
  (void)P;
  (void)ctx;
  SETERRQ(PETSC_COMM_SELF, PETSC_ERR_ORDER,
          "akantu-rs: TAO Hessian called outside akantu_petsc_tao_solve");
}

int akantu_petsc_tao_solve(void *tao, void *mat, void *x, const double *lower,
                           const double *upper, aka_objective_fn objective,
                           aka_jacobian_fn hessian, void *user, int64_t n,
                           int alias, struct akantu_tao_report *rep) {
  Tao t = (Tao)tao;
  struct aka_native_user u;
  TaoConvergedReason reason = TAO_CONTINUE_ITERATING;
  PetscInt nit = 0, maxit = 0;
  PetscReal ga = 0, gr = 0, gt = 0, fval = 0, gnorm = 0;
  Vec lb = NULL, ub = NULL;
  PetscErrorCode ierr = PETSC_SUCCESS, detach;
  u.user = user;
  u.residual = NULL;
  u.jacobian = hessian;
  u.objective = objective;
  u.n = n;
  u.xsol = (Vec)x;
  u.rvec = NULL;
  u.alias = alias;
  AKA_TRY(VecZeroEntries((Vec)x));
  AKA_TRY(TaoSetSolution(t, (Vec)x));
  AKA_CHECK(TaoSetObjectiveAndGradient(t, NULL, aka_tao_objective, &u));
  AKA_CHECK(TaoSetHessian(t, (Mat)mat, (Mat)mat, aka_tao_hessian, &u));
  if (lower && upper) {
    PetscScalar *w = NULL;
    AKA_CHECK(VecDuplicate((Vec)x, &lb));
    AKA_CHECK(VecDuplicate((Vec)x, &ub));
    AKA_CHECK(VecGetArray(lb, &w));
    for (int64_t i = 0; i < n; ++i)
      w[i] = lower[i];
    AKA_CHECK(VecRestoreArray(lb, &w));
    AKA_CHECK(VecGetArray(ub, &w));
    for (int64_t i = 0; i < n; ++i)
      w[i] = upper[i];
    AKA_CHECK(VecRestoreArray(ub, &w));
    AKA_CHECK(TaoSetVariableBounds(t, lb, ub));
  }
  AKA_CHECK(TaoSolve(t));
  AKA_CHECK(TaoGetConvergedReason(t, &reason));
  AKA_CHECK(TaoGetIterationNumber(t, &nit));
  AKA_CHECK(TaoGetTolerances(t, &ga, &gr, &gt));
  AKA_CHECK(TaoGetMaximumIterations(t, &maxit));
  /* The final objective and gradient norm, so that a
   * not-converged solve reports meaningful values (upstream's exception has
   * reason, iterations and tolerances only). */
  AKA_CHECK(TaoGetSolutionStatus(t, NULL, &fval, &gnorm, NULL, NULL, NULL));
  rep->iterations = nit;
  rep->reason = (int)reason;
  rep->gatol = ga;
  rep->grtol = gr;
  rep->gttol = gt;
  rep->fval = fval;
  rep->gnorm = gnorm;
  rep->max_it = maxit;
fail: /* bounds freed and the frame's context detached on every path;
       * TAO keeps its own references to the bounds */
  VecDestroy(&lb);
  VecDestroy(&ub);
  detach = TaoSetObjectiveAndGradient(t, NULL, aka_tao_detached_objective, NULL);
  if (detach == PETSC_SUCCESS)
    detach = TaoSetHessian(t, NULL, NULL, aka_tao_detached_hessian, NULL);
  return (int)(ierr != PETSC_SUCCESS ? ierr : detach);
}
