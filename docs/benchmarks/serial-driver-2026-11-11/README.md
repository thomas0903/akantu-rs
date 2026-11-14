# Serial cohesive driver against both C++ installs (2026-11-11)

The port's driver (`cohesive_extrinsic`, binary `964215fe…`, static solve
`eigen`) against the C++ harness built on the strict install (`38cda4fc…`)
and on the default install (`48e0675f…`), on one x86_64 Linux virtual machine
(boat.dev `large`, AMD Ryzen 9 9950X host, 8 vCPU, kernel 6.8.0-117), interleaved, order
rotated per repetition (`crates/akantu-bench/scripts/timing_ab.sh`, tables
by `compare_cpp.py`). h = 0.12: frozen mesh, 500 steps, 5 repetitions, the
upstream problem on both sides; h = 0.06 and 0.04: 300 and 100 steps, 3
repetitions, the port without the static preload (`rust_static_solve =
false`, another state, labelled apart in `summary.md`).

This series measures the port before the explicit-step work; every port /
C++ ratio of the explicit step in it is superseded. It is kept for one
result that `docs/BENCHMARKS.md` uses: the default and strict C++ installs
show no measurable difference in the cohesive driver (paired default /
strict medians 0.80–1.24, every range across 1).

| File | What |
|---|---|
| `summary.md` | the tables of `compare_cpp.py`: medians and paired ratios per phase |
| `provenance-h012.txt`, `provenance-h006.txt`, `provenance-h004.txt` | host, CPU, image, mesh digests, the build information of each C++ install, binary digests |
| `harness-binaries.txt`, `meshes.sha256` | binary and mesh digests |
