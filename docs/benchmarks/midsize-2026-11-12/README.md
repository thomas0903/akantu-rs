# Mid-size rehearsal and MUMPS series (2026-11-12)

Both codes on one x86_64 Linux virtual machine (boat.dev `large`, AMD EPYC 9455 host, 8 vCPU,
15.6 GiB, Ubuntu 24.04, kernel 6.8.0-117), one run at a time:
`crates/akantu-bench/scripts/c1prep_campaign.sh` alternates the C++
harness (strict install, JOSS emulation, MUMPS) and the port's driver
(`--features mpi,scotch,mumps`) on the same mesh and Scotch partition;
memory by `memsample.py` (every rank's VmRSS / VmHWM every 0.25 s) and the
port's own records. Tables by `c1prep_analyze.py`.

| Campaign | What |
|---|---|
| `item1-h012/` | h = 0.12 frozen mesh, 1,500 steps with insertion, 1 / 2 / 4 ranks × 3 repetitions, plus the SparseLU static solve at 1 rank |
| `item2-h006/` | h = 0.06 (177,328 tetrahedra), the full 5,000-step schedule, 1 / 2 / 4 / 8 ranks (1 repetition at 1 and 2 ranks, 2 at 4 and 8) |
| `item2-h004/` | h = 0.04 (558,875 tetrahedra), static solve and 300 steps before insertion, 1 / 2 / 4 / 8 ranks, 1 repetition; the C++ run at 8 ranks was killed by the 14 GiB container limit |
| `item2-h006pre/` | h = 0.06, 300 steps before insertion, 1 and 8 ranks |
| `quota-q8/`, `quota-q0/` | the port at 8 ranks with and without the container CPU quota (3 alternated pairs) |
| `c1fix-mscale-h004-np16/`, `-np32/`, `c1fix-mscale-h005-np8/`, `-np16/`, `-np32/` | the MUMPS allocation at 8 to 32 ranks on h = 0.05 (150,702 DOFs) and h = 0.04 (killed at the 14 GiB limit at 16 and 32 ranks) |
| `c1fix-joss-rust8/` | the port on the JOSS mesh at 8 ranks, a short run; its `rust/np8/rep1/input.dat` is the reference input of `crates/akantu-bench/scripts/tests/test_c1_session.py` |

Files per campaign: `summary.md` and `summary.json` (per run the Chrono
phases of rank 0, wall time, counts, energies, invariants, memory, the
port's MUMPS statistics), `provenance.txt` (revision, host CPU and memory,
mesh digest, images, binary digests, thread pins). The per-run directories,
memory samples and energies are not kept. The MUMPS allocation of h = 0.12
and h = 0.06 at 16 to 64 ranks in `docs/BENCHMARKS.md` ("MUMPS allocation
beyond 8 ranks") was measured in a separate run on the same machine type;
its files are not kept.
