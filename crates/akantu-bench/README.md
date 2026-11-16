# akantu-bench

Benchmark drivers of akantu-rs (`docs/BENCHMARKS.md`): the cohesive
scalability test (`cohesive_extrinsic`) and the timing suites of the
performance campaign (`docs/DECISIONS.md`, D-122).

## Model suite (`suite_models`)

Timing cases for the models other than plain solid mechanics and cohesive
elements. Every case generates its mesh in code (structured grids written as
Gmsh MSH 2.2 text and read with `akantu_io::read_mesh_from_str`, so the
models see the element groups and `physical_names` data of a Gmsh mesh), runs
through the public API, prints one JSON object per phase and hashes its
results (FNV-1a over the result arrays after the last step).

```text
cargo build --release -p akantu-bench --bin suite_models
suite_models --list
suite_models --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE]
             [--loop-seconds S] [--threads N]
SUITE_VERBOSE=1 suite_models ...   # counts that show what a case did (stderr)
```

| Case | Exposes |
|---|---|
| `heat_explicit_tet4` | explicit lumped heat steps, 3-D tet4 (distributed under `mpirun`) |
| `heat_explicit_quad4` | explicit lumped heat steps, 2-D quad4 |
| `heat_implicit_hex8_kvar` | backward Euler, `T`-dependent conductivity: Newton, assembly and solve per iteration |
| `heat_static_tri6` | conductivity assembly and one static solve, 2-D tri6 |
| `phasefield_staggered_tri3` | phase field + solid staggered step, each piece a phase; both stiffness assemblies (distributed under `mpirun`) |
| `phasefield_staggered_tet4` | the same in 3-D |
| `contact_search_quad4` / `_tet4` | contact detection alone, 2-D / 3-D, many surface elements |
| `contact_explicit_quad4` | solid–contact coupler, explicit penalty steps |
| `contact_implicit_static_quad4` | solid–contact coupler, static Newton steps with the contact stiffness |
| `contact_cohesive_quad4` | solid–cohesive–contact coupler: extrinsic insertion, then crack-face contact |
| `structural_beam3_frame_static` | 3-D Bernoulli frame: stiffness assembly, one solve |
| `structural_beam3_frame_dynamic` | 3-D Bernoulli frame: implicit Newmark steps |
| `structural_dkt18_plate_static` | DKT18 plate: stiffness assembly, one solve |
| `embedded_rebar_tri3` / `_tet4` | embedded bars: intersection set-up, assemblies with the reinforcement, one solve |
| `ntn_friction_quad4` | split-node contact with slip-weakening friction, explicit steps |

Sizes: `s` (about a second, run twice by `tests/suite_models.rs`, which checks
that every digest repeats), `m` (the timing size), `l` (about 8× `m`). Cases
of serial-only models print `skip: <reason>` under `mpirun -n P` with
`P > 1`. Parallel runs need the `mpi` feature and the project's MPI image
(`docs/TESTING.md`). IGFEM has no case here: its enrichment set-up needs the
GPL-3.0-or-later sphere intersector, and a GPL switch in this crate is a
licensing decision (D-042).

Baseline, profiles and the ranked hot spots:
`docs/benchmarks/perf-campaign/suite-models/`.
