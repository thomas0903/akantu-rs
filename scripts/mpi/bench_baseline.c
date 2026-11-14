/* SPDX-License-Identifier: LGPL-3.0-or-later
 * akantu-rs benchmark support (no upstream counterpart): raw-MPI baseline of
 * crates/akantu-comm/examples/mpi_bench.rs. Same measurements, plain
 * blocking MPI:
 *
 *   mpirun -np 2 bench_baseline pingpong <bytes> <iterations>
 *   mpirun -np N bench_baseline halo <bytes> <iterations>
 *
 * Prints one "BASE ..." line on rank 0 (ping-pong: half round-trip time;
 * halo: time per exchange, maximum over the ranks). */
#include <mpi.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

int main(int argc, char **argv) {
  MPI_Init(&argc, &argv);
  int me, p;
  MPI_Comm_rank(MPI_COMM_WORLD, &me);
  MPI_Comm_size(MPI_COMM_WORLD, &p);
  if (argc < 4) {
    if (me == 0) fprintf(stderr, "usage: bench_baseline pingpong|halo <bytes> <iterations>\n");
    MPI_Finalize();
    return 2;
  }
  const char *what = argv[1];
  int len = atoi(argv[2]);
  int iters = atoi(argv[3]);
  char *buf = malloc((size_t)len + 1);
  char *rbuf = malloc(((size_t)len + 1) * (size_t)p);
  memset(buf, 7, (size_t)len);
  int warm = iters / 10 + 1;
  double t0 = 0;
  if (!strcmp(what, "pingpong")) {
    if (me < 2) {
      int peer = 1 - me;
      for (int i = 0; i < iters + warm; i++) {
        if (i == warm) t0 = MPI_Wtime();
        if (me == 0) {
          MPI_Send(buf, len, MPI_BYTE, peer, 80, MPI_COMM_WORLD);
          MPI_Recv(rbuf, len, MPI_BYTE, peer, 80, MPI_COMM_WORLD, MPI_STATUS_IGNORE);
        } else {
          MPI_Recv(rbuf, len, MPI_BYTE, peer, 80, MPI_COMM_WORLD, MPI_STATUS_IGNORE);
          MPI_Send(buf, len, MPI_BYTE, peer, 80, MPI_COMM_WORLD);
        }
      }
      if (me == 0) {
        double h = (MPI_Wtime() - t0) / iters / 2;
        printf("BASE pingpong len=%d: half-RTT %.2f us, %.3f GB/s\n", len, h * 1e6, len / h / 1e9);
      }
    }
  } else {
    MPI_Request *rq = malloc(sizeof(MPI_Request) * 2 * (size_t)p);
    MPI_Barrier(MPI_COMM_WORLD);
    for (int i = 0; i < iters + warm; i++) {
      if (i == warm) {
        MPI_Barrier(MPI_COMM_WORLD);
        t0 = MPI_Wtime();
      }
      int n = 0;
      for (int q = 0; q < p; q++)
        if (q != me)
          MPI_Irecv(rbuf + (size_t)q * len, len, MPI_BYTE, q, 81, MPI_COMM_WORLD, &rq[n++]);
      for (int q = 0; q < p; q++)
        if (q != me) MPI_Isend(buf, len, MPI_BYTE, q, 81, MPI_COMM_WORLD, &rq[n++]);
      MPI_Waitall(n, rq, MPI_STATUSES_IGNORE);
    }
    double dt = MPI_Wtime() - t0, mx;
    MPI_Allreduce(&dt, &mx, 1, MPI_DOUBLE, MPI_MAX, MPI_COMM_WORLD);
    if (me == 0)
      printf("BASE halo np=%d len=%d: %.2f us per exchange (%d peers)\n", p, len, mx / iters * 1e6, p - 1);
    free(rq);
  }
  free(buf);
  free(rbuf);
  MPI_Finalize();
  return 0;
}
