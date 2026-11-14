# Unpinned x86_64 corpus comparison

Data set "unpinned" of `docs/NUMERICAL_VALIDATION.md` ("x86_64 corpus
"): the x86_64 oracle as Eigen and OpenBLAS configure themselves on the
host (Eigen cache sizes from CPUID = l1 64 KiB / l2 512 KiB / l3 16 MiB on
the AMD EPYC 9455; OpenBLAS `Cooperlake` kernels), image
`sha256:45122c213e9fa194b6b58dcc31fc23b5fa4f7c0f64d0a452e40ae072c51c5cdb`,
strict install, regenerated 2026-11-11
02:59–03:40 UTC in staging clones on an x86_64 Linux virtual machine. A = committed
(aarch64) corpus at main 48529bcd, B = x86_64 regeneration.

| file | content |
|---|---|
| `summary-model-solver.json` | `summarize.py` of `fixture-diff` for `model/smm`, `model/smm_fd`, `solver`, `solver_model` (clone 1) |
| `summary-core-mesh-parallel.json` | same for `selftest`, `fe`, `mesh_io`, `material`, generic `model/*`, `mesh_utils`, `parallel` (clone 2) |
| `summary-io.json` | same for `io/mesh_io` (clone 3, the two `diana_3d*` cases excluded: UD-094 makes them run-to-run nondeterministic on x86_64) |
| `files-*.json` | `compare_files.py` for the non-manifest outputs: crate fixtures (random, parser, dense/gemm/complex division), `fixtures/dump`, the re-frozen Gmsh meshes vs `fixtures/meshes` |

The full `fixture-diff --json` reports (34 MB each) are not committed; every
differing quantity is listed in the summaries.
