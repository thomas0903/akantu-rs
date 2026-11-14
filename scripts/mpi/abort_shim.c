/* SPDX-License-Identifier: LGPL-3.0-or-later */
/* akantu-rs MPI test support (no upstream counterpart): a PMPI interception
 * of MPI_Abort, LD_PRELOADed into the mpi_abort_paths scenarios by
 * scripts/mpi/run-mpi-tier.sh.
 * It records, on stderr, the calling thread's MPI
 * main-thread status and the provided thread level, then performs the real
 * abort (PMPI_Abort). scripts/mpi/check_evidence.py rejects any record with
 * thread_main=0 under MPI_THREAD_SINGLE/FUNNELED (provided < 2).
 *
 * The rank is read from the RANK environment variables of the launcher (no
 * further MPI call is made from a possibly non-permitted thread). */
#include <mpi.h>
#include <stdio.h>
#include <stdlib.h>

int MPI_Abort(MPI_Comm comm, int errorcode) {
  int is_main = -1, provided = -1;
  /* Both are callable from any thread (MPI-4.1 §11.6). */
  PMPI_Is_thread_main(&is_main);
  PMPI_Query_thread(&provided);
  const char *rank = getenv("OMPI_COMM_WORLD_RANK");
  fprintf(stderr,
          "ABORT_INTERCEPT rank=%s thread_main=%d provided=%d errorcode=%d\n",
          rank ? rank : "?", is_main, provided, errorcode);
  fflush(stderr);
  return PMPI_Abort(comm, errorcode);
}
