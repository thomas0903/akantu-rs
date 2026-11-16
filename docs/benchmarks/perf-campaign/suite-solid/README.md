# Benchmark suite, solid mechanics and cohesive elements: data

The cases are those of `suite_solid` (`crates/akantu-bench/README.md`,
section "Benchmark suite", "Solid mechanics"). Ranked hot spots:
`HOTSPOTS.md`.

## Baseline (`baseline/`, `baseline/summary.md`)

- Binary: `suite_solid` of commit `c698395e8`, release profile, default
  flags, no features (SHA-256 `fbbd93f8…`, built in a boat.dev sandbox with
  the workspace toolchain; the same Ubuntu 24.04 as the measurement
  machine).
- Machine: an AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, 32 GiB), Ubuntu
  24.04; one process per run pinned to core 2 (`taskset -c 2`), under the
  campaign's timing wrapper (timing alone on cores 0–3 while builds may run
  on cores 4–7).
- Command: `scripts/perf/suite/run_ab.sh --out baseline --size m --reps 1
  --rep-offset R --core 2 --cases <all serial cases> base=<binary>`, one
  call per repetition R = 0…4 (repetition 0 in two calls), size `m`.
  Dates: `baseline/meta.r*.json`.
- Raw data: `baseline/base/<case>.r<R>.jsonl`, one JSON line per phase.
- Table: `python3 scripts/perf/suite/summarize.py baseline` →
  `baseline/summary.md` (median [min, max] over the 5 repetitions, per call
  and per element and call).

## Parallel cases (`parallel/`)

- Binaries: `suite_solid` of `c698395e8` with the `mpi` feature (and with
  `mumps` for `static_linear_mumps`), built on the measurement machine in
  the project images `akantu-rs-mpi:dev` / `akantu-rs-mumps:dev`.
- Runs: `scripts/perf/suite/run_mpi.sh` (one container per run,
  `mpirun -n P --bind-to core --map-by core`): P = 1, 2, 4 on cores 0–3
  under the timing wrapper, P = 8 on cores 0–7 with the whole machine
  alone; 5 repetitions, rank counts rotated per repetition.
- Raw data: `parallel/np<P>/<case>.r<R>.jsonl`, `parallel/mumps/np<P>/…`;
  table `parallel/summary.md`.

## Profiles (`profiles/`)

- `<case>.txt`: `scripts/perf/suite/profile.sh` (`perf record -F 499
  --call-graph dwarf,16384`, the main loop of the case extended to 10 s with
  `--loop-seconds 10`; set-up cases `--reps 3`): the phases of the profiled
  run, the top 25 symbols by self and by inclusive time, the caller chains.
- `<case>.callers.txt`: the same case with `-F 299 --call-graph
  dwarf,65528` and the library of each symbol, for cases whose shorter
  stack dumps lost frames.
- Recorded in a boat.dev sandbox (AMD EPYC 4585PX, Zen 5, 4 vCPU) with the
  same binary as the baseline: shares of a profile, not times. Rust symbols
  demangled with `rustfilt`. Unresolved `libc.so.6` addresses near
  `0x1a14d3`–`0x1a1d7a` are glibc's AVX-512 `memmove`.
