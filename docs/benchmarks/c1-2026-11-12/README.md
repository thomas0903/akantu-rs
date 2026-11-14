# Full-size run (2026-11-12): result summaries

The full-size run of the cohesive scalability benchmark: the JOSS mesh,
5,000 steps, the port at 64, 32 and 8 ranks, upstream at 64 and 32 (the C++
run at 8 ranks was not made: when its turn came, the time left in the
session, 2.35 h, was shorter than its scaled estimate, 2.49 h), one AWS
`r7a.16xlarge` (AMD EPYC 9R14, 64 cores). Discussion and tables:
`docs/BENCHMARKS.md`, "The full-size run" and "Memory".

| File | What it is |
|---|---|
| `state.json` | from the final state of the session tool (`crates/akantu-bench/scripts/c1_session.py`), per point: code, ranks, outcome, hours and the session's estimate (read by `c1_report.py tables`) |
| `smoke.log` | the session's set-up check (image ids, the campaign tool on h = 0.12 at 2 ranks) |
| `<campaign>/summary.md`, `summary.json` | `c1prep_analyze.py`'s tables of each campaign: per run the Chrono phases of rank 0, wall time, counts, energies, invariants, memory (sampler and the port's records), MUMPS statistics (port) |
| `<campaign>/provenance.txt` | revision, host CPU and memory, mesh SHA-256, images, binary digest, thread settings |
| `extract.json` | what the tables need from the per-run files that are not kept: Chrono phases, the C++ run and build records, the port's binary digest and container limits, per-rank counts (tetrahedra, ghosts, cohesive elements, owned nodes) and memory, the sampler's per-rank peaks |
| `sources.sha256` | SHA-256 of every source file read when the directory was packed |
| `joss_detail_r64_r32_1100_1140.txt` | the energy differences between the 64- and the 32-rank run at steps 1,100 to 1,140, read by `results_report.py` (its injection test) |

Campaigns: `test-np64` (the 20-step test of both codes at 64 ranks, also
the memory pilot), `rust-np64-a1`, `cpp-np64-a1`, `rust-np32-a1`,
`cpp-np32-a1`, `rust-np8-a1` (5,000 steps each, one run per point).

Made with `python3 -I crates/akantu-bench/scripts/c1_report.py pack
<results> docs/benchmarks/c1-2026-11-12`, which refuses to write if any
file holds an IP address, an instance or account id, a cloud host name, an
ARN, a key file or a home directory other than `/home/<user>`. The tables of
`docs/BENCHMARKS.md` come from these files only: `python3 -I
crates/akantu-bench/scripts/c1_report.py tables docs/benchmarks/c1-2026-11-12`.

Not kept: the session's log and the rest of its state (scheduling data), the per-step files (`energies.csv`, `insertions.csv`), the
per-rank summaries, the memory samples, the partitions (about 120 MB each)
and the logs; `sources.sha256` identifies the ones the tables use.
