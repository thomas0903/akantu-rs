# Final campaign, part C: the cohesive benchmark at small and mid sizes, 1 to 8 ranks

- Machine: AMD EPYC 9R14, 8 cores, kernel 7.0.0-1014-aws.
- Sessions (UTC): 2026-11-17T10:33:38Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `53de1eeefb78`.
- Size `full`, repetitions requested: 5. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 20 planned, 20 complete and checked (only these are tabulated).

C++: the strict install with the JOSS emulation (`FNTE = joss`), MUMPS 5.7.3; the port's MPI build (`mpi,scotch,mumps`); the same mesh and Scotch partition for both (`c1prep_campaign.sh`); rank r bound to the r-th core (block of T cores with T threads) of the host's cores ordered NUMA node by NUMA node, recorded and checked per run; one container per run; the variants of a point adjacent, rotated per repetition, the point order reversed every other repetition. Series `reh6` runs C++ and the head only (before → head is measured by `ws`, `ins`, `full12` and `reh4`).

Checks (`checks.jsonl`): 20 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 5 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 50 runs, 50 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Series of this part not planned in this data directory (the part was split over machines with `--only`; they were planned, run and summarised in another data directory): `ws`, `ins`, `full12`, `reh4`, `eig`.

## Series `reh6`: the full schedule on h = 0.06 ("The mid-size rehearsal")

Table "Points" (rank 0's Chrono phases; n = repetitions in the medians; core-µs per element and step = (`solve_step` + check) × cores / tetrahedra / steps; loop efficiency = core-µs at 1 rank / core-µs at P, same code; peaks of the memory sampler):

| h | ranks | code | n | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.06 | 1 | C++ | 5 | 1,952 [1,951, 1,974] | 13.9 [13.9, 14.0] | 13.0 [12.9, 13.1] | 82.0 [82.0, 82.5] | 278 [278, 282] | 2.03 [2.03, 2.06] | 100% | 3,180 [3,179, 3,180] | 3,180 [3,179, 3,180] |
| 0.06 | 1 | port | 5 | 498 [498, 500] | 1.34 [1.33, 1.34] | 10.2 [10.2, 10.3] | 35.1 [35.1, 35.2] | 57.0 [56.8, 57.1] | 0.519 [0.518, 0.520] | 100% | 2,622 [2,620, 2,622] | 2,622 [2,620, 2,622] |
| 0.06 | 2 | C++ | 5 | 1,238 [1,233, 1,242] | 6.26 [5.72, 6.27] | 8.60 [8.56, 8.72] | 55.8 [55.5, 56.0] | 175 [174, 176] | 2.60 [2.60, 2.61] | 78% | 1,875 [1,875, 1,879] | 3,743 [3,743, 3,747] |
| 0.06 | 2 | port | 5 | 302 [301, 304] | 0.737 [0.736, 0.739] | 7.40 [7.39, 7.41] | 21.5 [21.5, 21.6] | 34.6 [34.4, 34.8] | 0.633 [0.630, 0.637] | 82% | 1,608 [1,600, 1,610] | 3,187 [3,177, 3,196] |
| 0.06 | 2 | port 2 x 4 | 5 | 223 [222, 223] | 0.740 [0.738, 0.744] | 7.16 [7.14, 7.17] | 17.3 [17.2, 17.3] | 22.9 [22.8, 22.9] | 1.81 [1.81, 1.81] | 29% | 1,617 [1,615, 1,623] | 3,209 [3,200, 3,218] |
| 0.06 | 4 | C++ | 5 | 736 [734, 748] | 2.56 [2.54, 2.61] | 4.62 [4.58, 4.65] | 39.9 [39.7, 40.1] | 98.4 [98.1, 101] | 3.12 [3.11, 3.18] | 65% | 989 [964, 998] | 3,878 [3,749, 3,891] |
| 0.06 | 4 | port | 5 | 200 [200, 201] | 0.381 [0.380, 0.383] | 4.06 [4.03, 4.42] | 15.0 [14.9, 15.1] | 22.4 [22.3, 22.6] | 0.842 [0.839, 0.849] | 62% | 849 [847, 857] | 3,304 [3,299, 3,341] |
| 0.06 | 4 | port 4 x 2 | 5 | 188 [187, 189] | 0.379 [0.378, 0.381] | 3.94 [3.92, 4.17] | 14.5 [14.4, 14.5] | 20.5 [20.4, 20.5] | 1.58 [1.57, 1.58] | 33% | 858 [854, 859] | 3,335 [3,332, 3,350] |
| 0.06 | 8 | C++ | 5 | 496 [491, 502] | 1.19 [1.17, 1.23] | 3.23 [3.02, 3.29] | 26.1 [25.7, 26.3] | 66.2 [65.6, 67.2] | 4.16 [4.12, 4.22] | 49% | 626 [616, 689] | 4,517 [4,438, 4,570] |
| 0.06 | 8 | port | 5 | 175 [174, 176] | 0.220 [0.218, 0.226] | 2.94 [2.89, 3.01] | 13.7 [13.6, 13.8] | 19.2 [19.0, 19.2] | 1.48 [1.47, 1.49] | 35% | 532 [517, 609] | 3,831 [3,812, 3,928] |

Table "Port / upstream, series reh6" (paired: the runs of each repetition; median [min, max]):

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.06 | 1 | 0.255 [0.252, 0.256] | 0.428 [0.426, 0.429] | 0.205 [0.202, 0.205] | 0.787 [0.785, 0.793] | 0.0961 [0.0956, 0.0963] | 0.255 [0.252, 0.256] |
| 0.06 | 2 | 0.243 [0.242, 0.244] | 0.385 [0.385, 0.387] | 0.198 [0.196, 0.199] | 0.860 [0.850, 0.864] | 0.118 [0.117, 0.129] | 0.244 [0.243, 0.245] |
| 0.06 | 4 | 0.270 [0.267, 0.272] | 0.377 [0.374, 0.378] | 0.227 [0.224, 0.230] | 0.881 [0.867, 0.960] | 0.149 [0.146, 0.150] | 0.272 [0.269, 0.274] |
| 0.06 | 8 | 0.356 [0.351, 0.359] | 0.528 [0.521, 0.531] | 0.288 [0.285, 0.292] | 0.910 [0.881, 0.995] | 0.185 [0.178, 0.194] | 0.352 [0.349, 0.356] |

Table "Ranks × threads" (the port, loop = `solve_step` + check, paired ratios):

| h | configuration | loop s | against 8 ranks | against the same ranks, 1 thread |
|---|---|---|---|---|
| 0.06 | 2 ranks x 4 threads | 201 [201, 201] | 1.22 [1.21, 1.23] | 0.716 [0.711, 0.720] |
| 0.06 | 4 ranks x 2 threads | 175 [174, 175] | 1.06 [1.06, 1.07] | 0.931 [0.930, 0.940] |

Progress log: first line 2026-11-17T10:33:43Z, last line 2026-11-17T18:58:41Z (56 lines).
Cohesive runs: 8.40 h of run time in `index.jsonl` (all attempts).
