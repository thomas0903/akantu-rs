# Inventory R4 — Python bindings, examples, test suite, CI, documentation

- **Reference**: upstream Akantu @ `35a851780d0b9471d258ed53a003ae98aaa95ea0`
  (`v5.0.7-post1-554-g35a851780`). All paths below are relative to the upstream
  repository root unless prefixed with `akantu-rs:`.
- **Taken**: 2026-11-09.
- **Method**: every `python/` binding file, every `CMakeLists.txt` under `test/`,
  `examples/`, `extra_packages/*/test`, the CMake test/example macros, the test
  driver, all CI files and the documentation were read in full; test sources were
  scanned with a script (gtest macros, assertions, numeric constants, input files,
  failure paths, MPI usage) and the ones whose pass/fail logic was not obvious
  were read. No code was built or run for this inventory (the oracle build is a separate
  task); statements about *run-time* behaviour that could only be confirmed by
  executing are flagged "(unverified)".
- **ID prefixes**: `PY-` Python interface, `EX-` examples, `TEST-` tests,
  `GOLD-` golden/reference data, `MDATA-` meshes, `CI-` CI/tooling,
  `DOC-` documentation/hidden features. (`GOLD-`/`MDATA-` are sub-catalogues of
  section C requested by the brief.)

## 0. Key findings (read this first)

1. **The upstream test driver never compares outputs against `.verified`
   reference files nor runs post-processing scripts.**
   `cmake/akantu_test_driver.sh:147` contains `if $directory; then exit 0; fi`;
   `$directory` is never set, so the condition expands to an empty command
   (exit status 0) and the script exits before the `-s` post-processing and the
   `-r` reference `diff` (lines 151–159). Confirmed with
   `bash -c 'if $directory; then echo taken; fi'` → `taken`. Consequence: for
   every executable test, upstream pass/fail is **only the process exit code**
   (plus gtest assertions for gtest binaries). The 24 `.verified` files are
   therefore *stale golden data whose agreement with the current code is
   unknown* (see §C.4). The Rust port must not assume they currently match.
2. **Large parts of the C++ test tree are not registered at all** (dead
   directories or commented-out registrations): `test_cohesive_{intrinsic,
   extrinsic,intrinsic_impl,1d_element,extrinsic_implicit}`, the whole
   `test_parallel_cohesive/` tree, `test_contact_mechanics_model/{test_detector,
   test_coupler,test_resolution}` (and `test_selector` registers gtest sources
   without ever calling `register_gtest_test`), `test_segment_nodetype`,
   `test_cohesive_linear_friction`, the PETSc matrix tests, grid synchronizer
   tests, several fe_engine sources. Some of these do not even compile
   (`test_detection_fixture.hh:193-197` uses `_element_type_segement_2`,
   `_element_type_quadrange_4`, `_element_type_tetrahedron_6`;
   `test_selection_fixture.hh:67` uses `gtest_list_t<std::tuple<1, 2, 3>>`).
   They are catalogued with status `NOT REGISTERED` so their *intent* can still
   be ported (§C.3).
3. **Six heat-transfer patch tests are silently inactive**: they declare
   `PACKAGE heat_transfer`, but that package no longer exists (renamed
   `diffusion`, `packages/diffusion.cmake:23`); `package_is_activated()` on an
   undeclared package returns FALSE (`CMakePackagesSystemPrivateFunctions.cmake:341`).
   Affects `patch_test_linear_heat_transfer_{explicit,static,implicit}.cc` and
   the three `test_patch_linear_heat_transfer_*.py`.
4. **Upstream CI never exercises** CGAL (`test_geometry`), `embedded`
   (`test_embedded_interface`, `examples/c++/embedded_model`), `extra_materials`,
   `IGFEM` (22 tests), nor `AKANTU_BUILD_UNSTABLE_TESTS` (5 tests). The IGFEM
   and parts of extra-materials tests use APIs that may no longer exist (open
   question Q3).
5. **Python binding quirks that the Rust module must reproduce or consciously
   fix**: arrays are zero-copy, writable NumPy views into model memory;
   `Matrix<Real>` (Eigen, column-major) returned by reference is an
   F-ordered view; constitutive-law base classes have unusable Python names
   (`"ConstitutiveLawakantu::SolidMechanicsModel"`); `StructuralMechanicsModel.
   getMaterial()` returns a *copy* (so `structural_mechanics_softening.py`'s
   softening is a no-op); several genuine binding bugs (`NodeGroup.remove`
   calls `add`, `Element(...)` keyword names wrong, `setRandom` ignores the
   distribution) — §A.9.
6. **Expensive upstream tests/examples**: `structural_mechanics.py` performs
   10⁶ implicit steps, `structural_mechanics_softening.py` 6·10⁵,
   `bloc_friction` (C++ and Python) 3·10⁴ explicit contact steps,
   `compression.py` 2·10⁴, `fragmentation.py` 5·10³ cohesive steps — all
   registered as ctest tests when `AKANTU_TEST_EXAMPLES=ON` (Release CI jobs).
7. Counts (details in §H): **PY 84** rows, **EX 50**, **TEST 174** rows
   (149 rows for the 132 `register_test` registrations + 16 gtest binaries,
   25 rows for unregistered sources/helpers), **GOLD 15**, **MESH 6** groups
   (104 committed `.msh`, 134 `.geo`), **CI 25**, **DOC 24**; the Appendix maps
   **1039 files** (0 unmapped). Of the 149 registered-test rows: 106 active in
   upstream CI, 32 inactive (package not enabled), 6 dead (missing package),
   5 unstable.

---

## A. Python interface

### A.0 Architecture of the binding

The Python package is `akantu` (directory `python/akantu/`), a thin pure-Python
layer (`python/akantu/__init__.py`, 221 lines) over a single pybind11 extension
module `akantu.py11_akantu` built from `python/py_*.cc` (`python/CMakeLists.txt`).
All names of `py11_akantu` are re-exported into `akantu` at import
(`__init__.py:15-18`). Source files are compiled **conditionally on CMake
packages** (`python/CMakeLists.txt:35-98`): solid mechanics + material,
cohesive + fragment manager, diffusion, contact + couplers, phase field,
structural. `py_akantu.cc:80-131` calls the `register_*` functions in a fixed
order which matters for enum `export_values()` collisions (§A.9).

### A.1 Packaging, build and module-level machinery

| ID | Name | Original files | Behavior / exposed API | Required packages / options | Parallel aspects | Porting notes |
|---|---|---|---|---|---|---|
| PY-001 | pip/scikit-build packaging | `setup.py`, `pyproject.toml`, `setup.cfg`, `MANIFEST.in`, `requirements.txt`, `dev-requirements.txt`, `cmake/semver.py` | `skbuild.setup(name="akantu")`, `install_requires=["numpy","scipy"]`, packages from `python/`; build reqs `setuptools>=42,<71`, `pybind11~=2.13.6`, `scikit-build>=0.7`, `cmake>=3.5.3`, `ninja`, `packaging<22`. `setup.cfg [cmake_config]` forces `COHESIVE_ELEMENT, IMPLICIT (solver=Eigen), STRUCTURAL_MECHANICS, DIFFUSION, DAMAGE_NON_LOCAL, PHASE_FIELD, PYTHON_INTERFACE, CONTACT_MECHANICS = TRUE`, `PARALLEL=FALSE`, `EXAMPLES/TESTS=FALSE`. Version from `semver.get_version()` → `-DAKANTU_VERSION`. `CI_AKANTU_INSTALL_PREFIX` env switches to linking a pre-installed libakantu (`AKANTU_BYPASS_AKANTU_TARGET`). | python_interface | Wheels are **sequential** (`AKANTU_PARALLEL=FALSE`, Eigen sparse solver only) | Rust equivalent: `maturin`/`pyproject` with `pyo3`; the published wheel feature set (no MPI, Eigen-only direct solver) is the minimum Python feature set users expect. `ruff` line-length 79 (`pyproject.toml`). |
| PY-002 | Extension build | `python/CMakeLists.txt` | `pybind11_add_module(py11_akantu ...)`, output dir `akantu/`, installs into `lib/pythonX.Y/site-packages/akantu` (or `python/akantu` under skbuild); copies `python/akantu/*.py`. Clang adds `-fsized-deallocation`. | python_interface + per-file packages (see A.0) | — | Module name `akantu.py11_akantu` is imported by tests indirectly only through `akantu`; keep `akantu` as the public import name. |
| PY-003 | Package `__init__` re-export & init | `python/akantu/__init__.py:1-35,217-221` | Re-exports every attribute of `py11_akantu`; if `has_mpi()` tries `from mpi4py import MPI` (so MPI is initialised by mpi4py first); `initialize()` raises `RuntimeError("No need to call initialize, use parseInput ...")`; `finalize()` emits `DeprecationWarning`; aliases `FromStress = FromHigherDim`, `FromTraction = FromSameDim`; calls `py11_akantu.__initialize()` at import; `__version__ = getVersion()`. | python_interface | mpi4py import ordering (MPI_Init done by mpi4py; akantu's communicator then attaches; PETSc told not to finalize MPI — git log 2024-10-05) | Import-time initialisation reads `sys.argv` (PY-004) — Rust module must do the same for `--aka_*` options (DOC-004). |
| PY-004 | Core module functions & exceptions | `python/py_akantu.cc:136-179`, `py_aka_common.cc:59-72` | `py11_akantu.Exception` (← `akantu::debug::Exception`), `NLSNotConvergedException` (← `debug::NLSNotConvergedException`, registered before the generic one so it is not shadowed); translator prints a backtrace if `debugger.printBacktrace()`; `has_mpi()`, `getVersion()`, `__initialize()` (converts `sys.argv` to `argc/argv` and calls `akantu::initialize(argc, argv)`). | core | `has_mpi()` reflects `AKANTU_USE_MPI` | Tests use `pytest.raises(aka.Exception)` (`test_parser.py:88`). Exception class hierarchy: both derive from Python `Exception` (not from each other). |
| PY-005 | `Array<T>` ↔ NumPy type caster | `python/py_aka_array.hh:44-226` | **C++→Python** (`cast`, l.211): returns a NumPy `ndarray` of shape `(size, nb_component)`, C-contiguous, dtype = `T`, **sharing the C++ buffer (no copy)** for policies `reference`, `automatic`, `automatic_reference` (base=None → no lifetime link) and `reference_internal` (base=parent → keeps parent alive); `copy` policy makes an independent array. Always 2-D, even for 1-component arrays. Writable unless `writeable=false` (never used). **Python→C++** (`load`, l.165): accepts 1-D or 2-D arrays; if already a C-contiguous array of the exact dtype → wraps its buffer in an `ArrayProxy<T>` (zero-copy, C++ writes are visible in Python); otherwise (with `convert`) makes a converted copy kept alive for the call (`forcecast`), so C++ writes to it are **lost**. A 1-D input of length n becomes `ArrayProxy(n, shape(1))` → note `ref.shape(1)` on a 1-D array (UB-ish, effectively 1 or garbage; unverified). `ArrayProxy` cannot be resized (`AKANTU_EXCEPTION("cannot resize a temporary array")`). | core | None (each rank sees its local array incl. ghost nodes) | Defines the core numerical contract: all `get*()` accessors (`getDisplacement`, `getNodes`, `getConnectivity`, `getStress`, internal fields...) return **live writable views**; examples mutate them in place (`displacement[:,0] = ...`, `velocity *= 0.9`, `displacement.reshape(n*2)[mask] = v`, `blocked_dofs[nodes,:] = True`). Rust: `numpy`/`pyo3` `PyArray2::borrow_from_array`-style views with the model as base object; must keep the model alive (pybind11 does *not* for `reference` policy — dangling views possible upstream). Index dtypes: `Idx`/`Int` = `int64`?, `UInt` = `uint32`? per `aka_config` (`AKANTU_INTEGER_SIZE`) — see Q6. `bool` arrays (`getBlockedDOFs`) are `numpy.bool_`. |
| PY-006 | `Vector<T>` / `Matrix<T>` / `VectorProxy` conversions | `py_aka_array.hh:24` (includes `pybind11/eigen.h`); `test/test_python_interface/test_common.cc` | Since v5 `Vector<T>`=`Eigen::Matrix<T,Dynamic,1>`, `Matrix<T>`=`Eigen::Matrix<T,Dynamic,Dynamic>` (column-major), `VectorProxy`/`MatrixProxy`=`Eigen::Map` (CHANGELOG v5.0.0). pybind11's Eigen casters: by-value returns copy; `reference`/`reference_internal` returns a NumPy view (F-ordered for matrices; shape `(n,)` for vectors, `(r,c)` for matrices) — verified by `test_pybind.py::test_{vector,matrix}_nocopy`. `Eigen::Map` arguments passed to Python overrides (BC functors: `flags`, `primal`, `coord`, `dual`, `normals`) are **writable views** into C++ storage (`const` ones read-only). Python→C++ for `const Vector&`/`Matrix&` parameters: converted copy (any sequence). | core | — | BC functors write `primal[axis] = v; flags[axis] = True` in place — Rust must pass mutable views (or proxy objects supporting `__setitem__`). Matrix order: Python users index `m[i, j]` = row i, col j regardless of storage. |
| PY-007 | Version / misc | `python/akantu/__init__.py:221` | `akantu.__version__` (from `getVersion()`) | core | — | Keep string semantics (`semver`). |

### A.2 Enumerations

All enums use `py::enum_` with `.export_values()` (values also exported at
module scope) unless noted. "Class enums" (`AKANTU_CLASS_ENUM`) expose value
names **without** the leading underscore (e.g. `SolveConvergenceCriteria.residual`),
plain C enums **with** it (e.g. `_static`).

| ID | Name | Original files | Values | Required packages | Parallel | Porting notes |
|---|---|---|---|---|---|---|
| PY-010 | `SpatialDirection` | `py_aka_common.cc:76` | `_x, _y, _z` | core | — | Used as `int(aka._x)` and as `BC::Axis` argument (`FixedValue(0., aka._x)`); must be int-convertible. |
| PY-011 | `AnalysisMethod` | `py_aka_common.cc:82-90`; C++ `aka_common.hh:159-167` | `_static(0), _implicit_dynamic(1), _explicit_lumped_mass(2), _explicit_lumped_capacity(2, alias), _explicit_consistent_mass(3), _explicit_contact(4), _implicit_contact(5)` | core | — | Used both as `aka._static` and `aka.AnalysisMethod._implicit_dynamic`. Duplicate value 2 (alias). |
| PY-012 | `ModelType` | `py_aka_common.cc:92`; `aka_common.hh:129-140` | `model, solid_mechanics_model, solid_mechanics_model_cohesive, heat_transfer_model, structural_mechanics_model, embedded_model, contact_mechanics_model, coupler_solid_contact, coupler_solid_cohesive_contact, phase_field_model, coupler_solid_phasefield` | core | — | `export_values` puts `model` etc. at module scope → collides with `ParserType.model` (ParserType wins, registered last). |
| PY-013 | `NonLinearSolverType` | `py_aka_common.cc:93-94`; `aka_common.hh:174-182` | `linear, newton_raphson, newton_raphson_modified, lumped, newton_raphson_contact, petsc_snes, petsc_tao, auto` | core | — | `auto` collides with `SparseSolverType.auto` at module scope. |
| PY-014 | `TimeStepSolverType` | `py_aka_common.cc:95-96`; `aka_common.hh:229-233` | `static, dynamic, dynamic_lumped, not_defined` | core | — | Default arg of `ModelSolverOptions`. |
| PY-015 | `IntegrationSchemeType` | `py_aka_common.cc:97-98`; `aka_common.hh:251-261` | `pseudo_time, forward_euler, trapezoidal_rule_1, backward_euler, central_difference, fox_goodwin, trapezoidal_rule_2, linear_acceleration, newmark_beta, generalized_trapezoidal` | core | — | Used by `Model.setIntegrationScheme` and `ModelSolverOptions`. |
| PY-016 | `SolveConvergenceCriteria` | `py_aka_common.cc:99-100`; `aka_common.hh:288-291` | `residual, solution, residual_mass_wgh` | core | — | Ubiquitous in examples: `solver.set("convergence_type", aka.SolveConvergenceCriteria.residual)`. |
| PY-017 | `SparseSolverType` | `py_aka_common.cc:101-102`; `aka_common.hh:207-211` | `mumps, eigen, petsc, auto` | core | — | `ModelSolverOptions(sparse_solver_type=...)`. |
| PY-018 | `CohesiveMethod` | `py_aka_common.cc:104-107` | `_intrinsic, _extrinsic` | core (meaningful with cohesive_element) | — | Registered even without cohesive package. |
| PY-019 | `GhostType` | `py_aka_common.cc:109-113` | `_not_ghost, _ghost, _casper` | core | ghost elements = MPI halo | Default argument of most accessors. |
| PY-020 | `MeshIOType` | `py_aka_common.cc:115-121` | `_miot_auto, _miot_gmsh, _miot_gmsh_struct, _miot_diana, _miot_abaqus` | core | — | `Mesh.read(filename, mesh_io_type)`. |
| PY-021 | `MatrixType` | `py_aka_common.cc:123-126` | `_unsymmetric, _symmetric` | core | — | `AkantuSparseMatrix` mirrors the lower triangle when `_symmetric`; `solver_callback.py` checks `model.getMatrixType("K") == aka._unsymmetric`. |
| PY-022 | `RandomDistributionType` (sub-module) | `py_aka_common.cc:128-141` | sub-module `RandomDistributionType` containing enum `RandomDistributionTypes` with `_uniform, _exponential, _gamma, _weibull, _extreme_value, _normal, _lognormal, _chi_squared, _cauchy, _fisher_f, _student_t` | core | — | Unusual nesting: `aka.RandomDistributionType.RandomDistributionTypes._weibull` (and exported into the sub-module). Used by `ParameterRegistry.setRandom`. |
| PY-023 | `ElementType` | `py_aka_common.cc:143-144`; `aka_element_classes_info.hh:37-95` | `_not_defined`, regular `_point_1, _segment_2, _segment_3, _triangle_3, _triangle_6, _quadrangle_4, _quadrangle_8, _tetrahedron_4, _tetrahedron_10, _pentahedron_6, _pentahedron_15, _hexahedron_8, _hexahedron_20`; if cohesive: `_cohesive_1d_2, _cohesive_2d_4, _cohesive_2d_6, _cohesive_3d_12, _cohesive_3d_16, _cohesive_3d_6, _cohesive_3d_8`; if structural: `_bernoulli_beam_2, _bernoulli_beam_3, _discrete_kirchhoff_triangle_18`; if IGFEM: `_igfem_segment_3, _igfem_triangle_4, _igfem_triangle_5` | core (+cohesive/structural/IGFEM) | — | **Integer values depend on enabled packages** (cohesive types shift structural ones). Python code uses names only; Rust must keep names, and should not promise numeric stability. |
| PY-024 | `ElementKind` | `py_aka_common.cc:145-146` | `_ek_regular, _ek_cohesive, _ek_structural, _ek_not_defined` | core | — | Default `_ek_regular` in many signatures. |
| PY-025 | `ParserType` | `py_aka_common.cc:148`; `parser.hh:35-60` | `cohesive_inserter, contact, embedded_interface, friction, global, heat, integration_scheme, material, phasefield, constitutive_law, mesh, model, model_solver, neighborhood, neighborhoods, non_linear_solver, non_local, rules, solver, time_step_solver, user, weight_function, contact_detector, contact_resolution, not_defined` | core | — | `aka.ParserType.material` in `test_parser.py`. Exported names `global`, `model`, ... at module scope (`aka.global` is a syntax error but `getattr` works). |
| PY-026 | `DebugLevel` + `debug` sub-module | `py_aka_error.cc:37-80` | enum `dblError, dblException, dblCritical, dblMajor, dblWarning, dblInfo, dblTrace, dblAccessory, dblDebug, dblDump, dblTest`; `debug.setDebugLevel`, `debug.getDebugLevel`, `debug.printBacktrace(bool)`; deprecated module-level `setDebugLevel/getDebugLevel/printBacktrace` (emit `DeprecationWarning`) | core | debug level is per process | `akantu.debug.setDebugLevel(akantu.dblError)` used by patch-test fixture. |
| PY-027 | `ParameterAccessType` | `py_parser.cc:45-52` | `_pat_internal, _pat_writable, _pat_readable, _pat_modifiable, _pat_parsable, _pat_parsmod` — `py::arithmetic()` so `aka._pat_readable \| aka._pat_parsable` works (result is an `int`) | core | — | `registerParamReal(..., int type, ...)` takes an `int` bitmask. |
| PY-028 | `IntegrationScheme.SolutionType` | `py_integration_scheme.cc:40-50` | nested enum: `_not_defined, _displacement, _temperature, _damage, _velocity, _temperature_rate, _acceleration` (exported into the class scope) | core | — | `ModelSolverOptions(solution_type={...})`. |

### A.3 Module-level functions

| ID | Name | Original files | Behavior | Required packages | Parallel | Porting notes |
|---|---|---|---|---|---|---|
| PY-030 | `getElementTypes()` | `py_aka_common.cc:156-166` | Returns `dict[str, ElementType]` of the **regular** element types only (`_point_1` … `_hexahedron_20`). | core | — | Python patch-test fixture iterates it to parametrise tests (skips `_point_1`). |
| PY-031 | `parseInput(file)` / `getStaticParser()` | `py_parser.cc:230-240` | `parseInput` → `getStaticParser().parse(file)` (accumulates into the global parser); `getStaticParser()` returns the global `Parser` by reference. | core | each rank parses | Called repeatedly in tests (`test_multiple_init`); must not reset/duplicate state incorrectly (Q7). |

### A.4 Classes — parser, solvers, I/O, groups, mesh, FE

| ID | Class(es) | Original files | Methods / properties (Python names, overloads) | Required packages | Parallel | Porting notes |
|---|---|---|---|---|---|---|
| PY-040 | `ParameterRegistry` | `py_parser.cc:57-157` | `registerParamReal(name, type:int, description)`; `registerParamReal(default:float, name, type, description)` — **allocates a heap `Real` stored in a global map (leaked; `map_params`, l.42)**; `set(name, value: float\|int\|bool\|str\|Matrix\|Vector)` (std::variant → `std::any`); `get(name) → float\|int\|bool\|str\|ndarray` (raises `aka.Exception` if not readable — used by `test_parser.py:88`); `getReal/getBool/getString/getInt(name)`; `getMatrix(name)` (copy); `setReal/setBool/setString/setInt(name, v)`; `setRandom(name, value, distribution, a, b)` (**bug**: builds a random parameter but sets the plain `value`, l.151-156). `py::multiple_inheritance`. | core | — | Python-defined materials register parameters with this API (`custom-material.py`). Rust: parameters declared from Python need storage owned by the law object. |
| PY-041 | `Parsable` | `py_parser.cc:158-160` | `Parsable(ParserType, id)`; `parseSection(ParserSection)` | core | — | `test_parser.py::TestParsable`. |
| PY-042 | `ParserParameter` | `py_parser.cc:162-200` | `getName()`, `getValue()` (raw string, e.g. `"2*pi + max(2, general)"`), `asInt()`, `asReal()`, `asBool()`, `asVector()` (copy), `asMatrix()` (`reference` policy on a temporary — **dangling view risk**, l.194-200) | core | — | Algebraic expression evaluation (`pi`, `max`, references to other parameters) must match (`test_parser.py:58-63`). |
| PY-043 | `ParserSection`, `Parser` | `py_parser.cc:201-228` | `ParserSection(name, ParserType)`, `ParserSection(name, type, option, parent)`; `getParameter(name)` (ref-internal); `getSubSections(type, id=0)` → the id-th sub-section (raises "Sub section not found"); `Parser()` (derives `ParserSection`). | core | — | — |
| PY-044 | `SparseMatrix`, `SparseMatrixAIJ` | `py_solver.cc:50-84` | `getMatrixType()`, `size()`, `zero()`, `saveProfile(file)`, `saveMatrix(file)` (Matrix Market, used by `solver_callback.py`), `add(i,j)` (profile), `add(i,j,value)`, `add(A, alpha=1.)`, `isFinite()`, `getRelease()` (bound twice), `__call__(i,j)`; AIJ: `getIRN()`, `getJCN()`, `getA()` (1-based index arrays as `(nnz,1)` views) | core / implicit | distributed matrices on several ranks | `AkantuSparseMatrix` (PY-090) relies on 1-based IRN/JCN and on lower-triangle storage for symmetric matrices. |
| PY-045 | `SolverVector` | `py_solver.cc:85-95` | `getValues()` → view of the underlying `Array<Real>` (ref-internal, "not copied"), `isDistributed()` | core | distributed (PETSc) | — |
| PY-046 | `SparseSolver`, `SparseSolverEigen`, `SparseSolverMumps`, `SparseSolverPETSc`, `SparseMatrixPETSc` | `py_solver.cc:96-106` | Opaque classes (derive `Parsable`, so `set("pc_type","cholesky")` works through `ParameterRegistry.set`). Mumps/PETSc classes only exist when compiled with those libs. | implicit (+mumps/petsc) | MUMPS/PETSc parallel | **Compatibility trap**: `patch_test_linear_fixture.py:62` evaluates `isinstance(s, akantu.SparseSolverPETSc)` → `AttributeError` on builds without PETSc. Rust module should always define the class name (possibly unconstructible). |
| PY-047 | `TermToAssemble`, `TermsToAssemble` | `py_solver.cc:107-128` | `TermToAssemble(i,j)`, `+= float`, props `i`, `j`; `TermsToAssemble(dof_id_m, dof_id_n)`, `getDOFIdM()`, `getDOFIdN()`, `__call__(i, j, val=0.)` (sets the term) | core | — | Used with `DOFManager.assemblePreassembledMatrix` for periodic constraints (`solver_callback.py`). |
| PY-048 | `Dumpable` | `py_dumpable.cc:40-106` | `registerDumperParaview(dumper_name, file_name, is_default=False)`, `addDumpMeshToDumper(dumper_name, mesh, dimension, ghost_type=_not_ghost, element_kind=_ek_regular)`, `addDumpMesh(mesh, dimension, ...)`, `addDumpField(field_id)`, `addDumpFieldToDumper(dumper_name, field_id)`, `addDumpFieldExternal(field_id, Field)`, `addDumpFieldExternal(field_id, ndarray)` (**copies the ArrayProxy into a global vector `tmp_array` that keeps pointing at the NumPy buffer**, l.36,62-68), `addDumpFieldExternalToDumper(dumper_name, field_id, Field)`, `dump()`, `dump(step)`, `dump(time, step)`, `dump(dumper_name)`, `dump(dumper_name, step)`, `dump(dumper_name, time, step)` | dumpers / IOHelper | each rank writes its own piece (`.pvtu`) | Output format = ParaView XML (`.vtu/.pvtu/.pvd`) via IOHelper — numerically a side channel; Rust needs an equivalent dumper to run examples unchanged (files are not checked by tests). |
| PY-049 | `dumper_module` (`Field`, `ElementalFieldUInt`) | `py_dumpable.cc:108-122` | A **second `PYBIND11_MODULE(dumper_module)` inside the same shared object**; its init function `PyInit_dumper_module` can never be imported as `akantu.py11_akantu.dumper` → dead code; `dumpers::Field` is therefore *not* registered and `addDumpFieldExternal(field_id, Field)` overload is unreachable. | dumpers | — | Do not port (document as dead); expose only the ndarray overload. |
| PY-050 | `NodeGroup` | `py_group_manager.cc:42-67` | `getNodes()` (view `(n,1)` of node ids), `__len__`, `__iter__`, `__contains__(node)`, `getName()`, `clear()`, `empty()`, `append(other)`, `add(node, check_for_duplicate=True)`, `remove` (**bug: bound to `NodeGroup::add`**, l.67) | core | groups are local to each rank | Examples iterate `getNodes()` and `getNodes().flatten()`. |
| PY-051 | `ElementGroup` | `py_group_manager.cc:70-104` | derives `Dumpable`; `getNodeGroup()` (bound twice), `getName()`, `getElements()` (`ElementTypeMapArray<Idx>`), `__len__`, `clear()`, `empty()`, `append()`, `optimize()`, `add(element, add_nodes=False, check_for_duplicate=True)`, `fillFromNodeGroup()`, `addDimension()` | core | local | `convertGmshToAkantuMesh` uses `add(Element, True, False)` then `optimize()`. |
| PY-052 | `GroupManager`, `ClusteringFilter` | `py_group_manager.cc:108-185` | `getElementGroup(name)`, `iterateElementGroups()` (list), `iterateNodeGroups()` (list), `createNodeGroup(...)`, `createElementGroup(id, spatial_dimension, replace_group=False)`, `createGroupsFromMeshDataUInt(dataset_name)`, `createElementGroupFromNodeGroup(name, node_group, dimension=_all_dimensions)`, `getNodeGroup(name)`, `nodeGroups()`, `elementGroups()`, `createBoundaryGroupFromGeometry()`, `createClusters(element_dimension, prefix, filter)`; `ClusteringFilter()` | core | `createClusters` is parallel-aware (C++) | Patch-test fixture: `MeshUtils.buildFacets(mesh); mesh.createBoundaryGroupFromGeometry()` then iterates groups. |
| PY-053 | `Mesh`, `MeshData`, `PeriodicSlaves` | `py_mesh.cc:153-270` | `Mesh(spatial_dimension, id="mesh")`; `read(filename, mesh_io_type=_miot_auto)`; `getNodes()` (view), `getNbNodes()`, `getConnectivity(type)` (view), `getConnectivities()`, `addConnectivityType(type, ghost_type=_not_ghost)`, `distribute()`, `isDistributed()`, `fillNodesToElements(dimension=_all_dimensions)`, `getAssociatedElements(node, list)` (appends into a Python list), `makePeriodic(direction)`, `getNbElement(*, spatial_dimension=_all_dimensions, ghost_type=_not_ghost, kind=_ek_not_defined)`, `getNbElement(*, element_type, ghost_type=_not_ghost)` (keyword-only), static `getSpatialDimension(type)`, `getDataReal(name, type, ghost_type)` (view), `hasDataReal(...)`, `isPeriodic()`, `getPeriodicMaster(node)`, `getPeriodicSlaves(node)` (iterable), `isPeriodicSlave/Master(node)`, `getMeshFacets()`, `getUpperBounds()`, `getLowerBounds()`, `initMeshFacets(id="mesh_facets")`; inherits `GroupManager`, `Dumpable`, `MeshData` (`getElementalDataUInt(name)`, `getElementalDataReal(name)`) | core | `distribute()` = Scotch partition + ghost layer (rank 0 reads, `plate.py` pattern `if prank==0: mesh.read(); mesh.distribute()`) | `std::shared_ptr<Mesh>` holder. `getNbElement(element_type=...)` keyword-only API used by `test_pybind.py::test_mesh_interface`. |
| PY-054 | `MeshUtils`, `MeshAccessor` | `py_mesh.cc:271-289` | `MeshUtils.buildFacets(mesh)` (static); `MeshAccessor(mesh)`, `resizeConnectivity(new_size, type, ghost_type=_not_ghost)`, `resizeNodes(new_size)`, `makeReady()` | core | — | Programmatic mesh creation (structural examples, `convertGmshToAkantuMesh`). |
| PY-055 | `ElementTypeMapArrayReal/UInt/Int/bool` | `py_mesh.cc:41-150` | `ElementTypeMapArray*(id="by_element_type_array", parent_id="no_parent")`; `__call__(type, ghost_type=_not_ghost)` → view (keep_alive); `elementTypes(dim=_all_dimensions, ghost_type=_not_ghost, kind=_ek_regular)` → list; `initialize(mesh, *, ghost_type=_casper, nb_component=1, spatial_dimension=-2 (=mesh dim), element_kind=_ek_not_defined, with_nb_element=False, with_nb_nodes_per_element=False, default_value=T{}, do_not_default=False)`; same with `fe_engine` (registered twice, once keyword-only); `getID()` | core | ghost/not-ghost arrays | Note: `UInt` variant is a different dtype from `Int`/`Idx` variant — internal fields of type `Idx` map to `ElementTypeMapArrayInt`? (Q6). |
| PY-056 | `Element`, `ElementNull` | `py_fe_engine.cc:41-51` | `Element(type, id, ghost_type=_not_ghost)` — **keyword names bug**: declared `py::arg("type"), py::arg("ghost_type"), py::arg("ghost_type")=...` (l.45) so the 2nd parameter is named `ghost_type`; only positional use is safe; `__lt__`, `__repr__`; module attribute `ElementNull`. No attribute accessors for `type/element/ghost_type`. | core | ghost elements | Rust: accept positional; decide whether to fix keyword names (document deviation). |
| PY-057 | `FEEngine`, `IntegrationPoint` | `py_fe_engine.cc:53-275` | `getNbIntegrationPoints(type, ghost_type)`, `initShapeFunctions(ghost_type)`, `integrate(f: ETMA, intf: ETMA, filter_elements=None)`, `integrate(f, intf, nb_degree_of_freedom, type, ghost_type, filter_elements=empty)`, `integrate(f, type, ghost_type, filter) → float`, `gradientOnIntegrationPoints(u, nablauq, nb_dof, type, ghost_type, filter)`, `interpolateOnIntegrationPoints(u, uq, nb_dof, type, ghost_type, filter)`, `interpolateOnIntegrationPoints(u, uq: ETMA, filter=None)`, `computeBtD(Ds, BtDs, type, ...)`, `computeBtDB(Ds, BtDBs, order_d, type, ...)`, `computeNtb(bs, Ntbs, type, ...)`, `computeNtbN(bs, NtbNs, type, ...)`, `computeIntegrationPointsCoordinates(coordinates: ETMA, filter=None)`, `assembleFieldLumped(field_funct(Matrix, Element), lumped_matrix, dof_manager, type, ghost_type)`, `assembleFieldMatrix(field_funct, matrix_id, dof_id, dof_manager, type, ghost_type)`, `getElementInradius(element)`, `getNormalsOnIntegrationPoints(type, ghost_type)`, `getShapes(type, ghost_type, id=0)`, `getShapesDerivatives(type, ghost_type, id=0)`, `getMesh()`; `IntegrationPoint` opaque | core | — | Output arrays (`intf`, `nablauq`, ...) must be **pre-allocated C-contiguous float64 arrays of the right shape**, otherwise the converted copy discards results silently (PY-005). Empty-filter sentinel trick (`filter.data()==empty_filter.data()`) was a 5.0.7 bug fix (CHANGELOG). `field_funct` is a Python callable invoked from C++ (callback extension point, A.8). |
| PY-058 | Integration schemes | `py_integration_scheme.cc:38-66` | `IntegrationScheme`, `IntegrationScheme2ndOrder`; `NewmarkBeta(dof_manager, id, alpha, beta)` with properties `alpha`, `beta`; subclasses `CentralDifference`, `TrapezoidalRule2`, `FoxGoodwin`, `LinearAceleration` (sic) — no constructors | core | — | `TimeStepSolver.getIntegrationScheme(dof_id)` returns these (downcast to most-derived registered type). |

### A.5 Classes — DOF manager, solvers, model infrastructure, boundary conditions

| ID | Class(es) | Original files | Methods / properties | Required packages | Parallel | Porting notes |
|---|---|---|---|---|---|---|
| PY-060 | `DOFManager` | `py_dof_manager.cc:151-184` | `getMatrix(id)` (→ `SparseMatrix`, downcast to `SparseMatrixAIJ`), `getNewMatrix(name, matrix_to_copy_id)`, `getResidual()` (`SolverVector`), `getArrayPerDOFs(...)`, `hasMatrix(name)`, `assembleToResidual(dof_id, array_to_assemble, scale_factor=1.)`, `assembleToLumpedMatrix(dof_id, array, lumped_mtx, scale_factor=1.)`, `assemblePreassembledMatrix(matrix_id, terms)`, `zeroResidual()` | core | default vs PETSc DOF managers | Matrix ids used by examples: `"K"`, `"M"`, `"C"`, `"J"`. |
| PY-061 | `NonLinearSolver`, `NonLinearSolverNewtonRaphson`, `NonLinearSolverLumped` | `py_dof_manager.cc:185-207` | derive `Parsable`; `set(id, float)` (special case: `"max_iterations"` cast to `int`), `set(id, SolveConvergenceCriteria)`; NR: `getSparseSolver()` | core / implicit | — | Ubiquitous: `solver.set("max_iterations", 2); solver.set("threshold", 1e-10); solver.set("convergence_type", ...)`. Returned via polymorphic downcast; `hasattr(solver, "getSparseSolver")` used to detect NR. |
| PY-062 | `TimeStepSolver` | `py_dof_manager.cc:208-210` | `getIntegrationScheme(dof_id)` | core | — | — |
| PY-063 | `SolverCallback` (**trampoline** `PySolverCallback`) | `py_dof_manager.cc:44-96,211-227` | `SolverCallback(dof_manager)` (`init_alias`); overridable: `getMatrixType(matrix_id)` (pure), `assembleMatrix(matrix_id)` (pure), `assembleLumpedMatrix(matrix_id)` (pure), `assembleResidual()` (pure), `predictor()`, `corrector()`, `beforeSolveStep()`, `afterSolveStep(converged)`; property `dof_manager` | core | callbacks run on every rank | Extension point (A.8). `ModelSolver` derives from it, so `model.assembleMatrix("K")`, `model.assembleResidual()`, `model.getMatrixType("K")` are available on all models. |
| PY-064 | `InterceptSolverCallback` (**trampoline** `PyInterceptSolverCallback`) | `py_dof_manager.cc:97-140,228-230` | `InterceptSolverCallback(solver_callback)`; all methods overridable, default = forward to the wrapped callback (model) | core | — | `solver_callback.py` subclasses it to add periodic constraints and passes it to `model.solveStep(callback)`. |
| PY-065 | `ModelSolverOptions`, `ModelOptions` | `py_model.cc:117-133` | `ModelSolverOptions(time_step_solver_type=not_defined, non_linear_solver_type=auto, sparse_solver_type=auto, integration_scheme_type={}, solution_type={})`; `ModelOptions(analysis_method=_explicit_lumped_mass, solver_options=default)`; aliases `SolidMechanicsModelOptions`, `ContactMechanicsModelOptions`, `PhaseFieldModelOptions` = `ModelOptions` (module attributes) | core | — | `aka.SolidMechanicsModelOptions(aka._static)` used by `test_pybind.py`. |
| PY-066 | `ModelSolver` | `py_model.cc:134-159` | derives `SolverCallback`, `Parsable`; `getNonLinearSolver(solver_id="")`, `getTimeStepSolver(solver_id="")`, `solveStep(solver_id="")`, `solveStep(callback, solver_id="")` | core | — | Solver ids used: `""` (default), `"static"`, `"explicit_lumped"`, `"dynamic"`, `"implicit"` (names produced by `initNewSolver`). |
| PY-067 | `Model` | `py_model.cc:160-252` | `setBaseName`, `setDirectory`, `getFEEngine(name="")`, `getFEEngineBoundary(name="")` (**bound to `getFEEngine`**, not the boundary engine), `addDumpFieldVector`, `addDumpField`, `setBaseNameToDumper`, `addDumpFieldVectorToDumper`, `addDumpFieldToDumper`, 6× `dump(...)` overloads, `dumpGroup(group_name)`, `setGroupDirectory(directory, group_name)`, `setGroupBaseName(basename, group_name)`, `addDumpGroupField(field_id, group_name)`, `addDumpGroupFieldVector(...)`, `initNewSolver(analysis_method, option=ModelSolverOptions())`, `getNewSolver(id, solver_options)` (returns None), `setIntegrationScheme(id, primal, scheme_type, solution_type=_not_defined)`, `getDOFManager()`, `assembleMatrix(matrix_id)` | core | — | `getFEEngine("FacetsFEEngine")` used by cohesive custom material. |
| PY-068 | `register_constitutive_law_handler` (dead) | `py_model.cc:41-115` | Template defined but never instantiated (the live one is PY-075). | — | — | Do not port. |
| PY-069 | Boundary-condition functors (**trampolines** `PyDirichletFunctor<T>`, `PyNeumannFunctor<T>`) | `py_boundary_conditions.cc:37-120` | `BCFunctor` (opaque); `DirichletFunctor()`, `DirichletFunctor(axis)` — override `__call__(node, flags, primal, coord)` (writable `flags: bool[dim]`, `primal: float[dim]`, read-only `coord`); `NeumannFunctor()` — override `__call__(quad_point, dual, coord, normals)` (pure); predefined: `FixedValue(value, axis)`, `IncrementValue(value, axis)`, `Increment(vector)`, `FromHigherDim(matrix)` (= `FromStress`), `FromSameDim(vector)` (= `FromTraction`), `FreeBoundary()`; predefined ones can also be subclassed and `__call__` overridden (`dynamics.py: class MyFixedValue(aka.FixedValue)`). Applied with `model.applyBC(functor, group_name)`. | core | applied on local nodes/facets of each rank | Extension point (A.8). Missing vs C++: `FlagOnly` (used in C++ example EX-016) is not bound. |

### A.6 Classes — constitutive-law framework, materials, models

| ID | Class(es) | Original files | Methods / properties | Required packages | Parallel | Porting notes |
|---|---|---|---|---|---|---|
| PY-070 | `ConstitutiveLawSelector` (**trampoline**) and selectors | `py_constitutive_law_selector.cc:54-115` | `ConstitutiveLawSelector()` — override `__call__(element) → int`; `setFallback(int)`, `setFallback(selector)`; `DefaultMaterialSelector(ETMA<Idx>)`; `MeshDataMaterialSelectorString(name, model, first_index=1)`; cohesive: `DefaultMaterialCohesiveSelector(model)`, `MeshDataMaterialCohesiveSelector(model)`, `MaterialCohesiveRulesSelector(model, rules: dict[(str,str)→str], mesh_data_id="physical_names")` | core (+solid_mechanics, cohesive_element) | evaluated per local element | Selector classes have `shared_ptr` holders; `multi-material/plate.py` passes a dict keyed by tuples. Extension point (A.8). |
| PY-071 | `InternalFieldReal`, `InternalFieldInt` | `py_constitutive_law.cc:53-109` | derive `ElementTypeMapArray*`; `previous(type, ghost_type=_not_ghost)` (view), `previous()` (field), `__call__(type, ghost_type)` (view), `__repr__` (`<InternalFieldReal (0x…): id [type:ghost - Array<double n, c>]...>`), `getID()`, `initializeHistory()`, `hasHistory()` | core | ghost parts present | Python materials store and index internal fields per element type. |
| PY-072 | `ConstitutiveLawInternalHandler` | `py_constitutive_law.cc:111-145` | `ConstitutiveLawInternalHandler(id, spatial_dimension, parent_id)`; `registerInternalReal(name, nb_component) → InternalFieldReal` (shared_ptr), `registerInternalInt(name, nb_component)`, `getInternalReal(id)`, `getInternalInt(id)`, `getElementFilter()` (ETMA<Idx>), `getSpatialDimension()` | core | — | Internal names are arbitrary UTF-8: heat-transfer internals are named **`"∇u"`** and **`"D∇u"`** (`patch_test_linear_heat_transfer_fixture.py:27,31`). |
| PY-073 | `ConstitutiveLaw<Handler>` (**trampoline** `PyConstitutiveLaw`) — Python names `ConstitutiveLawakantu::SolidMechanicsModel`, `ConstitutiveLawakantu::DiffusionModel`, `ConstitutiveLawakantu::PhaseFieldModel` | `py_constitutive_law.hh:15-96` | ctor `(handler, id, spatial_dimension, element_kind, parent_id)`; overridable `initConstitutiveLaw()`, `getEnergy(energy_id)`, `getEnergy(energy_id, element)`; `setReal/setBool/setString/setInt(name, value)` override `Parsable` setters so `updateInternalParameters()` runs; pack/unpack stubs return 0 (no parallel sync of Python-side data) | core | **Python constitutive laws are not synchronised across ranks** (`getNbData` returns 0) | Class names come from `debug::demangle` (`aka_error.cc:63`) → contain `::`; only reachable with `getattr`. Rust may pick clean names; nothing in tests/examples names them. |
| PY-074 | `ConstitutiveLawsHandler<Law, Model>` — names `ConstitutiveLawsHandlerakantu::Materialakantu::Model`, `...akantu::DiffusionLawakantu::Model`, `...akantu::PhaseFieldakantu::Model` | `py_constitutive_laws_handler.hh:13-103` | ctor `(mesh, model_type, dim, id)`; `registerNewConstitutiveLaw(name, type, opt_param="")`, `reassignConstitutiveLaw()`, `isInternal(field_name, element_kind=_ek_regular)`, `flattenInternalReal(field_name, kind=_ek_regular, ghost_type=_not_ghost)` (ETMA view), `inflateInternalReal(field_name, field, kind, ghost_type)`, `getConstitutiveLaws()`, `getConstitutiveLaw(idx \| name \| element)`, `getConstitutiveLawIndex(name)`, `getNbConstitutiveLaws()`, `getConstitutiveLawByElement()`, `getConstitutiveLawLocalNumbering()` | core | — | `model.getConstitutiveLaw(0)` used on `HeatTransferModel` in the heat patch test. |
| PY-075 | `SolidMechanicsModel` | `py_solid_mechanics_model.cc:55-215` | `SolidMechanicsModel(mesh, spatial_dimension=_all_dimensions, id="solid_mechanics_model", dof_manager=None, model_type=solid_mechanics_model)` (keep_alive mesh); `initFull(option=ModelOptions())`, `initFull(_analysis_method=...)`, `initFull(materials: dict, _analysis_method=...)` (registers each material from `{"name": {"type": t, param: value}}`, int/float/str/bool params, then `initConstitutiveLaws()`); `applyDirichletBC()` (deprecated no-op + warning); `applyBC(DirichletFunctor, group)`, `applyBC(NeumannFunctor, group)`; `setTimeStep(time_step, solver_id="")`; `getEnergy(energy_id)`, `getEnergy(energy_id, group_id)`; `assembleStiffnessMatrix()`, `assembleInternalForces()`, `assembleMass()`, `assembleMassLumped()`, `getStableTimeStep()`; views: `getExternalForce`, `getDisplacement`, `getPreviousDisplacement`, `getCurrentPosition`, `getIncrement`, `getInternalForce`, `getMass`, `getVelocity`, `getAcceleration`, `getBlockedDOFs`, `getMesh`; `getMaterial(id \| name \| element)` (reference), `getNbMaterials()`, `getMaterialIndex(name)`, `setMaterialSelector(selector)`, `getMaterialSelector()`, `getMaterialByElement()`, `reassignMaterial()`, `registerNewMaterial(material_name, material_type, option="")` | solid_mechanics | `getStableTimeStep()` reduced over ranks (C++); energies summed over ranks (C++) | Energy ids: `"potential"`, `"kinetic"`, `"external work"` (also `"external work new"`, `solid_mechanics_model.cc:649-653`), and material/cohesive ones `"dissipated"`, `"reversible"`, `"contact"` (used by `fragmentation.py`). `getMesh()` returns the `Mesh` by reference. |
| PY-076 | `Material` (**trampoline** `PyMaterial<Material>`) | `py_material.cc:47-77,148-193` | `Material(model, id)`; overridable `initMaterial()`, `computeStress(el_type, ghost_type)`, `computeTangentModuli(el_type, tangent_matrix, ghost_type)`, `computePotentialEnergy(el_type)`, `getPushWaveSpeed(element)`, `getShearWaveSpeed(element)`, `hasStiffnessMatrixChanged()`; accessors `getGradU(el_type, ghost_type)`, `getStress(el_type, ghost_type)`, `getPotentialEnergy(el_type)` (view), `getPotentialEnergy(element)`, `getPotentialEnergy()`, `getModel()`, `hasMatrixChanged(id)`; inherits ConstitutiveLaw (PY-073), InternalHandler (PY-072), Parsable (PY-040) | solid_mechanics | see PY-073 | **Central extension point** (A.8). Shapes: `getGradU` → `(nb_quad, dim*dim)`, reshaped by users to `(nq, dim, dim)`; `tangent_matrix` → `(nb_quad, voigt²)` (`(nq, 3, 3)` in 2-D). |
| PY-077 | `MaterialLinearElastic1D/2D/3D` | `py_material.cc:78-85,194-196` | `(model, id)` constructors of `MaterialElastic<d>` with `PyMaterial` trampoline (subclassable) | solid_mechanics | — | — |
| PY-078 | `MaterialCohesive` (**trampoline** `PyMaterialCohesive`) and `MaterialCohesiveLinear2D/3D`, `MaterialCohesiveLinearFriction2D/3D` | `py_material.cc:104-138,200-238` | overridable `checkInsertion(check_only)`, `computeTraction(el_type, ghost_type)`, `computeTangentTraction(el_type, tangent_matrix, ghost_type)` (+ PyMaterial methods; `computeStress/TangentModuli` final no-ops); accessors `getFacetFilter()`, `getFacetFilter(type, ghost_type)`, `getOpening(type, ghost_type)`, `getTraction(type, ghost_type)`. **Not bound: `getNormals`** (used by `examples/python/.../cohesive/custom_material.py:132`, which is therefore broken and not registered as a test) | cohesive_element | cohesive insertion is parallel-aware in C++ | — |
| PY-079 | `MaterialFactory` | `py_material.cc:240-261` | `MaterialFactory.getInstance()`, `registerAllocator(id, func(dim, option, model, id) → Material)` (Python object is kept alive by `inc_ref` + `obj.release()`; C++ `unique_ptr` takes ownership of the C++ part), `getPossibleAllocators()` | solid_mechanics | — | Ownership model must be redesigned in Rust (Python object must outlive the model's use). |
| PY-080 | `CohesiveElementInserter` | `py_solid_mechanics_model_cohesive.cc:49-82` | `setLimit(...)`, `getCheckFacets()` / `(type, ghost_type)`, `getInsertionFacets()` (by element) / `(type, ghost_type)`, `addPhysicalSurface(name)`, `addPhysicalVolume(name)` | cohesive_element | — | — |
| PY-081 | `SolidMechanicsModelCohesiveOptions`, `SolidMechanicsModelCohesive` | `py_solid_mechanics_model_cohesive.cc:83-115` | Options `(analysis_method=_explicit_lumped_mass, solver_options=default, is_extrinsic=False)`; model `(mesh, spatial_dimension, id="solid_mechanics_model")`; `initFull(_analysis_method=_explicit_lumped_mass, _is_extrinsic=False)`; `checkCohesiveStress()`, `getElementInserter()`, `getStressOnFacets(type, ghost_type)`, `getTangents(type, ghost_type)`, `updateAutomaticInsertion()` | cohesive_element | extrinsic insertion synchronises facet stresses (C++) | Odd: `initFull` has `keep_alive<1,2>` on a non-object argument (harmless). |
| PY-082 | `FragmentManager` | `py_fragment_manager.cc:40-58` | `FragmentManager(model, dump_data=True, ID="fragment_manager")`; `buildFragments(damage_limit=1.)`, `computeCenterOfMass()`, `computeVelocity()`, `computeInertiaMoments()`, `computeAllData(damage_limit=1.)`, `computeNbElementsPerFragment()`, `getNbFragment()`, `getMass()`, `getVelocity()`, `getMomentsOfInertia()`, `getPrincipalDirections()`, `getNbElementsPerFragment()` — getters via `def_function` (default policy → **copies**) | cohesive_element | fragments across ranks (C++) | `fragmentation.py` prints fragment counts. |
| PY-083 | `DiffusionLaw`, `HeatDiffusion1D/2D/3D` (**trampoline** `PyDiffusionLaw`) | `py_heat_transfer_model.cc:43-83` | `HeatDiffusion<d>(model, id)`; overridable `computeDiffusivityGradUOnQuadPoints(type, ghost_type)`, `computeDiffusivityOnQuadPoints(type, ghost_type)`, `getStableTimeStep(element_size)` | diffusion | — | Extension point (A.8). |
| PY-084 | `DiffusionModel`, `HeatTransferModel` | `py_heat_transfer_model.cc:86-114` | `DiffusionModel` (no ctor): `initFull(_analysis_method)`, `setTimeStep(time_step, solver_id="")`, `getStableTimeStep()`, `getBlockedDOFs()` (**default return policy → copy**: writes to it are lost; unverified); `HeatTransferModel(mesh, spatial_dimension, id="diffusion_model")`, `getTemperature()` (view) | diffusion | — | Heat patch tests (inactive, finding 3) would exercise `getBlockedDOFs()` writes — likely broken by the copy policy (Q8). Not bound: external heat rate, capacity, conductivity getters. |
| PY-085 | `StructuralMaterial`, `StructuralMechanicsModel` | `py_structural_mechanics_model.cc:53-160` | `StructuralMaterial()` / copy ctor, read-write fields `E, A, I, Iz, Iy, GJ, rho, t, nu`; model `(mesh, spatial_dimension, id="structural_mechanics_model")`; `initFull(_analysis_method)`, `initFull()`; views `getExternalForce, getDisplacement, getInternalForce, getVelocity, getAcceleration, getBlockedDOFs, getMesh`; `setTimeStep`; `getElementMaterial(type, ghost_type)` (view of material index per element); `getMaterialByElement(element)`; `addMaterial(mat, name="") → index`; `getMaterial(index \| name)` (**returns a copy**); `getNbMaterials()`, `getKineticEnergy()`, `getPotentialEnergy()`, `getEnergy(id)`, `getLumpedMass()`, `getMass()`, `assembleLumpedMassMatrix()`, `hasLumpedMass()` | structural_mechanics (needs implicit) | — | Structural examples run 10⁵–10⁶ steps. Copy semantics of `getMaterial` makes `structural_mechanics_softening.py` softening ineffective — keep or fix consciously (Q9). |
| PY-086 | Contact detection: `ContactDetector`, `SurfaceSelector`, `PhysicalSurfaceSelector`, `CohesiveSurfaceSelector`, `AllSurfaceSelector` | `py_contact_mechanics_model.cc:79-104` | `ContactDetector(mesh, id="contact_detector")`, `(mesh, positions, id)`; `setSurfaceSelector(selector)`; selectors `(mesh)` (shared_ptr) | contact_mechanics | — | — |
| PY-087 | `ContactMechanicsModel`, `ContactElement`, `ContactElementsView` | `py_contact_mechanics_model.cc:105-170` | model `(mesh, spatial_dimension, id="contact_mechanics_model", dof_manager=None, model_type)`; `initFull(options)`, `initFull(_analysis_method)`; `search()`, `assembleStiffnessMatrix()`, `assembleInternalForces()`; views `getExternalForce, getNormalForce, getTangentialForce, getInternalForce, getGaps, getNormals, getNodalArea`; `getContactDetector()`; `getContactElements()` → `ContactElementsView` (`__iter__`, `__size__` (sic, not `__len__`), `__contains__`, `__getitem__`); `ContactElement()` with `master`, `slave` r/w, `__repr__` `{master: i, slave: j}` | contact_mechanics | — | `detection-explicit.py` prints normals, gaps and `list(model.getContactElements())`. |
| PY-088 | `GeometryUtils` | `py_contact_mechanics_model.cc:172-222` | static `normal(mesh, positions, element, outward=True)`, `covariantBasis(mesh, positions, element, normal, natural_projection)`, `curvature(mesh, positions, element, natural_coord)`, `contravariantBasis(covariant_basis)`, `realProjection(mesh, positions, slave, element, normal)`, `isBoundaryElement(...)` | contact_mechanics | — | — |
| PY-089 | `CouplerSolidContact`, `CouplerSolidCohesiveContact` | `py_model_couplers.cc:40-121` | common: `(mesh, spatial_dimension, id="coupler_solid_contact", dof_manager=None)`, `applyBC(Dirichlet\|Neumann, group)`, `getContactMechanicsModel()`, `getDisplacement()`, `getVelocity()`, `getAcceleration()`, `getMass()`, `getContactForce()`, `getContactDetector()`, `setTimeStep(time_step, solver_id="")`, `getStableTimeStep()`; Solid: `getSolidMechanicsModel()`, `initFull(_analysis_method=_explicit_lumped_mass)`; Cohesive: `initFull(_analysis_method, _is_extrinsic=False)`, `checkCohesiveStress()`, `getSolidMechanicsModelCohesive()` | contact_mechanics (+model_couplers, cohesive_element) | — | Coupler inherits `Model` → `dump`, `addDumpField*` (fields `contact_force`, `gaps`, `areas`, `normals`, `tangents`, `grad_u`, ...). |
| PY-091 | `PhaseFieldModel` | `py_phase_field_model.cc:55-126` | `(mesh, spatial_dimension, id="phase_field_model", dof_manager=None, model_type)`; `initFull(PhaseFieldModelOptions)` (arg named `_analysis_method`!), `initFull(_analysis_method)`; `applyDirichletBC()` (deprecated), `applyBC(...)` ×2; `setTimeStep`; `getEnergy(energy_id="dissipated")`, `getEnergy(group_id)` (**ambiguous overload: both take one string; the second is unreachable by position**); `assembleStiffnessMatrix()`, `assembleInternalForces()`, `savePreviousState()`; views `getDamage, getInternalForce, getBlockedDOFs`; `getPhaseField(id)`, `getPhaseFieldIndex(name)`, `setPhaseFieldSelector(selector)` | phase_field | — | `phase.getDamage()` is compared across staggered iterations as a live view (`phasefield-static.py`). |
| PY-092 | `PhaseField` (**trampoline** `PyPhaseField`), `PhaseFieldFactory` | `py_phasefield.cc:30-133` | `PhaseField(model, id)`; overridable `initPhaseField()`, `computeDrivingForce(el_type, ghost_type)`, `computeDissipatedEnergy(el_type)`; `getDamage(el_type, ghost_type)` / `getDamage()`, `getStrain(...)` / `getStrain()`, `getEnergy(id)`, `getEnergy(id, element)`; `PhaseFieldFactory.getInstance()`, `registerAllocator(id, func(dim, energy_split, model, id))` | phase_field | see PY-073 | Extension point (A.8). No concrete phase-field subclasses are bound (unlike materials). |
| PY-093 | `CouplerSolidPhaseField`, `PhaseFieldElementFilter` | `py_phase_field_model.cc:128-158` | coupler `(mesh, spatial_dimension, id="coupler_solid_phasefield", model_type)`; `solve(solid_solver_id, phase_solver_id)`, `getSolidMechanicsModel()`, `getPhaseFieldModel()`, `computeStrainOnQuadPoints(ghost_type)`, `computeDamageOnQuadPoints(ghost_type)`; `PhaseFieldElementFilter(model, max_damage)` (a `ClusteringFilter`) | phase_field | — | Staggered scheme driven from Python: `model.solve("static","static")` / `("explicit_lumped","static")`. |

### A.7 Python-side helpers (pure Python in `akantu/__init__.py`)

| ID | Name | Original files | Behavior | Required packages | Parallel | Porting notes |
|---|---|---|---|---|---|---|
| PY-090 | `AkantuSparseMatrix` | `python/akantu/__init__.py:37-75` | Subclass of `scipy.sparse.coo_matrix`; built from a `SparseMatrixAIJ`: rows = `getIRN()[:,0]-1`, cols = `getJCN()[:,0]-1`, data = `getA()[:,0]` (copied); if `getMatrixType()==_symmetric` mirrors off-diagonal entries (lower → full storage). `aka_sparse` attribute keeps the source. | implicit (scipy) | sequential only (local AIJ) | Used by `stiffness_matrix.py`, `eigen_modes.py`, `structural_mechanics_dynamics.py`. Keep as pure Python on top of the Rust accessors. |
| PY-094 | `registerCustomMaterials(dict)` | `__init__.py:78-99` | For each `name → class`, registers allocator `lambda dim, option, model, id_: cls(model, id_)` into `MaterialFactory`. | solid_mechanics | — | Used by `bi-material.py`. |
| PY-095 | `convertGmshToAkantuMesh(gmsh)` | `__init__.py:102-214` | Builds an `akantu.Mesh` from the gmsh Python API (`gmsh.model.mesh`): MSH type table (gmsh ids 1-18 → `_segment_2 … _pentahedron_15`, unsupported → `ValueError`/`IndexError`), copies node coordinates (first `dim` comps), connectivities (gmsh node tags → 0-based), physical groups → `createElementGroup(name, dim, replace_group=True)` + `add(Element, True, False)` + `optimize()`, then `accessor.makeReady()`. | core (+ external `gmsh` python package) | sequential | Not exercised by any test/example in-tree. Gmsh element types 8 (`_segment_3`), 9 (`_triangle_6`), 11 (`_tetrahedron_10`), 15 (`_point_1`), 16 (`_quadrangle_8`), 17 (`_hexahedron_20`), 18 (`_pentahedron_15`). |
| PY-096 | `initialize` / `finalize` stubs, `FromStress` / `FromTraction` aliases | `__init__.py:27-35,217-218` | see PY-003 | core | — | Backward-compatibility layer of v5. |

### A.8 Extension points (subclassing C++ from Python) — explicit list

| Extension point | Trampoline (file:line) | Overridable methods (Python name) | How it is installed | Used by |
|---|---|---|---|---|
| Material | `PyMaterial<T>` `py_material.cc:47-77` | `initMaterial`, `computeStress`, `computeTangentModuli`, `computePotentialEnergy`, `getPushWaveSpeed`, `getShearWaveSpeed`, `hasStiffnessMatrixChanged` (+ `ConstitutiveLaw`: `initConstitutiveLaw`, `getEnergy`) | `MaterialFactory.getInstance().registerAllocator(name, f)` or `akantu.registerCustomMaterials({...})`; then referenced by `material <name> [...]` in the `.dat` or by `initFull({...})` dict | EX-033, EX-034, TEST-058/059 (fixture class `LocalElastic` defined but unused), TEST-143 (C++ material checked against `py_mazars.py` in an embedded interpreter — not a subclass) |
| Concrete elastic materials | same, over `MaterialElastic<d>` | idem | `class X(aka.MaterialLinearElastic2D)` | none in-tree |
| Cohesive material | `PyMaterialCohesive<T>` `py_material.cc:104-138` | `checkInsertion`, `computeTraction`, `computeTangentTraction` (+ material ones) | `registerAllocator` | EX-038 (`custom_material.py`, broken: uses unbound `getNormals`) |
| Diffusion law | `PyDiffusionLaw<T>` `py_heat_transfer_model.cc:43-66` | `computeDiffusivityGradUOnQuadPoints`, `computeDiffusivityOnQuadPoints`, `getStableTimeStep` | no factory binding exposed (Q10) | none |
| Phase field | `PyPhaseField<T>` `py_phasefield.cc:30-54` | `initPhaseField`, `computeDrivingForce`, `computeDissipatedEnergy` | `PhaseFieldFactory.getInstance().registerAllocator(name, f(dim, energy_split, model, id))` | none |
| Generic constitutive law | `PyConstitutiveLaw<H>` `py_constitutive_law.hh:15-49` | `initConstitutiveLaw`, `getEnergy` ×2 | via subclasses above | — |
| Dirichlet BC | `PyDirichletFunctor<T>` `py_boundary_conditions.cc:37-51` | `__call__(node, flags, primal, coord)` | `model.applyBC(obj, "group")` | EX-031 (`dynamics.py`), EX-039 (`fragmentation.py`), TEST-153 (`test_pybind.py`), EX-012 (C++ embeds Python) |
| Neumann BC | `PyNeumannFunctor<T>` `py_boundary_conditions.cc:54-68` | `__call__(quad_point, dual, coord, normals)` (pure) | `model.applyBC(obj, "group")` | TEST-153 |
| Solver callback | `PySolverCallback` `py_dof_manager.cc:44-96` | `getMatrixType`, `assembleMatrix`, `assembleLumpedMatrix`, `assembleResidual` (pure), `predictor`, `corrector`, `beforeSolveStep`, `afterSolveStep` | `model.solveStep(callback, solver_id)` | none directly |
| Intercepting solver callback | `PyInterceptSolverCallback` `py_dof_manager.cc:97-140` | same, defaults forward to wrapped model | `model.solveStep(cb)` | EX-035 (`solver_callback.py`) |
| Material/constitutive-law selector | `PyConstitutiveLawSelector<T>` `py_constitutive_law_selector.cc:54-63` | `__call__(element) → index` | `model.setMaterialSelector(sel)` / `setConstitutiveLawSelector` / `setPhaseFieldSelector` | none (only built-in selectors used) |
| FE field functor | `std::function<void(Matrix&, const Element&)>` (pybind11/functional) `py_fe_engine.cc:231-256` | Python callable `f(matrix, element)` filling the matrix | `FEEngine.assembleFieldLumped/assembleFieldMatrix(f, ...)` | none |
| Embedding Python in C++ | `pybind11::embed` | — | C++ test/example creates `py::scoped_interpreter` and imports a Python module | EX-012, `test_fe_engine_precomputation` (py_engine), `test_damage_materials` (py_mazars) |

**MPI / mpi4py.** `akantu/__init__.py:20-24` imports `mpi4py.MPI` when the build
has MPI; `plate-hole/plate.py` uses `MPI.COMM_WORLD.Get_rank()` to read the mesh
on rank 0 then `mesh.distribute()`. No Python test runs under MPI upstream
(`register_test(... PYTHON)` never sets `PARALLEL`). Wheels are built without
MPI. Python-defined constitutive laws are not communicated (PY-073).

### A.9 Binding defects and semantic traps (must be decided for the Rust module)

| # | Location | Defect / trap | Recommendation |
|---|---|---|---|
| 1 | `py_group_manager.cc:67` | `NodeGroup.remove` bound to `NodeGroup::add` | Fix in Rust (document as deviation). |
| 2 | `py_fe_engine.cc:45` | `Element.__init__` keyword names `(type, ghost_type, ghost_type=...)`; second arg should be `id` | Accept positional; name the arg `id`. |
| 3 | `py_parser.cc:151-156` | `setRandom` ignores distribution (sets plain value) | Fix. |
| 4 | `py_parser.cc:42,61-73` | `registerParamReal` leaks a heap `Real` per call in a global map | Store in the law object. |
| 5 | `py_parser.cc:193-198` | `ParserParameter.asMatrix()` returns a reference to a temporary | Return a copy. |
| 6 | `py_dumpable.cc:36,108-122` | global `tmp_array`; dead second module `dumper_module` | Do not port global; keep NumPy object alive by reference. |
| 7 | `py_model.cc:165` | `getFEEngineBoundary` returns `getFEEngine` | Decide; probably fix. |
| 8 | `py_model.cc:39-109` | dead `register_constitutive_law_handler` | Ignore. |
| 9 | `py_phase_field_model.cc:98-109` | two `getEnergy(str)` overloads; second unreachable | Single `getEnergy(energy_id="dissipated")`. |
| 10 | `py_contact_mechanics_model.cc:113` | `ContactElementsView.__size__` instead of `__len__` | Provide both. |
| 11 | `py_heat_transfer_model.cc:100` | `DiffusionModel.getBlockedDOFs` returned by copy (default policy) | Return view (and note change). |
| 12 | `py_structural_mechanics_model.cc:117-133` | `getMaterial` returns copies | Keep copy semantic for compatibility, or return a live reference and note that `structural_mechanics_softening.py` output changes. |
| 13 | `py_solid_mechanics_model.cc:170,174` / `py_structural_mechanics_model.cc:84,87` | `getInternalForce` bound twice | Harmless. |
| 14 | `py_aka_common.cc` `export_values` | module-scope name collisions (`model`, `auto`, `static`...) | Reproduce "last wins" or avoid exporting class-enum values (Python code only uses `Enum.value` form for class enums). |
| 15 | `py_aka_array.hh:121-130,211-224` | `reference` policy returns views without keeping the owner alive → use-after-free if the model is deleted while a view is held | Rust: always tie views to the owning object. |
| 16 | `py_aka_array.hh:165-205` | Non-contiguous / wrong-dtype input arrays are silently copied, so output arguments are lost | Raise a `TypeError` for output parameters instead (deviation, safer). |
| 17 | `py_material.cc:240-261`, `py_phasefield.cc:110-131` | allocator ownership via `inc_ref` + `release()` (Python object leaked deliberately) | Model must hold a strong reference to the Python object. |
| 18 | `examples/python/.../cohesive/custom_material.py` | calls unbound `MaterialCohesive.getNormals` | Either bind `getNormals` or treat example as broken upstream. |

---

## B. Examples

### B.0 How examples are built and registered

- `examples/CMakeLists.txt` → `add_example(c++ ... PACKAGE core)`,
  `add_example(python ... PACKAGE python_interface)`; nested `add_example(dir
  desc PACKAGE pkg)` calls (`cmake/AkantuExampleMacros.cmake:107-163`) only
  descend into a directory when all listed packages are active and
  `AKANTU_BUILD_ALL_EXAMPLES` (default ON) or `AKANTU_BUILD_EXAMPLES_<DIR>` is set.
- `register_example(name SOURCES ... | SCRIPT file.py [PYTHON] [PARALLEL]
  DEPENDS <mesh targets> FILES_TO_COPY ... [USE_PACKAGES ...]
  [DIRECTORIES_TO_CREATE ...])` (`AkantuExampleMacros.cmake:27-105` and
  `AkantuSimulationMacros.cmake:30-170`) builds the executable (or copies the
  script) and **only if `AKANTU_TEST_EXAMPLES=ON`** registers a ctest named
  `<name>-test` run through `akantu_test_driver.sh` (`-P <python>` for scripts;
  `PARALLEL` examples run with **2 ranks**, `-N "2"`, l.47-54). Pass/fail =
  exit code only. Upstream CI sets `AKANTU_TEST_EXAMPLES=TRUE` only for the
  Release jobs (`.gitlab/ci.d/templates.yaml` `.build_release`).
- Meshes are generated at build time by `add_mesh(target file.geo DIM ORDER
  [OUTPUT name.msh])` (`cmake/Modules/FindGMSH.cmake:46-110`): runs
  `gmsh -<dim> -order <order> [-optimize if gmsh<3] -o <out>.msh <file>.geo`.
  If gmsh is not found, `add_mesh` silently does nothing (but
  `find_package(GMSH REQUIRED)` is called when tests/examples are on).
- Python examples are copied into the build tree and run from there with
  `PYTHONPATH` from `akantu_environement.sh` (`cmake/akantu_environement.sh.in`).

Legend for *Test?*: `yes` = registered as `<name>-test` when
`AKANTU_TEST_EXAMPLES=ON`; `no` = never run by ctest. Run-time scale is an
estimate from step counts and mesh sizes (unverified): S < 10 s, M < 2 min,
L < 30 min, XL ≥ 30 min.

### B.1 C++ examples (`examples/c++/**`)

| ID | Name (ctest name) | Original files | Purpose / physics | Inputs | Features exercised (subsystems) | Expected outputs | Run time | Test? | Required packages | Parallel aspects | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| EX-001 | `contact_explicit_dynamic` (`contact_explicit_dynamic-test`) | `contact_mechanics_model/hertz/contact_explicit_dynamic.cc`, `hertz.geo`, `material.dat`, `README.rst`, `images/hertz.{png,svg}` | Hertzian contact of a half-disc pressed on a block, explicit dynamics, 20 000 steps with prescribed incremental displacement | `hertz.geo` (2D, order 1), `material.dat` (elastic + contact detector/resolution sections) | `CouplerSolidContact`, `ContactMechanicsModel` detection/resolution (penalty), `IncrementValue`/`FixedValue` BCs, explicit lumped mass, `PhysicalSurfaceSelector` | ParaView series `paraview/contact-explicit-dynamic*` | L | yes | contact_mechanics (→ model_couplers, cohesive_element) | sequential | Reference behaviour for the Hertz analytic solution (README); no numeric check upstream. |
| EX-002 | `contact_explicit_static` | `hertz/contact_explicit_static.cc` | Same geometry, **static** coupled solve, 100 load increments | `hertz.geo`, `material.dat` | `CouplerSolidContact` static (Newton + contact), implicit solver | ParaView `contact-explicit-static` | M | yes | contact_mechanics, implicit | sequential | Requires a sparse direct solver. |
| EX-003 | `cohesive_contact_explicit_dynamic` | `hertz/cohesive_contact_explicit_dynamic.cc`, `cohesive-contact.geo`, `material-cohesive.dat` | Cohesive crack + contact on the crack faces (`CouplerSolidCohesiveContact`), explicit, 25 000 steps | `cohesive-contact.geo`, `material-cohesive.dat` | extrinsic cohesive insertion + contact on new crack faces, `MeshDataMaterialCohesiveSelector` with fallback, `CohesiveSurfaceSelector` | ParaView `cohesive-contact-explicit-dynamic` | L | yes | contact_mechanics, cohesive_element | sequential | — |
| EX-004 | `bloc_friction` | `contact_mechanics_model/friction/bloc_friction.cc`, `bloc.geo`, `material.dat`, `README.rst`, `images/bloc_friction.gif` | Elastic block compressed then sheared on a foundation with Coulomb friction; 3 phases × 10 000 explicit steps with velocity damping in contact | `bloc.geo`, `material.dat` (two materials by physical name, friction section) | `CouplerSolidContact` + friction, `MeshDataMaterialSelector`, `IncrementValue` | ParaView `bloc_friction` | XL | yes | contact_mechanics | sequential | Python twin EX-045. |
| EX-005 | `heat_diffusion_dynamics_2d` | `diffusion_model/heat_diffusion_dynamics_2d.cc`, `square.geo`, `material.dat`, `README.rst`, `images/*` | Transient heat diffusion of a hot spot in a square, explicit lumped capacity, 15 000 steps | `square.geo` (2D, **order 2**), `material.dat` (`heat_diffusion` law) | `HeatTransferModel`/`DiffusionModel`, explicit lumped capacity, stable time step | ParaView `heat_diffusion_square2d` | L | yes | diffusion | sequential | — |
| EX-006 | `heat_diffusion_static_2d` | `heat_diffusion_static_2d.cc` | Static heat conduction with imposed temperatures | `square.geo` (order 2), `material.dat` | static diffusion (linear solve) | ParaView `heat_diffusion_static_2d` | S | yes | diffusion, implicit | sequential | — |
| EX-007 | `heat_diffusion_dynamics_3d` | `heat_diffusion_dynamics_3d.cc`, `cube.geo` (`cube_tet.geo` unused) | 3D transient diffusion, 1000 explicit steps | `cube.geo` (3D, order 2) | explicit diffusion in 3D | ParaView `heat_diffusion_cube3d` | M | yes | diffusion | sequential | `cube_tet.geo` is not referenced by CMake. |
| EX-008 | `embedded` | `embedded_model/embedded.cc`, `concrete.geo`, `reinforcement.geo`, `material.dat` | Concrete matrix with embedded 1D reinforcement bars (`EmbeddedInterfaceModel`), static | `concrete.geo` (2D order 2), `reinforcement.geo` (1D order 1), `material.dat` | embedded interface model, CGAL intersections, `FromTraction` | ParaView `concrete` + `reinforcement` dumper | S | yes | embedded (→ CGAL), implicit | sequential | Not built in upstream CI (CGAL/embedded off). No README. |
| EX-009 | `steady_state` | `node_to_node/steady_state.cc`, `ntn_test_ras.geo`, `ras_ss.in` | Node-to-node (traction-at-split-node) frictional contact of a slider on a base with **rate-and-state friction** (revised Dieterich, regularised ageing), periodic in x, steady sliding at prescribed velocity, compares with a static analytical solution | `ntn_test_ras.geo`, `ras_ss.in` (`user parameters`, `friction rate_and_state:...`, two elastic materials) | `NTNContactSolverCallback`, `makePeriodic`, user parser section, friction laws (extra package) | `steady_state` ParaView series, energies text file | L | yes | traction_at_split_node_contact | sequential | Added 2025-04 (git log); twin of `test_ntn_friction` (TEST). |
| EX-010 | `phase_field_notch` | `phase_field_model/phase_field_notch.cc`, `square_notch.geo`, `material_notch.dat`, `README.rst`, `images/phase_field_clusters.png` | Single-edge notched plate in tension, phase-field fracture, **staggered** solid/phase-field static solves, 1000 load steps, clusters of broken elements | `square_notch.geo` (2D order 1), `material_notch.dat` | `CouplerSolidPhaseField`, `PhaseFieldModel`, `IncrementValue`, `createClusters` + `PhaseFieldElementFilter` | ParaView `phase_notch` | L | yes | phase_field, implicit | sequential | — |
| EX-011 | `phase_field_parallel` (PARALLEL) | `phase_field_parallel.cc` | Same as EX-010 on a distributed mesh | idem | `mesh.distribute()`, parallel phase field + solid | ParaView `phase_notch_parallel` | L | yes (2 ranks) | phase_field, parallel | **mpirun -np 2** | Uses phase-field default synchronisation (fixed 2026-02, git log). |
| EX-012 | `python_user_defined_bc` | `solid_mechanics_model/boundary_conditions/python_user_defined_bc/python_user_defined_bc.cc`, `boundary_condition.py`, `fine_mesh.geo`, `material.dat` | C++ program embedding a Python interpreter (`pybind11::embed`); a C++ `DirichletFunctor` delegates to Python `SinBoundary.compute(disp, coord, flags)` | `fine_mesh.geo`, `material.dat`, `boundary_condition.py` | `py::scoped_interpreter`, Eigen-map → NumPy view passing, BC functors | ParaView `plate` | S | yes | python_interface, core | sequential | In a Rust port this becomes "Rust program embedding Python" — optional; the intent (user BC written in Python) is covered by PY-069. |
| EX-013 | `predefined_bc` | `boundary_conditions/predefined_bc/predefined_bc.cc`, `square.geo`, `material.dat`, `../README.rst`, `../images/*` | Shows built-in `FixedValue` Dirichlet BCs; only `initFull` + dump, no solve | `square.geo`, `material.dat` | `applyBC(FixedValue)`, dumper | ParaView `plate` | S | yes | core | sequential | — |
| EX-014 | `user_defined_bc` | `user_defined_bc/user_defined_bc.cc`, `fine_mesh.geo`, `material.dat` | User C++ `DirichletFunctor` (sine displacement) + `FromTraction` | idem | custom functor subclassing | ParaView `plate` | S | yes | core | sequential | — |
| EX-015 | `cohesive_extrinsic` | `solid_mechanics_cohesive_model/cohesive_extrinsic/cohesive_extrinsic.cc`, `triangle.geo`, **`triangle.msh` (committed)**, `material.dat`, `README.rst`, `images/*.gif` | Extrinsic cohesive fracture of a pre-stretched plate under velocity BC, explicit, 1000 steps, energies printed | committed `triangle.msh`, `material.dat` (`elastic` + `cohesive_linear`) | `SolidMechanicsModelCohesive(_is_extrinsic=true)`, `checkCohesiveStress`, energies | ParaView `extrinsic` | M | yes | cohesive_element | sequential | Uses committed mesh (no `add_mesh`). |
| EX-016 | `cohesive_extrinsic_ig_tg` | `cohesive_extrinsic_ig_tg/cohesive_extrinsic_ig_tg.cc`, `triangle.geo`, `material.dat` | Extrinsic cohesive with **intergranular vs transgranular** cohesive properties: `MaterialCohesiveRulesSelector` (pairs of physical groups `btop`/`bbottom` → `tg_cohesive`/`ig_cohesive`) chained with `MeshDataMaterialSelector<std::string>` fallbacks; custom velocity Dirichlet functor writing into `getVelocity()` + `FlagOnly`; 1000 explicit steps | `triangle.geo` → `square.msh` (OUTPUT), `material.dat` | selector chaining (`setFallback`), `BC::Dirichlet::FlagOnly`, extrinsic insertion | ParaView `extrinsic` | M | yes | cohesive_element | sequential | `FlagOnly` not available in Python (PY-069). |
| EX-017 | `cohesive_intrinsic` | `cohesive_intrinsic/cohesive_intrinsic.cc`, `triangle.geo`, **`triangle.msh`**, `material.dat`, `README.rst`, `images/*.png` | Intrinsic cohesive elements inserted along a line at start, explicit, 350 steps | committed `triangle.msh` | intrinsic insertion (`CohesiveElementInserter` with limits), explicit | ParaView `intrinsic` | S | yes | cohesive_element | sequential | — |
| EX-018 | `scalability_test` | `scalability_test/cohesive_extrinsic.cc`, `cube.geo`, **`cube.msh`**, `material-elastic.dat`, `material-cohesive.dat`, `README.rst`, `images/*.svg` | JOSS weak/strong-scaling benchmark: 3D cube in compression + shear, elastic or extrinsic cohesive (selected by input file through `--aka_input_file`), timing per step, energies CSV | committed `cube.msh` (16 kB), `material-*.dat` (`user section`: `compression`, `shear`, `inc_shear`, `max_steps = 5000`, `output_energy`, `output_paraview`, `cohesive_insertion`, `mesh`; `model solid_mechanics_model_cohesive [ material elastic [... finite_deformation = true] ]`) | `mesh.distribute()`, extrinsic cohesive in parallel, `FromHigherDim`, timers | `energies.csv`, stdout timings | L (sequential as ctest) | yes (sequential) | cohesive_element (parallel for real use) | designed for 1…720 ranks (EX-049); ctest runs it serially | Benchmark reference for `docs/BENCHMARKS.md`; JOSS timing CSVs (DOC-019). |
| EX-019 | `explicit_dynamic` | `solid_mechanics_model/explicit/explicit_dynamic.cc`, `bar.geo`, `material.dat`, `README.rst`, `images/*` | 3D bar, initial sinusoidal pulse, explicit central difference, 1000 steps, energy balance written | `bar.geo` (3D order 1) | explicit lumped mass, energies (`potential`, `kinetic`) | `energy.csv`, ParaView `explicit_dynamic` | M | yes | core | sequential | Good candidate for a numeric differential test (energy conservation). |
| EX-020 | `implicit_dynamic` | `implicit/implicit_dynamic.cc`, `beam.geo`, `material_dynamic.dat`, `README.rst`, `images/*` | Simply supported 3D beam under point load, implicit Newmark (trapezoidal), until `max_time`; writes position of a node | `beam.geo` (3D **order 2**) | implicit dynamic, Newmark-β, sparse solver, `mesh.distribute()` (works serial) | `position.csv`, ParaView `dynamic` | M | yes | implicit | distribute call present but registered serial | — |
| EX-021 | `static` | `static/static.cc`, `square.geo`, `material.dat`, `README.rst`, `images/*` | Static plane problem with imposed displacement | `square.geo` (2D order 2) | static Newton, `FixedValue` | ParaView `static` | S | yes | implicit | sequential | — |
| EX-022 | `parallel_2d` (PARALLEL) | `parallel/parallel_2d.cc`, `square_2d.geo`, `material.dat`, `README.rst`, `images/parallel.png` | Explicit wave in a 2D square on a distributed mesh, 10 000 steps | `square_2d.geo` | `mesh.distribute()` (Scotch), ghost synchronisation, parallel dumping | ParaView `multi` (`.pvtu`) | L | yes (2 ranks) | parallel | **mpirun -np 2** | — |
| EX-023 | `new_local_material` | `new_material/new_local_material.cc`, `local_material_damage.{cc,hh}`, `local_material_damage_inline_impl.hh`, `barre_trou.geo`, `material.dat`, `README.rst`, `images/barre_trou.svg` | How to write a new C++ material (local damage) and register it in `MaterialFactory`; plate with hole under traction, explicit, 10 000 steps | `barre_trou.geo` (2D order 2) | material plug-in API (`registerAllocator`), internal fields, `FromStress` | ParaView `local_material` | L | yes | core | sequential | Same material source is reused by `test_local_material` (TEST). Rust: trait-based material plug-in. |
| EX-024 | `material_viscoelastic_maxwell_energies` | `new_material/viscoelastic_maxwell/material_viscoelastic_maxwell_energies.cc`, `material_viscoelastic_maxwell.dat`, `material_viscoelastic_maxwell_mesh.geo` | Generalised Maxwell viscoelastic material under prescribed biaxial strain ramp; prints potential/dissipated energy and work per step; fails (`EXIT_FAILURE`) on Newton non-convergence (`NLSNotConvergedException`) | mesh (2D order 1), `.dat` | `material viscoelastic_maxwell`, static Newton with history, `getEnergy("potential"/"dissipated"/"work")` | text output file, ParaView | M | yes | core (USE_PACKAGES iohelper) | sequential | — |
| EX-025 | `dumper_low_level` | `io/dumper/dumper_low_level.cc`, `locomotive_tools.{cc,hh}` (static lib `locomotive_tools`), `swiss_train.geo`, `io/README.rst`, `io/images/train.gif` | Low-level IOHelper `DumperParaview` usage: register mesh, filtered mesh (element groups), nodal/elemental fields, animate a train | `swiss_train.geo` | `DumperParaview`, `registerFilteredMesh`, `dumpers::NodalField`, `ElementTypeMapArray` colouring | `paraview/` series | S | yes | dumpers/IOHelper | sequential | Output format compatibility (VTU) — Rust needs a ParaView writer. |
| EX-026 | `dumpable_interface` | `io/dumper/dumpable_interface.cc` (+ `locomotive_tools`) | Same scene via the high-level `Dumpable` interface of `Mesh`/`ElementGroup` (`registerDumper`, `addDumpFieldExternal`, `setDirectoryToDumper`) | `swiss_train.geo` | `Dumpable` API | `paraview/dumpable/*` | S | yes | dumpers | sequential | — |
| EX-027 | `example_parser` | `io/parser/example_parser.cc`, `input_file.dat`, `swiss_cheese.geo` | Reading user parameters (mesh file, BC values, `eigen_stress` matrix) from the input file's `user` section; static solve | `input_file.dat`, `swiss_cheese.geo` (2D order 2) | `getUserParser()`, `getParameterValue<std::string>`, `FromStress`, static | ParaView `swiss_cheese` | S | yes | core (implicit for static solve) | sequential | — |
| EX-028 | `bernoulli_beam_2_example` | `structural_mechanics_model/bernoulli_beam_2_example.cc` | Two-span Bernoulli beam with distributed load equivalent nodal forces/moments, static; mesh built programmatically | none | `StructuralMechanicsModel`, `MeshAccessor`, `StructuralMaterial`, element-material map | ParaView | S | yes | structural_mechanics | sequential | Analytic solution available (classical beam). |
| EX-029 | Example build infrastructure | `examples/CMakeLists.txt`, every `examples/**/CMakeLists.txt` (46 files), `cmake/AkantuExampleMacros.cmake`, `cmake/AkantuSimulationMacros.cmake` | see B.0 | — | — | — | — | — | — | `PARALLEL` → 2 ranks | Rust: example binaries under `examples/` of the workspace + a runner script mirroring `<name>-test`. |

### B.2 Python examples (`examples/python/**`)

All are run as `python <script>` from the build directory; they require the
`akantu` package and the generated mesh. None asserts results (exit code only).

| ID | Name (ctest name) | Original files | Purpose / physics | Inputs | Python API exercised | Expected outputs | Run time | Test? | Required packages | Parallel | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
| EX-030 | `stiffness_matrix.py` (`stiffness_matrix-test`) | `solid_mechanics_model/stiffness_matrix/stiffness_matrix.py`, `plate.geo`, **`plate.msh` (committed, but CMake regenerates via `add_mesh`)**, `material.dat`, `README.rst` | Assemble K of a static plate and print it as a dense matrix | `plate.msh`, `material.dat` | `SolidMechanicsModel.initFull(_analysis_method=_static)`, `assembleStiffnessMatrix()`, `getDOFManager().getMatrix("K")`, `AkantuSparseMatrix(K).toarray()` | stdout dense matrix | S | yes | solid_mechanics, implicit, scipy | seq | Good differential-test seed (K entries). |
| EX-031 | `dynamics.py` (`dynamics-python-test`) | `solid_mechanics_model/dynamics/dynamics.py`, `bar.geo`, `material.dat`, `README.rst`, `images/*` | 2D bar, Gaussian-modulated pulse, explicit dynamics 250 steps, prints energy CSV | `bar.msh`, `material.dat` | **subclass of `aka.FixedValue` overriding `__call__`**, `applyBC`, `getDisplacement` view writes, `getStableTimeStep`, `setTimeStep`, `solveStep`, `getEnergy("potential"/"kinetic")`, dumpers | stdout `step,time,epot,ekin,total`; ParaView `waves` | S | yes | solid_mechanics | seq | Also the smoke test of the wheel jobs (CI-012, CI-013). |
| EX-032 | `plate-hole/plate.py` (`plate-hole-test`) | `solid_mechanics_model/plate-hole/plate.py`, `plate.geo`, `material.dat`, `README.rst`, `images/*` | Static plate with a hole under traction | `plate.msh` | **mpi4py** rank detection, `mesh.distribute()`, `FromTraction`, `getExternalForce()[:] = 0`, `NonLinearSolver.set(...)` | ParaView `plate` | S | yes | solid_mechanics, implicit | MPI-capable (run serial in CI) | Only example written for MPI. |
| EX-033 | `custom-material.py` (`custom-material-test`) | `solid_mechanics_model/custom-material/custom-material.py`, `bar.geo`, `material.dat` (`material local_elastic [...]`), `README.rst`, `images/*` | Python-defined linear elastic material (`LocalElastic(aka.Material)`) registered with `MaterialFactory.registerAllocator`; explicit wave in a bar, 250 steps | `bar.msh`, `material.dat` | Material trampoline (`initMaterial`, `computeStress`, `computeTangentModuli`, `computePotentialEnergy`, `getPushWaveSpeed`), `registerParamReal`, `getReal`, `getGradU`/`getStress`/`getPotentialEnergy` views | stdout energies, ParaView `waves` | M (Python per-quad loops vectorised with einsum) | yes | solid_mechanics | seq | Core Python extension scenario. |
| EX-034 | `bi-material.py` (`bi-material-test`) | `custom-material/bi-material.py`, `square.geo` | Python material with internal fields (`registerInternalReal("factor",1)`, `"quad_coordinates"`), stiffness halved for y<0.5; static; material defined through `initFull({...dict...})` (no `.dat` needed) and `registerCustomMaterials` | `square.msh` | `registerInternalReal`, `InternalFieldReal.elementTypes()`/`__call__`, `getFEEngine().computeIntegrationPointsCoordinates(field, getElementFilter())`, `getInternalReal`, `initFull(dict, _analysis_method=_static)`, `addDumpField("factor")` (dumping a Python-registered internal) | stdout potential energy, ParaView `bimaterial` | S | yes | solid_mechanics, implicit | seq | Tests that Python-registered internals are dumpable. |
| EX-035 | `solver_callback.py` (`solver_callback-test`) | `solid_mechanics_model/solver_callback/solver_callback.py`, `bar.geo`, `material.dat`, `README.rst` | Periodic left/right coupling of a 2D bar via a Python `InterceptSolverCallback` that modifies K (`TermsToAssemble`) and the residual; implicit dynamics, 250 steps | `bar.msh` | `InterceptSolverCallback` overrides `assembleMatrix`, `assembleResidual`; `getMatrixType`, `getRelease`, `saveMatrix("K0.mtx")`, `assemblePreassembledMatrix`, `assembleToResidual`, `model.solveStep(callback)` | `K0.mtx`, `K1.mtx`, stdout energies, ParaView `solver_callback` | M | yes | solid_mechanics, implicit | seq | Exercises the matrix "release" counter semantics. |
| EX-036 | `eigen_modes.py` (`eigen-modes-test`) | `solid_mechanics_model/eigen_modes/eigen_modes.py`, `image_saver.py`, `material.dat`, `README.rst`, `images/*` | Writes a `.geo`, **calls `gmsh -2` via `subprocess`**, assembles K and M, solves the generalised eigenproblem with `scipy.sparse.linalg.eigsh` on free DOFs, sets an eigenmode as initial displacement and integrates 500 implicit steps | generated `bar.geo` → `bar.msh` | `assembleMass`, `getMatrix("M")`, `AkantuSparseMatrix`, `displacement.reshape(n*2)[mask] = v` (write through a reshaped view), optional matplotlib | `modes.txt` (append), ParaView `waves-<mode>` | M | yes | solid_mechanics, implicit, scipy | seq | Requires `gmsh` executable at run time. |
| EX-037 | `cohesive/plate.py` (`cohesive_python-test`) | `solid_mechanics_cohesive_model/cohesive/plate.py`, `plate.geo`, `material.dat`, `README.rst`, `images/*` | Static pre-load of a plate then extrinsic cohesive explicit dynamics, 100 steps | `plate.msh` | `SolidMechanicsModelCohesive.initFull(_static, _is_extrinsic=True)`, `initNewSolver(_explicit_lumped_mass)`, named solvers `"static"`/`"explicit_lumped"`, `checkCohesiveStress`, cohesive dumper `"cohesive elements"` fields `damage, tractions, opening` | ParaView `plate`, `cohesive` | M | yes | cohesive_element, implicit | seq | — |
| EX-038 | `cohesive/custom_material.py` (none) | `cohesive/custom_material.py`, `local_material.dat` | Python-defined linear cohesive law (`LinearCohesive(aka.MaterialCohesive)`) with `checkInsertion` and `computeTraction` | `plate.msh`, `local_material.dat` | `MaterialCohesive` trampoline, `getFacetFilter`, `getElementInserter().getCheckFacets/getInsertionFacets`, `getFEEngine("FacetsFEEngine")`, `getStressOnFacets`, `getTangents`, **`getNormals` (not bound → AttributeError)** | — | M | **no (not registered; broken)** | cohesive_element | seq | Decide whether Rust binds `getNormals` (A.9 #18). |
| EX-039 | `fragmentation.py` (`fragmentation-test`) | `solid_mechanics_cohesive_model/fragmentation/fragmentation.py`, `fragmentation_mesh.geo`, `material.dat`, `README.rst`, `images/*` | 1D-like bar pulled apart by a velocity field, extrinsic cohesive fragmentation, 5000 steps; fragment statistics | `fragmentation_mesh.msh` | custom `DirichletFunctor` with time state re-applied each step, `updateAutomaticInsertion`, energies `potential/kinetic/dissipated/reversible/contact`, `getMaterial(0).getStress(_triangle_3)`, `FragmentManager.computeAllData/getNbFragment/getNbElementsPerFragment` | stdout fragment counts, ParaView | L | yes | cohesive_element | seq | Statistical output (fragment count) — good integration check. |
| EX-040 | `multi-material/plate.py` (`multi_material_py-test`) | `solid_mechanics_cohesive_model/multi-material/plate.py`, `plate.geo`, `material.dat`, `README.rst` | Cohesive properties chosen per pair of physical groups via `MaterialCohesiveRulesSelector({("Top","Bottom"): "tough", ...})`, 1000 explicit steps | `plate.msh` | rules selector (dict-of-tuple conversion), `setMaterialSelector` before `initFull` | ParaView | L | yes | cohesive_element | seq | — |
| EX-041 | `phasefield-static.py` (`phasefield_static-test`) | `phase_field_model/phasefield-static.py`, `plate_static.geo`, `material_static.dat`, `README.rst`, `images/*` | Quasi-static phase-field fracture, 1500 load steps, Python-driven staggered loop until displacement & damage increments < 1e-5 | `plate_static.msh`, `material_static.dat` | `CouplerSolidPhaseField`, `getSolidMechanicsModel/getPhaseFieldModel`, `phase.savePreviousState()`, `model.solve("static","static")`, live views of displacement/damage | ParaView `phasefield-static` | XL | yes | phase_field, implicit | seq | Long; iteration cap 5000 raises `Exception`. |
| EX-042 | `phasefield-dynamic.py` (`phasefield_dynamic-test`) | `phase_field_model/phasefield-dynamic.py`, `plate.geo`, `material.dat` | Dynamic phase-field fracture: explicit solid + static phase field, 1500 steps | `plate.msh` | `initNewSolver(_explicit_lumped_mass)`, `model.solve("explicit_lumped","static")` | ParaView `plate` | L | yes | phase_field, implicit | seq | — |
| EX-043 | `compression.py` (`compression-test`) | `contact_mechanics_model/compression/compression.py`, `compression.geo`, `compression.dat`, `README.rst`, `images/*` | Two bodies in contact compressed, explicit, 20 000 steps with periodic velocity damping | `compression.msh` | `CouplerSolidContact`, `MeshDataMaterialSelectorString("physical_names", solid)`, `PhysicalSurfaceSelector`, `velocity *= 0.9` in place, coupler dump fields `contact_force, gaps, areas, grad_u` | ParaView `compression` | XL | yes | contact_mechanics | seq | — |
| EX-044 | `detection-explicit.py` (`detection_explicit.py-test`) | `compression/detection-explicit.py`, **`detection-explicit.msh` (committed)**, `detection-explicit.dat` | One contact search on a committed mesh; prints normals, gaps and contact element list, times the search | committed mesh | `ContactMechanicsModel.search()`, `getNormals`, `getGaps`, `getContactElements()` → list of `ContactElement` repr | stdout | S | yes | contact_mechanics | seq | Deterministic output → easy golden test. |
| EX-045 | `friction/bloc_friction.py` (`bloc_friction_python-test`) | `contact_mechanics_model/friction/bloc_friction.py`, `bloc.geo`, `material.dat`, `README.rst` | Python twin of EX-004 (3×10 000 steps) with per-node damping loop in Python | `bloc.msh` | coupler API, `zip(gaps, velocity)` iteration over views | ParaView `bloc_dumped` | XL | yes | contact_mechanics | seq | Very slow in Python (per-node loops each step). |
| EX-046 | `structural_mechanics.py` (`structural_mechanics-test`) | `structural_mechanics_model/structural_mechanics.py`, `images/displacements.png` | 2-element Bernoulli beam built with `MeshAccessor`, implicit dynamics, **N = 1 000 000 steps** | none | `Mesh.addConnectivityType`, `MeshAccessor`, `StructuralMaterial` fields, `addMaterial`, `getElementMaterial` view, `getExternalForce`/`getBlockedDOFs` writes | optional matplotlib plot | XL | yes | structural_mechanics | seq | Dominant CI cost; Rust port should keep step count for parity runs but mark expensive tier. |
| EX-047 | `structural_mechanics_softening.py` (`structural_mechanics_softening-test`) | `structural_mechanics_softening.py`, `images/displacements_softening.png` | 3-element beam, middle element "softened" over 1000 steps by `mat2.E *= factor`, N = 600 000 implicit steps | none | `addMaterial(mat, name)`, `getMaterial(name)` (**copy** → softening has no effect, A.9 #12) | plot | XL | yes | structural_mechanics | seq | Behavioural divergence risk if Rust returns live references. |
| EX-048 | `structural_mechanics_dynamics.py` (`structural_mechanics_dynamics-test`) | `structural_mechanics_dynamics.py` | 40-element simply supported beam, point load, implicit dynamics 900 steps, Rayleigh-type damping `C += 1e-5 M + 1e-5 K`, comparison with series analytical solution (plotted only) | none | `assembleMatrix("M"/"K")`, `getDOFManager().getMatrix("C").add(M, a)`, `getDataReal("extra_normal", _bernoulli_beam_3)` (branch for 3D beam) | plot | M | yes | structural_mechanics | seq | Analytical solution in-script → turn into a real assertion in Rust. |
| EX-049 | Scalability tooling | `examples/c++/scalability_test/run.sh`, `scalability.sh`, `plot.py` | SLURM scripts submitting 1–720 ranks (72 per node) for the elastic/cohesive scalability study; `plot.py` plots time-to-solution from CSVs | — | — | — | — | no | — | HPC (srun) | For `docs/BENCHMARKS.md`; AWS-only in our setting. |
| EX-050 | Example documentation assets | all `examples/**/README.rst` (36 files) and `examples/**/images/*` (43 files) | Sphinx-included tutorials (`doc/dev-doc/examples.rst` includes `examples/README.rst`) describing each example, analytic background (Hertz, beams), figures | — | — | — | — | no | documentation | — | Port prose as Rust-side docs; images are illustrative only. |

---

## C. Test catalogue

### C.0 Registration macros and pass/fail semantics (summary; details in CI-003…CI-008)

| Macro / mechanism | File | Semantics relevant to porting |
|---|---|---|
| `add_test_tree(test)` | `cmake/AkantuTestsMacros.cmake:118-140` | Root entry; adds the test folder of every **activated** package (`package_get_all_test_folders`), i.e. `test/` plus `extra_packages/<pkg>/test` for activated extra packages. |
| `add_akantu_test(dir desc)` | `AkantuTestsMacros.cmake:173-237` | Silently `return()`s if `dir` does not exist (e.g. `test_mesh`, `test_contact`, `test_heat_transfer_model`, `test_pbc_tweak` listed but absent). Creates option `AKANTU_BUILD_<DIR>`; forced ON by `AKANTU_BUILD_ALL_TESTS` (default ON). |
| `register_test(name ...)` | `AkantuTestsMacros.cmake:391-603` | Flags `UNSTABLE PARALLEL PYTHON GTEST HEADER_ONLY`; one-value `POSTPROCESS SCRIPT`; multi `SOURCES FILES_TO_COPY DEPENDS DIRECTORIES_TO_CREATE COMPILE_OPTIONS EXTRA_FILES LINK_LIBRARIES INCLUDE_DIRECTORIES PACKAGE PARALLEL_LEVEL`. Active only if **all** `PACKAGE`s are activated (`PYTHON` adds `python_interface`) and not (`UNSTABLE` and not `AKANTU_BUILD_UNSTABLE_TESTS`). Executable = sources; if `<name>.sh` exists in the source dir **the script is run instead of the binary**; if `<name>.verified` exists it is passed to the driver as `-r` (but never compared, finding 1). `COMPILE_OPTIONS` become preprocessor definitions (used for typed variants, e.g. `TYPE=...`). |
| Rank counts | `AkantuTestsMacros.cmake:504-536,559-603` | Without `PARALLEL` (or package `parallel` inactive) → 1 rank, test name `<name>`. With `PARALLEL` and `PARALLEL_LEVEL l1 l2 ...` → one ctest per level, suffix `_<p>` for p>1. With `PARALLEL` and no level → `N, N/2, ..., 2, 1` where `N = ProcessorCount` (bug: `set(N MPIEXEC_MAX_NUMPROCS)` assigns the literal string). `AKANTU_TESTS_ALWAYS_USE_MPI` runs sequential tests through `mpirun -np 1`. **gtest binaries with p>1 are skipped when CMake < 3.31** (l.565-570). |
| `register_gtest_sources(...)` + `register_gtest_test(name ...)` | `AkantuTestsMacros.cmake:279-389` | Sources (each with its own `PACKAGE` filter) accumulate per directory; `register_gtest_test` builds ONE binary `<name>_gtest` with `test/test_gtest_main.cc`, links `gtest_main` (+ `pybind11::embed` and `-DAKANTU_TEST_USE_PYBIND11` if any source required `python_interface`), and uses `gtest_discover_tests` (each gtest case = one ctest, with `--discovery` passed to the driver). Flags of any source (e.g. `PARALLEL`) become global for the binary. A directory calling only `register_gtest_sources` produces **no test** (`test_selector`, `test_detector`). |
| gtest main | `test/test_gtest_main.cc:33-62` | `akantu::initialize/finalize` in a global environment; optional `py::scoped_interpreter`; only rank 0 prints results. |
| Typed-test helpers | `test/test_gtest_utils.hh:28-257` | `gtest_list_t`, `tuple_filter_t`, `cross_product_t`; `TestElementTypes` = all regular element types except `_point_1` (12 types: `_segment_2, _segment_3, _triangle_3, _triangle_6, _quadrangle_4, _quadrangle_8, _tetrahedron_4, _tetrahedron_10, _pentahedron_6, _pentahedron_15, _hexahedron_8, _hexahedron_20`); `TestCohesiveElementTypes`, `StructuralTestElementTypes`, `TestAllDimensions` (1,2,3); `Polynomial<degree>` helper with exact integration (used by gauss-integration test). |
| Driver | `cmake/akantu_test_driver.sh` | `-e exe -n name [-p "mpirun ... -np"] [-N nprocs] [-s pp] [-r ref] [-w dir] [-E env] [-P python] [-v valgrind]`; sources `akantu_environement.sh` (sets `PYTHONPATH` to `build/python`, `test/`, `test/test_fe_engine`), runs `[valgrind] [python] exe` (or `mpirun -np N ...`), tees `name.lastout/.lasterr`; exits 0 before post-processing/diff (finding 1). Pass = exit code 0. |
| Mesh generation | `cmake/Modules/FindGMSH.cmake:46-110` | `add_mesh` → `gmsh -<dim> -order <order> -o out.msh in.geo` at build time. |

### C.1 Activation summary (upstream CI configuration, `.gitlab/ci.d/templates.yaml` `.configure`)

CI enables: cohesive_element, implicit (`Mumps+PETSc`), parallel, structural_mechanics,
diffusion, damage_non_local, phase_field, python_interface, contact_mechanics,
traction_at_split_node_contact, examples (+`AKANTU_TEST_EXAMPLES` in Release), tests.
Not enabled: CGAL, embedded, extra_materials, extra_materials_non_local, IGFEM,
unstable tests. Hence the status column below:

- `CI` — active in upstream CI;
- `INACTIVE(pkg)` — registered but needs a package CI does not enable;
- `DEAD(pkg)` — registered with a package that does not exist (never active);
- `UNSTABLE` — needs `AKANTU_BUILD_UNSTABLE_TESTS`;
- `NOT REGISTERED` — source exists but no registration reaches it (§C.3).

Kinds: `gtest` (cases discovered by ctest), `exe` (exit code), `exe+sh`
(script `<name>.sh` run instead), `py-script` (Python script run by the driver;
pass = exit code, assertions raise), `pytest` (script calling `pytest.main`).

Cost: S < 10 s, M < 1 min, L < 10 min, XL ≥ 10 min (estimates, unverified).

### C.2 Registered tests

#### C.2.1 `test/test_common` (core containers, tensors)

| ID | Name (ctest) | Original files | Kind | Behavior / purpose; pass/fail criterion | Inputs | Parameterisation | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-001 | `test_grid` | `test/test_common/test_grid.cc`, `circle.geo` | exe | Builds a `SpatialGrid` from the element barycenters of a circle mesh and prints it; smoke test (no assertion; also references `grid.msh` output) | `circle.geo`→`circle.msh` (2D o1) | — | core / CI | 1 | S | Port as unit test of the spatial grid with explicit cell-count checks. |
| TEST-002 | `test_common_gtest: CSR` | `test_csr.cc` | gtest | `TestCsrFixture.{CheckInsertion,Iteration,ReverseIteration}` — `CSR<UInt>` container insertion/iteration, `EXPECT_EQ` | — | — | core / CI | 1 | S | `CSR` = compressed row storage helper (R1). |
| TEST-003 | `test_common_gtest: Array` | `test_array.cc` | gtest | `ArrayConstructor.ConstructDefault1-5, Reserve`; `ArrayFixture.{Copy, Set, Resize, PushBack, ViewVectorDynamic/Static, ViewMatrixStatic/Dynamic, ViewVectorWrong, ViewMatrixWrong (EXPECT_THROW), ViewMatrixIter, ConstViewVector, EnumerateArray, ZipArray, IteratorIncrement, IteratorFixSize, IteratorBracket, IteratorSimple, IteratorThrow, IteratorRange, DynamicSizeIteratorFilter, IteratorFilter}` | — | typed over `Real, UInt, NonTrivial` | core / CI | 1 | S | Defines Array/view/iterator contract (make_view, zip, enumerate, filter). |
| TEST-004 | `test_common_gtest: Tensors` | `test_tensors.cc` | gtest | 47 cases on `Vector`/`Matrix` (Eigen-based) construction, proxies, arithmetic, slicing, iterators, `MatrixEigs` (eigenvalues, tol 1e-14) — `EXPECT_NEAR/DOUBLE_EQ` | — | — | core / CI | 1 | S | Mostly Eigen semantics; port the akantu-specific ones (proxy views, eigs ordering). |
| TEST-005 | `test_common_gtest: VoigtHelper` | `test_voigt_helper.cc` | gtest | `VoigtHelperFixture.{Size, Indicies, Factors, MatrixToVoight, MatrixToVoightFactors, VoightToMatrix}` exact | — | typed over dim 1,2,3 | core / CI | 1 | S | Voigt ordering conventions are load-bearing for all materials. |

#### C.2.2 `test/test_fe_engine`

| ID | Name (ctest) | Original files | Kind | Behavior / purpose; pass/fail criterion | Inputs | Parameterisation | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-010 | `test_mesh_data_string` | `test_mesh_data.cc` (`-DTYPE=string -DVALUE1="5" -DVALUE2="10"`) | exe | `MeshData` typed storage round-trip; `AKANTU_DEBUG_ASSERT` (active only in debug builds!) | — | compile-time `TYPE` | core / CI | 1 | S | In Release builds the assert is compiled out → test is a smoke test. |
| TEST-011 | `test_mesh_data_UInt` | `test_mesh_data.cc` (`-DTYPE=UInt -DVALUE1=5 -DVALUE2=10`) | exe | idem for `UInt` | — | — | core / CI | 1 | S | — |
| TEST-012 | `test_mesh_boundary` | `test_mesh_boundary.cc`, `cube.geo`, `cube_physical_names.geo` | exe | Reads `cube_physical_names.msh`, fails (`return 1`) if no boundary element groups of dim-1; prints groups | `cube_physical_names.geo` (3D o1) | — | core / CI | 1 | S | — |
| TEST-013 | `test_facet_element_mapping` | `test_facet_element_mapping.cc` | exe | Builds facets, prints element→sub-element and sub-element→element maps (`ElementNull` handling); smoke | `cube_physical_names.msh` | — | core / CI | 1 | S | Output could become a golden file. |
| TEST-014 | `test_fe_engine_gtest: Precompute` | `test_fe_engine_precomputation.cc`, `test_fe_engine_fixture.hh`, `py_engine/py_engine.py`, `py_engine/__init__.py` | gtest (+embedded Python) | `TestFEMPyFixture.Precompute`: shapes N, derivatives B and Jacobians j computed by akantu vs an **independent NumPy implementation** (`py_engine.Shapes(type).precompute(N,B,j,X,Q)`), `EXPECT_NEAR(0, ‖·‖∞, 1e-10)` | `_<type>.msh` (committed) | 12 regular types | core + python_interface / CI | 1 | S | `py_engine.py` (532 lines) is an excellent independent oracle for shape functions — reuse in Rust tests. |
| TEST-015 | `test_fe_engine_gtest: PrecomputeRotations` | `test_fe_engine_precomputation_structural.cc`, `test_fe_engine_structural_fixture.hh` | gtest | `TestBernoulliB2/B3.PrecomputeRotations` (rotation matrices of beam elements, 1e-15); `TestDKT18.DISABLED_PrecomputeRotations` | `_bernoulli_beam_2.msh`, `_bernoulli_beam_3.msh`, `_discrete_kirchhoff_triangle_18.msh` (read with `_miot_gmsh_struct`) | — | structural_mechanics / CI | 1 | S | DKT18 disabled upstream. |
| TEST-016 | `test_fe_engine_gtest: Inradius` | `test_fe_engine_inradius.cc` | gtest | `TestFEMFixture.Inradius`: element inradius vs values in `_<type>.csv`, `EXPECT_NEAR` rel 1e-6 | `_<type>.msh`, `_<type>.csv` (GOLD-002) | 12 types | core / CI | 1 | S | CSV = golden data (12 files). |
| TEST-017 | `test_fe_engine_gtest: GaussIntegration` | `test_fe_engine_gauss_integration.cc` | gtest | Exact integration of polynomials up to the quadrature order on each element type (`Polynomial` helper), 5e-14 | `_<type>.msh` | 12 types × polynomial degrees | core / CI | 1 | S | Independent math check (quadrature exactness). |
| TEST-018 | `test_fe_engine_gtest: Gradient` | `test_gradient.cc` | gtest | `GradientPoly` (gradient of a linear field = constant, 2e-14) and `GradientPositions` (∇x = I, 5e-13) | `_<type>.msh` | 12 types | core / CI | 1 | S | — |
| TEST-019 | `test_fe_engine_gtest: Integrate` | `test_integrate.cc` | gtest | `IntegrateConstant`: ∫1 = mesh volume, 1e-14 | `_<type>.msh` | 12 types | core / CI | 1 | S | — |
| TEST-020 | `test_fe_engine_gtest: InverseMap` | `test_inverse_map.cc` | gtest | `InverseMap`: natural coords of quadrature points recovered from physical coords, 3.5e-11 | `_<type>.msh` | 12 types | core / CI | 1 | S | Newton inverse map tolerance. |
| TEST-021 | `test_fe_engine_gtest: VariationalOperators` | `test_variational_operators.cc` | gtest | `evaluate_field_on_support`, `consistent_force`, `stiffness_matrix` — prototype of expression-style variational operators (`Support`, `NodalTensorField`, `GradientOperator`, `IntegrationPointTensorField` defined in the test file itself, added 2023-12); **no assertions, evaluation lines commented out** → construction smoke test only | `_<type>.msh` | 12 types | core / CI | 1 | S | Experimental/unfinished upstream; port only if the variational-operator API is adopted (ARCHITECTURE decision). |

#### C.2.3 `test/test_geometry` (CGAL)

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-025 | `test_geometry_predicates` | `test_geometry/test_geometry_predicates.cc` | exe | CGAL predicates (points/segments/triangles), `EXIT_FAILURE` on mismatch | — | — | CGAL / INACTIVE(CGAL) | 1 | S | Needs a Rust robust-predicates crate. |
| TEST-026 | `test_geometry_intersection` | `test_geometry_intersection.cc`, `test_geometry_triangle.msh` | exe | Mesh-geometry intersection (triangle mesh vs primitives) | committed `test_geometry_triangle.msh` | — | CGAL / INACTIVE | 1 | S | — |
| TEST-027 | `test_segment_intersection_triangle_3` | `test_segment_intersection_triangle_3.cc` | exe | Segment ∩ triangle mesh, tol 1e-10 | `test_geometry_triangle.msh` | — | CGAL / INACTIVE | 1 | S | Used by embedded model. |
| TEST-028 | `test_segment_intersection_tetrahedron_4` | `test_segment_intersection_tetrahedron_4.cc` | exe | Segment ∩ tetrahedral mesh | committed `test_geometry_tetrahedron.msh` | — | CGAL / INACTIVE | 1 | S | — |

#### C.2.4 `test/test_io`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-030 | `test_dumper` | `test_io/test_dumper/test_dumper.cc` | exe | Dumps a mesh, filtered boundary group, nodal & elemental fields through `DumperParaview`/IOHelper (with `unRegisterField`); smoke | `input_file.dat`, committed `test_dumper.msh`, dir `paraview` | — | IOHelper / CI | 1 | S | VTU writer smoke test. |
| TEST-031 | `test_parser` | `test_io/test_parser/test_parser.cc`, `test_parser.verified` | exe | Parses `input_file.dat`; checks `toto == 2π + max(2, general)` to machine epsilon (`return 1` otherwise); prints sections, random parameters (`10 uniform [0.2,0.5]`, `weibull`), vector/matrix params | `input_file.dat` | — | core / CI | 1 | S | `.verified` (GOLD-001) = expected printout incl. seed check `123456==123456`; never diffed upstream. |
| TEST-032 | `parser_gtest: AlgebraicParser` | `test_parser_gtest.cc` | gtest | `AlgebraicParser.{Number, Expr, Term, Factor, Func, Constant, Complex}` — expression grammar (numbers like `4e5`, `1e9`, functions, constants), `EXPECT_NEAR` | — | — | core / CI | 1 | S | Grammar spec for the Rust parser (R1). |
| TEST-033 | `test_parser_python` | `test_parser.py` | pytest (script ends with `pytest.main([__file__, "-v"])` under `__main__`) | Intended: `ParameterRegistry` register/set/get, `parseInput` + `getStaticParser().getParameter("toto")` value & expression string, `asMatrix()` vs reference, `Parsable.parseSection` and `pytest.raises(aka.Exception)` for non-readable param | `input_file.dat` | — | core + python_interface / CI | 1 | S | Must pass unchanged on the Rust module (5 test methods in 3 classes). |

#### C.2.5 `test/test_mesh_utils`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-035 | `test_purify_mesh` | `test_mesh_utils/test_purify_mesh.cc` | exe | `MeshUtils::purifyMesh` removes unused nodes; `AKANTU_ERROR` unless 21 nodes and 4 `_quadrangle_8` remain; writes `purify_mesh_after.msh` (mesh writer!) | committed `purify_mesh.msh` | — | core / CI | 1 | S | Requires a Gmsh **writer**. |
| TEST-036 | `test_mesh_iterators` | `test_mesh_iterators.cc`, `iterators_mesh.geo` | exe | Iterates element/node groups (`iterateElementGroups`, `enumerate`), prints; smoke | `iterators_mesh.geo` → `iterators_mesh.msh` (3D o1) | — | core / CI | 1 | S | — |
| TEST-037 | `test_mesh_io_msh` | `test_mesh_io/test_mesh_io_msh.cc`, `cube.geo` | exe | `MeshIOMSH::read` of a generated cube, prints mesh | `cube.geo` (3D o1), dir `paraview` | — | core / CI | 1 | S | Output depends on gmsh version (node numbering). |
| TEST-038 | `test_mesh_io_msh_physical_names` | `test_mesh_io_msh_physical_names.cc`, `cube_physical_names.geo`, `test_mesh_io_msh_physical_names.verified` | exe | Reads physical names as `physical_names` mesh data and prints per-element names | `cube_physical_names.geo` | — | core / CI | 1 | S | GOLD-001 entry; output depends on gmsh numbering → golden only valid for a given gmsh version. |
| TEST-039 | `test_buildfacets_gtest: Buildfacets` | `test_buildfacets/test_buildfacets.cc`, 13 committed meshes, `test_buildfacets_gtest.verified` | gtest | `TestBuildFacetFixture.Buildfacets` builds facets (and facets of facets) and prints facet/element connectivity tables; assertions (unverified, appears print-only) | committed `_triangle_3.msh … _hexahedron_20_pentahedron_15.msh` (MDATA-002) | 13 element-type combos incl. mixed meshes (quad+tri, hex+penta) | core / CI | 1 | S | The `.verified` file (GOLD-001) is the real oracle — but never compared upstream. |
| TEST-040 | `test_mesh_partitionate_scotch` | `test_mesh_partitionate/test_mesh_partitionate_scotch.cc`, `triangle.geo` | exe | `MeshPartitionScotch::partitionate(8)`, dump partition field, `reorder()`; smoke | `triangle.geo` (2D **o2**) | — | Scotch / CI (parallel→scotch) | 1 | S | Partition is library-dependent; Rust may use another partitioner (record decision). |
| TEST-041 | `test_mesh_partitionate_mesh_data` | `test_mesh_partitionate_mesh_data.cc`, `quad.geo` | exe | Partition from user-provided mesh data (barycenter-based proc id) via `MeshPartitionMeshData`; checks partition equals data (`return 1`) | `quad.geo` (2D o1) | 8 partitions | Scotch / CI | 1 | S | Deterministic → easy port. |
| TEST-042 | `test_mesh_periodic_gtest: MeshPeriodic` | `test_mesh_periodic/test_mesh_periodic.cc`, `test_mesh_periodic_fixture.hh`, `line.geo`, `square.geo`, `cube.geo` | gtest | `TestMeshPeriodic.{SimpleMakePeriodic, MakeConsistentForPeriodicityIdentity, MakeConsistentForPeriodicitySum}` — master/slave pairing, synchronisation of periodic arrays (1e-7) | generated `line.msh`, `square.msh`, `cube.msh` | `(dim, periodic dims, no_load)`: (1,1),(2,1),(2,2),(3,1),(3,2),(3,3) | core / CI | **PARALLEL_LEVEL 1 2** (`_2` variant only with CMake ≥3.31) | S | Periodicity is newer code (2024–25). |
| TEST-043 | `test_mesh_periodic_gtest: PeriodicStatic` | `test_periodic_static.cc`, `test_periodic_static_fixture.hh`, `material.dat` | gtest | `TestPeriodicStatic.Traction`: periodic static solve under tensile load, comparison with analytical solution (1e-6, 2e-9) | `cube.msh`, `material.dat` | `(dim, periodic dirs, tensile)`: (1,1),(2,1),(3,0),(3,1),(3,2) | core (implicit at run time) / CI | 1, 2 | M | — |
| TEST-044 | `test_mesh_periodic_gtest: PeriodicMass` | `test_periodic_mass.cc` | gtest | `TestPeriodicMass.UniformMass`: lumped mass with periodicity uniform (2e-11) | `cube.msh`, `material.dat` | — | core / CI | 1, 2 | S | — |

#### C.2.6 `test/test_model/patch_tests`

Patch-test meshes: `test/test_model/patch_tests/data/_<type>.msh` (12 committed,
MDATA-003) and generated `bar_<type>.msh` (from `bar_*.geo`, used by the SMM
dynamics/energy tests).

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-050 | `patch_test_linear_gtest: Explicit*` | `patch_test_linear_elastic_explicit.cc`, `patch_test_linear_fixture.hh`, `patch_test_linear_solid_mechanics_fixture.hh` | gtest | `TestPatchTestSMMLinear.{Explicit, ExplicitFiniteDeformation}`: linear displacement field imposed on boundary + initial state, 100 explicit steps; checks `ekin < 1e-16`, DOFs exact (1e-10), gradient (1e-8 rel), stress (1e-9 rel) vs Hooke (plane strain/stress), internal-force balance 1e-8 | `data/material_check_stress_plane_{strain,stress}{,_finite_deformation}.dat`, `_<type>.msh` | cross product: 12 element types × {plane strain, plane stress} (stress only meaningful in 2D: filtered by `invalid_plan_stress`) × solver options (default/eigen [+mumps] [+petsc, petsc_snes]) | solid_mechanics / CI | 1 | M | **Primary numerical acceptance test** for elements + linear elasticity. |
| TEST-051 | `patch_test_linear_gtest: Implicit/Static*` | `patch_test_linear_elastic_implicit.cc` | gtest | `{Implicit, Static, ImplicitFiniteDeformation, StaticFiniteDeformation}`; implicit: `ekin < 1e-13`; static: NR `threshold 1e-6` (PETSc SNES: `snes_rtol 1e-10, snes_atol 2e-5, snes_stol 1e-10`, `ksp_rtol 1e-30`) | idem | idem | solid_mechanics + implicit / CI | 1 | M | Solver-option axis tests MUMPS, Eigen, PETSc backends. |
| TEST-052 | `patch_test_linear_gtest: AnisotropicExplicit` | `patch_test_linear_anisotropic_explicit.cc` | gtest | `AnisotropicExplicit` with full 3×3×3×3 C tensor (hard-coded, contains ~1e-10 noise), `result_tolerance = 1e-11`, `ekin < 1e-16` | `data/material_anisotropic_{1,2,3}.dat` | element types × options | solid_mechanics / CI | 1 | M | — |
| TEST-053 | `patch_test_linear_gtest: AnisotropicStatic` | `patch_test_linear_anisotropic_implicit.cc` | gtest | static; `result_tolerance = 1e-9`, NR threshold 1e-10 | idem | idem | solid_mechanics + implicit / CI | 1 | M | — |
| TEST-054 | `patch_test_linear_gtest: TestLumpedMass` | `test_lumped_mass.cc` | gtest | `TestLumpedMassesFixture.TestLumpedMass`: Σ lumped nodal mass = ρ·V (rel 2e-15) per component | `data/material_lumped.dat`, `_<type>.msh` | 12 types | solid_mechanics / CI | 1 | S | Independent check for mass lumping rules. |
| TEST-055 | `patch_test_linear_gtest: TestPatchTestHTMLinear.Explicit` | `patch_test_linear_heat_transfer_explicit.cc`, `patch_test_linear_heat_transfer_fixture.hh` | gtest | Linear temperature patch test, explicit | `data/heat_transfer_input.dat` | 12 types | **DEAD(heat_transfer)** | 1 | S | Port with `diffusion` model; finding 3. |
| TEST-056 | `...: TestPatchTestHTMLinear.Static` | `patch_test_linear_heat_transfer_static.cc` | gtest | static, threshold 2e-4 | idem | 12 types | **DEAD(heat_transfer)** + implicit | 1 | S | idem |
| TEST-057 | `...: TestPatchTestHTMLinear.Implicit` | `patch_test_linear_heat_transfer_implicit.cc` | gtest | implicit dynamic | idem | 12 types | **DEAD(heat_transfer)** + implicit | 1 | S | idem |
| TEST-058 | `test_linear_elastic_explicit_python` | `test_patch_linear_elastic_explicit.py`, `patch_test_linear_fixture.py`, `patch_test_linear_solid_mechanics_fixture.py` | py-script (unittest-style, invoked directly) | Python port of TEST-050 (plane strain only, `plane_strain = True`): 100 explicit steps, `ekin < 1e-16`, DOFs 1e-15, gradient 1e-13, stress 1e-13 (tighter than C++!) | patch meshes, `material_check_stress_plane_strain.dat` | all `getElementTypes()` except `_point_1` | python_interface / CI | 1 | M | **Must run unchanged on Rust module**; uses `MeshUtils.buildFacets`, `createBoundaryGroupFromGeometry`, `iterateElementGroups`, views, `getMaterial(0).getReal("nu")`, `getGradU`, `getStress`. Note `FILES_TO_COPY ${PATCH_TEST_MESHES}` only on this test (others rely on the shared binary dir). |
| TEST-059 | `test_linear_elastic_static_python` | `test_patch_linear_elastic_static.py` | py-script | Static version: NR `max_iterations 2, threshold 2e-4, residual`; checks as above | idem | idem | python_interface + implicit / CI | 1 | M | Fixture probes `akantu.SparseSolverPETSc` (A.9). |
| TEST-060 | `test_linear_anisotropic_explicit_python` | `test_patch_linear_anisotropic_explicit.py` | py-script | Intended anisotropic Python patch test — **broken** (`strain(k, l)` call on ndarray, undefined `checkDisplacements`/`checkStrains`, file `material_anisotropic.dat` vs copied `material_anisotropic_3.dat`) | `data/material_anisotropic_3.dat` | — | python_interface / **UNSTABLE** | 1 | — | Do not expect it to pass; port intent via TEST-052. |
| TEST-061 | `patch_test_linear_heat_transfer_explicit_python` | `test_patch_linear_heat_transfer_explicit.py`, `patch_test_linear_heat_transfer_fixture.py` | py-script | Heat patch test in Python (uses `HeatTransferModel`, `getConstitutiveLaw(0).getMatrix("conductivity")`, internals `"∇u"`, `"D∇u"`) | `heat_transfer_input.dat` | element types | **DEAD(heat_transfer)** | 1 | M | Would currently fail? (`DiffusionModel.getBlockedDOFs` copy, A.9 #11) — Q8. |
| TEST-062 | `patch_test_linear_heat_transfer_static_python` | `test_patch_linear_heat_transfer_static.py` | py-script | static | idem | idem | **DEAD(heat_transfer)** | 1 | M | idem |
| TEST-063 | `patch_test_linear_heat_transfer_implicit_python` | `test_patch_linear_heat_transfer_implicit.py` | py-script | implicit (label says "Explicit") | idem | idem | **DEAD(heat_transfer)** | 1 | M | idem |

#### C.2.7 `test/test_model/test_common` (DOF manager, model solver, non-local toolbox)

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-070 | `test_dof_manager_gtest: DOFManagerFixture.*` | `test_model/test_common/test_dof_manager.cc`, `mesh.geo` | gtest | `Construction, DoubleConstruction, RegisterGenericDOF1/2, RegisterNodalDOF1/2, RegisterMixedDOF, splitArrayPerDOFs, AssembleVector, AssembleMatrixNodal` — equation numbering, local/global sizes over ranks, assembly (1e-14) | `mesh.geo` → `mesh.msh` (3D o1) | typed over `{dof_manager_petsc_t (if PETSc), dof_manager_default_t}` | core / CI | **PARALLEL (N, N/2, …, 1)** | S | Contract for the Rust DOF manager incl. distributed numbering. |
| TEST-071 | `test_dof_manager_gtest: TestMesh.TestMeshDistribute` | `test_dof_mesh_distribute.cc` | gtest | `distribute()` succeeds on all ranks / throws when called twice (`EXPECT_THROW`/`NO_THROW`) | `mesh.msh` | — | parallel / CI | PARALLEL | S | — |
| TEST-072 | `test_dof_manager_default` | `test_model_solver/test_dof_manager_default.cc`, `test_dof_manager_default.verified` | exe | Custom `SolverCallback` (`MySolverCallback`: 3-DOF generic 1D truss, force F at the tip, node 0 blocked, K assembled by hand into `SparseMatrixAIJ`) solved through `DOFManagerDefault` + static time-step solver; prints displacements | — | — | **mumps** / CI | 1 | S | GOLD-001; independent check script `test_model_solver.py` (TEST-074 note). |
| TEST-073 | `test_model_solver_mumps` / `test_model_solver_petsc` | `test_model_solver.cc`, `test_model_solver_my_model.hh` (`-DDOF_MANAGER_TYPE="mumps"/"petsc"`), `.verified` ×2 | exe ×2 | `MyModel` (1D truss implemented as an akantu `Model`+`SolverCallback`) static solve; writes `disp.csv`? (checked offline by `test_model_solver.py`, not registered) | — | DOF manager type | mumps / petsc — CI | 1 | S | `test_model_solver_my_model.hh` (448 lines) is the reference "how to write a Model" — port as Rust test model. |
| TEST-074 | `test_model_solver_dynamic_explicit` / `_implicit` / `_petsc` | `test_model_solver_dynamic.cc` (`-DEXPLICIT=true/false`, `-DDOF_MANAGER_TYPE="petsc"`), `.verified` ×3 | exe ×3 | Dynamic truss (explicit central difference / implicit Newmark / PETSc) with `MyModel`, dumps mesh; print-only | — | explicit/implicit/petsc | core / mumps / petsc — CI | 1 | S | `test_model_solver_dynamic.py` is a matplotlib visualiser using `python_fe` (not a test). |
| TEST-075 | `test_non_local_neighborhood_base` | `test_non_local_toolbox/test_non_local_neighborhood_base.cc`, `my_model.hh`, `.verified`, `plot_neighborhoods.py` | exe | Builds a `NonLocalNeighborhoodBase` (pairs of quadrature points within radius) and writes pair lists; smoke (plot script for humans) | `material.dat`, committed `plate.msh` | — | damage_non_local / CI | 1 | S | GOLD-001. |
| TEST-076 | `test_weight_computation` | `test_weight_computation.cc`, `.verified` | exe | Non-local weight function values (base weight) written; smoke | `material_weight_computation.dat`, `plate.msh` | — | damage_non_local / CI | 1 | S | GOLD-001. |
| TEST-077 | `test_non_local_averaging` | `test_non_local_averaging.cc`, `test_material.{hh,cc}` | exe | Non-local average of a constant field must be constant: total norm error > 1e-12 → `AKANTU_EXCEPTION` | `material_avg.dat`, `plate.msh` | — | damage_non_local / CI | 1 | S | — |
| TEST-078 | `test_remove_damage_weight_function` | `test_remove_damage_weight_function.cc`, `test_material_damage.{hh,cc}`, `.verified` | exe | "remove damaged" weight function: averaged values with damaged points removed; `EXIT_FAILURE` if error > 1e-4 | `material_remove_damage.dat`, `plate.msh` | — | damage_non_local / CI | 1 | S | — |
| TEST-079 | `test_build_neighborhood_parallel_2` | `test_build_neighborhood_parallel.cc`, `test_material.{hh,cc}` | exe | Neighborhood construction across ranks (ghost quadrature points within radius); dumps ghost dumper; no explicit check | `material_parallel_test.dat`, committed `parallel_test.msh` | — | damage_non_local / CI | **PARALLEL_LEVEL 2** (only `_2`) | S | — |
| TEST-080 | `test_pair_computation` / `_2` | `test_pair_computation.cc` | exe | Pair lists identical in serial and parallel (distance checks), several `EXIT_FAILURE` | `material_remove_damage.dat`, committed `pair_test.msh` | — | damage_non_local / CI | **PARALLEL_LEVEL 1 2** | S | — |

#### C.2.8 `test/test_model/test_contact_mechanics_model`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-085 | `test_explicit_dynamic` | `test_contact_mechanics_model/test_explicit_dynamic.cc`, `flat_on_flat.geo` | exe | Flat-on-flat contact under pressure (explicit coupler), until steady; stress σ_yy in the bodies vs prescribed pressure, rel error ≤ 1e-2 else `EXIT_FAILURE` | `flat_on_flat.geo` (2D o1), `material.dat` | — | contact_mechanics / CI | 1 | M | — |
| TEST-086 | `test_explicit_friction` | `test_explicit_friction.cc`, `sliding-block-2D.geo` | exe | Sliding block with Coulomb friction: mean tangential traction vs μ·p (expected), fail if rel error > 1e-3 **and** numerical > expected | `sliding-block-2D.geo`, `material-friction.dat` | — | contact_mechanics / CI | 1 | M | One-sided check (only too-large traction fails). |

#### C.2.9 `test/test_model/test_phase_field_model`

All are single-executable, mostly one-element verifications against analytical
solutions; each writes `data.csv` (output, not golden).

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-090 | `test_phasefield_selector` | `test_phase_field_model/test_phasefield_selector.cc` | exe | Phase-field selection by mesh data: elements 0/1/2 get phase fields `chocolate/chewing_gum/candy` | `phasefield_selector.dat`, committed `phasefield_selector.msh` | — | phase_field / CI | 1 | S | Mirrors `test_material_selector`. |
| TEST-091 | `test_phase_solid_coupling` | `test_phase_solid_coupling.cc` | exe | One element, staggered coupling, uniaxial loading; damage, stress, energy vs analytical (tol 1e-8 / 1e-4) | `material_coupling.dat`, `test_one_element.msh` | — | phase_field / CI | 1 | S | — |
| TEST-092 | `test_phase_field_anisotropic` | `test_phase_field_anisotropic.cc` | exe | Anisotropic (tension/compression split) phase field, hybrid formulation; damage & stress vs analytical | `material_hybrid.dat`, `material_penalization.dat`, `test_one_element.msh` | — | phase_field / CI | 1 | S | — |
| TEST-093 | `test_phase_field_deviatoric_split` | `test_phase_field_deviatoric_split.cc` | exe | Volumetric/deviatoric energy split; analytical comparison | `material_deviatoric.dat`, `test_one_element.msh` | — | phase_field / CI | 1 | S | — |
| TEST-094 | `test_phase_field_optimal` | `test_phase_field_optimal.cc` | exe | 1D optimal damage profile d(x)=exp(−\|x\|/l) vs nodal damage (1e-6/1e-8) | `material_optimal.dat`, committed `test_optimal_damage.msh` | — | phase_field / CI | 1 | S | Analytical oracle. |
| TEST-095 | `test_phase_field_optimal_linear` | `test_phase_field_optimal_linear.cc` | exe | Optimal profile for AT1/linear model (bound-constrained) | `material_optimal_linear.dat`, `test_optimal_damage.msh` | — | phase_field + **petsc** (TAO) / CI | 1 | S | Requires a bound-constrained solver (PETSc TAO) — Rust needs an equivalent (Q11). |
| TEST-096 | `test_phase_solid_explicit` | `test_phase_solid_explicit.cc` | exe | Explicit solid + phase field one-element | `material_coupling.dat`, `test_one_element.msh` | — | phase_field / CI | 1 | S | writes `data-explicit.csv`. |
| TEST-097 | `test_multi_material` | `test_multi_material.cc` | exe | Two elements with different phase fields | `material_multiple.dat`, `test_two_element.msh` | — | phase_field / **UNSTABLE** | 1 | S | — |
| TEST-098 | `test_penalization` | `test_penalization.cc` | exe | Irreversibility by penalisation, 4 elements; damage & stress vs analytical | `material_penalization.dat`, `test_four_elements.msh` | — | phase_field / CI | 1 | S | — |
| TEST-099 | `test_phase_field_linear` | `test_phase_field_linear.cc` | exe | AT1 ("linear") model, one element; stress/energy | `material_linear.dat`, `test_one_element.msh` | — | phase_field + **petsc** / CI | 1 | S | — |
| TEST-100 | `test_phase_field_tao` | `test_phase_field_tao.cc` | exe | Same with PETSc TAO solver configured in `.dat` (`non_linear_solver petsc_tao`) | `material_linear_tao.dat`, `test_one_element.msh` | — | phase_field + **petsc** / CI | 1 | S | — |

#### C.2.10 `test/test_model/test_solid_mechanics_model`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-105 | `test_solid_mechanics_model_reassign_material` (+`_N` variants) | `test_solid_mechanics_model_reassign_material.cc`, `cube_two_materials.geo` | exe | Custom `MaterialSelector` reassigns elements between two materials after init (`reassignMaterial()`), verifies selector condition; `EXIT_FAILURE`/`AKANTU_ERROR` | `cube_two_materials.geo` (3D o1), `two_materials.dat` | — | parallel + implicit / CI | **PARALLEL (N…1)** | S | Element migration between materials incl. internals. |
| TEST-106 | `test_solid_mechanics_model_material_eigenstrain` | `test_solid_mechanics_model_material_eigenstrain.cc` | exe | Imposed eigen-gradient-u; static solve; stress vs prescribed (1e-13 style) | committed `cube_3d_tet_4.msh`, `material_elastic_plane_strain.dat` | — | implicit / CI | 1 | S | — |
| TEST-107 | `test_solid_mechanics_model_material_large_rotation` | `test_solid_mechanics_model_material_large_rotation.cc` | exe | Finite-deformation material under rigid large rotation | `cube_3d_tet_4.msh`, `material_elastic.dat` | — | implicit / **UNSTABLE** | 1 | S | — |
| TEST-108 | `test_material_selector` | `test_material_selector.cc` | exe | `MeshDataMaterialSelector` by physical names: elements 0/1/2 → `chocolate/chewing_gum/candy` | `material_selector.dat`, committed `material_selector.msh` | — | core / CI | 1 | S | — |
| TEST-109 | `test_solid_mechanics_model_gtest: Dynamics` | `test_solid_mechanics_model_dynamics.cc`, `test_solid_mechanics_model_fixture.hh`, `test_solid_mechanics_model_dynamics_material.dat` | gtest | `TestSMMFixtureBarExplicit.Dynamics`, `TestSMMFixtureBarImplicit.Dynamics`: 1D wave in a bar, analytical solution vs displacement/velocity (tolerances 1e-1 / 2e-3) | `bar_<type>.msh` (generated) | 12 element types × {explicit, implicit (if AKANTU_IMPLICIT)} | core / CI | **PARALLEL (N…1)** (needs CMake ≥3.31 for >1) | M | Fixture distributes the mesh. |

#### C.2.11 `.../test_solid_mechanics_model/test_cohesive`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-115 | `test_solid_mechanics_model_cohesive_gtest: TestSMMCFixture.*` | `test_cohesive/test_cohesive.cc`, `test_cohesive_fixture.hh`, `data/cohesive_*.geo` | gtest | `ExtrinsicModeI, ExtrinsicModeIFiniteDef, ExtrinsicModeII, IntrinsicModeI, IntrinsicModeII`: straight crack, imposed strain (`(1−1e-5)·strain` then steps of `2e-2·strain`), checks all facets on the crack got cohesive elements (`EXPECT_EQ` count = group size) and dissipated energy = G_c × surface (`EXPECT_NEAR`, abs tol 5e-1) | generated `_cohesive_{1d_2_segment_2, 2d_4_triangle_3, 2d_6_triangle_6, 2d_4_quadrangle_4, 2d_6_quadrangle_8, 2d_4_triangle_3_quadrangle_4, 2d_6_triangle_6_quadrangle_8, 3d_6_tetrahedron_4, 3d_12_tetrahedron_10, 3d_8_hexahedron_8, 3d_16_hexahedron_20}.msh`; `material_0.dat`, `material_1.dat`, `material_0_finite_def.dat` | 9 (cohesive, bulk1, bulk2) combos (hex variants commented out) | cohesive_element / CI | **PARALLEL_LEVEL 1 2** | M | Main cohesive acceptance test. Hex meshes generated but unused. |
| TEST-116 | `test_cohesive_buildfragments` | `test_cohesive_buildfragments/test_cohesive_buildfragments.cc`, `mesh.geo` | exe | Bar broken in N fragments by velocity; `FragmentManager`: number of fragments, mass, center of mass, velocity vs theory (`Math::are_float_equal`) | `mesh.geo` (2D o1), `material.dat`, dir `paraview` | — | cohesive_element / CI | 1 | M | — |
| TEST-117 | `test_cohesive_insertion_along_physical_surfaces` | `test_cohesive_insertion/test_cohesive_insertion_along_physical_surfaces.cc`, `3d_spherical_inclusion.geo` | exe | Intrinsic insertion restricted to physical surfaces listed in input; counts inserted vs expected facets (`return 1`) | `3d_spherical_inclusion.geo` (3D **o2**), `input_file.dat` | — | cohesive_element / CI | 1 | M | 3D order-2 mesh generation cost. |
| TEST-118 | `test_cohesive_group_update` | `test_group_update.cc`, `bar.geo` | exe | Element groups updated when cohesive elements are inserted (new nodes/elements added to groups); prints | `bar.geo` (2D o1), `material.dat` | — | cohesive_element / CI | 1 | S | — |
| TEST-119 | `test_material_cohesive_gtest: TestMaterialCohesiveLinearFixture.*` | `test_materials/test_material_cohesive_linear.cc`, `test_material_cohesive_fixture.hh` | gtest | `ModeI, ModeII, Cycles`: constitutive point tests of `MaterialCohesiveLinear` (traction–opening law, unloading/reloading), analytic energies (1e-6 … 1e-16) | — (no mesh: material driven directly) | dim 1,2,3 | cohesive_element / CI | 1 | S | Pure constitutive test — easiest cohesive port. |

#### C.2.12 `.../test_embedded_interface`, `.../test_energies`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-125 | `test_embedded_element_matrix` | `test_embedded_interface/test_embedded_element_matrix.cc` | exe | One embedded bar in one triangle: interface mesh has 1 element of dim 2 space; checks stiffness entries (1e-8) | committed `triangle.msh`, `embedded_element.dat` | — | embedded + implicit / INACTIVE(CGAL) | 1 | S | — |
| TEST-126 | `test_embedded_interface_model` | `test_embedded_interface_model.cc`, **`test_embedded_interface_model.sh`**, `matrix` (expected stiffness) | exe+sh | `.sh` runs the binary **and** `diff matrix matrix_test` (but the diff output is discarded `>/dev/null`; exit status of `&&` chain is the diff status → real golden comparison); binary checks `\|E_pot − 7.37343e-06\| ≤ 1e-5` (meaningless: tol > value) | committed `embedded_mesh.msh`, `material.dat`, `matrix` (GOLD-008) | — | embedded + implicit / INACTIVE | 1 | S | The only executable test whose golden file is actually compared (via its own .sh). |
| TEST-127 | `test_embedded_interface_model_prestress` | `test_embedded_interface_model_prestress.cc` | exe | Prestressed reinforcement; relative error of a reaction vs analytical ≤ 1e-3 | committed `embedded_mesh_prestress{,_reinforcement}.msh`, `prestress.dat` | — | embedded + implicit / INACTIVE | 1 | S | — |
| TEST-128 | `test_solid_mechanics_model_energies_gtest: LinearElasticPotentialEnergy` | `test_energies/test_solid_mechanics_model_linear_elastic_potential_energy.cc` | gtest | Potential energy of an imposed linear displacement vs ½∫σ:ε analytic (1e-8, 1e-6) | `test_solid_mechanics_model_linear_elastic_potential_energy_material.dat`, patch meshes | 12 types (`TestSMMFixture`) | implicit / CI | **PARALLEL_LEVEL 1 2** | S | Fixture (`test_solid_mechanics_model_fixture.hh:47-80`) distributes the mesh and sets PETSc `pc_type cholesky`, `ksp_rtol 1e-30` when PETSc is the sparse solver. |
| TEST-129 | `...: KineticEnergyImplicit/Explicit` | `test_solid_mechanics_model_kinetic_energy.cc` | gtest | Kinetic energy of a uniform velocity field = ½ m v² (1e-8) | `..._kinetic_energy_material.dat` | 12 types | implicit / CI | 1, 2 | S | — |
| TEST-130 | `...: WorkQuasistatic` | `test_solid_mechanics_model_work_quasistatic.cc` | gtest | External work = potential energy in quasi-static loading (1e-2 rel?, 1e-6, 1e-12) | `test_solid_mechanics_model_work_material.dat` | 12 types | implicit / CI | 1, 2 | M | — |
| TEST-131 | `...: TestSMMFixtureBar.WorkImplicit`, `TestSMMFixtureWorkDynamic.WorkExplicit` | `test_solid_mechanics_model_work_dynamics.cc` | gtest | Energy balance E_kin + E_pot − W_ext ≈ 0 over a dynamic run (5e-2 rel, 1e-6) | `..._work_material.dat`, bar meshes | 12 types | core (implicit part if enabled) / CI | 1, 2 | M | — |

#### C.2.13 `.../test_materials`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-135 | `test_local_material` | `test_materials/test_local_material.cc`, `local_material_damage.{cc,hh}`, `local_material_damage_inline_impl.hh`, `mesh_section_gap.geo` | exe | User-defined local damage material (same as EX-023) on a notched bar under traction; checks damage localisation (`std::terminate` on failure; threshold 5e7) | `mesh_section_gap.geo` (2D **o2**), `material.dat`, dir `paraview` | — | core / CI | 1 | M | Plug-in material API. |
| TEST-136 | `test_interpolate_stress` | `test_interpolate_stress.cc`, `interpolation.geo` | exe | `interpolateElementalFieldFromIntegrationPoints`/stress interpolation at arbitrary points vs analytic linear field (tolerance check, `EXIT_FAILURE`) | `interpolation.geo` (3D **o2**), `material_interpolate.dat` | — | core / CI | 1 | M | — |
| TEST-137 | `test_material_orthotropic` | `test_material_orthotropic.cc`, `square.geo` | exe | Orthotropic elastic plate, explicit run, writes `energy.csv`; smoke | `square.geo`, `orthotropic.dat` | — | core / CI | 1 | M | Add analytic check in Rust. |
| TEST-138 | `test_material_mazars` | `test_material_mazars.cc` | exe | Mazars damage on one element under compression/tension cycles; checks peak stress ≈ 6.24e6 (±1e5) and softening; writes CSV | `material_mazars.dat` | — | core / **UNSTABLE** | 1 | S | — |
| TEST-139 | `test_multi_material_elastic` | `test_multi_material_elastic.cc`, `test_multi_material_elastic.geo` | exe | Two elastic materials in series under static load; stress/strain per material vs analytic (1e-10/1e-8) | generated mesh, `.dat` | — | implicit / CI | 1 | S | — |
| TEST-140 | `test_material_gtest: TestFiniteDeformation.NotUnit` | `test_finite_deformation.cc`, `cube.geo` | gtest | Finite-deformation elastic material on one tetrahedron under a non-unit deformation; stress (1e-14/1e-5) | committed `1_tetrahedron.msh`, `cube.geo`, `material_finite_deformation.dat` | — | implicit / CI | 1 | S | — |
| TEST-141 | `test_material_gtest: TestElasticMaterialFixture.*` | `test_elastic_materials.cc`, `test_material_fixtures.hh` | gtest | `ComputeStress, EnergyDensity, ComputeTangentModuli, ComputeCelerity` with hand-computed references (1e-13/1e-14) | — | `MaterialElastic` 1/2/3D, `MaterialElasticOrthotropic` 2/3D, `MaterialElasticLinearAnisotropic` 2/3D | core / CI | 1 | S | Constitutive unit tests (895 lines of reference values). |
| TEST-142 | `test_material_gtest: TestFiniteDefMaterialFixture.*` | `test_finite_def_materials.cc` | gtest | all four cases `DISABLED_` (Neo-Hookean 1/2/3D; bodies `AKANTU_TO_IMPLEMENT`) | — | Neo-Hookean 1-3D | core / CI (disabled) | 1 | — | Placeholder; implement real tests in Rust. |
| TEST-143 | `test_material_gtest: TestDamageMaterialFixture.ComputeStress` | `test_damage_materials.cc`, `py_mazars.py` | gtest (+embedded Python) | Mazars (with elastic) stress & damage along a strain path vs **Python reference implementation** `py_mazars.Mazars.compute_step` (1e-10…1e-5); other cases `DISABLED_` | — | Mazars 1/2/3D | core + python_interface / CI | 1 | S | `py_mazars.py` = independent oracle. |
| TEST-144 | `test_material_gtest: TestPlasticMaterialFixture.ComputeStress` | `test_plastic_materials.cc` | gtest | Linear isotropic hardening 3D: loading/unloading stress path vs analytic (1e-13); other cases disabled | — | `MaterialLinearIsotropicHardening<3>` | core / CI | 1 | S | — |
| TEST-145 | `test_material_gtest: TestMaterialThermalFixture.ThermalComputeStress` | `test_material_thermal.cc` | gtest | Thermal strain stress σ = −E α ΔT (1e-14) | — | `MaterialThermal` 1/2/3D | core / CI | 1 | S | Plane-stress thermal added 2025-03. |
| TEST-146 | `test_material_standard_linear_isotropic_hardening` | `test_material_elasto_plastic_linear_isotropic_hardening/test_material_elasto_plastic_linear_isotropic_hardening.cc`, `.geo`, `.dat`, `test_material_standard_linear_isotropic_hardening.verified` | exe | Elasto-plastic plate under cyclic displacement, prints stress–strain history; criteria by eye/`.verified` (1e-5 consts) | generated mesh, `.dat` | — | core + implicit / CI | 1 | S | GOLD-001 (never compared). |
| TEST-147 | `test_material_damage_non_local` | `test_material_non_local/test_material_damage_non_local.cc` | exe | Non-local damage on notched bar: damage must localise (`std::terminate` otherwise; threshold 5e8) | committed `mesh_section_gap.msh`, `material_damage_non_local.dat`, dir `paraview` | — | damage_non_local / CI | 1 | M | — |
| TEST-148 | `test_material_non_local` | `test_material_non_local.cc`, `custom_non_local_test_material.{cc,hh}`, `mesh.geo` | exe | Custom non-local material averaging a field; output check by print (unverified) | `mesh.geo` → `mesh.msh` (2D o1), `material.dat` | — | damage_non_local / CI | 1 | S | Uses `distribute()` when parallel. |
| TEST-149 | `test_material_standard_linear_solid_deviatoric_relaxation` | `test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation.cc`, `.geo` | exe | SLS viscoelastic shear relaxation vs analytic exponential (rel 1e-7) | generated mesh, `material_standard_linear_solid_deviatoric_relaxation.dat` | — | core / CI | 1 | M | — |
| TEST-150 | `test_material_standard_linear_solid_deviatoric_relaxation_tension` | `..._tension.cc` | exe | Same under tension (rel 1e-5) | same | — | core / CI | 1 | M | — |
| TEST-151 | `test_material_viscoelasti_maxwell_relaxation` (sic) | `test_material_viscoelastic_maxwell/test_material_viscoelastic_maxwell_relaxation.cc`, `.geo` | exe | Generalised Maxwell relaxation vs analytic (1e-6/1e-7) | generated mesh, `material_viscoelastic_maxwell.dat` | — | core / **UNSTABLE** | 1 | M | — |

#### C.2.14 Structural mechanics, Python interface, solvers, synchronizers

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-152 | `test_structural_mechanics_gtest: *` | `test_model/test_structural_mechanics_model/test_structural_mechanics_model_bernoulli_beam_2.cc`, `..._bernoulli_beam_3.cc`, `..._discrete_kirchhoff_triangle_18.cc`, `..._bernoulli_beam_dynamics.cc`, `test_structural_mechanics_model_fixture.hh` | gtest | `TestStructBernoulli2.TestDisplacements`, `TestStructBernoulli3Static.TestDisplacements`, `TestStructDKT18.TestDisplacements` (static beam/plate solutions vs analytic, 1e-12…), `TestStructBernoulliDynamic.{TestBeamMatrices, TestBeamOscilation}` (consistent mass/stiffness matrices vs closed form 1e-13; free oscillation frequency 1e-6/1e-8) | committed `_bernoulli_beam_2.msh`, `_bernoulli_beam_3.msh`, `_discrete_kirchhoff_triangle_18.msh` (`_miot_gmsh_struct`) | dynamics typed over B2, B3 | implicit + structural_mechanics / CI | 1 | S | — |
| TEST-153 | `test_python_interface` | `test/test_python_interface/test_pybind.py`, `test_common.cc` (module `py11_akantu_test_common`), `mesh_dcb_2d.geo`, `elastic.dat` | pytest (script calls `pytest.main(sys.argv)`) | `test_array_size/nocopy/copy`, `test_modify_array`, `test_vector_*`, `test_matrix_*` (zero-copy proof via raw pointers, 1e-15), `test_multiple_init` (create/delete model twice), `test_boundary_condition_functors` (Python `DirichletFunctor`/`NeumannFunctor` subclasses; blocked DOFs set; external force > 0), `test_mesh_interface` (`getNbElement(element_type=_quadrangle_8) == getNbElement(spatial_dimension=2)`) | `mesh_dcb_2d.geo` (2D **o2** → `_quadrangle_8`?), `elastic.dat` | — | python_interface / CI | 1 | S | Requires a **C++ helper extension module**; Rust equivalent: a small test-only `pyo3` module exposing `createArray/getArray/copyArray/getRawPointer*` for Array/Vector/Matrix. |
| TEST-155 | `test_sparse_matrix_profile` | `test/test_solver/test_sparse_matrix_profile.cc`, `triangle.geo` | exe | Builds AIJ profile from mesh connectivity, prints/saves profile | `triangle.geo` (2D o1) | — | implicit / CI | 1 | S | — |
| TEST-156 | `test_sparse_matrix_assemble` | `test_sparse_matrix_assemble.cc` | exe | Assembles element matrices into AIJ; smoke | `triangle.msh` | — | implicit / CI | 1 | S | — |
| TEST-157 | `test_sparse_matrix_product` | `test_sparse_matrix_product.cc`, `test_sparse_matrix_product.verified` | exe | Sparse matrix–vector product on a distributed bar; prints result | committed `bar.msh` | — | implicit / CI | 1 | S | GOLD-001. |
| TEST-158 | `test_sparse_solver_mumps` (+`_N`) | `test_sparse_solver_mumps.cc` | exe | 1D Laplacian-like system solved by MUMPS on distributed DOFs; residual check ≤ 1e-15 else `AKANTU_EXCEPTION` | — (mesh built in code) | — | mumps / CI | **PARALLEL (N…1)** | S | — |
| TEST-159 | `test_sparse_solver_eigen` (+`_N`) | `test_sparse_solver_eigen.cc` | exe | Same with `SparseSolverEigen` | — | — | **mumps** (sic) / CI | PARALLEL (N…1) — Eigen solver is sequential; behaviour on >1 rank? (Q13) | S | Package label probably a copy-paste bug. |
| TEST-160 | `test_sparse_solver_petsc` (+`_N`) | `test_sparse_solver_petsc.cc` | exe | Same with PETSc KSP | — | — | petsc / CI | PARALLEL (N…1) | S | — |
| TEST-165 | `test_dof_synchronizer` (+`_N`) | `test/test_synchronizer/test_dof_synchronizer.cc`, `test_data_accessor.hh` | exe | DOF synchronisation (gather/scatter of DOF arrays across ranks) on a distributed bar; `exit()` on mismatch | committed `bar.msh` | — | parallel / CI | PARALLEL (N…1) | S | — |
| TEST-166 | `test_synchronizers_gtest: Communicator.*` | `test_communicator.cc` | gtest | `Bcast`, `ReceiveAny` (any-source receives) | — | — | parallel / CI | PARALLEL (N…1) | S | MPI wrapper contract. |
| TEST-167 | `...: TestElementSynchronizerFixture.*` | `test_synchronizer_communication.cc`, `test_data_accessor.hh`, `test_synchronizers_fixture.hh`, `cube.geo` | gtest | `SynchroneOnce, Synchrone, Asynchrone` element-data ghost synchronisation (barycenters, 1e-15) | `cube.geo` (3D **o2**) | — | parallel / CI | PARALLEL | S | — |
| TEST-168 | `...: TestNodeSynchronizerFixture.*` | `test_node_synchronizer.cc` | gtest | `SynchroneOnce, Synchrone, Asynchrone, Gather, Scatter` for node data | `cube.msh` | — | parallel / CI | PARALLEL | S | — |
| TEST-169 | `...: TestSynchronizerFixture.DataDistribution*` | `test_data_distribution.cc` | gtest | `DataDistribution`, `DataDistributionTags`: mesh data (incl. tags/strings) distributed with the mesh (1e-7) | `cube.msh` | — | parallel / CI | PARALLEL | S | — |
| TEST-170 | `...: TestFacetSynchronizerFixture.*` | `test_facet_synchronizer.cc`, `facet.geo` | gtest | `SynchroneOnce, Synchrone, Asynchrone` facet ghost synchronisation (cohesive) | `facet.geo` (3D **o2**) | — | parallel + cohesive_element / CI | PARALLEL | S | — |

#### C.2.15 `extra_packages/*/test`

| ID | Name | Original files | Kind | Behavior; pass/fail | Inputs | Param. | Packages / status | Parallel | Cost | Porting notes |
|---|---|---|---|---|---|---|---|---|---|---|
| TEST-175 | `test_ntn_model_gtest: TestNTNFixture.*` | `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_basics.cc`, `test_ntn_fixture.hh` | gtest | `TestNodesPairing` (slave/master pairs across a split interface), `TestLumpedBoundary` (lumped boundary areas), `TestImpedance` (1e-10/1e-11) | committed `ntn_test_2d.msh`, `ntn_test_3d.msh`, `data/material_ntn_model.dat` | dim 2, 3 | core (folder only added if traction_at_split_node_contact active) / CI | 1 (fixture distribute-aware) | S | Added to CI 2026-03. |
| TEST-176 | `test_ntn_friction_gtest: TestNTNFrictionFixture.TestCompression` | `test_ntn_friction.cc`, `test_ntn_friction_fixture.hh` | gtest | Pure compression with slip-weakening law; `ASSERT_NEAR` on velocities/tractions (1e-10, 5e-9) | committed `ntn_test_compression.msh`, `data/material_ntn_friction_slip_weakening.dat` | — | core / CI | 1 | M | `test_ntn_compression.verified` (GOLD-001) is not linked to any test name. |
| TEST-177 | `test_ntn_friction_gtest: MaterialFileTests/ParamTestNTNFrictionFixture.TestRateAndState/*` | `test_ntn_friction.cc` | gtest (value-parameterised) | Steady sliding with rate-and-state laws; friction coefficient / slip velocity vs analytical steady state (5e-4, 5e-5) | generated `ntn_test_ras.msh` (`ntn_test_ras.geo`), 12 `data/material_ntn_friction_rate_and_state_*.dat` | 12 laws: dieterich {ageing, ageing_regularized, slip}, revised_dieterich {…}, regularized_dieterich {…}, n_shaped {…}; revised_n_shaped ×3 commented out ("not passing") | core / CI | 1 | L | 3 committed `.dat` files are unused (revised n-shaped). |
| TEST-180 | `test_material_FE2` | `extra_packages/extra-materials/test/test_material_FE2/test_material_FE2.cc` | exe | FE² material (RVE per quadrature point, `SolidMechanicsModelRVE`) on a macro square, distributed; smoke | `material.dat`, committed `periodic_plate.msh`, `square.msh` | — | extra_materials / INACTIVE | 1 (code is MPI-aware) | L | Uses many internals; feasibility question Q3. |
| TEST-181 | `test_eigenstrain_homogenization` | `test_eigenstrain_homogenization.cc` | exe | RVE homogenisation of eigenstrain; `EXIT_FAILURE` on mismatch | `mesoscale_materials.dat`, committed `one_inclusion.msh` | — | extra_materials / INACTIVE | 1 | M | — |
| TEST-182 | `test_elastic_homogenization` | `test_elastic_homogenization.cc`, `homogenized_plate.geo` | exe | Effective elastic tensor of RVE vs orthotropic reference | generated `homogenized_plate.msh`, `material_orthotropic.dat` | — | implicit + extra_materials / INACTIVE | 1 | M | — |
| TEST-183 | `test_node_selection` | `test_node_selection.cc`, `periodic_plate.geo` | exe | Corner/boundary node selection for RVE BCs | generated `periodic_plate.msh` (also a committed one exists), `material_test_boundary.dat` | — | implicit + extra_materials / INACTIVE | 1 | S | — |
| TEST-184 | `test_material_damage_iterative` | `test_material_damage/test_material_damage_iterative.cc` | exe | Iterative (event-driven) damage: one element at a time reaches threshold, stiffness reduced; checks (1e-13) | `material.dat`, committed `plate.msh` | — | extra_materials + implicit / INACTIVE | 1 | M | — |
| TEST-185 | `test_material_damage_iterative_parallel` | same source, **`test_material_damage_iterative_parallel.sh`** | exe+sh | `.sh` runs `mpirun -np 2 ./test_material_damage_iterative` (hard-coded ranks) | idem | — | extra_materials + implicit / INACTIVE | 2 (script) | M | The `.sh` refers to a binary built by TEST-184's target name. |
| TEST-186 | `test_material_damage_iterative_non_local_serial` | `test_material_damage_iterative_non_local_serial.cc` | exe | Non-local iterative damage, serial (1e-13, 1e-4) | `material_non_local.dat`, `plate.msh` | — | extra_materials_non_local + implicit / INACTIVE | 1 | M | — |
| TEST-187 | `test_material_damage_iterative_non_local_parallel` | `..._non_local_parallel.cc`, **`.sh`** | exe+sh | `.sh` runs serial **and** `mpirun -np 4` | `two_materials.dat`, committed `one_circular_inclusion.msh` | — | extra_materials_non_local + implicit / INACTIVE | 1 and 4 (script) | M | — |
| TEST-188 | `test_material_iterative_stiffness_reduction` | `test_material_iterative_stiffness_reduction.cc`, `.verified` | exe | Stiffness-reduction damage on two elements; prints history | `material_stiffness_reduction.dat`, committed `two_elements.msh` | — | extra_materials / INACTIVE | 1 | S | GOLD-001. |
| TEST-190 | `test_igfem_triangle_5` | `extra_packages/igfem/test/patch_tests/test_igfem_triangle_5.cc`, `plate.geo` | exe | IGFEM patch test with `_igfem_triangle_5` (1e-12, 1e-6) | generated `plate.msh`, `material.dat` | — | IGFEM (→CGAL) / INACTIVE | 1 | S | IGFEM element classes (R1/R2). |
| TEST-191 | `test_igfem_triangle_4` | `test_igfem_triangle_4.cc` | exe | idem `_igfem_triangle_4` | committed `test_mesh.msh`, `material.dat` | — | IGFEM / INACTIVE | 1 | S | — |
| TEST-192 | `test_interface_position` | `test_interface_position.cc`, **`test_interface_position.sh`** | exe+sh | interface location sweep; `.sh` = `mpirun -np 5` | `material_test_interface_position.dat`, committed `test_interface_position.msh` | — | IGFEM / INACTIVE | 5 (script) | M | — |
| TEST-193 | `test_interpolate` (IGFEM) | `test_fe_engine/test_interpolate.cc`, `igfem_mesh_generation.cc`, `.verified` | exe | IGFEM interpolation (3e-13) on generated meshes | `mesh_info_igfem_triangle_{4,5}.txt` | — | IGFEM / INACTIVE | 1 | S | Name clash with `test/test_fe_engine/test_interpolate.cc` (different file). |
| TEST-194 | `test_integrate` (IGFEM) | `test_integrate.cc`, `igfem_mesh_generation.cc`, `.verified` | exe | integration (3e-13) | idem | — | IGFEM / INACTIVE | 1 | S | — |
| TEST-195 | `test_fe_engine_precomputation` (IGFEM) | `test_fe_engine_precomputation.cc`, `.verified` | exe | prints precomputed shapes | idem | — | IGFEM / INACTIVE | 1 | S | — |
| TEST-196 | `test_gradient` (IGFEM) | `test_gradient.cc`, `.verified` | exe | gradient (3e-13) | idem | — | IGFEM / INACTIVE | 1 | S | — |
| TEST-197 | `test_spherical_kernel` | `test_geometry/test_spherical_kernel.cc` | exe | CGAL spherical kernel smoke | — | — | CGAL / INACTIVE | 1 | S | — |
| TEST-198 | `test_generation_igfem_mesh_from_sphere_static` | `test_generation_igfem_mesh_from_sphere_static.cc` | exe | IGFEM mesh from spherical inclusion (1e-10) | committed `test_geometry_triangle.msh` | — | CGAL + IGFEM / INACTIVE | 1 | S | — |
| TEST-199 | `test_generation_igfem_mesh_from_sphere` | `test_generation_igfem_mesh_from_sphere.cc` | exe | moving inclusion (1e-12) | idem | — | CGAL + IGFEM / INACTIVE | 1 | S | — |
| TEST-200 | `test_solid_mechanics_model_igfem` | `test_solid_mechanics_model_igfem/test_solid_mechanics_model_igfem.cc`, **`.sh`**, `.verified` | exe+sh | `.sh` runs with PETSc options (`-ksp_type preonly -pc_type lu ... mumps`) | `material.dat`, committed `plate.msh` | — | IGFEM + implicit / INACTIVE | 1 | M | — |
| TEST-201 | `test_transfer_internals` | `test_transfer_internals.cc`, **`.sh`** | exe+sh | `.sh` = `mpirun -np 40` (!) | `material_damage.dat`, committed `fine_mesh.msh` | — | IGFEM / INACTIVE | 40 | L | Unrealistic rank count for CI. |
| TEST-202 | `test_eigenstrain` (IGFEM) | `test_eigenstrain.cc` | exe | eigenstrain in IGFEM | `material_damage.dat`, `fine_mesh.msh` | — | IGFEM + extra_materials / INACTIVE | 1 | M | — |
| TEST-203 | `test_igfem_element_orientation` / `_parallel` | `test_igfem_element_orientation.cc`, **`.sh`** ×2 | exe+sh ×2 | orientation of IGFEM sub-elements; parallel `.sh` = `mpirun -np 10` | `material.dat`, committed `test_igfem_element_orientation.msh` | — | IGFEM / INACTIVE | 1 / 10 | M | — |
| TEST-204 | `test_ASR_damge` (sic) | `test_ASR_damage.cc` | exe | Alkali-silica reaction damage with IGFEM inclusion | `material_ASR.dat`, committed `one_inclusion.msh` | — | IGFEM + extra_materials / INACTIVE | 1 | M | — |
| TEST-205 | `test_material_igfem_iterative_stiffness_reduction` (+`_damage_step_transfer`, `_parallel`, `_tangent_transfer`), `test_volume_computation` | 5 sources (+ `.sh` for the first: runs `standard_fem` and `igfem` variants) | exe | iterative stiffness reduction within IGFEM; volume computation | `material_stiffness_reduction{,_2,_weibull}.dat`, committed `regular_mesh.msh`, `igfem_mesh.msh`, `test_damage_transfer.msh` | — | IGFEM + implicit + extra_materials / INACTIVE | 1 | M | 5 registrations in one row. |

### C.3 Test sources that are NOT registered (dead / orphaned) and test helpers

These are not run upstream. They are kept in the catalogue because the
mission requires porting every relevant test *intent*; each row says whether
the intent is worth porting.

| ID | Name | Original files | Behavior / intent | Why not run | Required packages | Parallel aspects | Porting notes |
|---|---|---|---|---|---|---|---|
| TEST-210 | `test_interpolate` (core) | `test/test_fe_engine/test_interpolate.cc` | `TestFEMFixture.{InterpolateConstant, InterpolatePosition}` (1e-14) on 12 types | Not listed in `register_gtest_sources` of `test/test_fe_engine/CMakeLists.txt` | core | 1 | **Port** (cheap, valuable interpolation check). |
| TEST-211 | `test_interpolate_bernoulli_beam_2` | `test_fe_engine/test_interpolate_bernoulli_beam_2.cc`, `.verified` (empty, 0 lines) | Interpolation on beam elements, writes `out.txt` | `register_fem_test` function defined but never called | structural_mechanics | 1 | Low value; covered by TEST-015/152. |
| TEST-212 | `test_fe_engine_precomputation_bernoulli_2/3` | `test_fe_engine_precomputation_bernoulli_2.cc`, `_bernoulli_3.cc` | Old-style precomputation checks for beams (`return 1`) | unregistered | structural_mechanics | 1 | Superseded by TEST-015. |
| TEST-213 | `generate_test` | `test/test_fe_engine/generate_test` | Bash generator of `test_integrate<type>.cc` from a non-existent template `test_integrate_XXXX.cc` | obsolete | — | — | Ignore. |
| TEST-214 | `test_segment_nodetype` | `test/test_mesh_utils/test_segment_nodetype/test_segment_nodetype.cc`, `.sh` (`mpirun -np 8`), `mesh.geo` | Node types (master/slave/pure ghost) of segment meshes after distribution | registration commented out | parallel | 8 | Port intent as a parallel node-type test (R3/MPI). |
| TEST-215 | `test_contact_coupling` | `test/test_model/test_contact_mechanics_model/test_coupler/test_contact_coupling.cc`, `coupling.geo`, `material.dat` | `CouplerSolidContact` smoke | `add_akantu_test(test_coupler)` missing | contact_mechanics, model_couplers | 1 | Covered by TEST-085/086. |
| TEST-216 | `test_coupled_stiffness` | `test_coupler/test_coupled_stiffness.cc`, `flat_on_flat.geo`, `material_stiffness.dat` | Coupled (solid+contact) tangent stiffness vs finite differences (1e-2) | same | contact_mechanics | 1 | **Port** — implicit contact tangent check. |
| TEST-217 | `test_detection` | `test_detector/test_detection.cc`, `test_detection_fixture.hh`, `node.geo`, `options.dat`, `data/*.geo`, `data/cohesive_strait_2D.msh`, `data/test.msh` | `TestCMMDFixture.{Implicit, Explicit}` contact detection on straight interfaces for (seg,seg), (tri,tri), (tri,quad), (quad,quad), (tet,tet) | directory not added; **fixture does not compile** (typos `_element_type_segement_2`, `_quadrange_4`, `_tetrahedron_6`); `register_gtest_test` missing | contact_mechanics | 1 | **Port intent** (detection is central to contact). |
| TEST-218 | `test_explicit_resolution`, `test_resolution` | `test_resolution/test_explicit_resolution.cc`, `test_resolution.cc`, `explicit_2d.geo`, `options.dat` | Resolution (penalty) smoke; typed `TestResolutionFixture.{TestComputeN, TestComputeDalpha (empty), TestComputeNalpha (empty)}` | dir not added; gtest source never registered | contact_mechanics | 1 | Port `TestComputeN` intent only. |
| TEST-219 | `test_selection` | `test_selector/test_selection.cc`, `test_selection_fixture.hh`, `data/selection_{1D,2D,3D}.geo`, `material.dat` | `TestSurfaceSelectionFixture.PhysicalSurface` | `register_gtest_sources` without `register_gtest_test`; **does not compile** (`gtest_list_t<std::tuple<1,2,3>>`) | contact_mechanics | 1 | Port intent (physical surface selector). Note `add_mesh(detection_tet ...)` target name clash with the detector dir. |
| TEST-220 | `test_cohesive_1d_element` | `test_cohesive/test_cohesive_1d_element/test_cohesive_1d_element.cc`, `bar.geo`, `material.dat` | 1D cohesive element under tension (1e-7) | `add_akantu_test` commented out | cohesive_element | 1 | Covered by TEST-115 (cohesive_1d_2). |
| TEST-221 | `test_cohesive_extrinsic*` | `test_cohesive_extrinsic/{test_cohesive_extrinsic, _quadrangle, _tetrahedron, _fatigue}.cc`, `triangle.geo`, `quadrangle.geo`, `tetrahedron.geo`, committed `fatigue.msh`, `material.dat`, `material_fatigue.dat` | Extrinsic insertion in 2D tri/quad and 3D tet; fatigue cohesive law (writes `fatigue_edis.txt`; checks dissipated energy, 1e-13) | `add_akantu_test` commented out | cohesive_element | 1 | **Port fatigue intent** (no other fatigue test). |
| TEST-222 | `test_assembling_K_cohe_elements` | `test_cohesive_extrinsic_implicit/test_assembling_K_cohe_elements.cc`, `.sh`, committed `quadrangle.msh`, `material.dat`, `K_matrix_verified.dat` | Assembles K of cohesive elements, writes `K_matrix_test.dat`, compares to `K_matrix_verified.dat` (1e-13, `EXIT_FAILURE`) | commented out | cohesive_element, implicit | 1 | **Port** (CHANGELOG 5.0.4: "Assemble of the K matrix for cohesive elements was wrong since v5" → golden may be stale; regenerate from the oracle). |
| TEST-223 | `test_cohesive_intrinsic*` | `test_cohesive_intrinsic/{test_cohesive_intrinsic, _quadrangle, _tetrahedron, _tetrahedron_fragmentation}.cc`, `triangle.geo`, `quadrangle.geo`, `tetrahedron.geo`, `tetrahedron_full.geo`, `material.dat`, `material_tetrahedron.dat` | Intrinsic cohesive tests incl. energies `edis.txt`/`erev.txt` output, tet fragmentation | commented out | cohesive_element | 1 | Covered by TEST-115 except tet fragmentation. |
| TEST-224 | `test_cohesive_intrinsic_impl` | `test_cohesive_intrinsic_impl/test_cohesive_intrinsic_impl.cc`, committed `implicit.msh`, `material.dat` | Intrinsic cohesive with implicit static solver (1e-3/1e-5) | commented out | cohesive_element, implicit | 1 | **Port** (only implicit cohesive test). |
| TEST-225 | `test_cohesive_friction` | `test_cohesive_linear_friction/test_cohesive_friction.cc`, committed `mesh_cohesive_friction.msh`, `material.dat` | `MaterialCohesiveLinearFriction` behaviour | registration commented out | cohesive_element | 1 | **Port** (only test of the friction cohesive law, which is exposed in Python, PY-078). |
| TEST-226 | Parallel cohesive suite | `test_parallel_cohesive/**` — `test_cohesive_facet_stress_synchronizer.cc` (+`.sh` np 8, `tetrahedron.geo`), `test_cohesive_parallel_buildfragments.cc` (+`.sh` np 8, `mesh.geo`), `test_cohesive_parallel_extrinsic{,_tetrahedron,_tetrahedron_displacement}.cc` (+`.sh` np 8), `test_cohesive_parallel_extrinsic_IG_TG.cc` (+`.sh` np 8, committed `square.msh`), `test_cohesive_parallel_insertion_along_physical_surfaces.cc` (+`.sh` np 8), `test_cohesive_parallel_intrinsic_implicit_insertion.cc` (+`verify_insertion.sh`, `output_dir_verified/*.out`, `2d_basic_interface.msh`, `input_file_iii.dat`, PARALLEL_LEVEL "2 4"), `test_cohesive_parallel_intrinsic{,_tetrahedron}.cc` (+`.sh` np 3/8), `.dat` files | Parallel cohesive insertion (intrinsic/extrinsic), facet-stress synchronisation, parallel fragment building, IG/TG — the core HPC feature advertised in the JOSS paper | `#add_akantu_test(test_parallel_cohesive ...)` commented out; package `parallel_cohesive_element` does not exist either | parallel, cohesive_element | 2–8 ranks | **High-value intent** — parallel cohesive is a headline feature (DOC-019); port as MPI tests (2/4 ranks) with serial↔parallel equivalence checks. |
| TEST-227 | `test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5` | `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5.cc` | Kirchhoff shell patch test (MacNeal-Harder 4.5.5) on `b_beam_3_12_10_13.msh` (absent) | unregistered, mesh missing | structural_mechanics | 1 | Port intent if shells are in scope (R3). |
| TEST-228 | PETSc matrix/solver tests | `test/test_solver/test_petsc_matrix_{apply_boundary,diagonal,profile,profile_parallel}.cc`, `test_solver_petsc.cc`, scripts `*.sh` (np 2/4), `test_petsc_matrix_profile{,_parallel}.verified`, `1D_bar.geo` (`add_mesh(test_solver_petsc_mesh)` exists but no test uses it) | PETSc matrix profile/diagonal/BC application and KSP solve | not registered (old API) | petsc | 2–4 | Covered by TEST-160; ignore unless a PETSc backend is ported. |
| TEST-229 | `test_sparse_matrix_profile_parallel` | `test/test_solver/test_sparse_matrix_profile_parallel.cc` | Distributed AIJ profile | unregistered | implicit, parallel | N | Port intent with parallel sparse matrix. |
| TEST-230 | Grid synchronizer & DOF-synchronizer-communication | `test/test_synchronizer/test_grid_synchronizer.cc`, `test_grid_synchronizer_check_neighbors.cc`, `test_grid_tools.hh`, `test_dof_synchronizer_communication.cc`, `test_dof_data_accessor.hh` | Spatial-grid based ghost detection (used by non-local), DOF communication | unregistered | parallel | N | Port intent of grid synchronizer (needed by non-local in parallel, R3). |
| TEST-231 | `test_model_solver_dynamic_petsc.cc` (stand-alone) | `test/test_model/test_common/test_model_solver/test_model_solver_dynamic_petsc.cc` (828 lines) | Old self-contained PETSc dynamic solver test | unused (the `_petsc` ctest uses `test_model_solver_dynamic.cc`) | petsc | — | Ignore. |
| TEST-232 | Independent Python oracles (helpers, not tests) | `test/python_fe/{__init__,fe,static_solver,dynamic_solver,truss_fe}.py`, `test/test_model/test_common/test_model_solver/test_model_solver.py`, `test_model_solver_dynamic.py`, `test/test_fe_engine/py_engine/{__init__,py_engine}.py`, `test/test_model/test_solid_mechanics_model/test_materials/py_mazars.py`, `test/test_model/test_common/test_non_local_toolbox/plot_neighborhoods.py` | `python_fe`: minimal NumPy truss FE (`TrussFE`, `StaticSolver`, `DynamicSolver` Newmark, tol 1e-10) used to check `disp.csv` from TEST-073 (`test_model_solver.py`, rel 1e-14) and to animate TEST-074; `py_engine`: independent shape functions/quadrature for 12 types (TEST-014); `py_mazars`: Mazars reference (TEST-143); `plot_neighborhoods.py`: matplotlib visualisation | `test_model_solver*.py` never registered; others imported via `PYTHONPATH=test:test/test_fe_engine` | numpy, scipy, matplotlib, termcolor (optional) | — | **Reuse as independent oracles** in akantu-rs differential tests (pure Python/NumPy, no akantu dependency). |
| TEST-233 | Shared test infrastructure | `test/test_gtest_main.cc`, `test/test_gtest_utils.hh`, `test/CMakeLists.txt`, every `CMakeLists.txt` under `test/` (54) and `extra_packages/*/test` (9), fixtures `*_fixture.hh` | gtest main with akantu init and optional embedded interpreter; type-list helpers; directory registration | — | gtest | rank-0 printing | Rust: a common `akantu-test-utils` crate with element-type lists and an MPI-aware harness. |
| TEST-234 | CI helper headers | `test/ci/includes_for_ci/aka_config.hh`, `aka_element_classes_info.hh` | Pre-generated (stale, version 4.0.0) config headers for static analysis without configuring | — | — | — | Ignore. |

### C.4 Golden / reference data present in the tree (`GOLD-`)

None of the `.verified` files is compared by the upstream driver (finding 1);
they reflect the output at the time they were last committed and may be
**stale**. Treat as hints, regenerate with the oracle before use.

| ID | Files | Produced by / consumed by | Content | Status / porting notes |
|---|---|---|---|---|
| GOLD-001 | `.verified` of registered tests: `test/test_io/test_parser/test_parser.verified` (46 l., updated 2025-11-14), `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh_physical_names.verified` (32), `test/test_mesh_utils/test_buildfacets/test_buildfacets_gtest.verified` (1363, last changed 2022-01-26), `test/test_model/test_common/test_model_solver/{test_dof_manager_default (12), test_model_solver_mumps (101), test_model_solver_petsc (101), test_model_solver_dynamic_explicit (402), _implicit (402), _petsc (402)}.verified`, `test/test_model/test_common/test_non_local_toolbox/{test_non_local_neighborhood_base (1110), test_remove_damage_weight_function (584), test_weight_computation (322)}.verified`, `test/test_model/test_solid_mechanics_model/test_materials/test_material_elasto_plastic_linear_isotropic_hardening/test_material_standard_linear_isotropic_hardening.verified` (20), `test/test_solver/test_sparse_matrix_product.verified` (13), `extra_packages/extra-materials/test/test_material_damage/test_material_iterative_stiffness_reduction.verified` (44) | stdout of TEST-031, -038, -039, -072, -073, -074, -075, -078, -076, -146, -157, -188 | Plain text, compared upstream (when it worked) with `diff -biwI "([0-9]+ ms( total)?)"` (ignore case/whitespace, ignore timing lines) | Mesh-dependent ones (buildfacets, physical names, non-local) depend on gmsh node numbering of committed/generated meshes. |
| GOLD-002 | `test/test_fe_engine/_<type>.csv` ×12 | TEST-016 (inradius) | per-element reference inradius values | **Actively used** (read by the test). |
| GOLD-003 | `.verified` of unregistered tests: `test/test_fe_engine/test_interpolate_bernoulli_beam_2.verified` (empty), `test/test_solver/test_petsc_matrix_profile{,_parallel}.verified` | TEST-211, TEST-228 | — | Dead. |
| GOLD-004 | IGFEM `.verified`: `extra_packages/igfem/test/test_fe_engine/{test_fe_engine_precomputation, test_gradient, test_integrate, test_interpolate}.verified`, `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_solid_mechanics_model_igfem.verified` | TEST-193…196, TEST-200 | printed shapes/integrals | Inactive package. |
| GOLD-005 | `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_compression.verified` | not attached to any test name | compression history | Orphan; may serve as reference for TEST-176. |
| GOLD-006 | `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/K_matrix_verified.dat` | TEST-222 (unregistered) | dense/sparse K of cohesive elements | Likely stale (CHANGELOG 5.0.4 K-assembly fix). |
| GOLD-007 | `.../test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_{0,1}_out_of_2.out`, `output_from_proc_{0..3}_out_of_4.out` | TEST-226 (`verify_insertion.sh`) | per-rank inserted cohesive elements | Dead suite; useful for parallel-insertion regression intent. |
| GOLD-008 | `test/test_model/test_solid_mechanics_model/test_embedded_interface/matrix` (Matrix Market, symmetric 18×18, 91 nnz) | TEST-126 via its `.sh` | expected stiffness of embedded model | Actually compared (exact `diff`) when embedded is enabled. |
| GOLD-009 | Hard-coded reference values in test sources | e.g. anisotropic C tensor (`patch_test_linear_anisotropic_*.cc`, `test_patch_linear_anisotropic_explicit.py`), elastic/plastic/thermal material references (`test_elastic_materials.cc` 895 l.), Mazars peak 6.24e6 (`test_material_mazars.cc`), embedded energy 7.37343e-6, parser expected values, steady-state friction coefficient `fss = 0.323687421261617` (`ras_ss.in`) | — | Port with the same values; tolerances in §C.2. |
| GOLD-010 | Independent oracle code (TEST-232) | `py_engine.py`, `py_mazars.py`, `python_fe/*` | analytic/NumPy references | Reuse. |
| GOLD-011 | Analytical solutions embedded in tests/examples | phase-field optimal profiles (TEST-094/095), viscoelastic relaxation (TEST-149…151), 1D wave (TEST-109), beam formulas (TEST-152, EX-048), Hertz (EX-001 README) | — | Preferred oracles (no stale data). |
| GOLD-012 | Example-produced data without reference | `energy.csv` (EX-019), `position.csv` (EX-020), `energies.csv` (EX-018), `modes.txt` (EX-036), ParaView series | — | Candidates for new golden fixtures generated by the oracle build. |
| GOLD-013 | Benchmark timing data | `joss/results/timmings_{cohesive,elastic}_gcc_jed_v{4.0.1,5.0.4}.csv` | JOSS scaling study (EPFL "jed" cluster) | Performance reference only (DOC-019). |
| GOLD-014 | Committed input mesh info | `extra_packages/igfem/test/test_fe_engine/mesh_info_igfem_triangle_{4,5}.txt` | IGFEM mesh generation inputs | Inputs, not outputs. |
| GOLD-015 | Unused data files | 3 `material_ntn_friction_rate_and_state_revised_n_shaped_*.dat` (tests commented out), `examples/c++/diffusion_model/cube_tet.geo`, `test/test_solver/1D_bar.geo` (mesh target without consumer) | — | Keep for completeness. |

### C.5 Meshes (`MDATA-`) and Gmsh requirements

- **Counts**: 104 committed `.msh` files and 134 `.geo` files under `test/`,
  `extra_packages/*/test`, `examples/` (full per-file list in the Appendix).
- **Committed MSH versions** (all ASCII): MSH **2.1** ×12 (patch-test meshes,
  `cube_3d_tet_4.msh`, `implicit.msh`, `test_solver/bar.msh`,
  `test_synchronizer/bar.msh`), MSH **2.2** ×81, MSH **4.0** ×4
  (`detection-explicit.msh`, `stiffness_matrix/plate.msh`,
  `test_one_element.msh`, `test_two_element.msh`), MSH **4.1** ×7
  (`scalability_test/cube.msh`, `ntn_test_{2d,3d,compression}.msh`,
  `test_four_elements.msh`, `test_optimal_damage.msh`,
  `test_material_non_local/mesh_section_gap.msh`). ⇒ The Rust Gmsh reader must
  support **MSH 2.1, 2.2, 4.0 and 4.1 ASCII** (binary not needed by fixtures).
- **Generated meshes**: no `.geo` sets `Mesh.MshFileVersion`, so meshes produced
  by `add_mesh` are in the **default format of the installed gmsh** (MSH 4.1
  for gmsh ≥ 4.1). `.geo` features used: built-in kernel only (no OpenCASCADE,
  no `Field`, no `Mesh.Algorithm` overrides, no `Periodic`), `Transfinite`
  (63 files), `Recombine` (45), `Extrude` (21), `Mesh.ElementOrder`/
  `SecondOrder` in 21 files (in addition to `-order` on the command line),
  physical groups with names (90 files) or numbers only (40).
- **Gmsh version requirement**: `FindGMSH.cmake` only requires a `gmsh`
  executable (adds `-optimize` for gmsh < 3). Upstream CI images ship
  gmsh **4.13.1** (Debian trixie, `test/ci/debian:stable/Dockerfile` — note
  `FROM debian:trixie`), **4.8.4** (Ubuntu 22.04, `ubuntu:lts`), and the wheel
  test job installs Debian bookworm's gmsh (**4.11.1**) into `python:3.10`
  (versions from distribution package archives — unverified locally). Generated
  node/element numbering differs between gmsh versions, so any golden data
  derived from generated meshes is gmsh-version-specific: **pin one gmsh
  version for fixture generation (recommend the Debian trixie 4.13.1 used by
  the main CI image) and commit generated `.msh` files as fixtures** in
  akantu-rs to remove the run-time gmsh dependency from the fast tier.
- `eigen_modes.py` (EX-036) calls `gmsh -2` at run time; `test_python_package`
  CI jobs call `gmsh -2 bar.geo`.

| ID | Group | Files | Committed / generated | Consumers | Porting notes |
|---|---|---|---|---|---|
| MDATA-001 | FE-engine element meshes | `test/test_fe_engine/_{segment_2,segment_3,triangle_3,triangle_6,quadrangle_4,quadrangle_8,tetrahedron_4,tetrahedron_10,pentahedron_6,pentahedron_15,hexahedron_8,hexahedron_20,bernoulli_beam_2,bernoulli_beam_3,discrete_kirchhoff_triangle_18}.msh` + `.geo` sources (`segment.geo`, `line.geo`, `circle.geo`, `square.geo`, `square_structured.geo`, `cube.geo`, `cube_physical_names.geo`, `hexa_structured.geo`, `pentahedron.geo`); structural meshes also in `test/test_model/test_structural_mechanics_model/` | committed (+ some generated: `cube.msh`, `cube_physical_names.msh`) | TEST-012…021, TEST-152, TEST-210 | Structural meshes use the `_miot_gmsh_struct` variant (extra per-element data). |
| MDATA-002 | Buildfacets meshes | `test/test_mesh_utils/test_buildfacets/_{triangle_3,triangle_6,quadrangle_4,quadrangle_8,quadrangle_4_triangle_3,quadrangle_8_triangle_6,tetrahedron_10,hexahedron_8,hexahedron_20,pentahedron_6,pentahedron_15,hexahedron_8_pentahedron_6,hexahedron_20_pentahedron_15}.msh` | committed | TEST-039 | Mixed-element meshes. |
| MDATA-003 | Patch-test meshes | `test/test_model/patch_tests/data/_<12 types>.msh` (MSH 2.1/2.2), `bar_{segment,triangle,quadrangle,tetrahedron,hexahedron,pentahedron}.geo` → `bar_<type>.msh` (orders 1 and 2) | committed + generated | TEST-050…063, TEST-109, TEST-128…131 | `PATCH_TEST_MESHES`/`PATCH_TEST_BAR_MESHES` cache variables. |
| MDATA-004 | Cohesive/contact straight-interface geometries | `test/.../test_cohesive/data/cohesive_{1D,strait_2D,strait_2D_structured,strait_2D_mixte,strait_3D,strait_3D_structured}.geo` (+ copies in `test_detector/data/`, plus `cohesive_strait_2D.msh`, `test.msh`), `test_selector/data/selection_{1D,2D,3D}.geo` | generated (order 1 and 2) | TEST-115, TEST-217, TEST-219 | 11 generated variants named `_cohesive_<dim>d_<nodes>_<types>.msh`. |
| MDATA-005 | Other test meshes | committed: `test_geometry_{triangle,tetrahedron}.msh`, `test_dumper.msh`, `purify_mesh.msh`, `plate.msh`/`parallel_test.msh`/`pair_test.msh` (non-local), `phasefield_selector.msh`, `test_{one,two,four}_element(s).msh`, `test_optimal_damage.msh`, `material_selector.msh`, `cube_3d_tet_4.msh`, `1_tetrahedron.msh`, `mesh_section_gap.msh`, `triangle.msh`/`embedded_mesh*.msh` (embedded), `bar.msh` (solver, synchronizer), `fatigue.msh`, `quadrangle.msh`, `implicit.msh`, `square.msh` (IG_TG), `2d_basic_interface.msh`, `mesh_cohesive_friction.msh`, etc.; generated from `.geo` in each test dir (see Appendix) | mixed | various | Commit all generated meshes in akantu-rs. |
| MDATA-006 | Extra packages and examples meshes | `extra_packages/**/test/**/*.msh` (committed: FE2, damage, IGFEM, NTN), `*.geo` there; `examples/**/*.geo` (38) and committed `examples/c++/solid_mechanics_cohesive_model/{cohesive_extrinsic,cohesive_intrinsic}/triangle.msh`, `examples/c++/scalability_test/cube.msh`, `examples/python/.../stiffness_matrix/plate.msh`, `.../compression/detection-explicit.msh` | mixed | TEST-175…205, EX-001…048 | — |

---

## D. CI and tooling

### D.1 Upstream CI matrix (GitLab, `.gitlab-ci.yml` + `.gitlab/ci.d/*.yaml`)

| Job family | Image / compiler | Build type | `AKANTU_TEST_EXAMPLES` | What runs | Ranks used |
|---|---|---|---|---|---|
| `configure/build_libs/build_tests/test:debian_stable_gcc` | `registry.gitlab.com/akantu/akantu/debian:stable` (= `FROM debian:trixie`), gcc | Coverage | FALSE | full ctest (`-T test --timeout 1800`) + gcovr Cobertura | per `register_test` (PARALLEL: N…1 where N = runner cores; gtest p>1 only if CMake ≥ 3.31 — trixie ships CMake ≥ 3.31, so yes, unverified) |
| `...:debian_stable_clang` | idem, clang | Release | TRUE | ctest incl. `*-test` examples | idem, examples PARALLEL = 2 |
| `...:ubuntu_lts_gcc` | `ubuntu:lts` (= `FROM ubuntu:22.04`, CMake 3.22 → **gtest MPI variants skipped**) | Release | TRUE | ctest incl. examples | idem |
| `...:ubuntu_lts_gcc_valgrind` | `ubuntu:lts`, gcc | Valgrind | FALSE | ctest under `valgrind --error-exitcode=111 --leak-check=full --suppressions=test/ci/ompi_init.supp` | idem |
| `configure/build_akantu/build_pip/test:python_package` | `manylinux_2_28_x86_64:latest` (gcc-toolset-14) → test in `python:3.10` | Release | — | lib built without tests/python (`AKANTU_PYTHON_INTERFACE=FALSE`, `AKANTU_PARALLEL=FALSE`, solver Eigen), wheels for cp39–cp313 via `pip wheel` + `auditwheel`; test = `pip install akantu`, `python -c "import akantu"`, `gmsh -2 bar.geo && python dynamics.py` | 1 |
| `...:python_package_macos` | macOS runner (spack env, apple-clang 17, aarch64) | Release | — | same with `delocate-wheel`; `CMAKE_OSX_DEPLOYMENT_TARGET=14.0` | 1 |
| code quality | ruff component (allow failure), clang-tidy & clang-format components (`code_quality:latest` image, `-x third-party -x extra-packages -x build`), warning-parser over build logs | — | — | lint only | — |
| deploy | `python:latest` | — | — | twine upload to GitLab registry (default branch) / PyPI (tags); `package:release` tars `build/examples/python` + release-cli | — |

### D.2 CI / tooling catalogue

| ID | Name | Original files | Behavior / purpose | Required packages / options | Parallel aspects | Porting notes |
|---|---|---|---|---|---|---|
| CI-001 | GitLab pipeline skeleton | `.gitlab-ci.yml:1-35` | Stages `version, configure, build_libs, build_tests, test, code_quality, deploy, release`; workflow on MR, default branch, `v*` tags; `.code_only_rules` (only when C++/CMake/Python/CI files change) | — | — | Rust CI (GitHub/GitLab) needs: fmt, clippy, fast tier, medium tier (MPI 1/2/4), python tier. |
| CI-002 | Code-quality components | `.gitlab-ci.yml:36-65`, `.gitlab/ci.d/code-quality.yaml`, `.clang-format`, `.clang-tidy`, `.clangd`, `.flake8`, `pyproject.toml [tool.ruff]`, `.codeclimate.yml`, `.editorconfig` | ruff (line length 79), clang-tidy/format with CMake args `-DAKANTU_PYTHON=ON -DAKANTU_EXAMPLES=ON -DAKANTU_TESTS=ON -DAKANTU_PARALLEL=ON`; warning-parser on `build-*.log` | — | — | Map to `cargo fmt`/`clippy -D warnings`, `ruff` for Python shims. |
| CI-003 | `register_test` | `cmake/AkantuTestsMacros.cmake:391-603` | see §C.0 (packages, UNSTABLE, `.sh` override, `.verified`, PARALLEL levels, gtest discovery, valgrind) | AKANTU_TESTS | PARALLEL_LEVEL / default N, N/2, …, 1 | Rust: test registry with explicit rank lists (fixed 1/2/4 per mission). |
| CI-004 | gtest aggregation | `AkantuTestsMacros.cmake:279-389`, `test/test_gtest_main.cc` | `register_gtest_sources` / `register_gtest_test` (one binary per dir, `gtest_discover_tests` with `--discovery`) | gtest (fetched: `third-party/google-test`) | rank-0 printing; p>1 needs CMake ≥ 3.31 | — |
| CI-005 | Test tree / activation | `AkantuTestsMacros.cmake:118-277`, `test/CMakeLists.txt` | `add_test_tree`, `add_akantu_test`, `is_test_active`, options `AKANTU_BUILD_ALL_TESTS` (ON), `AKANTU_BUILD_UNSTABLE_TESTS`, `AKANTU_TESTS_ALWAYS_USE_MPI`; optional `test/benchmark` (gbenchmark / `AKANTU_BENCHMARK`, directory absent) | — | — | Absent dirs referenced: `test_mesh`, `test_contact`, `test_heat_transfer_model`, `test_pbc_tweak`, `benchmark`. |
| CI-006 | Test driver | `cmake/akantu_test_driver.sh` | Runs test in work dir with env file, optional MPI launcher/valgrind/python; tees `.lastout/.lasterr`; **early `exit 0` at l.147 disables post-processing and `.verified` diff** | bash | `-p "mpirun ... -np" -N <p>` | Finding 1. In Rust use explicit assertions; golden comparisons inside tests. |
| CI-007 | `akantu_diff.sh` | `cmake/akantu_diff.sh` | Legacy: run `./$1`, diff stdout with `$2` | — | — | Unused anywhere in the tree. |
| CI-008 | Mesh generation | `cmake/Modules/FindGMSH.cmake` | `find_program(gmsh)`, version parse, `add_mesh(target geo DIM ORDER [OUTPUT])` | gmsh executable (REQUIRED when tests/examples on) | — | §C.5. |
| CI-009 | Example registration | `cmake/AkantuExampleMacros.cmake`, `cmake/AkantuSimulationMacros.cmake` | §B.0 | AKANTU_EXAMPLES, AKANTU_TEST_EXAMPLES | PARALLEL examples → 2 ranks | — |
| CI-010 | Test environment & sanitizers | `cmake/akantu_environement.sh.in`, `.csh.in`, `akantu_install_environement.*.in`, `cmake/asan_akantu.supp`, `cmake/lsan_akantu.supp`, `cmake/sanitize-blacklist.txt`, `cmake/AkantuExtraCompilationProfiles.cmake` | `PYTHONPATH` (build/python, `test/`, `test/test_fe_engine`), `LD_LIBRARY_PATH`, ASAN/LSAN suppressions (MPI, gtest, NumPy/Python allocations) | — | MPI leak suppressions | Rust: miri/ASAN not needed for safe code; keep Python-path setup for pytest tier. |
| CI-011 | Debian image | `test/ci/debian:stable/Dockerfile`, `.openmpi/mca-params.conf` | `debian:trixie` + g++/gfortran/clang/cmake, OpenMPI, MUMPS, Scotch, PETSc, Boost, OpenBLAS, Eigen, HDF5 (+openmpi), Python3 + numpy/scipy/mpi4py, Sphinx/breathe/doxygen, **gmsh**, pytest, gcovr, ninja, ccache, clang-format/tidy, gdb/valgrind | — | MCA: `btl tcp,self`, `rmaps_base_oversubscribe = 1` (oversubscription allowed → N ranks > cores OK) | Our oracle Docker should mirror this (aarch64 Debian trixie). |
| CI-012 | Ubuntu image | `test/ci/ubuntu:lts/Dockerfile`, `.openmpi/mca-params.conf` | Ubuntu 22.04 + MUMPS, Scotch, **CGAL** (installed but not enabled), PETSc, Eigen, mpi4py (pip), gmsh, pytest | — | same MCA params | CMake 3.22 → gtest p>1 skipped. |
| CI-013 | manylinux wheel image | `test/ci/manylinux_2_28_x86_64:latest/{Dockerfile, spack.yaml, packages.yaml, repos.yaml, compilers.yaml, Toolchain.cmake, install-pip.sh, make-wheels.sh, build_env.sh, configure_env.sh}` | Spack v0.22.2 env (boost graph only, eigen, hdf5 ~mpi, ccache) built with gcc 14.2.1; `install-pip.sh` installs `scikit-build==0.8.1` for all CPythons; `make-wheels.sh` loops cp310…cp313, cp39 and repairs with `auditwheel`; `compilers.yaml` (devtoolset-8/centos6) and `repos.yaml` are stale/unused | — | sequential | Rust wheels: `maturin build --manylinux 2_28` (pyo3 abi3 could replace per-version builds — architecture decision). |
| CI-014 | macOS wheel env | `test/ci/macosx/{spack.yaml, configure_env.sh, build_env.sh, make-wheels.sh}` | Spack v0.22.5 env on apple-clang 17 aarch64; venv + `pip wheel` + `delocate-wheel` | — | — | Our dev machine is macOS arm64 → native wheel builds feasible. |
| CI-015 | Code-quality image | `test/ci/code_quality:latest/Dockerfile` | trixie-slim based image with build deps + gmsh for clang-tidy | — | — | — |
| CI-016 | Docker build jobs | `.gitlab-ci.yml:67-86`, `.gitlab/ci.d/images.yaml:1-20` | kaniko builds of `test/ci/${IMAGE_NAME}` on MR changes (manual). **Inconsistent**: job `docker build:debian-stable` uses `IMAGE_NAME: 'debian:bookworm'` (directory is `debian:stable`), `docker build:debian-testing` uses `debian:testing` (no such directory) → these two jobs cannot work | — | — | Note only. `package:release` uses image `debian:testing` that would come from that broken job. |
| CI-017 | Version determination | `.gitlab-ci.yml` `version_determination`, `cmake/semver.py`, `cmake/git_info`, `cmake/Modules/CMakeVersionGenerator.cmake` | `python3 cmake/semver.py > VERSION` from git describe (needs `GIT_DEPTH 500`) | git | — | Rust: version from Cargo + git describe in build script for `getVersion()`. |
| CI-018 | Configure options (CI) | `.gitlab/ci.d/templates.yaml:22-82` (`.configure`) | `-DAKANTU_COHESIVE_ELEMENT -DAKANTU_IMPLICIT -DAKANTU_IMPLICIT_SOLVER="Mumps+PETSc" -DAKANTU_PARALLEL -DAKANTU_STRUCTURAL_MECHANICS -DAKANTU_DIFFUSION -DAKANTU_DAMAGE_NON_LOCAL -DAKANTU_PHASE_FIELD -DAKANTU_PYTHON_INTERFACE -DAKANTU_CONTACT_MECHANICS -DAKANTU_TRACTION_AT_SPLIT_NODE_CONTACT -DAKANTU_EXAMPLES -DAKANTU_BUILD_ALL_EXAMPLES -DAKANTU_TESTS -DAKANTU_RUN_IN_DOCKER -DAKANTU_TEST_EXAMPLES=${TEST_EXAMPLES}`, Ninja, `BLA_VENDOR=Generic` | as listed | parallel ON | This is the **reference oracle configuration** for the oracle (add CGAL/embedded/extra packages separately to cover INACTIVE tests). |
| CI-019 | Build jobs | `templates.yaml:83-152` | `cmake --build build -t akantu -j1`, then `-t py11_akantu -j1`, then full build `-j1` (memory: TUs up to ~12 GB), ccache 1 GiB, 5 h timeout | — | — | Matches AGENTS.md advice (low parallelism). |
| CI-020 | Test job | `templates.yaml:153-199`, `test/ci/ctest2junit.xsl` | `ctest -T test --output-on-failure --no-compress-output --timeout 1800`; coverage via gcovr (Coverage builds); JUnit conversion with `xsltproc` | xsltproc, gcovr | — | Per-test timeout 30 min (structural example XL). |
| CI-021 | Python package pipeline | `templates.yaml:200-314`, `.gitlab-ci.yml:247-313`, `setup.py` (`CI_AKANTU_INSTALL_PREFIX`) | configure lib (no python, no tests, Eigen, sequential, `Boost_NO_BOOST_CMAKE`), `cmake --install`, build wheels against the installed lib, test import + `dynamics.py` | numpy, scipy (runtime), gmsh (test) | sequential | Defines the **minimal Python acceptance test** upstream: `import akantu` + EX-031. |
| CI-022 | Deploy & release | `.gitlab-ci.yml:314-395`, `.pypirc` | twine to GitLab registry/PyPI; release with examples tarball, `markdown-extract` of CHANGELOG section, RTD link | — | — | Out of scope for the port (no remote pushes without approval). |
| CI-023 | Jenkins (obsolete) | `Jenkinsfile` | Phabricator/c4science pipeline (`test/ci/phabricator/Dockerfile`, `test/ci/scripts/hbm` — **both absent**), configure with `-DAKANTU_HEAT_TRANSFER` (obsolete option), `ctest -T test`, XUnit thresholds | — | — | Dead; CHANGELOG 4.0.0 says CI moved to GitLab. |
| CI-024 | Documentation build | `.readthedocs.yaml`, `doc/CMakeLists.txt`, `doc/dev-doc/{CMakeLists.txt, conf.py, akantu.dox.j2, requirements.txt}`, `packages/documentation.cmake`, `cmake/Modules/FindSphinx.cmake` | Sphinx + breathe over Doxygen XML (`breathe_projects = {"Akantu": .../xml}`), RTD on ubuntu-22.04/python 3.11 | documentation | — | Rust: rustdoc + mdBook/Sphinx for Python API. |
| CI-025 | Developer tooling misc | `CMakePresets.json`, `.gitpod.yml`, `AGENTS.md` (upstream), `CTestConfig.cmake`, `cmake/libakantu/*` (gdb pretty printers), `cmake/libakantu-gdb.py.in`, `cmake/akantu-debug.cc.in`, `test/ci/ompi_init.supp`, `test/ci/{debian:stable,ubuntu:lts}/.openmpi/mca-params.conf` | Presets `common` (cohesive, implicit, parallel, structural, diffusion, damage_non_local, phase_field, contact, traction_at_split_node_contact), `debug` (+tests), `release`, `relwithdebinfo`, gcc variants, `devcontainer` (+examples, Mumps+PETSc, python, bundled Eigen); Gitpod sequential Eigen build; CDash drop site `lsmssrv1.epfl.ch` (legacy); valgrind suppressions for OpenMPI init | — | OpenMPI oversubscription settings | `CMakePresets.json` `common` = good default oracle config. |

---

## E. Documentation-only and hidden features

| ID | Feature | Original files | Behavior (as documented / found) | Required packages | Parallel aspects | Porting notes |
|---|---|---|---|---|---|---|
| DOC-001 | Input-file grammar | `doc/dev-doc/manual/io.rst:6-75`, `test/test_io/test_parser/input_file.dat` | Sections `type name [option] [ ... ]` (nestable, e.g. `rules material [...]` inside a material), `#` comments, `\` line continuation, algebraic expressions referencing other parameters and constants (`2*pi + max(2, general)`), vectors `[a, b]`, matrices `[[...],[...]]`, random parameters `value distribution [a, b]` (`uniform`, `weibull`, …), `permissive_parser`, `seed`, `debug_level` globals; `user` section (`getUserParser()`) for application parameters | core | each rank parses the file | Parser semantics are R1's, but Python tests (TEST-033) and all examples depend on exact evaluation. |
| DOC-002 | Solver configuration from the input file | `io.rst:76-246` | `model <model_type> [ dof_manager_type = default\|petsc; default_solver = <name>; time_step_solver <static\|dynamic\|dynamic_lumped> [ name; sparse_solver_type = mumps\|eigen\|petsc\|auto; non_linear_solver <linear\|newton_raphson\|newton_raphson_modified\|lumped\|petsc_snes\|petsc_tao> [ max_iterations=10; threshold=1e-10; convergence_type=solution; force_linear_recompute=true; verbose_iteration=false ] ; integration_scheme <type> [ name = <dof>; solution_type ] ] ]`; PETSc options forwarded verbatim (`snes_*`, `ksp_*`, `pc_*`, `tao_*`), Akantu NR names rejected for PETSc solvers; command-line PETSc options override | implicit (+petsc) | DOF manager `petsc` is the distributed backend | Not obvious from source layout; used by `material_linear_tao.dat` (TEST-100) and patch tests. Rust: same keys, PETSc options become backend-specific (decision needed if PETSc not ported — Q11). |
| DOC-003 | Material sections placement | `io.rst:247-276`, `constitutive-laws.rst:25-60` | `material <law>[:<flavor>] [ name = ...; rho = ...; finite_deformation = true; ... ]` inside `model` block (preferred) or at top level (fallback); random-distributed parameters; Python alternative `initFull({name: {"type": law, ...}})` (PY-075) | solid_mechanics | — | — |
| DOC-004 | Command-line & environment options | `src/common/aka_common.cc:62-130`, `examples/c++/scalability_test/run.sh` | `--aka_input_file <file>` (overrides the file given to `initialize`), `--aka_debug_level <n>`, `--aka_print_backtrace`, `--aka_seed <n>`; env `AKA_SEED`, `AKA_DEBUG_LEVEL`; seed precedence env > CLI > input `seed` > time; **effective seed = seed × (rank + 1)**; parsed args removed from argv; Python passes `sys.argv` at import (PY-004) | core | rank-dependent seed | Must be reproduced for parity of random-parameter tests (random fields differ per rank count!). |
| DOC-005 | Mesh formats and mesh data | `doc/dev-doc/manual/getting_started.rst:226-252`, `basic_types.rst:262-570` | Readers: Gmsh (`_miot_gmsh`, `_miot_gmsh_struct` for structural extra data), Abaqus, Diana; Gmsh physical tag → `tag_0` (Int), names → `physical_names` (string) + element groups created by default, geometric tag → `tag_1`; element tables (1D/2D/3D, cohesive, Bernoulli) with node ordering figures | core | — | MSH versions §C.5; Abaqus/Diana readers have no tests in-tree (R1). |
| DOC-006 | Dumpable field names (SMM) | `io.rst:319-348` | `displacement, mass, velocity, acceleration, force, residual, increment, blocked_dofs, partitions, material_index` (nodal/elemental), `strain, Green strain, principal strain, principal Green strain, grad_u, stress, Von Mises stress, material_index` (quadrature) — plus model-specific ones used in examples: `internal_force, external_force, damage, tractions, opening, contact_force, gaps, areas, normals, tangents, slip_velocity_norm` (NTN), Python-registered internals (`factor`) | dumpers | `partitions` shows rank ownership | Field names are part of the user API (`addDumpField("...")`). |
| DOC-007 | Cohesive & fragmentation output | `io.rst:349-392` | dumper `"cohesive elements"`; `FragmentManager` computations (mass, center, velocity, inertia, principal directions, elements per fragment) | cohesive_element | — | PY-082. |
| DOC-008 | Advanced dumping | `io.rst:393-450`, `doc/dev-doc/reference.rst` (`DumperText`) | external fields (`addDumpFieldExternal`), padding helpers, custom dumpers via IOHelper (`DumperParaview`, `DumperText`) | dumpers | parallel `.pvtu` | `DumperText` has no test/example — check scope with R1. |
| DOC-009 | Parallel workflow | `getting_started.rst:253-290` | `mesh.distribute()` (Scotch; vertex weights = element computational cost, edge weights = communication), run with `mpirun`; only rank 0 needs to read the mesh | parallel, scotch | ghost-element layer (JOSS) | Rust MPI design (R3, architecture). |
| DOC-010 | Constitutive laws catalogue | `doc/dev-doc/manual/constitutive-laws.rst` (1407 lines) | Elastic isotropic / anisotropic / orthotropic, Neo-Hookean, viscoelastic SLS and generalised Maxwell, plasticity (linear isotropic hardening), damage Marigo & Mazars, non-local damage, cohesive laws (linear "Snozzi", friction, fatigue, exponential) | solid_mechanics, cohesive_element, damage_non_local | — | Cross-check with R2 inventory; cohesive **exponential** and **fatigue** laws have no registered test (fatigue only in dead TEST-221). |
| DOC-011 | Writing new constitutive laws | `doc/dev-doc/manual/new-constitutive-laws.rst` (375 lines), `examples/c++/solid_mechanics_model/new_material/*` | C++ plug-in procedure (internals, parameters, factory registration); Python equivalent shown by examples | solid_mechanics | — | Plug-in API is a first-class feature (Rust trait + registry + Python trampoline). |
| DOC-012 | Contact mechanics model | `doc/dev-doc/manual/contactmechanicsmodel.rst` (476 lines) | detection (surface selectors, search parameters), resolution (penalty normal/tangential, friction), coupling with SMM via couplers | contact_mechanics | — | R3. |
| DOC-013 | Node-to-node contact & rate-and-state laws | `doc/dev-doc/manual/nodetonodecontact.rst` (218 lines) | NTN residual formulation, friction laws: Coulomb, slip weakening, rate-and-state with state evolution laws (ageing, slip; regularised variants; Dieterich, revised, N-shaped), time step factor ~0.01 | traction_at_split_node_contact | — | Extra package actively developed in 2024-26 (git log). |
| DOC-014 | Phase-field model | `doc/dev-doc/manual/phasefieldmodel.rst` (265 lines) | Staggered coupling, exponential/AT1/AT2, energy splits, irreversibility options (penalisation, TAO bounds) | phase_field | — | R3. |
| DOC-015 | Diffusion model | `doc/dev-doc/manual/diffusion.rst` (184 lines), CHANGELOG 5.0.4 | `DiffusionModel` replaces `HeatTransferModel` (kept as compatibility class), diffusion laws as constitutive laws (`heat_diffusion`), explicit lumped capacity / static / implicit | diffusion | — | Python must keep `HeatTransferModel` name (PY-084). |
| DOC-016 | Structural mechanics model | `doc/dev-doc/manual/structuralmechanicsmodel.rst` (219 lines) | `StructuralMaterial` fields table, element-material array, static Newton, nodal forces set directly | structural_mechanics | — | — |
| DOC-017 | Basic types & iterators (v5) | `doc/dev-doc/manual/basic_types.rst` (571 lines), CHANGELOG 5.0.0 | `Vector`/`Matrix` = Eigen; `make_view<rows,cols>` static views; named `zip` tuples (`"a"_n = as`); groups iteration | core | — | R1; Python consequence PY-006. |
| DOC-018 | Released API changes (CHANGELOG 5.0.0–5.0.7-post1) | `CHANGELOG.md`, `doc/dev-doc/changelog.md` (identical copy) | 5.0.6: Python FEEngine (`integrate`, `computeBtD`, `computeBtDB`, `computeNtb`, `computeNtbN`, `getShapes`, `getShapesDerivatives`) and group dumping (`dumpGroup`, `setGroupDirectory`, `setGroupBaseName`, `addDumpGroupField`, `addDumpGroupFieldVector`); 5.0.7: empty-filter fix in Python; 5.0.4: `SparseSolverEigen`, macOS wheels, Diffusion replaces HeatTransfer, cohesive K-assembly fix; 5.0.3: contact Python API; 5.0.0: Eigen types, Python `debug` sub-module, `finalize` not needed, `applyDirichletBC` → `applyBC`; deprecations `begin_reinterpret`, `storage()`, `get*Energy(type, index)` | — | — | All reflected in PY rows. |
| DOC-019 | JOSS paper claims & benchmark data | `joss/paper.md`, `joss/paper.bib`, `joss/compile.sh`, `joss/results/{plot.py, TTS.svg, cube.svg, cohesive_insertion.png, timmings_*_gcc_jed_v{4.0.1,5.0.4}.csv}`, `joss/.gitignore` | HPC FE library for contact & dynamic fracture: dynamic cohesive insertion in parallel, contact incl. rate-and-state friction, cohesive–contact interaction, **ghost-element (not ghost-node) communication layer**, scalability study 1–720 cores (EX-018/EX-049) | parallel, cohesive_element, contact | ghost elements | Defines the "headline features" that must be parity-verified; timings for `docs/BENCHMARKS.md`. |
| DOC-020 | Unreleased features since v5.0.7-post1 (469 commits, not in CHANGELOG) | `git log v5.0.7-post1..HEAD` of the reference | PETSc SNES & TAO non-linear solvers; `SparseSolverFactory` and new `_auto` logic; PETSc DOF manager rework (ghosted vectors, AO); periodic BC tests (static, mass) and pbc consistency of arrays; NTN contact with rate-and-state laws, steady-state example and tests; phase field: anisotropic split, AT1, penalisation, thermal stress, mass degradation, TAO irreversibility; thermal plane-stress materials; external work computed during force assembly; Python: generic `set/get` on parameters, material dict in `initFull`, `registerCustomMaterials`, `hasStiffnessMatrixChanged`, phase-field CL bindings, mesh keep-alive, removal of global variables, initialisation without argc/argv; examples run as tests (2026-03) | various | — | These are the newest (least battle-tested) features; prioritise oracle comparison. |
| DOC-021 | External tutorials | `doc/dev-doc/tutorials.rst` | Python notebooks at `gitlab.com/akantu/tutorials` (Renku) — not in tree | python_interface | — | Out-of-tree compatibility target (Q14). |
| DOC-022 | Element appendix | `doc/dev-doc/manual/appendix/elements.rst` (+ `appendix.rst`, `figures/elements/*`) | Per element: node coordinates/ordering, shape functions, quadrature point coordinates & weights tables for all regular elements | core | — | Independent source for element-class verification (with `py_engine.py`). |
| DOC-023 | Manual structure & misc | `doc/dev-doc/{index.rst, examples.rst, reference.rst, manual/models.rst, models_commons.rst (empty), solidmechanicsmodel.rst, extra.rst, bibliography.rst, cite-akantu.{rst,bib}, manual-bibliography.bib}`, `doc/dev-doc/_static/*.svg`, `doc/dev-doc/manual/figures/**` | SMM manual: initial conditions, Dirichlet/Neumann BCs, cohesive extrinsic/intrinsic insertion, static/implicit/explicit methods; API reference via doxygen directives | — | — | Port prose as needed. |
| DOC-024 | Coding conventions & design artefacts | `doc/coding_convention/{README, akantu-input-mode.el, snippet/*}`, `doc/uml.dia`, upstream `AGENTS.md` | Emacs mode for `.dat` files, code snippets, UML diagram | — | — | Informational. |

---

## F. Python API compatibility plan inputs

Goal: a Rust-backed `akantu` Python package (pyo3 + numpy) on which the
existing Python examples and Python tests run **unchanged**, or with the
minimal, documented adjustments listed in F.6. Inputs below were extracted
mechanically from the 32 Python files of `examples/python/**`, `test/**`
(registered tests and fixtures) and `examples/c++/.../python_user_defined_bc`,
intersected with the binding definitions (script over `python/*.cc`).

### F.1 Acceptance tiers

| Tier | Content | Upstream status | Requirement for akantu-rs |
|---|---|---|---|
| P0 | `import akantu`; EX-031 `dynamics.py` (wheel smoke test, CI-021) | CI (wheels) | Must pass. |
| P1 | TEST-153 `test_pybind.py` (+ helper module), TEST-033 `test_parser.py`, TEST-058 `test_patch_linear_elastic_explicit.py`, TEST-059 `test_patch_linear_elastic_static.py` | CI | Must pass unchanged (except the helper module, which must be re-implemented — F.5). |
| P2 | Registered Python examples EX-030…037, EX-039…048 (18 scripts) | CI Release jobs (exit code only) | Must run to completion; add numeric checks in akantu-rs (energies, fragment counts, eigenvalues, contact output) compared with the C++ oracle. |
| P3 | Inactive/broken upstream: TEST-060 (anisotropic, broken), TEST-061…063 (heat, dead package), EX-038 (`custom_material.py`, unbound `getNormals`) | not run | Port intent; adjust scripts minimally (document). |
| P4 | Out-of-tree tutorials (DOC-021) | — | Best effort; not part of parity evidence. |

### F.2 Module-level names that must exist

From usage counts (number of files using each):

- **Classes / constructors**: `Mesh` (19), `SolidMechanicsModel` (9),
  `SolidMechanicsModelCohesive` (3), `StructuralMechanicsModel` (3),
  `StructuralMaterial` (3), `HeatTransferModel` (1), `ContactMechanicsModel` (1),
  `CouplerSolidContact` (2), `CouplerSolidPhaseField` (2), `FragmentManager` (1),
  `MeshAccessor` (3), `MeshUtils` (static `buildFacets`), `Material` (3, subclassed),
  `MaterialCohesive` (1, subclassed), `MaterialFactory` (`getInstance`),
  `DirichletFunctor` (2, subclassed), `NeumannFunctor` (1, subclassed),
  `FixedValue` (11, also subclassed), `IncrementValue` (3), `FromTraction` (2),
  `InterceptSolverCallback` (1, subclassed), `TermsToAssemble` (1),
  `MeshDataMaterialSelectorString` (2), `MaterialCohesiveRulesSelector` (1),
  `PhysicalSurfaceSelector` (3), `Parsable` (1), `SolidMechanicsModelOptions` (2),
  `AkantuSparseMatrix` (3), `SparseSolverPETSc` (1 — `isinstance` probe only),
  `Exception` (1).
- **Functions**: `parseInput` (16), `getStaticParser` (1), `getElementTypes` (1),
  `registerCustomMaterials` (1), `debug.setDebugLevel` (1).
- **Enum values / namespaces**: `_static` (12), `_explicit_lumped_mass` (12),
  `_implicit_dynamic` (5), `AnalysisMethod._implicit_dynamic` (2), `_x` (12),
  `_y` (11), `_bernoulli_beam_2` (3), `_bernoulli_beam_3` (1), `_quadrangle_8` (1),
  `_triangle_3` (1), `_unsymmetric` (1), `_pat_readable` (4), `_pat_parsable` (4),
  `_pat_parsmod` (2), `_pat_modifiable` (1), `dblError` (1),
  `SolveConvergenceCriteria.residual` (6), `SolveConvergenceCriteria.solution` (6),
  `ParserType.material` (1).
- Implicitly required: everything reachable from these (A.4–A.6), the aliases
  `FromStress`/`FromTraction`, `ElementNull`, `__version__`, `has_mpi()`.

### F.3 Methods that must exist (name → number of files using it)

`add`(1) · `addConnectivityType`(3) · `addDumpField`(13) · `addDumpFieldToDumper`(3)
· `addDumpFieldVector`(13) · `addDumpFieldVectorToDumper`(3) · `addMaterial`(3)
· `append`(2) · `applyBC`(15) · `asMatrix`(1) · `asReal`(1) · `assembleMass`(1)
· `assembleMatrix`(2) · `assemblePreassembledMatrix`(1) · `assembleResidual`(1)
· `assembleStiffnessMatrix`(2) · `assembleToResidual`(1) · `buildFacets`(1)
· `checkCohesiveStress`(3) · `computeAllData`(1) · `computeIntegrationPointsCoordinates`(1)
· `createBoundaryGroupFromGeometry`(1) · `distribute`(1) · `dump`(13)
· `elementTypes`(2, incl. keyword `dim=`) · `get`(1) · `getAcceleration`(1)
· `getBlockedDOFs`(8) · `getCheckFacets`(1) · `getConnectivity`(3) · `getConstitutiveLaw`(1)
· `getContactDetector`(3) · `getContactElements`(1) · `getContactMechanicsModel`(2)
· `getDOFManager`(4) · `getDamage`(1) · `getDataReal`(1) · `getDisplacement`(12)
· `getElementFilter`(1) · `getElementGroup`(3) · `getElementInserter`(1)
· `getElementMaterial`(3) · `getEnergy`(9; ids `potential, kinetic, dissipated, reversible, contact`)
· `getExternalForce`(7) · `getFEEngine`(2; incl. `"FacetsFEEngine"`) · `getFacetFilter`(1)
· `getGaps`(2) · `getGradU`(3) · `getInsertionFacets`(1) · `getInstance`(2)
· `getInternalForce`(1) · `getInternalReal`(3; incl. `"∇u"`, `"D∇u"`, `"factor"`, `"delta_max"`)
· `getMaterial`(5; by index and by name) · `getMatrix`(6; `"K"`, `"M"`, `"C"`) · `getMatrixType`(1)
· `getMesh`(4) · `getModel`(2) · `getNbElement`(1; keyword-only) · `getNbElementsPerFragment`(1)
· `getNbFragment`(1) · `getNbIntegrationPoints`(1) · `getNbNodes`(6) · `getNodeGroup`(3)
· `getNodes`(15) · `getNonLinearSolver`(13; with and without solver id) · `getNormals`(2)
· `getNormalsOnIntegrationPoints`(1) · `getOpening`(1) · `getParameter`(1)
· `getPhaseFieldModel`(2) · `getPotentialEnergy`(3) · `getReal`(5) · `getRelease`(1)
· `getSolidMechanicsModel`(4) · `getSparseSolver`(1) · `getSpatialDimension`(1, static)
· `getStableTimeStep`(12) · `getStress`(4) · `getStressOnFacets`(1) · `getSubSections`(1)
· `getTangents`(1) · `getTemperature`(1) · `getTraction`(1) · `getValue`(1)
· `getVelocity`(6) · `initFull`(19; signatures: `()`, `(ModelOptions)`, `(AnalysisMethod)` positional,
`(_analysis_method=...)`, `(_analysis_method=..., _is_extrinsic=...)`, `(dict, _analysis_method=...)`)
· `initMaterial`(4) · `initNewSolver`(4) · `iterateElementGroups`(1) · `makeReady`(3)
· `parseInput`(16) · `parseSection`(1) · `read`(16) · `registerAllocator`(2)
· `registerInternalReal`(2) · `registerParamReal`(5; both overloads) · `resizeConnectivity`(3)
· `resizeNodes`(3) · `saveMatrix`(1) · `savePreviousState`(2) · `search`(1)
· `set`(14; `NonLinearSolver.set` with float/int/enum and `ParameterRegistry.set`)
· `setBaseName`(13) · `setBaseNameToDumper`(3) · `setDebugLevel`(1) · `setMaterialSelector`(3)
· `setSurfaceSelector`(3) · `setTimeStep`(15) · `solve`(2) · `solveStep`(19; `()`, `(solver_id)`, `(callback)`)
· `updateAutomaticInsertion`(1). Plus Python **override points**: `__call__`
(Dirichlet/Neumann), `initMaterial`, `computeStress`, `computeTangentModuli`,
`computePotentialEnergy`, `getPushWaveSpeed`, `checkInsertion`, `computeTraction`,
`assembleMatrix`, `assembleResidual`.

### F.4 Semantic requirements (behaviour that scripts rely on)

1. **Zero-copy writable NumPy views** for every `get*` array accessor
   (`getDisplacement`, `getVelocity`, `getBlockedDOFs`, `getExternalForce`,
   `getNodes`, `getConnectivity`, `getElementMaterial`, `getStress`, `getGradU`,
   `getPotentialEnergy(type)`, internal fields `field(type)`, `getDamage`, `getGaps`,
   `getTemperature`, `getDataReal`, NodeGroup `getNodes`), shape `(n, ncomp)`,
   C-contiguous, so that `a[:] = …`, `a *= 0.9`, `a.reshape(n*d)[mask] = v`,
   `a[i, :] = [...]`, boolean `a[nodes, :] = True` write through to the model
   and later reads see solver updates (live views compared across iterations in
   `phasefield-static.py`). dtype `float64` for `Real`, `bool` for blocked DOFs,
   integer dtype for connectivities/indices (Q6). Views must keep their owner
   alive (fix of A.9 #15).
2. **Model memory stability**: views obtained before `solveStep()` are reused
   across thousands of steps (`velocity`, `gaps`, `displacement`, `damage` in the
   contact/phase-field scripts), so storage must not move during ordinary
   solves. Extrinsic cohesive insertion **adds nodes** (nodal arrays grow); no
   in-tree script keeps a nodal view across insertions (`fragmentation.py`
   writes `getVelocity()[:]` once before the loop), but upstream would leave
   such a view dangling — akantu-rs must either refresh or invalidate views
   (Q15).
3. **Overload resolution and keyword names**: `initFull(_analysis_method=...,
   _is_extrinsic=...)`, `Mesh.getNbElement(element_type=...)` /
   `(spatial_dimension=...)` (keyword-only), `ElementTypeMapArray.elementTypes(dim=...)`,
   `getNonLinearSolver("static")`, `solveStep("explicit_lumped")`,
   `registerParamReal(name, type, descr)` vs `(default, name, type, descr)`,
   `NonLinearSolver.set("max_iterations", 100)` (int coerced), `set(name, enum)`.
4. **Enums**: integer conversion (`int(aka._y)`, used as array index and passed as
   `BC::Axis`), bit-or of `ParameterAccessType` producing an `int`, equality with
   returned values (`model.getMatrixType("K") == aka._unsymmetric`), scoped access
   (`aka.AnalysisMethod._implicit_dynamic`, `aka.SolveConvergenceCriteria.residual`).
5. **Subclassing & virtual dispatch from C++/Rust into Python** for the eight
   extension points of A.8, including calling `super().__init__(...)`,
   `super().registerParamReal(...)`, `super().initMaterial()`, and the default
   implementations of non-overridden virtuals. Python objects created by
   allocators must stay alive while the model uses them (A.9 #17).
6. **Polymorphic return types**: `getNonLinearSolver()` returns the most-derived
   class (scripts probe `hasattr(solver, "getSparseSolver")`); `Material.getModel()`
   returns the cohesive model in cohesive scripts (`model.getElementInserter()`);
   `DOFManager.getMatrix()` returns an AIJ matrix exposing `getIRN/getJCN/getA`.
7. **Parameter system**: `getReal("E")` on Python-defined materials after parsing
   `material local_elastic [E = ...; nu = ...; rho = ...]`; `rho` is a
   pre-registered parameter of every `Material`; `getMatrix("conductivity")`
   on diffusion laws; `get()` raising `akantu.Exception` for non-readable params.
8. **Parser**: `parseInput` can be called several times (global parser
   accumulates); exact algebraic evaluation (`2*pi + max(2, general)`),
   `getParameter(...).getValue()` returns the raw expression string.
9. **Exceptions**: C++ errors surface as `akantu.Exception`; Newton
   non-convergence as `akantu.NLSNotConvergedException` (both subclass Python
   `Exception`).
10. **Side effects**: dumpers create `paraview/` (default directory) with VTU/PVD
    files; scripts do not read them back. `eigen_modes.py` shells out to `gmsh`.
    `solver_callback.py` writes Matrix-Market `K0.mtx`/`K1.mtx` via `saveMatrix`.
11. **Import-time behaviour**: `sys.argv` parsed for `--aka_*` options (DOC-004);
    `mpi4py` imported first when MPI is available; no explicit `initialize()`.
12. **Energies**: `getEnergy(id)` and `getEnergy(id, group)` on SMM; on
    structural model `getEnergy("kinetic"|"potential")`; values reduced over ranks.
13. **SciPy interop**: `AkantuSparseMatrix(K).toarray()` (1-based IRN/JCN,
    symmetric-lower expansion); `eigsh` on masked dense matrices.

### F.5 Test-only native helper (`py11_akantu_test_common`)

`test_pybind.py` imports a separate extension module (built from
`test/test_python_interface/test_common.cc`) with functions `createArray(n, c)`,
`getArray(ptr)`, `copyArray(ptr)`, `getRawPointerArray(array)`,
`createVector(n)`, `getVector`, `copyVector`, `getRawPointerVector`,
`createMatrix(r, c)`, `getMatrix`, `copyMatrix`, `getRawPointerMatrix`, keyed by
raw data pointers (Python `int`). akantu-rs must ship an equivalent test-only
pyo3 module (same name, same semantics: returned arrays alias the native
buffer; `copy*` returns a distinct buffer; passing a NumPy array back yields the
same raw pointer — i.e. Python→native conversion must be zero-copy for
C-contiguous float64 input).

### F.6 Minimal adjustments expected/acceptable (to be recorded as deviations)

| Script / API | Upstream behaviour | Proposed handling |
|---|---|---|
| `patch_test_linear_fixture.py:62` `akantu.SparseSolverPETSc` | attribute exists only in PETSc builds | Always expose the name (non-instantiable if no PETSc backend). |
| `cohesive/custom_material.py` `getNormals` | not bound → AttributeError | Bind `MaterialCohesive.getNormals(type, ghost_type)` (additive). |
| `structural_mechanics_softening.py` `getMaterial()` copy | softening silently ineffective | Keep copy semantics by default for parity; offer a separate live accessor. |
| `DiffusionModel.getBlockedDOFs` copy | writes lost (heat patch tests) | Return a view (bug fix; dead tests become meaningful). |
| `Element(type, id, ghost)` keyword names | wrong names | Accept positional; correct keyword name `id`. |
| Heat-transfer Python patch tests | dead package | Run them in akantu-rs with the diffusion model; fix label ("Explicit" for implicit). |
| `test_patch_linear_anisotropic_explicit.py` | broken script | Rewrite minimal fixes (use `strain[k, l]`, existing check methods). |
| Class names `ConstitutiveLaw...`/`ConstitutiveLawsHandler...` with `::` | demangled C++ names | Use clean names; not referenced by scripts. |
| Module name `akantu.py11_akantu` | native module name | Provide alias module name for compatibility (cheap). |
| `ContactElementsView.__size__` | wrong dunder | Provide `__len__` too. |

### F.7 Architecture inputs

- Keep the two-layer structure: native module (pyo3) + pure-Python
  `akantu/__init__.py` re-implementing `AkantuSparseMatrix`,
  `registerCustomMaterials`, `convertGmshToAkantuMesh`, aliases and stubs
  (these are Python already and can be reused nearly verbatim).
- The native module must allow **Python-implemented trait objects** (materials,
  cohesive laws, diffusion laws, phase fields, BC functors, solver callbacks,
  selectors, FE field functors) to be stored inside Rust models: requires a
  `PyObject`-holding adapter implementing the Rust traits, GIL acquisition in
  callbacks, and error propagation (Python exception in a callback → abort the
  solve with `akantu.Exception` carrying the traceback).
- Array ownership model: Rust model owns `Vec<f64>` storage; NumPy views are
  created with the Python model object as `base`; storage reallocation
  (cohesive insertion, `resize`) must invalidate or refresh views (Q15).
- Wheels: sequential feature set (no MPI) + Eigen-equivalent direct solver
  (pure-Rust sparse LU/Cholesky) to match upstream wheels (PY-001).
- MPI-enabled Python (mpi4py) is optional upstream and untested; plan it as a
  later tier.

---

## G. Open questions

| # | Question | Why it matters | Suggested owner / next step |
|---|---|---|---|
| Q1 | Should any `.verified` file be used as golden data, given the driver never compared them (finding 1)? | Stale goldens would encode old bugs (e.g. cohesive K before 5.0.4). | Oracle: re-run each test with the oracle, diff against `.verified`, record which still match; regenerate otherwise. |
| Q2 | Which NOT REGISTERED intents (TEST-210…231) are in scope as "relevant upstream tests" (AGENTS.md, "Rules for tests and fixtures")? Recommended minimum: TEST-210, 216, 217, 221 (fatigue), 222, 224, 225, 226 (parallel cohesive), 230. | Parity matrix completeness vs effort. | Orchestrator decision. |
| Q3 | Do `extra_materials`, `extra_materials_non_local` and `IGFEM` still compile/pass against the current core? Their tests use older APIs (e.g. `.sh` scripts with `-pc_factor_mat_solver_package`, hard-coded `mpirun -np 40`). | 31 tests (TEST-180…205) are INACTIVE upstream; porting dead code is wasted effort. | Oracle: configure the oracle with these packages ON and report. |
| Q4 | Which gmsh version is pinned for fixture generation, and do we commit generated meshes? | Generated numbering (and thus some goldens) is gmsh-version-specific. | Recommend gmsh 4.13.1 (Debian trixie, main CI image); commit all generated `.msh` in akantu-rs. |
| Q5 | Rank policy: upstream `PARALLEL` without `PARALLEL_LEVEL` means N, N/2, …, 1 (machine dependent; 14 cores locally → 14, 7, 3, 1). The mission requires 1/2/4 (8 expensive). | Defines the medium tier. | Map every PARALLEL test to {1,2,4}; flag tests that need specific counts (TEST-079: 2 only; TEST-042/043/044/080/115/128…131: 1, 2). |
| Q6 | Exact NumPy dtypes of integer arrays (connectivity, element filters, material index, `getNodes()` of node groups): `Idx`/`Int` vs `UInt` sizes in the reference build (`aka_config.hh` `AKANTU_INTEGER_SIZE`). | Python scripts index with these arrays; dtype changes can break `==`/indexing semantics subtly. | Oracle: print dtypes from the oracle wheel (`mesh.getConnectivity(t).dtype`, ...). |
| Q7 | Semantics of repeated `parseInput()` (global parser accumulates sections?) and of several models in one process (`test_multiple_init`). | Python tests call it several times. | R1 (parser) + oracle experiment. |
| Q8 | Do the heat-transfer patch tests (C++ and Python) pass once their package name is fixed? `DiffusionModel.getBlockedDOFs` returns a copy in Python. | Decides whether these become real acceptance tests. | Oracle: patch locally `heat_transfer`→`diffusion` in an out-of-tree copy and run. |
| Q9 | Keep `StructuralMechanicsModel.getMaterial()` copy semantics (parity) or return live references (fix)? | Changes the output of `structural_mechanics_softening.py`. | Architecture decision; document in DECISIONS.md. |
| Q10 | Python registration of custom diffusion laws: trampoline exists but no factory binding. Add `DiffusionLawFactory` in Rust? | Completeness of extension points. | Architecture. |
| Q11 | PETSc-dependent features: phase field AT1/TAO (TEST-095, 099, 100), PETSc solver-option axis of patch tests, `test_sparse_solver_petsc`, `dof_manager_type = petsc`, SNES. Will akantu-rs provide a PETSc backend, or native equivalents (bound-constrained Newton for TAO)? | 3 CI-active tests require PETSc. | dependency audit. |
| Q12 | Python wheel strategy: per-version wheels (upstream: cp39–cp313) vs `abi3`; whether an MPI-enabled Python build is required. | Packaging effort. | Architecture. |
| Q13 | `test_sparse_solver_eigen` is registered with `PACKAGE mumps` and `PARALLEL`: what does the sequential Eigen solver do on >1 rank (gather to rank 0? error?). | Determines expected behaviour for the Rust sequential direct solver under MPI. | Oracle: run with 2 ranks. |
| Q14 | Are the out-of-tree Python tutorials (`gitlab.com/akantu/tutorials`) a compatibility target? | Possibly additional API surface. | Orchestrator / user. |
| Q15 | View invalidation: upstream NumPy views alias C++ buffers without lifetime or reallocation tracking. What should Rust do when storage grows (cohesive insertion, `resizeNodes`)? | Memory safety vs compatibility. | Architecture (pyo3 design). |
| Q16 | Run-time scale of examples/tests is estimated, not measured (e.g. 10⁶-step structural example). | Tier assignment (fast/medium/expensive) and CI timeouts. | Oracle: record wall times in `docs/BENCHMARKS.md`. |
| Q17 | gtest MPI variants (`_2`, …) exist only with CMake ≥ 3.31: which upstream jobs actually ran them (Debian trixie CMake vs Ubuntu 22.04 CMake 3.22)? | Which parallel gtest cases count as "verified upstream". | Oracle: check CMake version in the oracle image; run `ctest -N`. |
| Q18 | `test_variational_operators` (TEST-021) and `src/fe_engine` "fields" prototypes: is the experimental variational-operator API in scope? | Avoid porting unfinished upstream experiments. | R1, architecture. |

---

## H. Counts per prefix

Recomputed from the tables of this document (row IDs in the first column):

| Prefix | Rows | Notes |
|---|---|---|
| PY | 84 | 7 packaging/machinery, 19 enums, 2 functions, 52 class rows, 4 Python-helper rows (A.1–A.7); plus 12 extension points (A.8) and 18 binding defects (A.9) described without separate IDs |
| EX | 50 | 28 C++ examples (EX-001…028), 1 infrastructure (EX-029), 19 Python examples (EX-030…048, of which EX-038 is not registered), scalability tooling (EX-049), documentation assets (EX-050). CMake has **46 `register_example` calls** (28 C++ + 18 Python) |
| TEST | 174 | 149 rows for registered tests (TEST-001…205; several rows group variants — e.g. TEST-073 ×2, TEST-074 ×3, TEST-203 ×2, TEST-205 ×5 — and gtest rows are per source file), 25 rows for unregistered sources/helpers (TEST-210…234). CMake has **132 `register_test` registrations**, **16 gtest binaries** built from **51 effective `register_gtest_sources`** (2 more in dirs without a binary) |
| GOLD | 15 | 24 `.verified` files + CSV/Matrix-Market/`.out` references + in-source and analytical oracles |
| MESH | 6 | groups; 104 committed `.msh` + 134 `.geo` listed individually in the Appendix |
| CI | 25 | — |
| DOC | 24 | — |

Activation status of the 149 registered-test rows (upstream CI configuration,
computed from the status column of §C.2): **CI-active 106**; **INACTIVE 32**
(CGAL 4, embedded 3, extra_materials 9 rows / 9 registrations, IGFEM 16 rows /
22 registrations, see Q3); **DEAD 6** (`heat_transfer`); **UNSTABLE 5**
(TEST-060, 097, 107, 138, 151). Registered examples: 46 (`<name>-test` only when
`AKANTU_TEST_EXAMPLES=ON`; EX-008 needs CGAL/embedded).

---

## Appendix: file coverage

Every file under `python/`, `examples/`, `test/` and `extra_packages/*/test` (plus the root Python packaging files) mapped to at least one inventory ID. Generated by `docs/inventory/tests_examples_python/coverage.py` (ordered regex rules, first match wins; the script fails if any file is unmapped). Total: **1039 files**, 0 unmapped.

| Area | Files |
|---|---|
| python/ and root packaging files | 61 |
| examples (top level) | 2 |
| examples/c++ | 159 |
| examples/python | 101 |
| extra_packages/extra-materials/test | 33 |
| extra_packages/igfem/test | 58 |
| extra_packages/traction-at-split-node-contact/test | 28 |
| test (top level) | 3 |
| test/ci | 23 |
| test/python_fe | 5 |
| test/test_common | 7 |
| test/test_fe_engine | 60 |
| test/test_geometry | 7 |
| test/test_io | 11 |
| test/test_mesh_utils | 47 |
| test/test_model | 386 |
| test/test_python_interface | 5 |
| test/test_solver | 25 |
| test/test_synchronizer | 18 |

### App.1 `python/ and root packaging files` (61 files)

| File | ID(s) |
|---|---|
| `MANIFEST.in` | PY-001 |
| `dev-requirements.txt` | PY-001 |
| `pyproject.toml` | PY-001 |
| `python/CMakeLists.txt` | PY-002 |
| `python/akantu/__init__.py` | PY-003, PY-090, PY-094, PY-095, PY-096 |
| `python/py_aka_array.hh` | PY-005, PY-006 |
| `python/py_aka_common.cc` | PY-004, PY-010…PY-025, PY-030 |
| `python/py_aka_common.hh` | PY-004, PY-010…PY-025, PY-030 |
| `python/py_aka_error.cc` | PY-026 |
| `python/py_aka_error.hh` | PY-026 |
| `python/py_akantu.cc` | PY-004, PY-007 |
| `python/py_akantu.hh` | PY-004, PY-007 |
| `python/py_akantu_pybind11_compatibility.hh` | PY-002 |
| `python/py_boundary_conditions.cc` | PY-069 |
| `python/py_boundary_conditions.hh` | PY-069 |
| `python/py_constitutive_law.cc` | PY-071, PY-072, PY-073 |
| `python/py_constitutive_law.hh` | PY-071, PY-072, PY-073 |
| `python/py_constitutive_law_selector.cc` | PY-070 |
| `python/py_constitutive_law_selector.hh` | PY-070 |
| `python/py_constitutive_laws_handler.hh` | PY-074 |
| `python/py_contact_mechanics_model.cc` | PY-086, PY-087, PY-088 |
| `python/py_contact_mechanics_model.hh` | PY-086, PY-087, PY-088 |
| `python/py_dof_manager.cc` | PY-060…PY-064 |
| `python/py_dof_manager.hh` | PY-060…PY-064 |
| `python/py_dumpable.cc` | PY-048, PY-049 |
| `python/py_dumpable.hh` | PY-048, PY-049 |
| `python/py_fe_engine.cc` | PY-056, PY-057 |
| `python/py_fe_engine.hh` | PY-056, PY-057 |
| `python/py_fragment_manager.cc` | PY-082 |
| `python/py_fragment_manager.hh` | PY-082 |
| `python/py_group_manager.cc` | PY-050, PY-051, PY-052 |
| `python/py_group_manager.hh` | PY-050, PY-051, PY-052 |
| `python/py_heat_transfer_model.cc` | PY-083, PY-084 |
| `python/py_heat_transfer_model.hh` | PY-083, PY-084 |
| `python/py_integration_scheme.cc` | PY-028, PY-058 |
| `python/py_integration_scheme.hh` | PY-028, PY-058 |
| `python/py_material.cc` | PY-076, PY-077, PY-078, PY-079 |
| `python/py_material.hh` | PY-076, PY-077, PY-078, PY-079 |
| `python/py_mesh.cc` | PY-053, PY-054, PY-055 |
| `python/py_mesh.hh` | PY-053, PY-054, PY-055 |
| `python/py_model.cc` | PY-065…PY-068 |
| `python/py_model.hh` | PY-065…PY-068 |
| `python/py_model_couplers.cc` | PY-089 |
| `python/py_model_couplers.hh` | PY-089 |
| `python/py_parser.cc` | PY-027, PY-031, PY-040…PY-043 |
| `python/py_parser.hh` | PY-027, PY-031, PY-040…PY-043 |
| `python/py_phase_field_model.cc` | PY-091, PY-093 |
| `python/py_phase_field_model.hh` | PY-091, PY-093 |
| `python/py_phasefield.cc` | PY-092 |
| `python/py_phasefield.hh` | PY-092 |
| `python/py_solid_mechanics_model.cc` | PY-075 |
| `python/py_solid_mechanics_model.hh` | PY-075 |
| `python/py_solid_mechanics_model_cohesive.cc` | PY-080, PY-081 |
| `python/py_solid_mechanics_model_cohesive.hh` | PY-080, PY-081 |
| `python/py_solver.cc` | PY-044…PY-047 |
| `python/py_solver.hh` | PY-044…PY-047 |
| `python/py_structural_mechanics_model.cc` | PY-085 |
| `python/py_structural_mechanics_model.hh` | PY-085 |
| `requirements.txt` | PY-001 |
| `setup.cfg` | PY-001 |
| `setup.py` | PY-001 |

### App.2 `examples (top level)` (2 files)

| File | ID(s) |
|---|---|
| `examples/CMakeLists.txt` | EX-029 |
| `examples/README.rst` | EX-050 |

### App.3 `examples/c++` (159 files)

| File | ID(s) |
|---|---|
| `examples/c++/CMakeLists.txt` | EX-029 |
| `examples/c++/README.rst` | EX-050 |
| `examples/c++/contact_mechanics_model/CMakeLists.txt` | EX-029 |
| `examples/c++/contact_mechanics_model/README.rst` | EX-050 |
| `examples/c++/contact_mechanics_model/friction/CMakeLists.txt` | EX-004, EX-029 |
| `examples/c++/contact_mechanics_model/friction/README.rst` | EX-004, EX-050 |
| `examples/c++/contact_mechanics_model/friction/bloc.geo` | EX-004 |
| `examples/c++/contact_mechanics_model/friction/bloc_friction.cc` | EX-004 |
| `examples/c++/contact_mechanics_model/friction/images/bloc_friction.gif` | EX-004, EX-050 |
| `examples/c++/contact_mechanics_model/friction/material.dat` | EX-004 |
| `examples/c++/contact_mechanics_model/hertz/CMakeLists.txt` | EX-001, EX-002, EX-003, EX-029 |
| `examples/c++/contact_mechanics_model/hertz/README.rst` | EX-001, EX-050 |
| `examples/c++/contact_mechanics_model/hertz/cohesive-contact.geo` | EX-003 |
| `examples/c++/contact_mechanics_model/hertz/cohesive_contact_explicit_dynamic.cc` | EX-003 |
| `examples/c++/contact_mechanics_model/hertz/contact_explicit_dynamic.cc` | EX-001 |
| `examples/c++/contact_mechanics_model/hertz/contact_explicit_static.cc` | EX-002 |
| `examples/c++/contact_mechanics_model/hertz/hertz.geo` | EX-001, EX-002 |
| `examples/c++/contact_mechanics_model/hertz/images/hertz.png` | EX-001, EX-050 |
| `examples/c++/contact_mechanics_model/hertz/images/hertz.svg` | EX-001, EX-050 |
| `examples/c++/contact_mechanics_model/hertz/material-cohesive.dat` | EX-003 |
| `examples/c++/contact_mechanics_model/hertz/material.dat` | EX-001, EX-002 |
| `examples/c++/diffusion_model/CMakeLists.txt` | EX-029 |
| `examples/c++/diffusion_model/README.rst` | EX-050 |
| `examples/c++/diffusion_model/cube.geo` | EX-007 |
| `examples/c++/diffusion_model/cube_tet.geo` | EX-007 (unused), GOLD-015 |
| `examples/c++/diffusion_model/heat_diffusion_dynamics_2d.cc` | EX-005 |
| `examples/c++/diffusion_model/heat_diffusion_dynamics_3d.cc` | EX-007 |
| `examples/c++/diffusion_model/heat_diffusion_static_2d.cc` | EX-006 |
| `examples/c++/diffusion_model/images/diffusion_3d.gif` | EX-005…EX-007, EX-050 |
| `examples/c++/diffusion_model/images/diffusion_static.png` | EX-005…EX-007, EX-050 |
| `examples/c++/diffusion_model/images/hot-point-1.png` | EX-005…EX-007, EX-050 |
| `examples/c++/diffusion_model/images/hot-point-2.png` | EX-005…EX-007, EX-050 |
| `examples/c++/diffusion_model/material.dat` | EX-005, EX-006, EX-007 |
| `examples/c++/diffusion_model/square.geo` | EX-005, EX-006 |
| `examples/c++/embedded_model/CMakeLists.txt` | EX-029 |
| `examples/c++/embedded_model/concrete.geo` | EX-008 |
| `examples/c++/embedded_model/embedded.cc` | EX-008 |
| `examples/c++/embedded_model/material.dat` | EX-008 |
| `examples/c++/embedded_model/reinforcement.geo` | EX-008 |
| `examples/c++/node_to_node/CMakeLists.txt` | EX-029 |
| `examples/c++/node_to_node/ntn_test_ras.geo` | EX-009 |
| `examples/c++/node_to_node/ras_ss.in` | EX-009 |
| `examples/c++/node_to_node/steady_state.cc` | EX-009 |
| `examples/c++/phase_field_model/CMakeLists.txt` | EX-029 |
| `examples/c++/phase_field_model/README.rst` | EX-050 |
| `examples/c++/phase_field_model/images/phase_field_clusters.png` | EX-010, EX-050 |
| `examples/c++/phase_field_model/material_notch.dat` | EX-010, EX-011 |
| `examples/c++/phase_field_model/phase_field_notch.cc` | EX-010 |
| `examples/c++/phase_field_model/phase_field_parallel.cc` | EX-011 |
| `examples/c++/phase_field_model/square_notch.geo` | EX-010, EX-011 |
| `examples/c++/scalability_test/CMakeLists.txt` | EX-029 |
| `examples/c++/scalability_test/README.rst` | EX-050 |
| `examples/c++/scalability_test/cohesive_extrinsic.cc` | EX-018 |
| `examples/c++/scalability_test/cube.geo` | EX-018 |
| `examples/c++/scalability_test/cube.msh` | EX-018 |
| `examples/c++/scalability_test/images/TTS.svg` | EX-018, EX-050 |
| `examples/c++/scalability_test/images/cube.svg` | EX-018, EX-050 |
| `examples/c++/scalability_test/material-cohesive.dat` | EX-018 |
| `examples/c++/scalability_test/material-elastic.dat` | EX-018 |
| `examples/c++/scalability_test/plot.py` | EX-049 |
| `examples/c++/scalability_test/run.sh` | EX-049 |
| `examples/c++/scalability_test/scalability.sh` | EX-049 |
| `examples/c++/solid_mechanics_cohesive_model/CMakeLists.txt` | EX-029 |
| `examples/c++/solid_mechanics_cohesive_model/README.rst` | EX-050 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/CMakeLists.txt` | EX-015, EX-029 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/README.rst` | EX-015, EX-050 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/cohesive_extrinsic.cc` | EX-015 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/images/cohesive_extrinsic.gif` | EX-015, EX-050 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/material.dat` | EX-015 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/triangle.geo` | EX-015 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic/triangle.msh` | EX-015 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/CMakeLists.txt` | EX-016, EX-029 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/README.rst` | EX-016, EX-050 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/cohesive_extrinsic_ig_tg.cc` | EX-016 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/images/cohesive_extrinsic_ig_tg.gif` | EX-016, EX-050 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/material.dat` | EX-016 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/triangle.geo` | EX-016 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/CMakeLists.txt` | EX-017, EX-029 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/README.rst` | EX-017, EX-050 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/cohesive_intrinsic.cc` | EX-017 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/images/cohesive_intrinsic.png` | EX-017, EX-050 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/material.dat` | EX-017 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/triangle.geo` | EX-017 |
| `examples/c++/solid_mechanics_cohesive_model/cohesive_intrinsic/triangle.msh` | EX-017 |
| `examples/c++/solid_mechanics_model/CMakeLists.txt` | EX-029 |
| `examples/c++/solid_mechanics_model/README.rst` | EX-050 |
| `examples/c++/solid_mechanics_model/boundary_conditions/CMakeLists.txt` | EX-029 |
| `examples/c++/solid_mechanics_model/boundary_conditions/README.rst` | EX-013, EX-014, EX-050 |
| `examples/c++/solid_mechanics_model/boundary_conditions/images/predefined_bc.svg` | EX-013, EX-014, EX-050 |
| `examples/c++/solid_mechanics_model/boundary_conditions/images/user_defined_bc_displ_mag.png` | EX-013, EX-014, EX-050 |
| `examples/c++/solid_mechanics_model/boundary_conditions/predefined_bc/CMakeLists.txt` | EX-013, EX-029 |
| `examples/c++/solid_mechanics_model/boundary_conditions/predefined_bc/material.dat` | EX-013 |
| `examples/c++/solid_mechanics_model/boundary_conditions/predefined_bc/predefined_bc.cc` | EX-013 |
| `examples/c++/solid_mechanics_model/boundary_conditions/predefined_bc/square.geo` | EX-013 |
| `examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc/CMakeLists.txt` | EX-012, EX-029 |
| `examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc/boundary_condition.py` | EX-012 |
| `examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc/fine_mesh.geo` | EX-012 |
| `examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc/material.dat` | EX-012 |
| `examples/c++/solid_mechanics_model/boundary_conditions/python_user_defined_bc/python_user_defined_bc.cc` | EX-012 |
| `examples/c++/solid_mechanics_model/boundary_conditions/user_defined_bc/CMakeLists.txt` | EX-014, EX-029 |
| `examples/c++/solid_mechanics_model/boundary_conditions/user_defined_bc/fine_mesh.geo` | EX-014 |
| `examples/c++/solid_mechanics_model/boundary_conditions/user_defined_bc/material.dat` | EX-014 |
| `examples/c++/solid_mechanics_model/boundary_conditions/user_defined_bc/user_defined_bc.cc` | EX-014 |
| `examples/c++/solid_mechanics_model/explicit/CMakeLists.txt` | EX-019, EX-029 |
| `examples/c++/solid_mechanics_model/explicit/README.rst` | EX-019, EX-050 |
| `examples/c++/solid_mechanics_model/explicit/bar.geo` | EX-019 |
| `examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc` | EX-019 |
| `examples/c++/solid_mechanics_model/explicit/images/bar_pulse.gif` | EX-019, EX-050 |
| `examples/c++/solid_mechanics_model/explicit/images/explicit.svg` | EX-019, EX-050 |
| `examples/c++/solid_mechanics_model/explicit/images/sphx_glr_explicit.png` | EX-019, EX-050 |
| `examples/c++/solid_mechanics_model/explicit/material.dat` | EX-019 |
| `examples/c++/solid_mechanics_model/implicit/CMakeLists.txt` | EX-020, EX-029 |
| `examples/c++/solid_mechanics_model/implicit/README.rst` | EX-020, EX-050 |
| `examples/c++/solid_mechanics_model/implicit/beam.geo` | EX-020 |
| `examples/c++/solid_mechanics_model/implicit/images/dynamic_analysis.png` | EX-020, EX-050 |
| `examples/c++/solid_mechanics_model/implicit/images/implicit_dynamic.svg` | EX-020, EX-050 |
| `examples/c++/solid_mechanics_model/implicit/implicit_dynamic.cc` | EX-020 |
| `examples/c++/solid_mechanics_model/implicit/material_dynamic.dat` | EX-020 |
| `examples/c++/solid_mechanics_model/io/CMakeLists.txt` | EX-029 |
| `examples/c++/solid_mechanics_model/io/README.rst` | EX-025, EX-026, EX-027, EX-050 |
| `examples/c++/solid_mechanics_model/io/dumper/CMakeLists.txt` | EX-025, EX-026, EX-029 |
| `examples/c++/solid_mechanics_model/io/dumper/dumpable_interface.cc` | EX-026 |
| `examples/c++/solid_mechanics_model/io/dumper/dumper_low_level.cc` | EX-025 |
| `examples/c++/solid_mechanics_model/io/dumper/locomotive_tools.cc` | EX-025, EX-026 |
| `examples/c++/solid_mechanics_model/io/dumper/locomotive_tools.hh` | EX-025, EX-026 |
| `examples/c++/solid_mechanics_model/io/dumper/swiss_train.geo` | EX-025, EX-026 |
| `examples/c++/solid_mechanics_model/io/images/train.gif` | EX-025, EX-026, EX-027, EX-050 |
| `examples/c++/solid_mechanics_model/io/parser/CMakeLists.txt` | EX-027, EX-029 |
| `examples/c++/solid_mechanics_model/io/parser/example_parser.cc` | EX-027 |
| `examples/c++/solid_mechanics_model/io/parser/input_file.dat` | EX-027 |
| `examples/c++/solid_mechanics_model/io/parser/swiss_cheese.geo` | EX-027 |
| `examples/c++/solid_mechanics_model/new_material/CMakeLists.txt` | EX-023, EX-024, EX-029 |
| `examples/c++/solid_mechanics_model/new_material/README.rst` | EX-023, EX-050 |
| `examples/c++/solid_mechanics_model/new_material/barre_trou.geo` | EX-023 |
| `examples/c++/solid_mechanics_model/new_material/images/barre_trou.svg` | EX-023, EX-050 |
| `examples/c++/solid_mechanics_model/new_material/local_material_damage.cc` | EX-023 |
| `examples/c++/solid_mechanics_model/new_material/local_material_damage.hh` | EX-023 |
| `examples/c++/solid_mechanics_model/new_material/local_material_damage_inline_impl.hh` | EX-023 |
| `examples/c++/solid_mechanics_model/new_material/material.dat` | EX-023 |
| `examples/c++/solid_mechanics_model/new_material/new_local_material.cc` | EX-023 |
| `examples/c++/solid_mechanics_model/new_material/viscoelastic_maxwell/CMakeLists.txt` | EX-024, EX-029 |
| `examples/c++/solid_mechanics_model/new_material/viscoelastic_maxwell/material_viscoelastic_maxwell.dat` | EX-024 |
| `examples/c++/solid_mechanics_model/new_material/viscoelastic_maxwell/material_viscoelastic_maxwell_energies.cc` | EX-024 |
| `examples/c++/solid_mechanics_model/new_material/viscoelastic_maxwell/material_viscoelastic_maxwell_mesh.geo` | EX-024 |
| `examples/c++/solid_mechanics_model/parallel/CMakeLists.txt` | EX-022, EX-029 |
| `examples/c++/solid_mechanics_model/parallel/README.rst` | EX-022, EX-050 |
| `examples/c++/solid_mechanics_model/parallel/images/parallel.png` | EX-022, EX-050 |
| `examples/c++/solid_mechanics_model/parallel/material.dat` | EX-022 |
| `examples/c++/solid_mechanics_model/parallel/parallel_2d.cc` | EX-022 |
| `examples/c++/solid_mechanics_model/parallel/square_2d.geo` | EX-022 |
| `examples/c++/solid_mechanics_model/static/CMakeLists.txt` | EX-021, EX-029 |
| `examples/c++/solid_mechanics_model/static/README.rst` | EX-021, EX-050 |
| `examples/c++/solid_mechanics_model/static/images/static_BC.svg` | EX-021, EX-050 |
| `examples/c++/solid_mechanics_model/static/images/static_displ_mag.png` | EX-021, EX-050 |
| `examples/c++/solid_mechanics_model/static/material.dat` | EX-021 |
| `examples/c++/solid_mechanics_model/static/square.geo` | EX-021 |
| `examples/c++/solid_mechanics_model/static/static.cc` | EX-021 |
| `examples/c++/structural_mechanics_model/CMakeLists.txt` | EX-029 |
| `examples/c++/structural_mechanics_model/bernoulli_beam_2_example.cc` | EX-028 |

### App.4 `examples/python` (101 files)

| File | ID(s) |
|---|---|
| `examples/python/CMakeLists.txt` | EX-029 |
| `examples/python/README.rst` | EX-050 |
| `examples/python/contact_mechanics_model/CMakeLists.txt` | EX-029 |
| `examples/python/contact_mechanics_model/README.rst` | EX-050 |
| `examples/python/contact_mechanics_model/compression/CMakeLists.txt` | EX-043, EX-044, EX-029 |
| `examples/python/contact_mechanics_model/compression/README.rst` | EX-043, EX-050 |
| `examples/python/contact_mechanics_model/compression/compression.dat` | EX-043 |
| `examples/python/contact_mechanics_model/compression/compression.geo` | EX-043 |
| `examples/python/contact_mechanics_model/compression/compression.py` | EX-043 |
| `examples/python/contact_mechanics_model/compression/detection-explicit.dat` | EX-044 |
| `examples/python/contact_mechanics_model/compression/detection-explicit.msh` | EX-044 |
| `examples/python/contact_mechanics_model/compression/detection-explicit.py` | EX-044 |
| `examples/python/contact_mechanics_model/compression/images/compression.svg` | EX-043, EX-050 |
| `examples/python/contact_mechanics_model/compression/images/contact.gif` | EX-043, EX-050 |
| `examples/python/contact_mechanics_model/friction/CMakeLists.txt` | EX-045, EX-029 |
| `examples/python/contact_mechanics_model/friction/README.rst` | EX-045, EX-050 |
| `examples/python/contact_mechanics_model/friction/bloc.geo` | EX-045 |
| `examples/python/contact_mechanics_model/friction/bloc_friction.py` | EX-045 |
| `examples/python/contact_mechanics_model/friction/material.dat` | EX-045 |
| `examples/python/phase_field_model/CMakeLists.txt` | EX-029 |
| `examples/python/phase_field_model/README.rst` | EX-050 |
| `examples/python/phase_field_model/images/phasefield-dynamic-geo.svg` | EX-042, EX-050 |
| `examples/python/phase_field_model/images/phasefield-dynamic.png` | EX-042, EX-050 |
| `examples/python/phase_field_model/images/phasefield-static-geo.svg` | EX-041, EX-050 |
| `examples/python/phase_field_model/images/phasefield-static.png` | EX-041, EX-050 |
| `examples/python/phase_field_model/material.dat` | EX-042 |
| `examples/python/phase_field_model/material_static.dat` | EX-041 |
| `examples/python/phase_field_model/phasefield-dynamic.py` | EX-042 |
| `examples/python/phase_field_model/phasefield-static.py` | EX-041 |
| `examples/python/phase_field_model/plate.geo` | EX-042 |
| `examples/python/phase_field_model/plate_static.geo` | EX-041 |
| `examples/python/solid_mechanics_cohesive_model/CMakeLists.txt` | EX-029 |
| `examples/python/solid_mechanics_cohesive_model/README.rst` | EX-050 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/CMakeLists.txt` | EX-037, EX-029 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/README.rst` | EX-037, EX-050 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/custom_material.py` | EX-038 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/images/plate.gif` | EX-037, EX-050 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/images/plate.svg` | EX-037, EX-050 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/local_material.dat` | EX-038 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/material.dat` | EX-037 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/plate.geo` | EX-037, EX-038 |
| `examples/python/solid_mechanics_cohesive_model/cohesive/plate.py` | EX-037 |
| `examples/python/solid_mechanics_cohesive_model/fragmentation/CMakeLists.txt` | EX-039, EX-029 |
| `examples/python/solid_mechanics_cohesive_model/fragmentation/README.rst` | EX-039, EX-050 |
| `examples/python/solid_mechanics_cohesive_model/fragmentation/fragmentation.py` | EX-039 |
| `examples/python/solid_mechanics_cohesive_model/fragmentation/fragmentation_mesh.geo` | EX-039 |
| `examples/python/solid_mechanics_cohesive_model/fragmentation/images/fragmentation.gif` | EX-039, EX-050 |
| `examples/python/solid_mechanics_cohesive_model/fragmentation/material.dat` | EX-039 |
| `examples/python/solid_mechanics_cohesive_model/multi-material/CMakeLists.txt` | EX-040, EX-029 |
| `examples/python/solid_mechanics_cohesive_model/multi-material/README.rst` | EX-040, EX-050 |
| `examples/python/solid_mechanics_cohesive_model/multi-material/material.dat` | EX-040 |
| `examples/python/solid_mechanics_cohesive_model/multi-material/plate.geo` | EX-040 |
| `examples/python/solid_mechanics_cohesive_model/multi-material/plate.py` | EX-040 |
| `examples/python/solid_mechanics_model/CMakeLists.txt` | EX-029 |
| `examples/python/solid_mechanics_model/README.rst` | EX-050 |
| `examples/python/solid_mechanics_model/custom-material/CMakeLists.txt` | EX-033, EX-034, EX-029 |
| `examples/python/solid_mechanics_model/custom-material/README.rst` | EX-033, EX-034, EX-050 |
| `examples/python/solid_mechanics_model/custom-material/bar.geo` | EX-033 |
| `examples/python/solid_mechanics_model/custom-material/bi-material.py` | EX-034 |
| `examples/python/solid_mechanics_model/custom-material/custom-material.py` | EX-033 |
| `examples/python/solid_mechanics_model/custom-material/images/pulse_bar_custom.gif` | EX-033, EX-034, EX-050 |
| `examples/python/solid_mechanics_model/custom-material/images/square_displ.png` | EX-033, EX-034, EX-050 |
| `examples/python/solid_mechanics_model/custom-material/material.dat` | EX-033, EX-034 |
| `examples/python/solid_mechanics_model/custom-material/square.geo` | EX-034 |
| `examples/python/solid_mechanics_model/dynamics/CMakeLists.txt` | EX-031, EX-029 |
| `examples/python/solid_mechanics_model/dynamics/README.rst` | EX-031, EX-050 |
| `examples/python/solid_mechanics_model/dynamics/bar.geo` | EX-031 |
| `examples/python/solid_mechanics_model/dynamics/dynamics.py` | EX-031 |
| `examples/python/solid_mechanics_model/dynamics/images/bar.gif` | EX-031, EX-050 |
| `examples/python/solid_mechanics_model/dynamics/images/bar_geom.svg` | EX-031, EX-050 |
| `examples/python/solid_mechanics_model/dynamics/material.dat` | EX-031 |
| `examples/python/solid_mechanics_model/eigen_modes/CMakeLists.txt` | EX-036, EX-029 |
| `examples/python/solid_mechanics_model/eigen_modes/README.rst` | EX-036, EX-050 |
| `examples/python/solid_mechanics_model/eigen_modes/eigen_modes.py` | EX-036 |
| `examples/python/solid_mechanics_model/eigen_modes/image_saver.py` | EX-036 |
| `examples/python/solid_mechanics_model/eigen_modes/images/eigen_modes.png` | EX-036, EX-050 |
| `examples/python/solid_mechanics_model/eigen_modes/material.dat` | EX-036 |
| `examples/python/solid_mechanics_model/plate-hole/CMakeLists.txt` | EX-032, EX-029 |
| `examples/python/solid_mechanics_model/plate-hole/README.rst` | EX-032, EX-050 |
| `examples/python/solid_mechanics_model/plate-hole/images/plate_hole.svg` | EX-032, EX-050 |
| `examples/python/solid_mechanics_model/plate-hole/images/plate_hole_displ_mag.png` | EX-032, EX-050 |
| `examples/python/solid_mechanics_model/plate-hole/material.dat` | EX-032 |
| `examples/python/solid_mechanics_model/plate-hole/plate.geo` | EX-032 |
| `examples/python/solid_mechanics_model/plate-hole/plate.py` | EX-032 |
| `examples/python/solid_mechanics_model/solver_callback/CMakeLists.txt` | EX-035, EX-029 |
| `examples/python/solid_mechanics_model/solver_callback/README.rst` | EX-035, EX-050 |
| `examples/python/solid_mechanics_model/solver_callback/bar.geo` | EX-035 |
| `examples/python/solid_mechanics_model/solver_callback/material.dat` | EX-035 |
| `examples/python/solid_mechanics_model/solver_callback/solver_callback.py` | EX-035 |
| `examples/python/solid_mechanics_model/stiffness_matrix/CMakeLists.txt` | EX-030, EX-029 |
| `examples/python/solid_mechanics_model/stiffness_matrix/README.rst` | EX-030, EX-050 |
| `examples/python/solid_mechanics_model/stiffness_matrix/material.dat` | EX-030 |
| `examples/python/solid_mechanics_model/stiffness_matrix/plate.geo` | EX-030 |
| `examples/python/solid_mechanics_model/stiffness_matrix/plate.msh` | EX-030 |
| `examples/python/solid_mechanics_model/stiffness_matrix/stiffness_matrix.py` | EX-030 |
| `examples/python/structural_mechanics_model/CMakeLists.txt` | EX-029 |
| `examples/python/structural_mechanics_model/images/displacements.png` | EX-046, EX-050 |
| `examples/python/structural_mechanics_model/images/displacements_softening.png` | EX-047, EX-050 |
| `examples/python/structural_mechanics_model/structural_mechanics.py` | EX-046 |
| `examples/python/structural_mechanics_model/structural_mechanics_dynamics.py` | EX-048 |
| `examples/python/structural_mechanics_model/structural_mechanics_softening.py` | EX-047 |

### App.5 `extra_packages/extra-materials/test` (33 files)

| File | ID(s) |
|---|---|
| `extra_packages/extra-materials/test/CMakeLists.txt` | TEST-233 |
| `extra_packages/extra-materials/test/test_material_FE2/CMakeLists.txt` | TEST-180…TEST-183 |
| `extra_packages/extra-materials/test/test_material_FE2/homogenized_plate.geo` | TEST-182 |
| `extra_packages/extra-materials/test/test_material_FE2/material.dat` | TEST-180 |
| `extra_packages/extra-materials/test/test_material_FE2/material_orthotropic.dat` | TEST-182 |
| `extra_packages/extra-materials/test/test_material_FE2/material_test_boundary.dat` | TEST-183 |
| `extra_packages/extra-materials/test/test_material_FE2/mesoscale_materials.dat` | TEST-181 |
| `extra_packages/extra-materials/test/test_material_FE2/one_inclusion.msh` | TEST-181 |
| `extra_packages/extra-materials/test/test_material_FE2/periodic_plate.geo` | TEST-183 |
| `extra_packages/extra-materials/test/test_material_FE2/periodic_plate.msh` | TEST-180, TEST-183 |
| `extra_packages/extra-materials/test/test_material_FE2/square.geo` | TEST-180…TEST-183 |
| `extra_packages/extra-materials/test/test_material_FE2/square.msh` | TEST-180 |
| `extra_packages/extra-materials/test/test_material_FE2/test_eigenstrain_homogenization.cc` | TEST-181 |
| `extra_packages/extra-materials/test/test_material_FE2/test_elastic_homogenization.cc` | TEST-182 |
| `extra_packages/extra-materials/test/test_material_FE2/test_material_FE2.cc` | TEST-180 |
| `extra_packages/extra-materials/test/test_material_FE2/test_node_selection.cc` | TEST-183 |
| `extra_packages/extra-materials/test/test_material_damage/CMakeLists.txt` | TEST-184…TEST-188 |
| `extra_packages/extra-materials/test/test_material_damage/energy_study.geo` | TEST-184…TEST-188 (add_mesh single_agg, no consumer) |
| `extra_packages/extra-materials/test/test_material_damage/material.dat` | TEST-184, TEST-185, TEST-186 |
| `extra_packages/extra-materials/test/test_material_damage/material_non_local.dat` | TEST-186 |
| `extra_packages/extra-materials/test/test_material_damage/material_stiffness_reduction.dat` | TEST-188 |
| `extra_packages/extra-materials/test/test_material_damage/one_circular_inclusion.geo` | TEST-184…TEST-188 |
| `extra_packages/extra-materials/test/test_material_damage/one_circular_inclusion.msh` | TEST-187 |
| `extra_packages/extra-materials/test/test_material_damage/plate.msh` | TEST-184, TEST-185, TEST-186 |
| `extra_packages/extra-materials/test/test_material_damage/test_material_damage_iterative.cc` | TEST-184, TEST-185 |
| `extra_packages/extra-materials/test/test_material_damage/test_material_damage_iterative_non_local_parallel.cc` | TEST-187 |
| `extra_packages/extra-materials/test/test_material_damage/test_material_damage_iterative_non_local_parallel.sh` | TEST-187 |
| `extra_packages/extra-materials/test/test_material_damage/test_material_damage_iterative_non_local_serial.cc` | TEST-186 |
| `extra_packages/extra-materials/test/test_material_damage/test_material_damage_iterative_parallel.sh` | TEST-185 |
| `extra_packages/extra-materials/test/test_material_damage/test_material_iterative_stiffness_reduction.cc` | TEST-188 |
| `extra_packages/extra-materials/test/test_material_damage/test_material_iterative_stiffness_reduction.verified` | TEST-188, GOLD-001 |
| `extra_packages/extra-materials/test/test_material_damage/two_elements.msh` | TEST-188 |
| `extra_packages/extra-materials/test/test_material_damage/two_materials.dat` | TEST-187 |

### App.6 `extra_packages/igfem/test` (58 files)

| File | ID(s) |
|---|---|
| `extra_packages/igfem/test/CMakeLists.txt` | TEST-233 |
| `extra_packages/igfem/test/patch_tests/CMakeLists.txt` | TEST-190…TEST-192 |
| `extra_packages/igfem/test/patch_tests/material.dat` | TEST-190, TEST-191 |
| `extra_packages/igfem/test/patch_tests/material_test_interface_position.dat` | TEST-192 |
| `extra_packages/igfem/test/patch_tests/plate.geo` | TEST-190 |
| `extra_packages/igfem/test/patch_tests/test_igfem_triangle_4.cc` | TEST-191 |
| `extra_packages/igfem/test/patch_tests/test_igfem_triangle_5.cc` | TEST-190 |
| `extra_packages/igfem/test/patch_tests/test_interface_position.cc` | TEST-192 |
| `extra_packages/igfem/test/patch_tests/test_interface_position.msh` | TEST-192 |
| `extra_packages/igfem/test/patch_tests/test_interface_position.sh` | TEST-192 |
| `extra_packages/igfem/test/patch_tests/test_mesh.msh` | TEST-191 |
| `extra_packages/igfem/test/test_fe_engine/CMakeLists.txt` | TEST-193…TEST-196 |
| `extra_packages/igfem/test/test_fe_engine/igfem_mesh_generation.cc` | TEST-193…TEST-196 |
| `extra_packages/igfem/test/test_fe_engine/mesh_info_igfem_triangle_4.txt` | TEST-193…TEST-196, GOLD-014 |
| `extra_packages/igfem/test/test_fe_engine/mesh_info_igfem_triangle_5.txt` | TEST-193…TEST-196, GOLD-014 |
| `extra_packages/igfem/test/test_fe_engine/test_fe_engine_precomputation.cc` | TEST-195 |
| `extra_packages/igfem/test/test_fe_engine/test_fe_engine_precomputation.verified` | TEST-195 |
| `extra_packages/igfem/test/test_fe_engine/test_gradient.cc` | TEST-196 |
| `extra_packages/igfem/test/test_fe_engine/test_gradient.verified` | TEST-196 |
| `extra_packages/igfem/test/test_fe_engine/test_integrate.cc` | TEST-194 |
| `extra_packages/igfem/test/test_fe_engine/test_integrate.verified` | TEST-194 |
| `extra_packages/igfem/test/test_fe_engine/test_interpolate.cc` | TEST-193 |
| `extra_packages/igfem/test/test_fe_engine/test_interpolate.verified` | TEST-193 |
| `extra_packages/igfem/test/test_geometry/CMakeLists.txt` | TEST-197…TEST-199 |
| `extra_packages/igfem/test/test_geometry/test_generation_igfem_mesh_from_sphere.cc` | TEST-199 |
| `extra_packages/igfem/test/test_geometry/test_generation_igfem_mesh_from_sphere_static.cc` | TEST-198 |
| `extra_packages/igfem/test/test_geometry/test_geometry_triangle.msh` | TEST-197…TEST-199 |
| `extra_packages/igfem/test/test_geometry/test_spherical_kernel.cc` | TEST-197 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/CMakeLists.txt` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/fine_mesh.msh` | TEST-201, TEST-202 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/igfem_mesh.msh` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/material.dat` | TEST-200, TEST-203 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/material_ASR.dat` | TEST-204 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/material_damage.dat` | TEST-201, TEST-202 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/material_stiffness_reduction.dat` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/material_stiffness_reduction_2.dat` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/material_stiffness_reduction_weibull.dat` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/one_inclusion.msh` | TEST-204 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/plate.msh` | TEST-200 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/regular_mesh.msh` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_ASR_damage.cc` | TEST-204 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_damage_transfer.msh` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_eigenstrain.cc` | TEST-202 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_igfem_element_orientation.cc` | TEST-203 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_igfem_element_orientation.msh` | TEST-203 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_igfem_element_orientation.sh` | TEST-203 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_igfem_element_orientation_parallel.sh` | TEST-203 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction.cc` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction.sh` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction_damage_step_transfer.cc` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction_parallel.cc` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction_tangent_transfer.cc` | TEST-205 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_solid_mechanics_model_igfem.cc` | TEST-200 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_solid_mechanics_model_igfem.sh` | TEST-200 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_solid_mechanics_model_igfem.verified` | TEST-200 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_transfer_internals.cc` | TEST-201 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_transfer_internals.sh` | TEST-201 |
| `extra_packages/igfem/test/test_solid_mechanics_model_igfem/test_volume_computation.cc` | TEST-205 |

### App.7 `extra_packages/traction-at-split-node-contact/test` (28 files)

| File | ID(s) |
|---|---|
| `extra_packages/traction-at-split-node-contact/test/CMakeLists.txt` | TEST-233 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/CMakeLists.txt` | TEST-175…TEST-177, TEST-233 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_dieterich_ageing.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_dieterich_ageing_regularized.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_dieterich_slip.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_n_shaped_ageing.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_n_shaped_ageing_regularized.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_n_shaped_slip.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_regularized_dieterich_ageing.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_regularized_dieterich_ageing_regularized.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_regularized_dieterich_slip.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_dieterich_ageing.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_dieterich_ageing_regularized.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_dieterich_slip.dat` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_n_shaped_ageing.dat` | TEST-177 (commented out), GOLD-015 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_n_shaped_ageing_regularized.dat` | TEST-177 (commented out), GOLD-015 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_rate_and_state_revised_n_shaped_slip.dat` | TEST-177 (commented out), GOLD-015 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_friction_slip_weakening.dat` | TEST-176 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/data/material_ntn_model.dat` | TEST-175 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/ntn_test_2d.msh` | TEST-175 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/ntn_test_3d.msh` | TEST-175 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/ntn_test_compression.msh` | TEST-176 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/ntn_test_ras.geo` | TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_basics.cc` | TEST-175 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_compression.verified` | GOLD-005 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_fixture.hh` | TEST-175 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_friction.cc` | TEST-176, TEST-177 |
| `extra_packages/traction-at-split-node-contact/test/test_ntn_model/test_ntn_friction_fixture.hh` | TEST-176, TEST-177 |

### App.8 `test (top level)` (3 files)

| File | ID(s) |
|---|---|
| `test/CMakeLists.txt` | TEST-233 |
| `test/test_gtest_main.cc` | TEST-233 |
| `test/test_gtest_utils.hh` | TEST-233 |

### App.9 `test/ci` (23 files)

| File | ID(s) |
|---|---|
| `test/ci/code_quality:latest/Dockerfile` | CI-015 |
| `test/ci/ctest2junit.xsl` | CI-020 |
| `test/ci/debian:stable/.openmpi/mca-params.conf` | CI-011 |
| `test/ci/debian:stable/Dockerfile` | CI-011 |
| `test/ci/includes_for_ci/aka_config.hh` | TEST-234 |
| `test/ci/includes_for_ci/aka_element_classes_info.hh` | TEST-234 |
| `test/ci/macosx/build_env.sh` | CI-014 |
| `test/ci/macosx/configure_env.sh` | CI-014 |
| `test/ci/macosx/make-wheels.sh` | CI-014 |
| `test/ci/macosx/spack.yaml` | CI-014 |
| `test/ci/manylinux_2_28_x86_64:latest/Dockerfile` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/Toolchain.cmake` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/build_env.sh` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/compilers.yaml` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/configure_env.sh` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/install-pip.sh` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/make-wheels.sh` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/packages.yaml` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/repos.yaml` | CI-013 |
| `test/ci/manylinux_2_28_x86_64:latest/spack.yaml` | CI-013 |
| `test/ci/ompi_init.supp` | CI-025 |
| `test/ci/ubuntu:lts/.openmpi/mca-params.conf` | CI-012 |
| `test/ci/ubuntu:lts/Dockerfile` | CI-012 |

### App.10 `test/python_fe` (5 files)

| File | ID(s) |
|---|---|
| `test/python_fe/__init__.py` | TEST-232 |
| `test/python_fe/dynamic_solver.py` | TEST-232 |
| `test/python_fe/fe.py` | TEST-232 |
| `test/python_fe/static_solver.py` | TEST-232 |
| `test/python_fe/truss_fe.py` | TEST-232 |

### App.11 `test/test_common` (7 files)

| File | ID(s) |
|---|---|
| `test/test_common/CMakeLists.txt` | TEST-001…TEST-005, TEST-233 |
| `test/test_common/circle.geo` | TEST-001 |
| `test/test_common/test_array.cc` | TEST-003 |
| `test/test_common/test_csr.cc` | TEST-002 |
| `test/test_common/test_grid.cc` | TEST-001 |
| `test/test_common/test_tensors.cc` | TEST-004 |
| `test/test_common/test_voigt_helper.cc` | TEST-005 |

### App.12 `test/test_fe_engine` (60 files)

| File | ID(s) |
|---|---|
| `test/test_fe_engine/CMakeLists.txt` | TEST-010…TEST-021, TEST-233 |
| `test/test_fe_engine/_bernoulli_beam_2.msh` | TEST-015, MDATA-001 |
| `test/test_fe_engine/_bernoulli_beam_3.msh` | TEST-015, MDATA-001 |
| `test/test_fe_engine/_discrete_kirchhoff_triangle_18.msh` | TEST-015, MDATA-001 |
| `test/test_fe_engine/_hexahedron_20.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_hexahedron_20.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_hexahedron_8.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_hexahedron_8.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_pentahedron_15.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_pentahedron_15.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_pentahedron_6.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_pentahedron_6.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_quadrangle_4.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_quadrangle_4.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_quadrangle_8.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_quadrangle_8.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_segment_2.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_segment_2.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_segment_3.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_segment_3.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_tetrahedron_10.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_tetrahedron_10.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_tetrahedron_4.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_tetrahedron_4.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_triangle_3.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_triangle_3.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/_triangle_6.csv` | TEST-016, GOLD-002 |
| `test/test_fe_engine/_triangle_6.msh` | TEST-014…TEST-021, MDATA-001 |
| `test/test_fe_engine/circle.geo` | MDATA-001 (source geometries of committed element meshes; not used by CMake) |
| `test/test_fe_engine/cube.geo` | TEST-012, TEST-013 |
| `test/test_fe_engine/cube_physical_names.geo` | TEST-012, TEST-013 |
| `test/test_fe_engine/generate_test` | TEST-213 |
| `test/test_fe_engine/hexa_structured.geo` | MDATA-001 (source geometries of committed element meshes; not used by CMake) |
| `test/test_fe_engine/line.geo` | MDATA-001 (source geometries of committed element meshes; not used by CMake) |
| `test/test_fe_engine/pentahedron.geo` | MDATA-001 (source geometries of committed element meshes; not used by CMake) |
| `test/test_fe_engine/pentahedron.msh` | MDATA-001 (unused committed mesh), GOLD-015 |
| `test/test_fe_engine/py_engine/__init__.py` | TEST-014, TEST-232 |
| `test/test_fe_engine/py_engine/py_engine.py` | TEST-014, TEST-232 |
| `test/test_fe_engine/segment.geo` | MDATA-001 (source geometries of committed element meshes; not used by CMake) |
| `test/test_fe_engine/segment.msh` | MDATA-001 (unused committed mesh), GOLD-015 |
| `test/test_fe_engine/square.geo` | MDATA-001 (source geometries of committed element meshes; not used by CMake) |
| `test/test_fe_engine/square_structured.geo` | MDATA-001 (source geometries of committed element meshes; not used by CMake) |
| `test/test_fe_engine/test_facet_element_mapping.cc` | TEST-013 |
| `test/test_fe_engine/test_fe_engine_fixture.hh` | TEST-014, TEST-016…TEST-021, TEST-210 |
| `test/test_fe_engine/test_fe_engine_gauss_integration.cc` | TEST-017 |
| `test/test_fe_engine/test_fe_engine_inradius.cc` | TEST-016 |
| `test/test_fe_engine/test_fe_engine_precomputation.cc` | TEST-014 |
| `test/test_fe_engine/test_fe_engine_precomputation_bernoulli_2.cc` | TEST-212 |
| `test/test_fe_engine/test_fe_engine_precomputation_bernoulli_3.cc` | TEST-212 |
| `test/test_fe_engine/test_fe_engine_precomputation_structural.cc` | TEST-015 |
| `test/test_fe_engine/test_fe_engine_structural_fixture.hh` | TEST-015 |
| `test/test_fe_engine/test_gradient.cc` | TEST-018 |
| `test/test_fe_engine/test_integrate.cc` | TEST-019 |
| `test/test_fe_engine/test_interpolate.cc` | TEST-210 |
| `test/test_fe_engine/test_interpolate_bernoulli_beam_2.cc` | TEST-211 |
| `test/test_fe_engine/test_interpolate_bernoulli_beam_2.verified` | TEST-211, GOLD-003 |
| `test/test_fe_engine/test_inverse_map.cc` | TEST-020 |
| `test/test_fe_engine/test_mesh_boundary.cc` | TEST-012 |
| `test/test_fe_engine/test_mesh_data.cc` | TEST-010, TEST-011 |
| `test/test_fe_engine/test_variational_operators.cc` | TEST-021 |

### App.13 `test/test_geometry` (7 files)

| File | ID(s) |
|---|---|
| `test/test_geometry/CMakeLists.txt` | TEST-025…TEST-028, TEST-233 |
| `test/test_geometry/test_geometry_intersection.cc` | TEST-026 |
| `test/test_geometry/test_geometry_predicates.cc` | TEST-025 |
| `test/test_geometry/test_geometry_tetrahedron.msh` | TEST-028, MDATA-005 |
| `test/test_geometry/test_geometry_triangle.msh` | TEST-026, TEST-027, MDATA-005 |
| `test/test_geometry/test_segment_intersection_tetrahedron_4.cc` | TEST-028 |
| `test/test_geometry/test_segment_intersection_triangle_3.cc` | TEST-027 |

### App.14 `test/test_io` (11 files)

| File | ID(s) |
|---|---|
| `test/test_io/CMakeLists.txt` | TEST-233 |
| `test/test_io/test_dumper/CMakeLists.txt` | TEST-030, TEST-233 |
| `test/test_io/test_dumper/input_file.dat` | TEST-030 |
| `test/test_io/test_dumper/test_dumper.cc` | TEST-030 |
| `test/test_io/test_dumper/test_dumper.msh` | TEST-030, MDATA-005 |
| `test/test_io/test_parser/CMakeLists.txt` | TEST-031…TEST-033, TEST-233 |
| `test/test_io/test_parser/input_file.dat` | TEST-031, TEST-033 |
| `test/test_io/test_parser/test_parser.cc` | TEST-031 |
| `test/test_io/test_parser/test_parser.py` | TEST-033 |
| `test/test_io/test_parser/test_parser.verified` | TEST-031, GOLD-001 |
| `test/test_io/test_parser/test_parser_gtest.cc` | TEST-032 |

### App.15 `test/test_mesh_utils` (47 files)

| File | ID(s) |
|---|---|
| `test/test_mesh_utils/CMakeLists.txt` | TEST-035, TEST-036, TEST-233 |
| `test/test_mesh_utils/iterators_mesh.geo` | TEST-036 |
| `test/test_mesh_utils/purify_mesh.msh` | TEST-035 |
| `test/test_mesh_utils/test_buildfacets/CMakeLists.txt` | TEST-039, TEST-233 |
| `test/test_mesh_utils/test_buildfacets/_hexahedron_20.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_hexahedron_20_pentahedron_15.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_hexahedron_8.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_hexahedron_8_pentahedron_6.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_pentahedron_15.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_pentahedron_6.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_quadrangle_4.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_quadrangle_4_triangle_3.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_quadrangle_8.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_quadrangle_8_triangle_6.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_tetrahedron_10.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_triangle_3.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/_triangle_6.msh` | TEST-039, MDATA-002 |
| `test/test_mesh_utils/test_buildfacets/test_buildfacets.cc` | TEST-039 |
| `test/test_mesh_utils/test_buildfacets/test_buildfacets_gtest.verified` | TEST-039, GOLD-001 |
| `test/test_mesh_utils/test_mesh_io/CMakeLists.txt` | TEST-037, TEST-038, TEST-233 |
| `test/test_mesh_utils/test_mesh_io/cube.geo` | TEST-037 |
| `test/test_mesh_utils/test_mesh_io/cube_physical_names.geo` | TEST-038 |
| `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh.cc` | TEST-037 |
| `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh_physical_names.cc` | TEST-038 |
| `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh_physical_names.verified` | TEST-038, GOLD-001 |
| `test/test_mesh_utils/test_mesh_iterators.cc` | TEST-036 |
| `test/test_mesh_utils/test_mesh_partitionate/CMakeLists.txt` | TEST-040, TEST-041, TEST-233 |
| `test/test_mesh_utils/test_mesh_partitionate/quad.geo` | TEST-041 |
| `test/test_mesh_utils/test_mesh_partitionate/test_mesh_partitionate_mesh_data.cc` | TEST-041 |
| `test/test_mesh_utils/test_mesh_partitionate/test_mesh_partitionate_scotch.cc` | TEST-040 |
| `test/test_mesh_utils/test_mesh_partitionate/triangle.geo` | TEST-040 |
| `test/test_mesh_utils/test_mesh_periodic/CMakeLists.txt` | TEST-042…TEST-044, TEST-233 |
| `test/test_mesh_utils/test_mesh_periodic/cube.geo` | TEST-042, TEST-043, TEST-044 |
| `test/test_mesh_utils/test_mesh_periodic/line.geo` | TEST-042 |
| `test/test_mesh_utils/test_mesh_periodic/material.dat` | TEST-043, TEST-044 |
| `test/test_mesh_utils/test_mesh_periodic/square.geo` | TEST-042 |
| `test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic.cc` | TEST-042 |
| `test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic_fixture.hh` | TEST-042 |
| `test/test_mesh_utils/test_mesh_periodic/test_periodic_mass.cc` | TEST-044 |
| `test/test_mesh_utils/test_mesh_periodic/test_periodic_static.cc` | TEST-043 |
| `test/test_mesh_utils/test_mesh_periodic/test_periodic_static_fixture.hh` | TEST-043 |
| `test/test_mesh_utils/test_purify_mesh.cc` | TEST-035 |
| `test/test_mesh_utils/test_segment_nodetype/CMakeLists.txt` | TEST-214 |
| `test/test_mesh_utils/test_segment_nodetype/mesh.geo` | TEST-214 |
| `test/test_mesh_utils/test_segment_nodetype/mesh.msh` | TEST-214 |
| `test/test_mesh_utils/test_segment_nodetype/test_segment_nodetype.cc` | TEST-214 |
| `test/test_mesh_utils/test_segment_nodetype/test_segment_nodetype.sh` | TEST-214 |

### App.16 `test/test_model` (386 files)

| File | ID(s) |
|---|---|
| `test/test_model/CMakeLists.txt` | TEST-233 |
| `test/test_model/patch_tests/CMakeLists.txt` | TEST-050…TEST-063, TEST-233 |
| `test/test_model/patch_tests/data/CMakeLists.txt` | MDATA-003, TEST-233 |
| `test/test_model/patch_tests/data/_hexahedron_20.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_hexahedron_8.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_pentahedron_15.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_pentahedron_6.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_quadrangle_4.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_quadrangle_8.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_segment_2.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_segment_3.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_tetrahedron_10.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_tetrahedron_4.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_triangle_3.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/_triangle_6.msh` | MDATA-003 (TEST-050…TEST-063) |
| `test/test_model/patch_tests/data/bar_hexahedron.geo` | MDATA-003 (TEST-109, TEST-128…TEST-131) |
| `test/test_model/patch_tests/data/bar_pentahedron.geo` | MDATA-003 (TEST-109, TEST-128…TEST-131) |
| `test/test_model/patch_tests/data/bar_quadrangle.geo` | MDATA-003 (TEST-109, TEST-128…TEST-131) |
| `test/test_model/patch_tests/data/bar_segment.geo` | MDATA-003 (TEST-109, TEST-128…TEST-131) |
| `test/test_model/patch_tests/data/bar_tetrahedron.geo` | MDATA-003 (TEST-109, TEST-128…TEST-131) |
| `test/test_model/patch_tests/data/bar_triangle.geo` | MDATA-003 (TEST-109, TEST-128…TEST-131) |
| `test/test_model/patch_tests/data/heat_transfer_input.dat` | TEST-055…TEST-057, TEST-061…TEST-063 |
| `test/test_model/patch_tests/data/material.dat` | TEST-050…TEST-054 (generic material file, unused), GOLD-015 |
| `test/test_model/patch_tests/data/material_anisotropic_1.dat` | TEST-052, TEST-053, TEST-060 |
| `test/test_model/patch_tests/data/material_anisotropic_2.dat` | TEST-052, TEST-053, TEST-060 |
| `test/test_model/patch_tests/data/material_anisotropic_3.dat` | TEST-052, TEST-053, TEST-060 |
| `test/test_model/patch_tests/data/material_check_stress_plane_strain.dat` | TEST-050, TEST-051, TEST-058, TEST-059 |
| `test/test_model/patch_tests/data/material_check_stress_plane_strain_finite_deformation.dat` | TEST-050, TEST-051, TEST-058, TEST-059 |
| `test/test_model/patch_tests/data/material_check_stress_plane_stress.dat` | TEST-050, TEST-051, TEST-058, TEST-059 |
| `test/test_model/patch_tests/data/material_check_stress_plane_stress_finite_deformation.dat` | TEST-050, TEST-051, TEST-058, TEST-059 |
| `test/test_model/patch_tests/data/material_lumped.dat` | TEST-054 |
| `test/test_model/patch_tests/patch_test_linear_anisotropic_explicit.cc` | TEST-052 |
| `test/test_model/patch_tests/patch_test_linear_anisotropic_implicit.cc` | TEST-053 |
| `test/test_model/patch_tests/patch_test_linear_elastic_explicit.cc` | TEST-050 |
| `test/test_model/patch_tests/patch_test_linear_elastic_implicit.cc` | TEST-051 |
| `test/test_model/patch_tests/patch_test_linear_fixture.hh` | TEST-050…TEST-054 |
| `test/test_model/patch_tests/patch_test_linear_fixture.py` | TEST-058…TEST-063 |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_explicit.cc` | TEST-055 |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_fixture.hh` | TEST-055…TEST-057 |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_fixture.py` | TEST-061…TEST-063 |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_implicit.cc` | TEST-057 |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_static.cc` | TEST-056 |
| `test/test_model/patch_tests/patch_test_linear_solid_mechanics_fixture.hh` | TEST-050…TEST-054 |
| `test/test_model/patch_tests/patch_test_linear_solid_mechanics_fixture.py` | TEST-058…TEST-060 |
| `test/test_model/patch_tests/test_lumped_mass.cc` | TEST-054 |
| `test/test_model/patch_tests/test_patch_linear_anisotropic_explicit.py` | TEST-060 |
| `test/test_model/patch_tests/test_patch_linear_elastic_explicit.py` | TEST-058 |
| `test/test_model/patch_tests/test_patch_linear_elastic_static.py` | TEST-059 |
| `test/test_model/patch_tests/test_patch_linear_heat_transfer_explicit.py` | TEST-061 |
| `test/test_model/patch_tests/test_patch_linear_heat_transfer_implicit.py` | TEST-063 |
| `test/test_model/patch_tests/test_patch_linear_heat_transfer_static.py` | TEST-062 |
| `test/test_model/test_common/CMakeLists.txt` | TEST-070, TEST-071, TEST-233 |
| `test/test_model/test_common/mesh.geo` | TEST-070, TEST-071 |
| `test/test_model/test_common/test_dof_manager.cc` | TEST-070 |
| `test/test_model/test_common/test_dof_mesh_distribute.cc` | TEST-071 |
| `test/test_model/test_common/test_model_solver/CMakeLists.txt` | TEST-072…TEST-074, TEST-233 |
| `test/test_model/test_common/test_model_solver/test_dof_manager_default.cc` | TEST-072 |
| `test/test_model/test_common/test_model_solver/test_dof_manager_default.verified` | TEST-072, GOLD-001 |
| `test/test_model/test_common/test_model_solver/test_model_solver.cc` | TEST-073, TEST-074 |
| `test/test_model/test_common/test_model_solver/test_model_solver.py` | TEST-232 |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic.cc` | TEST-074 |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic.py` | TEST-232 |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic_explicit.verified` | TEST-074, GOLD-001 |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic_implicit.verified` | TEST-074, GOLD-001 |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic_petsc.cc` | TEST-231 |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic_petsc.verified` | TEST-074, GOLD-001 |
| `test/test_model/test_common/test_model_solver/test_model_solver_mumps.verified` | TEST-073, GOLD-001 |
| `test/test_model/test_common/test_model_solver/test_model_solver_my_model.hh` | TEST-073, TEST-074 |
| `test/test_model/test_common/test_model_solver/test_model_solver_petsc.verified` | TEST-073, GOLD-001 |
| `test/test_model/test_common/test_non_local_toolbox/CMakeLists.txt` | TEST-075…TEST-080, TEST-233 |
| `test/test_model/test_common/test_non_local_toolbox/fine_mesh.msh` | TEST-075…TEST-080 (unused mesh), GOLD-015 |
| `test/test_model/test_common/test_non_local_toolbox/material.dat` | TEST-075 |
| `test/test_model/test_common/test_non_local_toolbox/material_avg.dat` | TEST-077 |
| `test/test_model/test_common/test_non_local_toolbox/material_parallel_test.dat` | TEST-079 |
| `test/test_model/test_common/test_non_local_toolbox/material_remove_damage.dat` | TEST-078, TEST-080 |
| `test/test_model/test_common/test_non_local_toolbox/material_weight_computation.dat` | TEST-076 |
| `test/test_model/test_common/test_non_local_toolbox/my_model.hh` | TEST-075 |
| `test/test_model/test_common/test_non_local_toolbox/pair_test.msh` | TEST-080 |
| `test/test_model/test_common/test_non_local_toolbox/parallel_test.msh` | TEST-079 |
| `test/test_model/test_common/test_non_local_toolbox/plate.geo` | TEST-075…TEST-078 (source of committed plate.msh), MDATA-005 |
| `test/test_model/test_common/test_non_local_toolbox/plate.msh` | TEST-075…TEST-078, MDATA-005 |
| `test/test_model/test_common/test_non_local_toolbox/plot_neighborhoods.py` | TEST-075 |
| `test/test_model/test_common/test_non_local_toolbox/test_build_neighborhood_parallel.cc` | TEST-079 |
| `test/test_model/test_common/test_non_local_toolbox/test_material.cc` | TEST-077, TEST-079 |
| `test/test_model/test_common/test_non_local_toolbox/test_material.hh` | TEST-077, TEST-079 |
| `test/test_model/test_common/test_non_local_toolbox/test_material_damage.cc` | TEST-078, TEST-080 |
| `test/test_model/test_common/test_non_local_toolbox/test_material_damage.hh` | TEST-078, TEST-080 |
| `test/test_model/test_common/test_non_local_toolbox/test_non_local_averaging.cc` | TEST-077 |
| `test/test_model/test_common/test_non_local_toolbox/test_non_local_neighborhood_base.cc` | TEST-075 |
| `test/test_model/test_common/test_non_local_toolbox/test_non_local_neighborhood_base.verified` | TEST-075, GOLD-001 |
| `test/test_model/test_common/test_non_local_toolbox/test_pair_computation.cc` | TEST-080 |
| `test/test_model/test_common/test_non_local_toolbox/test_remove_damage_weight_function.cc` | TEST-078 |
| `test/test_model/test_common/test_non_local_toolbox/test_remove_damage_weight_function.verified` | TEST-078, GOLD-001 |
| `test/test_model/test_common/test_non_local_toolbox/test_weight_computation.cc` | TEST-076 |
| `test/test_model/test_common/test_non_local_toolbox/test_weight_computation.verified` | TEST-076, GOLD-001 |
| `test/test_model/test_contact_mechanics_model/CMakeLists.txt` | TEST-085, TEST-086, TEST-219, TEST-233 |
| `test/test_model/test_contact_mechanics_model/flat_on_flat.geo` | TEST-085 |
| `test/test_model/test_contact_mechanics_model/material-friction.dat` | TEST-086 |
| `test/test_model/test_contact_mechanics_model/material.dat` | TEST-085 |
| `test/test_model/test_contact_mechanics_model/sliding-block-2D.geo` | TEST-086 |
| `test/test_model/test_contact_mechanics_model/test_coupler/CMakeLists.txt` | TEST-215, TEST-216 |
| `test/test_model/test_contact_mechanics_model/test_coupler/coupling.geo` | TEST-215 |
| `test/test_model/test_contact_mechanics_model/test_coupler/flat_on_flat.geo` | TEST-216 |
| `test/test_model/test_contact_mechanics_model/test_coupler/material.dat` | TEST-215 |
| `test/test_model/test_contact_mechanics_model/test_coupler/material_stiffness.dat` | TEST-216 |
| `test/test_model/test_contact_mechanics_model/test_coupler/test_contact_coupling.cc` | TEST-215 |
| `test/test_model/test_contact_mechanics_model/test_coupler/test_coupled_stiffness.cc` | TEST-216 |
| `test/test_model/test_contact_mechanics_model/test_detector/CMakeLists.txt` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/cohesive_1D.geo` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/cohesive_strait_2D.geo` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/cohesive_strait_2D.msh` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/cohesive_strait_2D_mixte.geo` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/cohesive_strait_2D_structured.geo` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/cohesive_strait_3D.geo` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/cohesive_strait_3D_structured.geo` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/data/test.msh` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/node.geo` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/options.dat` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/test_detection.cc` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_detector/test_detection_fixture.hh` | TEST-217 |
| `test/test_model/test_contact_mechanics_model/test_explicit_dynamic.cc` | TEST-085 |
| `test/test_model/test_contact_mechanics_model/test_explicit_friction.cc` | TEST-086 |
| `test/test_model/test_contact_mechanics_model/test_resolution/CMakeLists.txt` | TEST-218 |
| `test/test_model/test_contact_mechanics_model/test_resolution/explicit_2d.geo` | TEST-218 |
| `test/test_model/test_contact_mechanics_model/test_resolution/options.dat` | TEST-218 |
| `test/test_model/test_contact_mechanics_model/test_resolution/test_explicit_resolution.cc` | TEST-218 |
| `test/test_model/test_contact_mechanics_model/test_resolution/test_resolution.cc` | TEST-218 |
| `test/test_model/test_contact_mechanics_model/test_selector/CMakeLists.txt` | TEST-219 |
| `test/test_model/test_contact_mechanics_model/test_selector/data/selection_1D.geo` | TEST-219 |
| `test/test_model/test_contact_mechanics_model/test_selector/data/selection_2D.geo` | TEST-219 |
| `test/test_model/test_contact_mechanics_model/test_selector/data/selection_3D.geo` | TEST-219 |
| `test/test_model/test_contact_mechanics_model/test_selector/material.dat` | TEST-219 |
| `test/test_model/test_contact_mechanics_model/test_selector/test_selection.cc` | TEST-219 |
| `test/test_model/test_contact_mechanics_model/test_selector/test_selection_fixture.hh` | TEST-219 |
| `test/test_model/test_phase_field_model/CMakeLists.txt` | TEST-090…TEST-100, TEST-233 |
| `test/test_model/test_phase_field_model/material_coupling.dat` | TEST-091, TEST-096 |
| `test/test_model/test_phase_field_model/material_deviatoric.dat` | TEST-093 |
| `test/test_model/test_phase_field_model/material_hybrid.dat` | TEST-092 |
| `test/test_model/test_phase_field_model/material_linear.dat` | TEST-099 |
| `test/test_model/test_phase_field_model/material_linear_tao.dat` | TEST-100 |
| `test/test_model/test_phase_field_model/material_multiple.dat` | TEST-097 |
| `test/test_model/test_phase_field_model/material_optimal.dat` | TEST-094 |
| `test/test_model/test_phase_field_model/material_optimal_linear.dat` | TEST-095 |
| `test/test_model/test_phase_field_model/material_penalization.dat` | TEST-092, TEST-098 |
| `test/test_model/test_phase_field_model/phasefield_selector.dat` | TEST-090 |
| `test/test_model/test_phase_field_model/phasefield_selector.msh` | TEST-090 |
| `test/test_model/test_phase_field_model/test_four_elements.geo` | TEST-091…TEST-100 (sources of committed meshes), MDATA-005 |
| `test/test_model/test_phase_field_model/test_four_elements.msh` | TEST-098 |
| `test/test_model/test_phase_field_model/test_multi_material.cc` | TEST-097 |
| `test/test_model/test_phase_field_model/test_one_element.geo` | TEST-091…TEST-100 (sources of committed meshes), MDATA-005 |
| `test/test_model/test_phase_field_model/test_one_element.msh` | TEST-091…TEST-093, TEST-096, TEST-099, TEST-100, MDATA-005 |
| `test/test_model/test_phase_field_model/test_optimal_damage.msh` | TEST-094, TEST-095, MDATA-005 |
| `test/test_model/test_phase_field_model/test_penalization.cc` | TEST-098 |
| `test/test_model/test_phase_field_model/test_phase_field_anisotropic.cc` | TEST-092 |
| `test/test_model/test_phase_field_model/test_phase_field_deviatoric_split.cc` | TEST-093 |
| `test/test_model/test_phase_field_model/test_phase_field_linear.cc` | TEST-099 |
| `test/test_model/test_phase_field_model/test_phase_field_optimal.cc` | TEST-094 |
| `test/test_model/test_phase_field_model/test_phase_field_optimal_linear.cc` | TEST-095 |
| `test/test_model/test_phase_field_model/test_phase_field_tao.cc` | TEST-100 |
| `test/test_model/test_phase_field_model/test_phase_solid_coupling.cc` | TEST-091 |
| `test/test_model/test_phase_field_model/test_phase_solid_explicit.cc` | TEST-096 |
| `test/test_model/test_phase_field_model/test_phasefield_selector.cc` | TEST-090 |
| `test/test_model/test_phase_field_model/test_two_element.geo` | TEST-091…TEST-100 (sources of committed meshes), MDATA-005 |
| `test/test_model/test_phase_field_model/test_two_element.msh` | TEST-097 |
| `test/test_model/test_solid_mechanics_model/CMakeLists.txt` | TEST-105…TEST-109, TEST-233 |
| `test/test_model/test_solid_mechanics_model/cube_3d_tet_4.msh` | TEST-106, TEST-107, MDATA-005 |
| `test/test_model/test_solid_mechanics_model/cube_two_materials.geo` | TEST-105 |
| `test/test_model/test_solid_mechanics_model/material_elastic.dat` | TEST-107 |
| `test/test_model/test_solid_mechanics_model/material_elastic_plane_strain.dat` | TEST-106 |
| `test/test_model/test_solid_mechanics_model/material_selector.dat` | TEST-108 |
| `test/test_model/test_solid_mechanics_model/material_selector.geo` | TEST-108 (source of committed mesh), MDATA-005 |
| `test/test_model/test_solid_mechanics_model/material_selector.msh` | TEST-108 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/CMakeLists.txt` | TEST-115, TEST-233 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/cohesive_1D.geo` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/cohesive_strait_2D.geo` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/cohesive_strait_2D.msh` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/cohesive_strait_2D_mixte.geo` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/cohesive_strait_2D_structured.geo` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/cohesive_strait_3D.geo` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/cohesive_strait_3D_structured.geo` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/data/test.msh` | TEST-115, MDATA-004 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/material.dat` | TEST-115 (unused), GOLD-015 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/material_0.dat` | TEST-115 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/material_0_finite_def.dat` | TEST-115 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/material_1.dat` | TEST-115 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive.cc` | TEST-115 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_1d_element/CMakeLists.txt` | TEST-220 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_1d_element/bar.geo` | TEST-220 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_1d_element/material.dat` | TEST-220 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_1d_element/test_cohesive_1d_element.cc` | TEST-220 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_buildfragments/CMakeLists.txt` | TEST-116 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_buildfragments/material.dat` | TEST-116 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_buildfragments/mesh.geo` | TEST-116 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_buildfragments/test_cohesive_buildfragments.cc` | TEST-116 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/CMakeLists.txt` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/fatigue.msh` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/material.dat` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/material_fatigue.dat` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/quadrangle.geo` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic.cc` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic_fatigue.cc` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic_quadrangle.cc` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic_tetrahedron.cc` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/tetrahedron.geo` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic/triangle.geo` | TEST-221 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/CMakeLists.txt` | TEST-222 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/K_matrix_verified.dat` | TEST-222, GOLD-006 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/material.dat` | TEST-222 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/quadrangle.msh` | TEST-222 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/test_assembling_K_cohe_elements.cc` | TEST-222 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/test_assembling_K_cohe_elements.sh` | TEST-222 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_fixture.hh` | TEST-115 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/3d_spherical_inclusion.geo` | TEST-117 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/CMakeLists.txt` | TEST-117, TEST-118 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/bar.geo` | TEST-118 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/input_file.dat` | TEST-117 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/material.dat` | TEST-118 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/test_cohesive_insertion_along_physical_surfaces.cc` | TEST-117 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/test_group_update.cc` | TEST-118 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/CMakeLists.txt` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/material.dat` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/material_tetrahedron.dat` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/quadrangle.geo` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic.cc` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic_quadrangle.cc` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic_tetrahedron.cc` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic_tetrahedron_fragmentation.cc` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/tetrahedron.geo` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/tetrahedron_full.geo` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic/triangle.geo` | TEST-223 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic_impl/CMakeLists.txt` | TEST-224 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic_impl/implicit.msh` | TEST-224 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic_impl/material.dat` | TEST-224 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_intrinsic_impl/test_cohesive_intrinsic_impl.cc` | TEST-224 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_linear_friction/CMakeLists.txt` | TEST-225 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_linear_friction/material.dat` | TEST-225 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_linear_friction/mesh_cohesive_friction.msh` | TEST-225 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_linear_friction/test_cohesive_friction.cc` | TEST-225 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_materials/CMakeLists.txt` | TEST-119 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_materials/test_material_cohesive_fixture.hh` | TEST-119 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_materials/test_material_cohesive_linear.cc` | TEST-119 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/CMakeLists.txt` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/CMakeLists.txt` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/material.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/test_cohesive_facet_stress_synchronizer.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/test_cohesive_facet_stress_synchronizer.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/tetrahedron.geo` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_buildfragments/CMakeLists.txt` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_buildfragments/material.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_buildfragments/mesh.geo` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_buildfragments/test_cohesive_parallel_buildfragments.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_buildfragments/test_cohesive_parallel_buildfragments.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/CMakeLists.txt` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/material.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/mesh.geo` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic_tetrahedron.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic_tetrahedron.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic_tetrahedron_displacement.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic_tetrahedron_displacement.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/tetrahedron.geo` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic_IG_TG/CMakeLists.txt` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic_IG_TG/material.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic_IG_TG/square.msh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic_IG_TG/test_cohesive_parallel_extrinsic_IG_TG.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic_IG_TG/test_cohesive_parallel_extrinsic_IG_TG.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/2d_basic_interface.msh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/3d_spherical_inclusion.geo` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/CMakeLists.txt` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/input_file.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/input_file_iii.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_0_out_of_2.out` | TEST-226, GOLD-007 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_0_out_of_4.out` | TEST-226, GOLD-007 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_1_out_of_2.out` | TEST-226, GOLD-007 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_1_out_of_4.out` | TEST-226, GOLD-007 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_2_out_of_4.out` | TEST-226, GOLD-007 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/output_dir_verified/output_from_proc_3_out_of_4.out` | TEST-226, GOLD-007 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/test_cohesive_parallel_insertion_along_physical_surfaces.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/test_cohesive_parallel_insertion_along_physical_surfaces.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/test_cohesive_parallel_intrinsic_implicit_insertion.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/verify_insertion.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/CMakeLists.txt` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/material.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/material_tetrahedron.dat` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/mesh.geo` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/test_cohesive_parallel_intrinsic.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/test_cohesive_parallel_intrinsic.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/test_cohesive_parallel_intrinsic_tetrahedron.cc` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/test_cohesive_parallel_intrinsic_tetrahedron.sh` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/tetrahedron.geo` | TEST-226 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/CMakeLists.txt` | TEST-125…TEST-127, TEST-233 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/embedded_element.dat` | TEST-125 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/embedded_mesh.msh` | TEST-126 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/embedded_mesh_prestress.msh` | TEST-127 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/embedded_mesh_prestress_reinforcement.msh` | TEST-127 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/material.dat` | TEST-126 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/matrix` | TEST-126, GOLD-008 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/prestress.dat` | TEST-127 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/test_embedded_element_matrix.cc` | TEST-125 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/test_embedded_interface_model.cc` | TEST-126 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/test_embedded_interface_model.sh` | TEST-126 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/test_embedded_interface_model_prestress.cc` | TEST-127 |
| `test/test_model/test_solid_mechanics_model/test_embedded_interface/triangle.msh` | TEST-125 |
| `test/test_model/test_solid_mechanics_model/test_energies/CMakeLists.txt` | TEST-128…TEST-131, TEST-233 |
| `test/test_model/test_solid_mechanics_model/test_energies/test_solid_mechanics_model_kinetic_energy.cc` | TEST-129 |
| `test/test_model/test_solid_mechanics_model/test_energies/test_solid_mechanics_model_kinetic_energy_material.dat` | TEST-129 |
| `test/test_model/test_solid_mechanics_model/test_energies/test_solid_mechanics_model_linear_elastic_potential_energy.cc` | TEST-128 |
| `test/test_model/test_solid_mechanics_model/test_energies/test_solid_mechanics_model_linear_elastic_potential_energy_material.dat` | TEST-128 |
| `test/test_model/test_solid_mechanics_model/test_energies/test_solid_mechanics_model_work_dynamics.cc` | TEST-131 |
| `test/test_model/test_solid_mechanics_model/test_energies/test_solid_mechanics_model_work_material.dat` | TEST-130, TEST-131 |
| `test/test_model/test_solid_mechanics_model/test_energies/test_solid_mechanics_model_work_quasistatic.cc` | TEST-130 |
| `test/test_model/test_solid_mechanics_model/test_material_selector.cc` | TEST-108 |
| `test/test_model/test_solid_mechanics_model/test_materials/1_tetrahedron.msh` | TEST-140 |
| `test/test_model/test_solid_mechanics_model/test_materials/1_tetrehedron.geo` | TEST-140 (source of 1_tetrahedron.msh), MDATA-005 |
| `test/test_model/test_solid_mechanics_model/test_materials/CMakeLists.txt` | TEST-135…TEST-145, TEST-233 |
| `test/test_model/test_solid_mechanics_model/test_materials/barre_trou.geo` | TEST-135 (unused; target uses mesh_section_gap.geo), GOLD-015 |
| `test/test_model/test_solid_mechanics_model/test_materials/cube.geo` | TEST-140 |
| `test/test_model/test_solid_mechanics_model/test_materials/interpolation.geo` | TEST-136 |
| `test/test_model/test_solid_mechanics_model/test_materials/local_material_damage.cc` | TEST-135 |
| `test/test_model/test_solid_mechanics_model/test_materials/local_material_damage.hh` | TEST-135 |
| `test/test_model/test_solid_mechanics_model/test_materials/local_material_damage_inline_impl.hh` | TEST-135 |
| `test/test_model/test_solid_mechanics_model/test_materials/material.dat` | TEST-135 |
| `test/test_model/test_solid_mechanics_model/test_materials/material_finite_deformation.dat` | TEST-140 |
| `test/test_model/test_solid_mechanics_model/test_materials/material_interpolate.dat` | TEST-136 |
| `test/test_model/test_solid_mechanics_model/test_materials/material_mazars.dat` | TEST-138 |
| `test/test_model/test_solid_mechanics_model/test_materials/material_thermal.dat` | TEST-145 (unused; test builds material in code), GOLD-015 |
| `test/test_model/test_solid_mechanics_model/test_materials/mesh_section_gap.geo` | TEST-135 |
| `test/test_model/test_solid_mechanics_model/test_materials/orthotropic.dat` | TEST-137 |
| `test/test_model/test_solid_mechanics_model/test_materials/py_mazars.py` | TEST-143 |
| `test/test_model/test_solid_mechanics_model/test_materials/square.geo` | TEST-137 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_damage_materials.cc` | TEST-143 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_elastic_materials.cc` | TEST-141 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_finite_def_materials.cc` | TEST-142 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_finite_deformation.cc` | TEST-140 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_interpolate_stress.cc` | TEST-136 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_local_material.cc` | TEST-135 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_elasto_plastic_linear_isotropic_hardening/CMakeLists.txt` | TEST-146 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_elasto_plastic_linear_isotropic_hardening/test_material_elasto_plastic_linear_isotropic_hardening.cc` | TEST-146 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_elasto_plastic_linear_isotropic_hardening/test_material_elasto_plastic_linear_isotropic_hardening.dat` | TEST-146 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_elasto_plastic_linear_isotropic_hardening/test_material_elasto_plastic_linear_isotropic_hardening.geo` | TEST-146 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_elasto_plastic_linear_isotropic_hardening/test_material_standard_linear_isotropic_hardening.verified` | TEST-146, GOLD-001 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_fixtures.hh` | TEST-141…TEST-145 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_mazars.cc` | TEST-138 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/CMakeLists.txt` | TEST-147, TEST-148 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/custom_non_local_test_material.cc` | TEST-148 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/custom_non_local_test_material.hh` | TEST-148 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/material.dat` | TEST-148 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/material_damage_non_local.dat` | TEST-147 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/mesh.geo` | TEST-148 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/mesh_section_gap.geo` | TEST-147 (source of committed mesh), MDATA-005 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/mesh_section_gap.msh` | TEST-147 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/test_material_damage_non_local.cc` | TEST-147 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/test_material_non_local.cc` | TEST-148 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_orthotropic.cc` | TEST-137 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_thermal.cc` | TEST-145 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic/CMakeLists.txt` | TEST-149, TEST-150 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic/material_standard_linear_solid_deviatoric_relaxation.dat` | TEST-149, TEST-150 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation.cc` | TEST-149 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation.geo` | TEST-149, TEST-150 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation_tension.cc` | TEST-150 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic_maxwell/CMakeLists.txt` | TEST-151 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic_maxwell/material_viscoelastic_maxwell.dat` | TEST-151 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic_maxwell/test_material_viscoelastic_maxwell.geo` | TEST-151 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_material_viscoelastic_maxwell/test_material_viscoelastic_maxwell_relaxation.cc` | TEST-151 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_multi_material_elastic.cc` | TEST-139 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_multi_material_elastic.dat` | TEST-139 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_multi_material_elastic.geo` | TEST-139 |
| `test/test_model/test_solid_mechanics_model/test_materials/test_plastic_materials.cc` | TEST-144 |
| `test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_dynamics.cc` | TEST-109 |
| `test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_dynamics_material.dat` | TEST-109 |
| `test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_fixture.hh` | TEST-109, TEST-128…TEST-131 |
| `test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_material_eigenstrain.cc` | TEST-106 |
| `test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_material_large_rotation.cc` | TEST-107 |
| `test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_reassign_material.cc` | TEST-105 |
| `test/test_model/test_solid_mechanics_model/two_materials.dat` | TEST-105 |
| `test/test_model/test_structural_mechanics_model/CMakeLists.txt` | TEST-152, TEST-233 |
| `test/test_model/test_structural_mechanics_model/_bernoulli_beam_2.msh` | TEST-152, MDATA-001 |
| `test/test_model/test_structural_mechanics_model/_bernoulli_beam_3.msh` | TEST-152, MDATA-001 |
| `test/test_model/test_structural_mechanics_model/_discrete_kirchhoff_triangle_18.msh` | TEST-152, MDATA-001 |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_bernoulli_beam_2.cc` | TEST-152 |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_bernoulli_beam_3.cc` | TEST-152 |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_bernoulli_beam_dynamics.cc` | TEST-152 |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_discrete_kirchhoff_triangle_18.cc` | TEST-152 |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_fixture.hh` | TEST-152 |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5.cc` | TEST-227 |

### App.17 `test/test_python_interface` (5 files)

| File | ID(s) |
|---|---|
| `test/test_python_interface/CMakeLists.txt` | TEST-153 |
| `test/test_python_interface/elastic.dat` | TEST-153 |
| `test/test_python_interface/mesh_dcb_2d.geo` | TEST-153 |
| `test/test_python_interface/test_common.cc` | TEST-153 |
| `test/test_python_interface/test_pybind.py` | TEST-153 |

### App.18 `test/test_solver` (25 files)

| File | ID(s) |
|---|---|
| `test/test_solver/1D_bar.geo` | TEST-228, GOLD-015 |
| `test/test_solver/CMakeLists.txt` | TEST-155…TEST-160, TEST-233 |
| `test/test_solver/bar.msh` | TEST-157 |
| `test/test_solver/square.geo` | TEST-228 |
| `test/test_solver/test_petsc_matrix_apply_boundary.cc` | TEST-228 |
| `test/test_solver/test_petsc_matrix_apply_boundary.sh` | TEST-228 |
| `test/test_solver/test_petsc_matrix_diagonal.cc` | TEST-228 |
| `test/test_solver/test_petsc_matrix_diagonal.sh` | TEST-228 |
| `test/test_solver/test_petsc_matrix_profile.cc` | TEST-228 |
| `test/test_solver/test_petsc_matrix_profile.verified` | TEST-228, GOLD-003 |
| `test/test_solver/test_petsc_matrix_profile_parallel.cc` | TEST-228 |
| `test/test_solver/test_petsc_matrix_profile_parallel.sh` | TEST-228 |
| `test/test_solver/test_petsc_matrix_profile_parallel.verified` | TEST-228, GOLD-003 |
| `test/test_solver/test_solver_petsc.cc` | TEST-228 |
| `test/test_solver/test_solver_petsc.sh` | TEST-228 |
| `test/test_solver/test_solver_petsc_parallel.sh` | TEST-228 |
| `test/test_solver/test_sparse_matrix_assemble.cc` | TEST-156 |
| `test/test_solver/test_sparse_matrix_product.cc` | TEST-157 |
| `test/test_solver/test_sparse_matrix_product.verified` | TEST-157, GOLD-001 |
| `test/test_solver/test_sparse_matrix_profile.cc` | TEST-155 |
| `test/test_solver/test_sparse_matrix_profile_parallel.cc` | TEST-229 |
| `test/test_solver/test_sparse_solver_eigen.cc` | TEST-159 |
| `test/test_solver/test_sparse_solver_mumps.cc` | TEST-158 |
| `test/test_solver/test_sparse_solver_petsc.cc` | TEST-160 |
| `test/test_solver/triangle.geo` | TEST-155, TEST-156 |

### App.19 `test/test_synchronizer` (18 files)

| File | ID(s) |
|---|---|
| `test/test_synchronizer/CMakeLists.txt` | TEST-165…TEST-170, TEST-233 |
| `test/test_synchronizer/bar.msh` | TEST-165 |
| `test/test_synchronizer/cube.geo` | TEST-167 |
| `test/test_synchronizer/data_split.msh` | TEST-169 (unused), GOLD-015 |
| `test/test_synchronizer/facet.geo` | TEST-170 |
| `test/test_synchronizer/test_communicator.cc` | TEST-166 |
| `test/test_synchronizer/test_data_accessor.hh` | TEST-165, TEST-167, TEST-170 |
| `test/test_synchronizer/test_data_distribution.cc` | TEST-169 |
| `test/test_synchronizer/test_dof_data_accessor.hh` | TEST-230 |
| `test/test_synchronizer/test_dof_synchronizer.cc` | TEST-165 |
| `test/test_synchronizer/test_dof_synchronizer_communication.cc` | TEST-230 |
| `test/test_synchronizer/test_facet_synchronizer.cc` | TEST-170 |
| `test/test_synchronizer/test_grid_synchronizer.cc` | TEST-230 |
| `test/test_synchronizer/test_grid_synchronizer_check_neighbors.cc` | TEST-230 |
| `test/test_synchronizer/test_grid_tools.hh` | TEST-230 |
| `test/test_synchronizer/test_node_synchronizer.cc` | TEST-168 |
| `test/test_synchronizer/test_synchronizer_communication.cc` | TEST-167 |
| `test/test_synchronizer/test_synchronizers_fixture.hh` | TEST-167…TEST-170 |

## Errata (independent completeness review, 2026-11-09)

Factual corrections from an independent completeness review of the inventory. Where they conflict with the text above, **the errata win**.

| Finding | Existing row / location | Evidence and correction |
|---|---|---|
| E5 | DOC-005, matrix and tests/examples/Python inventory | Readers: Abaqus incorrectly promotes documentation to implemented capability. `src/io/mesh_io.cc:60-70` dispatches only Gmsh, optional structural Gmsh and Diana. _miot_abaqus falls through to null; read dereferences it at :75-78. `src/io/mesh_io/mesh_io_abaqus.cc` is absent. Correct to documented/enum-only, unsupported at pinned commit; cross-reference IO-012. |
