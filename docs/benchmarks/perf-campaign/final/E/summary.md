# Final campaign, part E: memory

- Machine: AMD EPYC 9R14, 8 cores, kernel 7.0.0-1014-aws.
- Sessions (UTC): 2026-11-17T10:42:18Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `53de1eeefb78`.
- Size `full`, repetitions requested: 5. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 20 planned, 20 complete and checked (only these are tabulated).

Checks (`checks.jsonl`): 20 points; at every point the binary that ran (its SHA-256 as recorded in the run) against `VARIANT.json` / `CPP.json`; the thread count and precision of each port run (`rust_exec` of `summary.json`: the process default policy, read from the environment) against the request; per-step energies and insertions (`energies.csv`, `insertions.csv`, 17 digits) identical across the bitwise variants at 1 rank (C++ included) and in the SparseLU series at every rank count; 5 comparisons made, 0 points failed. The MUMPS runs at P > 1 are compared on time only (the distributed solve does not repeat run to run).

Core bindings (`runs/*.bindings.txt`, rank -> cores read from /proc during each run): 80 runs, 80 as planned (rank r on the r-th block of its thread count in the core list ordered NUMA node by NUMA node, every block inside one NUMA node).

Table "Memory, h = 0.04" (n = repetitions; peaks of the sampler: `memsample.py`, VmHWM of every rank every 0.25 s; container = the cgroup's `memory.peak`; the port's own records in `summary.json`; the last column = (Σ rank peaks − the port's `INFOG(19)`) / tetrahedra, as the document's table "Memory besides MUMPS" (the C++ row also subtracts the port's `INFOG(19)`: upstream records no MUMPS statistics, and both hand MUMPS the same matrix); "factors released" = `rust_release_static_solver = true`):

| ranks | code | n | largest rank MiB | Σ rank peaks MiB | container MiB | static: largest / Σ (port) MiB | port model before static: largest / Σ MiB | MUMPS INFOG(19) MB | besides MUMPS, bytes per tetrahedron |
|---|---|---|---|---|---|---|---|---|---|
| 1 | C++ | 5 | 11,669 [11,669, 11,669] | 11,669 [11,669, 11,669] | 11,659 [11,659, 11,687] | — | — | 5302 | 12,408 |
| 1 | port before | 5 | 10,271 [10,270, 10,271] | 10,271 [10,270, 10,271] | 10,269 [10,269, 10,287] | 7,149 / 7,149 | 1,733 / 1,733 | 5302 | 9,783 |
| 1 | port | 5 | 8,965 [8,964, 8,965] | 8,965 [8,964, 8,965] | 8,960 [8,960, 8,960] | 7,302 / 7,302 | 1,732 / 1,732 | 5302 | 7,334 |
| 1 | port, factors released | 5 | 7,302 [7,302, 7,303] | 7,302 [7,302, 7,303] | 7,294 [7,294, 7,294] | 7,302 / 7,302 | 1,732 / 1,732 | 5302 | 4,214 |
| 2 | C++ | 5 | 6,961 [6,961, 6,962] | 13,741 [13,740, 13,747] | 13,707 [13,707, 13,715] | — | — | 7586 | 12,207 |
| 2 | port before | 5 | 5,885 [5,884, 5,892] | 11,352 [11,347, 11,359] | 11,323 [11,318, 11,330] | 4,333 / 8,249 | 955 / 1,878 | 7586 | 7,725 |
| 2 | port | 5 | 5,566 [5,557, 5,576] | 11,013 [10,992, 11,023] | 10,982 [10,958, 10,992] | 4,790 / 9,458 | 955 / 1,876 | 7586 | 7,090 |
| 2 | port, factors released | 5 | 4,792 [4,790, 4,801] | 9,464 [9,459, 9,474] | 9,430 [9,425, 9,439] | 4,792 / 9,464 | 955 / 1,878 | 7586 | 4,183 |
| 4 | C++ | 5 | 3,489 [3,381, 3,652] | 13,171 [13,117, 13,673] | 13,045 [12,991, 13,548] | — | — | 6900 | 12,365 |
| 4 | port before | 5 | 3,022 [2,999, 3,123] | 11,728 [11,661, 11,747] | 11,639 [11,570, 11,657] | 2,252 / 8,673 | 545 / 2,138 | 6900 | 9,658 |
| 4 | port | 5 | 2,709 [2,687, 2,952] | 10,400 [10,339, 11,043] | 10,302 [10,239, 10,946] | 2,590 / 9,547 | 542 / 2,139 | 6900 | 7,167 |
| 4 | port, factors released | 5 | 2,419 [2,315, 2,604] | 8,873 [8,837, 9,436] | 8,773 [8,738, 9,338] | 2,315 / 8,873 | 542 / 2,139 | 6900 | 4,301 |
| 8 | C++ | 5 | 2,199 [2,154, 2,699] | 15,367 [13,976, 15,465] | 15,037 [13,633, 15,134] | — | — | 8051 | 14,426 |
| 8 | port before | 5 | 2,245 [1,889, 2,439] | 13,938 [12,328, 14,463] | 13,697 [12,078, 14,212] | 1,504 / 9,913 | 313 / 2,373 | 8051 | 11,744 |
| 8 | port | 5 | 1,843 [1,682, 2,228] | 12,133 [11,452, 12,277] | 11,878 [11,197, 12,023] | 2,086 / 10,802 | 323 / 2,389 | 8051 | 8,359 |
| 8 | port, factors released | 5 | 1,614 [1,553, 2,134] | 10,716 [10,358, 11,170] | 10,466 [10,105, 10,918] | 1,857 / 11,170 | 323 / 2,388 | 8051 | 5,700 |

Progress log: first line 2026-11-17T10:42:24Z, last line 2026-11-17T14:06:40Z (85 lines).
Cohesive runs: 3.38 h of run time in `index.jsonl` (all attempts).
