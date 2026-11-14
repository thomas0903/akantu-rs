# Full-size rerun with the corrected load placement (2026-11-13)

The port only, on one AWS `r7a.16xlarge` (64 cores, AMD EPYC 9R14): the
JOSS mesh, 5,000 steps, at 64 and 32 ranks, each with its own MUMPS static
solve, on the 64- and 32-rank partitions exported by the C++ harness for the
run of 12 November (`docs/benchmarks/c1-2026-11-12/`). Settings of every
run: `rust_cohesive_compat = upstream`, `rust_doubled_node_load =
follows_elements` (UD-1280). Driver binary `b321054d…`.

| Run | Loop | Initialisation (with the static solve) | Cohesive elements | Insertion steps | First insertion |
|---|---:|---:|---:|---:|---:|
| `rust-np64-a1` | 1,926 s | 484 s | 471,932 | 4,114 | step 683 |
| `rust-np32-a1` | 2,669 s | 434 s | 471,936 | 4,114 | step 683 |

These runs were made before a later fix of the Neumann store for boundary
elements whose nodal shares differ. With the benchmark's `_triangle_3`
boundaries that fix changes the energies at rounding level only (the
insertion lists are unchanged at 1 to 8 ranks), so the current code does
not reproduce these energies bit for bit.

Files:

- the runs were made by `c1_session.sh … --codes rust --ranks "64 32"
  --partitions … --rust-settings …`; both points completed (the session's
  log and state are not kept);
- `<campaign>/summary.md`, `summary.json`, `provenance.txt`: the campaign
  tool's records (revision, image and binary digests, mesh SHA-256, thread
  pins); campaigns `test-np64` (20 steps), `rust-np64-a1`, `rust-np32-a1`;
- `<campaign>/run/`: the run's `summary.json`, `energies.csv` (step,
  dissipated, potential, kinetic, external work, total), `insertions.csv`
  (one row per insertion step), `input.dat`, `binary.sha256`, `chrono.csv`,
  `wall.txt`, `memory_peak`, `status.txt`;
- `joss64x8-control/`: the same binary on the 64-rank partition across
  eight hosts (from `docs/benchmarks/mn-2026-11-13/`), the control of the
  comparison; `pair-d108.txt` is the multi-node comparator's verdict;
- `d108.txt`: the output of `python3 -I
  crates/akantu-bench/scripts/d108_rerun.py
  docs/benchmarks/c1-rerun-2026-11-13/rust-np64-a1/run
  docs/benchmarks/c1-rerun-2026-11-13/rust-np32-a1/run`.

Every number of the comparison is printed by `python3 -I
crates/akantu-bench/scripts/results_report.py` (output:
`docs/benchmarks/results-2026-11-13.txt`). The reading of the comparison
against the statement of D-108, written before the run, is in
`docs/NUMERICAL_VALIDATION.md` and `docs/MPI_VALIDATION.md`. In short: the
insertion counts are equal through step 3,176; part 1 (b) of the statement
is not met as written (the external work's difference exceeds 1e-10 of the
step's largest energy at step 2,716), and the control pairs, in which no
load placement can differ, diverge in the same way, so the divergence is
amplified rounding. The per-rank summaries and the memory samples are not
kept.
