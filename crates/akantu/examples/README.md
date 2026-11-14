# Examples

Rust ports of the upstream Akantu examples (`examples/c++/**` of the
reference, commit `35a85178`), one cargo example per upstream example (all
compiled examples are ported; the Python examples and EX-012 are not yet):

```sh
cargo run -p akantu --example static                 # writes ./paraview/static_*
cargo run -p akantu --release --example explicit_dynamic -- --output-dir /tmp/explicit
cargo run -p akantu --release --example parallel_2d -- --local-ranks 2
```

Each example lives in `<name>/main.rs` with the upstream input file copied
next to it; meshes are the frozen Gmsh 4.13.1 meshes of the oracle
(`fixtures/meshes/gmsh-4.13.1/examples/c++`, decision D-009), so no Gmsh is
needed. Outputs (`paraview/`, CSV files) go below the current directory, as
upstream writes them in its run directory. Common options (see
`common/mod.rs`): `--output-dir`, `--mesh-dir`, `--aka_input_file`,
`--steps <n>` (reduced runs), `--no-dump`, `--solver eigen` (pin the sparse
solver of implicit examples), `--upstream-compat` (reproduce the upstream
defects of an example, D-003; **not** UD-352 of `scalability_test`, see below),
`--quiet`.

Integration tests: `crates/akantu/tests/examples_<name>.rs` (the upstream
`<name>-test` ctests; reduced runs in the fast tier, full runs in optimised
builds). Oracle runs of the upstream binaries: `oracle/probes/examples/`
(fixtures `fixtures/examples/<case>/`).

## Ported

| Example | Upstream | Parity row | Notes |
|---|---|---|---|
| `predefined_bc` | `solid_mechanics_model/boundary_conditions/predefined_bc` | EX-013 | `FixedValue` Dirichlet functors |
| `user_defined_bc` | `solid_mechanics_model/boundary_conditions/user_defined_bc` | EX-014 | a user `DirichletFunctor`, `FromTraction` |
| `static` | `solid_mechanics_model/static` | EX-021 | static Newton–Raphson |
| `explicit_dynamic` | `solid_mechanics_model/explicit` | EX-019 | explicit lumped mass, energies |
| `implicit_dynamic` | `solid_mechanics_model/implicit` | EX-020 | implicit Newmark (trapezoidal), modal solution |
| `example_parser` | `solid_mechanics_model/io/parser` | EX-027 | user parameters of the input file, `FromStress` |
| `new_local_material` | `solid_mechanics_model/new_material` | EX-023 | a new material written with the `Material` trait (UD-472) |
| `dumper_low_level` | `solid_mechanics_model/io/dumper` | EX-025 | low-level ParaView dumpers, filtered mesh; **default ≠ upstream**: the wheels turn (UD-473 corrected: upstream's `applyRotation` writes into a copy, its wheels never turn), so 200 of the 404 files differ; `--upstream-compat` gives upstream's 404 files byte for byte |
| `dumpable_interface` | `solid_mechanics_model/io/dumper` | EX-026 | the `Dumpable` registry, group dumper; the wheels turn by default (UD-473, as `dumper_low_level`); in both modes the `train` files and the `wheels` pieces differ from upstream's (UD-474, no switch): upstream's `train` piece holds a 1,496-value `color` array for 15,244 cells (ParaView reports an error for `color` and shows the piece empty: "Cannot read cell data array "color" … may be too short") and its `wheels` pieces hold whole-mesh arrays for 810 points and 1,496 cells; the port writes every field over its own mesh part and leaves the wheel-filtered `color` off `train` |
| `parallel_2d` | `solid_mechanics_model/parallel` | EX-022 | distributed explicit dynamics (`--features mpi`, `--local-ranks N`, `--partition`); see the UD-475 note below; the padded `stress` (`addDumpFieldTensor`) never equals upstream's, see the stress note below (UD-110, UD-111) |

**`new_local_material` and the `damage` dump field.** Upstream's example adds
the dump field `damage` unconditionally, which throws at registration when no
law of the model has a `damage` internal. The port registers the field only
when a law has that internal (the `local_damage` law of the example has it),
so an input file whose laws have none runs and dumps the other fields
(UD-683, category B).

**Dumps of distributed runs (UD-475).** As upstream, the per-rank dumps of
`parallel_2d` hold the **pure-ghost** points of each rank, whose nodal fields
are advanced by the time integration but never synchronised: after 10,000
steps they drift to values of about 1.76 while the real nodes stay near
1.2e-6. ParaView's colour range of a whole array is then dominated by these
meaningless points: restrict the range to the cells (or use the
`.pvtu` piece of the owning rank) when inspecting `displacement`, `velocity`
or `acceleration`. The values are reproduced for parity (UD-475, category C).

**Padded stress of `parallel_2d` (UD-110, UD-111).** `addDumpFieldTensor("stress")`
pads the 2-D stress to a 3 × 3 tensor. Upstream writes each 9-component
tuple as a zero-filled 9 × 9 square (81 values per cell, `NumberOfComponents="9"`:
ParaView takes the first 9 × n values as the n cells' tuples, so cell i shows
row i mod 9 of cell ⌊i/9⌋'s square, a meaningless field) and never writes the out-of-plane
shear entries of the padded tensor (`PadderGeneric::pad` fills a new,
uninitialised Eigen matrix: positions (0,2), (1,2), (2,0), (2,1), values up
to 8.19e227 in the oracle's files). The port writes 9 values per cell by
default (UD-110 corrected) and upstream's 81 with `--upstream-compat`; the
four out-of-plane shear entries are zero in both modes (UD-111, never
reproduced), so the stress pieces never equal upstream's byte for byte.
`examples_parallel_2d` compares every other array of every per-rank piece
bitwise and the stress of rank 0's step-200 piece (the one kept in the
fixture) value by value, except those four entries; the ParaView visual
check (`docs/reports/2026-11-12-paraview-visual-check.md`) found the same
for all 102 pieces.

**Oracle fixtures (D-050).** The fixtures `fixtures/examples/<case>/`
were regenerated on the **unpinned oracle build pinned at run
time** (image `sha256:45122c21…`: Eigen's cache sizes
16384 / 524288 / 524288 through `Eigen::setCpuCacheSizes` in the variants
and the `LD_PRELOAD` shim `eigen_cache_pin.so` for every run, and
`OPENBLAS_CORETYPE=Haswell`; not D-050's pinned build,
which gives the same files):
`example_parser_eigen` is then byte-identical to the port. The earlier
unpinned runs of `example_parser`, `example_parser_eigen`, `static` and
`implicit_dynamic` are kept as recorded variants `<case>_unpinned`
(diagnostics only). The printed output (material block, energies, progress)
is compared with upstream's `stdout.txt`.

## Ported (cohesive examples)

| Example | Upstream | Parity row | Notes |
|---|---|---|---|
| `cohesive_extrinsic` | `solid_mechanics_cohesive_model/cohesive_extrinsic` | EX-015 | extrinsic insertion in a window (`set_limit`), explicit, energies |
| `cohesive_intrinsic` | `solid_mechanics_cohesive_model/cohesive_intrinsic` | EX-017 | intrinsic `cohesive_bilinear` inserted at initialisation |
| `cohesive_extrinsic_ig_tg` | `solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg` | EX-016 | intergranular/transgranular laws (`MaterialCohesiveRulesSelector` chain), `FlagOnly`, a velocity functor (UD-633) |
| `scalability_test` | `scalability_test` | EX-018 | static + explicit cohesive benchmark driven by the input file's `user` section (`--aka_input_file …/material-cohesive.dat`); compared serially (on several ranks it runs, not compared, EX-049); UD-352 (D-064: kept by `--upstream-compat`, UD-352 not reproduced), UD-803; `output_paraview = true` (off in both input files) writes upstream's two series, `extrinsic` (solid model) and `solid_mechanics_model_cohesive` (`cohesive elements` dumper), byte-identical with `--upstream-compat` (oracle cases `scalability_test{,_cohesive}_paraview`; by default the empty pieces before the first insertion use valid base64, UD-114, and the cohesive run departs with UD-217) |

The cohesive examples read upstream's committed meshes (`triangle.msh`,
`cube.msh`, copied next to the sources) or the frozen `square.msh`; their
`--upstream-compat` sets every upstream cohesive defect
(`CohesiveUpstreamCompat::upstream()`, D-003): only `scalability_test`
(3-D, UD-217) runs differently with it. For `scalability_test` the compat run
keeps D-064's rule (UD-352 is not reproduced); its outputs
are bitwise upstream's all the same (5000 steps, by a different internal path:
the port's cohesive assembly refreshes the position during the static solve).
Each example returns its standard
output (and, for the tests, per-step energies) so that the integration tests
compare them with the oracle runs of `oracle/probes/examples`
(`fixtures/examples/cohesive_*`, `scalability_test*`).

**Dissipated energy of `scalability_test` (UD-803).** The `Ed` column of
`energies.csv` and the final `Cohesive info: dissipated energy` line are
upstream's trapezoidal bookkeeping of the cohesive work
(`getEnergy("dissipated")`, reproduced bitwise), **not a physical
dissipation** for this benchmark: its explicit time step does not resolve
the cohesive softening (`δc,eff` ≈ 0.2 µm), half of the cohesive points
go from their insertion traction to fully broken within one step, and the
trapezoid then books a sign-indefinite `½ t_ins·Δδ`. With
`material-cohesive.dat` the run ends with `Ed = −15.67` J (`--upstream-compat`;
+19.72 J with the UD-217 correction), while the law dissipates
`½ σc,eff min(δmax, δc,eff)` per point, i.e. `Gc` per broken point:
+260.06 J (+237.53 J). The 2-D `cohesive_extrinsic`, whose softening is
resolved, meets upstream's own 0.1 % check `Ed = 200√2`.

**The velocity functor of `cohesive_extrinsic_ig_tg`** records the
velocities and writes them after the boundary-condition application (a
Rust `DirichletFunctor` has no access to the other model fields; same state
as upstream's write from inside the functor). In the upstream example the
write changes no value (the blocked side DOFs already move at ±0.1 from the
initial condition); the test with the initial velocity `0.05 x` and its
oracle variant `cohesive_extrinsic_ig_tg_vel` make it observable.

## Ported (the other models)

| Example | Upstream | Parity row | Comparison with the oracle | Default = upstream? |
|---|---|---|---|---|
| `heat_diffusion_dynamics_2d` | `diffusion_model/heat_diffusion_dynamics_2d` | EX-005 | unmodified upstream binary: the 150 dumps and the `.pvd` byte-identical, standard output | yes (no switch) |
| `heat_diffusion_static_2d` | `diffusion_model/heat_diffusion_static_2d` | EX-006 | Eigen-pinned variant: both dumps byte-identical; upstream binary (MUMPS): the unsolved arrays bitwise | yes (no switch; `--solver eigen` for the Eigen-pinned variant) |
| `heat_diffusion_dynamics_3d` | `diffusion_model/heat_diffusion_dynamics_3d` | EX-007 | unmodified upstream binary: the 10 dumps, `.pvd`, standard output | yes (no switch) |
| `embedded` | `embedded_model/embedded` | EX-008 | feature `cgal`; **no oracle comparison**: on the patched embedded oracle variant (D-060: upstream's embedded sources do not compile) the upstream example crashes in `initFull` (UD-851); independent checks only | no oracle; the default dumps `stress` where upstream dumps an empty field (UD-597), `--upstream-compat` keeps the empty dump |
| `steady_state` | `node_to_node/steady_state` | EX-009 | unmodified upstream binary: the 101 dumps, `.pvd` and energies file byte-identical; full-state variant: energies (17 digits) and state digest of every step | yes (switches inert; both modes tested) |
| `phase_field_notch` | `phase_field_model/phase_field_notch` | EX-010 | Eigen-pinned variant: state digest of every load step, the 10 dumps and the two cluster dumps byte-identical; upstream binary (MUMPS): initial dump and cluster dumps | yes, with `--solver eigen` (switches inert; both modes tested) |
| `phase_field_parallel` | `phase_field_model/phase_field_parallel` | EX-011 | one rank: Eigen-pinned variant byte-identical; two and four ranks (recorded partitions, 100 steps, Eigen): initial dumps and the step-100 mesh / partition arrays byte-identical with upstream's runs, values within 1e-14 of the port's one-rank run, see below | yes, with `--solver eigen` (switches inert; both modes tested on 1, 2 and 4 ranks) |
| `contact_explicit_dynamic` | `contact_mechanics_model/hertz/contact_explicit_dynamic` | EX-001 | unmodified upstream binary: the 20 dumps, standard output and error (energies) identical; full-state variant every 20 steps | yes (switches inert; both modes tested) |
| `contact_explicit_static` | `contact_mechanics_model/hertz/contact_explicit_static` | EX-002 | Eigen-pinned variant: the 100 dumps, the contact elements and the Newton iterations and error of every step bitwise; upstream binary (MUMPS): unsolved arrays | yes, with `--solver eigen` (switches inert; both modes tested) |
| `cohesive_contact_explicit_dynamic` | `contact_mechanics_model/hertz/cohesive_contact_explicit_dynamic` | EX-003 | unmodified upstream binary: the 50 dumps (UD-850 for `blocked_dofs` after the insertion), standard output and error; full-state variant every 50 steps | yes except the contact `gaps` (field and dumps) from step 42 507 (UD-363, ≤ 2e-14 m, read by nothing); every value exactly with `--upstream-compat`. In both modes the 31 dumps after the insertion (19…49) are not byte-identical: the port's `blocked_dofs` has a row for each of the 18 points, upstream's 15 (UD-850, no switch: upstream's array is shorter than the piece; its 15 values equal the port's first 15) |
| `bloc_friction` | `contact_mechanics_model/friction/bloc_friction` | EX-004 | unmodified upstream binary: the 301 dumps and standard output; full-state variant every 30 steps | **no**: departs at step 1911 (UD-365, `trial_slip_on_previous_element`: upstream measures the friction slip on the previous master facet); upstream with `--upstream-compat` |
| `material_viscoelastic_maxwell_energies` | `solid_mechanics_model/new_material/viscoelastic_maxwell` | EX-024 | Eigen-pinned variant: the 1002 dumps, `.pvd`, output file, the energies (17 digits) and Newton iterations of every step; upstream binary (MUMPS): output file | **no** (UD-029, UD-010, UD-110); upstream with `--upstream-compat --solver eigen` |
| `bernoulli_beam_2_example` | `structural_mechanics_model/bernoulli_beam_2_example` | EX-028 | `_static` Eigen-pinned variant (UD-514): printed values, displacement, stresses, internal forces bitwise, dump byte-identical; the upstream binary's explicit step (`-nan`) is refused by the port | **no**: the default runs `_static` (UD-514), which the compared variant runs too (`--solver eigen`); upstream's explicit step (`-nan`) is refused in both modes |

Every example with defects of its model selects upstream's behaviour with
`--upstream-compat` (the switches each one sets are named in its source);
the last column says which mode reproduces upstream.
By default 9 of the 13 examples give upstream's output: the three
diffusion examples, the two Hertz contact examples, the cohesive contact
example (up to its `gaps`), `steady_state` and the two phase-field
examples, whose switches are inert in these set-ups; their oracle
comparisons run in both modes. `bloc_friction` and the viscoelastic
example differ by default and reproduce upstream with `--upstream-compat`;
the beam runs the static analysis it prints in both modes; `embedded` has
no oracle. The implicit examples take `--solver eigen` for the
Eigen-pinned variants (upstream's `_auto` is MUMPS in the oracle). Oracle
cases: `oracle/probes/examples/cases.tsv` (rows), fixtures
`fixtures/examples/<case>`.

**Coupler dumps (contact examples).** Upstream every model registers its
ParaView dumper on the mesh and makes it the mesh's default, and
`coupler.setBaseName`, `coupler.addDumpField*` and `coupler.dump()` act on
the mesh's default dumper; the coupler resolves a field with the contact
model first, then the solid model (so `blocked_dofs` is the contact model's
one-component array, never set). The examples dump through the couplers'
model-level dumpers (`akantu::couplers::dump`, CPL-001, UD-982), which act
on that series and read each field from the model that provides it
(byte-identical dumps).

**Shared default dumper (phase-field examples).** For the same reason the
phase-field examples' `model.setBaseName` renames the phase-field model's
dumper (the last one registered), and in `phase_field_parallel`
`phase.addDumpField("partitions")` / `phase.dump()` act on the series the
solid model's calls fill: the port adds the partitions to, and dumps from,
the solid model's dumper.

**`phase_field_parallel` (EX-011).** On one rank upstream's
`mesh.distribute()` only flags the mesh distributed and builds empty
synchronizers (UD-011), which changes no value: the port keeps the mesh
serial there and dumps the partitions (all 0) as upstream does. On two and
four ranks the port distributes the mesh and runs the coupler on it (the
`_pfm_damage` / `_for_dump` synchronisation, MPI-044/045). The
tests (`examples_phase_field_parallel::phase_field_parallel_{two,four}_ranks`)
replay the oracle's recorded partitions
(`fixtures/examples/phase_field_parallel_partition{,_np4}`) for 100 load
steps with the Eigen solver in both modes: the initial dumps, both `.pvtu`
files and the positions, connectivity, offsets, types and partitions of
the step-100 pieces are byte-identical with upstream's two- and four-rank
runs (`fixtures/examples/phase_field_parallel{,_np4}`), and the step-100
displacement and damage are within 1e-14 of the port's one-rank run
(decision D-082's second check). Upstream's own values at two and four
ranks are not compared: 18 of the 30 (36 of the 50) output files differ
between three identical runs of upstream's binary (cause not established:
the distributed MUMPS solves and the arrival order of upstream's ghost
accumulations, `waitAny`, D-039, are both candidates and were not
separated). The ghost-element damage gradients of UD-004 enter the
distributed runs only and are inert in the solves.

**`bernoulli_beam_2_example` (UD-514).** Upstream calls `initFull()`
without an analysis method (`_explicit_lumped_mass`): its `solveStep` is
one explicit step with a zero lumped mass (UD-005) and every printed value
is `-nan`. The port runs `_static` (the static solution the example
prints, `TestStructBernoulli2`'s values); with `--upstream-compat` the
explicit step is refused (the port does not divide by a zero lumped mass).

**`embedded` (EX-008).** Upstream builds the example with the package
`CGAL`; the port's example needs the facade feature `cgal` too (`cargo run
-p akantu --features cgal --example embedded`), although its intersector is
the LGPL `LinearScan` of `akantu-geometry` (order-independent result, no
GPL-derived code). Upstream's embedded package does not compile at the
reference commit; built against the patched variant of
`docs/ORACLE_BUILD.md` (`oracle/probes/examples/embedded`), the
upstream example ends with signal 11 in `initFull` (a null background
filter in `MaterialReinforcement::initBackgroundShapeDerivatives`, UD-851),
so the example is checked independently only (`examples_embedded.rs`, and
the same set-up's equilibrium checks in `akantu-embedded`).

**Scalability study (EX-049).** `scalability_test/run.sh` (one SLURM job),
`scalability.sh` (the study: 1 to 720 ranks, 72 per node) and `plot.py`
(`collect` merges the chronometer CSV of the jobs' standard error,
`plot` draws upstream's time-to-solution figure) are ported for the Rust
`scalability_test`; the project does not submit them.

### The upstream tutorials (`examples/**/README.rst`)

*Adapted from upstream's example descriptions (Copyright © 2010-2023 EPFL,
LSMS; Akantu is distributed under the GNU Lesser General Public License,
version 3 or later), for the ported programs.*

* **Diffusion** (`diffusion_model`). `heat_diffusion_static_2d`: a square
  plate of 1 m² at 100 K everywhere but a non-centred hot point held at
  300 K; the material has a density of 8940 kg/m³, a conductivity of
  401 W/m/K and a specific heat capacity of 385 J/K/kg.
  `heat_diffusion_dynamics_2d` solves the same problem dynamically with an
  explicit time scheme (`ExplicitLumpedMass`; time step 0.12 s, 15 000
  steps ≈ 30 minutes). `heat_diffusion_dynamics_3d` is the 3-D version: a
  cube at 100 K with a centred sphere held at 300 K.
* **Contact** (`contact_mechanics_model`). `contact_explicit_static` and
  `contact_explicit_dynamic` solve a 2-D Hertz contact patch test with the
  `CouplerSolidContact`; they differ only by the analysis method (`Static`
  / `ExplicitLumpedMass`). The input file has a `contact_detector` and a
  `contact_resolution penalty_linear` section. `bloc_friction`: a block is
  compressed against a wall, then undergoes a constrained lateral
  displacement of its upper surface; a friction coefficient of 0.3 at the
  interface creates a stick-and-slip phenomenon. Wall and block have the
  same properties; `contact_detector` holds the numerical parameters of the
  detection, `contact_resolution` the interface properties and penalties
  (`mu`: friction coefficient; `epsilon_n` prevents interpenetration,
  `epsilon_t` detects friction; `is_master_deformable = false` ignores the
  wall's deformation to reduce the computation time).
* **Phase field** (`phase_field_model`). `phase_field_notch` sets up a
  quasi-static fracture simulation: a square plate with a notch under an
  imposed displacement at its top (mode I). It also extracts the clusters
  delimited by the mesh boundaries and the elements whose damage is beyond
  a threshold, e.g. to extract fragments after crack propagation.
* **Scalability test** (`scalability_test`). A cube under a compressive
  force on top and a shear force on four sides, with an elastic material
  or with cohesive elements inserted on the fly. Upstream's `cube.geo`
  generates about 4 500 000 elements and 730 000 nodes (`gmsh -3 cube.geo
  -o cube.msh`); the committed `cube.msh` is a tiny cube to test the code.

## Python examples

Upstream's Python examples (`examples/python/**`) are **not** rewritten
in Rust: the scripts are copied unchanged into `python/examples/` and run
with the port's `akantu` package; `python/examples/harness.py` runs a
script in a fresh directory (its input files from the parser corpus, the
frozen Gmsh meshes of D-009) and compares everything it prints, writes and
computes with the same script run by upstream's module in the pinned
oracle image (fixtures `fixtures/python/examples/<case>`, tests
`python/tests/examples/test_python_examples.py`; `docs/TESTING.md`). Upstream's `_auto` sparse solver is MUMPS in the
oracle build: a script with a linear solve is compared with an oracle run
pinned to Eigen through a marked variant input file
(`python/examples/variants/`), the port's unmodified run being the
default (Eigen) one.

The dumps of these scripts are not byte-identical with upstream's where a
script dumps `strain` (or another matrix-valued field): upstream writes each
4-component 2-D strain as a zero-filled 4 × 4 square (16 values per cell,
UD-110), which ParaView reads as rows of other cells' squares; the port
writes the 4 values. The harness maps upstream's squares back to tuples (rule
`UD-110`: the top-left 2 × 2 block of each square) and the arrays are then
bitwise equal; in ParaView only the port's `strain` can be read. Cohesive pieces dumped before the first
insertion hold no cell: upstream's base64 streams of their empty arrays are
invalid, the port's are valid (rule `UD-114`).

| Script (`examples/python/…`) | Row | Case(s) | Comparison |
|---|---|---|---|
| `solid_mechanics_model/stiffness_matrix/stiffness_matrix.py` | EX-030 | `stiffness_matrix` | unmodified, bitwise (printed matrix, dense K) |
| `solid_mechanics_model/dynamics/dynamics.py` | EX-031 | `dynamics` | unmodified, bitwise (251 energy lines, 26 dumps) |
| `solid_mechanics_model/plate-hole/plate.py` | EX-032 | `plate_hole`, `plate_hole_eigen` | Eigen-pinned oracle, bitwise |
| `solid_mechanics_model/custom-material/custom-material.py` | EX-033 | `custom_material` | unmodified, bitwise (Python `Material`) |
| `solid_mechanics_model/custom-material/bi-material.py` | EX-034 | `bi_material`, `bi_material_eigen` | Eigen-pinned marked variant script (dict form → parsed law), bitwise |
| `solid_mechanics_model/solver_callback/solver_callback.py` | EX-035 | `solver_callback`, `solver_callback_eigen` | Eigen-pinned oracle, bitwise (`K0.mtx`/`K1.mtx` with upstream's 15 digits, UD-061) |
| `solid_mechanics_model/eigen_modes/eigen_modes.py` | EX-036 | `eigen_modes`, `eigen_modes_eigen` | Eigen-pinned oracle, bitwise incl. SciPy `eigsh`; `gmsh` stand-in with the oracle's mesh (x86_64) |
| `solid_mechanics_cohesive_model/cohesive/plate.py` | EX-037 | `cohesive_plate`, `cohesive_plate_eigen` | Eigen-pinned oracle, bitwise (cohesive dumper) |
| `solid_mechanics_cohesive_model/fragmentation/fragmentation.py` | EX-039 | `fragmentation` | unmodified, bitwise (5000 steps, energies, fragments; `nodes`: UD-085) |
| `solid_mechanics_cohesive_model/multi-material/plate.py` | EX-040 | `multi_material`, `multi_material_eigen` | Eigen-pinned oracle, bitwise |
| `phase_field_model/phasefield-static.py` | EX-041 | `phasefield_static`, `phasefield_static_eigen` | Eigen-pinned oracle, bitwise (1500 load steps) + independent checks |
| `phase_field_model/phasefield-dynamic.py` | EX-042 | `phasefield_dynamic_full`, `phasefield_dynamic_eigen_full` | Eigen-pinned oracle, bitwise to its end (the coupler's dumps, UD-982) |
| `solid_mechanics_cohesive_model/cohesive/custom_material.py` | EX-038 | `cohesive_custom_material_full`, `cohesive_custom_material_eigen_full` | Eigen-pinned oracle; both runs raise at line 191 (UD-980: upstream binds no `getNormals`), bitwise before: dumps, variables and probes of the Python cohesive law's insertion where the script raises |
| `contact_mechanics_model/compression/compression.py` | EX-043 | `compression_full` | bitwise to its end with the port's `setUpstreamCompat(True)` (coupler dumps) |
| `contact_mechanics_model/compression/detection-explicit.py` | EX-044 | `detection_explicit_full` | bitwise to its end with the port's `setUpstreamCompat(True)` (the selector's mesh group `contact_surface`); wall-clock time masked |
| `contact_mechanics_model/friction/bloc_friction.py` | EX-045 | `bloc_friction_full` | bitwise to its end with the port's `setUpstreamCompat(True)` (coupler dumps) |
| `structural_mechanics_model/structural_mechanics.py` | EX-046 | `structural_mechanics_full`, `structural_mechanics_eigen_full` | Eigen-pinned oracle (parsed model section), bitwise to its end (1 000 000 steps; writable `getElementMaterial` / `getExternalForce`) |
| `structural_mechanics_model/structural_mechanics_softening.py` | EX-047 | `structural_mechanics_softening_full`, `structural_mechanics_softening_eigen_full` | Eigen-pinned oracle (parsed model section), bitwise to its end (600 000 steps) |
| `structural_mechanics_model/structural_mechanics_dynamics.py` | EX-048 | `structural_mechanics_dynamics_full`, `structural_mechanics_dynamics_eigen_full` | Eigen-pinned oracle (parsed model section), bitwise to its end (900 steps) |

**`python_user_defined_bc` (EX-012)** — upstream's C++ program that embeds
Python (`pybind11::embed`) — is a cargo example of the facade behind its
feature `python`: `PYO3_PYTHON=<python> cargo run -p akantu --features
python --example python_user_defined_bc` (NumPy importable by the
embedded interpreter). Its `SineBoundary` holds the unchanged upstream
`boundary_condition.SinBoundary` and calls its `compute(disp, coord,
flags)` through `py11_akantu::embed::PythonDirichletFunctor`, which hands
the node's displacement and blocked-flag rows over as zero-copy writable
NumPy views (the export protocol of the Python package, D-047). Test:
`examples_python_user_defined_bc` against the upstream binary's dump
(`fixtures/examples/python_user_defined_bc`).

## Not yet ported

| Upstream | Rows | Waits for |
|---|---|---|
| `scalability_test` on several ranks | EX-018 | multi-rank runs compared (the cohesive model runs distributed) and the benchmark campaign |
| `phase_field_parallel` on several ranks | EX-011 | the distributed phase-field model (MPI-044/045) |
