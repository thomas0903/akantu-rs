<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The full-size benchmark across hosts and the final one-machine run

The JOSS mesh (4,392,183 tetrahedra, 5,000 steps with insertion; the
benchmark of `docs/BENCHMARKS.md`, "The cohesive scalability benchmark")
across four and two 32-core hosts, both codes, before and after the change
of the port's collective error protocol (D-126), and the port at its final
revision on one 128-core machine. Part G (`../G/`) holds the one-machine
runs of both codes before D-126; `table.md` puts every run of both
directories side by side.

## Machines

Every host: AWS `m7a.8xlarge`, AMD EPYC 9R14, 32 cores, one thread per
core, one NUMA node, 123 GiB visible, Ubuntu 24.04, kernel 7.0.0-1014-aws,
Docker 29.1.3; the four hosts of a session were started together in one
cluster placement group and one availability zone, from the same machine
image as part G's host, with nothing else running. Sessions 2 and 3 used
four new hosts each; their descriptions are recorded in every run's
`status.txt` (`host_descriptions`); session 1's launcher did not record
them yet, they are in `s1-port/hosts-record.txt` (read on each host before
the session). The one-machine host: AMD EPYC 9R14, 128 cores, one thread
per core, two NUMA nodes of 64 cores, 493 GiB visible, the same kernel
(`one-machine-final/lscpu.txt`, `campaign.log`); part G's host has the same
description.

Open MPI 5.0.7 (TCP between hosts, shared memory inside a host), one
container per host limited to its 32 cores, ranks in consecutive blocks
of P / H per host, rank 0 on `host1`; every rank binds itself to one core
(`bind = self`, `benchmarks/cohesive-perf/scripts/in-container/bind-rank.sh`:
Open MPI 5.0.7's `--bind-to core` cannot start more than 10 ranks per host
in a Docker container with the default seccomp profile). The preflight and
every run record each rank's host, local rank and allowed cores
(`check.json`, `status.txt` `bindings`, `binding_problems`): one distinct
core per rank in every run.

Hosts are written `host1` … `host4` (the order of the run's `hosts`,
`host1` holds rank 0) and their names `node-host1` …; home directories
`~`.

## Sessions

All on 2026-11-18, UTC.

| Folder | Session | Code, revision, driver SHA-256 | Runs (start–end) |
|---|---|---|---|
| `s1-port/` | 1, four hosts | port `740a79a36` (part G's head), `90d301c9…` | `smoke-h012` 04:12; `joss128x4` 04:13–04:44; `joss64x2` 04:44–05:07 |
| `s1-cpp/` | 1, same hosts | C++ `35a85178`, strict install, JOSS emulation, `5b85f310…` | `smoke-h012` 05:18; `joss128x4` 05:19–05:43 |
| `s1-cpp64/` | 1, the first two hosts | C++, as above | `joss64x2` 05:44–06:23 |
| `s2-after/` | 2, four new hosts | port `6cdb1b601` (D-126, first round), `2dea7f76…` | `smoke-h012` 14:10; `joss128x4` 14:10–14:25 |
| `s2-before/` | 2, same hosts | port `740a79a36`, `90d301c9…` | `smoke-h012` 14:26; `joss128x4` 14:26–14:59 |
| `s2-cpp/` | 2, same hosts | C++, as above | `smoke-h012` 15:00; `joss128x4` 15:00–15:25 |
| `s3-final128/` | 3, four new hosts | port `4a8d9d38e` (final), `b68096d0…` | `smoke-h012` 18:23; `joss128x4` 18:23–18:38 |
| `s3-final64/` | 3, the first two hosts | port `4a8d9d38e`, `b68096d0…` | `joss64x2` 18:39–18:55 |
| `one-machine-final/` | 4, one 128-core machine | port `4a8d9d38e`, `b68096d0…` | 128 ranks 19:18–19:37 |

The full SHA-256 of every driver is in each run's `binary.sha256`; the
images and the revisions they were built from in `images-*.txt` (the
port's driver of sessions 1 and 2 "before" is part G's binary, copied
into the image; `git_head` there is the launcher's checkout), the C++
build record in `verify-cpp.json`. The C++ runs are those of part G's C++
points with the same input but the mesh path: strict install,
`fillNodesToElements()` rebuild skipped (`FNTE = joss`), Scotch with
`AKA_SEED = 1` and one thread, the partition re-created and checked
against the file of part G (`partition_matches`).

Runs: `joss128x4` = 128 ranks, 32 per host on four hosts; `joss64x2` = 64
ranks, 32 per host on two hosts; both replay part G's exported Scotch
partitions (`partition-np128.csv`, SHA-256 `699d9025…`;
`partition-np64.csv`, `6f8dbbac…`). `smoke-h012` is the launcher's check
(h = 0.12, 1,500 steps, 8 ranks, 2 per host; not a measurement). MUMPS
static solve (`auto`), the drivers' default settings, the energies of
every step written. Every configuration ran once. The one-machine run of
session 4 is part G's `128:head` point at the final revision, run with
`c1prep_campaign.sh` as part G runs it (its records: `run/`,
`bindings.txt` checked with `scripts/perf/final/bindings.py check`: as
planned).

Session 1 ran the port's two runs, then the C++ runs; the C++ run at 64
ranks ran after the other two hosts had been stopped. Session 2 ran
"after", "before" and C++ one after the other on the same hosts. Session 3
ran the final revision's two runs.

## Files

Per run (copied by `collect.py`, addresses replaced): `status.txt` (the
launcher's record: hosts, limits, bindings, per-host memory peak, outcome),
`summary.json`, `chrono.csv` (rank 0's phase totals), `wall.txt`,
`binary.sha256`, `input.dat`, `hostfile`, and `outputs.sha256` (the SHA-256
of `energies.csv` and `insertions.csv`, which are not kept). Per session:
`check.json` (the preflight), `session.log`; `chain.log` (the order of the
sessions of one day's set of hosts). Not kept: the per-rank summaries,
chrono files and memory samples, the logs of the processes, the per-step
energies and insertions, the partitions and meshes.

- `table.md`: every run and the comparison tables, written by
  `python3 -I table.py > table.md` from this directory's run folders,
  `../G/runs.jsonl` and upstream's published timings; the tables of
  `docs/BENCHMARKS.md` ("The full-size run", "Multi-node runs") are copied
  from it.
- `compare.txt`: `crates/akantu-bench/scripts/mn_compare.py --pair` (the
  port) and `--cpp` (C++) for every multi-host run that has a one-machine
  run of the same binary and rank count (part G's, and session 4's for the
  final revision): every one passes (placement, partition, distribution,
  static matrix, MUMPS settings; C++: outcome, driver, input, partition,
  binding, phase repetitions). The final revision's pair could not check
  the partition file of the one-machine run (not kept with it); its
  per-rank element counts and static matrix are identical. The energies
  and insertions differ from the first insertions on in every pair, as two
  runs with MUMPS's parallel factorisation do on one machine (printed, not
  judged); cohesive element counts differ by at most 0.03 %.
- `collect.py`: how the folders were made, e.g.
  `python3 -I collect.py s1-port <results> <results>/session.log --extra
  images-port.txt <host record>=hosts-record.txt`;
  `python3 -I collect.py one-machine-final <campaign>/rust/np128/rep1 --c1
  --root <data root> --extra host.txt=lscpu.txt
  final-np128.bindings.txt=bindings.txt final-np128.bindings.json=bindings.json
  final-np128.log=campaign.log <run log>=run.log`.

Commands (head host; `mn_session.py` and the cluster directory as in
`crates/akantu-bench/scripts/mn_session.py`; plans
`mn_plan_final.txt`, and `mn_plan_final_cpp.txt` with `code = cpp`;
sessions 2 and 3 ran the plan's first two lines, then its third):

    python3 -I crates/akantu-bench/scripts/mn_session.py run SESSION RESULTS BUDGET_H \
        --plan crates/akantu-bench/scripts/mn_plan_final.txt \
        --meshes MESHES.tar --partitions PARTITIONS

and for session 4, on the 128-core machine (part G's point `128:head`):

    env PIN_CPUS_LIST=0,...,127 FNTE=joss CPP_CONFIG=strict PERF_MPI_BIND=core \
        bash crates/akantu-bench/scripts/c1prep_campaign.sh --memory 463g --cpus 128 \
        --partitions <root>/partitions/hjoss --rust-bin <driver b68096d0…> \
        --rust-volume akantu-rs-mpi-target --rust-image akantu-rs-mumps:dev \
        full-hjoss-np128-r1-head joss 5000 128 1 rust

(part G's command for that point, `../G/commands.log`, with the final
driver and a copy of part G's `partition-np128.csv`; the copy's digest is
not recorded, the pair in `compare.txt` finds every rank's element count
identical to the four-host run that replayed it).

## Reading

The tables and what can be compared are in `docs/BENCHMARKS.md`, "The
full-size run" and "Multi-node runs". In short (`table.md`): before D-126
the port's loop was 1,538 s (session 1) and 1,536 s (session 2) on four
hosts against C++'s 1,040 s and 1,019 s; at the final revision it is
526 s (session 3), 722 s on two hosts (C++ 1,924 s, session 1), and 814 s
on one machine (part G, before D-126: 835 s; C++ 1,948 s).
