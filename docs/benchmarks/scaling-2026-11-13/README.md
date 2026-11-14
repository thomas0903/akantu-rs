# Weak and strong scaling series at 1 to 8 ranks (2026-11-13): result summaries

Two interleaved series of the cohesive scalability benchmark, both codes
(the port's driver and upstream's harness), on one x86_64 Linux virtual machine
(boat.dev `large`, AMD EPYC 9455 host, 8 shared vCPUs). Discussion: `docs/BENCHMARKS.md`,
"Small and mid sizes at 1 to 8 ranks". Made with
`crates/akantu-bench/scripts/scaling_series.sh`, tables by
`scaling_analyze.py`.

| File | What it is |
|---|---|
| `ws-summary.md`, `ws-summary.json` | series `ws` (before the first insertion, 500 steps; ten points: weak A, weak B, strong h = 0.06; five repetitions, plus a sixth of h = 0.0476 on 8 ranks): every valid run (phases of rank 0, wall, memory, counts), the medians, the paired port / upstream ratios, the weak and strong studies (`scaling_analyze.py … ws --weak A=0.12:1,0.0925:2,0.0756:4,0.06:8 --weak B=0.0925:1,0.0756:2,0.06:4,0.0476:8 --strong 0.06`) |
| `ins-summary.md`, `ins-summary.json` | series `ins` (insertion regime, frozen h = 0.12 mesh, 1,500 steps, 1 / 2 / 4 / 8 ranks, five repetitions; `--strong 0.12 --expect-insertion`) |
| `ws-series.log`, `ins-series.log` | the series tool's logs: partition digests, the order of every repetition, the outcome of every campaign (one upstream run stopped on its MUMPS retry path, UD-216, and repeated) |
| `provenance-example.txt` | the provenance of one campaign (all campaigns share revision, host, images and binaries; `*-summary.json` "provenance" lists every distinct value) |
| `partitions.sha256` | SHA-256 of the Scotch partitions (exported once per point by upstream's harness, replayed by the port) |
| `campaign-summaries.sha256` | SHA-256 of the per-campaign `summary.json` files the two summaries were computed from |

Not kept: the per-run
directories (energies, insertion logs, memory samples, logs), the
per-campaign summaries and the partitions; the per-run values the tables
use are in `*-summary.json` ("runs"). Mesh statistics of the three meshes
generated for the series: `benchmarks/cohesive-perf/meshes/cube-h0.0925.x86_64.json`,
`cube-h0.0756.x86_64.json`, `cube-h0.0476.x86_64.json` (h = 0.06 is the
recorded `cube-h0.06.x86_64.json`, h = 0.12 the frozen fixture mesh).
