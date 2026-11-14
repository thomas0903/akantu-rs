# Development-machine records

Measured on the development Mac (Apple M4 Pro, 10 performance and 4
efficiency cores, 24 GB) in its aarch64 Linux VM, on 2026-11-10. Records
of the development; no conclusion of `docs/BENCHMARKS.md` rests on them,
and they are not compared with the x86_64 data.

`cpp-baseline-m4-2026-11-10/`: the C++ reference alone (harness of
`benchmarks/cohesive-perf/`, a `RelWithDebInfo` build with Akantu's
assertions on, FMA contraction and NEON Eigen), the aarch64 Gmsh meshes
(h = 0.06: 177,570 tetrahedra; h = 0.04: 558,455), 3 repetitions per point,
legacy provenance (binary digest per run, no build record).

- `baseline/`: before insertion, h = 0.12 and 0.06 (500 steps) and 0.04
  (300 steps), cohesive and elastic, 1 / 2 / 4 / 8 ranks; 77 runs including
  5 attempts rejected on upstream's MUMPS retry path (UD-216).
- `full-schedule/`: h = 0.12, 5,000 steps, upstream's behaviour and the JOSS
  emulation, 1 / 2 / 4 / 8 ranks.

The other development records of the appendix have their data elsewhere:
the MUMPS sizing series in `fixtures/solver/mumps/scaling-20261111.tsv`;
the mesh set-up scaling and the first MPI latency series only as numbers in
`docs/BENCHMARKS.md`.
