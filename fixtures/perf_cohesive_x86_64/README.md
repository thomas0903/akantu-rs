# Cohesive scalability test — correctness reference, x86_64 regeneration

The cases of `../perf_cohesive/` (same driver, same inputs, same frozen mesh
`../perf_cohesive/meshes/cube-h0.12.msh`, 1500 steps, 3 fixed-seed
repetitions), regenerated on **x86_64** (D-043) with the
validated pipeline below:

```sh
benchmarks/cohesive-perf/scripts/gen-fixtures.sh --steps 1500 --reps 3 --out perf_cohesive_x86_64
```

on an x86_64 Linux virtual machine (AMD Ryzen 7 9700X, 8 cores), image
`akantu-oracle:dev` amd64 `sha256:45122c21…`, oracle installs built from
the x86_64 build scripts (`READY` `e168f393`; the `default` install was built
on the same machine with those unchanged scripts because only `strict` was
installed there). Every attempt was validated (`attempts.json`: 16 attempts, all
valid, exit status 0); `provenance.json` carries the runs' own build records
(strict libakantu build-id `729cf07b…`, default `4a412a2d…`).
`harness_git_dirty` is true only because the per-machine mesh statistics
(`benchmarks/cohesive-perf/meshes/cube-h*.x86_64.json`) were untracked on
that machine when the fixtures were generated (harness commit `c034ef1d`).

**Status: preparatory data, not golden fixtures**, exactly as
`../perf_cohesive/README.md` describes (format exception, gate before
golden use, comparisons kept apart); read it and `qualification.json`
first. The committed aarch64 set stays authoritative until the orchestrator
approves the x86_64 classification (D-043, `docs/ORACLE_BUILD.md`).

## Qualification (D-035(b), `qualification.json`, field level)

| Case | Qualified (bitwise over 3 runs) | Diagnostic |
|---|---|---|
| `coh_np1`, `ela_np1` | every file | — |
| `coh_np2`, `coh_np2_global` | `partition.csv`, `insertions.csv`, `summary_rank0/1.json`; in `summary.json` every count and mode field | `energies.csv`; in `summary.json` `time_step`, energies, displacement norms |

Here **all three** 2-rank runs differ from each other (on aarch64 two of
three were identical). Repeat variation at step 1500 (max over runs 2, 3 vs
1): `coh_np2` `ed` 2.1e-6, `ek` 3.4e-9, `ep` 2.4e-10; `coh_np2_global` `ed`
5.8e-8, `ek` 2.9e-11, `ep` 1.6e-12; up to step 800 ≤ 5.8e-13 (`ek`), ≤ 8.8e-15
(`ep`, `et`), ≤ 7.3e-12 (`ew`), `ed` = 0. The `coh_np2_global` `ed`
variation is 13× the aarch64 sample and 2.8× the one an independent review saw: three
samples do not bound it, as the diagnostic label says.

Enforced at generation and satisfied: JOSS-patch emulation bitwise equal to
upstream (`coh_np1`); explicit partition = `mesh.distribute()` (same
insertions and counts, `coh_np2`).

## Differences from the aarch64 set (`vs_perf_cohesive_aarch64.json`, diagnostic)

Identical across the two platforms: every `partition.csv`, every
`insertions.csv` (294 / 290 / 294 / 0 insertion steps, 404 cohesive
elements in all cohesive cases), every `summary_rank*.json`. Energies and
Δt differ (same source and flags; libm and BLAS kernels differ):

| Case | Δt rel | steps 0–800 | step 1500 |
|---|---|---|---|
| `coh_np1` | 1.8e-14 | `ep` ≤ 3.5e-15, `ek` ≤ 3.2e-13, `ew` ≤ 7.6e-12, `ed` = 0 | `ed` 3.4e-8 (> 1e-12 from step 1141), `ek` 1.7e-11, `ep` 9.6e-13, `et` 1.1e-12 |
| `ela_np1` | 1.8e-14 | as `coh_np1` | `ek` 3.2e-13, `ep` 4.7e-15, `ew` 7.6e-12 |
| `coh_np2` | 0 | `ek` ≤ 3.7e-13, `ep` ≤ 5.5e-15 | `ed` 1.5e-6, `ek` 2.4e-9 (within the 2-rank run-to-run band) |
| `coh_np2_global` | 5.5e-15 | `ek` ≤ 3.3e-13, `ep` ≤ 7.8e-15 | `ed` 1.2e-7, `ek` 6.1e-11 |

x86_64 strict vs default build (FMA contraction, `sensitivity.json`):
`coh_np1` step 1500 `ed` 6.9e-8 (first > 1e-9 at step 1239), `ek` 3.4e-11,
`ep` 1.9e-12, `et` 2.3e-12, `ew` 3.9e-12; steps 0–800 `ek` ≤ 7.3e-15, `ep`
4.4e-16, `ew` 4.1e-13; insertion sequence identical. `ela_np1`: ≤ 7.3e-15
(`ek`), ≤ 5.8e-16 (`ep`, `et`), 4.1e-13 (`ew`) over all steps.

**Consequence for the PROPOSED serial rules of `../perf_cohesive/README.md`:**
both the cross-platform difference and the x86_64 FMA perturbation exceed
three of them — Δt rel ≤ 1e-14 (observed 1.8e-14 across platforms), `ed`
rel ≤ 2e-8 after step 800 (3.4e-8 across platforms, 6.9e-8 strict vs
default on x86_64) and `ek` rel ≤ 1e-11 after step 800 (1.7e-11, 3.4e-11).
Those three proposals were derived from the aarch64 perturbation only and
are **not supported** by the x86_64 evidence; they are on hold and must be
re-derived from x86_64 baselines and reviewed (docs/NUMERICAL_VALIDATION.md)
before any use. Nothing was loosened here.
