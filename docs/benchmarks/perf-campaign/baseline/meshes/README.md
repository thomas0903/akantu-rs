# Meshes of the baseline (regenerated on the measurement machine, 2026-11-16)

Explicit-step meshes: `scripts/perf/explicit-step/make_meshes.sh` with
Gmsh 4.12.1 (Ubuntu 24.04 package, in an `ubuntu:24.04` container). All five
SHA-256 digests equal `docs/benchmarks/explicit-step/final/meshes-SHA256SUMS`
(`explicit-SHA256SUMS`).

Cohesive meshes: `benchmarks/cohesive-perf/scripts/gen-meshes.sh 0.12 0.06 0.04`
with Gmsh 4.13.1 of the oracle image (each generated twice, byte-identical).
The digests equal the recorded x86_64 statistics
`benchmarks/cohesive-perf/meshes/cube-h<h>.x86_64.json` (h = 0.12:
`e03a859d…`, h = 0.06: `20523d2d…`, h = 0.04: `1a815017…`). The timing runs
at h = 0.12 use the frozen fixture mesh
`fixtures/perf_cohesive/meshes/cube-h0.12.msh` (`6af43648…`, generated on
aarch64), as the earlier driver series did (`cohesive-SHA256SUMS`).
