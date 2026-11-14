/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* akantu-rs MUMPS shim. Ported from Akantu (EPFL/LSMS):
 * src/solver/sparse_solver_mumps.cc (the DMUMPS_STRUC_C fields upstream
 * sets: sym, par, comm_fortran, icntl, n, nz_loc/irn_loc/jcn_loc/a_loc, rhs,
 * job; info/infog for error handling).
 *
 * The DMUMPS_STRUC_C layout depends on the MUMPS version and build options,
 * so Rust never sees it: the structure lives on the C heap behind an opaque
 * pointer and is accessed only through these functions, compiled against the
 * installed dmumps_c.h (Debian libmumps-headers-dev 5.7.3 in the test image).
 * Index arguments are 1-based like the MUMPS documentation (ICNTL(i) is
 * icntl[i-1]); the Rust side validates every index before calling.
 * Compiled by crates/akantu-linalg/build.rs (feature `mumps`). */
/* dlsym/RTLD_DEFAULT (BLAS provenance below). */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <dmumps_c.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

/* The Rust side passes `int` coordinates and an `int64_t` NNZ_loc: a
 * 64-bit-index MUMPS (-DINTSIZE64) needs other
 * coordinate pointers, refused at build time instead of at run time. */
_Static_assert(sizeof(MUMPS_INT) == sizeof(int),
               "akantu-rs MUMPS shim: MUMPS_INT must be a 32-bit int");
_Static_assert(sizeof(MUMPS_INT8) == sizeof(int64_t),
               "akantu-rs MUMPS shim: MUMPS_INT8 must be 64-bit");

typedef struct akantu_mumps {
  DMUMPS_STRUC_C id;
} akantu_mumps;

/* Zero-initialised instance (NULL on allocation failure). */
akantu_mumps *akantu_mumps_alloc(void) {
  return (akantu_mumps *)calloc(1, sizeof(akantu_mumps));
}

void akantu_mumps_free(akantu_mumps *m) { free(m); }

/* Fields read by JOB = -1 (upstream SparseSolverMumps::initialize). */
void akantu_mumps_set_init(akantu_mumps *m, int comm_fortran, int sym,
                           int par) {
  m->id.comm_fortran = (MUMPS_INT)comm_fortran;
  m->id.sym = (MUMPS_INT)sym;
  m->id.par = (MUMPS_INT)par;
}

/* Run one MUMPS job (dmumps_c); returns INFOG(1), identical on every
 * process of the instance's communicator. */
int akantu_mumps_job(akantu_mumps *m, int job) {
  m->id.job = (MUMPS_INT)job;
  dmumps_c(&m->id);
  return (int)m->id.infog[0];
}

void akantu_mumps_set_icntl(akantu_mumps *m, int i, int v) {
  m->id.icntl[i - 1] = (MUMPS_INT)v;
}
int akantu_mumps_get_icntl(const akantu_mumps *m, int i) {
  return (int)m->id.icntl[i - 1];
}
void akantu_mumps_set_cntl(akantu_mumps *m, int i, double v) {
  m->id.cntl[i - 1] = v;
}
double akantu_mumps_get_cntl(const akantu_mumps *m, int i) {
  return m->id.cntl[i - 1];
}
int akantu_mumps_infog(const akantu_mumps *m, int i) {
  return (int)m->id.infog[i - 1];
}
double akantu_mumps_rinfog(const akantu_mumps *m, int i) {
  return m->id.rinfog[i - 1];
}

/* Distributed assembled matrix (ICNTL(18) = 3, upstream initMumpsData /
 * factorize): order n (significant on the host), this process's nnz_loc
 * entries with 1-based coordinates and, for the factorisation, values. The
 * 64-bit NNZ_loc is set (and the 32-bit NZ_loc of upstream when it fits;
 * MUMPS 5.7 reads NNZ_loc). Pointers may be NULL when nnz_loc == 0 or (for
 * a) outside the factorisation; MUMPS does not write through them. */
void akantu_mumps_set_matrix(akantu_mumps *m, int n, int64_t nnz_loc,
                             const int *irn_loc, const int *jcn_loc,
                             const double *a_loc) {
  m->id.n = (MUMPS_INT)n;
  m->id.nz_alloc = 0;
  m->id.nnz_loc = (MUMPS_INT8)nnz_loc;
  m->id.nz_loc = (nnz_loc <= INT32_MAX) ? (MUMPS_INT)nnz_loc : 0;
  m->id.irn_loc = (MUMPS_INT *)irn_loc;
  m->id.jcn_loc = (MUMPS_INT *)jcn_loc;
  m->id.a_loc = (DMUMPS_COMPLEX *)a_loc;
}

/* Forget the matrix pointers after a job (order and counts are kept: MUMPS
 * checks N at later jobs). */
void akantu_mumps_clear_matrix(akantu_mumps *m) {
  m->id.irn_loc = NULL;
  m->id.jcn_loc = NULL;
  m->id.a_loc = NULL;
}

/* Centralised dense right-hand side (ICNTL(20) = 0, ICNTL(21) = 0): on the
 * host, n entries overwritten by the solution; NULL elsewhere. */
void akantu_mumps_set_rhs(akantu_mumps *m, double *rhs, int n) {
  m->id.rhs = (DMUMPS_COMPLEX *)rhs;
  m->id.nrhs = 1;
  m->id.lrhs = (MUMPS_INT)n;
}

/* NUL-terminated version string (valid after JOB = -1). */
const char *akantu_mumps_version(const akantu_mumps *m) {
  return m->id.version_number;
}

/* Bytes of MUMPS_INT (4 unless MUMPS was built with -DINTSIZE64). */
int akantu_mumps_int_bytes(void) { return (int)sizeof(MUMPS_INT); }

/* BLAS provenance (D-050): the OpenBLAS core type and thread
 * count actually in use by the BLAS that MUMPS links (OpenBLAS selects its
 * kernels at run time unless OPENBLAS_CORETYPE pins them). Looked up in the
 * process's global scope; NULL / -1 when the BLAS is not OpenBLAS. */
const char *akantu_mumps_blas_corename(void) {
  char *(*f)(void) = (char *(*)(void))dlsym(RTLD_DEFAULT, "openblas_get_corename");
  return f ? f() : NULL;
}
int akantu_mumps_blas_threads(void) {
  int (*f)(void) = (int (*)(void))dlsym(RTLD_DEFAULT, "openblas_get_num_threads");
  return f ? f() : -1;
}
