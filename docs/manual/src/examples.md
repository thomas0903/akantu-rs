# Examples

This chapter corresponds to *Examples* (`examples.rst`, which includes
`examples/README.rst`) of the upstream manual. Every compiled example of
upstream (`examples/c++/**`) is a program of the facade crate
(`crates/akantu/examples/<name>/main.rs`), written line by line after the
upstream source so that the two can be compared; its integration test
(`crates/akantu/tests/examples_<name>.rs`) compares its outputs with runs of
the upstream program (dumps byte for byte, printed values, energies or
state digests bit for bit; `crates/akantu/examples/README.md` lists what is
compared for each). The meshes are the frozen Gmsh meshes of the
repository (`fixtures/meshes`), so no Gmsh is needed.

```sh
cargo run -p akantu --example static                      # writes ./paraview/static_*
cargo run -p akantu --release --example bloc_friction -- --output-dir /tmp/friction
cargo run -p akantu --release --example parallel_2d -- --local-ranks 2
cargo run -p akantu --features cgal --example embedded
```

Common options: `--output-dir`, `--mesh-dir`, `--aka_input_file`,
`--steps <n>` (a shorter run), `--no-dump`, `--solver eigen` (the sparse
solver of the implicit examples; upstream's default resolves to MUMPS) and
`--upstream-compat` (the example's upstream defects, see each source).

| Model | Examples |
|---|---|
| solid mechanics | `predefined_bc`, `user_defined_bc`, `static`, `explicit_dynamic`, `implicit_dynamic`, `example_parser`, `new_local_material`, `material_viscoelastic_maxwell_energies`, `dumper_low_level`, `dumpable_interface`, `parallel_2d` |
| cohesive elements | `cohesive_extrinsic`, `cohesive_intrinsic`, `cohesive_extrinsic_ig_tg`, `scalability_test` (with the SLURM study scripts `run.sh`, `scalability.sh` and `plot.py`) |
| diffusion | `heat_diffusion_dynamics_2d`, `heat_diffusion_static_2d`, `heat_diffusion_dynamics_3d` |
| structural mechanics | `bernoulli_beam_2_example` |
| phase field | `phase_field_notch`, `phase_field_parallel` (one rank, or two / four ranks on a distributed mesh) |
| contact | `contact_explicit_dynamic`, `contact_explicit_static`, `cohesive_contact_explicit_dynamic`, `bloc_friction` |
| embedded interfaces | `embedded` (feature `cgal`, as upstream's `USE_PACKAGES CGAL`) |
| split-node contact | `steady_state` (rate-and-state friction) |

Which mode reproduces upstream's output (the per-example column of
`crates/akantu/examples/README.md`):

- Diffusion, Hertz contact, split-node contact, phase field (the three
  diffusion examples, `contact_explicit_dynamic`,
  `contact_explicit_static`, `steady_state`, `phase_field_notch`,
  `phase_field_parallel` on one rank): by default; their
  `--upstream-compat` switches are inert in these set-ups. The implicit
  ones need `--solver eigen` to match the Eigen-pinned oracle runs
  (upstream's `_auto` is MUMPS).
- `cohesive_contact_explicit_dynamic`: by default, except the contact
  model's `gaps` from step 42 507 on, which the corrected natural
  projection (UD-363) changes in their last bits; exactly with
  `--upstream-compat`.
- `bloc_friction` (the frictional example): with `--upstream-compat`.
  By default it departs from step 1911 on: when a node in contact passes
  to the next master facet, upstream measures its slip on the previous
  facet (UD-365) and reverses the friction; the port measures it on the
  current one.
- `material_viscoelastic_maxwell_energies`: with `--upstream-compat
  --solver eigen` (UD-029, UD-010, UD-110 are corrected by default).
- `bernoulli_beam_2_example`: in neither mode as upstream runs it:
  upstream's `initFull()` is one explicit step with a zero lumped mass
  that prints `-nan` (UD-514); the port runs the static analysis the
  example prints by default and refuses that explicit step with
  `--upstream-compat`.
- `embedded`: no oracle (UD-851); by default it dumps `stress` where
  upstream dumps an empty field (UD-597), `--upstream-compat` keeps the
  empty dump.

In both modes the contact examples write the coupler's dump as upstream
does, through the couplers' model-level dumpers (`akantu::couplers::dump`,
UD-982).

The examples written for the bindings and the compiled program that
embeds the interpreter (`python_user_defined_bc`) are run with the Python
interface, behind the `python` feature (chapter [Python
interface](models/python.md)).
