# Pinned x86_64 corpus comparison (D-050)

Data set "pinned" (candidate authoritative) of `docs/NUMERICAL_VALIDATION.md`
("x86_64 corpus"): image
`sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5`
(`OPENBLAS_CORETYPE=Haswell`, `OPENBLAS_NUM_THREADS=1`), strict install
`cca6e460` compiled with `-DEIGEN_NO_CPUID` + Eigen's aarch64 cache-size
defaults, regenerated 2026-11-11 04:41–05:09 UTC on an x86_64 Linux virtual machine
(AMD EPYC 9455) by `oracle/x86/regenerate-corpus.sh` (lanes core, model, misc, mesh).

| file | A | B |
|---|---|---|
| `summary.json` | committed aarch64 corpus (main 48529bcd) | pinned x86_64 (all areas) |
| `files-*.json` | committed crate fixtures / `fixtures/dump` / `fixtures/meshes` | pinned x86_64 outputs (re-frozen Gmsh meshes for `files-meshes.json`) |
| `summary-pin-vs-unpinned-model-solver.json` | unpinned x86_64 (`model/smm*`, `solver*`) | pinned: the 69 differing quantities are exactly the X1 (Eigen cache size) cases |
| `summary-run-A-vs-run-B.json`, `files-run-A-vs-run-B-*.json` | pinned run A (04:41 UTC) | pinned run B (07:21 UTC): repeatability, all cases bit-identical (`io` not promoted in B: UD-094) |
| `summary-pin-vs-unpinned-core.json` | unpinned x86_64 (generic model, parallel, fe, …) | pinned: the differing quantities are the X2 (MUMPS) cases, Haswell vs Cooperlake kernels |

`dense/dense_config_comparison.txt` in `files-akantu-linalg.json` was produced
while the `default` install was still unpinned (its strict records are pinned).
