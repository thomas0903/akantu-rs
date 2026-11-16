# Execution policy (D-123): measurements

Machine: an AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, 32 GiB), Ubuntu
24.04, release profile of the workspace, one process per run, pinned with
`taskset` (thread `i` of a run on core `i`). Meshes: the tet4 and hex8 cubes
of `scripts/perf/explicit-step/make_meshes.sh`, made with gmsh 4.15.2
(`meshes-SHA256SUMS`).

PLACEHOLDER
