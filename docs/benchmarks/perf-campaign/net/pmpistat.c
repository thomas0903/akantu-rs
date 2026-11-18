// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: a PMPI interposer (LD_PRELOAD) that counts the MPI calls of a
// process: calls, bytes of point-to-point sends, time inside each call, and
// the lifetime of non-blocking requests (start to the completing
// MPI_Test/MPI_Wait). Written at MPI_Finalize to summary_rank<r>.pmpi.json in
// the working directory (r = OMPI_COMM_WORLD_RANK). Diagnostic only.
#define _GNU_SOURCE
#include <mpi.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <execinfo.h>

// call-site sampling (env PMPI_BT = an op name, e.g. Iallgather): the call stack of
// every such call, counted per distinct stack, written to summary_rank<r>.pmpi-bt.txt
#define BTN 4096
#define BTD 40
static int bt_op = -1;
static struct { uint64_t n; int d; void *f[BTD]; } bts[BTN];
static void bt_sample(void) {
  void *f[BTD];
  int d = backtrace(f, BTD);
  uint64_t h = 1469598103934665603ull;
  for (int i = 0; i < d; i++) h = (h ^ (uintptr_t)f[i]) * 1099511628211ull;
  size_t i = h % BTN;
  for (size_t k = 0; k < BTN; k++, i = (i + 1) % BTN) {
    if (bts[i].n == 0) { bts[i].d = d; memcpy(bts[i].f, f, sizeof(void *) * (size_t)d); bts[i].n = 1; return; }
    if (bts[i].d == d && !memcmp(bts[i].f, f, sizeof(void *) * (size_t)d)) { bts[i].n++; return; }
  }
}

#define OPS(X) X(Init) X(Allreduce) X(Reduce) X(Bcast) X(Allgather) X(Allgatherv) X(Gather) \
  X(Gatherv) X(Scatter) X(Scatterv) X(Alltoall) X(Alltoallv) X(Barrier) X(Scan) X(Exscan)        \
  X(Send) X(Ssend) X(Recv) X(Sendrecv) X(Probe) X(Iprobe) X(Improbe) X(Mprobe) X(Mrecv) X(Imrecv) \
  X(Isend) X(Issend) X(Irecv) X(Iallgather) X(Iallgatherv) X(Igatherv) X(Iscatter) X(Iscatterv)   \
  X(Ibcast) X(Ibarrier) X(Iallreduce) X(Comm_idup) X(Comm_split) X(Comm_dup)                       \
  X(Test) X(Testall) X(Testany) X(Testsome) X(Wait) X(Waitall) X(Waitany) X(Waitsome)

#define ENUM(n) OP_##n,
enum { OPS(ENUM) NOPS };
#define NAME(n) #n,
static const char *names[NOPS] = {OPS(NAME)};

static uint64_t cnt[NOPS], nsec[NOPS], byts[NOPS], hits[NOPS];
static uint64_t life_cnt[NOPS], life_ns[NOPS], life_max[NOPS];
static uint64_t hist[NOPS][5];  // bytes: <=256, <=4K, <=64K, <=1M, >1M
static uint64_t t_init;

static inline uint64_t now(void) {
  struct timespec ts;
  clock_gettime(CLOCK_MONOTONIC, &ts);
  return (uint64_t)ts.tv_sec * 1000000000ull + (uint64_t)ts.tv_nsec;
}

#define TAB 262144
struct ent { MPI_Request r; int op; uint64_t t0; };
static struct ent tab[TAB];
static inline size_t slot(MPI_Request r) { return ((uintptr_t)r >> 4) * 2654435761u % TAB; }
static void track(MPI_Request r, int op, uint64_t t0) {
  if (r == MPI_REQUEST_NULL) return;
  size_t i = slot(r);
  for (size_t k = 0; k < TAB; k++, i = (i + 1) % TAB)
    if (tab[i].r == NULL || tab[i].r == r) { tab[i].r = r; tab[i].op = op; tab[i].t0 = t0; return; }
}
static void done(MPI_Request r, uint64_t t1) {
  if (r == MPI_REQUEST_NULL) return;
  size_t i = slot(r);
  for (size_t k = 0; k < TAB; k++, i = (i + 1) % TAB) {
    if (tab[i].r == NULL) return;
    if (tab[i].r == r) {
      int op = tab[i].op;
      uint64_t d = t1 - tab[i].t0;
      life_cnt[op]++; life_ns[op] += d; if (d > life_max[op]) life_max[op] = d;
      // backward-shift deletion
      size_t j = i;
      for (;;) {
        tab[j].r = NULL;
        size_t m = j;
        for (;;) {
          m = (m + 1) % TAB;
          if (tab[m].r == NULL) return;
          size_t h = slot(tab[m].r);
          if ((j <= m) ? (h <= j || h > m) : (h <= j && h > m)) break;
        }
        tab[j] = tab[m];
        j = m;
      }
    }
  }
}
static inline void sized(int op, MPI_Datatype dt, int n) {
  int s = 0;
  PMPI_Type_size(dt, &s);
  uint64_t b = (uint64_t)s * (uint64_t)(n < 0 ? 0 : n);
  byts[op] += b;
  hist[op][b <= 256 ? 0 : b <= 4096 ? 1 : b <= 65536 ? 2 : b <= 1048576 ? 3 : 4]++;
}
#define T0 uint64_t t0 = now()
#define T1(op) do { cnt[op]++; nsec[op] += now() - t0; if ((op) == bt_op) bt_sample(); } while (0)

static void bt_init(void);
int MPI_Init(int *a, char ***b) { bt_init(); T0; int rc = PMPI_Init(a, b); T1(OP_Init); t_init = now(); return rc; }
int MPI_Init_thread(int *a, char ***b, int r, int *p) {
  bt_init();
  T0; int rc = PMPI_Init_thread(a, b, r, p); T1(OP_Init); t_init = now(); return rc;
}
static void bt_init(void) {
  const char *e = getenv("PMPI_BT");
  if (e) for (int i = 0; i < NOPS; i++) if (!strcmp(e, names[i])) bt_op = i;
}
static void bt_write(const char *rk) {
  if (bt_op < 0) return;
  char path[256];
  snprintf(path, sizeof path, "summary_rank%s.pmpi-bt.txt", rk ? rk : "x");
  FILE *f = fopen(path, "w");
  if (!f) return;
  for (size_t i = 0; i < BTN; i++) {
    if (!bts[i].n) continue;
    fprintf(f, "== %llu\n", (unsigned long long)bts[i].n);
    char **sy = backtrace_symbols(bts[i].f, bts[i].d);
    for (int k = 0; k < bts[i].d; k++) fprintf(f, "%s\n", sy ? sy[k] : "?");
    free(sy);
  }
  fclose(f);
}
int MPI_Finalize(void) {
  bt_write(getenv("OMPI_COMM_WORLD_RANK"));
  uint64_t t = now();
  const char *rk = getenv("OMPI_COMM_WORLD_RANK");
  char path[256];
  snprintf(path, sizeof path, "summary_rank%s.pmpi.json", rk ? rk : "x");
  FILE *f = fopen(path, "w");
  if (f) {
    fprintf(f, "{\"rank\": %s, \"wall_ns\": %llu, \"ops\": {", rk ? rk : "-1",
            (unsigned long long)(t - t_init));
    int first = 1;
    for (int i = 0; i < NOPS; i++) {
      if (!cnt[i] && !life_cnt[i]) continue;
      fprintf(f, "%s\n \"%s\": {\"calls\": %llu, \"ns\": %llu, \"bytes\": %llu, \"hits\": %llu, "
              "\"life_n\": %llu, \"life_ns\": %llu, \"life_max_ns\": %llu, \"hist\": [%llu, %llu, %llu, %llu, %llu]}",
              first ? "" : ",", names[i], (unsigned long long)cnt[i], (unsigned long long)nsec[i],
              (unsigned long long)byts[i], (unsigned long long)hits[i], (unsigned long long)life_cnt[i],
              (unsigned long long)life_ns[i], (unsigned long long)life_max[i],
              (unsigned long long)hist[i][0], (unsigned long long)hist[i][1], (unsigned long long)hist[i][2],
              (unsigned long long)hist[i][3], (unsigned long long)hist[i][4]);
      first = 0;
    }
    fprintf(f, "}}\n");
    fclose(f);
  }
  return PMPI_Finalize();
}

#define COLL(NAME, PROTO, ARGS) \
  int MPI_##NAME PROTO { T0; int rc = PMPI_##NAME ARGS; T1(OP_##NAME); return rc; }
COLL(Allreduce, (const void *s, void *r, int n, MPI_Datatype d, MPI_Op o, MPI_Comm c), (s, r, n, d, o, c))
COLL(Reduce, (const void *s, void *r, int n, MPI_Datatype d, MPI_Op o, int root, MPI_Comm c), (s, r, n, d, o, root, c))
COLL(Bcast, (void *b, int n, MPI_Datatype d, int root, MPI_Comm c), (b, n, d, root, c))
COLL(Allgather, (const void *s, int sn, MPI_Datatype sd, void *r, int rn, MPI_Datatype rd, MPI_Comm c), (s, sn, sd, r, rn, rd, c))
COLL(Allgatherv, (const void *s, int sn, MPI_Datatype sd, void *r, const int rn[], const int di[], MPI_Datatype rd, MPI_Comm c), (s, sn, sd, r, rn, di, rd, c))
COLL(Gather, (const void *s, int sn, MPI_Datatype sd, void *r, int rn, MPI_Datatype rd, int root, MPI_Comm c), (s, sn, sd, r, rn, rd, root, c))
COLL(Gatherv, (const void *s, int sn, MPI_Datatype sd, void *r, const int rn[], const int di[], MPI_Datatype rd, int root, MPI_Comm c), (s, sn, sd, r, rn, di, rd, root, c))
COLL(Scatter, (const void *s, int sn, MPI_Datatype sd, void *r, int rn, MPI_Datatype rd, int root, MPI_Comm c), (s, sn, sd, r, rn, rd, root, c))
COLL(Scatterv, (const void *s, const int sn[], const int di[], MPI_Datatype sd, void *r, int rn, MPI_Datatype rd, int root, MPI_Comm c), (s, sn, di, sd, r, rn, rd, root, c))
COLL(Alltoall, (const void *s, int sn, MPI_Datatype sd, void *r, int rn, MPI_Datatype rd, MPI_Comm c), (s, sn, sd, r, rn, rd, c))
COLL(Alltoallv, (const void *s, const int sn[], const int sdi[], MPI_Datatype sd, void *r, const int rn[], const int rdi[], MPI_Datatype rd, MPI_Comm c), (s, sn, sdi, sd, r, rn, rdi, rd, c))
COLL(Barrier, (MPI_Comm c), (c))
COLL(Scan, (const void *s, void *r, int n, MPI_Datatype d, MPI_Op o, MPI_Comm c), (s, r, n, d, o, c))
COLL(Exscan, (const void *s, void *r, int n, MPI_Datatype d, MPI_Op o, MPI_Comm c), (s, r, n, d, o, c))
COLL(Comm_split, (MPI_Comm c, int col, int key, MPI_Comm *n), (c, col, key, n))
COLL(Comm_dup, (MPI_Comm c, MPI_Comm *n), (c, n))
COLL(Recv, (void *b, int n, MPI_Datatype d, int s, int t, MPI_Comm c, MPI_Status *st), (b, n, d, s, t, c, st))
COLL(Probe, (int s, int t, MPI_Comm c, MPI_Status *st), (s, t, c, st))
COLL(Mprobe, (int s, int t, MPI_Comm c, MPI_Message *m, MPI_Status *st), (s, t, c, m, st))
COLL(Mrecv, (void *b, int n, MPI_Datatype d, MPI_Message *m, MPI_Status *st), (b, n, d, m, st))

int MPI_Send(const void *b, int n, MPI_Datatype d, int de, int t, MPI_Comm c) {
  T0; int rc = PMPI_Send(b, n, d, de, t, c); T1(OP_Send); sized(OP_Send, d, n); return rc;
}
int MPI_Ssend(const void *b, int n, MPI_Datatype d, int de, int t, MPI_Comm c) {
  T0; int rc = PMPI_Ssend(b, n, d, de, t, c); T1(OP_Ssend); sized(OP_Ssend, d, n); return rc;
}
int MPI_Sendrecv(const void *sb, int sn, MPI_Datatype sd, int de, int st, void *rb, int rn, MPI_Datatype rd,
                 int so, int rt, MPI_Comm c, MPI_Status *s) {
  T0; int rc = PMPI_Sendrecv(sb, sn, sd, de, st, rb, rn, rd, so, rt, c, s); T1(OP_Sendrecv);
  sized(OP_Sendrecv, sd, sn); return rc;
}
int MPI_Iprobe(int s, int t, MPI_Comm c, int *f, MPI_Status *st) {
  T0; int rc = PMPI_Iprobe(s, t, c, f, st); T1(OP_Iprobe); if (*f) hits[OP_Iprobe]++; return rc;
}
int MPI_Improbe(int s, int t, MPI_Comm c, int *f, MPI_Message *m, MPI_Status *st) {
  T0; int rc = PMPI_Improbe(s, t, c, f, m, st); T1(OP_Improbe); if (*f) hits[OP_Improbe]++; return rc;
}

#define NB(NAME, PROTO, ARGS, REQ) \
  int MPI_##NAME PROTO { T0; int rc = PMPI_##NAME ARGS; T1(OP_##NAME); track(*REQ, OP_##NAME, t0); return rc; }
int MPI_Isend(const void *b, int n, MPI_Datatype d, int de, int t, MPI_Comm c, MPI_Request *r) {
  T0; int rc = PMPI_Isend(b, n, d, de, t, c, r); T1(OP_Isend); sized(OP_Isend, d, n); track(*r, OP_Isend, t0); return rc;
}
int MPI_Issend(const void *b, int n, MPI_Datatype d, int de, int t, MPI_Comm c, MPI_Request *r) {
  T0; int rc = PMPI_Issend(b, n, d, de, t, c, r); T1(OP_Issend); sized(OP_Issend, d, n); track(*r, OP_Issend, t0); return rc;
}
int MPI_Irecv(void *b, int n, MPI_Datatype d, int s, int t, MPI_Comm c, MPI_Request *r) {
  T0; int rc = PMPI_Irecv(b, n, d, s, t, c, r); T1(OP_Irecv); sized(OP_Irecv, d, n); track(*r, OP_Irecv, t0); return rc;
}
int MPI_Imrecv(void *b, int n, MPI_Datatype d, MPI_Message *m, MPI_Request *r) {
  T0; int rc = PMPI_Imrecv(b, n, d, m, r); T1(OP_Imrecv); sized(OP_Imrecv, d, n); track(*r, OP_Imrecv, t0); return rc;
}
NB(Iallgather, (const void *s, int sn, MPI_Datatype sd, void *rv, int rn, MPI_Datatype rd, MPI_Comm c, MPI_Request *r), (s, sn, sd, rv, rn, rd, c, r), r)
NB(Iallgatherv, (const void *s, int sn, MPI_Datatype sd, void *rv, const int rn[], const int di[], MPI_Datatype rd, MPI_Comm c, MPI_Request *r), (s, sn, sd, rv, rn, di, rd, c, r), r)
NB(Igatherv, (const void *s, int sn, MPI_Datatype sd, void *rv, const int rn[], const int di[], MPI_Datatype rd, int root, MPI_Comm c, MPI_Request *r), (s, sn, sd, rv, rn, di, rd, root, c, r), r)
NB(Iscatter, (const void *s, int sn, MPI_Datatype sd, void *rv, int rn, MPI_Datatype rd, int root, MPI_Comm c, MPI_Request *r), (s, sn, sd, rv, rn, rd, root, c, r), r)
NB(Iscatterv, (const void *s, const int sn[], const int di[], MPI_Datatype sd, void *rv, int rn, MPI_Datatype rd, int root, MPI_Comm c, MPI_Request *r), (s, sn, di, sd, rv, rn, rd, root, c, r), r)
NB(Ibcast, (void *b, int n, MPI_Datatype d, int root, MPI_Comm c, MPI_Request *r), (b, n, d, root, c, r), r)
NB(Ibarrier, (MPI_Comm c, MPI_Request *r), (c, r), r)
NB(Iallreduce, (const void *s, void *rv, int n, MPI_Datatype d, MPI_Op o, MPI_Comm c, MPI_Request *r), (s, rv, n, d, o, c, r), r)
NB(Comm_idup, (MPI_Comm c, MPI_Comm *n, MPI_Request *r), (c, n, r), r)

int MPI_Test(MPI_Request *r, int *f, MPI_Status *s) {
  MPI_Request q = *r;
  T0; int rc = PMPI_Test(r, f, s); uint64_t t1 = now(); cnt[OP_Test]++; nsec[OP_Test] += t1 - t0;
  if (*f) { hits[OP_Test]++; done(q, t1); }
  return rc;
}
int MPI_Wait(MPI_Request *r, MPI_Status *s) {
  MPI_Request q = *r;
  T0; int rc = PMPI_Wait(r, s); uint64_t t1 = now(); cnt[OP_Wait]++; nsec[OP_Wait] += t1 - t0;
  done(q, t1);
  return rc;
}
#define MAXQ 4096
int MPI_Waitall(int n, MPI_Request r[], MPI_Status s[]) {
  MPI_Request q[MAXQ]; int m = n < MAXQ ? n : MAXQ;
  memcpy(q, r, sizeof(MPI_Request) * (size_t)m);
  T0; int rc = PMPI_Waitall(n, r, s); uint64_t t1 = now(); cnt[OP_Waitall]++; nsec[OP_Waitall] += t1 - t0;
  for (int i = 0; i < m; i++) done(q[i], t1);
  return rc;
}
int MPI_Testall(int n, MPI_Request r[], int *f, MPI_Status s[]) {
  MPI_Request q[MAXQ]; int m = n < MAXQ ? n : MAXQ;
  memcpy(q, r, sizeof(MPI_Request) * (size_t)m);
  T0; int rc = PMPI_Testall(n, r, f, s); uint64_t t1 = now(); cnt[OP_Testall]++; nsec[OP_Testall] += t1 - t0;
  if (*f) { hits[OP_Testall]++; for (int i = 0; i < m; i++) done(q[i], t1); }
  return rc;
}
int MPI_Waitany(int n, MPI_Request r[], int *idx, MPI_Status *s) {
  MPI_Request q[MAXQ]; int m = n < MAXQ ? n : MAXQ;
  memcpy(q, r, sizeof(MPI_Request) * (size_t)m);
  T0; int rc = PMPI_Waitany(n, r, idx, s); uint64_t t1 = now(); cnt[OP_Waitany]++; nsec[OP_Waitany] += t1 - t0;
  if (*idx >= 0 && *idx < m) done(q[*idx], t1);
  return rc;
}
int MPI_Testany(int n, MPI_Request r[], int *idx, int *f, MPI_Status *s) {
  MPI_Request q[MAXQ]; int m = n < MAXQ ? n : MAXQ;
  memcpy(q, r, sizeof(MPI_Request) * (size_t)m);
  T0; int rc = PMPI_Testany(n, r, idx, f, s); uint64_t t1 = now(); cnt[OP_Testany]++; nsec[OP_Testany] += t1 - t0;
  if (*f && *idx >= 0 && *idx < m) { hits[OP_Testany]++; done(q[*idx], t1); }
  return rc;
}
int MPI_Waitsome(int n, MPI_Request r[], int *oc, int idx[], MPI_Status s[]) {
  MPI_Request q[MAXQ]; int m = n < MAXQ ? n : MAXQ;
  memcpy(q, r, sizeof(MPI_Request) * (size_t)m);
  T0; int rc = PMPI_Waitsome(n, r, oc, idx, s); uint64_t t1 = now(); cnt[OP_Waitsome]++; nsec[OP_Waitsome] += t1 - t0;
  for (int i = 0; i < *oc && *oc != MPI_UNDEFINED; i++) if (idx[i] < m) done(q[idx[i]], t1);
  return rc;
}
int MPI_Testsome(int n, MPI_Request r[], int *oc, int idx[], MPI_Status s[]) {
  MPI_Request q[MAXQ]; int m = n < MAXQ ? n : MAXQ;
  memcpy(q, r, sizeof(MPI_Request) * (size_t)m);
  T0; int rc = PMPI_Testsome(n, r, oc, idx, s); uint64_t t1 = now(); cnt[OP_Testsome]++; nsec[OP_Testsome] += t1 - t0;
  if (*oc > 0) hits[OP_Testsome]++;
  for (int i = 0; i < *oc && *oc != MPI_UNDEFINED; i++) if (idx[i] < m) done(q[idx[i]], t1);
  return rc;
}
