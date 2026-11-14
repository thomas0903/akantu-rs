/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* akantu-rs MPI test support (no upstream counterpart): adversarial
 * PMIx_Abort interposers for the PMIx fallback of the emergency abort.
 * LD_PRELOADed together with abort_shim.so into
 * `mpi_abort_paths worker_abort_funneled` by scripts/mpi/run-mpi-tier.sh;
 * an interposer comes first in the global scope, so the dlsym(RTLD_DEFAULT)
 * done when MPI starts resolves it. Built twice:
 *   -DPMIX_SHIM_BLOCK : never returns (a server that never answers): the
 *                       abort path must still exit 71 after its 2 s bound;
 *   -DPMIX_SHIM_ERR   : returns PMIX_ERROR (-1) at once (no server, not
 *                       connected): the process exits 71 at once.
 * Same C signature as OpenPMIx 5 pmix.h (pmix_status_t is int). */
#include <stddef.h>
#include <stdio.h>
#include <unistd.h>

int PMIx_Abort(int status, const char msg[], void *procs, size_t nprocs) {
  (void)msg; (void)procs; (void)nprocs;
#ifdef PMIX_SHIM_BLOCK
  fprintf(stderr, "PMIX_SHIM_BLOCK PMIx_Abort(%d) called, blocking\n", status);
  fflush(stderr);
  for (;;) pause();
#else
  fprintf(stderr, "PMIX_SHIM_ERR PMIx_Abort(%d) called, returning -1\n", status);
  fflush(stderr);
  return -1;
#endif
}
