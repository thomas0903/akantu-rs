# Benchmark data

The data behind `docs/BENCHMARKS.md`: per study, the summary files that the
document, the report data in `docs/reports/data/` and the tests read. Raw
per-run records (logs, memory samples, per-rank files, partitions) and the
session tools' logs and scheduling state are not kept in the tree; a few
studies keep a run's own small files (listed below). Dates are local dates
(UTC+11), except the final campaign's, which are UTC dates; the provenance
files record UTC times. Host names are written
`node-host1` … `node-host8`. Home directories are written `/home/<user>`,
except the home of the generic account `user` of the x86_64 virtual
machines, which is kept; run directories in `summary.json` are relative to
their campaign.

| Study | What | Machine | Date | Files |
|---|---|---|---|---|
| `explicit-step/final/` | the explicit step of the port against the strict and default C++ installs, ten configurations; the cohesive driver before and after the explicit-step work (`driver/`) | boat.dev `large`, AMD Ryzen 7 9700X, 8 vCPU | 2026-11-12 | `summary.md`, `digests.txt`, `codegen-x86_64.txt`, `meshes-SHA256SUMS`; `driver/summary.md`, `paired.txt`, `provenance.txt`, `bench_paired.py` |
| `explicit-step/development/` | the explicit-step work: interleaved A/B series of every change, the final series against the strict C++ install, the cohesive step | boat.dev `large`, AMD Ryzen 9 9950X, 8 vCPU | 2026-11-11 | `tables.md`, `BUILDS.md` |
| `serial-driver-2026-11-11/` | the serial cohesive driver against the strict and default C++ installs before the explicit-step work (kept for the default / strict comparison) | boat.dev `large`, AMD Ryzen 9 9950X, 8 vCPU | 2026-11-11 | `summary.md`, `provenance-h0*.txt`, `harness-binaries.txt`, `meshes.sha256` |
| `cpp-baseline-x86-2026-11-11/` | the C++ reference alone: the cohesive benchmark before insertion at 1 / 2 / 4 ranks, and the full schedule on h = 0.12 with and without the JOSS emulation | boat.dev `large`, AMD Ryzen 7 9700X, 8 vCPU | 2026-11-11 | per campaign `timmings_*.csv` (upstream's format), `collection.json`, `provenance.json`, `runs.jsonl` |
| `midsize-2026-11-12/` | the mid-size rehearsal (h = 0.06 full schedule, h = 0.04 static solve and 300 steps), the MUMPS series at h = 0.12, the CPU-quota check, the MUMPS allocation at 8 to 32 ranks | boat.dev `large`, AMD EPYC 9455, 8 vCPU | 2026-11-12 | per campaign `summary.md`, `summary.json`, `provenance.txt` |
| `c1-2026-11-12/` | the full-size run on the JOSS mesh, both codes, 64 / 32 / 8 ranks | AWS `r7a.16xlarge`, AMD EPYC 9R14, 64 cores | 2026-11-12 | `extract.json`, `state.json` (the outcome, hours and estimate of each point), `smoke.log`, per campaign `summary.*`, `provenance.txt` |
| `c1-rerun-2026-11-13/` | the port's full-size rerun after UD-1280, 64 and 32 ranks, and the eight-host control run | AWS `r7a.16xlarge` (and 8 × `r7a.2xlarge` for the control) | 2026-11-13 | per run `summary.json`, `chrono.csv`, `energies.csv`, `insertions.csv` |
| `mn-2026-11-13/` | the multi-node runs of the port and the per-copy load check at full size | 8 × AWS `r7a.2xlarge`, AMD EPYC 9R14 | 2026-11-13 | per run `summary.json`, `chrono.csv`, `status.txt`, `wall.txt` |
| `results-2026-11-13.txt` | the tables of the rerun, its controls, the per-copy check and the multi-node runs, printed by `crates/akantu-bench/scripts/results_report.py` from the two directories above | | 2026-11-13 | |
| `percopy-2026-11-13/` | the driver's per-copy load check at mid sizes (h = 0.12, h = 0.06, 1 to 8 ranks) | boat.dev `large`, x86_64 | 2026-11-13 | `analysis.txt`, `runs/files.sha256` |
| `scaling-2026-11-13/` | weak and strong scaling of both codes at 1 to 8 ranks, before insertion and with insertion | boat.dev `large`, AMD EPYC 9455, 8 vCPU | 2026-11-13 | `ws-summary.{md,json}`, `ins-summary.{md,json}`, series logs, partition digests |
| `mpi-latency-2026-11-14/` | akantu-comm against raw MPI, all message sizes, shared memory and TCP | boat.dev `default`, AMD Ryzen 9 9950X, 4 vCPU | 2026-11-14 | `separate.tsv`, `calibrated.txt`, `perf-lines.txt`, `provenance.txt`, the scripts |
| `perf-campaign/final/` | the final campaign, stage 1: the C++ reference, the port before the performance campaign and the port at `53de1eeef` in every table of `docs/BENCHMARKS.md` (parts A to F: explicit step, suites, cohesive benchmark at 1 to 8 ranks, static solves, memory, set-up and MPI layer); `README.md` says what was left out | 4 × AWS `m7a.2xlarge`, AMD EPYC 9R14, 8 cores | 2026-11-17 (UTC) | per part `summary.md`, `provenance.txt`, `plan.{txt,jsonl}`, `groups.jsonl`, `sessions.jsonl`, `progress.log`, `commands.log`; `checks.jsonl`, `index.jsonl`, `runs.jsonl` (parts C to F); `suite-records.tsv`, `suite-bindings.tsv` (B, D, F); `digests.tsv`, `rounds/` (A); `mpilayer/` (F) |
| `perf-campaign/final-dryrun/` | the final campaign's dry run of every part (not results) | development machine (a Google Cloud VM) | 2026-11-17 | per part `summary.md` and records |
| `perf-campaign/` (other directories) | the performance campaign's development records, branch by branch (`perf-campaign/README.md`; not results) | development machines, AWS `m7a.2xlarge` shared with other jobs | 2026-11-16, 2026-11-17 | per branch `README.md` and data |
| `dev-machine/` | development records on the Apple M4 Pro (appendix of `docs/BENCHMARKS.md`; no conclusion rests on them) | Apple M4 Pro, aarch64 Linux VM | 2026-11-10 | `cpp-baseline-m4-2026-11-10/` |

The upstream JOSS study's published timings are in
`benchmarks/cohesive-perf/results/published-jed/` (read by the parity tests).

Readers of these files:

- `crates/akantu-bench/scripts/c1_report.py tables docs/benchmarks/c1-2026-11-12`
  prints the full-size tables of `docs/BENCHMARKS.md`.
- `crates/akantu-bench/scripts/results_report.py` reads
  `c1-2026-11-12/`, `c1-rerun-2026-11-13/` and `mn-2026-11-13/`;
  `crates/akantu-bench/scripts/tests/test_results_report.py` checks that its
  output equals `results-2026-11-13.txt`.
- `crates/akantu-bench/scripts/tests/test_c1_session.py` compares the
  campaign tool's input file with
  `midsize-2026-11-12/c1fix-joss-rust8/rust/np8/rep1/input.dat`.
- `crates/akantu-bench/scripts/c1prep_extrapolate.py` reads the
  `summary.json` files of `midsize-2026-11-12/`.
- The CSV files of `docs/reports/data/` name their sources here.
