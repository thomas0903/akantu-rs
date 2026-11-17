# Final campaign, part E: memory

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T05:15:10Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Checks (`checks.jsonl`): 8 points; at every point the thread count and precision of each port run (`rust_exec` of `summary.json`) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 2 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 32 runs, 32 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "Memory, h = 0.12" (peaks of the sampler: `memsample.py`, VmHWM of every rank every 0.25 s; container = the cgroup's `memory.peak`; the port's own records in `summary.json`; the last column = (Σ rank peaks − the port's `INFOG(19)`) / tetrahedra, as the document's table "Memory besides MUMPS"; "factors released" = `rust_release_static_solver = true`):

| ranks | code | largest rank MiB | Σ rank peaks MiB | container MiB | static: largest / Σ (port) MiB | port model before static: largest / Σ MiB | MUMPS INFOG(19) MB | besides MUMPS, bytes per tetrahedron |
|---|---|---|---|---|---|---|---|---|
| 1 | C++ | 408 [408, 408] | 408 [408, 408] | 378 [378, 378] | — | — | 79 | 15,082 |
| 1 | port before | 366 [366, 366] | 366 [366, 366] | 345 [344, 345] | 238 / 238 | 108 / 108 | 79 | 13,162 |
| 1 | port | 307 [307, 307] | 307 [307, 307] | 284 [284, 284] | 235 / 235 | 109 / 109 | 79 | 10,510 |
| 1 | port, factors released | 235 [235, 235] | 235 [235, 235] | 212 [212, 213] | 235 / 235 | 109 / 109 | 79 | 7,225 |

Table "Memory, h = 0.12" (peaks of the sampler: `memsample.py`, VmHWM of every rank every 0.25 s; container = the cgroup's `memory.peak`; the port's own records in `summary.json`; the last column = (Σ rank peaks − the port's `INFOG(19)`) / tetrahedra, as the document's table "Memory besides MUMPS"; "factors released" = `rust_release_static_solver = true`):

| ranks | code | largest rank MiB | Σ rank peaks MiB | container MiB | static: largest / Σ (port) MiB | port model before static: largest / Σ MiB | MUMPS INFOG(19) MB | besides MUMPS, bytes per tetrahedron |
|---|---|---|---|---|---|---|---|---|
| 2 | C++ | 251 [251, 251] | 496 [496, 497] | 426 [426, 426] | — | — | 120 | 17,297 |
| 2 | port before | 222 [222, 222] | 440 [440, 440] | 390 [390, 391] | 159 / 314 | 75 / 149 | 120 | 14,736 |
| 2 | port | 193 [193, 193] | 382 [382, 382] | 330 [330, 330] | 159 / 314 | 76 / 151 | 120 | 12,118 |
| 2 | port, factors released | 159 [159, 159] | 314 [314, 314] | 262 [262, 262] | 159 / 314 | 76 / 151 | 120 | 9,037 |

Table "Memory, h = 0.12" (peaks of the sampler: `memsample.py`, VmHWM of every rank every 0.25 s; container = the cgroup's `memory.peak`; the port's own records in `summary.json`; the last column = (Σ rank peaks − the port's `INFOG(19)`) / tetrahedra, as the document's table "Memory besides MUMPS"; "factors released" = `rust_release_static_solver = true`):

| ranks | code | largest rank MiB | Σ rank peaks MiB | container MiB | static: largest / Σ (port) MiB | port model before static: largest / Σ MiB | MUMPS INFOG(19) MB | besides MUMPS, bytes per tetrahedron |
|---|---|---|---|---|---|---|---|---|
| 4 | C++ | 159 [159, 159] | 619 [619, 619] | 465 [465, 465] | — | — | 188 | 19,905 |
| 4 | port before | 138 [137, 140] | 540 [530, 550] | 430 [421, 439] | 109 / 427 | 58 / 225 | 188 | 16,331 |
| 4 | port | 123 [122, 123] | 473 [473, 474] | 359 [358, 359] | 108 / 413 | 59 / 228 | 188 | 13,308 |
| 4 | port, factors released | 108 [107, 108] | 410 [407, 413] | 298 [295, 300] | 108 / 413 | 59 / 228 | 188 | 10,448 |

Table "Memory, h = 0.12" (peaks of the sampler: `memsample.py`, VmHWM of every rank every 0.25 s; container = the cgroup's `memory.peak`; the port's own records in `summary.json`; the last column = (Σ rank peaks − the port's `INFOG(19)`) / tetrahedra, as the document's table "Memory besides MUMPS"; "factors released" = `rust_release_static_solver = true`):

| ranks | code | largest rank MiB | Σ rank peaks MiB | container MiB | static: largest / Σ (port) MiB | port model before static: largest / Σ MiB | MUMPS INFOG(19) MB | besides MUMPS, bytes per tetrahedron |
|---|---|---|---|---|---|---|---|---|
| 8 | C++ | 118 [117, 118] | 866 [863, 868] | 522 [521, 522] | — | — | 444 | 20,024 |
| 8 | port before | 96.8 [96.8, 96.8] | 721 [720, 722] | 467 [466, 468] | 84 / 606 | 47 / 362 | 444 | 13,478 |
| 8 | port | 64.9 [64.2, 65.6] | 503 [500, 506] | 408 [408, 409] | 86 / 623 | 48 / 371 | 444 | 3,606 |
| 8 | port, factors released | 72.9 [68.8, 77.1] | 536 [521, 551] | 363 [360, 366] | 86 / 621 | 48 / 368 | 444 | 5,098 |

Progress log: first line 2026-11-17T05:15:11Z, last line 2026-11-17T05:18:01Z (37 lines).
Cohesive runs: 0.04 h of run time in `index.jsonl` (all attempts).
