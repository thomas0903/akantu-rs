# Where each performance number comes from

One row per section of `docs/BENCHMARKS.md`: the machine, the date (local
time), the weight of its numbers and the data behind them. The rule the
document follows: every comparison of the port with the C++ reference, and
every number a conclusion rests on, comes from an x86_64 cloud machine
(AWS or a boat.dev sandbox) with both codes on the same machine; the
development Mac (Apple M4 Pro, natively or in its aarch64 Linux VM) gives
records of the development only.

Weights: *headline* = a result the reports quote; *supporting* = a result a
headline or a stated conclusion rests on; *record* = a development record on
which no conclusion rests.

| Section | What is measured | Machine | Date | Port vs C++ | Weight | Data |
|---|---|---|---|---|---|---|
| Method: default against strict C++ install | the two C++ installs in the serial cohesive driver | boat `large`, AMD Ryzen 9 9950X | 2026-11-11 | C++ vs C++ | supporting | `docs/benchmarks/serial-driver-2026-11-11/` |
| The explicit step on one core | `solve_step`, internal forces, stresses; port against both C++ installs, ten configurations | boat `large`, AMD Ryzen 7 9700X | 2026-11-12 | yes | headline | `docs/benchmarks/explicit-step/final/` |
| What was optimised | interleaved A/B of each change; profiles before and after | boat `large`, AMD Ryzen 9 9950X | 2026-11-11 | strict C++ only | supporting | `docs/benchmarks/explicit-step/development/` |
| The cohesive step and the benchmark driver | the cohesive step harness; the driver before and after | boat `large`, Ryzen 9 9950X (step), Ryzen 7 9700X (driver) | 2026-11-11, 2026-11-12 | no | supporting | `explicit-step/development/`, `explicit-step/final/driver/` |
| The SparseLU translation | static solve, upstream's SparseLU against the port's translation, MUMPS for scale | boat, AMD EPYC 9455 (h = 0.06 MUMPS and `faer`: Ryzen 9 9950X) | 2026-11-11 | yes | supporting | numbers in the document only |
| MUMPS in the cohesive benchmark | h = 0.12 static solve and loop at 1 / 2 / 4 ranks; the paired static-solve ratios of the scaling series | boat `large`, AMD EPYC 9455 | 2026-11-12, 2026-11-13 | yes | supporting | `docs/benchmarks/midsize-2026-11-12/item1-h012/`, `scaling-2026-11-13/` |
| The cohesive scalability benchmark (meshes, JOSS emulation) | the C++ reference alone: the `fillNodesToElements` share and the emulation | boat `large`, AMD Ryzen 7 9700X | 2026-11-11 | no | supporting | `docs/benchmarks/cpp-baseline-x86-2026-11-11/` |
| Small and mid sizes at 1 to 8 ranks | ten points before insertion, four with insertion, 5 repetitions, paired | boat `large`, AMD EPYC 9455, 8 vCPU | 2026-11-13 | yes | headline | `docs/benchmarks/scaling-2026-11-13/` |
| The mid-size rehearsal | h = 0.06 full schedule, h = 0.04 static solve and 300 steps, 1–8 ranks | boat `large`, AMD EPYC 9455, 8 vCPU | 2026-11-12 | yes | headline | `docs/benchmarks/midsize-2026-11-12/` |
| The full-size run | JOSS mesh, 5,000 steps; port at 64 / 32 / 8 ranks, C++ at 64 / 32; one run per point | AWS `r7a.16xlarge`, AMD EPYC 9R14, 64 cores | 2026-11-12 | yes | headline | `docs/benchmarks/c1-2026-11-12/` |
| The full-size run: the port's rerun after UD-1280 | the port at 64 and 32 ranks | AWS `r7a.16xlarge` | 2026-11-13 | no | supporting | `docs/benchmarks/c1-rerun-2026-11-13/` |
| Comparison with upstream's published data | per element and step and loop efficiency against jed v5.0.4 on the same mesh | AWS `r7a.16xlarge` against published jed (Intel Xeon 8360Y) | 2026-11-12 | yes, and against jed | headline | `c1-2026-11-12/`, `benchmarks/cohesive-perf/results/published-jed/` |
| Multi-node runs | the port only: JOSS mesh on 8 hosts against one node; h = 0.04 on 1–8 hosts; strong and weak series to 64 ranks | 8 × AWS `r7a.2xlarge`, AMD EPYC 9R14 | 2026-11-13 | no | headline | `docs/benchmarks/mn-2026-11-13/` |
| Memory | full-size memory and MUMPS statistics; memory per tetrahedron at mid size; MUMPS allocation at 8 to 64 ranks | AWS `r7a.16xlarge`; boat `large`, AMD EPYC 9455 | 2026-11-12 | yes | headline (full size), supporting (mid size) | `c1-2026-11-12/`, `midsize-2026-11-12/` |
| MPI layer: point-to-point latency | akantu-comm against raw MPI, all sizes, shared memory and TCP, two methods | boat `default`, AMD Ryzen 9 9950X, 4 vCPU | 2026-11-14 | port's layer vs raw MPI | supporting | `docs/benchmarks/mpi-latency-2026-11-14/` |
| MPI layer: allocator state in one job | page faults and backend ratio in one job | boat `default`, 4 vCPU, CPU model not recorded | 2026-11-12 | port's layer vs raw MPI | supporting | numbers in the document only |
| Set-up costs | `init_full`, `dist_mesh`, `read_mesh`, `before_step` | from the sections above | | yes | supporting | as above |
| Known slow paths | each from a section above | | | | supporting | as above |
| Appendix: development-machine records | mesh set-up scaling, the first MPI latency series, the MUMPS sizing series, the first C++ cohesive baselines | Apple M4 Pro (native and aarch64 VM) | 2026-11-10, 2026-11-11 | no | record | `docs/benchmarks/dev-machine/`, `fixtures/solver/mumps/scaling-20261111.tsv` |

Superseded and no longer in the document: the serial driver's port / C++
step ratios before the explicit-step work (2.4–4.2×, replaced by 0.28–0.60×),
the distributed cohesive run before the MUMPS static solve, the extrapolation
from the rehearsal to the full size (replaced by the full-size run), the
before / after tables of single fixes (storage migration, lumped mass) and
the M4 comparison with jed (replaced by the comparison on the JOSS mesh).
They remain in the history of the repository.

## Other documents

| Document | Performance statements | Source |
|---|---|---|
| `README.md` | none (links to `docs/BENCHMARKS.md`) | |
| `docs/manual/src/appendix/joss.md` | full-size table, the jed efficiencies and per-element costs | AWS |
| `docs/VALIDATION_SUMMARY.md`, `docs/MPI_VALIDATION.md` | none (point to `docs/BENCHMARKS.md`) | |
| `benchmarks/cohesive-perf/README.md` | the harness; quotes no result | |
| `fixtures/solver/mumps/scaling-20261111.tsv` | raw data of the MUMPS sizing record | aarch64 VM |
