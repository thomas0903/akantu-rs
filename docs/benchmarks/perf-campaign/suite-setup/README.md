# Set-up suite: baseline, scaling, profiles

Raw data of the set-up suite (`crates/akantu-bench/README.md`, "Set-up
suite"); the ranked findings are in `HOTSPOTS.md`.

| Directory | What | Machine | Commit |
|---|---|---|---|
| `baseline/base-m/` | serial cases, size `m`, 5 repetitions | AWS `m7a.2xlarge` (AMD EPYC 9R14) | `e2bebff49` |
| `baseline/py-m/` | Python cases, size `m`, 5 repetitions | same | `a1b454b0c` (extension built from `fa409279f`, the Rust library is unchanged since) |
| `baseline/twin-ab/` | Rust twin and Python loop, interleaved A/B, 5 pairs | same | `e2bebff49` / `a1b454b0c` |
| `baseline/scotch-*/` | Scotch and RCB partitions (MPI build, 1 rank) | same | `6b12c7d8c` |
| `baseline/mpi-l-p*/` | parallel cases, size `l`, 1, 2, 4, 8 ranks | same | `6b12c7d8c` |
| `baseline/coh-l/` | `cohesive_insert_tet4` at `l` with `/usr/bin/time -v` | same | `e2bebff49` |
| `scaling/{s,m,l}/` | serial mesh cases at the three sizes (screening) | 4-core AMD EPYC Milan development machine | `f51cea974` (same library and suite code as `e2bebff49`) |
| `scaling/exponents.md` | growth exponents b of t ∝ N^b (`scaling.py`) | | |
| `profiles/` | `perf record --call-graph dwarf` text reports | boat.dev sandbox (AMD Ryzen 9 9950X, 4 vCPU); the two Python files on the development machine | `e865bfe04`, `6b12c7d8c` for `ghost_elements` |

Method (AWS series): release build of the branch, one process per run
(`suite_setup --case C --size S --reps 1 --out FILE`), pinned with
`taskset -c 2`; all cases of one repetition run back to back inside one
timing job of the machine's job scheme (timing jobs run alone on cores
0–3), the case order rotated by one place per repetition
(`grouped.sh`, a variant of `scripts/perf/suite/run_ab.sh` of the
suite-solid branch that groups the runs of a repetition). Parallel cases:
`mpirun -n P --bind-to core` in the Docker image `akantu-rs-scotch:dev`
(Open MPI 5), the container restricted to the job's cores, and
`SCOTCH_PTHREAD_NUMBER=1`; 8 ranks in a whole-machine job. The tables are
`summarize.py` output (`scripts/perf/suite/summarize.py` of the
suite-solid branch): median [min, max] over the repetitions, per call, and
ns per element and call.

Digests: equal across the repetitions of every case (column `digest` of
the tables). Two notes: `dump_text_tet4` of `baseline/base-m` reports the
empty digest `cbf29ce484222325` (a defect of the case fixed in
`c4a879451`, after the series had started; its timing is unaffected);
the Python `import` case has no result (digest 0).

The scaling screening ran on a different CPU from the baseline (Zen 3
against Zen 4); the exponents are ratios between sizes of the same
machine. `cohesive_insert_tet4` at `l` needs more than the development
machine's 7 GiB (killed); its `l` point is in `baseline/coh-l/`.
