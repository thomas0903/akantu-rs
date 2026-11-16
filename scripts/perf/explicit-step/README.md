# Explicit-step timing harnesses

Times the explicit step of the port and of the C++ reference on the same
problem, mesh and machine, phase by phase, and checks that both compute the
same bits. Results: `docs/BENCHMARKS.md`, "Explicit dynamics"; data:
`docs/benchmarks/explicit-step/`.

The problem is `examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc`:
a unit cube, `elastic` with `rho = 2500`, `E = 70e9`, `nu = 0.22`, small
strain or `finite_deformation = true` (`--fd`), lumped mass,
`Δt = 0.8 Δt_crit`, a sine-Gaussian displacement pulse along x. Phases:
`step` (the whole `solve_step`), `fint` (internal forces), `stress` (every
law's stresses, gradient included), and the kernels `grad`, `btd`, `integ`,
`asm`.

| File | What |
|---|---|
| `cube_tet.geo`, `cube_hex.geo`, `make_meshes.sh` | the meshes (tet4, tet10, hex8 at about 20 k, 200 k and 1 M elements), made with the local gmsh; freeze them for a session, since another gmsh version or platform gives other meshes |
| `explicit_step_timing.cc`, `CMakeLists.txt`, `cpp_build.sh`, `cpp_run.sh` | the C++ twin of `crates/akantu-solid/examples/explicit_step_timing.rs`, built in the oracle image against `/build/install-strict` or `/build/install-default` |
| `run_ab.sh` | an interleaved series: every round runs every case with every program, one process per run, program order alternated between rounds |
| `timing_rotated.sh` | the same with the program order rotated cyclically per round (the method of the final series) |
| `tables.py`, `summarize.py` | tables of `run_ab.sh` output |
| `summarize_timing.py` | tables of `timing_rotated.sh` output with per-round paired ratios (its column labels are those of the final series) |
| `digests.sh` | the FNV-1a digest of `u`, `v`, `a`, `f_int` after N steps for several builds of the port (the C++ driver prints the same digest with `cpp_run.sh … DIGEST_STEPS`): identical digests mean bitwise identical computations |
| `codegen_check.sh`, `ir_fma_owners.py` | fast-math flags and `llvm.fma` in the LLVM IR of the crates, FMA and horizontal-add instructions in the release binaries |

## Steps

Requirements: a Rust toolchain (`rust-toolchain.toml`), gmsh, Docker with
the oracle image `akantu-oracle:dev` and its build volume
(`docs/ORACLE_BUILD.md`). Use an otherwise idle machine and one run at a
time. The default directories are `~/perf/bin`, `~/perf/cpp-bin` and
`~/perf/meshes` (`BIN_DIR`, `CPP_BIN_DIR`, `MESH_DIR`).

```sh
scripts/perf/explicit-step/make_meshes.sh ~/perf/meshes tet4-20k tet4-200k tet10-20k hex8-20k hex8-200k
cargo build --release -p akantu-solid --example explicit_step_timing
mkdir -p ~/perf/bin && cp target/release/examples/explicit_step_timing ~/perf/bin/port
scripts/perf/explicit-step/cpp_build.sh ~/perf/cpp-bin strict default
# bitwise check first: the digest after 30 steps, port and strict C++ build
~/perf/bin/port --mesh ~/perf/meshes/tet4-20k.msh --digest-steps 30
scripts/perf/explicit-step/cpp_run.sh ~/perf/cpp-bin strict ~/perf/meshes tet4-20k 1 1 0 0 check 30
# 7 rounds, program order rotated, the five meshes in small strain and finite deformation
scripts/perf/explicit-step/timing_rotated.sh out.jsonl 7 "cpp-strict cpp-default port" \
  "tet4-20k:30 tet4-20k:30:fd tet4-200k:6 tet4-200k:6:fd tet10-20k:6 tet10-20k:6:fd hex8-20k:8 hex8-20k:8:fd hex8-200k:3 hex8-200k:3:fd"
python3 -I scripts/perf/explicit-step/summarize.py out.jsonl cpp-strict port
```

`MESH:STEPS[:fd]` gives the number of calls per timed phase. To compare two
builds of the port, build the harness at each commit into `~/perf/bin/<label>`
and list both labels. The cohesive extrinsic step has its own harness,
`crates/akantu-cohesive/examples/cohesive_step_timing.rs` (labels `coh-*` in
`run_ab.sh`). The release profile is the workspace's (no LTO, no
`target-cpu`, no fast-math). The build configurations (`-C target-cpu`,
LTO, codegen units, PGO) were checked for bit identity and timed in
`docs/benchmarks/perf-campaign/build-profile/` (scripts in
`scripts/perf/build/`); after a toolchain update, rerun `digests.sh` for
such a build before timing it.
