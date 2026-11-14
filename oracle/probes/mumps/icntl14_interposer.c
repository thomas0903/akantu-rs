/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* akantu-rs oracle helper (UD-216): LD_PRELOAD wrapper of MUMPS's
 * dmumps_c that sets ICNTL(14) to $AKANTU_ICNTL14 before the first analysis
 * of every instance whose ICNTL(14) still has the MUMPS default (20), so an
 * unmodified Akantu library takes its out-of-workspace retry path
 * (src/solver/sparse_solver_mumps.cc:336-347). Upstream's own +10 increments
 * are left alone (the value is no longer 20). No upstream counterpart. */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <dmumps_c.h>
#include <stdio.h>
#include <stdlib.h>

void dmumps_c(DMUMPS_STRUC_C *id) {
  static void (*real)(DMUMPS_STRUC_C *) = NULL;
  if (!real) {
    real = (void (*)(DMUMPS_STRUC_C *))dlsym(RTLD_NEXT, "dmumps_c");
  }
  const char *v = getenv("AKANTU_ICNTL14");
  if (v && id->job == 1 && id->icntl[13] == 20) {
    id->icntl[13] = atoi(v);
  }
  real(id);
  if (getenv("AKANTU_ICNTL14_LOG") && (id->job == 2 || id->job == 1 || id->job == 3)) {
    fprintf(stderr, "[interposer] job=%d icntl14=%d infog1=%d\n", id->job, id->icntl[13],
            id->infog[0]);
  }
}
