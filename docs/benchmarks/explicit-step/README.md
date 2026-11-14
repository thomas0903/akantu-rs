# Explicit step: data

Summarised in `docs/BENCHMARKS.md`, "Explicit dynamics". The harnesses and
scripts are in `scripts/perf/explicit-step/` (its README has the steps).

## `final/`: the port against both C++ installs (2026-11-12)

boat.dev `large`, AMD Ryzen 7 9700X, 8 vCPU, 15.6 GiB, Ubuntu 24.04, kernel
6.8.0-117; nothing else running. Programs: the Rust harness
`crates/akantu-solid/examples/explicit_step_timing.rs` before the
explicit-step work (label `main`), after the reviewed changes (`v98head`)
and in the final tree (`head`), host toolchain 1.99.1, workspace release
profile, x86-64 baseline; the C++ harness
`scripts/perf/explicit-step/explicit_step_timing.cc` against the strict
install (`cpp-strict`, binary `2d2bf33e…`) and the default install
(`cpp-default`, `df8af9fc…`), oracle image `akantu-oracle:dev`
`sha256:2da26d87…`. Meshes: gmsh 4.12.1 (Ubuntu package) from
`cube_{tet,hex}.geo`.

| File | What |
|---|---|
| `summary.md` | `summarize_timing.py` over 7 rounds of `timing_rotated.sh` (program order rotated cyclically): median [min, max] ns per element and step of the phases `step`, `fint`, `stress`, and the per-round paired ratios |
| `digests.txt` | FNV-1a digests of `u`, `v`, `a`, `f_int` after n steps for every program on the ten configurations (`digests.sh`), with the binary digests: the Rust programs and `cpp-strict` agree on all ten, `cpp-default` differs |
| `codegen-x86_64.txt` | `codegen_check.sh`: fast-math flags and `llvm.fma` in the IR per crate, FMA, horizontal-add and packed instructions in the hot functions of the release binaries |
| `meshes-SHA256SUMS` | the five meshes |
| `driver/summary.md`, `driver/paired.txt`, `driver/provenance.txt` | the cohesive benchmark driver (`crates/akantu-bench`, `cohesive_extrinsic`) before (`main`, `f35e5e25…`) and after (`head`, `df7442d2…`) the explicit-step work: `crates/akantu-bench/scripts/timing_ab.sh`, h = 0.12 frozen mesh, 500 steps with the static preload, materials `cohesive` and `elastic`, 5 rotated repetitions; `summary.md` = `compare_cpp.py --cpp main --rust head`, `paired.txt` = `driver/bench_paired.py` (per-repetition ratios and peak RSS) |

The raw JSON lines and the per-run directories are not kept.

## `development/`: the explicit-step work (2026-11-11)

boat.dev `large`, AMD Ryzen 9 9950X, 8 vCPU, 16 GB, Ubuntu 24.04, kernel
6.8.0-117. `run_ab.sh` series (program order alternated per round, 5
rounds; 3 for the meshes of about one million elements and for the cohesive
step), each program a release build of one commit of the work (`BUILDS.md`
maps the labels to commits and trees). `tables.md` holds the tables of
every series, made with `tables.py`:

| Series | Programs | Cases |
|---|---|---|
| `series1` | base, fe1 (FE fast paths), fd1 (+ finite-deformation residual kernel) | tet4 20k / 200k, tet10 20k, hex8 20k; ss, fd |
| `series2` | fd1, o5a (filter by value), o5b (stress temporaries), o5c (assembly buffers) | same |
| `series3` | o5c, g1 (Green strain), f2 (fused finite-deformation kernel) | same, fd |
| `series4` | f2, cur (scatter), int2 (integration loop order) | same, ss and fd |
| `series5` | int2, head (Cauchy pass, errors built on failure), head-lto (fat LTO, one codegen unit) | tet4 20k / 200k ss, fd; tet10 20k fd; hex8 20k ss, fd |
| `series6` | coh-cur, coh-cauchy, coh-head, coh-head-lto (the cohesive step) | tet4 20k / 200k fd |
| `finalA` | base, cur, cpp-strict | tet4, tet10, hex8 at 20k and 200k; ss, fd |
| `finalB` | base, cur | tet4 874k, hex8 1M; ss, fd |
| `finalC` | coh-base, coh-cur (cohesive extrinsic step) | tet4 20k / 200k / 874k |

Mesh SHA-256 (gmsh 4.12.1, `make_meshes.sh`):

```text
9e1bb6ad7f72143764a62f8f0c0c460cd329751b5d97578a9283426fb97d1729  tet4-20k.msh
fce4aca08ff8c9f10106802c69ac10bf8b1ab2f74bd847de3f4256b28ae1aac1  tet4-200k.msh
2e9d7c850dc0a41579c9761ae451f5b81e786ea51f16e4f5bf3d2c74c122789a  tet4-1m.msh
2d949041487d9a18667dc8c5abad4ffd3a290d8945249b3b68cf5253a2212734  tet10-20k.msh
5bce79f6e06447a423d1080538ca82101eb823e184d9bb0bba187751b0df9b81  tet10-200k.msh
263c5650aae0897229b034e6dc6db1dfca62ca261f2d61450a5356ae68083f31  hex8-20k.msh
20224f55f5363882ecba68a13299446f01169993d4746859c8b94a0be30e15eb  hex8-200k.msh
727ca85d28d6e5eb4492bb676ac0e2443eef2492e073252289f2f234a8436a8a  hex8-1m.msh
```

C++ in this series: the strict install only (binary `6310d387…`, image
`sha256:45122c21…`). The raw JSON lines are not kept.
