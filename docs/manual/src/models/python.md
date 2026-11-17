# Python interface

<!-- BEGIN GENERATED: availability-status python -->
**Available behind the Cargo feature `python` of `akantu-python`**, off by default (crate `akantu-python`).
<!-- END GENERATED: availability-status python -->

Upstream: the `akantu` Python package (pybind11 module `py11_akantu`;
`getting_started.rst`, *Using the python interface*; parity rows PY-*).
The port's package has the same name, the same extension name
`py11_akantu` and upstream's pure-Python `akantu/__init__.py`; the
extension is the crate `akantu-python` (PyO3) built with its feature
`python` by maturin. The default build of the workspace needs no Python.

## Building the package

The package is not published on PyPI (the name `akantu` on PyPI is
upstream's own package); it is built from source. It needs Python 3.9 or
later (the extension uses the stable ABI, `abi3`), NumPy and SciPy, as
upstream's package does. From the root of the repository:

```sh
python3 -m venv .venv
.venv/bin/pip install maturin numpy scipy pytest greenlet matplotlib
cd python && ../.venv/bin/maturin develop --release   # installs `akantu` into .venv
../.venv/bin/python -m pytest -q tests                 # the package's tests
```

`maturin build --release` writes a wheel instead. Scripts written for
upstream's module run with `import akantu` as before; what differs is
listed below.

## A first script

As upstream's, the module is imported as `aka` by convention. Where a
C++ program calls `initialize("material.dat", argc, argv)`, a script
reads its input file with `aka.parseInput` (the module's `initialize`
raises a `RuntimeError` that says so). The rest follows the C++ API
with upstream's names. The static solve of upstream's example
`plate-hole/plate.py`, abridged (one rank, no MPI), reads:

```python
import akantu as aka
import numpy as np

aka.parseInput("material.dat")

spatial_dimension = 2
mesh = aka.Mesh(spatial_dimension)
mesh.read("plate.msh")

model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._static)

model.applyBC(aka.FixedValue(0.0, aka._x), "XBlocked")
model.applyBC(aka.FixedValue(0.0, aka._y), "YBlocked")
model.applyBC(aka.FromTraction(np.array([0.0, 1.0])), "Traction")

solver = model.getNonLinearSolver()
solver.set("max_iterations", 2)
solver.set("threshold", 1e-10)
solver.set("convergence_type", aka.SolveConvergenceCriteria.residual)

model.setBaseName("plate")
model.addDumpFieldVector("displacement")
model.addDumpField("stress")
model.solveStep()
model.dump()
```

`model.getDisplacement()` then returns the displacement as a NumPy
array ([Arrays are NumPy views](#arrays-are-numpy-views)). The complete
script is `python/examples/solid_mechanics_model/plate-hole/plate.py`;
it is one of the examples compared bit for bit with upstream's module
([Upstream's Python examples](#upstreams-python-examples)).

## What the package binds

Classes, functions and enums keep upstream's names (`Mesh`,
`SolidMechanicsModel`, `_triangle_3`, `_not_ghost`, …), argument names and
class hierarchy (`issubclass` answers as upstream). This release binds:

* the enums, the parser (`parseInput`, `getStaticParser`, parameters and
  sections), meshes with their groups, mesh data, `MeshAccessor`,
  `MeshUtils` and element type maps (also `initialize(fe_engine, …)`);
* `FEEngine` with every upstream method (interpolation, gradients,
  integration, `computeBtD`, `computeBtDB`, `computeNtb`, `computeNtbN`,
  `assembleFieldLumped`, `assembleFieldMatrix`, the element-type-map
  overloads);
* the solver layer: `DOFManager`, `SparseMatrix`/`SparseMatrixAIJ`,
  `SolverVector`, the sparse and non-linear solvers, `TimeStepSolver` and
  the integration schemes, `TermsToAssemble`, `ModelSolver.solveStep` with
  a Python `SolverCallback` (which replaces the model's callback) or an
  `InterceptSolverCallback` (which wraps it and may call the model, the
  matrices and the DOF manager of the running solve);
* `SolidMechanicsModel` with its materials, Python subclasses of
  `Material` (internals, parameters, `computeStress`,
  `computeTangentModuli`, …, as upstream's `custom-material.py` and
  `bi-material.py`), `MaterialFactory`, `registerCustomMaterials`, the
  constitutive-law selectors (also as Python subclasses);
* the dumpers of meshes and element groups (`registerDumperParaview`,
  `addDumpFieldExternal`, `dump`) and the group dumpers of the models;
* the cohesive model (`SolidMechanicsModelCohesive`, its inserter, the
  cohesive laws and selectors, `FragmentManager`), the heat-transfer model
  (`DiffusionModel`, `HeatTransferModel`), the structural model, the
  phase-field model, the contact model with its surface selectors and
  `GeometryUtils`, and the couplers `CouplerSolidContact`,
  `CouplerSolidCohesiveContact` and `CouplerSolidPhaseField`.

The status of every binding is in the PY rows of
[`docs/FEATURE_PARITY.md`](../../../FEATURE_PARITY.md). Upstream's own
Python tests run unchanged against it: `python/tests/test_pybind.py`
(upstream `test_pybind.py`), the linear elastic and heat-transfer patch
tests (`python/tests/patch_tests/`) and `python/tests/test_parser.py`;
upstream's examples are compared with the upstream module value by value
(`python/oracle/`, Python sections of [`docs/TESTING.md`](../../../TESTING.md)).

A Python callback (a boundary-condition functor, a material method, a
selector, a solver callback) may raise: the exception reaches the Python
call that started the computation with its type, and the model is left as
after a failed step. Calls that would need the model while it is held by
the running computation, outside what that computation lends to the
callback, raise `akantu.Exception` instead of waiting.

Where upstream's bindings have a defect, the port follows the intent and
the difference is registered (`docs/UPSTREAM_DEFECTS.md`, UD-830…837): for
example, `TimeStepSolver.getIntegrationScheme` returns the scheme (upstream
raises), a field functor of `assembleFieldLumped` fills the matrix it
receives (upstream hands it a copy and assembles zeros), and the external
fields of a dumper are read from the NumPy array at each dump.

### Upstream-compatibility switches

The contact model, the phase-field model and the three couplers have a
method of the port, `setUpstreamCompat(upstream=True)`, absent upstream:
`True` reproduces the reference where the port corrects it (for example
the contact force assembly and normal orientation, UD-002 and UD-371, or
the phase-field driving energy of the volumetric–deviatoric split,
UD-387), `False` restores the corrected defaults; `getUpstreamCompat()`
reads it back (`True` when every switch is on). The switches cover:

| Class | Upstream defects reproduced with `setUpstreamCompat(True)` |
|---|---|
| `ContactMechanicsModel` | UD-002, UD-008, UD-363, UD-364, UD-365, UD-368, UD-369, UD-371, UD-372 |
| `CouplerSolidContact` | UD-374, and its contact model's |
| `CouplerSolidCohesiveContact` | UD-374, UD-772, and its contact model's |
| `PhaseFieldModel` | UD-377, UD-378, UD-379, UD-380, UD-385, UD-387, UD-388 |
| `CouplerSolidPhaseField` | UD-382, UD-619, UD-620, and its phase-field model's |

The same lists are in the methods' docstrings. The comparison of
upstream's `compression.py` with the upstream module is bitwise with the
switches on.

### Callbacks and execution contexts

What a running computation lends to a Python callback (the solve's DOF
manager and solver-callback interface, the model's FE engines, the
material's own state) is reachable only from the code that runs inside
that callback, including generators, coroutines and event loops driven
from inside it. A call made from anywhere else while the callback runs
(another thread, another greenlet (also one started inside the callback,
or the main greenlet while the callback's greenlet is suspended), a
generator or coroutine resumed outside the callback) raises
`akantu.Exception`; upstream's bindings let such calls reach the running
computation (UD-085). `getCurrentPosition()` inside a callback returns the
current position updated to the current displacement. Outside callbacks
it is updated as upstream does it (when the displacement's release
changed); inside a callback of a running call it is always `X + u` of the
current values, so it is never older than upstream's and differs from it
only after a Python write into the displacement view that upstream's
release check does not see (UD-1001).

### Python cohesive laws, model dumpers and writable arrays

A Python subclass of `MaterialCohesive` is a cohesive law of the model
(decision D-078): registered with
`MaterialFactory.getInstance().registerAllocator` and named in the input
file, it overrides `initMaterial`, `computeTraction`,
`computeTangentTraction` and `checkInsertion`, registers its own
parameters and internals, and reads and writes the law's openings,
tractions and internals (`getOpening`, `getTraction`, `getInternalReal`)
as NumPy views while the model calls it. Inside its `checkInsertion`,
`getInsertionFacets(type)` of the inserter is a writable view of the
insertion flags; outside the insertion check it is a read-only copy
(UD-984; a view kept past the check is detached, writes into it are
lost). The Python laws check the insertion after the built-in laws
(UD-985). A `checkCohesiveStress()` (or `initFull`) called from inside a
`checkInsertion` is refused with `akantu.Exception` and changes nothing:
the running check inserts what its laws flagged when they return
(port-only; upstream's nested call inserts it itself; UD-985). An
exception raised by the law reaches the caller unchanged.

`setBaseName`, `setDirectory`, `addDumpField…` and `dump` of
`ContactMechanicsModel` and of the three couplers write the dumps
upstream writes: a coupler's calls act on the series upstream's mesh
default dumper belongs to (the contact model's for `CouplerSolidContact`,
the cohesive solid model's for `CouplerSolidCohesiveContact`, the solid
model's, named `phase_field_model`, for `CouplerSolidPhaseField`), and
each field is read from the model that provides it (UD-982). The part
calls upstream sends to that series reach it too: `dump()` of a
`CouplerSolidContact`'s contact model, `setTimeStep`, `setBaseName`,
`setDirectory`, `addDumpField…` and `dump` of its solid model, and
`dump()` of a `CouplerSolidCohesiveContact`'s cohesive solid model. The
phase-field model of a `CouplerSolidPhaseField` keeps its own series,
named `phase_field_model_part` (upstream: one series with the coupler's;
UD-982). Setting a surface selector creates the mesh group
`contact_surface` as upstream's selectors do. The
structural model's external and internal forces and element materials and
the nodal fields of `ContactMechanicsModel` (gaps, normals, nodal areas,
internal, external, normal and tangential forces) are writable NumPy views (D-078), and
`reassignConstitutiveLaw()` of the diffusion and phase-field models moves
elements between laws as upstream does (D-078, UD-983). Every model and
coupler object has upstream's `Parsable` methods (`get`, `set`,
`registerParamReal`, …).

### Law subclasses, the live residual and the stand-alone detector

* Elastic subclasses. A Python subclass of `MaterialLinearElastic1D`,
  `MaterialLinearElastic2D` or `MaterialLinearElastic3D` returned by a
  `MaterialFactory` allocator *is* the elastic law with the subclass's
  overrides, as upstream's `PyMaterial<MaterialElastic<dim>>`: the methods
  it does not define (`computeStress`, `computeTangentModuli`, the
  energies, the wave speeds, `hasStiffnessMatrixChanged`) are the elastic
  law's, and `super().initMaterial()`, `super().getPushWaveSpeed(el)`,
  `super().getShearWaveSpeed(el)`, `super().hasStiffnessMatrixChanged()`
  inside an override run the elastic implementation. Its parameters are
  the elastic law's (`E`, `nu`, `Plane_Stress`, the derived `lambda`, `mu`,
  …) followed by those it registers. The class's dimension must be the
  model's. Upstream binds no `computeStress` method to call from Python,
  and the port binds none either. The elastic law's parameters are
  readable once the factory has built the law; inside `__init__` only the
  parameters the subclass registers exist (upstream also shows the elastic
  ones there, with their defaults).
* Phase-field subclasses. `PhaseFieldFactory.registerAllocator(name,
  func)` registers a Python allocator `func(dim, energy_split, model, id)`
  returning a `PhaseField` subclass object; a `phasefield <name> [ … ]`
  section of the input file then makes it a law of the phase-field model.
  The model calls its `computeDrivingForce(el_type, ghost_type)` (after the
  damage and its gradient are interpolated; the subclass writes
  `driving_force`, `driving_energy`, `damage_energy` and
  `damage_energy_density` through `getInternalReal(name)(type)`, zero-copy
  views of the law's storage, and reads `getDamage(type)`,
  `getStrain(type)`, `getInternalReal("grad_d")(type)`, `phi`) and its
  `computeDissipatedEnergy(el_type)` for `getEnergy("dissipated")`; an
  `initPhaseField` override replaces the base initialisation (upstream
  binds no base method to call from Python). The `PhaseField` base's
  parameters (`l0`, `gc`, `E`, `nu`, `Plane_Stress`,
  `irreversibility_type`) come first.
* Diffusion-law subclasses. `HeatDiffusion1D/2D/3D(model, id)` and
  their Python subclasses are constructible (`super().__init__(model,
  id)`), with the law's parameters and the internals they register. As
  upstream, nothing adds such an object to a model (upstream binds no
  allocator of the diffusion factory), so its overrides are never called;
  `DiffusionLaw` has no constructor. A `setReal` before the conductivity
  is set is refused (UD-285: upstream reads the empty conductivity matrix
  and crashes).
* `SolverVector.getValues()` is a zero-copy, writable view of the DOF
  manager's residual, as upstream's: an array kept across later calls
  follows every assembly (`assembleResidual`, `assembleToResidual`,
  `zeroResidual`, the solves' residuals) and writes reach the residual;
  inside a solver callback the view is taken through the running solve.
  A renumbering of the equations (a new DOF family, a cohesive insertion,
  a node removal) follows the retained-view rule of UD-085: a growth
  within the residual's capacity keeps an earlier view attached (it shows
  the old prefix of the live residual), a growth beyond it or any shrink
  retires it, and it then keeps the values it had (stale but safe;
  upstream's view would dangle). Take a new view after a renumbering.
* The phase-field model's `getInternalForce()` is a zero-copy writable
  view too (D-078), like the structural and contact arrays.
* `ContactDetector(mesh, id="contact_detector")` builds a stand-alone
  detector on the mesh's node coordinates with the static parser's first
  `contact_detector` section (without one: `akantu.Exception`, UD-1171);
  `setSurfaceSelector` writes the mesh group `contact_surface` while no
  model uses the mesh. The form `ContactDetector(mesh, positions, id)` is
  refused (UD-1170: upstream frees the positions twice and aborts).
* `convertGmshToAkantuMesh(gmsh)` converts any gmsh model built with
  supported element types, and fills each physical group from the
  elements of its entities. Upstream's function only converts models with
  one element type whose physical-group tags are also entity tags
  (UD-1172, UD-1173, corrected: category B);
  `convertGmshToAkantuMesh(gmsh, upstream_compat=True)` runs upstream's
  code unchanged.
* `ParameterRegistry.get(name)` of a readable but not writable parameter
  (an elastic law's `lambda`) raises `akantu.Exception("Parameter <name>
  is not accessible")`, as upstream's (its generic `get` uses the mutable
  accessor); `getReal` / `getInt` / … return the value. For a parameter
  that is not a `Real` the generic `get` returns its value with its type
  (`Plane_Stress` → `False`); upstream's raises a type error for every
  such parameter (UD-1174, corrected).
* The overridable methods of a Python law subclass (`initMaterial`,
  `getPushWaveSpeed`, `getShearWaveSpeed`, `hasStiffnessMatrixChanged`,
  and `hasMatrixChanged("K")` through the latter) follow pybind11's
  rule: `super().getPushWaveSpeed(el)` *inside the class's own
  `getPushWaveSpeed`* is the base class's computation; any other call
  (`super().getPushWaveSpeed(el)` from `computeStress`,
  `aka.Material.getPushWaveSpeed(self, el)`, a top-level
  `mat.hasStiffnessMatrixChanged()`) runs the class's override if it
  defines one. `hasMatrixChanged("M")` is the law's mass-matrix answer
  (`False` but for the phase-field law with `degrade_mass`). The override
  is looked up on the class only: a function assigned to the instance
  (`mat.getPushWaveSpeed = f`) is not used by the library's calls.
  Upstream's pybind11 lookup (`getattr(self, name)`) also finds it there,
  subject to pybind11's per-type cache of types without overrides, so an
  instance-level override can run upstream and does not in the port
  (`docs/UPSTREAM_DEFECTS.md` UD-1290, a documented difference).
* A Python `__init__` that registers a parameter the base law already has
  (`registerParamReal("E", …)` on a `MaterialLinearElastic2D` subclass,
  `"sigma_c"` on a `MaterialCohesive` one) raises upstream's
  `Parameter named E already registered.` when the law is built (the
  base law is built only after `__init__` returns).

### Group method of Dirichlet functors

A Python `DirichletFunctor` is called once per node of the group with
`__call__(node, flags, primal, coord)`, as upstream; `flags` and `primal`
are views of the node's rows. The port adds an optional method, absent
upstream: a subclass that defines `callNodes(nodes, flags, primal,
coords)` is called once per group instead, and `callNodes` is used instead
of `__call__` when both exist. It receives copies of the group's rows:
`nodes` (int32, one entry per node), `flags` (bool) and `primal` (float64),
one row per node and one column per degree of freedom, and `coords`
(float64, read-only, one row per node). The rows of `flags` and `primal`
are written back to the model after it returns, in node order (a node
listed twice keeps its last row); nothing is written if it raises, and the
exception reaches `applyBC` with its type. It is not called for a group
without nodes, as `__call__` is not.

```python
class Ramp(aka.DirichletFunctor):
    def __init__(self, slope):
        super().__init__()
        self.slope = slope

    def callNodes(self, nodes, flags, primal, coords):
        primal[:, 0] = self.slope * coords[:, 0]
        flags[:, 0] = True

model.applyBC(Ramp(1e-3), "left")
```

On 40,401 nodes one `applyBC` with `callNodes` takes about 1 ms against
about 23 ms with `__call__` (`docs/benchmarks/perf-campaign/io-python/`).

### The rest of the Python interface

A user-provided DOF manager in a model constructor raises
`RuntimeError`, as upstream's binding does (D-078).

### The MPI build of the module (mesh level)

The default wheel is sequential, like upstream's published one:
`has_mpi()` is `False` and `Mesh.distribute()` marks the mesh distributed
over its single rank. The module can also be built with MPI (the
`mpi` feature of the `akantu-python` crate: MPI and Scotch,
`python/oracle/mpi/build-module.sh`), like upstream's CMake build with
`AKANTU_PARALLEL` (D-101):

* `has_mpi()` is `True`; `import akantu` imports `mpi4py` first, as
  upstream's `__init__.py` does, so mpi4py initialises MPI and finalises
  it at interpreter exit. If mpi4py is not importable, or imported with
  `mpi4py.rc.initialize = False`, the module initialises MPI itself
  (thread level `funneled`: akantu's MPI work must then come from the
  main thread; a call from another Python thread raises
  `akantu.Exception`) and finalises it once the last object holding a
  communicator is freed, at the latest at process exit after a normal
  end. MPI is never finalised twice, and akantu makes no MPI call after
  someone else finalised it.
  An uncaught Python exception on one rank while the other ranks are
  inside a collective call (a `distribute()`, for example) leaves the job
  waiting until the launcher's time limit, with this module as with
  upstream's (measured in both import orders). Run distributed scripts
  with `mpirun -n N python -m mpi4py script.py`, which aborts the whole
  job on an uncaught exception.
* `aka.Mesh(dim)` lives on the world communicator.
  `mesh.distribute()` is collective: rank 0, which holds the mesh it read
  (`if prank == 0: mesh.read(...)`, as upstream's `plate.py`; another
  rank's mesh must be empty), partitions it with Scotch seeded with the
  global seed (`AKA_SEED`, `--aka_seed`, the input file's `seed` or the
  time; set `SCOTCH_PTHREAD_NUMBER=1` for reproducible partitions) and
  sends every rank its local mesh: local and ghost elements, the nodes
  they use, element and node groups. Views of the mesh arrays taken
  before the distribution keep the old values; fetch them again.
  `isDistributed()`, `getNodes()`, `getConnectivities()(type,
  ghost_type)`, the groups and the bounds then describe the local mesh;
  `mesh.getGlobalNodesIds()` and `mesh.getNbGlobalNodes()` (akantu-rs
  extensions) give the global node numbering.
* The distribution is compared with the oracle's MPI-enabled module at
  1, 2 and 4 ranks, rank by rank and bit for bit
  (`python/oracle/mpi/run.sh`).
* Not covered: models on several ranks from Python (a
  `SolidMechanicsModel` on a distributed mesh, solves, dumpers). Upstream
  never runs its Python module on several ranks (its `plate.py` example
  is registered as a one-process run), its default sparse solver refuses
  several ranks, and no reference run exists to compare with, so the
  port neither tests nor supports it (D-101 (2)); use the Rust API for
  parallel simulations.

## Upstream's Python examples

Upstream's Python examples (`examples/python/**`) are copied unchanged
into `python/examples/` and run with the package; the tests of
`python/tests/examples` compare every run with the same script run by the
upstream module (oracle fixtures `fixtures/python/examples`, x86_64;
the comparison rules in [`docs/NUMERICAL_VALIDATION.md`](../../../NUMERICAL_VALIDATION.md),
"Python interface", and the commands and the Python environment of the
oracle runs in [`docs/TESTING.md`](../../../TESTING.md), "Environment
variables of the tests" and "Running the tiers"). Equal bit for bit (the
port's default sparse solver, Eigen, against upstream's run pinned to
Eigen, upstream's matrix and empty arrays and its 15-digit Matrix Market
files mapped as documented: UD-110, UD-114, UD-061) are
`stiffness_matrix.py`, `dynamics.py`, `plate-hole/plate.py`,
`custom-material.py`, `bi-material.py`, `solver_callback.py`,
`eigen_modes.py` (with the mesh upstream's Gmsh makes from the script's
own `.geo`), `cohesive/plate.py`, `fragmentation.py`,
`multi-material/plate.py`, `phasefield-static.py`, `phasefield-dynamic.py`,
the three structural scripts and, with `setUpstreamCompat(True)` on the
coupler, `compression.py` and `bloc_friction.py`.

`cohesive/custom_material.py` (a Python cohesive law) raises
`AttributeError` at line 191 with upstream's module as with the port's:
its `computeTraction` calls `getNormals`, which upstream's
`MaterialCohesive` does not bind (UD-980). What it computed before (the
static solve, the dumps and, probed where it raises, the cohesive elements
the Python insertion check made the model insert, the facet stresses and
the displacement) is equal bit for bit. `detection-explicit.py` is equal
to its end (the mesh group `contact_surface` it reads is the one the
surface selector creates).

The example `python_user_defined_bc` of the facade crate (feature
`python`; upstream's C++ program embedding Python) runs a Python
boundary-condition object from a Rust program: `py11_akantu::embed`
starts the interpreter and passes the node's rows to Python as writable
NumPy views of the model's arrays.

## Arrays are NumPy views

Every array the package returns for a mesh or a model
(`mesh.getNodes()`, `model.getDisplacement()`, …) is a NumPy view of the
port's storage, without copy (decision D-047): writes from Python reach
the model, and a view kept by Python sees the values of later calls
without being fetched again. A view of an array that the port has
reallocated (for example after the mesh grew) is stale but safe: it
keeps the values it had and writes to it no longer reach the port. How
this interacts with the guards of the Rust accessors is described in
[Reading and writing model arrays](../solid-mechanics.md#reading-and-writing-model-arrays).
Tests: `python/tests/test_mesh_model_views.py`
(`test_model_views_are_writable_and_reach_the_model`,
`test_a_held_view_sees_the_values_after_each_call`,
`test_mesh_resize_makes_old_views_stale_but_safe`) and
`python/tests/test_views_runtime.py`.

Topology views are read-only once a model uses the mesh (UD-532).
While no model uses a mesh, all its views are writable, as upstream. When
a model is created on it, the connectivity and group-list views fetched
before become stale (they keep their values; writes no longer reach the
mesh) and the ones fetched later are read-only (NumPy `WRITEABLE` unset):
a write raises instead of changing the topology under the model's DOF
numbering, FE precomputations and law filters, which upstream does
silently. Topology changes then go through the mesh's event protocol
(insertions, removals). The node coordinates (`getNodes()`), the model's
nodal arrays and the law internals stay writable views. Tests:
`python/tests/test_zero_copy_views.py`
(`test_topology_views_go_read_only_and_earlier_ones_stale_with_a_model`,
`test_p5_node_write_after_model_lands_and_nothing_breaks`) and
`python/tests/test_cohesive.py::test_int008_views_across_insertion_and_after_the_parents_deletion`.

## External tutorials (DOC-021)

<!-- BEGIN GENERATED: availability-status tutorials -->
**Not ported:** upstream's external tutorials (Jupyter notebooks of `akantu/tutorials` on GitLab, run on Renku; the manual gives upstream's pointer, D-085).
<!-- END GENERATED: availability-status tutorials -->

Upstream ships no tutorials in its repository. Its documentation page
*Tutorials* (`doc/dev-doc/tutorials.rst`) points to Jupyter notebooks kept
in a separate GitLab project,
[`akantu/tutorials`](https://gitlab.com/akantu/tutorials), which can be run
online on [Renku](https://renkulab.io/projects/guillaume.anciaux/akantu-tutorials/sessions/new?autostart=1)
without installing Akantu (the Renku sessions run upstream's package, not
the port's). The port gives the same pointer and nothing more (decision
D-085): the notebooks are external content, written for
upstream's package; the port does not copy, adapt or run them, so this
manual makes no statement about any particular notebook.

What a reader can expect from the port's package is what the PY rows of
the parity matrix say about the bindings a notebook calls (generated from
[`docs/FEATURE_PARITY.md`](../../../FEATURE_PARITY.md); a status change
without regenerating this table fails
`manual_generated_sections_are_current`):

<!-- BEGIN GENERATED: parity-area PY -->
| Status | PY rows |
|---|---:|
| Verified | 85 |
| Exception (approved) | 2 |

The rows that are not *Verified*:

| Row | Feature | Status |
|---|---|---|
| `PY-049` | `dumper_module` (`Field`, `ElementalFieldUInt`) | Exception (approved) |
| `PY-068` | `register_constitutive_law_handler` (dead) | Exception (approved) |
<!-- END GENERATED: parity-area PY -->
