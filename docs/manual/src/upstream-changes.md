# Upstream changes and their status in the port

The port tracks upstream commit `35a85178`, 554 commits after the last
release `v5.0.7-post1` (`git describe`: `v5.0.7-post1-554-g35a851780`).
This page maps the changes listed in upstream's `CHANGELOG.md` for the
version 5 releases (parity row DOC-018) and the notable unreleased
features of `git log v5.0.7-post1..35a85178` (DOC-020) to their status in
this release of the port. *Available* means part of this release; the
verification status of each feature is in
[`docs/FEATURE_PARITY.md`](../../FEATURE_PARITY.md).

## Released changes (CHANGELOG, version 5)

| Version | Upstream change | In the port |
|---|---|---|
| 5.0.7 | Python: fix for empty filters | the Python interface is an opt-in build (feature `python` of `akantu-python`) of upstream's code at the tracked commit, which contains the fix ([Python interface](models/python.md)) |
| 5.0.6 | Python `FEEngine`: `integrate`, `computeBtD`, `computeBtDB`, `computeNtb`, `computeNtbN`, `getShapes`, `getShapesDerivatives` | the Rust `akantu::fe::FeEngine` provides these operations (FE rows); the Python `FEEngine` binds part of them (parity row PY-057, [Python interface](models/python.md)) |
| 5.0.6 | Python group dumping: `dumpGroup`, `setGroupDirectory`, `setGroupBaseName`, `addDumpGroupField`, `addDumpGroupFieldVector` | available in Rust: `register_group_dumper`, `set_directory_to_dumper`, `set_base_name_to_dumper`, `add_dump_group_field_to_dumper`, `dump_named` ([Output](output.md#group-dumpers)), and in Python with upstream's names ([Python interface](models/python.md)) |
| 5.0.5 | JOSS paper, scalability tests (elastic, cohesive) | the scalability examples are programs of the port (`scalability_test`, [Examples](examples.md)); the benchmark harness and the measurements, Rust against C++, are in [`docs/BENCHMARKS.md`](../../BENCHMARKS.md) and [`docs/reports/02-benchmarks-and-performance.md`](../../reports/02-benchmarks-and-performance.md); the cohesive model is available on one process and on a distributed mesh ([Cohesive elements](models/cohesive.md)) |
| 5.0.4 | `SparseSolverEigen`, a sequential sparse solver | available: `sparse_solver_type = eigen`, a translation of Eigen's `SparseLU`, bit-identical to the strict oracle on the committed fixtures ([platforms](differences.md#bitwise-reproducibility-and-platforms)), also usable on several ranks ([Solver configuration](solver-configuration.md)) |
| 5.0.4 | `DiffusionModel` replaces `HeatTransferModel` (name kept) | available: `akantu::diffusion::DiffusionModel` and `HeatTransferModel` (DOF `temperature`), serial and distributed ([Diffusion (heat transfer)](models/diffusion.md)) |
| 5.0.4 | cohesive stiffness assembly fix | available: the cohesive model follows upstream's code at the tracked commit, which contains the fix (oracle replays of parity rows COH-*; [Cohesive elements](models/cohesive.md)) |
| 5.0.4, 5.0.2, 5.0.7-post0/1 | wheels and PyPI packaging | not applicable: the Python package is built locally and no wheels are published ([Python interface](models/python.md)) |
| 5.0.3 | contact mechanics Python interface and fixes | the contact mechanics model is available ([Contact mechanics](models/contact.md)) and follows upstream's code at the tracked commit. Its Python bindings (`ContactMechanicsModel`, surface selectors, `GeometryUtils`, the couplers) are available ([Python interface](models/python.md)). The dumpers of the contact model and of the couplers are available, in Rust and in Python (UD-982) |
| 5.0.1 | `applyBC` fix; const-correctness of arrays | `apply_dirichlet_bc` / `apply_neumann_bc` available; const-correctness is enforced by Rust borrowing |
| 5.0.0 | `Vector`/`Matrix` become Eigen types, proxies become `Eigen::Map` | Rust fixed-size arrays and `MatrixX` with Eigen's column-major layout and evaluation order ([Basic types](basic-types.md)) |
| 5.0.0 | static-size `make_view<size>` / `make_view<rows, cols>` | `Array::as_fixed::<N>()`, `Array::tuple_matrix::<R, C>(i)` |
| 5.0.0 | named `zip` tuples (`"a"_n = as`) | standard iterator adaptors (`zip`, `enumerate`) with Rust tuples/closures; no named tuples |
| 5.0.0 | deprecated `begin_reinterpret`, `storage()` → `data()` | `as_slice()` / `as_mut_slice()`; no reinterpret iterators |
| 5.0.0 | `get*Energy(type, index)` → `getEnergy(element)` | `energy_of_element(id, Element)` ([Solid mechanics](solid-mechanics.md#energies)) |
| 5.0.0 | `begin_/end_(node\|element)_group` → `iterate(Node\|Element)Groups` | `groups().element_groups()`, `groups().node_groups()` |
| 5.0.0 | Python: debug functions moved to `debug`, `finalize` not needed, `applyDirichletBC` → `applyBC` | the Python package has upstream's current names (`applyBC`, the `debug` sub-module; parity rows PY-026, PY-069); no `finalize` in Rust |
| 5.0.0 | removed `getForce`, `firstType()`, `lastType()` | removed in the port too: `external_force()` and `element_types(filter)` replace them |
| 4.0 (still in force) | `clear()` empties a container, `zero()` sets it to 0 | same semantics (`Array::clear`, `Array::zero`) |

## Unreleased upstream features (after v5.0.7-post1)

| Upstream feature (main commits) | In the port |
|---|---|
| `SparseSolverFactory`, new `_auto` selection of the sparse solver | available: `sparse_solver_type = auto` selects MUMPS when it is compiled in (feature `mumps`) and the run uses an MPI communicator, else `eigen` ([MUMPS](mumps.md#selecting-it)) |
| choose the sparse solver from the input file / options; `solver_options` in `initFull` | available (`sparse_solver_type`, `ModelOptions::solver_options`) |
| rewrite of the algebraic expression parser with Boost.Spirit X3 | available: the port reproduces both upstream expression grammars ([Input files](input-files.md#algebraic-expressions)) |
| external work computed during the force assembly (`getEnergy("external work new")`) | available (`energy("external work new")`, `external_work_new()`) |
| 2-D thermal stress, thermal plane-stress materials (issue 74) | available in the `elastic` law (`alpha`, `delta_T`, `Plane_Stress`; [Materials](materials.md)) |
| periodic boundary conditions: consistency of arrays, periodic tests (static, lumped mass, shear) | mesh periodicity (`akantu::mesh::periodic`, MESH-028/029), the distribution of periodic meshes (PART-013) and the periodic node synchronizer (MPI-019) are available. Upstream's periodic model tests are TEST rows |
| material-selector bug fixes | available (`ElementDataSelector`, [Materials](materials.md#assigning-materials-to-elements)) |
| `Mesh::eraseElements` fix | element removal available (`akantu::mesh::removal`) |
| PETSc: SNES and TAO non-linear solvers, PETSc parsables, DOF manager rework (ghosted vectors, AO) | available behind the feature `petsc` of `akantu-solver` (off by default), SNES, TAO, the PETSc options and the application ordering of the PETSc DOF-manager matrices; the solid and phase-field models select the PETSc DOF manager (`dof_manager_type = petsc`, `init_dof_manager`); the DOF manager keeps local-equation vectors instead of ghosted PETSc vectors (UD-1021) ([The PETSc back-end](petsc.md)) |
| node-to-node contact with rate-and-state friction, steady-state example and tests | available: feature `ntn-contact` ([Split-node contact](models/ntn-contact.md)); the upstream tests are ported (`akantu-ntn-contact` tests), the example `steady_state` is a program of the port ([Examples](examples.md)) |
| contact friction in the coupler | available: feature `couplers` ([Model couplers](models/couplers.md)) |
| friction examples | available: `bloc_friction` and the Hertz contact examples are programs of the port ([Examples](examples.md)) |
| phase field: anisotropic split, AT1, penalisation, thermal stress, mass degradation, TAO irreversibility | available: `volumetric_deviatoric` split, `linear` (AT1), `penalization`, the thermal strain and `degrade_mass` of the `phasefield` material ([Phase-field fracture](models/phase-field.md)). TAO irreversibility: the bounded `petsc_tao` solve on the PETSc DOF manager (opt-in feature `petsc`, [Phase-field fracture](models/phase-field.md#bounded-tao-solve)) |
| Python: generic `set`/`get` on parameters, materials as dictionaries in `initFull`, `registerCustomMaterials`, `hasStiffnessMatrixChanged`, phase-field bindings, mesh keep-alive, no global variables | the Python package binds them with upstream's names (parity rows of area PY, for example PY-094 for `registerCustomMaterials`; [Python interface](models/python.md)); the Rust side of `registerCustomMaterials` is available ([Writing new constitutive laws](new-constitutive-laws.md)) |
| initialisation without `argc`/`argv` | available: `akantu::runtime::initialize` and its building blocks ([Command-line options](command-line.md)) |
| `FEEngine::getMesh` | not applicable: the port's FE engine never stores a mesh, every operation receives it |
| examples run as tests (2026-03), multi-material example | available for every compiled example: they are programs of the port (`crates/akantu/examples`, each compared with upstream's output in the tests; [Examples](examples.md), EX rows) |
