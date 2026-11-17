# Final campaign, part E: memory

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T07:11:34Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 8 planned, 8 complete and checked (only these are tabulated).

Checks (`checks.jsonl`): 8 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 2 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 32 runs, 32 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "Memory, h = 0.12" (n = repetitions; peaks of the sampler: `memsample.py`, VmHWM of every rank every 0.25 s; container = the cgroup's `memory.peak`; the port's own records in `summary.json`; the last column = (Σ rank peaks − the port's `INFOG(19)`) / tetrahedra, as the document's table "Memory besides MUMPS" (the C++ row also subtracts the port's `INFOG(19)`: upstream records no MUMPS statistics, and both hand MUMPS the same matrix); "factors released" = `rust_release_static_solver = true`):

| ranks | code | n | largest rank MiB | Σ rank peaks MiB | container MiB | static: largest / Σ (port) MiB | port model before static: largest / Σ MiB | MUMPS INFOG(19) MB | besides MUMPS, bytes per tetrahedron |
|---|---|---|---|---|---|---|---|---|---|
| 1 | C++ | 2 | 408 [408, 408] | 408 [408, 408] | 378 [377, 378] | — | — | 79 | 15,082 |
| 1 | port before | 2 | 365 [365, 366] | 365 [365, 366] | 344 [344, 344] | 238 / 238 | 108 / 108 | 79 | 13,139 |
| 1 | port | 2 | 307 [306, 307] | 307 [306, 307] | 284 [284, 284] | 234 / 234 | 108 / 108 | 79 | 10,486 |
| 1 | port, factors released | 2 | 235 [235, 235] | 235 [235, 235] | 212 [212, 212] | 235 / 235 | 109 / 109 | 79 | 7,233 |
| 2 | C++ | 2 | 251 [251, 251] | 496 [496, 496] | 426 [426, 427] | — | — | 120 | 17,291 |
| 2 | port before | 2 | 222 [222, 222] | 440 [440, 440] | 391 [391, 391] | 159 / 315 | 76 / 150 | 120 | 14,740 |
| 2 | port | 2 | 193 [192, 193] | 382 [381, 382] | 330 [330, 330] | 159 / 314 | 76 / 151 | 120 | 12,101 |
| 2 | port, factors released | 2 | 159 [159, 159] | 314 [314, 314] | 262 [262, 262] | 159 / 314 | 76 / 151 | 120 | 9,021 |
| 4 | C++ | 2 | 159 [158, 159] | 620 [619, 620] | 466 [466, 466] | — | — | 188 | 19,937 |
| 4 | port before | 2 | 136 [136, 136] | 529 [528, 530] | 421 [420, 421] | 105 / 407 | 58 / 224 | 188 | 15,823 |
| 4 | port | 2 | 122 [122, 123] | 472 [471, 473] | 359 [358, 359] | 107 / 412 | 58 / 226 | 188 | 13,255 |
| 4 | port, factors released | 2 | 107 [107, 108] | 414 [413, 414] | 301 [300, 301] | 107 / 414 | 59 / 227 | 188 | 10,606 |
| 8 | C++ | 2 | 117 [117, 117] | 864 [863, 864] | 522 [522, 522] | — | — | 444 | 19,943 |
| 8 | port before | 2 | 97.7 [97.5, 97.9] | 722 [722, 723] | 474 [474, 474] | 85 / 611 | 47 / 364 | 444 | 13,539 |
| 8 | port | 2 | 90.8 [90.6, 91.0] | 670 [670, 671] | 409 [409, 409] | 86 / 621 | 48 / 369 | 444 | 11,179 |
| 8 | port, factors released | 2 | 79.4 [69.5, 89.3] | 578 [528, 627] | 361 [361, 362] | 89 / 625 | 48 / 372 | 444 | 6,983 |

Progress log: first line 2026-11-17T07:11:37Z, last line 2026-11-17T07:14:51Z (38 lines).
Cohesive runs: 0.04 h of run time in `index.jsonl` (all attempts).
