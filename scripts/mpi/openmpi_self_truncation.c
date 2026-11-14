/* SPDX-License-Identifier: LGPL-3.0-or-later
 * akantu-rs MPI tier probe (no upstream counterpart; docs/DEPENDENCIES.md): does the MPI library write
 * past a posted receive buffer when the matched message is longer than the
 * receive (truncation)? Pure C, no akantu code. Rank 0 posts
 * MPI_Irecv(count 2) into a buffer of size+64 bytes filled with 0xAA, then a
 * message of `size` bytes arrives — from rank 0 itself ("self") and, with 2
 * or more processes, from rank 1 ("cross"). Prints one line per case:
 *
 *   OMPI_TRUNCATION peer=<self|cross> size=<bytes> class=<error class> overrun=<bytes>
 *
 * `overrun` counts bytes beyond the 2 posted ones that differ from the fill
 * pattern (0 = clean). Exit 0 always: the tier records the result as
 * provenance (the port never hands MPI a receive buffer shorter than the
 * matched message). Run with `mpirun -np 1` (self) and `-np 2` over every
 * BTL of interest (cross).
 */
#include <mpi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const int sizes[] = {64, 4096, 65536, 1048576};
#define NSIZES ((int)(sizeof sizes / sizeof sizes[0]))

/* Rank 0 receives `sz` bytes from `src` into a 2-byte posted receive. */
static void receive_case(const char *peer, int src, int sz, int tag) {
  int rc, cls = MPI_SUCCESS, flag = 0, j, overrun = 0;
  unsigned char *buf = malloc((size_t)sz + 64);
  MPI_Request rq;
  memset(buf, 0xAA, (size_t)sz + 64);
  MPI_Irecv(buf, 2, MPI_UINT8_T, src, tag, MPI_COMM_WORLD, &rq);
  /* Tell the sender the receive is posted. */
  if (src != 0)
    MPI_Send(NULL, 0, MPI_UINT8_T, src, 1000 + tag, MPI_COMM_WORLD);
  else {
    unsigned char *msg = malloc((size_t)sz);
    MPI_Request sq;
    memset(msg, 0x55, (size_t)sz);
    MPI_Isend(msg, sz, MPI_UINT8_T, 0, tag, MPI_COMM_WORLD, &sq);
    do {
      rc = MPI_Test(&rq, &flag, MPI_STATUS_IGNORE);
      if (rc != MPI_SUCCESS) {
        MPI_Error_class(rc, &cls);
        break;
      }
    } while (!flag);
    MPI_Wait(&sq, MPI_STATUS_IGNORE);
    goto report; /* msg is not freed: the heap may be damaged */
  }
  do {
    rc = MPI_Test(&rq, &flag, MPI_STATUS_IGNORE);
    if (rc != MPI_SUCCESS) {
      MPI_Error_class(rc, &cls);
      break;
    }
  } while (!flag);
report:
  for (j = 2; j < sz + 64; ++j)
    overrun += buf[j] != 0xAA;
  printf("OMPI_TRUNCATION peer=%s size=%d class=%d%s overrun=%d\n", peer, sz,
         cls, cls == MPI_ERR_TRUNCATE ? "(MPI_ERR_TRUNCATE)" : "", overrun);
  fflush(stdout);
  /* Never freed: the overrun may have damaged the heap metadata. */
}

int main(int argc, char **argv) {
  int me, np, i;
  MPI_Init(&argc, &argv);
  MPI_Comm_set_errhandler(MPI_COMM_WORLD, MPI_ERRORS_RETURN);
  MPI_Comm_rank(MPI_COMM_WORLD, &me);
  MPI_Comm_size(MPI_COMM_WORLD, &np);
  for (i = 0; i < NSIZES; ++i) {
    if (me == 0)
      receive_case("self", 0, sizes[i], 7 + i);
  }
  for (i = 0; np > 1 && i < NSIZES; ++i) {
    int sz = sizes[i], tag = 20 + i;
    if (me == 0) {
      receive_case("cross", 1, sz, tag);
    } else if (me == 1) {
      unsigned char *msg = malloc((size_t)sz);
      memset(msg, 0x55, (size_t)sz);
      MPI_Recv(NULL, 0, MPI_UINT8_T, 0, 1000 + tag, MPI_COMM_WORLD,
               MPI_STATUS_IGNORE);
      MPI_Send(msg, sz, MPI_UINT8_T, 0, tag, MPI_COMM_WORLD);
      free(msg);
    }
  }
  MPI_Barrier(MPI_COMM_WORLD);
  MPI_Finalize();
  return 0;
}
