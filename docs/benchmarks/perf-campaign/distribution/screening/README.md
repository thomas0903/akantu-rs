<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Intermediate A/B series (development VM, AMD EPYC Milan; `scripts/perf/distribution/ab.sh`)

| Series | Variants (code) | What it measured | Result (paired ratio, median [min, max]) |
|---|---|---|---|
| `s1` | `base` = 3cf66fdba, `h1` = 0a4c6a038 | per-ghost scratch, block-wise `split_by_law` | `solve_step` 0.851 / 0.878 / 0.921 at 2 / 4 / 8 ranks |
| `s2` | `h1`, `h2` = 9fd6fd056 (inline header) | exchanges and reductions | `all_reduce_2` 0.61 / 0.56 / 0.56; node exchange 0.91 / 0.90 / 0.84 |
| `s3` | `h3` = 3c0d90309, `h4` = bc31dea95 (radix sort) | energies | 0.820 at 1 rank, 0.853 at 4 |
| `s4` | `h4`, `h5` = h4 + gather/fold/broadcast all-reduce (not committed) | `all_reduce_4096` | 1.453 / 1.138 / 0.904 at 2 / 4 / 8: dropped |
| `s5` | `h6` (agreement counters, 63f4d9825 + 26d0f8365's case extras), `h7` = 26d0f8365 (overlap) | the stress-exchange overlap | 1.004 / 0.922 / 1.033: no measurable change |
| `s6` | `h7`, `h8` (dual graph, group tables, packing, zero-copy bytes: e07f2c38a) | partition, distribution, exchanges | partition 0.28–0.41, distribute 0.34–0.39, exchanges 0.90–0.98 |
| `s7` | `h8`, `h9` = 818a43195 (one-rank agreements) | the tiny serial step | 0.596 |
| `s8` | `h9`, `h9x` = h9 without the agreement after successful DOF-manager steps (experiment, not committed) | agreements per step (15 → 3) | `solve_step` 1.032 / 1.083 / 0.963: no measurable change |

`contention.txt` and `imbalance.txt`: the memory-bandwidth and per-vCPU
measurements of the README's "What remains".
