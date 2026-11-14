# Inventory R3 — Model infrastructure, solvers, parallelism, non-solid physics

_Status: first complete pass, 2026-11-09._
_Reference: upstream Akantu @ `35a851780d0b9471d258ed53a003ae98aaa95ea0`._

This file inventories, at source level, everything in the R3 scope:

* `src/model/model*.{hh,cc}`, `src/model/common/` (minus `constitutive_laws/`
  and `non_local_toolbox/`, which belong to R2 / the solid inventory);
* `src/solver/`, `src/synchronizer/`;
* the parallel / periodic pieces of `src/mesh_utils/` and `src/mesh/`;
* `src/model/contact_mechanics/`, `src/model/diffusion_model/`,
  `src/model/phase_field/`, `src/model/structural_mechanics/`,
  `src/model/model_couplers/`;
* the matching upstream tests.

All `path:line` references are relative to the reference checkout root, so
`src/model/model.cc:45` means line 45 of `src/model/model.cc` in the upstream repository.
Line numbers are taken from the pinned commit.

## Summary

| Prefix | Area | Rows |
|---|---|---|
| MODEL | Model base, model solver, solver callbacks | 11 |
| DOF | DOF managers (default, PETSc) | 17 |
| TS | Integration schemes and time-step solvers | 12 |
| NLS | Non-linear solvers (lumped, NR family, SNES, TAO) | 9 |
| BC | Boundary conditions | 5 |
| SPM | Sparse matrices and solver vectors | 13 |
| SOLV | Sparse solvers (MUMPS, Eigen, PETSc) | 7 |
| MPI | Communicator and synchronizers | 25 |
| PART | Partitioning, distribution, ghosts, periodicity | 18 |
| DIFF | Diffusion / heat transfer | 12 |
| PF | Phase field | 12 |
| CONT | Contact mechanics | 18 |
| STRUCT | Structural mechanics | 13 |
| CPL | Model couplers | 7 |
| **Total** | | **179** |

The appendix maps every in-scope source file (233 `.hh`/`.cc` under the
listed directories, including all of `src/mesh` and `src/mesh_utils`) and 68
test sources.

Headline risks for the port (details in the rows and §6):

* **Serial-only physics.** Contact mechanics (and its couplers) and
  structural mechanics are effectively serial in the reference (§3.11).
* **Reference defects that change results.**
  * contact friction uses uninitialised Eigen accumulators (UB);
  * contact stiffness is indexed by node number and doubled off the diagonal
    in symmetric storage;
  * `SparseMatrixAIJ::matVecMul`'s parallel reduction is a no-op;
  * phase-field ghost gradients are written into not-ghost storage;
  * the structural lumped mass is zero;
  * PETSc DOF manager numbering mismatches with several DOF ids.
* **Inactive upstream tests.**
  * Heat-transfer patch tests are never built (wrong package name).
  * Several contact, solver and synchronizer tests are not registered.
  * The `PARALLEL` rank-list logic in CMake looks broken (Q1).
* **Reproducibility.** It depends on exact local node numbering, global
  equation numbering, AIJ triplet order, the Scotch partition and the MUMPS
  configuration (§2.5, §3.5).

## Conventions

* **Package / build option** names are Akantu CMake packages
  (`packages/*.cmake`). The compile-time macros they define are listed in
  `src/common/aka_config.hh.in:73-101`: `AKANTU_PARALLEL`, `AKANTU_USE_MPI`,
  `AKANTU_USE_SCOTCH`, `AKANTU_USE_PTSCOTCH`, `AKANTU_IMPLICIT`,
  `AKANTU_USE_MUMPS`, `AKANTU_USE_PETSC`, `AKANTU_DIFFUSION`,
  `AKANTU_PHASE_FIELD`, `AKANTU_STRUCTURAL_MECHANICS`,
  `AKANTU_CONTACT_MECHANICS`, `AKANTU_MODEL_COUPLERS`,
  `AKANTU_COHESIVE_ELEMENT`, `AKANTU_DEBUG_TOOLS`.
  * `core` (always on). Includes all of `model/common` except NR/PETSc, all of
    `synchronizer/` except the MPI backend, `solver/` AIJ + Eigen, and
    `mesh_utils/` partition/distribution (`packages/core.cmake:28-300`).
  * `implicit` (META). Adds `NonLinearSolverNewtonRaphson`/`Linear`
    (`packages/implicit.cmake:31-34`). The `AKANTU_IMPLICIT_SOLVER` cache
    variable is one of `Eigen | Mumps | PETSc | Mumps+PETSc` (default
    `Mumps`) and selects the `Mumps`/`PETSc` package dependencies. It warns
    that Eigen is not parallel (`packages/implicit.cmake:23-58`).
  * `parallel` (META) = `MPI` + `Scotch` (`packages/parallel.cmake:21-24`).
    `MPI` adds `synchronizer/mpi_communicator_data.hh` and
    `synchronizer/communicator_mpi_inline_impl.hh` (`packages/mpi.cmake:29-32`).
    `Scotch` adds `mesh_utils/mesh_partition/mesh_partition_scotch.cc`
    (`packages/scotch.cmake:26-28`). Scotch's `SCOTCH_Num` must have the same
    size as Akantu `Int` (`packages/scotch.cmake:30-46`).
  * `Mumps` (`packages/z_mumps.cmake`) adds `solver/sparse_solver_mumps.*`.
    It uses the `parallel` MUMPS component if the `parallel` package is on,
    `sequential` otherwise.
  * `PETSc` (`packages/petsc.cmake`, depends on `parallel`, PETSc ≥ 3.5) adds
    `dof_manager_petsc.*`, `non_linear_solver_petsc.*`,
    `non_linear_solver_tao.*`, `solver/petsc_wrapper.hh`,
    `solver/solver_vector_petsc.*`, `solver/sparse_matrix_petsc.*`,
    `solver/sparse_solver_petsc.*`, `io/parser/parsable_petsc.hh`.
    `PetscInt` must match `Int` and `PetscReal` must match `Real`.
  * `diffusion` (default ON), `phase_field` (default ON, depends on
    `model_couplers`), `contact_mechanics` (default ON, depends on
    `model_couplers` and `cohesive_element`), `structural_mechanics` (default
    **OFF**, depends on `implicit`), `model_couplers` (ADVANCED, declares no
    sources: coupler sources are listed in `contact_mechanics.cmake` and
    `phase_field.cmake`), `cohesive_element` (R2 scope, owns
    `cohesive_element_inserter*`), `debug_tools` (owns
    `boost_graph_converter.*`).
* **Upstream tests** give the test name as registered in CMake, with its
  fixtures (meshes, `.dat` input files, `.verified` reference outputs).
  * `PARALLEL` means the test is run under `mpiexec` with an automatic list of
    rank counts. `cmake/AkantuTestsMacros.cmake:548-580` computes it as
    `N, N/2, …, 1` with `N = ProcessorCount()` or `MPIEXEC_MAX_NUMPROCS`; see
    Open questions Q1 for a defect in that logic.
  * `PARALLEL_LEVEL a b …` fixes the rank counts.
  * `UNSTABLE` tests are not built unless `AKANTU_BUILD_UNSTABLE_TESTS` is set.
  * The parallel CI list is `AKANTU_PARALLEL_TESTS` in
    `packages/parallel.cmake:26-35`.
* "Parallel aspects" uses these terms:
  * *ghost* elements: the overlap layer received from neighbours;
  * *local / master / slave / pure-ghost* nodes: the `NodeFlag` values,
    explained in the Parallel design section;
  * *rank-local* arrays (size = local nodes) versus *global* arrays.
* "Porting notes / risks" records defects, surprises and
  numerical-reproducibility traps found while reading. Defects are written as
  facts about the reference ("bug:") and are **not** to be reproduced blindly;
  see `docs/NUMERICAL_VALIDATION.md` for how discrepancies are handled.

## 1. Main feature table

### 1.1 Model base, model solver, solver callbacks (MODEL)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| MODEL-001 | `Model` base class | `src/model/model.hh:45-364`, `model.cc:33-97`; `Model(Mesh&, ModelType, Int dim, ID)` | Holds `Mesh&`, spatial dimension (default = mesh dim), map of named `FEEngine`s (`fems`, first registered = `default_fem`), the static `Parser&`, `AnalysisMethod method`, default dumper element kind. Registers itself as `MeshEventHandler` with priority `_ehp_model` (=94). Inherits `ModelSolver`. | core | — | Indirectly through every model test. | Element synchronizer registration is done by subclasses. | Rust: a `Model` trait plus a composed struct. Don't reproduce the diamond `ModelSolver`/`SynchronizerRegistry`/`Parsable`/`SolverCallback` inheritance. |
| MODEL-002 | `initFull` dispatch with named arguments | `model.hh:72-132`; `model_options.hh:27-166` | The variadic `initFull(_analysis_method=…, _solver_options=…, _is_extrinsic=…, _init_intersections=…)` builds the options struct for the current `ModelType`: `ModelOptions` (analysis_method default `_explicit_lumped_mass`, solver_options default `detail::_default_solver_options`); `SolidMechanicsModelCohesiveOptions` (`is_extrinsic` false); `EmbeddedInterfaceModelOptions` (`has_intersections` true); `CouplerSolidCohesiveContactOptions` (`is_extrinsic`). Then calls `initFullImpl` → `initNewSolver` (unless a default solver already exists) → `initModel()`. | core (+ the package of each options type, by `#ifdef`) | — | All model tests. | — | Named-argument metaprogramming should become a builder or options struct with `Default`. Keep the default analysis method `_explicit_lumped_mass`. |
| MODEL-003 | Enumerations `ModelType`, `AnalysisMethod` | `src/common/aka_common.hh:128-166` | `ModelType` = `model, solid_mechanics_model, solid_mechanics_model_cohesive, heat_transfer_model, structural_mechanics_model, embedded_model, contact_mechanics_model, coupler_solid_contact, coupler_solid_cohesive_contact, phase_field_model, coupler_solid_phasefield`, with stream I/O used by the parser. `AnalysisMethod` = `_static=0, _implicit_dynamic=1, _explicit_lumped_mass=2, _explicit_lumped_capacity=2 (alias), _explicit_consistent_mass=3, _explicit_contact=4, _implicit_contact=5`. | core | — | — | — | Two names share value 2; a Rust enum needs an alias constant. `_explicit_contact`/`_implicit_contact` are only handled by contact models. |
| MODEL-004 | Default solver selection (`initNewSolver`) | `model.cc:59-84`; `model.hh:140-143`; `model_solver.cc:302-348, 403-433` | `getDefaultSolverID(method)` (virtual, per model) returns `(solver_id, TimeStepSolverType)`. If the user's `timestep_solver_type` is `_not_defined` the model default is used. If no solver of that id exists: `getDefaultSolverOptions(type)` is merged into the user options with `ModelSolverOptions::update`, `getNewSolver` is called, and every default `integration_scheme_type[dof]` without a scheme is set together with its `solution_type`. Finally `method` and `default_solver_id` are set. | core | — | `test_model_solver*` (§1.1 MODEL-006). | — | **Surprise:** `update()` (`model_solver.cc:417-433`) *overwrites* user-provided integration scheme / solution type maps with the defaults, and `initNewSolver` sets schemes from `options` (the defaults), not from the user's options. User-provided integration schemes in `ModelSolverOptions` are therefore ignored by `initFull`. Reproduce the observable behaviour; see Open questions Q2. |
| MODEL-005 | `ModelSolver`: DOF manager creation and parser-driven configuration | `src/model/common/model_solver.hh:45-188`; `model_solver.cc:54-218` | `initDOFManager()` uses the parser section `model <model_type> [ name = <id> … ]` (default name = model id) to read `dof_manager_type` (default `"default"`), then `DOFManagerFactory::allocate(type, mesh, id+":dof_manager_"+type)`. Sub-sections `time_step_solver <type> [name=…, sparse_solver_type=…]` create solvers. At most one `non_linear_solver <type>` sub-section is allowed (more → exception) and it is forwarded to `NonLinearSolver::parseSection`. `integration_scheme <type> [name=<dof>, solution_type=…]` sub-sections set the schemes; missing defaults are filled in. `default_solver = <id>` must name an existing solver. `initDOFManager(shared_ptr)` reuses an external DOF manager (used by couplers). | core | — | `test_model_solver_{mumps,petsc}` (`dof_manager_type` via the `DOF_MANAGER_TYPE` compile option). | DOF manager type `petsc` requires PETSc + MPI. | Parser grammar belongs to the R1 inventory. The recognised keys are listed here: `dof_manager_type`, `sparse_solver_type`, `name`, `default_solver`, `solution_type`. Unknown DOF manager types raise "you will have to code it". |
| MODEL-006 | `ModelSolver` solver registry API | `model_solver.cc:221-400` | `getNewSolver(id, options)`: the first solver created becomes the default. `NonLinearSolverType::_auto` resolves to `_newton_raphson` for `_static`/`_dynamic` and `_lumped` for `_dynamic_lumped`. `SparseSolverType::_auto` resolves to `_mumps` if registered else `_eigen` for `DOFManagerDefault`, and `_petsc` for `DOFManagerPETSc`. It calls the model hook `initSolver(tss_type)`, creates the NLS and then the TSS in the DOF manager. Also: `solveStep(id)`, `solveStep(callback, id)` (empty id = default), `get/setTimeStep(id)`, `hasSolver`, `setDefaultSolver`, `hasDefaultSolver`, `setIntegrationScheme(solver, dof, type, solution_type)`, `setIntegrationScheme(solver, dof, unique_ptr<IntegrationScheme>&)`, `hasIntegrationScheme`, `getTimeStepSolver`, `getNonLinearSolver`. Default `getDefaultSolverType()` = `_dynamic_lumped`. Default `predictor()`/`corrector()` are no-ops. | core | — | `test_model_solver_mumps` / `_petsc` (`test/test_model/test_common/test_model_solver/test_model_solver.cc`): 1-D bar of 100 nodes, distributed, `MyModel` (`test_model_solver_my_model.hh`), static NR, `max_iterations=2`. Reference outputs `test_model_solver_mumps.verified`, `test_model_solver_petsc.verified`. `test_model_solver_dynamic_{explicit,implicit,petsc}` (`test_model_solver_dynamic.cc`) with `.verified` files. `test_dof_manager_default` (`test_dof_manager_default.cc` + `.verified`). Python `test_model_solver.py`, `test_model_solver_dynamic.py` (not registered in the CMakeLists). | Solve is collective. | The `MyModel` test model (a 1-D bar with custom K/M assembly) is the best fixture for differential tests of the solver stack; port it first. |
| MODEL-007 | `SolverCallback` interface | `src/model/common/solver_callback.hh:34-102`; `solver_callback.cc` | Virtual hooks: `getMatrixType(id)` (default `_mt_not_defined`), `assembleMatrix(id)`, `assembleLumpedMatrix(id)`, `assembleResidual()`, `assembleResidual(part)` (`"external"`, `"internal"`, `"inertial"`), `predictor()`, `corrector()`, `canSplitResidual()` (default false), `beforeSolveStep()`, `afterSolveStep(converged)`, `restoreLastConvergedStep()`. Keeps a back-pointer `sc_dof_manager`. `InterceptSolverCallback` (`:113-166`) forwards everything to a wrapped callback, so users can override selected hooks. `debug::SolverCallbackResidualPartUnknown` exception. | core | — | — | — | A Rust trait with default methods. Python bindings expose `InterceptSolverCallback` (R4). |
| MODEL-008 | Boundary FE engine management | `model_inline_impl.hh:31-133`; `model.cc:87-97` | `getFEEngineClassBoundary<T>(name)` lazily registers `"<name>:boundary"` with dimension `elem_dim-1` and computes normals on integration points for both ghost types. `initFEEngineBoundary()` initialises shapes and normals and swallows exceptions. Also `registerFEEngineObject` (throws if the name exists), `unRegisterFEEngineObject` (reassigns the default), `getFEEngine(name)`, `hasFEEngineBoundary`. | core | — | Neumann BC tests (solid R2). | Ghost elements get normals too, because Neumann BCs integrate over ghosts. | — |
| MODEL-009 | Model-level dumper convenience API | `model.hh:157-335`; `model.cc:100-339` | `dumpGroup`, `setGroupDirectory`, `setGroupBaseName`, `getGroupDumper`, `addDumpField[Vector/Tensor][ToDumper]`, `addDumpGroupField…`, `removeDumpGroupField…`, `setBaseName`, `setDirectory`, `setTextModeToDumper`, `dump(...)`. Field lookup order (`model.cc:255-294`): `createNodalFieldReal` → `Int` → `Bool` → `createElementalField` → mesh attached data `<Int>` → `<Real>`, with a warning if nothing is found (debug builds). | core + dumpers (R1) | IOHelper (R1) | Dump-based tests in each model. | Dumpers are per rank. `_for_dump` sync tags exist (MPI-024). | Output-format parity belongs to R1. Keep the field-name lookup chain and the exact field names, which are listed per model below. |
| MODEL-010 | Nodal field allocation and helpers | `model_inline_impl.hh:136-157` | `allocNodalField(unique_ptr<Array<T>>&, nb_component, name)` allocates once (`nb_nodes × nb_component`, value `T()`, id `model_id:name`). `getNbIntegrationPoints(Array<Element>, fem_id)` sums integration points over a list of elements; it is used for pack sizes. | core | — | — | Sizes include ghost and pure-ghost nodes (all local nodes). | — |
| MODEL-011 | Model ↔ synchronizer registry | `model_solver.hh:45-47` (`ModelSolver : SynchronizerRegistry`); see MPI-015 | Every model is a `SynchronizerRegistry`. In the constructor (if the mesh is distributed) it registers the mesh's `ElementSynchronizer` for its tags and itself as data accessor, then calls `synchronize(tag)` / `asynchronousSynchronize` / `waitEndSynchronize` during assembly. | core | — | — | Collective. | — |

### 1.2 DOF manager (DOF)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| DOF-001 | DOF registration (`registerDOFs`) | `src/model/common/dof_manager/dof_manager.hh:60-66, 555-606`; `dof_manager.cc:251-367` | `registerDOFs(id, Array<Real>&, DOFSupportType)` with `_dst_nodal` (array size must equal `nb_nodes`) or `_dst_generic` (all `size × nb_component` entries are free DOFs). `registerDOFs(id, array, group_name)` creates nodal DOFs supported on an element group's node group (array size = group node count). The DOF data stores pointers to dof / blocked / increment / previous / derivative arrays, a private `solution` array, `local_equation_number`, `associated_nodes`, the support type and the group (`"__mesh__"` = whole mesh). Registering the same id twice throws. After registration the global arrays are resized (`resizeGlobalArrays`). | core | — | `test_dof_manager` (gtest, typed over `DOFManagerDefault` (and PETSc when enabled); fixtures `mesh.geo` → `dof_manager_mesh`, 3-D order 1; registered `PARALLEL`): `RegisterGenericDOF1/2`, `RegisterNodalDOF1/2`, `RegisterMixedDOF`. | Counting excludes periodic slaves from the local count and counts only local/master nodes as "pure local" (`countDOFsForNodes`, `dof_manager.cc:231-248`). | Multiple DOF ids share one equation space. Numbering depends on registration order (see DOF-003). |
| DOF-002 | DOF auxiliary arrays | `dof_manager.hh:69-82, 276-316`; `dof_manager.cc:370-423, 585-587`; `dof_manager_inline_impl.hh:87-162` | `registerDOFsPrevious`, `registerDOFsIncrement`, `registerDOFsDerivative(id, order≥1, array)`, `registerBlockedDOFs`; each throws on double registration. Accessors `getDOFs`, `getDOFsDerivatives(id, order)` (order 0 = DOFs), `getBlockedDOFs`, `getDOFsIncrement`, `getPreviousDOFs`, `getSolution(id)`, `has*` predicates. `savePreviousDOFs(id)` copies the DOFs into previous. | core | — | `test_dof_manager` | — | **Bug:** `hasDOFsDerivatives` uses `order < derivatives.size()` (`dof_manager_inline_impl.hh:139`), so the highest registered order reports `false`. The models only call it with order 1 after registering order 1 (it then reports false, and they register again only if absent). Treat it as observable behaviour; see Q3. |
| DOF-003 | Equation numbering (local/global) | `dof_manager.cc:768-898`; `dof_manager_inline_impl.hh:165-218` | For every registration, local equation numbers are appended sequentially (`local_system_size` grows). Global numbers come from `first_global_dof_id + exclusiveScan(nb_new_pure_local)`. A rank numbers only its local/master DOFs in registration order; the others get `-1`. After each registration `first_global_dof_id += allReduce(nb_new_pure_local)`. Periodic slave DOFs get `local_equation_number = -1` in the first pass and are then remapped to the master's local equation (pairs `(master_node, component)`). If the mesh is distributed, `GlobalDOFInfoDataAccessor` (`:700-765`) sends master global numbers to slaves through `NodeSynchronizer::synchronizeOnce(_ask_nodes)`, and first through `PeriodicNodeSynchronizer` (`_giu_global_conn`) if the mesh is periodic. `dofs_flag` stores the node flag per local equation. `global_to_local_mapping` is an `unordered_map`. Generic DOFs are numbered contiguously, all flagged `_normal`. | core | MPI (exscan, allreduce) | `test_dof_manager` `splitArrayPerDOFs` (checks global ids), `AssembleVector`, `AssembleMatrixNodal`; `test_dof_synchronizer`. | Global numbering is **rank-blocked per registration call**: DOF id A gets `[0, nA)` across ranks, then DOF id B `[nA, nA+nB)`, etc. Pure-ghost DOFs keep global = -1, because `NodeSynchronizer` only links master→slave. | Equation numbering affects MUMPS ordering and therefore round-off. Reproduce the exact numbering for bitwise differential tests. |
| DOF-004 | System sizes | `dof_manager.hh:261-267, 644-650`; `dof_manager.cc:335-341, 612-618` | `local_system_size` = all local equations (local + master + slave + pure ghost, periodic slaves excluded); `pure_local_system_size` = local + master equations; `system_size` = global sum of pure local sizes. | core | MPI allreduce | `test_dof_manager` (`EXPECT_EQ(residual.size(), nb_total_nodes*3)`). | — | — |
| DOF-005 | Global blocked DOFs and release tracking | `dof_manager.cc:920-969`; `dof_manager.hh:255-258, 319-325, 681-696` | `updateGlobalBlockedDofs()` (called at the start of every non-linear solve) collects the local equation numbers of blocked DOFs (skipping `-1`), sorts them, de-duplicates them, and compares them with the previous set. If they differ, `global_blocked_dofs_indexes_release` is incremented and the boolean mask `global_blocked_dofs` (size = local system) is rebuilt. `hasBlockedDOFsChanged()` compares the current and previous releases. | core | — | indirectly in solver tests | Rank-local. Masks cover slave/ghost equations too. | Integration schemes skip blocked DOFs using the *per-DOF* blocked arrays, not this mask. The NR convergence norm uses this mask (NLS-005). |
| DOF-006 | Dirichlet treatment of the Jacobian (`applyBoundary`) | `dof_manager.cc:972-988`; AIJ implementation SPM-004; PETSc SPM-008 | `applyBoundary("J")`: if the J value release equals the last seen release, it re-applies only when the blocked set changed; otherwise it always applies. Then it records the J release and syncs the previous-blocked release. Called by `TimeStepSolverDefault::assembleMatrix("J")` after the integration schemes assembled J (`time_step_solver_default.cc:223-240`). | core | — | `test_petsc_matrix_apply_boundary` (unregistered, see SPM-008). | See SPM-004 for the diagonal-ownership rule. | Residual entries of blocked DOFs are **not** zeroed by `DOFManagerDefault`. With identity rows the solve gives `δ_blocked = r_blocked` (generally non-zero, e.g. the reactions); the correctors discard it because they skip blocked DOFs (NLS-005, §2.4). |
| DOF-007 | Vector assembly to residual / lumped matrices / local arrays | `dof_manager.cc:68-216`; `dof_manager_default.cc:78-118` | `assembleElementalArrayLocalArray(elem_vect, array, type, ghost, scale, filter)` scatter-adds element vectors (`nb_dof × nb_nodes_per_element` per element) into a nodal array: `array[conn[n]] += scale*elemental[n]`. With a filter, element *i* of `elem_vect` corresponds to `filter[i]`. `assembleElementalArrayToResidual` and `assembleElementalArrayToLumpedMatrix` go through a temporary nodal array, then `assembleToResidual` / `assembleToLumpedMatrix` call `assembleToGlobalArray(dof, array, global, scale)`, which adds `scale*array[i]` to `global[local_eq[i]]`. In periodic meshes, contributions of periodic-slave nodes are dropped (multiplied by 0) because the master entry is the shared one (`dof_manager_default.cc:91-97`). | core | — | `test_dof_manager` `AssembleVector` (tolerance 1e-14) | Rank-local, no communication. Models make the residual complete on all non-pure-ghost nodes by assembling over ghost elements (§3.6). | The scale factor is applied element-wise before summation. Keep the multiplication order (`scale_factor * value`) to match round-off. |
| DOF-008 | Element matrix assembly (`assembleElementalMatricesToMatrix`) | `dof_manager.hh:131-136`; `dof_manager_inline_impl.hh:233-305`; `dof_manager_default.cc:260-270`; `dof_manager_petsc.cc:354-366` | Applies to nodal DOFs only. Element equation numbers are `local_eq[conn[n]*nb_dof+d]` mapped to **global** numbers. Elements come from the filter, else the support group's elements, else all elements of `(type, ghost_type)`. `A.addValues(eq, eq, Ke, elemental_matrix_type)` where `elemental_matrix_type` (default `_symmetric`) describes the element matrix; see SPM-003. `DOFManagerDefault` first builds the profile with `addToProfile` (DOF-014). | core | — | `test_dof_manager` `AssembleMatrixNodal`; `test_sparse_matrix_assemble`. | Models assemble stiffness on `_not_ghost` elements only. Parallel matrices hold rank-partial sums (§3.6). | Element matrix layout: rows/cols ordered node-major (`n*nb_dof+d`). |
| DOF-009 | Pre-assembled term assembly | `dof_manager_inline_impl.hh:308-322`; `solver/terms_to_assemble.hh:32-91` | `assemblePreassembledMatrix(matrix_id, TermsToAssemble)`: each term `(i, j, val)` is in local DOF-array coordinates of `dof_id_m` / `dof_id_n`, mapped to global equations and added with `A.add(gi, gj, val)`. Used by contact / constraints. | core | — | — | — | `TermsToAssemble::operator()(i, j)` always appends a new term (no merging). |
| DOF-010 | Matrix–vector assemblies | `dof_manager.cc:165-174, 991-1011`; `dof_manager_inline_impl.hh:221-230`; `dof_manager_default.cc:240-292`; `dof_manager_petsc.cc:378-385, 468-482` | `assembleMatMulVectToResidual(dof, A, x, s)`: assembles x into `data_cache` (scale 1), then `residual += s * A * cache` via `matVecMul(cache, residual, s, 1)`. `assembleMatMulDOFsToResidual(A, s)` loops over all DOF ids. `assembleMatMulVectToArray(dof, A, x, array, s)` computes `array = per-DOF(s*A*x)`. `assembleLumpedMatMulVectToResidual(dof, A_lumped, x, s)` computes `r_i += A_i * (s*x)_i` (`cache` scaled while assembling). | core / petsc | PETSc Vec ops | via integration schemes | `matVecMul` parallel caveat: SPM-005. | The scale factor sits on x for the lumped variant and on the product for the consistent one. Round-off differs; keep it. |
| DOF-011 | Solution splitting and per-DOF extraction | `dof_manager.cc:177-198`; `dof_manager_default.cc:208-237`; `dof_manager_petsc.cc:339-351` | `splitSolutionPerDOFs()` resizes each `dof_data.solution` to `size × nb_component`, zeroes it, and fills it with `getArrayPerDOFs(dof, solution, ...)`, i.e. `local[i] = global[local_eq[i]]`. For nodal DOFs the result is then made periodic-consistent (`makeConsistentForPeriodicity(_id)`, slaves = masters). `getLumpedMatrixPerDOFs(dof, lumped_id, array)` works the same way. | core | — | `test_dof_manager` `splitArrayPerDOFs` | Rank-local. Values on slave/ghost equations come from the solver's scatter (SPM-011) or from the PETSc ghost update. | — |
| DOF-012 | Object registries and naming | `dof_manager.cc:426-582`; `dof_manager.hh:351-440, 609-697` | Matrices, lumped matrices, NLS and TSS are stored under the ids `<dofm_id>:mtx:<name>`, `:lumped_mtx:<name>`, `:nls:<name>`, `:tss:<name>`. `getMatrix`/`getLumpedMatrix` raise `AKANTU_SILENT_EXCEPTION` if missing; `has*` predicates exist; double registration throws. `registerLumpedMatrix` resizes the new vector immediately. `getNewMatrix(id, MatrixType)` and `getNewMatrix(id, copy_of_id)` are pure virtual; Default → AIJ, PETSc → PETSc. Matrix ids used by the framework: `K`, `M`, `C`, `J` (+ contact-specific ids). | core | — | — | — | — |
| DOF-013 | Mesh events in the DOF manager | `dof_manager.cc:601-917`; `dof_manager_default.cc:393-407` | `onNodesAdded`: for every nodal DOF (filtered to the support group) it counts the new local/pure-local DOFs, updates the sizes (`allReduce`), resizes the per-DOF solution, extends the numbering (DOF-003), then calls `resizeGlobalArrays()` (residual / solution / cache / lumped resized, **every matrix profile cleared**, Default also clears `matrix_profiled_dofs`), and finally `DOFSynchronizer::onNodesAdded`. `onMeshIsDistributed` only checks that the residual's distributed-ness matches the mesh (DOF manager created before distribution → exception). Element added / removed / changed and node removal are no-ops. | core | — | cohesive insertion tests (R2) | Node insertion is collective (exscan). | After cohesive insertion the profile is rebuilt on the next assembly, so the MUMPS analysis is redone. |
| DOF-014 | `DOFManagerDefault` | `dof_manager_default.hh:40-226`; `dof_manager_default.cc:41-424` | Uses `SparseMatrixAIJ` for matrices. Residual / solution / cache are `SolverVectorDistributed` if the mesh is distributed (and a `DOFSynchronizer` is created), else `SolverVectorDefault`. **Note:** `Mesh::distribute` sets `is_distributed=true` even with 1 rank, so this path is taken whenever `distribute()` was called (PART-001). Lumped matrices use the same vector types. `addToProfile(matrix, dof, type, ghost)` adds every `(i, j)` element-coupling pair with `i, j < size` once per `(matrix, dof, type, ghost_type)` (cached in `matrix_profiled_dofs`) and sets the matrix size to `system_size`. NLS factory: `_newton_raphson`, `_newton_raphson_contact`, `_newton_raphson_modified` → `NonLinearSolverNewtonRaphson`; `_linear` → `NonLinearSolverLinear` (these need `AKANTU_IMPLICIT`); `_lumped` → `NonLinearSolverLumped`; anything else throws. TSS: always `TimeStepSolverDefault`. Factory names: `"default"`, and `"mumps"` when `AKANTU_USE_MUMPS` (same class). | core (+implicit, mumps) | — | `test_dof_manager_default` (`.verified`), `test_dof_manager` | See §3.6. | Factory registration through static initialisers: Rust needs an explicit registry. `dof_manager_default_inline_impl.hh` is empty. |
| DOF-015 | `DOFManagerPETSc` | `dof_manager_petsc.hh:51-191`; `dof_manager_petsc.cc:55-490` | `PETScSingleton` calls `PetscInitialize` once with the global argc/argv, `-no_signal_handler`, and a custom error handler that throws. It requires `sizeof(Int)==sizeof(PetscInt)` and `sizeof(Real)==sizeof(PetscReal)`. After each DOF registration it rebuilds the AO (`AOCreateBasic` with the global numbers of local/master DOFs), a template Vec (`VECMPI` → `VecMPISetGhost` with the slave DOFs' PETSc indices; otherwise an identity ISLocalToGlobalMapping), the IS L2G map, and the per-DOF `local_equation_number_petsc` (AO app→petsc, then IS global→local with `IS_GTOLM_MASK`). It recreates residual / solution / cache as `SolverVectorPETSc` and resizes the matrices. Assembly: `assembleToGlobalArray` uses `addValuesLocal(local_equation_number_petsc)`, then `VecAssemblyBegin/End`. Element matrices go through the generic path (DOF-008) using **Akantu global numbers** with `MatSetValues` and `MatAssemblyBegin/End` after every call. NLS factory: `_petsc_snes`, `_petsc_tao`, NR family, `_linear`, `_lumped`; **default** → SNES. Factory name `"petsc"`. | petsc | PETSc ≥ 3.5 (3.17/3.18 API switches), MPI | `test_model_solver_petsc`, `test_model_solver_dynamic_petsc`, `test_sparse_solver_petsc` (PARALLEL), phase-field `test_phase_field_linear`, `test_phase_field_optimal_linear`, `test_phase_field_tao` (PETSc). | Uses `MPI_COMM_WORLD` from `MPICommunicatorData`. | **Bugs/risks:** (1) periodic branch of `assembleToGlobalArray` zeroes the slave entries in a *copy* and then assembles the original (`dof_manager_petsc.cc:318-330`). (2) Matrices are indexed by Akantu global numbers while vectors are indexed through the AO. These agree only if the AO is the identity, i.e. a single DOF id with rank-contiguous numbering; with several DOF ids the numbering is not rank-contiguous per rank. (3) `assemblePreassembledMatrix` and `addValues` use `MatSetValues` with global ids. Low priority for the port when the inventory was taken; flag for review. |
| DOF-016 | DOF classification predicates | `dof_manager_inline_impl.hh:165-206` | `isLocalOrMasterDOF(i)`: `(flag & 0xCC) == 0`. `isSlaveDOF`: `(flag & 0x0F) == 0x05`. `isPureGhostDOF`: `(flag & 0x0F) == 0x09`. Also `localToGlobalEquationNumber`, `globalToLocalEquationNumber` (asserts existence), `hasGlobalEquationNumber`, `getDOFFlag`. | core | — | — | Used for norms, gathers, saves. | `_local_master_mask = 0xCC` also accepts periodic masters, so periodic slaves count as non-local. |
| DOF-017 | Debug output helpers | `dof_manager.cc:1014-1034`; `non_linear_solver_newton_raphson.cc:101-164` | `saveAssociatedNodes()` creates a lumped vector `"nodes"` holding global node ids and saves it as Matrix Market (`nodes.mtx`). With debug level `dblTrace`, NR dumps `A_<n>.mtx`, `b_<n>.mtx`, `x_<n>.mtx` every iteration and prints "blip". | core (debug builds) | — | — | Per-rank files `_rank-<r>`. | Optional; useful for the oracle (matrix dumps). Keep the MatrixMarket format (SPM-007). |

### 1.3 Integration schemes and time-step solvers (TS)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| TS-001 | `IntegrationScheme` base | `src/model/common/integration_scheme/integration_scheme.hh:35-103`; `integration_scheme.cc` | Parsable (`ParserType::_integration_scheme`, id = dof id). Holds `order` (0/1/2), `m_release=-1`, `u_store` (order+1 arrays). `store()` copies the DOF and its derivatives `0..order` into stores (allocated on first use, id `integration_scheme_store:<dof>:<o>`); `restore()` copies them back. Pure virtual: `predictor(dt)`, `corrector(type, dt)`, `assembleJacobian(type, dt)`, `assembleResidual(is_lumped)`, `getNeededMatrixList()`. `SolutionType` enum: `_not_defined=-1`, `_displacement=_temperature=_damage=0`, `_velocity=_temperature_rate=1`, `_acceleration=2`. `operator>>` parses `displacement, temperature, velocity, temperature_rate, acceleration, damage`. | core | — | `test_model_solver_dynamic_*` | Rank-local. | Aliased enum values again. |
| TS-002 | First-order scheme base | `integration_scheme_1st_order.hh:31-79`; `.cc:30-84` | Needs `{"K","M"}`. `predictor(dt)` / `corrector(type, dt)` fetch `u`, `u̇`, blocked DOFs and solution, then dispatch to the 4/6-arg virtuals. `assembleResidual(is_lumped)`: `r -= M·u̇` (consistent, if `M` exists) or `r -= M_lumped ∘ u̇` (if the lumped `M` exists). | core | — | heat-transfer patch tests (DIFF) | — | "M" plays the role of the capacity matrix C. `u_dot_store` is an unused member. |
| TS-003 | Generalized trapezoidal family | `generalized_trapezoidal.hh:60-145`; `.cc:32-184` | Parameter `alpha` (parsable/modifiable, default 0 for the generic `_generalized_trapezoidal`). Predictor: `u += (1-α)Δt u̇` (non-blocked). Corrector with coefficients `e` (on u) and `d` (on u̇): `_temperature` → `e=1, d=1/(αΔt)`; `_temperature_rate` → `e=αΔt, d=1`; then `u += e·δ`, `u̇ += d·δ` for non-blocked DOFs. Jacobian `J = d·M + e·K` (profile copied from K), rebuilt only if the M/K release or the blocked set changed. Specialisations: `ForwardEuler` (α=0, needs only `{"M"}`), `TrapezoidalRule1` (α=1/2, Crank–Nicolson), `BackwardEuler` (α=1). | core | — | `patch_test_linear_heat_transfer_{explicit,static,implicit}` (gtest + Python variants; see DIFF-012) | — | **Bug:** `k_release` is uninitialised (`generalized_trapezoidal.hh:110`), so the first J build depends on garbage, but in practice `m_release=-1` forces it. With `_temperature` and α=0 there is a division by zero, which is why the explicit heat model uses `_temperature_rate`. |
| TS-004 | Second-order scheme base | `integration_scheme_2nd_order.hh:35-90`; `.cc:30-93` | Needs `{"K","M","C"}`. `assembleResidual(is_lumped)`: `r -= C·u̇` if `C` exists, then `r -= M·ü` (consistent) or `r -= M_lumped∘ü`. | core | — | solid dynamics tests (R2), `test_model_solver_dynamic_*` | — | — |
| TS-005 | Newmark-β | `newmark-beta.hh:72-139`; `newmark-beta.cc:30-251` | Akantu naming: **`beta` ≡ standard γ (velocity weight) and `alpha·beta` ≡ standard β (displacement weight)**, i.e. `alpha = β_std/γ`. Check: central difference (α=0) → β_std=0, γ=½; trapezoidal (α=β=½) → β_std=¼; Fox–Goodwin (α=1/6) → β_std=1/12; linear acceleration (α=1/3) → β_std=1/6. Both are parsable/modifiable and default to 0 for the generic `_newmark_beta`. Predictor (with internal `k=h=0`): `u += Δt u̇ + ½Δt² ü`; `u̇ += Δt ü`; `ü` unchanged (non-blocked only). Corrector coefficients `(c on ü, d on u̇, e on u)`: `_acceleration` → `(1, βΔt, αβΔt²)`; `_velocity` → `(1/(βΔt), 1, αΔt)`; `_displacement` → `(1/(αβΔt²), 1/(αΔt), 1)`; then `u += eδ`, `u̇ += dδ`, `ü += cδ`. Jacobian `J = c·M + d·C + e·K` (profile from M; K added only if `e≠0`, C only if it exists), rebuilt if any release or the blocked set changed. | core | — | `test_model_solver_dynamic_implicit` / `_explicit` / `_petsc` (+ `.verified`); solid implicit dynamics (R2) | — | The doc comment in the header (`newmark-beta.hh:34-70`) gives `e = 1/(βΔt)` etc. for velocity, which **does not match the code**; follow the code. `k`, `h` are dead parameters (always 0). |
| TS-006 | Central difference | `newmark-beta.hh:148-154` | `NewmarkBeta(α=0, β=½)`; needed matrices `{"M","C"}` (no K). With `_acceleration`: `e=0` ⇒ `J = M + (Δt/2)·C`; the corrector `u += 0`, `u̇ += (Δt/2) δ`, `ü += δ`. With lumped M and no C, the lumped solver gives `δ = r / M` (NLS-002). | core | — | solid explicit tests (R2), `test_model_solver_dynamic_explicit` | — | Explicit solid dynamics is: predictor (u full step, v half from old a) → residual → `a_new` via lumped solve → corrector `v += Δt/2·a_new`, `a = a_new` because `ü` was kept and corrected by `δ = a_new − a_old`. Verify against the oracle. |
| TS-007 | Other Newmark presets | `newmark-beta.hh:159-177` | `TrapezoidalRule2` (α=½, β=½), `FoxGoodwin` (α=1/6, β=½), `LinearAceleration` [sic] (α=1/3, β=½). | core | — | — | — | Class name typo `LinearAceleration`. |
| TS-008 | Pseudo-time (static) | `pseudo_time.hh:30-59`; `pseudo_time.cc:30-75` | Order 0, needs `{"K"}`. Predictor no-op. Corrector `u += δ` (non-blocked). Jacobian `J = K` (profile copy + add) when the K release or blocked set changed. Residual contribution none. | core | — | `test_model_solver_{mumps,petsc}`, all static tests | — | — |
| TS-009 | `TimeStepSolver` base | `time_step_solvers/time_step_solver.hh:38-155`; `.cc:30-192` | It is a `ParameterRegistry` (sub-registry `non_linear_solver`) and a `SolverCallback` wrapping the model's callback. `setIntegrationScheme(dof, type, sol_type)` creates the scheme through `getIntegrationSchemeInternal`, then registers its needed matrices (`needed_matrices[name] = _mt_not_defined`) and asks the model for each matrix type. Matrices with a defined type are created in the DOF manager. `assembleMatrix("J")` creates J with the *common* type (min over defined types, so `_unsymmetric` < `_symmetric`) and calls the model's `assembleMatrix(name)` for every needed matrix with a defined type. `assembleMatrix(other)` assembles only that one. `assembleLumpedMatrix(id)` creates the lumped matrix if needed. `assembleResidual()` zeroes the residual then calls the model. `time_step` (default 0). `is_solution_increment=true` (unused). | core | — | all dynamic tests | — | Matrix-type enum ordering is semantically used (`std::min`). |
| TS-010 | `TimeStepSolverDefault` | `time_step_solver_default.hh:38-124`; `.cc:35-307` | Type `_dynamic_lumped` sets `is_mass_lumped`. Scheme factory: in lumped mode only `_forward_euler` and `_central_difference` are allowed (others throw); otherwise all 10 types. One scheme per DOF id (a second one throws). `solveStep(cb)` sets the callback, calls `non_linear_solver.solve(*this)`, unsets it. `predictor()`: model predictor first, then for each DOF save previous (if registered) and run the scheme predictor. `corrector()`: run each scheme corrector; if an increment array is registered, compute it (`_displacement` solution type → increment = solution; otherwise increment = u − previous, with a warning if previous is missing); then model corrector. `assembleMatrix("J")`: base, then each scheme's `assembleJacobian`, then `dof_manager.applyBoundary("J")`. `assembleResidual()`: if `M` is needed, (lumped) mass assembly first; zero the residual; model residual; each scheme's inertial residual. `assembleResidual(part)`: `"inertial"` → scheme residuals only; other parts → model. `beforeSolveStep()`: model hook, then `store()` all schemes. `afterSolveStep(false)`: `restore()` all schemes, then the model hook. | core | — | everything that solves | — | M is re-assembled through the model callback on every residual assembly (the models cache it with flags). Rust: keep the callback order exactly, because models rely on it (e.g. contact search in `beforeSolveStep`). |
| TS-011 | Solver-type enums | `aka_common.hh:168-305` | `TimeStepSolverType {_static, _dynamic, _dynamic_lumped, _not_defined}`; `IntegrationSchemeType {_pseudo_time, _forward_euler, _trapezoidal_rule_1, _backward_euler, _central_difference, _fox_goodwin, _trapezoidal_rule_2, _linear_acceleration, _newmark_beta, _generalized_trapezoidal}`; `DOFSupportType {_dst_nodal, _dst_generic}`; `MatrixType {_unsymmetric, _symmetric, _mt_not_defined}`. All `AKANTU_CLASS_ENUM` types have stream I/O for the parser (names without the leading underscore). | core | — | — | — | — |
| TS-012 | `TimeStepSolverDefaultExplicit` (dead) | `time_step_solver_default_explicit.hh:28-61` | Declared class without an implementation, never included or compiled into anything. | core (listed) | — | — | — | No behaviour; do not port. |

### 1.4 Non-linear solvers (NLS)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| NLS-001 | `NonLinearSolver` base | `src/model/common/non_linear_solver/non_linear_solver.hh:39-91`; `.cc:30-67` | Parsable (`ParserType::_non_linear_solver`). Parameter `type` (parsable). `checkIfTypeIsSupported()` throws if the type isn't in `supported_type` (`_auto` is accepted). `assembleResidual(cb)`: if `cb.canSplitResidual()` and the type is `_linear`, it does `zeroResidual(); cb.assembleResidual("external"); residual -= K·u (assembleMatMulDOFsToResidual("K",-1)); cb.assembleResidual("inertial")`; otherwise `cb.assembleResidual()`. `debug::NLSNotConvergedException(threshold, niter, error)`. | core | — | — | — | — |
| NLS-002 | `NonLinearSolverLumped` | `non_linear_solver_lumped.hh:38-72`; `.cc:35-120` | Supports `_lumped`. Parameter `b_a2x` (α, default 1, parsable/modifiable). `solve`: `beforeSolveStep`, `updateGlobalBlockedDofs`, `predictor`, `assembleResidual`, `x.resize(); x.zero()`, then `x_i = α·b_i / A_i` with A the lumped `"M"` (PETSc: `VecPointwiseDivide` + `VecScale`), `splitSolutionPerDOFs`, `corrector`, `afterSolveStep(true)`. | core | — | all explicit tests | Rank-local; no communication. The residual must already be complete on shared nodes (§3.6). | No guard for zero lumped mass (gives inf/NaN) and no blocked-DOF masking: the corrector skips blocked DOFs. |
| NLS-003 | Newton–Raphson family | `non_linear_solver_newton_raphson.hh:37-102`; `.cc:52-228` | Supports `_newton_raphson`, `_newton_raphson_modified`, `_newton_raphson_contact`, `_linear`. Creates a sparse solver on matrix `"J"` from `SparseSolverFactory` with `solver_options.sparse_solver_type`. Parameters: `threshold` (1e-10), `convergence_type` (`_solution`), `max_iterations` (10), `force_linear_recompute` (true, modifiable), `verbose_iteration` (false, modifiable); readable `error`, `nb_iterations`, `converged`. Algorithm in §2.3. On failure it throws `debug::NLSNewtonRaphsonNotConvergedException(threshold_normalized, n_iter, error, type)` **after** `afterSolveStep(false)`, which restores the stored state. | implicit | sparse solver (MUMPS/Eigen/PETSc) | `test_model_solver_*`; solid implicit / static (R2); contact implicit; phase-field static; heat static / implicit | Collective (norm allreduce, solver). | `_residual` criterion with an already-converged initial residual returns **without** calling `afterSolveStep` and without `n_iter` increments (`.cc:130-135`); `_residual_mass_wgh` silently behaves as `_solution`. Reproduce. |
| NLS-004 | `NonLinearSolverLinear` | `non_linear_solver_newton_raphson.hh:105-113`; `.cc:264-288` | NR subclass with `linear=true`, created by the factories for `_linear`. `solve_linear`: `beforeSolveStep`, update blocked, `predictor`, `assembleMatrix("J")` (always), `assembleResidual` (split variant if possible), one sparse solve, `corrector`, then `assembleResidual("internal")` if splittable else the full residual, then `afterSolveStep(true)`. No convergence check. | implicit | sparse solver | solid static linear tests; `CouplerSolidPhaseField` static default | Collective. | `force_linear_recompute` is ignored in `solve_linear` (J always rebuilt; the integration scheme release checks avoid recomputation). |
| NLS-005 | NR convergence test and norms | `non_linear_solver_newton_raphson.cc:231-261` | `testConvergence(v)`: `‖v‖₂` over local equations that are not globally blocked and are local-or-master (`isLocalOrMasterDOF`), with `allReduce(_sum)` of the squares followed by `sqrt`. Asserts not NaN. `error = norm`; converged iff `error < convergence_criteria_normalized`. | implicit | MPI allreduce | as NLS-003 | Shared DOFs are counted once (master). Pure ghosts and slaves are excluded. | Summation order (rank-local sequential then MPI reduction order) affects the last bits. |
| NLS-006 | `NonLinearSolverPETSc` (SNES) | `non_linear_solver_petsc.hh:40-129`; `.cc:35-206` | Supports `_petsc_snes`; requires the PETSc sparse solver. `SNESNEWTONLS` plus `SNESSetFromOptions`. Akantu parameters `max_iterations`, `threshold`, `convergence_type` are **rejected** (use `snes_max_it`, `snes_atol/rtol/stol`). Readable: `n_iter`, `convergence_reason`, `function_norm`, `solution_norm`, `update_norm`, `error`. `FormFunction`: copy x into the solution, split, `restoreLastConvergedStep`, `corrector`, `assembleResidual`, zero blocked residual entries, `residual *= -1`. `FormJacobian`: corrector, `assembleMatrix("J")`, copy. `solve`: `beforeSolveStep`, update blocked, `predictor`, assemble J, `x=0`, `rhs=0`, save `J_petsc` (!), `SNESSolve`, history (1000 entries), `error = a[n]/a[0]`, split, restore, `corrector`, `afterSolveStep(reason≥0)`. Throws `SNESNotConvergedException`. | petsc | PETSc SNES | Patch tests with `nls_type = _petsc_snes` (`test/test_model/patch_tests/patch_test_linear_fixture.hh:61-87`: `initDOFManager("petsc")`, `snes_max_it=10`, `snes_rtol=1e-10`, `snes_atol=2e-5`, `snes_stol=1e-10`; R2/R4 scope); the SNES branch of `test_phase_field_optimal_linear.cc:41-56` (compiled out, TAO is selected). | PETSc-parallel. | `J.saveMatrix("J_petsc")` writes a file on every solve (side effect). `saveSolution`/`restoreSolution` are not implemented. |
| NLS-007 | `NonLinearSolverTAO` | `non_linear_solver_tao.hh:44-101`; `.cc:38-243` | Supports `_petsc_tao`; requires the PETSc sparse solver. Type `TAOGPCG` plus `TaoSetFromOptions`. Rejects `max_iterations`, `threshold`, `convergence_type`, `absolute_threshold`, `relative_threshold` (use `tao_max_it`, `tao_gatol`, `tao_grtol`, `tao_gttol`), although it also registers its own `max_iterations` (10), `threshold` (1e-10), `convergence_type` params. Objective from the quadratic model: `f(x) = ½ xᵀJx + rᵀx`, gradient `Jx + r` with `r = −R(x)` (the assembled residual with blocked entries zeroed, negated). Hessian = J. `setBounds(lower, upper)` (`TaoSetVariableBounds`) is used for phase-field irreversibility. PETSc ≥ 3.17 API switch. Throws `TAONotConvergedException`. | petsc | PETSc TAO | `test_phase_field_tao` (`material_linear_tao.dat`, `test_one_element.msh`, PETSc) | PETSc-parallel. | Inconsistent parameter rejection (they are both registered and rejected). |
| NLS-008 | PETSc option forwarding (`ParsablePETSc`) | `src/io/parser/parsable_petsc.hh` (R1 file; behaviour used here) | Parser-section parameters are put into a private `PetscOptions` database as `-<name> <value>` and applied with `XSetFromOptions` inside `PetscOptionsPush/Pop`, then once more from the command line / environment. `set(name, value)` sets an Akantu parameter if one exists, otherwise a PETSc option. Names in `akantu_to_petsc_options` throw with a hint. | petsc | PETSc | — | — | Cross-reference R1. |
| NLS-009 | `NonLinearSolverType` enum | `aka_common.hh:171-206` | `linear, newton_raphson, newton_raphson_modified, lumped, newton_raphson_contact, petsc_snes, petsc_tao, auto`. | core | — | — | — | — |

### 1.5 Boundary conditions (BC)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| BC-001 | `BoundaryCondition<ModelType>` mixin | `src/model/common/boundary_condition/boundary_condition.hh:34-79`; `boundary_condition_tmpl.hh:32-235` | `initBC(model, dof_id, primal, dual)` / `initBC(model, dof_id, primal, primal_increment, dual)`. `applyBC(functor, group_name)` (an exception is wrapped with the group name), `applyBC(functor, ElementGroup)` (debug info if the group dimension ≠ dim−1), `applyBC(functor)` over all groups. Dispatches on `FunctorType::type` (`_dirichlet` / `_neumann`). Used by `SolidMechanicsModel` (R2), `PhaseFieldModel`, `CouplerSolidPhaseField` and `ContactMechanicsModel`. `CouplerSolidContactTemplate` forwards `applyBC` to its solid model. `StructuralMechanicsModel` is **not** a `BoundaryCondition`: users write `blocked_dofs` / `displacement` directly. | core | — | solid BC tests (R2), phase-field tests | See BC-004 / BC-005. | **Bug:** `applyBC(func)` iterates `element_group_end()` (an end iterator) as if it were a range (`boundary_condition_tmpl.hh:195-199`). It is uncompilable if instantiated and unused. Port as "apply to all element groups". |
| BC-002 | Dirichlet functors | `boundary_condition_functor.hh:44-136`; `boundary_condition_functor_inline_impl.hh:44-82` | `DirichletFunctor(axis)`: virtual `operator()(node, flags, primal, coord)` (the VectorProxy overload copies into Vectors for user overrides). `FlagOnly(axis)`: `flags[axis]=true`. `FixedValue(value, axis)`: `flags[axis]=true; primal[axis]=value`. `IncrementValue(value, axis)`: `flags[axis]=true; primal[axis]+=value` (`setIncrement`). `Increment(Vector)`: all flags true, `primal += value` (`setIncrement`). `BC::Axis` = `SpatialDirection {_x,_y,_z}`. | core | — | solid / phase-field / structural tests | — | User functors (C++ subclass, Python via R4) must be supported through a closure / trait. |
| BC-003 | Neumann functors | `boundary_condition_functor.hh:141-200`; `boundary_condition_functor_inline_impl.hh:87-114`; aliases `src/model/solid_mechanics/solid_mechanics_model.hh:481-482` | `NeumannFunctor`: pure `operator()(IntegrationPoint, dual, coord, normal)`. `FromHigherDim(Matrix σ)`: `dual = σ·n` (alias **`FromStress`**). `FromSameDim(Vector t)`: `dual = t` (alias **`FromTraction`**). `FreeBoundary`: `dual = 0`. | core | — | solid Neumann tests (R2) | — | Python names `FromStress` / `FromTraction` also exist (R4). |
| BC-004 | Dirichlet application procedure | `boundary_condition_tmpl.hh:54-87` | For every node of the group's node group, call the functor on views of `primal[n]`, `model.getBlockedDOFs()[n]` and `coords[n]`. Then `makeConsistentForPeriodicity(flags, _or)` and `makeConsistentForPeriodicity(primal, _id)`. | core | — | — | Applied on every local node in the group (including ghosts / pure ghosts): no communication. Group membership of ghost nodes comes from distribution (PART-008). | Order of functor calls is irrelevant (node-local). |
| BC-005 | Neumann integration procedure | `boundary_condition_tmpl.hh:91-190` | `dim==1` → `AKANTU_TO_IMPLEMENT`. For `ghost ∈ {not_ghost, ghost}` and each `(dim−1)` element type of the group: interpolate the coordinates at boundary quadrature points, call the functor per quadrature point with the precomputed normals, then `N^T t` (`computeNtb`), integrate per element, and scatter-add with `assembleElementalArrayLocalArray` into a zero `dual_local`. Then `makeConsistentForPeriodicity(dual_local)` (default op `_sum`) and `dual += dual_local`. | core | — | — | Integrates over **ghost** boundary elements too, which completes nodal forces on shared nodes without communication. | Uses the model's boundary FE engine (MODEL-008). The functor receives `IntegrationPoint{type, element, num_point, ghost_type}`. |

### 1.6 Sparse matrices and solver vectors (SPM)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| SPM-001 | `SparseMatrix` abstract base | `src/solver/sparse_matrix.hh:37-147`; `sparse_matrix.cc`; `sparse_matrix_inline_impl.hh` | Holds the id, `DOFManager&`, `MatrixType`, `size_` (initialised to the system size), `nb_proc`, `nb_non_zero`. API: `clearProfile`, `set(v)`, `zero`, `add(i,j)` (profile; returns the entry index), `add(i,j,v)`, `mul(α)`, `add(B, α)` (double dispatch through `addMeTo`), `matVecMul(x, y, α=1, β=0)`, `applyBoundary(block_val=1)`, `copyProfile(other)`, `operator*=`, `isFinite`, `operator()(i,j)`, `getRelease()`, `min()`, `saveProfile`, `saveMatrix`. | core | — | `test_sparse_matrix_*` | — | — |
| SPM-002 | `SparseMatrixAIJ` storage | `sparse_matrix_aij.hh:39-186`; `sparse_matrix_aij_inline_impl.hh:30-108`; `sparse_matrix_aij.cc:39-51` | COO triplets `irn`, `jcn` (**1-based global** indices, `Int`) and `a`, plus a hash map `(i,j)→k`. For `_symmetric`, the key is canonicalised to `(min,max)`, but `irn/jcn` keep the orientation of the **first** insertion (both triangles may appear). `add(i,j)` appends a zero entry if new, grows `size_` to `max(i,j)+1`, bumps `profile_release` and `value_release`. `add(i,j,v)` adds into the entry (bumps the value release). `operator()(i,j)` const returns 0 if absent; the mutable version asserts the entry exists and bumps the value release. `clearProfile` empties everything (`size_=0`). `resize(n)` only sets `size_`. `getRelease()` = value release. | core | — | `test_sparse_matrix_profile` (`triangle.geo` → `test_solver_mesh`), `test_sparse_matrix_assemble` | Each rank holds triplets for its own `_not_ghost` element contributions with global indices; duplicates across ranks are summed by MUMPS (ICNTL(18)=3). | Insertion order defines triplet order, which defines MUMPS input order. Keep it for reproducibility. |
| SPM-003 | AIJ element-matrix insertion | `sparse_matrix_aij_inline_impl.hh:112-181` | `addValues(is, js, Ke, values_type)`: symmetric matrix + symmetric `Ke` → loop `j ≥ i` over **element-local** indices, add `Ke(i,j)`; symmetric matrix + unsymmetric `Ke` → add `Ke(i,j)` only when the **global** `js(j) ≥ is(i)`; unsymmetric matrix → add the full `Ke`. Entries with index `≥ size_` are skipped (`c < size_`). | core | — | `test_sparse_matrix_assemble` | — | The symmetric+symmetric path never reads `Ke(j,i)`, `j<i`, so element matrices must really be symmetric. A negative index (-1) passes the `< size_` filter (assert in debug). |
| SPM-004 | AIJ Dirichlet application | `sparse_matrix_aij.cc:54-82` | For every triplet whose row or column is globally blocked (blocked status looked up via `globalToLocalEquationNumber`): off-diagonal → 0; diagonal → `block_val` (1) if the row is **local-or-master** on this rank, else 0. Bumps the value release. | core | — | `test_petsc_matrix_apply_boundary` (PETSc, unregistered) | Exactly one rank contributes 1 on each blocked diagonal, so the distributed sum is 1. | Requires every triplet's global index to be known locally (true for own-element rows). |
| SPM-005 | AIJ `matVecMul` | `sparse_matrix_aij.cc:166-208` | `y = β y + α A x` on **local** arrays: global→local for i and j; for symmetric storage the transpose contribution is added for `i≠j`. Then `reduceSynchronizeArray<AddOperation>(tmp)` and `y += tmp` with `tmp` a zeroed copy. | core | — | `test_sparse_matrix_product` (`bar.msh`, `.verified`) | **Bug:** the reduction is applied to a zero array, so it is a no-op; in parallel `y` holds rank-partial products on shared DOFs (consistent with "partial" matrices but **inconsistent** with the "complete" residual convention). It matters for consistent-mass explicit dynamics, Rayleigh damping and `NonLinearSolver::assembleResidual` split mode in parallel. See Q4. | Decide on parity versus fix after an oracle run with 2 ranks. |
| SPM-006 | AIJ utilities | `sparse_matrix_aij.cc:211-290`; `sparse_matrix_aij_inline_impl.hh:184-186` | `copyProfile(other)`: copy irn/jcn, rebuild the hash, zero values, copy size and profile release, bump the value release. `copyContent` (memcpy values; profiles must match). `addMeTo(B, α)`: `B.add(i-1, j-1, α a)` for each triplet (B's profile may grow), AIJ targets only (others silently ignored). `mul(α)`, `set(v)`, `isFinite`, `min()`. | core | — | via integration schemes | — | `addMeTo` into a non-AIJ matrix does nothing silently. |
| SPM-007 | Matrix Market output | `sparse_matrix_aij.cc:85-163`; `solver_vector_default_tmpl.hh:79-103`; `solver_vector_distributed.cc:83-93`; `sparse_matrix_petsc.cc:94-108`; `solver_vector_petsc.cc:279-312` | AIJ `saveMatrix(file)`: `%%MatrixMarket matrix coordinate real {symmetric\|general}`, header `size size nnz`, 1-based triplets, precision `digits10`, `.mtx` extension added, `_rank-<r>` suffix if nb_proc > 1. `saveProfile` is a pattern file written sequentially by rank with barriers. Vectors: `coordinate real general`, `size 1 count`, only local-or-master entries with global index +1. PETSc uses `PetscViewer` MatrixMarket. | core / petsc | — | `test_sparse_matrix_profile` (prints profile), `.verified` files | Per-rank files. | Useful for oracle fixtures. The symmetric header plus mixed-triangle triplets may not be valid Matrix Market strictly. |
| SPM-008 | `SparseMatrixPETSc` | `sparse_matrix_petsc.hh:39-144`; `.cc:34-272` | `MatCreate` + `MatSetFromOptions` + sizes `(pure_local, pure_local, N, N)` + L2G mapping; `MAT_NEW_NONZERO_LOCATIONS`; `MAT_SYMMETRIC` if symmetric. `add`/`addLocal`/`addValues` use `MatSetValue(s)[Local]` with `ADD_VALUES` (unsymmetric values into a symmetric matrix clear the symmetric flags). `applyModifications` = assembly begin/end and release++. `applyBoundary`: `MatZeroRowsColumnsLocal(blocked, block_val)`. `matVecMul`: `w = A x` (0 if release == 0), `y = β y + α w`. `addMeTo` = `MatAXPY(SAME_NONZERO_PATTERN)`. `copyProfile` = `MatDuplicate(DO_NOT_COPY_VALUES)`. `zero` = `MatZeroEntries`. `clearProfile` = `MatResetPreallocation`. `set`, `min`, `operator()` are not implemented. | petsc | PETSc Mat | Unregistered sources: `test_petsc_matrix_profile[_parallel].cc` (+`.verified`, `.sh`), `test_petsc_matrix_diagonal.cc`, `test_petsc_matrix_apply_boundary.cc`, `test_solver_petsc.cc` (+`_parallel.sh`). | Distributed rows. | Low priority. Note that the `.sh`/`.verified` harness tests are not registered in `test/test_solver/CMakeLists.txt`. |
| SPM-009 | `SolverVector` abstract | `src/solver/solver_vector.hh:35-82` | `resize()`, `set(v)`, `zero()`, conversion to `const Array<Real>&`, `size()` (global), `localSize()`, `operator+`, `copy`, `release()` counter, `printself`, `isFinite`, `isDistributed` (default false), `saveVector`. | core | — | — | — | — |
| SPM-010 | Default (array) solver vectors | `solver_vector_default.hh:34-127`; `solver_vector_default_tmpl.hh` | `SolverVectorArray` (abstract `getVector()`), `SolverVectorArrayTmpl<Array_>` (owning `Array<Real>` = `SolverVectorDefault`, or a wrapping reference = `SolverVectorDefaultWrap`, `make_solver_vector_default_wrap`). `resize` = `localSize()` (local system size) and release++. `size()` = global system size. `copy` copies the release. `getGlobalVector()` / `setGlobalVector()` are the identity for the non-distributed case. | core | — | — | — | — |
| SPM-011 | `SolverVectorDistributed` | `solver_vector_distributed.hh:30-50`; `.cc:32-93` | `getGlobalVector()`: on rank 0, gathers the master values of all ranks into an array of size `system_size` via `DOFSynchronizer::gather` (other ranks send). `setGlobalVector(sol)`: rank 0 scatters, then `synchronizeArray` makes slaves equal to masters. `isFinite` uses `allReduce(_land)`. `isDistributed()=true`. Rank-suffixed save. | core | MPI | `test_sparse_solver_mumps` (PARALLEL) | Gather = only local/master entries (one copy per DOF); **values on slave DOFs are discarded**. That is consistent because the residual is complete on all local nodes. | MUMPS receives a centralized dense RHS (ICNTL(20)=0) built by this gather. |
| SPM-012 | `SolverVectorPETSc` | `solver_vector_petsc.hh:46-237`; `.cc:45-312` | Wraps `Vec`. Sizes `(pure_local, N)`, `VecMPISetGhost` (slave DOFs), L2G. `getValuesLocal` uses the ghosted local form (with a ghost update) or a local vector; `addValuesLocal` likewise; `getValues/addValues` use global indices; negative indices are ignored. Conversion to `Array` copies the local part into a cache. `operator=`/`copy`, `operator+` (AXPY), `isFinite` (VecMin/Max), `saveVector`. Helpers `internal::PETScWrapedVector`, `PETScLocalVector<read_only>`, `make_petsc_local_vector`, `make_petsc_wraped_vector` (C++20 concepts). | petsc | PETSc Vec | — | Ghost update `INSERT_VALUES, SCATTER_FORWARD`. | PETSc < 3.18 path has a bug: `VecSetType(x, roottype)` on the wrong vector (`solver_vector_petsc.cc:169`). |
| SPM-013 | `TermsToAssemble` | `src/solver/terms_to_assemble.hh:32-91` | Container of `(i, j, val)` terms in local DOF-array index space for a `(dof_id_m, dof_id_n)` pair. `operator()(i,j)` appends and returns a term (assignable / `+=`). Iterable. | core | — | — | — | — |

### 1.7 Sparse direct / iterative solvers (SOLV)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| SOLV-001 | `SparseSolver` base and factory | `src/solver/sparse_solver.hh:33-122`; `sparse_solver.cc:31-70`; `sparse_solver_inline_impl.hh` (empty) | Parsable (`ParserType::_solver`) and `CommunicatorEventHandler` (registered with the communicator; `onCommunicatorFinalize` → `destroyInternalData`). Members: DOF manager, `matrix_id`, `parallel_method`, communicator. Virtuals: `initialize`, `analysis`, `factorize`, `solve`, `destroyInternalData`, `beforeStaticSolverDestroy`. `SolverParallelMethod {_not_parallel, _fully_distributed, _master_slave_distributed}`. `SparseSolverFactory` keyed by `SparseSolverType`. `instantiateSparseSolver<Solver, DOFManagerClass>` checks the DOF manager type at allocation. `debug::SingularMatrixException`. | core | — | — | — | `createSynchronizerRegistry` is empty. |
| SOLV-002 | MUMPS solver | `sparse_solver_mumps.hh:38-142`; `.cc:54-373` | Double precision `DMUMPS`. `parallel_method` = `_fully_distributed` with MPI, else `_not_parallel`. Initialise: `par=1` (host participates; 0 for master–slave, unused), `comm_fortran = MPI_Comm_c2f(world)`, `sym = 2` (general symmetric) if the matrix is symmetric else 0, `job=-1`. ICNTL settings and the full solve flow are in §2.5. Analysis is redone when the profile release changes, factorisation when the value release changes, then solve (`job=3`) with the dense centralized RHS on rank 0. Result: `setGlobalVector(rhs_sol)` then `splitSolutionPerDOFs()`. | mumps (z_mumps.cmake) | MUMPS (seq or par) + MPI/ScaLAPACK for par | `test_sparse_solver_mumps` (PARALLEL) | Fully distributed assembled input (ICNTL(18)=3), centralized RHS/solution on rank 0 (ICNTL(20)=0, ICNTL(21)=0). | The solution is bitwise dependent on MUMPS version, ordering (ICNTL(7) default, ICNTL(28)=0 automatic), BLAS and number of ranks. Differential tests need tolerances (§2.5); see `docs/NUMERICAL_VALIDATION.md`. A Rust port needs either MUMPS bindings or a pure-Rust direct solver (`faer`) with tolerance-based validation. |
| SOLV-003 | MUMPS error handling | `sparse_solver_mumps.cc:322-364` | `INFO(1)` is reduced across ranks with min (errors) and min of negated (warnings). `-10` → `SingularMatrixException`; `-9` → `ICNTL(14) += 10` and a recursive re-solve with releases decremented (forcing re-analysis / factorisation) until `ICNTL(14)==90`, then error; other negatives → `AKANTU_ERROR` with `INFO(1)`; positives → warning. | mumps | MUMPS | — | Collective. | MUMPS default ICNTL(14)=20, so the retry steps are 30…80, then 90 stops. |
| SOLV-004 | Eigen sparse LU | `sparse_solver_eigen.hh:38-70`; `.cc:9-81` | Asserts nb_proc == 1. Builds an `Eigen::SparseMatrix<Real>` from triplets (symmetric storage expanded: `(i,j)` and `(j,i)` for i≠j; duplicate triplets summed by `setFromTriplets`). `SparseLU`: `analyzePattern` on a profile release change, `factorize` on a value release change, `solve(b)`; `b` = residual, x set as the global vector, split. Registered as `SparseSolverType::_eigen` for `DOFManagerDefault`. | core | Eigen3 (3.3.7 required / 5.0.1 third-party) | `test_sparse_solver_eigen` (registered with `PACKAGE mumps`, PARALLEL — but the solver asserts serial) | Serial only. | Default sparse solver when MUMPS is absent. Note `setA` rebuilds A when the release changed (it compares `last_value_release` with `getRelease()`). The registration of `test_sparse_solver_eigen` as PARALLEL looks wrong. |
| SOLV-005 | PETSc KSP solver | `sparse_solver_petsc.hh:38-62`; `.cc:36-101` | `KSPCreate(world)`; options via `ParsablePETSc` (`KSPSetFromOptions`). `solve`: `KSPSetOperators(J, J)` + `KSPSetUp` (exception text "is your matrix singular?"), `KSPSolve(residual, solution)`, split. Registered `_petsc` for `DOFManagerPETSc`. | petsc | PETSc KSP/PC | `test_sparse_solver_petsc` (PARALLEL) | Distributed. | KSP/PC types default to PETSc defaults (GMRES + block-Jacobi/ILU) unless configured: results are tolerance-based. |
| SOLV-006 | Sparse solver selection | `model_solver.cc:325-339`; `model_solver.cc:152-154`; `packages/implicit.cmake` | `_auto` → MUMPS (if registered) → Eigen for the default DOF manager; PETSc for the PETSc DOF manager. A parser key `sparse_solver_type` in a `time_step_solver` section overrides it. `SparseSolverType` enum: `mumps, eigen, petsc, auto`. | core | — | — | — | Rust default: a pure-Rust direct solver, with MUMPS as an optional oracle-equivalence backend. |
| SOLV-007 | PETSc wrapper structs (legacy) | `src/solver/petsc_wrapper.hh:37-64` | `PETScMatrixWrapper {Mat, AO, ISLocalToGlobalMapping, MPI_Comm}`, `PETScSolverWrapper {KSP, Vec, Vec, MPI_Comm}`, `CHKERRXX` macro for C-language PETSc. Not used by current sources. | petsc | PETSc | — | — | Build glue / legacy, no behaviour. |

### 1.8 Communicator and synchronizers (MPI)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| MPI-001 | `Communicator` API | `src/synchronizer/communicator.hh:77-528`; `communicator.cc:30-181`; `communicator_inline_impl.hh` | Process-wide static communicator (`getStaticCommunicator()` / `(argc, argv)`). Point-to-point: `send`/`receive` (blocking), `asyncSend`/`asyncReceive`, `probe`/`asyncProbe<T>`, for `Array<T>`, `std::vector<T>`, tensors, `CommunicationBuffer` (dynamic receive probes the size), and scalars. `CommunicationMode {_auto, _synchronous, _ready}`. Collectives: `allReduce`, `scan`, `exclusiveScan` (rank 0 result = `T()`), `allGather`, `allGatherV`, `reduce`, `gather` (in place on the root, or into `Array` on every rank), `gatherV`, `broadcast` (dynamic buffers broadcast the size first), `barrier`, `asyncBarrier`. Requests: `test`, `testAll`, `wait`, `waitAll`, `waitAny` (−1 when none), `freeCommunicationRequest`. Explicit instantiations for `bool, Real, UInt, Int, char, NodeFlag, SCMinMaxLoc<Real,int>, size_t` (+`int` if `Int` is 64-bit). `getMaxTag`/`getMinTag`. The destructor sends `FinalizeCommunicatorEvent` to handlers. | core (API), MPI (backend) | MPI | `test_communicator` (gtest in `test_synchronizers`, PARALLEL) | — | Rust: wrap `mpi` crate (rsmpi) behind a trait with a serial dummy. Keep `exclusiveScan`'s rank-0 = 0 semantics. |
| MPI-002 | MPI backend | `communicator_mpi_inline_impl.hh:51-499`; `mpi_communicator_data.hh:56-126` | `MPICommunicatorData`: `MPI_Init(nullptr,nullptr)` if not already initialised (then also finalises), an error handler on `MPI_COMM_WORLD` throwing `CommunicationException` with the MPI error string, `setMPICommunicator`. `SynchronizerOperation → MPI_Op` map (`_sum, _min, _max, _prod, _land, _band, _lor, _bor, _lxor, _bxor, _min_loc, _max_loc, _null`). Datatype map including `bool→MPI_CXX_BOOL`, `SCMinMaxLoc<double,int>→MPI_DOUBLE_INT`, `NodeFlag→uint8`. All collectives use `MPI_IN_PLACE` where applicable. `MPI_Exscan` with rank-0 zeroing. `getMaxTag` from the `MPI_TAG_UB` attribute. | MPI | MPI ≥ 3 (MPI_Ibarrier) | `test_synchronizers` | — | `MPI_CXX_BOOL` requires the MPI C++ datatype support even though `MPI_CXX_SKIP_MPICXX`. |
| MPI-003 | Dummy (serial) backend | `communicator_dummy_inline_impl.hh:29-145` | Everything is a no-op; `getNbProc()=1`, `whoAmI()=0`; `exclusiveScan` fills `T()`; `scan` copies; `asyncProbe` returns true; `test*` true; `waitAny` −1; `gather(values, n, gathered)` memcpy. | core (when not MPI) | — | serial tests | — | **Bug:** dummy `gatherImpl` memcpy's `nb_values` **bytes**, not `nb_values*sizeof(T)` (`:127`). |
| MPI-004 | Requests, status, min/max-loc | `communication_request.hh:33-98`; `communicator.cc:34-55` | `InternalCommunicationRequest(source, dest)` with a global counter id; `CommunicationRequest` shared-ptr wrapper (`free`, `isFreed`, source / dest); `CommunicationStatus {source, size, tag}`; `SCMinMaxLoc<T1,T2>{min_max, loc}`. | core | — | — | — | — |
| MPI-005 | Communication buffers | `communication_buffer.hh:33-179`; `communication_buffer_inline_impl.hh:31-290` | `CommunicationBufferTemplated<is_static>`: char array plus pack / unpack pointers. `operator<<`/`>>` for standard-layout scalars (memcpy, aligned unpack), tensors (raw data), `std::vector<T>` / `std::string` (size_t length prefix then elements). `sizeInBuffer` helpers. Static buffers assert on overflow; dynamic buffers grow on pack. `resize` (dynamic → resize to 0), `reserve`, `reset`, `zero`, `getPackedSize`, `getLeftToUnpack`, `empty`, `extractStream<T>` (debug). `CommunicationBuffer` = static, `DynamicCommunicationBuffer` = dynamic. | core | — | via all synchronizer tests | Binary layout is native (no endianness handling). | The Rust port can use its own serialization as long as both ends are Rust. Exact byte layout only matters for debug sanity-check sizes. |
| MPI-006 | Message tags | `communication_tag.hh:33-107` | `Tag::genTag(proc, msg_count, comm_tag[, hash])` = `((proc & 0xFFFFF)<<12) + ((msg_count & 0xFF)<<4) + (comm_tag & 0xF)`, XOR `hash`. `operator int` reduces modulo `max_tag` (set at `initialize`). `CommTags`: `_sizes=1, _connectivity=2, _data=3, _partitions=4, _nb_nodes=5, _nodes=6, _coordinates=7, _nodes_type=8, _mesh_data=9, _element_group=10, _node_group=11, _modify_scheme=12, _gather_initialization=1, _gather=2, _scatter=3, _synchronize=15, _reduce=16, _periodic_slaves=17, _periodic_nodes=18`. | core | — | — | Tag collisions are possible in principle (masking); the hash is `std::hash<std::string>(synchronizer_id) % max_tag`. | `std::hash` differs across implementations, so tags are implementation-defined; irrelevant for Rust–Rust. |
| MPI-007 | `Communications<Entity>` container | `communications.hh:34-251`; `communications_tmpl.hh` | Per send/recv direction: `schemes[proc] → Array<Entity>`, per-tag `Communication` per proc (buffer, size, request), per-tag counters, pending counts and "size computed" flags. Iterators over communications (with `waitAny` returning the ready descriptor), tags and schemes. `createScheme/createSendScheme/createRecvScheme`, `resetSchemes`, `setCommunicationSize`, `initializeCommunications(tag)`, `invalidateSizes`, `swapSendRecv`, `hasPendingRecv/Send`, `waitAll*`, `freeSendRequests/freeRecvRequests`, `incrementCounter`/`getCounter`. | core | — | `test_synchronizers` | — | — |
| MPI-008 | `CommunicationDescriptor<Entity>` | `communication_descriptor.hh:34-137`; `communication_descriptor_tmpl.hh` | `Communication {size, buffer, request, type}`. The descriptor binds the communication, scheme, tag, proc, rank and per-tag counter: `packData(accessor)` / `unpackData(accessor)` (type asserted), `postSend(hash)` (asserts packed size == declared size; tag `genTag(rank, counter, tag, hash)`), `postRecv(hash)` (tag `genTag(proc, counter, tag, hash)`), `freeRequest`, `resetBuffer`. | core | — | — | Counters keep consecutive async rounds of the same tag apart (8 bits of the counter in the tag). | — |
| MPI-009 | Data accessor pattern | `src/synchronizer/data_accessor.hh:37-398`; `data_accessor_tmpl.hh:32-111` | `DataAccessor<T>` (virtual base `DataAccessorBase`): `getNbData(entities, tag)`, `packData(buffer, entities, tag)`, `unpackData(buffer, entities, tag)`. Specialisations for `Element` and `Idx` have no-op defaults plus helpers: `pack/unpackNodalDataHelper(Array, buffer, elements, mesh)` (per element, per node of the connectivity), `pack/unpackElementalDataHelper(ElementTypeMapArray, buffer, elements[, fem])` (1 or nb_quad values × nb_component per element), `pack/unpackDOFDataHelper(Array, buffer, dofs)`. Reduction operators `AddOperation`, `IdentityOperation`, `OrOperation` (`Vector<bool>` specialisations). `ReduceDataAccessor<Entity, Op, T>` (unpack combines `vect = op(vect, received)`), `SimpleIdxDataAccessor<T>` = identity reduce on `Idx`, `SimpleElementDataAccessor<T>` (copies per-element component values). | core | — | `test_data_accessor.hh`, `test_dof_data_accessor.hh` in `test_synchronizer/` | Pack and unpack must iterate the same entity order on both sides. | Rust trait `DataAccessor<E>` with `nb_data` / `pack` / `unpack`; models implement it per tag. |
| MPI-010 | `Synchronizer` base | `synchronizer.hh:39-111`; `synchronizer.cc:31-42`; `synchronizer_tmpl.hh:33-123` | Holds the id, `hash_id = std::hash(id) % max_tag`, communicator, nb_proc, rank. Generic methods `slaveReductionOnce`, `synchronizeOnce`, `synchronize`, `asynchronousSynchronize`, `waitEndSynchronize`, `computeBufferSize` dispatch with `dynamic_cast` to `SynchronizerImpl<Element>` or `<Idx>`. | core | — | — | — | — |
| MPI-011 | `SynchronizerImpl<Entity>`: synchronization rounds | `synchronizer_impl.hh:31-196`; `synchronizer_impl_tmpl.hh:32-291` | **Persistent async** (`asynchronousSynchronize`): compute buffer sizes once per tag (cached until `invalidateSizes`), increment the tag counter, error if receives are still pending, post all receives, pack and post all sends. `waitEndSynchronize`: `waitAny` over receives → unpack → free; then wait for and free the sends. `synchronize` = async + wait. **One-shot** (`communicateOnce`): sizes computed on the fly (empty schemes / zero sizes skipped), receives posted first, then packed sends, then `waitAny` unpack loop. `synchronizeOnce` = send→recv direction (masters → ghosts / slaves). `slaveReductionOnce` = **reverse direction** (recv scheme sends, send scheme receives), tag `_reduce`. `computeAllBufferSizes`. | core | MPI | `test_synchronizer_communication`, `test_node_synchronizer`, `test_dof_synchronizer` | Non-blocking point-to-point only, plus the root gather / scatter in MPI-012. | Unpack order follows receive completion order (`waitAny`): data accessors must be order-independent across procs. They are, because each proc's buffer is unpacked into its own scheme entities. |
| MPI-012 | Scheme manipulation | `synchronizer_impl_tmpl.hh:294-437` | `reset()` empties schemes. `split(other, pred)` moves entities matching `pred` from `other`'s schemes into this one. `updateSchemes(fn(scheme, proc, sr))`. `filterScheme(pred)`: keeps recv entities satisfying `pred` and sends the kept positions to the owner (tag `_modify_scheme`), which filters its send scheme accordingly (blocking probe / receive). `swapSendRecv()`, `copySchemes`, `operator=`. All of them invalidate sizes. | core | MPI | — | `filterScheme` is collective among neighbours. | — |
| MPI-013 | Root gather / scatter and array sync | `synchronizer_impl_tmpl.hh:512-840` | `initScatterGatherCommunicationScheme()` (only for `Idx`; otherwise `TO_IMPLEMENT` when nb_proc > 1): each rank sends the global ids of its local/master entities (`fillEntityToSend`) to root 0; root records `master_receive_entities[p]`. Re-run when `hasChanged()` (allreduce `_lor` of `entities_changed`). `gather(to_gather, gathered)` on the root and `gather(to_gather)` on the others: per-entity values, placed by global id. `scatter(scattered, to_scatter)` / `scatter(scattered)`: the reverse, followed by `synchronizeArray` (slaves ← masters). `synchronizeArray(Array<T>)` = `synchronizeOnce` with `SimpleIdxDataAccessor` (tag `_whatever`). `reduceSynchronizeArray<Op>(Array<T>)` = `slaveReductionOnce` (slaves → master, combined with Op) then `synchronizeArray`. | core | MPI | `test_dof_synchronizer` (`bar.msh`, PARALLEL): gather / scatter / synchronize of DOF arrays; `test_node_synchronizer` | Root = 0 hard-coded. | Root gather of the full RHS limits scalability (MUMPS centralized RHS). |
| MPI-014 | Debug sanity checks in messages | `synchronizer_impl_tmpl.hh:440-509`; `element_synchronizer.cc:184-268`; `node_synchronizer.cc:125-208` | In debug builds every message carries a header (tag, nb_data, proc, rank) plus per-entity checks. Elements: barycenter (exception if the distance is non-zero, `Math::are_float_equal`) and global node connectivity (warning on mismatch, skipped for `_smmc_facets_conn` / `_giu_global_conn` and pure ghost nodes). Nodes: global id (skipped for `_giu_global_conn`), node flag compatibility (master↔slave, pure ghost), coordinates. | core (non-NDEBUG) | — | all parallel tests in debug | Message sizes differ between debug and release. | Optional in Rust (a feature flag). Valuable for debugging distribution. |
| MPI-015 | `SynchronizerRegistry` | `synchronizer_registry.hh:37-75`; `.cc:29-107` | `multimap<SynchronizationTag, Synchronizer*>` plus one data accessor. `registerSynchronizer(sync, tag)`, `registerDataAccessor(acc)`, `synchronize(tag)` / `asynchronousSynchronize(tag)` / `waitEndSynchronize(tag)` over all synchronizers registered for the tag (asserts that an accessor is set). | core | — | — | — | — |
| MPI-016 | `ElementSynchronizer` | `element_synchronizer.hh:39-176`; `element_synchronizer.cc:42-268` | `SynchronizerImpl<Element>` plus `MeshEventHandler` (priority `_ehp_synchronizer`=10). Send scheme to proc p = local elements that are ghosts on p; recv scheme from p = ghost elements owned by p (both filled during distribution, PART-007). `element_to_prank` (lazily built: default = own rank, recv entities → owner). `getRank(element)`. `onElementsChanged` substitutes old → new elements in the schemes. `onElementsRemoved`: `filterScheme(not removed)` then renumbering. Copy constructor. Many declared static helpers (`synchronizeTagsSend`, `fillTagBuffer`, `synchronizeElementGroups`, `synchronizeNodeGroupsMaster/Slaves`, `fillNodesType`, `removeElements`, …) are **declared but never defined** (dead declarations). | core | MPI | `test_synchronizer_communication`, `test_data_distribution`, all parallel model tests | Used by all models for ghost-element data (`_smm_*`, `_diffusion*`, `_pfm_damage`, `_csp_*`, …). | — |
| MPI-017 | `NodeSynchronizer` | `node_synchronizer.hh:31-90`; `node_synchronizer.cc:29-229`; `node_synchronizer_inline_impl.hh` | `SynchronizerImpl<Idx>` plus `MeshEventHandler`. Send scheme = master nodes needed by p (local ids); recv scheme = **slave** nodes owned by p (pure ghosts are not part of it). Built at distribution (PART-008). `onNodesAdded` rebuilds the recv schemes from flags and pranks, sends global ids to the owners (tag `genTag(rank, proc, 0xcafe)`), and the owners rebuild their send schemes for the existing neighbours. `getRank(node)` = `mesh.getNodePrank`. Gather / scatter on local/master nodes by global id (`canScatterSize = nb_nodes`, `gatheredSize = nb_global_nodes`). | core | MPI | `test_node_synchronizer` (in `test_synchronizers`, PARALLEL) | Master→slave only. | `onNodesAdded` only updates send schemes for procs that already had one: a new neighbour after cohesive insertion is not handled. |
| MPI-018 | `DOFSynchronizer` | `dof_synchronizer.hh:37-67`; `dof_synchronizer.cc:49-214`; `dof_synchronizer_inline_impl.hh` | Built by `DOFManagerDefault` for distributed meshes. Per nodal DOF id, it translates node schemes into **local equation** schemes: for each node in a node scheme, all equations of that node (found by linear search in `associated_nodes`), sorted by global number. Gather / scatter entities = local-or-master equations by global number (`canScatterSize = local_system_size`, `gatheredSize = system_size`). `onNodesAdded` rebuilds the schemes. `getRank` is not implemented. | core | MPI | `test_dof_synchronizer`, `test_dof_synchronizer_communication.cc` (**not registered**) | — | `registerDOFs` cost is O(nodes × dofs) (`std::find` per node). Generic DOFs are never synchronized. |
| MPI-019 | `PeriodicNodeSynchronizer` | `periodic_node_synchronizer.hh:30-72`; `.cc:33-120`; `_tmpl.hh:32-77` | A `NodeSynchronizer` whose schemes link remote periodic masters. `update()` rebuilds `masters_list` / `slaves_list` from `mesh.getPeriodicMasterSlaves()`; for distributed meshes the masters that are not local/master here are received from their prank (tag `genTag(0, count, _modify_scheme)`, synchronous sends, `receiveAnyNumber`). `synchronizeOnceImpl` / `waitEndSynchronizeImpl` do the remote part and then copy master→slave locally (pack masters, unpack slaves). `synchronizeArrayPBC` = remote sync + local copy. `reduceSynchronizeArrayPBC<Op>` = local slave→master reduce, remote reduce, local copy. | core | MPI | `test_solid_mechanics_model_pbc_parallel` (R2, in `AKANTU_PARALLEL_TESTS`) | Mixed local / remote periodic pairs. | Static counter `count` in `update()`: tags depend on the call history (all ranks must call it equally). |
| MPI-020 | `FacetSynchronizer` | `facet_synchronizer.hh:31-96`; `facet_synchronizer.cc:35-196` | An `ElementSynchronizer` on `mesh_facets` (dimension − 1). Ghost facet owner = **minimum rank** among the owners of the adjacent recv (ghost) elements, else own rank. Recv scheme from p = ghost facets owned by p, adjacent to elements received from p, de-duplicated. Facets are matched by **sorted global node connectivity**: the receiver sends the sorted connectivities per facet type to the owner (tag `genTag(proc, type, 1337)`, synchronous sends, `receiveAnyNumber`), and the owner finds each one with a linear search in its local facets to build the send scheme (exception if not found). Declared helpers (`updateDistributedSynchronizer`, `buildSendElementList`, …) are not defined. | core (used with cohesive_element) | MPI | `test_facet_synchronizer` (in `test_synchronizers`; `facet.geo` → 3-D order 2 mesh; packages parallel + cohesive_element) | Used for parallel cohesive insertion (`_smmc_facets*`, `_ce_*`). | O(n²) facet matching (`std::find`); replace with a hash map in Rust (same result). |
| MPI-021 | `GridSynchronizer` | `grid_synchronizer.hh:36-86`; `_tmpl.hh:30-59`; `grid_synchronizer.cc:36-464` | Builds an *additional* ghost layer from a `SpatialGrid<E>` (`E = Element` or `IntegrationPoint`). Bounding boxes are grown by one cell spacing on each side and all-gathered. For each intersecting proc, the elements of the grid cells in the intersection are sent (send scheme) as global connectivities, one type at a time (`SIZE_TAG`/`DATA_TAG`; terminator `_not_defined`). The receiver creates new ghost elements (or increments the ghost counter if an identical connectivity exists) and new pure-ghost nodes, asks the owners for the missing node coordinates (`ASK_NODES_TAG`/`SEND_NODES_TAG`), and finally sends `NewNodesEvent` / `NewElementsEvent`. Optional registration in a `SynchronizerRegistry` for a set of tags. | core | MPI | `test_grid_synchronizer.cc`, `test_grid_synchronizer_check_neighbors.cc` (**not registered**); used by the non-local toolbox (R2) | Requires the grid of the local + ghost region. | Duplicated element detection uses `Array::find(conn)` (linear). New ghosts get counter 1. |
| MPI-022 | `receiveAnyNumber` (unknown message count) | `communicator_inline_impl.hh:45-87` | NBX-style dynamic sparse exchange: loop `asyncProbe(_any_source, tag)` → receive → process. When all (synchronous-mode) sends have completed, post `MPI_Ibarrier` and stop when it completes. Returns immediately if nb_proc == 1. | core | MPI-3 | via facet / periodic synchronizers | Requires the senders to use `CommunicationMode::_synchronous` for correctness. | — |
| MPI-023 | Communicator event handling | `communicator_event_handler.hh:30-47`; `sparse_solver.cc:37-70` | `FinalizeCommunicatorEvent` is sent on communicator destruction; `CommunicatorEventHandler::onCommunicatorFinalize`. Used by sparse solvers to destroy MUMPS instances before `MPI_Finalize`. | core | — | — | — | In Rust use `Drop` ordering. |
| MPI-024 | `SynchronizationTag` catalogue | `aka_common.hh:352-478` | Tags, as compiled (macro list): `whatever, update, ask_nodes, size, smm_mass, smm_for_gradu, smm_boundary, smm_uv, smm_res, smm_stress, smm_gradu, smm_density, smmc_facets, smmc_facets_conn, smmc_facets_stress, smmc_damage, giu_global_conn, ce_groups, ce_insertion_order, gm_clusters, diffusion, diffusion_gradient, htm_phi, htm_gradient_phi, pfm_damage, csp_damage, csp_strain, mnl_for_average, mnl_weight, nh_criterion, test, user_1, user_2, constitutive_law_id, clh_init_cl, for_dump, cf_nodal, cf_incr, solver_solution`. | core | — | — | — | The Doxygen-only enum lists `_htm_temperature` / `_htm_gradient_temperature`, which **do not exist** in the compiled list (replaced by `_diffusion`, `_diffusion_gradient`). Use the macro list. |
| MPI-025 | `NodeFlag` bit semantics | `aka_common.hh:489-536`; `src/mesh/mesh_inline_impl.hh:497-560` | `_normal=0x00, _distributed=0x01, _master=0x03, _slave=0x05, _pure_ghost=0x09, _shared_mask=0x0F, _periodic=0x10, _periodic_master=0x30, _periodic_slave=0x50, _periodic_mask=0xF0, _local_master_mask=0xCC` plus debug combos `0x33, 0x35, 0x39, 0x53, 0x55, 0x59`. Bitwise operators. Mesh predicates: `isLocalNode` (shared==0), `isMasterNode`, `isSlaveNode`, `isPureGhostNode`, `isLocalOrMasterNode` (`&0xCC==0`), `isPeriodicSlave/Master`, `getNodeFlag`, `getNodePrank` (map; −1 if absent), `getNodeGlobalId` / `getNodeLocalId` (identity if no global ids), `getNbGlobalNodes`. | core | — | `test_node_synchronizer`, `test_data_distribution` | — | Use a `u8` newtype with the same bit values (they travel in messages and are checked by sanity tests). |

### 1.9 Mesh partitioning, distribution, ghosts, periodicity (PART)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| PART-001 | `Mesh::distribute` | `src/mesh/mesh.hh:140-168`; `src/mesh/mesh.cc:528-579` | `distribute(_communicator=…, _edge_weight_function=…, _vertex_weight_function=…)` (named args; defaults: static communicator, weights 1). On rank 0 with nb_proc > 1 it creates `MeshPartitionScotch(mesh, dim)` and calls `partitionate(nb_proc, edge_w, vertex_w)`; without Scotch → error. `distribute(shared_ptr<MeshPartition>, comm)`: asserts not already distributed, creates `ElementSynchronizer` / `NodeSynchronizer` (registered to mesh events), runs `distributeMeshCentralized` (rank 0 master, others slaves) if nb_proc > 1, **sets `is_distributed = true` even with 1 rank**, recomputes the bounding box (allSum) and sends `MeshIsDistributedEvent`. | parallel (Scotch for automatic partitioning) / core | Scotch | `test_dof_mesh_distribute` (gtest in `test_dof_manager`, package parallel: distributing twice throws); `test_data_distribution`; every parallel test | Centralized distribution: the whole mesh is read on rank 0. | Models / DOF managers must be created **after** `distribute()` (DOF-013 check). |
| PART-002 | `MeshPartition` base (dual graph, ghost partitions) | `src/mesh_utils/mesh_partition.hh:31-124`; `mesh_partition.cc:37-387` | Linearises elements of `spatial_dimension` over types (`linearized_offsets`). `buildDualGraph` (a C++ port of METIS `MeshToDual`): two elements are adjacent if they share ≥ m nodes, m = the smallest facet node count of the element; cohesive–cohesive adjacency is excluded; returns CSR `dxadj`/`dadjncy` with vertex / edge loads from user functions. `fillPartitionInformation(mesh, parts[])`: `partitions(type)`; `ghost_partitions_csr` (element el gets every partition ≠ own that owns an element sharing **at least one node** with el, sorted and unique) plus flat `ghost_partitions` / `ghost_partitions_offset`. Lower-dimensional elements (facets, boundaries): partition = **min** partition of the adjacent higher-dimensional elements (`element_to_subelement`), or 0 if none; ghost partitions = the other adjacent partitions ∪ ghost partitions of the min element. `tweakConnectivity` / `restoreConnectivity` replace periodic slaves by masters during partitioning so that periodic pairs end up adjacent. `hasPartitions`, accessors `getPartition(type, ghost)`, `getGhostPartitionCSR`, `getNbPartition`/`setNbPartition`. | core | — | `test_mesh_partitionate_*` (R1 mesh_utils tests; check R1) | Defines ownership of every element. | Node-sharing (not facet-sharing) defines the ghost layer, which gives a one-element-deep overlap across vertices. `ghost_partitions_offset` for lower-dimensional types accumulates `adjacent_elems.size()` (inconsistent bookkeeping, unused by distribution). |
| PART-003 | Scotch partitioning | `mesh_partition/mesh_partition_scotch.hh:32-62`; `mesh_partition_scotch.cc:51-375` | Asserts `sizeof(Int)==sizeof(SCOTCH_Num)`. Seeds Scotch with `RandomGenerator<Int>::seed()` (`SCOTCH_randomSeed` for v ≥ 6, `srandom` otherwise). `partitionate`: periodic tweak, `buildDualGraph`, `SCOTCH_graphBuild` (baseval 0, vertex and edge loads), `SCOTCH_graphCheck`, `SCOTCH_graphPart(nb_part, default strategy)`, then `fillPartitionInformation`, restore. Debug `dblDump` writes `GraphIniFile.grf`, `GeomIniFile.xyz`, `GraphFile.grf`, `MapFile.map`. PT-Scotch header with `AKANTU_USE_PTSCOTCH`. | Scotch (parallel) | Scotch / PT-Scotch | via all parallel tests | Executed on rank 0 only. | Partitions are **not reproducible** by another partitioner. For differential tests, feed identical partitions via `MeshPartitionMeshData` or dump Scotch's partition from the oracle. |
| PART-004 | Scotch node reordering | `mesh_partition_scotch.cc:378-445` | `reorder()`: build a Scotch mesh (element–node bipartite graph, `createMesh`), `SCOTCH_meshGraph`, `SCOTCH_graphOrder` (default strategy) → `permtab`; renumber the connectivities of all types and ghost types and permute the coordinates. | Scotch | Scotch | none found | serial | Changes node numbering, hence DOF numbering. |
| PART-005 | User-provided partition (`MeshPartitionMeshData`) | `mesh_partition/mesh_partition_mesh_data.hh:38-74`; `.cc:34-127` | Partition from an `ElementTypeMapArray<Idx>` mapping (per element of dimension `dim`, `_not_ghost`), periodic tweak, then `fillPartitionInformation`. Asserts (debug) that the number of distinct partitions equals `nb_part`. `reorder` and `setPartitionMappingFromMeshData` are not implemented. | core | — | `test_mesh_partitionate_mesh_data` (R1) | — | Key tool for deterministic parallel fixtures. |
| PART-006 | Centralized mesh distribution driver | `mesh_utils_distribution.hh:33-41`; `mesh_utils_distribution.cc:35-146` | Root: set `nb_global_nodes`, clear global ids, `synchronizeGroupNames()` (R1), then for each element type of `_not_ghost` with a partition (in `elementTypes` order) a `MasterElementInfoPerProc(...).synchronize()` with an incrementing message count, then a terminator (`_not_defined`), then `MasterNodeInfoPerProc`, `MeshUtils::fillElementToSubElementsData(mesh)`, `setDistributed()`. Slaves mirror this with `SlaveElementInfoPerProc` until the terminator, then `SlaveNodeInfoPerProc`. | core | MPI | `test_data_distribution` (gtest, `cube.geo` → 3-D order 2 mesh `test_synchronizer_communication_mesh`) | Collective. | Element types without a partition (types of other dimensions are partitioned too) are skipped. |
| PART-007 | Element distribution (`ElementInfoPerProc`) | `element_info_per_processor.hh:40-134`; `element_info_per_processor.cc:34-111`; `_tmpl.hh:33-130`; `master_element_info_per_processor.cc:37-443`; `slave_element_info_per_processor.cc:35-184` | Per type: (1) sizes `[type, nb_local, nb_ghost, nb_to_send, nb_tags]` (tag `_sizes`). (2) Connectivities: for proc p, first the elements with partition p in global order, then p's ghost elements in global order (tag `_connectivity`); `MeshUtils::renumberMeshNodes` builds the local node numbering (first-seen order, PART-009) and splits local / ghost connectivity; ghost counters = 1. (3) Partitions (tag `_partitions`): for each local element `[nb_ghost_parts, parts…]`, then for each ghost element the owner. If the type has the mesh dimension, `fillCommunicationScheme` builds the send scheme (local element → procs where it is a ghost) and the recv scheme (ghost element → owner). (4) Mesh data tags (`_mesh_data`): names, type codes and component counts broadcast (sorted names); then per element (local then ghost) values. (5) Element groups (`_element_group`): per element the list of group names (local then ghost); root clears its groups and refills them. | core | MPI | `test_data_distribution`, `test_synchronizer_communication` | Element schemes only for elements of the mesh dimension; facet schemes come from `FacetSynchronizer`. | Local element order = global order within each partition; ghost order = global order. Node local numbering follows first appearance (PART-009). Both affect equation numbering (DOF-003) and must be reproduced for parity of parallel runs. |
| PART-008 | Node distribution (`NodeInfoPerProc`) | `node_info_per_processor.hh:38-113`; `node_info_per_processor.cc:33-830` | Root broadcasts `nb_global_nodes`. **Nodes:** each slave sends its global node list (`_nb_nodes`, `_nodes`); root replies with the coordinates (`_coordinates`). **Types:** each proc computes its flags from its connectivity: nodes only in local elements → `_normal`, only in ghost elements → `_pure_ghost`, in both → `_master` (candidate). Root gathers the flags; for each global node with several "master" candidates, the **lowest rank** stays `_master`, the others become `_slave`, and master send info `(proc, global node)` is recorded; `nodes_pranks[g]` = owner (the normal proc, or the lowest master). Root sends back flags (`_nodes_type` msg 0), pranks (msg 2) and master send lists (msg 1); `fillCommunicationScheme` builds the node send schemes (sorted global ids) and recv schemes (slave nodes per prank, sorted). Pranks are stored only for non-local, non-master nodes. **Groups:** all node groups (global node → names) broadcast as one buffer (`_node_group`) and filtered by the local global ids. **Periodicity:** broadcast flag; root sends `(slave, master)` global pairs per proc (`_periodic_slaves`); procs reply with missing global nodes (`_periodic_nodes`), which are added as pure ghosts with coordinates and pranks, plus recv / send schemes. **Nodal data tags:** broadcast names / types / components, then per-proc values. | core | MPI | `test_data_distribution`, `test_node_synchronizer` | Ownership rule: master = min rank among the ranks owning an element touching the node. | `fillPeriodicPairs` prints `std::cout << "Missing node"` (debug noise). Exact ownership must match for parity. |
| PART-009 | Global → local node renumbering | `src/mesh_utils/mesh_utils.cc:423-494` | `renumberMeshNodes(mesh, conn, nb_local, nb_ghost, type, old_nodes)`: rebuilds `renumbering_map` (global id → local index) from the `old_nodes` already known (nodes of previously distributed types). `renumberNodesInConnectivity` (`:473-494`) then scans the received connectivity (local elements first, then ghost elements) and gives each **not yet seen** global id the next local index (`renumbering_map.size()`), i.e. **first-seen order**. `old_nodes` is rewritten as local → global and the local / ghost connectivities are copied into the mesh. | core | — | `test_data_distribution` | — | Local node numbering = first appearance across types in `elementTypes` order, local before ghost. Reproduce it exactly, because it drives DOF numbering. |
| PART-010 | `GlobalIdsUpdater` | `global_ids_updater.hh:34-88`; `global_ids_updater.cc:32-130`; `global_ids_updater_inline_impl.hh:34-141` | After node creation (cohesive insertion): `updateGlobalIDsLocally(n_new)`: count old and new local-or-master nodes, allreduce, give the new local/master nodes ids `old_global + exscan(new_local) + i`, set `nb_global_nodes`. `synchronizeGlobalIDs()`: `slaveReductionOnce` (ghost → owner: global ids of local-or-master nodes plus prank, per node of each element in the element schemes) then `synchronizeOnce` (owner → ghost: ids of non-pure-ghost nodes); slaves adopt the received id and prank (tag `_giu_global_conn`). Debug: warns about nodes that are pure ghost on every neighbour. | core (used by cohesive_element) | MPI | parallel cohesive tests (R2: `test_cohesive_parallel_*`) | Uses the cohesive element synchronizer (`model.cohesive_synchronizer`). | `getNbData` uses `sizeof(UInt)+sizeof(Int)` per node: OK only if `UInt` and `Int` have the same width. |
| PART-011 | Parallel cohesive insertion hooks | `src/mesh_utils/cohesive_element_inserter.cc:229-235`; `cohesive_element_inserter_helper.cc:76-117`; `cohesive_element_inserter_inline_impl.hh:40-87`; `cohesive_element_inserter_parallel.cc` (includes only) | Before insertion, `insertion_facets` flags are synchronized from facet owners to ghosts (`_ce_groups`, bool per facet). A global insertion order of facets is defined by `exclusiveScan(nb_local_facets_to_insert)` and synchronized to ghost facets (`_ce_insertion_order`, `SimpleElementDataAccessor<Int>`); the facets to double are sorted by it so that node doubling happens in the same order on every rank. New node global ids come from `GlobalIdsUpdater` (`solid_mechanics_model_cohesive.cc:79`). | cohesive_element | MPI | `test_cohesive_parallel_*` (R2) | — | Cross-reference R2 for the insertion algorithm itself. |
| PART-012 | Mesh facets in parallel | `src/mesh/mesh.cc:131-231, 233-262` | `initMeshFacets`: `buildAllFacets`; if distributed, `mesh_facets` gets a `FacetSynchronizer` (MPI-020), the global facet connectivities are synchronized from owners (`_smmc_facets_conn`, `FacetGlobalConnectivityAccessor`), and `MeshUtils::flipFacets(mesh_facets, remote_global_conn, _ghost)` reorders the ghost facets' local connectivity (and the element / subfacet relations) to match the owner's node order. | core | MPI | `test_facet_synchronizer` | — | `flipFacets` (`mesh_utils.cc:537+`) asserts permutation equality. |
| PART-013 | Periodic meshes | `src/mesh/mesh_periodic.cc:43-500`; `mesh.hh:170-200` | `makePeriodic(direction)`: nodes within `1e-10·L` (relative) of the min / max bound in `direction`. `makePeriodic(direction, group1, group2)` uses groups. Core: in distributed meshes, exchange (with procs whose bounding box intersects the opposite side) the local/master nodes of each side (global id, position, periodic flag, master / slave info), adding unknown nodes as pure ghost periodic nodes (`NewNodesEvent`). Both lists are sorted by projected position (lexicographic `Vector <`), then the O(n²) match `distance/L < 1e-10` (`match_found` merges master chains so each slave points to one master). Then `updatePeriodicSynchronizer()` (create or update `PeriodicNodeSynchronizer`; node synchronizer `filterScheme(not periodic slave)`) and `is_periodic = true`. `wipePeriodicInfo`. `makeConsistentForPeriodicity(array, op)`: `_id` → `synchronizeArrayPBC`; `_sum` / `_or` → zero periodic slave or pure-ghost copies, then `reduceSynchronizeArrayPBC<Add/Or>`. `getConnectivityWithPeriodicity`, `getPeriodicSlaves(master)`. | core | MPI | `test_solid_mechanics_model_pbc_parallel` (R2); periodic mesh tests (R1) | Periodic pairs can span ranks. | Tolerance 1e-10 relative is hard-coded. Ties in the position sort follow `std::sort` (not stable): first-match semantics in `match_pairs` (`break` after the first match). |
| PART-014 | Legacy PBC map | `src/mesh_utils/mesh_utils_pbc.cc:30-293` (`MeshUtils::computePBCMap`, `matchPBCPairs`, `CoordinatesComparison`) | Builds `std::map<node, node>` pairs on opposite faces (`Math::are_float_equal` on bounds), matched by sorting with a normalised coordinate comparator. **Not called anywhere** in `src/`, `test/`, `python/`. | core | — | none | serial | Dead code: list it but don't port (a project decision; D-028). |
| PART-015 | `BoostGraphConverter` (debug) | `src/mesh_utils/boost_graph_converter.hh:38-74`; `.cc:31-129` | Builds a Boost adjacency list (elements and facets as vertices, up / down edges) and writes Graphviz (`toGraphviz`). Only used in a commented-out test line. | debug_tools | Boost.Graph | none (commented in `test_cohesive_buildfragments.cc:124`) | — | Optional debug tool. |
| PART-016 | Distributed mesh queries | `mesh.cc:419-462`; `mesh_inline_impl.hh:497-560` | `computeBoundingBox()`: local bbox, `allSum` (sic, a union) if distributed. `getGlobalConnectivity(ElementTypeMapArray)` maps local → global node ids for all types / ghost types present. Node predicates are in MPI-025. | core | MPI | — | Bounding box includes ghost / pure-ghost nodes. | — |
| PART-017 | Global-data update hook after topology changes | `src/mesh/mesh_global_data_updater.hh:27-37`; `src/mesh/mesh.cc:645-658` (`Mesh::updateGlobalData`, `registerGlobalDataUpdater`); user `src/model/solid_mechanics/solid_mechanics_model_cohesive/solid_mechanics_model_cohesive.cc:43-100` (R2) | `MeshGlobalDataUpdater::updateData(NewNodesEvent&, NewElementsEvent&) → (nb_new_nodes, nb_new_elements)` (default: local list sizes). The mesh delegates to the registered updater. The cohesive model's updater copies the node flags of the doubled nodes, updates the cohesive synchronizers, calls `GlobalIdsUpdater::updateGlobalIDs`, allreduces the element count, and sends the events only if the global counts are non-zero. | core (hook) / cohesive_element (implementation) | MPI | parallel cohesive tests (R2) | Ensures every rank sends the mesh events consistently (collective). | Rust: a trait object on the mesh, or an explicit call in the insertion routine. |
| PART-018 | `MeshAccessor` (privileged mesh mutation) | `src/mesh/mesh_accessor.hh` | Friend accessor used by distribution / synchronizers: `get/setNbGlobalNodes`, `setDistributed`, `getNodesGlobalIds`, `getNodesFlags`, `setNodePrank`, `getNodes`, `getConnectivity(type, ghost)`, `getGhostsCounters`, `getElementToSubelement(NC)`, `getSubelementToElement(NC)`, `getNodeSynchronizer`, `getElementSynchronizer`, `wipePeriodicInfo`, `addPeriodicSlave`, `markMeshPeriodic`, `makeReady`, … (R1 owns the rest). | core | — | used by all distribution code and tests (`test_model_solver.cc` `genMesh`) | — | In Rust: crate-private mutators on `Mesh`. |

### 1.10 Diffusion / heat transfer (DIFF)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| DIFF-001 | `DiffusionModel` (generic scalar diffusion) | `src/model/diffusion_model/diffusion_model.hh:42-213`; `diffusion_model.cc:59-80` | `ConstitutiveLawsHandler<DiffusionLaw, Model>` (constitutive-law machinery in R2). Constructor `(mesh, dim, id="diffusion", dof_manager=null, dof_name="diffusion", model_type=_heat_transfer_model)`: `initDOFManager`, registers the element synchronizer for `_diffusion` and `_diffusion_gradient`, the FE engine `<id>:fem` = `FEEngineTemplate<IntegratorGauss, ShapeLagrange>`, a Paraview dumper `<id>`, and dumps the mesh (regular kind). Nodal fields: `<dof_name>`, `<dof_name>_rate`, `external_flow`, `internal_flow`, `blocked_dofs`, `increment` (never allocated). Release counter `diffusion_release` (bumped by predictor / corrector) caches gradients. | diffusion | — | heat patch tests (DIFF-012); examples `examples/c++/diffusion_model/heat_diffusion_{static_2d,dynamics_2d,dynamics_3d}.cc` (R4) | See DIFF-004. | Rust: a generic diffusion model parameterised by DOF name. |
| DIFF-002 | `HeatTransferModel` | `heat_transfer_model.hh:29-88` | `DiffusionModel` with id `heat_transfer` and DOF `temperature`. Accessor aliases `getTemperature`, `getTemperatureRate`, `getTemperatureRelease`, `getExternalHeatRate`, `getInternalHeatRate`. Dump aliases `internal_heat_rate→internal_flow`, `external_heat_rate→external_flow`, `conductivity→diffusivity`. | diffusion | — | heat patch tests | — | — |
| DIFF-003 | Diffusion solver configuration | `diffusion_model.cc:126-207` | `initSolver`: allocate the fields, register DOF `<dof_name>` (nodal) and blocked DOFs; for `_dynamic`/`_dynamic_lumped`, allocate and register the rate as derivative 1. Defaults: `_explicit_lumped_mass` → solver `explicit_lumped` (`_dynamic_lumped`, NLS `_lumped`, `_forward_euler`, `_temperature_rate`); `_static` → `static` (NR, `_pseudo_time`); `_implicit_dynamic` → `implicit` (`_dynamic`, NR, `_backward_euler`, `_temperature`); `_explicit_consistent_mass` (only through `_dynamic`) → NR, `_forward_euler`, `_temperature_rate`; others → `("unknown", _not_defined)`. Matrix types: K, M symmetric. | diffusion (+implicit for NR) | — | `patch_test_linear_heat_transfer_{explicit,static,implicit}` | — | `getDefaultSolverID` maps no solver for `_explicit_consistent_mass` (returns "unknown"); the branch in `getDefaultSolverOptions` is reachable only with an explicit `_dynamic` TSS while `method == _explicit_consistent_mass`. |
| DIFF-004 | Diffusion residual | `diffusion_model.cc:113-118, 221-235`; `diffusion_law.cc:19-72` | `assembleResidual`: `assembleInternalFlow()` then `r += external_flow + internal_flow`. Internal flow: zero; `synchronize(_diffusion)` (ghost elements' nodal u); for each ghost type and law: `computeGradU` (∇u at quadrature points, cached by `diffusion_release`), then `computeDiffusivityGradUOnQuadPoints` (law), then `assembleInternalFlow`: `internal_flow -= ∫ Bᵀ(D∇u)` (`computeBtD`, integrate, scatter with scale −1). | diffusion | — | heat patch tests | Ghost-element contributions make `internal_flow` complete on local / master / slave nodes. | **Bug:** an empty filter for one type does `return` instead of `continue` (`diffusion_law.cc:51-53`), skipping the remaining types. |
| DIFF-005 | Diffusivity (conductivity) matrix | `diffusion_model.cc:210-218`; `diffusion_law.cc:75-106` | `K` zeroed; per law and type (`_not_ghost`): `K_e = ∫ Bᵀ D B` (`computeBtDB` order 2), assembled symmetric. | diffusion | — | heat static / implicit patch tests | Not-ghost only (partial sums, §3.6). | Same `return`-instead-of-`continue` bug (`:82-84`). |
| DIFF-006 | Capacity matrices | `diffusion_model.cc:30-56, 97-110, 277-328` | Consistent `M = ∫ ρc NᵀN` (`assembleFieldMatrix` with `ComputeRhoFunctor` = law `getRho()` = `density*capacity`), not-ghost only, only while `need_to_reassemble_capacity`. Lumped: `assembleFieldLumped(rho)` over both ghost types into a nodal array, `makeConsistentForPeriodicity`, `assembleToLumpedMatrix("M")`. | diffusion | — | heat explicit patch test | Lumped capacity includes ghost elements, so it is complete on shared nodes. | **Bug:** the lumped path checks `need_to_reassemble_capacity` (not the `_lumped` flag), so the lumped capacity is rebuilt at every residual assembly. Harmless but slow. `assembleFieldLumped` / `assembleFieldMatrix` are in the FE engine (R1); the lumping scheme there (row sum vs HRZ) must be checked. |
| DIFF-007 | Stable time step | `diffusion_model.cc:238-267`; `heat_diffusion.cc:75-87` | `h_min` = min element **inradius** over not-ghost elements of the mesh dimension (`FEEngine::getElementInradius`); `Δt = min over laws of 2·h²/4·ρc/k_max`, k_max = max eigenvalue of the conductivity matrix; then `allReduce(_min)`. | diffusion | — | heat explicit patch test | Collective. | `getStableTimeStep` of the base law returns 0. |
| DIFF-008 | Diffusion laws (base) | `diffusion_law.hh:17-70`; `diffusion_law.cc:7-165` | `DiffusionLaw : ConstitutiveLaw<DiffusionModel>` with internals `∇u` (dim), `D∇u` (dim), `diffusivity` (dim²). `DiffusionFactory` keyed by name; `instantiateDiffusionLaw<Law, Model>("name")` checks the model type and dispatches on dimension. Data accessor for `_diffusion_gradient` (packs `∇u` per quadrature point). | diffusion | — | — | `_diffusion_gradient` tag registered but no caller found in the model (only the law packs it). | Unicode internal names `∇u` and `D∇u` appear in input / dump names. Keep the exact UTF-8. |
| DIFF-009 | `HeatDiffusion` law | `heat_diffusion.hh:8-55`; `heat_diffusion.cc:7-177` | Name `heat_diffusion`, only with `HeatTransferModel`. Parameters: `density`, `capacity`, `conductivity` (matrix; the top-left `dim×dim` block is kept), `conductivity_variation` (0), `temperature_reference` (0). Diffusivity at quadrature points: `C = conductivity + conductivity_variation·(T − T_ref)` added to **every entry** (diagonal and off-diagonal), recomputed when the temperature release changes. `D∇u = C·∇u`. `getRho() = density·capacity`. Energy `"thermal"`: `∫ ρ c T` per element or total (not-ghost). | diffusion | — | heat patch tests (`data/heat_transfer_input.dat`: ρ=8940, c=385, k=diag(401)) | Energy summed with `allReduce` in the model. | `getThermalEnergy()` integrates over the whole type without the element filter (`heat_diffusion.cc:145`): wrong with several laws. `T`-dependence broadcasts `conductivity_variation` onto off-diagonal terms: probably unintended; reproduce. |
| DIFF-010 | Diffusion data accessors | `diffusion_model.cc:455-560` | Nodes (`Idx`): `_diffusion` packs `u[node]` (other tags → `AKANTU_ERROR`). Elements: `_diffusion` packs nodal `u` per element node, plus the law-handler data. | diffusion | — | — | `_diffusion` element sync before every internal-flow assembly. | — |
| DIFF-012 | Heat-transfer patch tests (the only upstream tests of this package) | `test/test_model/patch_tests/patch_test_linear_heat_transfer_fixture.hh`; `patch_test_linear_heat_transfer_{explicit,static,implicit}.cc`; Python `test_patch_linear_heat_transfer_{explicit,static,implicit}.py` + `patch_test_linear_heat_transfer_fixture.py`; `data/heat_transfer_input.dat`; `test/test_model/patch_tests/CMakeLists.txt:38-104` | Linear temperature patch test, typed over every element type (`TestElementTypes`): boundary DOFs imposed from a linear field; explicit / implicit use `Δt = 0.5·getStableTimeStep()` and several steps; checks T (DOFs), `∇u` and `D∇u = C·∇T` at quadrature points against the exact linear solution (tolerances in `patch_test_linear_fixture.hh`, R2/R4). | diffusion (+implicit for static / implicit) | — | — | — | **These tests are never activated upstream:** they are registered with `PACKAGE heat_transfer`, which does not exist (the package is `diffusion`), so `package_is_activated` is false (`cmake/AkantuTestsMacros.cmake:244-251`). `test/test_model/CMakeLists.txt:24` also adds a non-existent `test_heat_transfer_model` directory. The Rust port must port them anyway, with `diffusion`. |
| DIFF-011 | Diffusion energies and dumps | `diffusion_model.cc:331-452` | `getEnergy(id)` sums the laws with allreduce; `getEnergy(id, element)`. `computeThermalEnergyByNode()` (`Σ internal_flow·Δt` over local / master nodes, allreduce) is private and unused. Dump fields: nodal `<dof>`, `<dof>_rate`, `external_flow`, `internal_flow`, `increment`, `blocked_dofs`; `capacity_lumped` → exception; elemental `partitions`, any law internal (homogenised). | diffusion | IOHelper (R1) | — | — | — |

### 1.11 Phase field (PF)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| PF-001 | `PhaseFieldModel` | `src/model/phase_field/phase_field_model.hh:42-249`; `phase_field_model.cc:45-129, 252-313` | `ConstitutiveLawsHandler<PhaseField, Model>` plus `BoundaryCondition<PhaseFieldModel>`. Constructor `(mesh, dim, id="phase_field_model", dof_manager, model_type)`: `initDOFManager`, FE engine `PhaseFieldFEEngine`, dumper `phase_field`, element synchronizer for `_pfm_damage` and `_for_dump`, parser type `_phasefield`. `initFullImpl` also does `initBC(*this, "damage", damage, external_force)`. `initSolver`: fields `damage`, `external_force`, `internal_force`, `blocked_dofs`, `previous_damage`; registers DOF `damage` (nodal), blocked, previous; `_dynamic` → `TO_IMPLEMENT`. K symmetric. `assembleLumpedMatrix` no-op; predictor / corrector no-ops. | phase_field | — | `test_phasefield_selector`, `test_phase_solid_coupling`, `test_phase_field_anisotropic`, `test_phase_field_deviatoric_split`, `test_phase_field_optimal`, `test_phase_solid_explicit`, `test_penalization`, `test_multi_material` (UNSTABLE); PETSc: `test_phase_field_optimal_linear`, `test_phase_field_linear`, `test_phase_field_tao`. Fixtures in `test/test_model/test_phase_field_model/`: `material_{coupling,deviatoric,hybrid,linear,linear_tao,multiple,optimal,optimal_linear,penalization}.dat`, `phasefield_selector.{dat,msh}`, `test_{one_element,two_element,four_elements}.{geo,msh}`, `test_optimal_damage.msh`. | — | — |
| PF-002 | Phase-field solver configuration | `phase_field_model.cc:133-191` | Default TSS type `_static`. IDs: `_explicit_lumped_mass` → `explicit_lumped`; `_explicit_consistent_mass` → `explicit`; `_static` → `static`; `_implicit_dynamic` → `implicit`. Options: `_dynamic_lumped` → `_lumped`, `_central_difference`, `_acceleration` (would fail: no lumped matrix); `_static` → NR, `_pseudo_time`; `_dynamic` → NR, `_backward_euler`, `_damage`. | phase_field | — | all PF tests use `_static` | — | Only the static path is functional. |
| PF-003 | Phase-field residual | `phase_field_model.cc:267-297`; `phasefield.cc:98-167` | `assembleResidual`: internal + external into the residual. `assembleInternalForces`: zero; each law `computeAllDrivingForces(_not_ghost)` (interpolate the nodal damage to quadrature points **without** the element filter, ∇d with the filter, then `computeDrivingForce`); `asynchronousSynchronize(_pfm_damage)` (driving_force, driving_energy, damage_on_qpoints to ghosts); assemble not-ghost; wait; assemble ghost. Per law and type: `f_int -= ∫ Nᵀ·driving_force` and `f_int -= ∫ Bᵀ·driving_energy`. | phase_field | — | PF tests | async ghost sync overlapping the assembly. | **Bug:** `computeAllDrivingForces` writes ∇d of *ghost* elements into the `_not_ghost` gradient array (`gradd(type, _not_ghost)`, `phasefield.cc:114`). Ghost driving energies then use not-ghost-sized data. Reproduce or fix after oracle comparison (Q6). |
| PF-004 | Phase-field stiffness | `phase_field_model.cc:252-264`; `phasefield.cc:170-227` | `K` zeroed (created if needed); per law, not-ghost: `K += ∫ Nᵀ (damage_energy_density) N` (`computeNtbN`) + `∫ Bᵀ (damage_energy) B` (`computeBtDB`), both symmetric. | phase_field | — | PF tests | Not-ghost only. | Empty filter → `return` (skips the remaining types). |
| PF-005 | `PhaseField` base law | `phasefield.hh:44-229`; `phasefield.cc:34-308`; `phasefield_inline_impl.hh:41-71` | Internals: `g_c` (random field, parameter `gc`), `damage` (with history), `grad_d`, `phi` (with history), `strain` (dim²), `driving_force`, `driving_energy` (dim), `damage_energy` (dim²), `damage_energy_density`, `dissipated_energy`. Parameters: `l0` (0), `gc`, `E`, `nu`, `irreversibility_type` (`history` default, `penalization`, `tao`), `Plane_Stress` (false). Derived `λ = νE/((1+ν)(1−2ν))` (plane stress `νE/((1+ν)(1−ν))`), `μ = E/(2(1+ν))`. `beforeSolveStep` computes the driving forces for both ghost types. Data accessor `_pfm_damage`: `(dim+2)` reals per quadrature point. `PhaseFieldFactory`, `instantiatePhaseField<PF>(name)` with an energy-split argument `""`/`"no_split"` → `NoEnergySplit`, `"volumetric_deviatoric"` → `VolumetricDeviatoricSplit`. Energy `"dissipated"` (integrated, not-ghost). | phase_field | — | PF tests | — | `finite_deformation` is declared but unused. |
| PF-006 | `PhaseFieldLinear` (AT1) | `phasefields/phasefield_linear.hh:11-79`; `.cc:8-220`; `_inline_impl.hh` | Registered `"linear"`. Parameters `irreversibility_tol` (1e-2), `recovery_tol` (1e-2). Init: `γ = g_c/l0·27/(64·tol_ir²)`; `ρ_rec = g_c/l0·9·(diam/l0 − 2)/(64·tol_rec)` with diam = bounding-box diagonal (dim components); creates the energy split. Initial internals: `damage_energy = ¾ g_c l0 I`, `previous_phi = 3g_c/(16 l0)`. Driving force: `φ = split.φ(ε)` (history: `φ = max(φ, φ_prev)`); `damage_energy_density = 2φ`; `driving_force = −2φ + 3g_c/(8l0) + d·2φ`; `driving_energy = damage_energy·∇d`; penalization (if enabled): `+= γ·min(0, d − d_prev) + ρ_rec·min(0, d)`, density `+= γ[d<d_prev] + ρ_rec[d<0]`. Dissipated energy `3/8·g_c l0 ‖∇d‖² + 3 g_c d/(8 l0)`. Explicit instantiation for dims 1–3 × both splits. | phase_field | — | `test_phase_field_linear`, `test_phase_field_optimal_linear`, `test_phase_field_tao` (PETSc, TAO with bounds) | — | **Bug:** in `computeDissipatedEnergyByElement`, `g_c_it` is not advanced in the loop (`.cc:186-189`). The doc-only `computeDrivingForceOnQuad` is unused. |
| PF-007 | `PhaseFieldQuadratic` (AT2) | `phasefields/phasefield_quadratic.hh:43-103`; `.cc:41-222`; `_inline_impl.hh` | Registered `"quadratic"`. Parameter `irreversibility_tol` (1e-2). `γ = g_c/l0·(1/tol_ir² − 1)`. Initial `damage_energy = g_c l0 I`, `previous_phi = 0`. Driving: `φ` (with history max), `damage_energy_density = 2φ + g_c/l0`, `driving_force = −2φ + d·(2φ + g_c/l0)`, `driving_energy = g_c l0 ∇d`; penalization `+= γ·min(0, d−d_prev)`, density `+= γ[d<d_prev]`. Dissipated `½ g_c l0 ‖∇d‖² + g_c d²/(2 l0)`. | phase_field | — | `test_phase_field_optimal`, `test_phase_solid_coupling`, `test_phase_solid_explicit`, `test_penalization`, `test_phase_field_anisotropic`, `test_phase_field_deviatoric_split`, `test_multi_material`, `test_phasefield_selector` (check the `.dat` per test) | — | Same `g_c_it` bug. Templates are instantiated implicitly (no explicit `template class`). |
| PF-008 | Energy-split base | `energy_split.hh:13-87`; `energy_split.cc:9-18` | Holds E, ν, plane_stress, λ, μ (same formulas as PF-005). Concepts `CanComputePhi`, `CanComputeSigma`, `CanComputeTangentCoefs`, `CanComputeSigmaAndTangent` (used by `MaterialPhaseField`, R2). | phase_field | — | — | — | — |
| PF-009 | `NoEnergySplit` | `energy_splits/no_energy_split.hh`, `_inline_impl.hh:9-66` | `φ = ½λ(tr ε)² + μ ε:ε`. σ⁺ = `2μ(ε − ε_th I) + λ tr(ε − ε_th I) I` (σ⁻ untouched). Tangent (Voigt `n = dim(dim+1)/2`): 1-D `E·g`; otherwise `M_iiii = g(λ+2μ)`, `M_iijj = gλ`, `M_ijij = gμ`. | phase_field | — | `test_phase_field_*` | — | — |
| PF-010 | `VolumetricDeviatoricSplit` | `energy_splits/volumetric_deviatoric_split.hh`, `_inline_impl.hh:9-85` | κ = λ + 2μ/3; `φ = ½κ⟨tr ε⟩₊² + μ ε_dev:ε_dev` with `ε_dev = ε − tr ε/3·I` (**/3 even in 2-D**). σ⁺ = `κ⟨tr⟩₊ I + 2μ ε_dev`, σ⁻ = `κ⟨tr⟩₋ I` (with ε_th). Tangent: `g_hyd = g` if `tr ε > 0` else 1; `M_iiii = g_hyd κ + g·2μ·(2/3)`, `M_iijj = g_hyd κ − g·2μ/3`, `M_ijij = gμ`; 1-D `E·g_hyd`. | phase_field | — | `test_phase_field_deviatoric_split` (`material_deviatoric.dat`) | — | 2-D uses the 3-D deviator definition; reproduce. |
| PF-011 | Phase-field selectors and element filter | `phasefield_selector.hh:39-46`; `phasefield_selector_tmpl.hh` (empty); `phase_field_element_filter.hh:7-47` | Aliases of the constitutive-law selectors (R2): `PhaseFieldSelector`, `DefaultPhaseFieldSelector`, `ElementDataPhaseFieldSelector<T>`, `MeshDataPhaseFieldSelector<T>`. `PhaseFieldElementFilter(model, max_damage=1)`: a `GroupManager::ClusteringFilter` keeping elements with at least one quadrature point with `damage < max_damage` (used for fragment clustering). | phase_field | — | `test_phasefield_selector` (`phasefield_selector.{dat,msh}`) | — | Uses FE engine id `"PhaseFieldFEEngine"`. |
| PF-012 | Phase-field energies / data accessors / dumps | `phase_field_model.cc:194-233, 316-474` | `getEnergy(id="dissipated")` with allreduce; per element; per element group (allreduce). Element sync `_for_dump` packs nodal damage; node sync `_for_dump` / `_pfm_damage` pack the damage value (other tags → error). Dump fields: `damage`, `external_force`, `internal_force` (nodal), `blocked_dofs`, elemental `partitions`. | phase_field | — | — | — | Unknown nodal field names return `createNodalField(nullptr)` (no null check). |

### 1.12 Contact mechanics (CONT)

All contact sources are in package `contact_mechanics` (default ON, depends on
`model_couplers` and `cohesive_element`), `packages/contact_mechanics.cmake:21-51`.

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| CONT-001 | `ContactMechanicsModel` | `src/model/contact_mechanics/contact_mechanics_model.hh:43-349`; `contact_mechanics_model.cc:37-232, 503-569` | `Model` plus `DataAccessor<Element/Idx>` plus `BoundaryCondition`. Constructor `(mesh, dim, id="contact_mechanics_model", dof_manager, type)`: `initDOFManager`, FE engines `ContactMechanicsModel` (dim) and `ContactFacetsFEEngine` (dim−1), dumper `contact_mechanics`, `ContactDetector(mesh, id+":contact_detector")`. `initFullImpl`: if an input file was parsed, `instantiateResolutions()` (sub-sections `contact_resolution <type> [name=…]` of the model section, then global ones; exception if none) and `initBC(displacement, increment, external_force)`. `initSolver` allocates nodal fields: `displacement`, `displacement_increment`, `internal_force` (= contact force), `external_force`, `normal_force`, `tangential_force` (dim); `gaps`, `areas`, `blocked_dofs`, `contact_state`, `previous_master_elements` (1); `normals` (dim); `tangents`, `previous_tangents` ((dim−1)·dim); `projections`, `previous_projections`, `tangential_tractions`, `previous_tangential_tractions` (dim−1). **It registers no DOFs**: it is always used through a coupler sharing the solid's DOF manager. Default solver IDs / options: `_explicit_lumped_mass` → `explicit_lumped` (`_lumped`, central difference, acceleration); `_explicit_consistent_mass` → `explicit` (`_dynamic`, also `_lumped` + central difference); `_static` → `static` (`_newton_raphson_contact`, pseudo-time); `_implicit_dynamic` → `implicit` (`_dynamic` → lumped + CD!). `assembleLumpedMatrix` → `TO_IMPLEMENT`. K symmetric. | contact_mechanics | — | `test_explicit_dynamic` (`flat_on_flat.geo` → 2-D order-1 mesh, `material.dat`); `test_explicit_friction` (`sliding-block-2D.geo`, `material-friction.dat`) — both in `test/test_model/test_contact_mechanics_model/`. | **Not parallel**: element / node data accessors pack nothing (`getNbData` returns 0, `contact_mechanics_model.cc:628-667`), assembly only `_not_ghost` (forces are assigned, not reduced), no ghost search. | `_implicit_contact` / `_explicit_contact` analysis methods are not handled ("unknown" solver). |
| CONT-002 | Contact search pipeline (`search`) | `contact_mechanics_model.cc:336-394` | `savePreviousState()` (copy projections, tangents, tangential tractions to `previous_*`; `previous_master_elements[slave] = master` for the current contact elements, others `ElementNull`), clear contact elements, resize and zero `gaps`, `normals`, `tangents`, `projections`, `tangential_tractions`, `contact_state`, `nodal_area`, `external_force`; `detector->search(...)`; **gap sign flipped** (`gap *= -1`, so a positive gap means penetration); if any contact elements exist, `computeNodalAreas()`. | contact_mechanics | — | same | serial | Called by the couplers (CPL-002) at each residual (explicit) or each corrector (static / implicit). |
| CONT-003 | Nodal areas | `contact_mechanics_model.cc:397-500` | Re-initialises the boundary FE engine shape functions on the **current positions** (detector positions), computes normals at quadrature points, and for each `(dim−1)` element of the hard-coded group **`contact_surface`** integrates the outward unit normal (sign fixed by `GeometryUtils::outsideDirection`), `∫Nᵀ n`, scattered into a temporary nodal vector. Nodal area = Euclidean norm of the resulting vector. | contact_mechanics | — | same | serial | The "area" is the norm of the integrated normal, not the integrated weight (it differs at corners). |
| CONT-004 | Contact force assembly | `contact_mechanics_model.cc:290-334`; `resolution.cc:65-127` | `assembleResidual`: `assembleInternalForces()` then `r += internal_force`. `assembleInternalForces`: clear and resize `internal_force`, `normal_force`, `tangential_force`; each resolution `assembleInternalForces(_not_ghost)`. Per contact element: local normal and tangential forces (size `(nb_master_nodes+1)·dim`) are **assigned** (`=`, not `+=`) to the global nodal arrays. Without `is_master_deformable` only the slave node block, with factor 1; with deformable masters all nodes with factor ½ (`resolution.cc:117-126`). Then `internal_force = normal_force + tangential_force` for all nodes. | contact_mechanics | — | same | serial | **Assignment semantics**: a master node shared by several contact elements keeps only the **last** contribution. The ½ factor for deformable masters is a heuristic. Reproduce for parity, flag for review. |
| CONT-005 | Contact stiffness assembly | `contact_mechanics_model.cc:540-550`; `resolution.cc:130-191` | Creates `K` if needed (symmetric), then for each resolution and contact element computes the normal and tangential moduli (`(n+1)dim` square) and adds them **directly to K** with `global.add(row, col, v)`, where `row = node·dim + d`. These are **mesh node numbers, not DOF-manager equation numbers**. Only the slave block (`dim×dim`) if the master is rigid. | contact_mechanics | — | `test_coupled_stiffness` (unregistered) | serial | Correct only when the global equation number of `(node, d)` equals `node·dim+d`, i.e. serial, a single DOF id `displacement` registered first, no periodicity. The Rust port must map through the DOF manager. It also adds the **full** local matrix into a **symmetric** AIJ matrix through `add(i, j, v)`, whose key is canonicalised to `(min,max)` (`sparse_matrix_aij_inline_impl.hh:30-84`). Both `(i,j)` and `(j,i)` therefore land in the same stored entry, so the effective matrix has off-diagonal `K_ij + K_ji`: symmetric parts of the contact moduli are **doubled** off the diagonal, and unsymmetric (friction) parts are symmetrised. This is invisible when n is axis-aligned (n⊗n diagonal) with a rigid master, which is the only tested configuration. Bug (Q18). |
| CONT-006 | `ContactDetector` | `contact_detector.hh:45-215`; `contact_detector.cc:28-266` | Parsable (`ParserType::_contact_detector`). The constructor copies the positions (default: mesh nodes) and parses the **first** `contact_detector [...]` section (mandatory): `type` (`explicit` / `implicit` → `DetectionType`), `projection_tolerance`, `max_iterations`, `extension_tolerance` (all required). `search()`: `fillNodesToElements(dim−1)`, `computeMaximalDetectionDistance()`, global search, local search, `createContactElements`. The `SurfaceSelector` must be set by the user (`setSurfaceSelector`); there is no default. | contact_mechanics | — | `test_detection` (gtest, `test_detector/`, **not added** to the CMake tree) | serial | `detection_type` only sets an unused `alpha` (passed to `orthogonalProjection` but ignored). |
| CONT-007 | Global search (bounding boxes + grid) | `contact_detector.cc:91-121`; `contact_detector_inline_impl.hh:50-118` | `max_dd = max_bb =` max inradius and `min_dd =` min inradius over all elements attached to master nodes (current positions). The master and slave bounding boxes are each enlarged by `max_bb` on every side and intersected. A `SpatialGrid<Idx>` centred on the intersection with spacing `√2·max_dd` in every direction holds the master nodes inside the intersection. | contact_mechanics | — | — | serial | `SpatialGrid` (R1 `aka_grid_dynamic.hh`) cell numbering must be reproduced for identical candidate ordering. |
| CONT-008 | Local search (closest master node) | `contact_detector.cc:140-216` | For each slave node inside the intersection: over the grid cell and its neighbours, find the master node with minimum Euclidean distance (`<=`, so the **last** equal-distance candidate wins). Masters belonging to a regular element that contains the slave node are skipped. The pair (slave, master) is stored. | contact_mechanics | — | — | serial | Tie-breaking depends on the grid's cell iteration and node insertion order. |
| CONT-009 | Contact element creation and projection | `contact_detector.cc:219-266`; `geometry_utils_inline_impl.hh:253-317` | For each pair: `GeometryUtils::orthogonalProjection(slave, elements_of(master_node))` over master facets that belong to the `contact_surface` group. Natural projection by Newton (CONT-010), unit covariant tangents, outward normal; accept if `(slave − master_point)·n < 0` (slave behind the surface, i.e. penetrating), `‖slave − master_point‖ ≤` the current minimum and the projection is inside `[-1−ext_tol, 1+ext_tol]` for every ξ. Then `gap = −distance`, ξ, normal and tangents are stored. Rejected pairs or invalid self-contact (`isValidSelfContact`, CONT-011) get zeros; accepted pairs add `ContactElement{slave, master_element}`. | contact_mechanics | — | — | serial | Only penetrating pairs produce contact elements: there is **no** search margin for "near" contact. |
| CONT-010 | Natural projection (Newton) | `geometry_utils_inline_impl.hh:340-450` | Unknown ξ ∈ ℝ^{dim−1}, start ξ=0. `x(ξ) = X·N(ξ)`, `f_α = −2 ∂_αx·(s − x)`, `‖f‖` as error. `J_αβ = 2(a_αβ − ∂²_αβx·(s − x))` with `a = (∂x)(∂x)ᵀ` (covariant metric of the unnormalised gradient), `ξ += −J⁻¹ f`. Stops when `‖f‖ ≤ projection_tolerance` or after `max_iterations`. Returns `(ξ, x(ξ))`. | contact_mechanics | — | — | — | Uses `ElementClassHelper::getN/getDNDS/getD2NDS2` (R1). No convergence failure reporting. |
| CONT-011 | Self-contact validity | `contact_detector_inline_impl.hh:234-287` | Reject if the master node belongs to any regular element of the slave node, or `abs(gap) > 2·min_dd`, or `n̄_slave·n_master ≥ 0` (n̄_slave = normalised sum of the outward normals of the slave's regular elements). | contact_mechanics | — | — | — | **UB:** `slave_normal` is an uninitialised `Vector<Real>` accumulated with `+` (`:250-263`). Akantu's Eigen types are not zero-initialised (no `EIGEN_INITIALIZE_MATRICES_BY_ZERO` anywhere in the tree). The result is non-deterministic in principle. Port with zero-init and document the deviation. |
| CONT-012 | Geometry utilities | `geometry_utils.hh:32-142`; `geometry_utils_inline_impl.hh:32-503`; `geometry_utils.cc` (empty) | `isBoundaryElement` (facet with one regular neighbour, or with fewer regular than total neighbours); `isValidProjection(ξ, tol)`; `outsideDirection(element)` = facet barycenter − barycenter of the first adjacent element (parent mesh if mesh facets); `normal(coords, element, outward)` (2-D `Math::normal(x1−x0)`, 3-D `normal(x1−x0, x2−x0)`, flipped outward); `normal(element, tangents, outward)` (2-D `(−t_y, t_x)`, 3-D `t1×t2` normalised; flips tangents too); `covariantBasis` (**unit** tangents `X·∂N/∂ξ`, normalised column-wise; the variant with a normal flips `t2` to match orientation); `curvature` (`X·∂²N`); `contravariantBasis`; `covariantMetricTensor(T) = TᵀT`; `contravariantMetricTensor = (TᵀT)⁻¹`; `covariantCurvatureTensor H_αβ = ∂²_αβx·n`; `realProjection` (plane projection or `X·N(ξ)`). | contact_mechanics | — | — | — | Unit-normalised "covariant" tangents mean the metric tensors are nearly identity on straight facets. Keep this, because the friction law depends on it. |
| CONT-013 | Surface selectors | `surface_selector.hh:46-133`; `surface_selector.cc:30-202` | `SurfaceSelector` (abstract; `MeshEventHandler` + Parsable). `PhysicalSurfaceSelector(mesh)`: reads `master` / `slave` group names from the first `contact_detector` section, creates the element group `contact_surface` (dim−1) = master ∪ slave; master / slave lists = node groups of those groups. `CohesiveSurfaceSelector(mesh)` (cohesive builds): an empty `contact_surface` group on `mesh_facets`; on `onElementsAdded` the facets of new cohesive elements are added; on cohesive `onNodesAdded` → `mesh_facets.fillNodesToElements(dim−1)`; master = slave = the group's nodes. `AllSurfaceSelector(mesh)`: `contact_surface` on mesh facets = master ∪ slave groups (from the parser) plus facets of new cohesive elements; master = slave = all its nodes. | contact_mechanics (+cohesive_element for the last two) | — | `test_selection` (gtest in `test_selector/`, meshes `data/selection_{1D,2D,3D}.geo`, `material.dat`; registered sources only — no `register_gtest_test`, so not run) | serial | The `new_nodes_list` members are never filled. |
| CONT-014 | `Resolution` base | `resolution.hh:57-169`; `resolution.cc:30-197` | Parsable (`ParserType::_contact_resolution`). Parameters `name` (string), `mu` (friction coefficient, 0, modifiable), `is_master_deformable` (false). Hooks: `computeNormalForce`, `computeTangentialForce`, `computeNormalModuli`, `computeTangentialModuli`, `beforeSolveStep`, `afterSolveStep`. Assembly helpers as in CONT-004 / CONT-005. `ResolutionFactory` (name → allocator per dim) through `INSTANTIATE_RESOLUTION(id, Class)`. Helpers `macaulay(x)=max(x,0)`, `heaviside(x) = x<0 ? 0 : 1`. | contact_mechanics | — | — | — | `heaviside(0) = 1`. |
| CONT-015 | Penalty (linear) resolution `penalty_linear` | `resolutions/resolution_penalty.hh:30-124`; `resolution_penalty.cc:29-602` | Parameters `epsilon_n`, `epsilon_t` (0, modifiable). Normal traction `p_N = ε_n⟨g⟩`. Normal force `f_N = p_N·A_slave·Aᵀ n` with `A` the `dim × (n+1)dim` shape matrix `[I, −N₁I, …]` at ξ. Friction (if μ≠0 and the slave had a previous master): trial traction transported from the previous tangents, minus `ε_t·a·Δξ`; stick if `‖t_trial‖ ≤ μ p_N` (metric norm with `a⁻¹`), else slip `t = μ p_N t_trial/‖t_trial‖`; the contact state is recorded; force `f_T = Σ_αβ t_β a^{αβ} A_slave Aᵀ τ_α`. Moduli: normal `k = ε_n H(g) A_s Aᵀ(n⊗n)A + rotational terms` (with `−ε_n H(g) g A_s`); stick / slip moduli in §2.7. | contact_mechanics | — | `test_explicit_dynamic` (μ=0, ε_n=1050e7, rigid master), `test_explicit_friction` (μ=0.3, ε_n=ε_t=4200e7, `extension_tolerance=1e-3`) | serial | **UB / bugs:** `computeTrialTangentialTraction` accumulates into the caller's uninitialised `traction_trial` (`:141-142, 222`) and an uninitialised `increment_xi` (`:242-252`); `k_third` in `computeSlipModuli` is never zeroed (`k_first.zero()` twice, `:535-538`). Friction results are therefore implementation-dependent in the oracle. Port with zero-init, validate on μ=0 first, and flag friction parity as "best effort" (Q8). |
| CONT-016 | Penalty quadratic resolution `penalty_quadratic` | `resolutions/resolution_penalty_quadratic.hh:31-51`; `.cc:29-114` | Inherits the friction from linear. `p_N = ε_n(⟨g⟩² + ⟨g⟩)`. Normal moduli `k_main` uses the factor `(2g+1)`; rotational terms use `(g²+g)`; **both** `k_rot1` and `k_rot2` use `n⊗t` (the linear version uses `t⊗n` for `k_rot2`). | contact_mechanics | — | example `cohesive_contact_explicit_dynamic.cc` (R4); no test | serial | The `k_rot2` asymmetry versus linear looks like a bug (Q9). |
| CONT-017 | `ContactElement` and `ContactState` | `contact_element.hh:37-72`; `aka_common.hh:318-336` | `ContactElement{slave: Idx, master: Element}`, `getNbNodes() = nb_nodes(master)+1`, equality. `ContactState {_no_contact=0, _stick=1, _slip=2}` with stream I/O. `DetectionType {_explicit, _implicit}`. | contact_mechanics | — | — | — | — |
| CONT-018 | Contact dumps and accessors | `contact_mechanics_model.cc:572-625`; `.hh:200-275` | Dump fields: nodal real `contact_force` (= internal_force), `normal_force`, `tangential_force`, `normals`, `tangents`, `gaps`, `areas`, `tangential_traction`; int `contact_state` (enum → int); bool `blocked_dofs`. Accessors for every array, `getContactElements`, `getPositions`, `getContactDetector`, a deprecated `getForce`. | contact_mechanics | IOHelper | — | — | An unknown field name dereferences a missing map entry (null → returns a field built on nullptr). |

### 1.13 Structural mechanics (STRUCT)

Package `structural_mechanics` (default **OFF**, depends on `implicit`). Its
shape functions / element classes (`fe_engine/element_class_structural.hh`,
`element_class_{bernoulli_beam,kirchhoff_shell,hermite}_inline_impl.hh`,
`shape_structural*`, `fe_engine_template_tmpl_struct.hh`) and the structural
Gmsh reader (`io/mesh_io/mesh_io_msh_struct.*`) belong to R1, but are listed in
`packages/structural_mechanics.cmake:25-43`.

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| STRUCT-001 | `StructuralMechanicsModel` | `src/model/structural_mechanics/structural_mechanics_model.hh:54-374`; `structural_mechanics_model.cc:56-187` | FE engine `StructuralMechanicsFEEngine` = `FEEngineTemplate<IntegratorGauss, ShapeStructural, _ek_structural>`. DOFs per node: 3 in 2-D, 6 in 3-D (other dims → `TO_IMPLEMENT`). Dumper `structural_mechanics_model` (structural kind); `initDOFManager()` in the constructor (default type). Elemental data `extra_normal` (dim components, 0) is created for all structural types. `initFullImpl`: stress `ElementTypeMapArray` with `getNbStressComponents()` per type. `initSolver`: nodal fields `displacement` (incl. rotations), `external_force`, `internal_force`, `blocked_dofs`; registers DOF `displacement` (nodal, ndof components) and blocked; for `_dynamic`/`_dynamic_lumped` also `velocity`, `acceleration` as derivatives 1 and 2; lumped: allocates `lumped_mass`. `initModel`: `element_material` (default 0) and shape functions. `initFEEngineBoundary` is a no-op. `getMatrixType` → `_symmetric` for every id. | structural_mechanics | — | `test_structural_mechanics` (gtest; meshes `_bernoulli_beam_2.msh`, `_bernoulli_beam_3.msh`, `_discrete_kirchhoff_triangle_18.msh`; fixture `test_structural_mechanics_model_fixture.hh`) | **Not parallel**: no synchronizers, `_not_ghost` only. Energies use `isLocalOrMasterNode` and allreduce. | Package OFF by default: the oracle build must enable it explicitly. |
| STRUCT-002 | Structural materials | `structural_mechanics_model.hh:42-52, 277-293`; `structural_mechanics_model_inline_impl.hh:31-80` | `StructuralMaterial {E=0, A=1, I=0, Iz=0, Iy=0, GJ=0, rho=0, t=0, nu=0}` (plain struct, not parsed from input files). `addMaterial(mat, name="")` (default name `material_<i>`, duplicate → exception) returns its index; `getMaterial(index/name)`, `getMaterialByElement`, `getNbMaterials`. Element → material assignment via `getElementMaterial(type)` (user-filled `UInt` array). | structural_mechanics | — | beam tests | — | `set_ID` ("beam sets") is allocated and unused. |
| STRUCT-003 | Bernoulli beam 2-D (`_bernoulli_beam_2`) | `structural_elements/structural_element_bernoulli_beam_2.hh:32-54`; shapes R1 | Tangent moduli (2 stress components: axial, bending) per quadrature point: `D = diag(E·A, E·I)`. | structural_mechanics | — | `TestStructBernoulli2.TestDisplacements` (two materials: E=3e10, I=0.0025 / 0.00128, A=0.01; expects rotations 5.6/4800, −3.7/4800 and axial 10/18, tolerance `tol` from the fixture) | — | — |
| STRUCT-004 | Bernoulli beam 3-D (`_bernoulli_beam_3`) | `structural_elements/structural_element_bernoulli_beam_3.hh:30-57` | 4 stress components: `D = diag(E·A, E·Iz, E·Iy, GJ)`. Uses the `extra_normal` elemental data (orientation, through the shape functions, R1). | structural_mechanics | — | `TestStructBernoulli3Static.TestDisplacements` (all properties 1; expects `v_z = −5/48`, `θ_y = −√2/8`, θ_x = θ_z = 0) | — | — |
| STRUCT-005 | Discrete Kirchhoff triangle (`_discrete_kirchhoff_triangle_18`) | `structural_elements/structural_element_kirchhoff_shell.hh:30-55` | 6 stress components. Membrane `D_m = E t/(1−ν²)·[[1,ν,0],[ν,1,0],[0,0,(1−ν)/2]]`; bending block `D_m·t³/12`. | structural_mechanics | — | `TestStructDKT18.TestDisplacements` (Batoz vol. 2 p. 392 patch test; E=1, t=1, ν=0.3; prescribed membrane + bending field; centre node expected `(22.5, 22.5, −337.5, −22.5, 22.5, 0)` with `‖err‖ < 1e-12`). `test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5.cc` is **not registered** and uses a removed API (`testConvergenceIncrement`). | — | **Suspect:** the bending block is `E t·t³/(12(1−ν²)) = E t⁴/…` (the extra factor t). It is invisible in the t=1 test (Q10). |
| STRUCT-006 | Stiffness assembly | `structural_mechanics_model.cc:190-217`; `structural_mechanics_model_inline_impl.hh:90-134` | Assembled once (`need_to_reassemble_stiffness` never reset → linear only): zero K; per structural type of the mesh dimension (not-ghost): `D` per quadrature point (element specialisations), `BᵀDB` with `B` = the FE engine's shape derivatives (`tangent_size × ndof·nnodes`), integrated, assembled symmetric. | structural_mechanics | — | all structural tests | — | Elements of lower dimension than the mesh (beams in a 3-D mesh) are included because `elementTypes(spatial_dimension, …)` filters on dimension; check the shells-in-3-D path. |
| STRUCT-007 | Stresses and internal force | `structural_mechanics_model.cc:220-234, 499-530`; `_inline_impl.hh:137-176` | `computeStresses()`: σ = D·B·u_e per quadrature point (generalised stresses). `assembleInternalForce()`: zero, compute the stresses, for every structural type (all dimensions) `f_int −= ∫ Bᵀσ`. Residual = external + internal; split parts `external` / `internal` (`canSplitResidual = true`). `afterSolveStep(converged)` recomputes the internal force. | structural_mechanics | — | all | — | — |
| STRUCT-008 | Solver configuration | `structural_mechanics_model.cc:423-496` | IDs: `_static` → `static`; `_implicit_dynamic` → `implicit`; `_explicit_lumped_mass` → `explicit_lumped`; `_explicit_consistent_mass` → `explicit`; others print `"UNKOWN."` and return `unknown`. Options: `_dynamic_lumped` → `_lumped`, central difference, acceleration; `_static` → NR, pseudo-time; `_dynamic` → NR + central difference + acceleration if the method is `_explicit_consistent_mass`, else NR + `_trapezoidal_rule_2` + displacement. | structural_mechanics | — | `TestStructBernoulliDynamic` (typed over beam_2 / beam_3, `_implicit_dynamic`, analytical solution `ω = π²/L²·√(EI/ρ)`; E=1e9, ρ=10, I=A=GJ=1; checks the consistent mass `ρAl/420·[…]` and stiffness matrices against hand-built element matrices) | — | — |
| STRUCT-009 | Consistent mass | `structural_mechanics_model_mass.cc:31-83` | `M` assembled once with `assembleFieldMatrix(ρ·A)` (mass per unit length), not-ghost. | structural_mechanics | — | `TestStructBernoulliDynamic` | — | Relies on R1's `assembleFieldMatrix` for structural shape functions (Hermite). |
| STRUCT-010 | Lumped mass | `structural_mechanics_model_mass.cc:86-205` | Only Bernoulli beams (others throw). Per beam: `m = ρ·A·L`, `m/2` to each node's translational DOFs, nodal volume `A·L/2`. Rotational inertia: 2-D `I = m·V/π` (disc), 3-D `I = 2/5·m·(3V/(4π))^{2/3}` (sphere). Assembled to the lumped `"M"` and also kept in the `lumped_mass` array. | structural_mechanics | — | none | — | **Bugs:** `node2 = conn(0)` (should be `conn(1)`, `:125-126`), so L = 0 and the lumped mass is zero; in 3-D the loop overwrites **all** DOFs (including translations) with the inertia (`:193-195`). Unusable as is (Q11). |
| STRUCT-011 | Energies | `structural_mechanics_model.cc:533-645` | `getEnergy("kinetic")`: lumped `½ Σ_local/master m v²` (all DOFs) or consistent `½ vᵀMv` (`assembleMatMulVectToArray`); 0 if there is no mass. `"potential"`: `½ uᵀKu` over local/master nodes; allreduce; other ids → 0. | structural_mechanics | — | — | Allreduce. | `getPotentialEnergy` calls `assembleStiffnessMatrix()`, which won't reassemble (linear). |
| STRUCT-012 | Line loads (traction arrays) | `structural_mechanics_model.cc:648-725`; `structural_mechanics_model_boundary.cc` (empty, includes only) | `computeForcesByLocalTractionArray(tractions[nb_elem·nb_quad × ndof], type)`: `f_ext += ∫ Nᵀ t` (not-ghost). `computeForcesByGlobalTractionArray(t_global, type)`: rotate with the shape functions' per-element rotation `R` (`t_local = R·t_global`) and then the local version. | structural_mechanics | — | beam_2 / beam_3 tests (Neumann via the fixture) | — | `computeForcesFromFunction` is `#if 0` dead code. |
| STRUCT-013 | Structural dumps | `structural_mechanics_model.cc:244-349` | Nodal: `displacement`, `velocity`, `acceleration` (first n = 2 (2-D) / 3 (3-D) components), `rotation` (remaining components), `force`/`external_force`, `momentum`, `internal_force`, `internal_momentum`, `mass` (strided fields, padding 3); `blocked_dofs`. Elemental: `element_index_by_material`, `stress`. `setTimeStep` also sets the dumper time step. | structural_mechanics | IOHelper | — | — | — |

### 1.14 Model couplers (CPL)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| CPL-001 | `CouplerSolidContactTemplate<SMM>` (monolithic solid + contact) | `src/model/model_couplers/coupler_solid_contact.hh:38-276`; `coupler_solid_contact.cc:27-57`; `coupler_solid_contact_tmpl.hh:31-374` | A `Model` owning `solid` (`SolidMechanicsModel`, id `solid_mechanics_model`) and `contact` (`ContactMechanicsModel`, id `contact_mechanics_model`), both built on the **coupler's DOF manager**. `initFullImpl` → `solid->initFull(method)`, `contact->initFull(method)`. Default solver ID / type / options are delegated to the solid. Residual: (explicit lumped) copy the solid's current positions into the detector and `contact->search()`; `assembleInternalForces` (solid then contact); `r += f_ext + f_int(solid) + f_contact`. Split residual: `external` = f_ext + f_contact, `internal` = f_int. `corrector`: solid corrector, then for `_static`/`_implicit_dynamic` a new search on the updated positions. `predictor`: solid only. `beforeSolveStep`/`afterSolveStep`: solid then contact. K: `solid->assembleStiffnessMatrix(true)` plus contact stiffness for static / implicit. M (consistent or lumped) from the solid. Forwards `applyBC`, `getStableTimeStep`, displacement / velocity / acceleration / external force / mass, `getContactForce()` (returns the **solid's** internal force), `getContactDetector`. Dumps: contact fields first, then solid fields; `dump()` calls `solid->onDump()` first. `CouplerSolidContact = CouplerSolidContactTemplate<SolidMechanicsModel>`. | contact_mechanics (+model_couplers) | — | `test_explicit_dynamic`, `test_explicit_friction` (registered); `test_contact_coupling`, `test_coupled_stiffness` (`test_coupler/`, **not added**; they use the unsupported `_implicit_contact`); `test_explicit_resolution` (`test_resolution/`, not added); examples (R4) | Not parallel (contact). Data accessors are empty. | `getContactForce()` returning the solid internal force is misleading. Keep the API but document it. |
| CPL-002 | Contact coupling iteration scheme | `coupler_solid_contact_tmpl.hh:66-148` | **Explicit**: one search per residual assembly, i.e. per time step (lumped NLS has one residual per step). **Static / implicit**: contact elements, gaps and normals are frozen during a residual evaluation and refreshed in every NR `corrector()`, so the NR iteration doubles as the contact active-set iteration; K includes the contact moduli of the current set. NR type `_newton_raphson_contact` is handled exactly like `_newton_raphson` (NLS-003). | contact_mechanics | — | as CPL-001 | — | — |
| CPL-003 | `CouplerSolidCohesiveContact` | `coupler_solid_cohesive_contact.hh:34-35`; `coupler_solid_cohesive_contact.cc:28-63` | Specialisation with `SolidMechanicsModelCohesive` (id `solid_mechanics_model_cohesive`, shared DOF manager) and a contact model built on **`mesh.getMeshFacets()`** with its **own** DOF manager. Dumper `coupler_solid_cohesive_contact` (cohesive kind). `initFullImpl` uses `CouplerSolidCohesiveContactOptions` (`is_extrinsic` forwarded to the solid). `checkCohesiveStress()` is forwarded. | contact_mechanics + cohesive_element | — | example `examples/c++/contact_mechanics_model/hertz/cohesive_contact_explicit_dynamic.cc` (R4); Python `py_model_couplers.cc` (R4); no test | Not parallel. | The contact model's own DOF manager is **not unused**: in static / implicit runs the contact stiffness is assembled into it and never solved (nor zeroed), so the solved `K` is the solid stiffness alone (UD-772; corrected by default in the port, compat switch). |
| CPL-004 | `CohesiveContactSolverCallback` | `cohesive_contact_solvercallback.hh:33-74`; `.cc:27-127` | A stand-alone `SolverCallback` combining `SolidMechanicsModelCohesive&`, `ContactMechanicsModel&` and an `AnalysisMethod&`, with the same logic as CPL-001/002 (search per residual when explicit, per corrector when static / implicit; K = solid + contact for static / implicit; M from the solid). `getMatrixType` always `_symmetric`. Intended for `solid.solveStep(callback)`. | contact_mechanics + cohesive_element | — | none | — | Duplicate logic of CPL-001; port as a thin adapter over the same Rust implementation. |
| CPL-005 | `CouplerSolidPhaseField` (staggered) | `coupler_solid_phasefield.hh:50-244`; `coupler_solid_phasefield.cc:34-527` | A `Model` owning `solid` (`SolidMechanicsModel`, id `solid_mechanics_model`) and `phase` (`PhaseFieldModel`, id `phase_field_model`), each with **its own** DOF manager (none passed). The coupler itself never calls `initDOFManager`; users call `solid.initFull(...)` and `phase.initFull(...)` directly (tests). Registers the element synchronizer for `_csp_damage`, `_csp_strain` (no corresponding accessor data). `solve(solid_solver_id, phase_solver_id)` (§2.9): **one staggered pass** per call: solid solve → `computeStrainOnQuadPoints(_not_ghost)` → phase solve → `computeDamageOnQuadPoints(_not_ghost)` and `(_ghost)` → `solid->assembleInternalForces()`. The SolverCallback overrides (residual / matrix / predictor / corrector) forward to both models but are only reachable through `Model::solveStep` on the coupler (not used). Default solver options: lumped → central difference; `_dynamic` → **lumped NLS + central difference**; static → `_linear` + pseudo-time. | phase_field (+model_couplers) | — | `test_phase_solid_coupling`, `test_phase_solid_explicit` (`material_coupling.dat`, `test_one_element.msh`; analytical 1-element damage `d = ε²c22/(g_c/l0 + ε²c22)` with relative tolerance 1e-8), `test_phase_field_anisotropic`, `test_phase_field_deviatoric_split`, `test_penalization`, `test_multi_material` (UNSTABLE); examples `phase_field_notch.cc`, `phase_field_parallel.cc`, Python `phasefield-static.py`, `phasefield-dynamic.py` (R4) | Damage is transferred for ghost elements too (from the ghost-interpolated nodal damage); strain only for not-ghost elements. | `checkConvergence` (unused) is buggy (`d_n_it` never advances, signed sum without squares). There is no fixed-point iteration: staggering convergence is the user's responsibility. |
| CPL-006 | Strain transfer solid → phase field | `coupler_solid_phasefield.cc:372-407` | For every regular element type of the mesh dimension: `ε = ½(∇u + ∇uᵀ) − ε_th·I` from the solid's flattened internals `grad_u` and `epsilon_th`, then `phase->inflateInternal("strain", …)`. | phase_field | — | phase-solid tests | not-ghost only | Requires every solid material to define `epsilon_th` (`MaterialPhaseField`, R2). |
| CPL-007 | Damage transfer phase field → solid | `coupler_solid_phasefield.cc:328-369` | Asserts equal counts of phase fields and materials. For every material / phasefield pair **with the same name**: interpolate the nodal damage to the phase-field quadrature points (no element filter), then copy into `MaterialDamage<dim>::getDamage()` per type. | phase_field + solid (MaterialDamage, R2) | — | phase-solid tests | Done for both ghost types. | Copies whole per-type arrays: a material and its phase field must cover exactly the same elements in the same order (same selector). |

## 2. Algorithms and numerical specifications

Notation: `Δt` = time step (TSS `time_step`); `u, u̇, ü` = DOF array and its
derivatives; `δ` = solution increment returned by the (non-)linear solve; `r` =
residual (right-hand side, "forces"); blocked = per-DOF blocked flag.
Everything is applied entry-wise to **non-blocked** DOFs only, unless stated.

### 2.1 Call graph of one `solveStep`

`ModelSolver::solveStep(id)` → `TimeStepSolverDefault::solveStep(model)`, which
stores the model callback and calls `NonLinearSolver::solve(tss)`. The TSS
itself is the callback seen by the NLS (`time_step_solver_default.cc:163-167`).

```
NLS::solve(tss):
  tss.beforeSolveStep()     → model.beforeSolveStep(); scheme.store() for each DOF
  dofm.updateGlobalBlockedDofs()
  tss.predictor()           → model.predictor(); for each DOF: savePrevious (if registered); scheme.predictor(Δt)
  [NR only] tss.assembleMatrix("J") → K/M/C via model.assembleMatrix(name); scheme.assembleJacobian; dofm.applyBoundary("J")
  tss.assembleResidual()    → (lumped) M via model; residual = 0; model.assembleResidual(); scheme.assembleResidual(is_lumped)
  solve linear system / lumped division → δ;  dofm.splitSolutionPerDOFs()
  tss.corrector()           → scheme.corrector(sol_type, Δt); increment update; model.corrector()
  ... (iterations) ...
  tss.afterSolveStep(conv)  → if !conv: scheme.restore(); model.afterSolveStep(conv)
```

* Each integration scheme adds its inertial contribution after the model's
  residual (`time_step_solver_default.cc:243-257`).
* Mass matrices are (re)requested from the model at every residual assembly.
  Models avoid recomputation with flags (`need_to_reassemble_*`).
* The predictor runs **before** the residual. Contact search and other
  "per-step" logic sit in model hooks (`beforeSolveStep`, `corrector`,
  `assembleResidual`); see §2.11.

### 2.2 Integration schemes (exact formulas as implemented)

The *corrector coefficient table* gives the coefficients for each solution type:
`u += e·δ`, `u̇ += d·δ`, `ü += c·δ`. The Jacobian assembled into `J` (after
`J.copyProfile(...)`, which zeroes it) is `J = c·M + d·C + e·K` for 2nd order
and `J = d·M + e·K` for 1st order.

| Scheme (enum) | Class (params) | Predictor | Solution type → (e, d, c) | J | Needed matrices | Residual extra term |
|---|---|---|---|---|---|---|
| `_pseudo_time` | `PseudoTime` | none | any → `u += δ` | `K` | K | none |
| `_forward_euler` | `ForwardEuler` (α=0) | `u += (1−α)Δt·u̇` | `_temperature`: (1, 1/(αΔt)); `_temperature_rate`: (αΔt, 1) | `d·M + e·K` (profile from K) | M only | `−M·u̇` (consistent) or `−M_L∘u̇` (lumped) |
| `_trapezoidal_rule_1` | `TrapezoidalRule1` (α=½) | idem | idem | idem | K, M | idem |
| `_backward_euler` | `BackwardEuler` (α=1) | idem | idem | idem | K, M | idem |
| `_generalized_trapezoidal` | `GeneralizedTrapezoidal` (param `alpha`, default 0) | idem | idem | idem | K, M | idem |
| `_central_difference` | `CentralDifference` (α=0, β=½) | `u += Δt u̇ + ½Δt² ü`; `u̇ += Δt ü`; `ü` kept | see Newmark | `M + (Δt/2)C` (K skipped since e=0) | M, C | `−C·u̇` (if C exists) then `−M·ü` or `−M_L∘ü` |
| `_fox_goodwin` | `FoxGoodwin` (α=1/6, β=½) | idem | see Newmark | full | K, M, C | idem |
| `_trapezoidal_rule_2` | `TrapezoidalRule2` (α=½, β=½) | idem | see Newmark | full | K, M, C | idem |
| `_linear_acceleration` | `LinearAceleration` (α=1/3, β=½) | idem | see Newmark | full | K, M, C | idem |
| `_newmark_beta` | `NewmarkBeta` (params `alpha`, `beta`, default 0, 0) | idem | `_acceleration`: (αβΔt², βΔt, 1); `_velocity`: (αΔt, 1, 1/(βΔt)); `_displacement`: (1, 1/(αΔt), 1/(αβΔt²)) | `c·M + d·C + e·K` (profile from M; K only if e≠0; C only if it exists) | K, M, C | idem |

Sources: `generalized_trapezoidal.cc:41-184`, `newmark-beta.cc:47-251`,
`pseudo_time.cc:37-75`, `integration_scheme_1st_order.cc:66-84`,
`integration_scheme_2nd_order.cc:70-93`.

* **Jacobian caching.** J is rebuilt only if a release changed (`M`, `K`,
  `C`) or `dof_manager.hasBlockedDOFsChanged()`. Releases are the AIJ
  `value_release`, which changes on any write.
* **Newmark parameter mapping.** `γ = beta`, `β_std = alpha·beta`.
* **Explicit central difference with lumped mass** (solid, contact, structural
  defaults). The step is: predictor; `r = f_ext + f_int(u_pred) − M_L ü_n`;
  `δ = r / M_L` (= `ü_{n+1} − ü_n`); corrector `ü += δ`, `u̇ += Δt/2·δ`,
  `u += 0`. This gives `u̇_{n+1} = u̇_n + Δt/2 (ü_n + ü_{n+1})`.
* **Increment arrays** (if registered, e.g. solid `displacement_increment`)
  are recomputed in the TSS corrector: `_displacement` solution type → the
  solution array; otherwise `u − u_previous` (`time_step_solver_default.cc:192-214`).

### 2.3 Newton–Raphson (`NonLinearSolverNewtonRaphson::solve`, `non_linear_solver_newton_raphson.cc:91-228`)

```
beforeSolveStep; updateGlobalBlockedDofs; predictor
if type == _linear and callback.canSplitResidual(): assembleMatrix("K")      (unreachable via factories, see NLS-004)
assembleResidual(cb)                       # split form only for _linear
if type == _newton_raphson_modified or (type == _linear and force_linear_recompute):
    assembleMatrix("J"); force_linear_recompute = false
n_iter = 0; converged = false; tol_eff = threshold
if criterion == _residual:
    converged = test(residual)             # absolute: ‖r‖ < threshold
    if converged: return                   # NOTE: no afterSolveStep, no corrector
    tol_eff = error₀ · threshold           # relative to the initial residual norm
loop:
    if type in {_newton_raphson, _newton_raphson_contact}: assembleMatrix("J")
    sparse_solver.solve()                  # J δ = r   (r already contains −K u etc.)
    corrector()
    if criterion == _residual: assembleResidual(cb); converged = test(residual)
    else:                      converged = test(solution δ)        # _solution and _residual_mass_wgh
    if criterion == _solution and not converged: assembleResidual(cb)
    n_iter++
while not converged and n_iter < max_iterations
if criterion == _solution: assembleResidual(cb)    # final residual for post-processing
afterSolveStep(converged)
if not converged: throw NLSNewtonRaphsonNotConvergedException(tol_eff, n_iter, error, criterion)
```

* `test(v)`: `error = sqrt(allReduce_sum Σ_{i local-or-master, not blocked} v_i²)`;
  converged iff `error < tol_eff` (strict). Defaults: `threshold = 1e-10`,
  `convergence_type = _solution`, `max_iterations = 10`.
* The **solution** criterion is the absolute ℓ² norm of the increment δ (all
  DOF ids together, unscaled).
* The **residual** criterion is relative to the initial residual, except for
  the early-return test, which is absolute.
* `_residual_mass_wgh` (mass-weighted residual) is **not implemented**: it
  falls into the solution branch.
* Iteration semantics: `n_iter` counts linear solves. The exception is thrown
  *after* `afterSolveStep(false)` has restored the stored state
  (`TimeStepSolverDefault::afterSolveStep`).
* Linear (`NonLinearSolverLinear::solve_linear`, `.cc:264-288`): one J
  assembly, one residual, one solve, corrector, then the internal residual is
  re-assembled for output.

### 2.4 Dirichlet boundary conditions in matrices, residuals and solutions

1. **Imposition.** BC functors write the prescribed value into the primal array
   (e.g. displacement) and set `blocked_dofs` (BC-002/BC-004). The integration
   scheme predictor and corrector never touch blocked entries, so the imposed
   values persist. Increments `δ` on blocked DOFs are ignored.
2. **Matrix.** `applyBoundary("J")` (DOF-006):
   * AIJ: zero every off-diagonal entry in a blocked row **or column**; set the
     diagonal to 1 on the owning (local/master) rank and to 0 on others
     (SPM-004).
   * PETSc: `MatZeroRowsColumnsLocal(blocked, 1.0)`.
   * No lifting of prescribed values into the RHS is performed, because the
     residual already uses the full `u` (including imposed values). Hence
     `J δ = r` with identity rows gives `δ_blocked = r_blocked` (non-zero in
     general) but it is discarded by the correctors.
3. **Residual.** `DOFManagerDefault` keeps the residual values on blocked rows.
   Reactions can be read there (the solid model's `internal_force` /
   `external_force` hold them; see R2). They are excluded from the NR norm
   (NLS-005). PETSc SNES / TAO explicitly zero the blocked residual entries
   before handing them to PETSc (`non_linear_solver_petsc.cc:107-112`).
4. **Lumped explicit.** No special treatment: `δ = r/M` is computed everywhere
   and discarded on blocked DOFs.

### 2.5 Sparse solvers (MUMPS / Eigen / PETSc settings)

**MUMPS** (`sparse_solver_mumps.cc`), double precision `DMUMPS_STRUC_C`:

| Setting | Value | Where |
|---|---|---|
| `par` | 1 (host participates); 0 only for the unused `_master_slave_distributed` | `:197-204` |
| `comm_fortran` | `MPI_Comm_c2f(MPI_COMM_WORLD of the communicator)` (MPI builds) | `:206-212` |
| `sym` | 2 (general symmetric) if the AIJ matrix is `_symmetric`, else 0 | `:221` |
| `job` | −1 init, 1 analysis (when the profile release changed), 2 factorisation (when the value release changed), 3 solve every call, −2 destroy | `:226, 242, 262, 313, 90` |
| ICNTL(1..4) | 0, 0, 0, 0 (silent); debug builds raise them with the debug level (ICNTL(1)=6 ≥ warning, ICNTL(3)=6 ≥ info, ICNTL(2)=6 ≥ trace, ICNTL(4) ∈ {1,2,3,4}); `write_problem = "mumps_matrix.mtx"` at `dblDump` | `:109-137` |
| ICNTL(5) | 0 (assembled matrix) | `:147` |
| ICNTL(8) | 77 (automatic scaling choice) | `:144` |
| ICNTL(18) | 3 (distributed assembled matrix: `nz_loc`, `irn_loc`, `jcn_loc`, `a_loc` = this rank's AIJ triplets) with MPI; 0 (centralized on host) otherwise | `:165-187` |
| ICNTL(20), ICNTL(21) | 0, 0 (dense centralized RHS on host; centralized solution on host) | `:150-151` |
| ICNTL(28) | 0 (automatic sequential / parallel analysis) | `:154` |
| ICNTL(14) | MUMPS default (20), +10 on each `INFO(1) = −9` retry up to 90 | `:336-352` |
| Other ICNTL (7 ordering, 6, 10 iterative refinement, …) | MUMPS defaults | — |
| `n` | `A.size()` (= system size) | `:156-163` |
| RHS | `getGlobalVector()` of the residual (root gather), overwritten with the solution, then `setGlobalVector` (root scatter + slave sync) and `splitSolutionPerDOFs` | `:271-283` |

Duplicated `(i, j)` triplets across ranks are summed by MUMPS. The symmetric
storage keeps **one** triangle per entry, but possibly mixed triangles (SPM-002),
which MUMPS accepts for `sym=2`.

**Eigen**: `SparseLU<SparseMatrix<double>>` with default ordering (COLAMD).
Symmetric AIJ is expanded to full. Serial only (`sparse_solver_eigen.cc:12-13`).

**PETSc**: `KSPCreate` + `KSPSetFromOptions` with options from the parser /
command line, operators `(J, J)`. No defaults are forced, so PETSc defaults
apply (KSPGMRES, PCBJACOBI with ILU(0) sub-PC in parallel, ILU(0) in serial;
rtol 1e-5). SNES uses `SNESNEWTONLS` + `SetFromOptions`; TAO uses `TAOGPCG` +
`SetFromOptions`.

### 2.6 Lumped mass / capacity solve

`x_i = b_a2x · r_i / M_i` for **every** local equation (`b_a2x` default 1),
including slave / pure-ghost equations (`non_linear_solver_lumped.cc:93-100`).
It is consistent because both r and M are complete on non-pure-ghost nodes
(§3.6). Pure-ghost values are garbage but unused.

### 2.7 Contact detection and penalty formulas

**Detection** (§1.12 CONT-006…011), per call to `ContactMechanicsModel::search()`:

1. `max_dd = max_bb =` max inradius, `min_dd =` min inradius of the elements
   around master nodes (current positions).
2. `BB_m`, `BB_s` = bounding boxes of the master / slave node lists, each
   enlarged by `max_bb`; `I = BB_m ∩ BB_s`. Master nodes inside I go into a
   grid of spacing `√2·max_dd` centred at the centre of I.
3. For each slave node in I: closest master node (Euclidean, ties → the last
   visited) among the neighbouring cells, excluding master nodes that share a
   regular element with the slave.
4. For each (slave s, master node m): over the facets around m that belong to
   `contact_surface`:
   * natural projection ξ (Newton, CONT-010) and unit tangents `τ_α` (columns
     of `X ∂N/∂ξ`, each normalised);
   * outward normal `n` (2-D `(−τ_y, τ_x)`, 3-D `τ₁×τ₂/|·|`), flipped outward
     using `outsideDirection`;
   * accept the facet if `(s − x(ξ))·n < 0` (penetration),
     `|s − x(ξ)| ≤` the best so far, and `ξ ∈ [−1−tol_ext, 1+tol_ext]^{d−1}`.
5. Self-contact filter: reject if `|g| > 2 min_dd` or `n̄_s·n ≥ 0`.
6. `gap = |s − x(ξ)| ≥ 0` (after the model's sign flip); `normal`,
   `tangents`, `projection` stored at the slave node;
   `ContactElement{s, facet}`; nodal area `A_s` (CONT-003).

**Penalty, linear (`penalty_linear`)** — `resolution_penalty.cc`:

* Normal traction `p_N = ε_n ⟨g⟩` (quadratic variant: `ε_n(⟨g⟩² + ⟨g⟩)`).
* Shape matrix `A = [I_d, −N₁(ξ)I_d, …, −N_n(ξ)I_d]` (`d × d(n+1)`); its ξ
  derivatives `A_β = [0, ∂_βN₁ I_d, …]` (`resolution_utils.hh:43-103`, note the
  sign: `+∂N`).
* Covariant metric `a_αβ = τ_α·τ_β`, contravariant `a^{αβ} = (a)⁻¹`.
* Normal force (local, size `d(n+1)`): `f_N = p_N A_s Aᵀ n`.
* Friction (only if μ ≠ 0 and the slave had a contact element at the previous
  search):
  * transported previous traction: `t̂_α = Σ_{γ,β} t^{prev}_γ (a^{prev})^{γβ} (τ^{prev}_β·τ_α)`;
  * slip increment (real space): `Δx = x_{prev elem}(ξ) − x_{prev elem}(ξ_prev)`,
    both evaluated with the **coordinates of the previous master element**,
    even for the current ξ (`:232-241`);
  * `Δξ^β = Σ_γ a^{βγ} (Δx·τ_γ)`;
  * trial: `t^{tr} = t̂ − ε_t a Δξ`;
  * `‖t‖ = sqrt(t_α a^{αβ} t_β)`;
  * stick if `‖t^{tr}‖ ≤ μ p_N` (`t = t^{tr}`, state `_stick`), else slip
    (`t = μ p_N t^{tr}/‖t^{tr}‖`, state `_slip`);
  * `tangential_tractions[s] = t`;
  * tangential force `f_T = A_s Σ_{α,β} t_β a^{αβ} Aᵀ τ_α`.
* Assembly: slave block `f[0:d]` assigned to the slave node (rigid master), or
  `½·f` to all nodes (deformable master), CONT-004. Contact force =
  `f_N + f_T`, added to the residual with a **plus** sign
  (`coupler_solid_contact_tmpl.hh:88-90`).
* Normal moduli:
  ```
  K_N = ε_n H(g) A_s Aᵀ(n⊗n)A
        − ε_n H(g) g A_s Σ_{α,β} a^{αβ} [ A_βᵀ (n⊗τ_α) A + Aᵀ (τ_α⊗n) A_β ]
  ```
  The quadratic variant uses `(2g+1)` in the first term, `(g²+g)` in the
  second, and `n⊗τ_α` in **both** bracket terms.
* Stick moduli:
  ```
  K_st = −ε_t A_s Σ_{αβ} a^{αβ} Aᵀ(τ_α⊗τ_β)A
         − A_s Σ_α t_α Σ_{β,γ,θ} [ Aᵀ(τ_γ⊗τ_θ)A_β a^{αθ} a^{βγ} + A_βᵀ(τ_γ⊗τ_θ)A a^{αγ} a^{βθ} ]
  ```
* Slip moduli: `K_sl = k₃ + k₄ − k₁ − k₂` with
  * `k₁ = Σ_{αβ} ε_n μ t_α a^{αβ}/‖t‖ A_s Aᵀ(τ_β⊗n)A`;
  * `k₂ = Σ_{αβ} ε_t μ p_N a^{αβ}/‖t‖ A_s Aᵀ(τ_α⊗τ_β)A`;
  * `k₃ = Σ ε_t μ p_N t_α t_β a^{αγ} a^{βθ}/‖t‖³ A_s Aᵀ(τ_γ⊗τ_θ)A` (**not zero-initialised**);
  * `k₄ = Σ μ p_N t_α/‖t‖ A_s [Aᵀ(τ_γ⊗τ_θ)A_β a^{αθ}a^{βγ} + A_βᵀ(τ_γ⊗τ_θ)A a^{αγ}a^{βθ}]`,
    where in `k₄` β runs over the zipped `(β, τ_β, A_β)`.

  See `resolution_penalty.cc:478-593` for exact index placement.
* Stiffness is added to the global K at row/col `node·d + i` (CONT-005).
  Tangential stiffness is computed only for static / implicit couplers.

### 2.8 Diffusion (heat transfer)

* PDE: `ρc ∂u/∂t − ∇·(D ∇u) = q`, with `D(T) = k + k_var(T − T_ref)·𝟙`
  (element-wise added to every entry of the conductivity matrix; HeatDiffusion).
* Residual: `r = q_ext − ∫ Bᵀ (D ∇u) dΩ − [M u̇ or M_L∘u̇]`.
* Matrices: `K = ∫ Bᵀ D B` (D at the current temperature release), consistent
  `M = ∫ ρc NᵀN` (`assembleFieldMatrix`), lumped `M_L` via
  `assembleFieldLumped` (R1 FE engine).
* Schemes:
  * explicit = forward Euler (`_temperature_rate`, lumped);
  * implicit = backward Euler (`_temperature`): `J = M/Δt + K`, NR (one
    iteration if linear; the solution criterion converges at the second);
  * static = pseudo-time: `J = K`.
* Stable step (explicit): `Δt = min_e(2 h_e²/4 · ρc / k_max)` with `h_e` = min
  inradius over the mesh and `k_max` = largest eigenvalue of the
  conductivity, then allreduce min (`heat_diffusion.cc:75-87`,
  `diffusion_model.cc:238-267`).

### 2.9 Phase field

* Unknown: nodal damage `d`. Internals per quadrature point: elastic energy
  density φ (from the energy split), history `φ_prev`, `g_c` (possibly random
  field), length `l0`.
* **AT2 (`quadratic`).** Weak residual
  `R(d) = −∫ N (−2φ + d(2φ + g_c/l0)) − ∫ Bᵀ (g_c l0 ∇d)`, Jacobian
  `K = ∫ (2φ + g_c/l0) NᵀN + ∫ g_c l0 BᵀB`. Strong form:
  `−g_c l0 Δd + (g_c/l0 + 2φ) d = 2φ`. History: `φ = max(φ(ε), φ_prev)`
  (`φ_prev` initialised to 0).
* **AT1 (`linear`).** `R = −∫ N (−2φ + 3g_c/(8l0) + 2φ d) − ∫ Bᵀ(¾ g_c l0 ∇d)`,
  `K = ∫ 2φ NᵀN + ∫ ¾ g_c l0 BᵀB`. `φ_prev` initialised to `3g_c/(16 l0)`
  (elastic threshold).
* **Penalisation** (`irreversibility_type = penalization`):
  * AT2: `+γ⟨d − d_prev⟩₋` in the driving force and `+γ[d < d_prev]` in the
    density, with `γ = g_c/l0 (1/tol_ir² − 1)`.
  * AT1: `γ = (27/64) g_c/(l0 tol_ir²)` plus a recovery term
    `ρ_rec⟨d⟩₋` with `ρ_rec = 9 g_c (diam/l0 − 2)/(64 l0 tol_rec)`.
* **`tao`**: no history, no penalisation (bounds are expected via
  `NonLinearSolverTAO::setBounds`).
* Energy splits:
  * none: `φ = ½λ tr(ε)² + μ ε:ε`;
  * volumetric–deviatoric: `φ = ½κ⟨tr ε⟩₊² + μ ε_d:ε_d`, κ = λ + 2μ/3,
    `ε_d = ε − (tr ε/3) I`.
  * λ uses the plane-stress form when `Plane_Stress = true`.
* Dissipated energy densities: AT2 `½ g_c l0 |∇d|² + g_c d²/(2 l0)`;
  AT1 `3/8 g_c l0 |∇d|² + 3 g_c d/(8 l0)`.
* Solid degradation (`MaterialPhaseField`, R2) uses the energy-split concepts
  `computeSigmaOnQuad` / `computeTangentCoefsOnQuad` (PF-009/PF-010).

### 2.10 Beam and shell formulations

Element shape functions and B matrices come from `ShapeStructural` /
`ElementClass<_bernoulli_beam_*>` / `<_discrete_kirchhoff_triangle_18>` (R1
`fe_engine/element_class_structural.hh`, `element_class_bernoulli_beam_inline_impl.hh`,
`element_class_hermite_inline_impl.hh`, `element_class_kirchhoff_shell_inline_impl.hh`,
`shape_structural*.hh`). The model contributes:

* generalised constitutive matrices `D` (STRUCT-003/004/005);
* `K_e = ∫ BᵀDB`;
* generalised stresses `σ = D B u_e`;
* `f_int = −∫ Bᵀσ`;
* `M_e = ∫ ρA NᵀN`;
* rotations of global tractions into local frames (`R` from the shape functions).

The beam test reference matrices (`test_structural_mechanics_model_bernoulli_beam_dynamics.cc:155-260`)
give the expected element `M` (`ρAl/420·[…]`) and `K`
(`b1 = 12EI/l³`, `b2 = 6EI/l²`, `b3 = 4EI/l`, …), which is the cleanest
independent verification available.

### 2.11 Coupler iteration schemes

* **Solid + contact (monolithic, CPL-001/002/004).**
  * Explicit: `predictor (solid) → [assembleResidual: positions ← solid current positions; search; f_int(solid); f_contact] → lumped solve → corrector (solid)`.
  * Static / implicit NR: `predictor → residual (with the current contact set) → loop { J = K_solid + K_contact; solve; corrector (solid) → positions ← current; search (new active set, gaps, normals) → residual }`.

  The contact set is thus updated once per NR iteration.
* **Solid + phase field (staggered, CPL-005).** One `coupler.solve(ids)`
  call = one alternate-minimisation pass:
  1. solve the solid (`solid.solveStep(solid_id)`);
  2. `ε = sym(∇u) − ε_th I` → phase-field internal `strain` (not-ghost);
  3. solve the phase field (`phase.solveStep(phase_id)`);
  4. nodal `d` → quadrature points → `MaterialDamage::damage` (both ghost types);
  5. re-assemble the solid internal forces.

  There is no convergence loop (tests call it once per load step).
* **Cohesive + contact (CPL-003).** Same as solid + contact, but the contact
  model lives on `mesh_facets` and the surface selector (`Cohesive`/`AllSurfaceSelector`)
  tracks facets of inserted cohesive elements through mesh events.

## 3. Parallel design

### 3.1 Process model and communicator

* SPMD over `MPI_COMM_WORLD`. One static `Communicator` per process
  (`Communicator::getStaticCommunicator()`), with the MPI backend or a serial
  dummy chosen at compile time (`AKANTU_USE_MPI`).
* `MPI_Init` is called lazily by `MPICommunicatorData` unless the application
  initialised MPI itself (then Akantu doesn't finalise).
* An MPI error handler converts errors to `debug::CommunicationException`.
* Root rank 0 is hard-coded for mesh distribution and for gathers / scatters.

### 3.2 Mesh distribution (centralized, PART-001…008)

1. All ranks construct a `Mesh`; **only rank 0 reads the full mesh** (users
   guard `mesh.read` with `if (prank == 0)`).
2. `mesh.distribute()`: rank 0 partitions the elements of the mesh dimension
   with Scotch on the dual graph (shared facet = edge; optional vertex / edge
   weight functions).
   * Lower-dimensional elements (boundary / facet elements) follow the
     **minimum partition** among their adjacent elements.
   * With a periodic mesh, the slave nodes are temporarily replaced by their
     masters in the connectivity, so that periodic neighbours are graph
     neighbours.
   * Alternatively the user passes a `MeshPartition` (e.g. `MeshPartitionMeshData`).
3. **Ghost layer.** Element `e` (owned by p) is a ghost on every partition q ≠ p
   that owns an element sharing at least **one node** with e. The overlap is one
   element deep, across vertices, edges and faces.
4. Per element type (in `elementTypes` order), rank 0 sends each rank its local
   elements (global order) followed by its ghost elements (global order), the
   partition info (number of ghost partitions and their list per local element;
   owner per ghost element), the mesh data tags, and the element group
   memberships.
   * Local node numbering is the order of first appearance in the received
     connectivities (local elements before ghosts, types in order).
   * Element communication schemes (`ElementSynchronizer`): send to q = my local
     elements that are ghosts on q; receive from p = my ghosts owned by p. They
     exist only for the elements of the mesh dimension.
5. Nodes: each rank reports its global node ids and gets the coordinates back.
   * **Node flags** from the connectivity:
     * only in local elements → `_normal` (prank = me);
     * only in ghost elements → `_pure_ghost`;
     * in both → candidate `_master`.
   * Rank 0 resolves the candidates: **the lowest rank keeps `_master`, the
     others become `_slave`** (prank = that lowest rank).
   * Node schemes (`NodeSynchronizer`): master sends to each slave rank;
     slaves receive from their prank. Pure ghosts are not in node schemes.
     Their prank is set; their values come through element-synchronizer
     nodal packs.
   * Node groups, periodic pairs (including missing remote periodic partners,
     added as pure ghosts) and nodal data tags are distributed.
6. `fillElementToSubElementsData`; `is_distributed = true` (**also with one
   rank**); bounding box allSum; `MeshIsDistributedEvent`. DOF managers and
   models must be created after this point.

### 3.3 Ownership rules (summary)

| Entity | Owner | Copies elsewhere |
|---|---|---|
| Element (mesh dimension) | Scotch / user partition | ghost on every rank sharing a node with it |
| Lower-dimension element | min partition of the adjacent elements | ghost on the other adjacent partitions and the ghost partitions of that element |
| Node | the rank owning all elements touching it (`_normal`), else the **lowest** rank having the node in both local and ghost elements (`_master`) | `_slave` on other ranks with local elements touching it; `_pure_ghost` on ranks seeing it only through ghosts |
| Ghost facet (`mesh_facets`) | min owner among the adjacent ghost elements received from neighbours, else me | — |
| DOF (nodal) | the owner of its node; DOFs of periodic slaves are aliased to the master's equation | slave / pure-ghost local equations |
| Global equation number | rank-blocked per DOF registration (exscan over local/master counts) | slaves learn theirs from the master (`_ask_nodes`); pure ghosts have none (−1) |
| Cohesive-inserted nodes | numbered by `GlobalIdsUpdater` (exscan over new local/master nodes) | slaves get ids and pranks from the owners (`_giu_global_conn`, reduce then sync) |

### 3.4 What each synchronizer exchanges and when

| Synchronizer | Entities | Typical tags (who registers) | When |
|---|---|---|---|
| `ElementSynchronizer` (mesh) | elements of the mesh dimension (send = local, recv = ghost) | SMM `_smm_mass`, `_smm_stress`, `_smm_gradu`, `_smm_density`, `_for_dump` (R2); diffusion `_diffusion`, `_diffusion_gradient`; phase field `_pfm_damage`, `_for_dump`; couplers `_csp_damage`, `_csp_strain`; constitutive-law handler `_constitutive_law_id`, `_clh_init_cl` (R2); non-local `_mnl_*` (R2) | Every residual assembly. SMM: stresses computed on not-ghost, async sync to ghosts while assembling not-ghost, wait, then ghost assembly. Diffusion: blocking sync of nodal u on ghost-element nodes before the flow assembly. Phase field: async sync of driving forces / energies / damage at quadrature points. |
| `NodeSynchronizer` (mesh) | master → slave nodes | `_ask_nodes` (DOF global numbering), `_whatever` (`synchronizeArray`), gather / scatter to root | DOF registration; solver vector scatter; user `synchronizeArray` |
| `DOFSynchronizer` (DOFManagerDefault) | local equations of master → slave nodes | `_whatever` (`synchronizeArray`, `reduceSynchronizeArray`), gather / scatter of RHS / solution | every MUMPS / Eigen solve (gather residual to root, scatter solution, sync slaves) |
| `PeriodicNodeSynchronizer` (mesh) | periodic master → slave (remote + local copy) | `_giu_global_conn` (DOF numbering), `_whatever` | `makeConsistentForPeriodicity` (BC application, DOF extraction, lumped mass) |
| `FacetSynchronizer` (mesh_facets) | facets (owner → ghost copies) | `_smmc_facets_conn` (global connectivities → `flipFacets`), `_smmc_facets`, `_smmc_facets_stress`, `_ce_groups`, `_ce_insertion_order` (R2) | `initMeshFacets`; cohesive insertion checks |
| Cohesive element synchronizer (R2, a split of the element synchronizer) | cohesive elements | `_smmc_damage`, `_giu_global_conn` | insertion / global-id update |
| `GridSynchronizer` | extra ghost elements within a spatial-grid overlap (`SpatialGrid<Element / IntegrationPoint>`) | `_mnl_for_average`, `_mnl_weight`, `_nh_criterion` (non-local, R2) | non-local neighbourhood construction (adds ghosts and pure-ghost nodes via mesh events) |

* **Collective phases.**
  * Persistent asynchronous synchronizations, with sizes computed once per tag
    and recomputed after `invalidateSizes`.
  * One-shot `synchronizeOnce` / `slaveReductionOnce`.
  * Root gather / scatter.
  * The scheme-update protocols (`filterScheme`, node / DOF `onNodesAdded`)
    and dynamic exchanges with `receiveAnyNumber` (NBX with `MPI_Ibarrier`).

### 3.5 Global DOF numbering

* DOFs are registered per id (`displacement`, `temperature`, `damage`, …),
  in model order.
* For each registration, ranks number their local / master DOFs
  contiguously: `first = Σ_{q<p} n_q` (exscan) `+ (Σ_q n_q of earlier registrations)`.
* Slaves receive their global numbers from the master through the node
  synchronizer (`GlobalDOFInfoDataAccessor`, `_ask_nodes`). Pure ghosts keep −1.
* `global_to_local_mapping` maps every known global number to its local equation.
* Periodic slaves alias the master's local equation (no own number). Periodic
  masters that are remote are handled through the periodic synchronizer first.

### 3.6 Distributed assembly conventions

* **Residual / force vectors are "complete" on every non-pure-ghost node.**
  * Models compute element quantities on not-ghost elements, synchronize what
    ghost elements need, and assemble the contributions of **both** not-ghost
    and ghost elements into rank-local nodal arrays.
  * Result: local, master and slave nodes have the full nodal force.
    Pure-ghost nodes are incomplete (unused).
  * Neumann BCs also integrate over ghost boundary elements (BC-005).
  * Lumped masses / capacities are assembled over both ghost types too
    (DIFF-006, solid in R2).
  * Consequences: explicit solves are communication-free (`δ = r/M` locally);
    the NR norm counts local / master DOFs only.
* **Sparse matrices are "partial".**
  * Each rank assembles stiffness / mass only over its **not-ghost** elements,
    in global numbering, into its own AIJ triplet list.
  * The global matrix is the sum over ranks (MUMPS ICNTL(18)=3).
  * Dirichlet rows: diagonal 1 only on the owning rank (SPM-004).
* **Linear solve.**
  * Rank 0 gathers the residual (master values only) into a dense global RHS
    (`DOFSynchronizer::gather`), MUMPS solves with the distributed matrix, and
    rank 0 scatters the solution.
  * `synchronizeArray` then copies master → slave, and
    `splitSolutionPerDOFs` maps it onto the DOF arrays.
* **matVecMul caveat.** `SparseMatrixAIJ::matVecMul` produces rank-partial
  products on shared DOFs (SPM-005), which mixes conventions where it is used
  (consistent-mass inertia, Rayleigh damping, split linear residual, energy
  computations with `assembleMatMulVectToArray`).
* **PETSc variant.** PETSc vectors own local / master DOFs with ghost slots for
  slaves; matrices are assembled with `ADD_VALUES` and PETSc's
  stash / assembly handles the off-process entries.

### 3.7 Reductions

All `allReduce`, `_sum` unless noted:

* system size;
* NR norm;
* MUMPS `INFO(1)` (min);
* stable time steps (min);
* energies — diffusion, phase field, structural, solid (R2);
* `SolverVectorDistributed::isFinite` (land);
* `SynchronizerImpl::hasChanged` (lor);
* `GlobalIdsUpdater` counts;
* bounding-box allSum;
* contact: none (not parallel).

The reduction order is MPI-implementation-defined, so bitwise reproducibility
across rank counts is not expected (see `docs/NUMERICAL_VALIDATION.md`).

### 3.8 Parallel cohesive insertion (R2 algorithm, R3 infrastructure)

1. `initMeshFacets` builds `mesh_facets` with a `FacetSynchronizer` and
   consistent ghost-facet orientation (`flipFacets`).
2. Before insertion, the `insertion_facets` flags of owned facets are pushed
   to ghost copies (`_ce_groups`).
3. A global insertion order is defined by exscan and synchronized to ghost
   facets (`_ce_insertion_order`); facets are processed in that order so node
   doubling matches across ranks.
4. New nodes get global ids from `GlobalIdsUpdater` (reduce ghost → owner,
   then sync owner → ghost; `_giu_global_conn`).
5. The node / DOF synchronizers and DOF manager react to `NewNodesEvent`
   (DOF-013, MPI-017, MPI-018).

### 3.9 Parallel contact

**Not supported.**

* `ContactMechanicsModel`'s data accessors are empty.
* Detection works on the local positions of local + ghost nodes without any
  exchange.
* Forces are assigned on `_not_ghost` contact elements only.
* The contact stiffness is indexed by local node number (CONT-005).

A distributed run would silently miss cross-partition contacts and
double-count or lose forces on shared nodes. The Rust port must either
document the same limitation or design parallel contact from scratch (a
project decision; see D-004, D-089).

### 3.10 Parallel output

* Each rank dumps its local part.
* `DumperIOHelper::setParallelContext` passes `(rank, nb_proc)` to IOHelper,
  which writes per-rank Paraview pieces plus a parallel master file (R1).
* Ghost elements are not dumped (models dump `_not_ghost`).
* The `partitions` elemental field (`dumpers::ElementPartitionField`) shows
  the owning rank.
* `_for_dump` synchronizations refresh ghost data before dumps where
  registered.
* Matrix / vector debug files carry a `_rank-<r>` suffix.

### 3.11 Features that are NOT parallel-capable in the reference

| Feature | Reason / evidence |
|---|---|
| Eigen sparse solver (SOLV-004) | asserts nb_proc == 1; CMake warns (`packages/implicit.cmake:41-43`) |
| Contact mechanics, solid–contact and cohesive–contact couplers (CONT-*, CPL-001…004) | empty data accessors, node-number-indexed stiffness, no ghost search |
| Structural mechanics (STRUCT-*) | no synchronizer registration, not-ghost assembly only (energies are allreduced, but the model is effectively serial) |
| `CouplerSolidPhaseField` strain transfer | strain is pushed only for not-ghost elements; ghost strains on the phase side stay stale. Upstream has an example `phase_field_parallel.cc` (R4), so a parallel run is *intended*. Oracle check needed (Q6). |
| Phase-field ghost gradient (PF-003) | ∇d of ghost elements is written into not-ghost storage |
| `SparseMatrixAIJ::matVecMul` reductions (SPM-005) | partial products on shared DOFs |
| `DOFManagerPETSc` with several DOF ids or periodicity (DOF-015) | matrix/vector numbering mismatch, periodic copy bug |
| Root-centralised steps (mesh read, partitioning, MUMPS RHS) | functional, but not scalable |
| `SynchronizerImpl<Element>` gather / scatter | `TO_IMPLEMENT` when nb_proc > 1 (`synchronizer_impl_tmpl.hh:518-527`) |

## 4. Public API surface

These are the C++ entry points user code, examples and the Python bindings
(R4) rely on. `→` gives the main members. Rust equivalents should keep the
concepts and names (snake_case), not the C++ signatures.

### 4.1 Model infrastructure

* `Model` → `initFull(named args)`, `getMesh`, `getSpatialDimension`,
  `getAnalysisMethod`, `getFEEngine(name)`, `getFEEngineBoundary(name)`,
  `registerFEEngineObject<T>`, `unRegisterFEEngineObject`, `dumpGroup*`,
  `setGroupDirectory`, `setGroupBaseName`, `getGroupDumper`,
  `addDumpField[Vector|Tensor][ToDumper]`, `addDumpGroupField*`,
  `removeDumpGroupField*`, `setBaseName[ToDumper]`, `setDirectory[ToDumper]`,
  `setTextModeToDumper`, `dump(...)`, `getSynchronizerRegistry`,
  `createNodalField{Real,Int,Bool}` / `createElementalField` hooks.
* `ModelSolver` → `initDOFManager()`, `initDOFManager(shared_ptr)`,
  `initDOFManager(type_id)`, `getDOFManager`, `getNewSolver(id, options)`,
  `setIntegrationScheme(solver, dof, type[, sol_type])` (and the
  `unique_ptr<IntegrationScheme>&` overload), `solveStep([callback,] id)`,
  `getTimeStep(id)`, `setTimeStep(dt, id)`, `hasSolver`,
  `setDefaultSolver`, `hasDefaultSolver`, `hasIntegrationScheme`,
  `getTimeStepSolver(id)`, `getNonLinearSolver(id)`,
  `getDefaultSolverType`, `getDefaultSolverOptions`, `getParserSection`.
* `ModelSolverOptions` {`timestep_solver_type`, `non_linear_solver_type`,
  `sparse_solver_type`, `integration_scheme_type[dof]`, `solution_type[dof]`};
  `ModelOptions` and its aliases / derived option structs (MODEL-002).
* `SolverCallback`, `InterceptSolverCallback` (MODEL-007).
* Enums: `ModelType`, `AnalysisMethod`, `TimeStepSolverType`,
  `NonLinearSolverType`, `SparseSolverType`, `IntegrationSchemeType`,
  `IntegrationScheme::SolutionType`, `SolveConvergenceCriteria`,
  `MatrixType`, `DOFSupportType`, `SynchronizationTag`, `NodeFlag`,
  `ContactState`, `DetectionType`, `GhostType`.

### 4.2 DOF manager and solvers

* `DOFManager` →
  * registration: `registerDOFs(id, array, support_type | group)`,
    `registerDOFsPrevious`, `registerDOFsIncrement`,
    `registerDOFsDerivative`, `registerBlockedDOFs`;
  * assembly: `assembleToResidual`, `assembleToLumpedMatrix`,
    `assembleElementalArrayLocalArray`, `assembleElementalArrayToResidual`,
    `assembleElementalArrayToLumpedMatrix`,
    `assembleElementalMatricesToMatrix`, `assembleMatMulVectToArray`,
    `assembleMatMulVectToResidual`, `assembleMatMulDOFsToResidual`,
    `assembleLumpedMatMulVectToResidual`, `assemblePreassembledMatrix`;
  * boundary / zeroing: `updateGlobalBlockedDofs`, `zeroResidual`,
    `zeroMatrix`, `zeroLumpedMatrix`, `applyBoundary`;
  * extraction: `getLumpedMatrixPerDOFs`, `splitSolutionPerDOFs`,
    `getArrayPerDOFs`, `assembleToGlobalArray`;
  * DOF accessors: `getDOFs`, `getDOFsDerivatives`, `getBlockedDOFs`,
    `getDOFsIncrement`, `getPreviousDOFs`, `getSolution(id)`,
    `savePreviousDOFs`, `has*`;
  * numbering: `isLocalOrMasterDOF`, `isSlaveDOF`, `isPureGhostDOF`,
    `localToGlobalEquationNumber`, `globalToLocalEquationNumber`,
    `hasGlobalEquationNumber`, `getDOFFlag`, `getSystemSize`,
    `getLocalSystemSize`, `getPureLocalSystemSize`, `getDOFIDs`,
    `getLocalEquationsNumbers`;
  * blocked set: `getGlobalBlockedDOFs[Indexes]`, `hasBlockedDOFsChanged`;
  * registries: `getNewMatrix(id, type | copy_id)`, `getMatrix`, `hasMatrix`,
    `getNewLumpedMatrix`, `getLumpedMatrix`, `hasLumpedMatrix`,
    `getNewNonLinearSolver`, `getNonLinearSolver`, `hasNonLinearSolver`,
    `getNewTimeStepSolver`, `getTimeStepSolver`, `hasTimeStepSolver`;
  * misc: `getMesh`, `getCommunicator`, `getSolution()`, `getResidual()`,
    `saveAssociatedNodes`.
* `DOFManagerFactory` names: `"default"`, `"mumps"`, `"petsc"`.
* `DOFManagerDefault::getSynchronizer`, `hasSynchronizer`.
* `DOFManagerPETSc::getMPIComm`, `getISLocalToGlobalMapping`.
* `TimeStepSolver` → `solveStep(cb)`, `setIntegrationScheme`,
  `getIntegrationScheme(dof)`, `hasIntegrationScheme`, `get/setTimeStep`,
  `getNonLinearSolver`; parameter-registry access (`get/set` of
  sub-registry `non_linear_solver`).
* `IntegrationScheme` and subclasses (TS-001…008): `predictor`, `corrector`,
  `assembleJacobian`, `assembleResidual`, `getNeededMatrixList`, `store`,
  `restore`, `getOrder`; `GeneralizedTrapezoidal::getAlpha`;
  `NewmarkBeta::get/setAlpha`, `get/setBeta`.
* `NonLinearSolver` (parsable, `get/set` parameters by name: `threshold`,
  `convergence_type`, `max_iterations`, `force_linear_recompute`,
  `verbose_iteration`, readable `error`, `nb_iterations`, `converged`;
  lumped `b_a2x`; SNES / TAO PETSc options).
  `NonLinearSolverNewtonRaphson::getSparseSolver`,
  `NonLinearSolverTAO::setBounds(Vec, Vec)`.
* Exceptions: `debug::NLSNotConvergedException`,
  `NLSNewtonRaphsonNotConvergedException`, `SNESNotConvergedException`,
  `TAONotConvergedException`, `SingularMatrixException`,
  `SolverCallbackResidualPartUnknown`, `CommunicationException`.
* `SparseMatrix` / `SparseMatrixAIJ` → `add`, `addValues`, `set`, `zero`,
  `mul`, `add(B, α)`, `matVecMul`, `applyBoundary`, `copyProfile`,
  `copyContent`, `clearProfile`, `saveProfile`, `saveMatrix`,
  `operator()(i,j)`, `getIRN/JCN/A`, `getNbNonZero`, `size`,
  `getMatrixType`, `getRelease`, `getProfileRelease`, `getValueRelease`,
  `isFinite`, `min`, `resize`.
* `SolverVector` family → `resize`, `set`, `zero`, conversion to
  `Array<Real>`, `size`, `localSize`, `copy`, `+`, `release`,
  `isDistributed`, `isFinite`, `saveVector`, `getVector`,
  `getGlobalVector`, `setGlobalVector`.
* `SparseSolver` (`SparseSolverMumps`, `SparseSolverEigen`,
  `SparseSolverPETSc`) → `solve`, `initialize`, `analysis`, `factorize`.
* `TermsToAssemble(dof_m, dof_n)` → `operator()(i, j) = value`.

### 4.3 Boundary conditions

* `BoundaryCondition<Model>` → `applyBC(functor[, group_name | ElementGroup])`,
  `initBC`, `getPrimal`, `getDual`, `getModel`, `getDOFID`.
* `BC::Dirichlet::{DirichletFunctor, FlagOnly, FixedValue, IncrementValue (setIncrement), Increment (setIncrement)}`.
* `BC::Neumann::{NeumannFunctor, FromHigherDim (alias FromStress), FromSameDim (alias FromTraction), FreeBoundary}`.
* `BC::Axis` = `_x, _y, _z`.

### 4.4 Parallelism

* `Communicator` (MPI-001 API), `CommunicationBuffer` /
  `DynamicCommunicationBuffer`, `CommunicationRequest`,
  `CommunicationStatus`, `SynchronizerOperation`, `CommunicationMode`,
  `Tag`.
* `DataAccessor<Element>` / `<Idx>` (virtual `getNbData`, `packData`,
  `unpackData`; static helpers), `ReduceDataAccessor`,
  `SimpleIdxDataAccessor`, `SimpleElementDataAccessor`, `AddOperation`,
  `IdentityOperation`, `OrOperation`.
* `Synchronizer` → `synchronize`, `asynchronousSynchronize`,
  `waitEndSynchronize`, `synchronizeOnce`, `slaveReductionOnce`,
  `computeBufferSize`, `getCommunicator`.
* `SynchronizerImpl<E>` → `getCommunications`, `gather`, `scatter`,
  `synchronizeArray`, `reduceSynchronizeArray<Op>`, `split`,
  `updateSchemes`, `filterScheme`, `swapSendRecv`, `reset`.
* `ElementSynchronizer` (`getMesh`, `getElementToRank`, `getRank`),
  `NodeSynchronizer`, `DOFSynchronizer`, `PeriodicNodeSynchronizer`
  (`update`, `synchronizeArrayPBC`, `reduceSynchronizeArrayPBC`),
  `FacetSynchronizer`, `GridSynchronizer`.
* `SynchronizerRegistry` → `registerSynchronizer`, `registerDataAccessor`,
  `synchronize`, `asynchronousSynchronize`, `waitEndSynchronize`.
* `Mesh` parallel API →
  * distribution: `distribute(named args)`, `distribute(partition, comm)`,
    `isDistributed`;
  * node queries: `getNodeFlag`, `getNodePrank`, `isLocalNode`,
    `isMasterNode`, `isSlaveNode`, `isPureGhostNode`,
    `isLocalOrMasterNode`, `getNodeGlobalId`, `getNodeLocalId`,
    `getNbGlobalNodes`, `getGlobalNodesIds`;
  * synchronizers: `getElementSynchronizer`, `getNodeSynchronizer`,
    `getPeriodicNodeSynchronizer`, `getCommunicator`;
  * periodicity: `makePeriodic(...)`, `isPeriodic`,
    `isPeriodicSlave/Master`, `getPeriodicMaster`, `getPeriodicSlaves`,
    `getPeriodicMasterSlaves`, `makeConsistentForPeriodicity`,
    `getConnectivityWithPeriodicity`;
  * other: `getGlobalConnectivity`, `initMeshFacets`,
    `registerGlobalDataUpdater`, `updateGlobalData`.
* `MeshPartition` (`partitionate`, `reorder`, `getPartition`,
  `getGhostPartitionCSR`, `getNbPartition`), `MeshPartitionScotch`,
  `MeshPartitionMeshData` (`setPartitionMapping`).
* `MeshUtilsDistribution::distributeMeshCentralized`.
* `GlobalIdsUpdater` (`updateGlobalIDs`, `updateGlobalIDsLocally`,
  `synchronizeGlobalIDs`). `MeshGlobalDataUpdater`.

### 4.5 Physics models

* `DiffusionModel` / `HeatTransferModel` →
  * fields: `getDiffusion` / `getTemperature`,
    `getDiffusionRate` / `getTemperatureRate`, `getExternalFlow` /
    `getExternalHeatRate`, `getInternalFlow` / `getInternalHeatRate`,
    `getBlockedDOFs`, `getDOFName`, `getDiffusionRelease` /
    `getTemperatureRelease`;
  * `getStableTimeStep`, `setTimeStep`, `assembleDiffisivityMatrix` [sic],
    `assembleCapacityMatrix[Lumped]`, `getEnergy(id[, element])`;
  * constitutive-law handler API (R2): `getConstitutiveLaw(i | name)`,
    selectors, …
  * Diffusion law names: `heat_diffusion`. Parameters: `density`,
    `capacity`, `conductivity`, `conductivity_variation`,
    `temperature_reference`.
* `PhaseFieldModel` →
  * fields: `getDamage`, `getPreviousDamage`, `getInternalForce`,
    `getExternalForce`, `getBlockedDOFs`;
  * `assembleStiffnessMatrix`, `assembleInternalForces`,
    `savePreviousState`, `setTimeStep`, `getEnergy(id[, element | group])`;
  * laws: `setPhaseFieldSelector`, `getPhaseField(i | name)`,
    `getNbPhaseFields`; `applyBC` (BC mixin).
  * Phase-field law names: `linear`, `quadratic`; energy split option
    `no_split` / `volumetric_deviatoric`. Parameters: `l0`, `gc`, `E`, `nu`,
    `irreversibility_type`, `Plane_Stress`, `irreversibility_tol`,
    `recovery_tol` (linear only). `PhaseFieldElementFilter`.
* `ContactMechanicsModel` →
  * `search`, `computeNodalAreas`, `registerNewResolution`,
    `assembleStiffnessMatrix`, `assembleInternalForces`;
  * fields: `getDisplacement`, `getIncrement`, `getInternalForce`,
    `getExternalForce`, `getNormalForce`, `getTangentialForce`,
    `getTangentialTractions`, `getPreviousTangentialTractions`,
    `getBlockedDOFs`, `getGaps`, `getNormals`, `getTangents`,
    `getPreviousTangents`, `getNodalArea`, `getProjections`,
    `getPreviousProjections`, `getContactState`,
    `getPreviousMasterElements`;
  * `getContactDetector`, `getContactElements`, `getPositions`, `applyBC`.
* `ContactDetector` → `search`, `globalSearch`, `localSearch`,
  `createContactElements`, `setSurfaceSelector`, `getSurfaceSelector`,
  `get/setPositions`, `get/setMaximumDetectionDistance`,
  `get/setMinimumDetectionDistance`, `get/setMaximumBoundingBox`.
* `PhysicalSurfaceSelector`, `CohesiveSurfaceSelector`, `AllSurfaceSelector`.
* `Resolution` (`penalty_linear`, `penalty_quadratic`; parameters `name`,
  `mu`, `is_master_deformable`, `epsilon_n`, `epsilon_t`). `GeometryUtils`
  static helpers. `ContactElement`.
* `StructuralMechanicsModel` →
  * fields: `getDisplacement`, `getVelocity`, `getAcceleration`,
    `getExternalForce`, `getInternalForce`, `getBlockedDOFs`,
    `getLumpedMass` / `getMass`, `hasLumpedMass`, `allocateLumpedMassArray`;
  * elements: `getRotationMatrix`, `getStress`, `getElementMaterial`,
    `getSet_ID`;
  * materials: `addMaterial(StructuralMaterial, name)`, `getMaterial(i | name)`,
    `getMaterialByElement`, `getNbMaterials`;
  * energies: `getKineticEnergy`, `getPotentialEnergy`, `getEnergy(id)`;
  * loads / assembly: `computeForcesByLocalTractionArray`,
    `computeForcesByGlobalTractionArray`, `assembleLumpedMassMatrix`,
    `setTimeStep`. `StructuralMaterial` struct.
* `CouplerSolidContact`, `CouplerSolidCohesiveContact` →
  * models: `getSolidMechanicsModel` / `getSolidMechanicsModelCohesive`,
    `getContactMechanicsModel`;
  * fields: `getDisplacement`, `getVelocity`, `getAcceleration`,
    `getExternalForce`, `getMass`, `getContactForce`,
    `getStableTimeStep`;
  * `getContactDetector`, `applyBC(...)`, `checkCohesiveStress`
    (cohesive), `assembleMass[Lumped]`, `assembleStiffnessMatrix`,
    `assembleInternalForces`, `isDefaultSolverExplicit`, dumps.
  * `CohesiveContactSolverCallback(solid, contact, method)`.
* `CouplerSolidPhaseField` → `getSolidMechanicsModel`,
  `getPhaseFieldModel`, `solve(solid_id, phase_id)`,
  `computeStrainOnQuadPoints`, `computeDamageOnQuadPoints`,
  `assembleMass[Lumped]`, `assembleStiffnessMatrix`,
  `assembleInternalForces`, `isDefaultSolverExplicit`, `applyBC` (BC mixin
  on `displacement`), dumps.

### 4.6 Input-file sections recognised by this scope

Section and key names are parsed by the R1 parser.

* `model <model_type> [ name = …; dof_manager_type = default|mumps|petsc; default_solver = …; time_step_solver <static|dynamic|dynamic_lumped> [ name = …; sparse_solver_type = mumps|eigen|petsc; non_linear_solver <type> [ threshold, convergence_type, max_iterations, force_linear_recompute, verbose_iteration | PETSc options ]; integration_scheme <type> [ name = <dof>; solution_type = …; alpha; beta ] ] ]`
* `contact_detector [ type = explicit|implicit; master; slave; projection_tolerance; max_iterations; extension_tolerance ]`
* `contact_resolution <penalty_linear|penalty_quadratic> [ name; mu; epsilon_n; epsilon_t; is_master_deformable ]`
  (inside the model section or at top level)
* `model heat_transfer_model [ constitutive_law heat_diffusion [ name; density; capacity; conductivity; conductivity_variation; temperature_reference ] ]`
  (law section format from the constitutive-law handler, R2)
* `model phase_field_model [ phasefield <linear|quadratic> <no_split|volumetric_deviatoric> [ name; l0; gc; E; nu; Plane_Stress; irreversibility_type; irreversibility_tol; recovery_tol ] ]`
  (the energy split is the section *option*; the solid side uses
  `model solid_mechanics_model [ material phasefield <split> [ name; rho; E; nu; Plane_Stress ] ]`,
  R2 — see `test/test_model/test_phase_field_model/material_coupling.dat`)

## 5. Appendix: file coverage

Completeness check generated from `find` over the scope directories at the pinned commit. Totals: 233 source files (`.hh`/`.cc`) and 63 test sources. "R1 scope" means the file belongs to the core/mesh inventory and only its parallel aspects (if any) are covered here.

### 5.1 Source files

| File | Feature IDs / note |
|---|---|
| `src/mesh/element_group.cc` | R1 scope (element groups); parallel: groups are filled during distribution (PART-007) |
| `src/mesh/element_group.hh` | R1 scope |
| `src/mesh/element_group_inline_impl.hh` | R1 scope |
| `src/mesh/element_type_map.cc` | R1 scope |
| `src/mesh/element_type_map.hh` | R1 scope |
| `src/mesh/element_type_map_filter.hh` | R1 scope |
| `src/mesh/element_type_map_tmpl.hh` | R1 scope |
| `src/mesh/group_manager.cc` | R1 scope (incl. `synchronizeGroupNames` used by PART-006, clusters `_gm_clusters`) |
| `src/mesh/group_manager.hh` | R1 scope |
| `src/mesh/group_manager_inline_impl.hh` | R1 scope |
| `src/mesh/mesh.cc` | PART-001 (`distribute`, `distributeImpl`), PART-012 (`initMeshFacets` parallel part, `FacetGlobalConnectivityAccessor`), PART-016 (`computeBoundingBox`, `getGlobalConnectivity`), PART-017 (`updateGlobalData`); rest R1 |
| `src/mesh/mesh.hh` | PART-001, PART-013, PART-016, MPI-025 (declarations); rest R1 |
| `src/mesh/mesh_accessor.hh` | PART-018 |
| `src/mesh/mesh_data.cc` | R1 scope (mesh data); distributed by PART-007/PART-008 |
| `src/mesh/mesh_data.hh` | R1 scope |
| `src/mesh/mesh_data_tmpl.hh` | R1 scope |
| `src/mesh/mesh_events.hh` | R1 scope; `MeshIsDistributedEvent` used by PART-001 / DOF-013 |
| `src/mesh/mesh_filter.hh` | R1 scope |
| `src/mesh/mesh_global_data_updater.hh` | PART-017 |
| `src/mesh/mesh_inline_impl.hh` | MPI-025 (node flag predicates, pranks, global ids), PART-013 (`getConnectivityWithPeriodicity`, periodic slaves); rest R1 |
| `src/mesh/mesh_iterators.hh` | R1 scope |
| `src/mesh/mesh_periodic.cc` | PART-013 |
| `src/mesh/node_group.cc` | R1 scope |
| `src/mesh/node_group.hh` | R1 scope |
| `src/mesh/node_group_inline_impl.hh` | R1 scope |
| `src/mesh_utils/boost_graph_converter.cc` | PART-015 (debug_tools package) |
| `src/mesh_utils/boost_graph_converter.hh` | PART-015 (debug_tools package) |
| `src/mesh_utils/cohesive_element_inserter.cc` | PART-011 (parallel hooks only; insertion algorithm is R2 scope) |
| `src/mesh_utils/cohesive_element_inserter.hh` | PART-011 (R2 owns the class) |
| `src/mesh_utils/cohesive_element_inserter_helper.cc` | PART-011 (global insertion order; rest R2) |
| `src/mesh_utils/cohesive_element_inserter_helper.hh` | PART-011 (R2 owns the class) |
| `src/mesh_utils/cohesive_element_inserter_inline_impl.hh` | PART-011 (`_ce_groups` data accessor) |
| `src/mesh_utils/cohesive_element_inserter_parallel.cc` | PART-011 (includes only, no code: build glue) |
| `src/mesh_utils/global_ids_updater.cc` | PART-010 |
| `src/mesh_utils/global_ids_updater.hh` | PART-010 |
| `src/mesh_utils/global_ids_updater_inline_impl.hh` | PART-010 |
| `src/mesh_utils/mesh_partition.cc` | PART-002 |
| `src/mesh_utils/mesh_partition.hh` | PART-002 |
| `src/mesh_utils/mesh_partition/mesh_partition_mesh_data.cc` | PART-005 |
| `src/mesh_utils/mesh_partition/mesh_partition_mesh_data.hh` | PART-005 |
| `src/mesh_utils/mesh_partition/mesh_partition_scotch.cc` | PART-003, PART-004 |
| `src/mesh_utils/mesh_partition/mesh_partition_scotch.hh` | PART-003, PART-004 |
| `src/mesh_utils/mesh_utils.cc` | PART-009 (`renumberMeshNodes`, `renumberNodesInConnectivity`), PART-012 (`flipFacets`), PART-006 (`fillElementToSubElementsData`); facet building / node-to-element / `purifyMesh` are R1 scope |
| `src/mesh_utils/mesh_utils.hh` | PART-009, PART-012, PART-014 (declarations); remainder R1 |
| `src/mesh_utils/mesh_utils_distribution.cc` | PART-006 |
| `src/mesh_utils/mesh_utils_distribution.hh` | PART-006 |
| `src/mesh_utils/mesh_utils_inline_impl.hh` | R1 scope (`MeshUtils` inline helpers; no parallel behaviour) |
| `src/mesh_utils/mesh_utils_pbc.cc` | PART-014 (dead code) |
| `src/model/common/boundary_condition/boundary_condition.hh` | BC-001 |
| `src/model/common/boundary_condition/boundary_condition_functor.hh` | BC-002, BC-003 |
| `src/model/common/boundary_condition/boundary_condition_functor_inline_impl.hh` | BC-002, BC-003 |
| `src/model/common/boundary_condition/boundary_condition_tmpl.hh` | BC-001, BC-004, BC-005 |
| `src/model/common/dof_manager/dof_manager.cc` | DOF-001 … DOF-013, DOF-017 |
| `src/model/common/dof_manager/dof_manager.hh` | DOF-001 … DOF-013, DOF-016 |
| `src/model/common/dof_manager/dof_manager_default.cc` | DOF-007, DOF-010, DOF-011, DOF-013, DOF-014 |
| `src/model/common/dof_manager/dof_manager_default.hh` | DOF-014 |
| `src/model/common/dof_manager/dof_manager_default_inline_impl.hh` | DOF-014 (empty file: build glue) |
| `src/model/common/dof_manager/dof_manager_inline_impl.hh` | DOF-002, DOF-003, DOF-008, DOF-009, DOF-010, DOF-016 |
| `src/model/common/dof_manager/dof_manager_petsc.cc` | DOF-015 |
| `src/model/common/dof_manager/dof_manager_petsc.hh` | DOF-015 |
| `src/model/common/integration_scheme/generalized_trapezoidal.cc` | TS-003 |
| `src/model/common/integration_scheme/generalized_trapezoidal.hh` | TS-003 |
| `src/model/common/integration_scheme/integration_scheme.cc` | TS-001 |
| `src/model/common/integration_scheme/integration_scheme.hh` | TS-001 |
| `src/model/common/integration_scheme/integration_scheme_1st_order.cc` | TS-002 |
| `src/model/common/integration_scheme/integration_scheme_1st_order.hh` | TS-002 |
| `src/model/common/integration_scheme/integration_scheme_2nd_order.cc` | TS-004 |
| `src/model/common/integration_scheme/integration_scheme_2nd_order.hh` | TS-004 |
| `src/model/common/integration_scheme/newmark-beta.cc` | TS-005, TS-006, TS-007 |
| `src/model/common/integration_scheme/newmark-beta.hh` | TS-005, TS-006, TS-007 |
| `src/model/common/integration_scheme/pseudo_time.cc` | TS-008 |
| `src/model/common/integration_scheme/pseudo_time.hh` | TS-008 |
| `src/model/common/model_solver.cc` | MODEL-004, MODEL-005, MODEL-006 |
| `src/model/common/model_solver.hh` | MODEL-005, MODEL-006, MODEL-011 |
| `src/model/common/non_linear_solver/non_linear_solver.cc` | NLS-001 |
| `src/model/common/non_linear_solver/non_linear_solver.hh` | NLS-001 |
| `src/model/common/non_linear_solver/non_linear_solver_lumped.cc` | NLS-002 |
| `src/model/common/non_linear_solver/non_linear_solver_lumped.hh` | NLS-002 |
| `src/model/common/non_linear_solver/non_linear_solver_newton_raphson.cc` | NLS-003, NLS-004, NLS-005, DOF-017 |
| `src/model/common/non_linear_solver/non_linear_solver_newton_raphson.hh` | NLS-003, NLS-004, NLS-005 |
| `src/model/common/non_linear_solver/non_linear_solver_petsc.cc` | NLS-006 |
| `src/model/common/non_linear_solver/non_linear_solver_petsc.hh` | NLS-006 |
| `src/model/common/non_linear_solver/non_linear_solver_tao.cc` | NLS-007 |
| `src/model/common/non_linear_solver/non_linear_solver_tao.hh` | NLS-007 |
| `src/model/common/solver_callback.cc` | MODEL-007 |
| `src/model/common/solver_callback.hh` | MODEL-007 |
| `src/model/common/time_step_solvers/time_step_solver.cc` | TS-009 |
| `src/model/common/time_step_solvers/time_step_solver.hh` | TS-009 |
| `src/model/common/time_step_solvers/time_step_solver_default.cc` | TS-010 |
| `src/model/common/time_step_solvers/time_step_solver_default.hh` | TS-010 |
| `src/model/common/time_step_solvers/time_step_solver_default_explicit.hh` | TS-012 (dead header, no behaviour) |
| `src/model/contact_mechanics/contact_detector.cc` | CONT-006, CONT-007, CONT-008, CONT-009 |
| `src/model/contact_mechanics/contact_detector.hh` | CONT-006, CONT-007, CONT-008, CONT-009, CONT-011 |
| `src/model/contact_mechanics/contact_detector_inline_impl.hh` | CONT-007, CONT-011, CONT-012 |
| `src/model/contact_mechanics/contact_element.hh` | CONT-017 |
| `src/model/contact_mechanics/contact_mechanics_model.cc` | CONT-001, CONT-002, CONT-003, CONT-004, CONT-005, CONT-018 |
| `src/model/contact_mechanics/contact_mechanics_model.hh` | CONT-001, CONT-018 |
| `src/model/contact_mechanics/geometry_utils.cc` | CONT-012 (empty namespace: build glue) |
| `src/model/contact_mechanics/geometry_utils.hh` | CONT-012 |
| `src/model/contact_mechanics/geometry_utils_inline_impl.hh` | CONT-009, CONT-010, CONT-012 |
| `src/model/contact_mechanics/resolution.cc` | CONT-004, CONT-005, CONT-014 |
| `src/model/contact_mechanics/resolution.hh` | CONT-014 |
| `src/model/contact_mechanics/resolution_utils.cc` | CONT-015 (empty namespace: build glue) |
| `src/model/contact_mechanics/resolution_utils.hh` | CONT-015 (shape matrices `A`, `A_β`) |
| `src/model/contact_mechanics/resolutions/resolution_penalty.cc` | CONT-015 |
| `src/model/contact_mechanics/resolutions/resolution_penalty.hh` | CONT-015 |
| `src/model/contact_mechanics/resolutions/resolution_penalty_quadratic.cc` | CONT-016 |
| `src/model/contact_mechanics/resolutions/resolution_penalty_quadratic.hh` | CONT-016 |
| `src/model/contact_mechanics/surface_selector.cc` | CONT-013 |
| `src/model/contact_mechanics/surface_selector.hh` | CONT-013 |
| `src/model/diffusion_model/diffusion_law.cc` | DIFF-004, DIFF-005, DIFF-008 |
| `src/model/diffusion_model/diffusion_law.hh` | DIFF-008 |
| `src/model/diffusion_model/diffusion_model.cc` | DIFF-001, DIFF-003, DIFF-004, DIFF-005, DIFF-006, DIFF-007, DIFF-010, DIFF-011 |
| `src/model/diffusion_model/diffusion_model.hh` | DIFF-001 |
| `src/model/diffusion_model/heat_diffusion.cc` | DIFF-007, DIFF-009 |
| `src/model/diffusion_model/heat_diffusion.hh` | DIFF-009 |
| `src/model/diffusion_model/heat_transfer_model.hh` | DIFF-002 |
| `src/model/model.cc` | MODEL-001, MODEL-002, MODEL-004, MODEL-008, MODEL-009 |
| `src/model/model.hh` | MODEL-001, MODEL-002, MODEL-008, MODEL-009, MODEL-010 |
| `src/model/model_couplers/cohesive_contact_solvercallback.cc` | CPL-004 |
| `src/model/model_couplers/cohesive_contact_solvercallback.hh` | CPL-004 |
| `src/model/model_couplers/coupler_solid_cohesive_contact.cc` | CPL-003 |
| `src/model/model_couplers/coupler_solid_cohesive_contact.hh` | CPL-003 |
| `src/model/model_couplers/coupler_solid_contact.cc` | CPL-001 |
| `src/model/model_couplers/coupler_solid_contact.hh` | CPL-001 |
| `src/model/model_couplers/coupler_solid_contact_tmpl.hh` | CPL-001, CPL-002 |
| `src/model/model_couplers/coupler_solid_phasefield.cc` | CPL-005, CPL-006, CPL-007 |
| `src/model/model_couplers/coupler_solid_phasefield.hh` | CPL-005 |
| `src/model/model_inline_impl.hh` | MODEL-008, MODEL-010 |
| `src/model/model_options.hh` | MODEL-002 |
| `src/model/phase_field/energy_split.cc` | PF-008 |
| `src/model/phase_field/energy_split.hh` | PF-008 |
| `src/model/phase_field/energy_splits/no_energy_split.hh` | PF-009 |
| `src/model/phase_field/energy_splits/no_energy_split_inline_impl.hh` | PF-009 |
| `src/model/phase_field/energy_splits/volumetric_deviatoric_split.hh` | PF-010 |
| `src/model/phase_field/energy_splits/volumetric_deviatoric_split_inline_impl.hh` | PF-010 |
| `src/model/phase_field/phase_field_element_filter.hh` | PF-011 |
| `src/model/phase_field/phase_field_model.cc` | PF-001, PF-002, PF-003, PF-004, PF-012 |
| `src/model/phase_field/phase_field_model.hh` | PF-001 |
| `src/model/phase_field/phase_field_model_inline_impl.hh` | PF-001 (empty namespace: build glue) |
| `src/model/phase_field/phasefield.cc` | PF-003, PF-004, PF-005 |
| `src/model/phase_field/phasefield.hh` | PF-005 |
| `src/model/phase_field/phasefield_inline_impl.hh` | PF-005 (`_pfm_damage` accessor) |
| `src/model/phase_field/phasefield_selector.hh` | PF-011 |
| `src/model/phase_field/phasefield_selector_tmpl.hh` | PF-011 (empty namespace: build glue) |
| `src/model/phase_field/phasefields/phasefield_linear.cc` | PF-006 |
| `src/model/phase_field/phasefields/phasefield_linear.hh` | PF-006 |
| `src/model/phase_field/phasefields/phasefield_linear_inline_impl.hh` | PF-006 |
| `src/model/phase_field/phasefields/phasefield_quadratic.cc` | PF-007 |
| `src/model/phase_field/phasefields/phasefield_quadratic.hh` | PF-007 |
| `src/model/phase_field/phasefields/phasefield_quadratic_inline_impl.hh` | PF-007 |
| `src/model/structural_mechanics/structural_elements/structural_element_bernoulli_beam_2.hh` | STRUCT-003 |
| `src/model/structural_mechanics/structural_elements/structural_element_bernoulli_beam_3.hh` | STRUCT-004 |
| `src/model/structural_mechanics/structural_elements/structural_element_kirchhoff_shell.hh` | STRUCT-005 |
| `src/model/structural_mechanics/structural_mechanics_model.cc` | STRUCT-001, STRUCT-006, STRUCT-007, STRUCT-008, STRUCT-011, STRUCT-012, STRUCT-013 |
| `src/model/structural_mechanics/structural_mechanics_model.hh` | STRUCT-001, STRUCT-002 |
| `src/model/structural_mechanics/structural_mechanics_model_boundary.cc` | STRUCT-012 (includes only, no code: build glue) |
| `src/model/structural_mechanics/structural_mechanics_model_inline_impl.hh` | STRUCT-002, STRUCT-006, STRUCT-007 |
| `src/model/structural_mechanics/structural_mechanics_model_mass.cc` | STRUCT-009, STRUCT-010 |
| `src/solver/petsc_wrapper.hh` | SOLV-007 (legacy structs, no behaviour) |
| `src/solver/solver_vector.hh` | SPM-009 |
| `src/solver/solver_vector_default.hh` | SPM-010 |
| `src/solver/solver_vector_default_tmpl.hh` | SPM-010, SPM-007 |
| `src/solver/solver_vector_distributed.cc` | SPM-011, SPM-007 |
| `src/solver/solver_vector_distributed.hh` | SPM-011 |
| `src/solver/solver_vector_petsc.cc` | SPM-012, SPM-007 |
| `src/solver/solver_vector_petsc.hh` | SPM-012 |
| `src/solver/sparse_matrix.cc` | SPM-001 |
| `src/solver/sparse_matrix.hh` | SPM-001 |
| `src/solver/sparse_matrix_aij.cc` | SPM-002, SPM-004, SPM-005, SPM-006, SPM-007 |
| `src/solver/sparse_matrix_aij.hh` | SPM-002 |
| `src/solver/sparse_matrix_aij_inline_impl.hh` | SPM-002, SPM-003, SPM-006 |
| `src/solver/sparse_matrix_inline_impl.hh` | SPM-001 |
| `src/solver/sparse_matrix_petsc.cc` | SPM-008, SPM-007 |
| `src/solver/sparse_matrix_petsc.hh` | SPM-008 |
| `src/solver/sparse_solver.cc` | SOLV-001, MPI-023 |
| `src/solver/sparse_solver.hh` | SOLV-001 |
| `src/solver/sparse_solver_eigen.cc` | SOLV-004 |
| `src/solver/sparse_solver_eigen.hh` | SOLV-004 |
| `src/solver/sparse_solver_inline_impl.hh` | SOLV-001 (empty file: build glue) |
| `src/solver/sparse_solver_mumps.cc` | SOLV-002, SOLV-003 |
| `src/solver/sparse_solver_mumps.hh` | SOLV-002, SOLV-003 |
| `src/solver/sparse_solver_petsc.cc` | SOLV-005 |
| `src/solver/sparse_solver_petsc.hh` | SOLV-005 |
| `src/solver/terms_to_assemble.hh` | SPM-013, DOF-009 |
| `src/synchronizer/communication_buffer.hh` | MPI-005 |
| `src/synchronizer/communication_buffer_inline_impl.hh` | MPI-005 |
| `src/synchronizer/communication_descriptor.hh` | MPI-008 |
| `src/synchronizer/communication_descriptor_tmpl.hh` | MPI-008 |
| `src/synchronizer/communication_request.hh` | MPI-004 |
| `src/synchronizer/communication_tag.hh` | MPI-006 |
| `src/synchronizer/communications.hh` | MPI-007 |
| `src/synchronizer/communications_tmpl.hh` | MPI-007 |
| `src/synchronizer/communicator.cc` | MPI-001, MPI-004 |
| `src/synchronizer/communicator.hh` | MPI-001 |
| `src/synchronizer/communicator_dummy_inline_impl.hh` | MPI-003 |
| `src/synchronizer/communicator_event_handler.hh` | MPI-023 |
| `src/synchronizer/communicator_inline_impl.hh` | MPI-001, MPI-022 |
| `src/synchronizer/communicator_mpi_inline_impl.hh` | MPI-002 |
| `src/synchronizer/data_accessor.hh` | MPI-009 |
| `src/synchronizer/data_accessor_tmpl.hh` | MPI-009 |
| `src/synchronizer/dof_synchronizer.cc` | MPI-018 |
| `src/synchronizer/dof_synchronizer.hh` | MPI-018 |
| `src/synchronizer/dof_synchronizer_inline_impl.hh` | MPI-018 |
| `src/synchronizer/element_info_per_processor.cc` | PART-007 |
| `src/synchronizer/element_info_per_processor.hh` | PART-007 |
| `src/synchronizer/element_info_per_processor_tmpl.hh` | PART-007 |
| `src/synchronizer/element_synchronizer.cc` | MPI-016, MPI-014 |
| `src/synchronizer/element_synchronizer.hh` | MPI-016, MPI-014 |
| `src/synchronizer/facet_synchronizer.cc` | MPI-020 |
| `src/synchronizer/facet_synchronizer.hh` | MPI-020 |
| `src/synchronizer/facet_synchronizer_inline_impl.hh` | MPI-020 (empty file: build glue) |
| `src/synchronizer/grid_synchronizer.cc` | MPI-021 |
| `src/synchronizer/grid_synchronizer.hh` | MPI-021 |
| `src/synchronizer/grid_synchronizer_tmpl.hh` | MPI-021 |
| `src/synchronizer/master_element_info_per_processor.cc` | PART-007 |
| `src/synchronizer/mpi_communicator_data.hh` | MPI-002 |
| `src/synchronizer/node_info_per_processor.cc` | PART-008 |
| `src/synchronizer/node_info_per_processor.hh` | PART-008 |
| `src/synchronizer/node_synchronizer.cc` | MPI-017, MPI-014 |
| `src/synchronizer/node_synchronizer.hh` | MPI-017, MPI-014 |
| `src/synchronizer/node_synchronizer_inline_impl.hh` | MPI-017 |
| `src/synchronizer/periodic_node_synchronizer.cc` | MPI-019 |
| `src/synchronizer/periodic_node_synchronizer.hh` | MPI-019 |
| `src/synchronizer/periodic_node_synchronizer_tmpl.hh` | MPI-019 |
| `src/synchronizer/slave_element_info_per_processor.cc` | PART-007 |
| `src/synchronizer/synchronizer.cc` | MPI-010 |
| `src/synchronizer/synchronizer.hh` | MPI-010 |
| `src/synchronizer/synchronizer_impl.hh` | MPI-011, MPI-012, MPI-013 |
| `src/synchronizer/synchronizer_impl_tmpl.hh` | MPI-011, MPI-012, MPI-013, MPI-014 |
| `src/synchronizer/synchronizer_registry.cc` | MPI-015 |
| `src/synchronizer/synchronizer_registry.hh` | MPI-015, MODEL-011 |
| `src/synchronizer/synchronizer_tmpl.hh` | MPI-010 |

### 5.2 Test sources (`test/test_model/test_common`, `test_solver`, `test_synchronizer`, `test_contact_mechanics_model`, `test_phase_field_model`, `test_structural_mechanics_model`, heat-transfer patch tests)

`test/test_model/test_common/test_non_local_toolbox/` belongs to R2 (non-local toolbox). No `test_diffusion_model`, `test_heat_transfer_model` or `test_model_couplers*` directory exists at this commit. Couplers are tested from the contact and phase-field directories.

| Test file | Features | Registration status |
|---|---|---|
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_explicit.cc` | DIFF-012 | registered with nonexistent package `heat_transfer`, so never built |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_fixture.hh` | DIFF-012 | helper |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_implicit.cc` | DIFF-012 | idem |
| `test/test_model/patch_tests/patch_test_linear_heat_transfer_static.cc` | DIFF-012 | idem |
| `test/test_model/test_common/test_dof_manager.cc` | DOF-001…DOF-011, DOF-014/015 | registered (gtest `test_dof_manager`, PARALLEL; mesh `mesh.geo` 3-D o1) |
| `test/test_model/test_common/test_dof_mesh_distribute.cc` | PART-001, DOF-013 | registered (in `test_dof_manager`, package parallel) |
| `test/test_model/test_common/test_model_solver/test_dof_manager_default.cc` | DOF-014 | registered `test_dof_manager_default` (mumps) + `.verified` |
| `test/test_model/test_common/test_model_solver/test_model_solver.cc` | MODEL-005/006, NLS-003, TS-008, SOLV-002/005 | registered `test_model_solver_mumps` / `_petsc` + `.verified` |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic.cc` | TS-005/006, NLS-002/003 | registered `test_model_solver_dynamic_{explicit,implicit,petsc}` + `.verified` |
| `test/test_model/test_common/test_model_solver/test_model_solver_dynamic_petsc.cc` | DOF-015, NLS-003 | **not registered** (the PETSc variant is built from `test_model_solver_dynamic.cc`) |
| `test/test_model/test_common/test_model_solver/test_model_solver_my_model.hh` | MODEL-006 fixture (1-D bar model `MyModel`) | helper |
| `test/test_model/test_contact_mechanics_model/test_coupler/test_contact_coupling.cc` | CPL-001/002 | **not added** to CMake (uses `_implicit_contact`) |
| `test/test_model/test_contact_mechanics_model/test_coupler/test_coupled_stiffness.cc` | CPL-001, CONT-005 | **not added** |
| `test/test_model/test_contact_mechanics_model/test_detector/test_detection.cc` | CONT-006…011 | **not added** (gtest sources only) |
| `test/test_model/test_contact_mechanics_model/test_detector/test_detection_fixture.hh` | CONT-006…011 | helper, not added |
| `test/test_model/test_contact_mechanics_model/test_explicit_dynamic.cc` | CONT-001…015, CPL-001/002 | registered `test_explicit_dynamic` |
| `test/test_model/test_contact_mechanics_model/test_explicit_friction.cc` | CONT-015 (friction), CPL-001 | registered `test_explicit_friction` |
| `test/test_model/test_contact_mechanics_model/test_resolution/test_explicit_resolution.cc` | CONT-015 | **not added** |
| `test/test_model/test_contact_mechanics_model/test_resolution/test_resolution.cc` | CONT-015 | **not registered** |
| `test/test_model/test_contact_mechanics_model/test_selector/test_selection.cc` | CONT-013 | gtest sources registered, no `register_gtest_test`, so never run |
| `test/test_model/test_contact_mechanics_model/test_selector/test_selection_fixture.hh` | CONT-013 | helper |
| `test/test_model/test_phase_field_model/test_multi_material.cc` | PF-001…007, CPL-005 | registered UNSTABLE |
| `test/test_model/test_phase_field_model/test_penalization.cc` | PF-006/007 penalization, CPL-005 | registered |
| `test/test_model/test_phase_field_model/test_phase_field_anisotropic.cc` | PF-010, CPL-005 | registered |
| `test/test_model/test_phase_field_model/test_phase_field_deviatoric_split.cc` | PF-010, CPL-005 | registered |
| `test/test_model/test_phase_field_model/test_phase_field_linear.cc` | PF-006 | registered (phase_field + petsc) |
| `test/test_model/test_phase_field_model/test_phase_field_optimal.cc` | PF-007 | registered |
| `test/test_model/test_phase_field_model/test_phase_field_optimal_linear.cc` | PF-006, NLS-007 | registered (petsc) |
| `test/test_model/test_phase_field_model/test_phase_field_tao.cc` | PF-006, NLS-007 | registered (petsc) |
| `test/test_model/test_phase_field_model/test_phase_solid_coupling.cc` | CPL-005/006/007 | registered |
| `test/test_model/test_phase_field_model/test_phase_solid_explicit.cc` | CPL-005/006/007, TS-006 | registered |
| `test/test_model/test_phase_field_model/test_phasefield_selector.cc` | PF-011 | registered |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_bernoulli_beam_2.cc` | STRUCT-003 | registered (gtest `test_structural_mechanics`) |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_bernoulli_beam_3.cc` | STRUCT-004 | registered |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_bernoulli_beam_dynamics.cc` | STRUCT-008/009 | registered |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_discrete_kirchhoff_triangle_18.cc` | STRUCT-005 | registered |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_fixture.hh` | STRUCT-* fixture | helper |
| `test/test_model/test_structural_mechanics_model/test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5.cc` | STRUCT-005 | **not registered**, stale API |
| `test/test_solver/test_petsc_matrix_apply_boundary.cc` | SPM-008 | **not registered** |
| `test/test_solver/test_petsc_matrix_diagonal.cc` | SPM-008 | **not registered** |
| `test/test_solver/test_petsc_matrix_profile.cc` | SPM-008 | **not registered** (`.verified` present) |
| `test/test_solver/test_petsc_matrix_profile_parallel.cc` | SPM-008 | **not registered** (`.verified` present) |
| `test/test_solver/test_solver_petsc.cc` | SOLV-005 | **not registered** (`.sh` present) |
| `test/test_solver/test_sparse_matrix_assemble.cc` | SPM-002/003, DOF-008 | registered `test_sparse_matrix_assemble` (implicit) |
| `test/test_solver/test_sparse_matrix_product.cc` | SPM-005 | registered `test_sparse_matrix_product` (implicit, `bar.msh`, `.verified`) |
| `test/test_solver/test_sparse_matrix_profile.cc` | SPM-002, SPM-007 | registered `test_sparse_matrix_profile` (implicit) |
| `test/test_solver/test_sparse_matrix_profile_parallel.cc` | SPM-002 parallel | **not registered** |
| `test/test_solver/test_sparse_solver_eigen.cc` | SOLV-004 | registered `test_sparse_solver_eigen` (mumps, PARALLEL; relative tolerance 1e-15) |
| `test/test_solver/test_sparse_solver_mumps.cc` | SOLV-002, SPM-011 | registered `test_sparse_solver_mumps` (mumps, PARALLEL; relative tolerance 1e-15) |
| `test/test_solver/test_sparse_solver_petsc.cc` | SOLV-005, DOF-015 | registered `test_sparse_solver_petsc` (petsc, PARALLEL) |
| `test/test_synchronizer/test_communicator.cc` | MPI-001/002, MPI-022 | registered (gtest `test_synchronizers`, PARALLEL): Bcast, ReceiveAny |
| `test/test_synchronizer/test_data_accessor.hh` | MPI-009 | helper |
| `test/test_synchronizer/test_data_distribution.cc` | PART-006/007/008 | registered (`test_synchronizers`): DataDistribution, DataDistributionTags |
| `test/test_synchronizer/test_dof_data_accessor.hh` | MPI-009, MPI-018 | helper |
| `test/test_synchronizer/test_dof_synchronizer.cc` | MPI-013, MPI-018 | registered `test_dof_synchronizer` (parallel, PARALLEL, `bar.msh`) |
| `test/test_synchronizer/test_dof_synchronizer_communication.cc` | MPI-018 | **not registered** (listed in `AKANTU_PARALLEL_TESTS`) |
| `test/test_synchronizer/test_facet_synchronizer.cc` | MPI-020, PART-012 | registered (`test_synchronizers`, + cohesive_element): SynchroneOnce, Synchrone, Asynchrone |
| `test/test_synchronizer/test_grid_synchronizer.cc` | MPI-021 | **not registered** |
| `test/test_synchronizer/test_grid_synchronizer_check_neighbors.cc` | MPI-021 | **not registered** |
| `test/test_synchronizer/test_grid_tools.hh` | MPI-021 | helper |
| `test/test_synchronizer/test_node_synchronizer.cc` | MPI-017, MPI-013 | registered (`test_synchronizers`): SynchroneOnce, Synchrone, Asynchrone, Gather, Scatter |
| `test/test_synchronizer/test_synchronizer_communication.cc` | MPI-011, MPI-016 | registered (`test_synchronizers`): SynchroneOnce, Synchrone, Asynchrone |
| `test/test_synchronizer/test_synchronizers_fixture.hh` | MPI-* fixture (`cube.geo` 3-D o2) | helper |

## 6. Open questions

Each item names who had to decide (project decision / oracle run / review)
and the evidence to collect, as of the inventory date.

1. **Q1 — Upstream parallel test rank counts.** `cmake/AkantuTestsMacros.cmake:566`
   does `set(N MPIEXEC_MAX_NUMPROCS)` (the variable *name*, not its value), so
   `while(N GREATER 1)` is false and `PARALLEL` tests probably run only with 1
   rank unless `PARALLEL_LEVEL` is given. *Oracle:* check `ctest -N`
   output for `test_synchronizers`, `test_dof_manager` and
   `test_sparse_solver_mumps` rank variants. Our medium tier should run 1/2/4
   ranks regardless.
2. **Q2 — `ModelSolverOptions::update` overwrite.** The defaults overwrite
   user-provided integration schemes in `initFull` (MODEL-004). Parity or fix?
   Proposal: keep the default behaviour, add an explicit Rust API to choose
   schemes, and record an ADR.
3. **Q3 — `hasDOFsDerivatives` off-by-one** (DOF-002). Observable effect: a
   model registering only order-1 derivatives (diffusion) throws "already
   registered" when a second dynamic solver is created
   (`initSolver` → `registerDOFsDerivative`). Parity, or fix?
4. **Q4 — Parallel `SparseMatrixAIJ::matVecMul`** (SPM-005): the reduction
   acts on a zero array. *Oracle:* run solid explicit with *consistent* mass,
   and a Rayleigh-damped implicit case, on 1 vs 2 ranks and compare. If the
   results differ, document the reference bug and decide.
5. **Q5 — `DOFManagerPETSc` numbering** (DOF-015): matrices use Akantu global
   numbers, vectors use AO/PETSc numbers. This is safe for a single DOF id,
   wrong otherwise; the periodic copy bug too. Is the PETSc backend in scope
   for the first Rust milestones? (The project's scope listed PETSc under full
   parity.)
6. **Q6 — Phase field in parallel.** Ghost ∇d is written into not-ghost storage
   (PF-003), and the strain transfer only covers not-ghost elements (CPL-006).
   *Oracle:* run `examples/c++/phase_field_model/phase_field_parallel.cc` with
   1 vs 2 ranks.
7. **Q7 — Parallel contact.** Not supported upstream (§3.9). Must the Rust port
   provide it (the mission says "full parity", which here means
   serial-only)? A project decision was needed before designing the
   contact crate (D-004, D-089).
8. **Q8 — Undefined behaviour in contact friction** (CONT-011, CONT-015):
   uninitialised Eigen vectors / matrices (`slave_normal`, `traction_trial`,
   `increment_xi`, `k_third`). Oracle results for `test_explicit_friction`
   and any implicit friction case may depend on stack garbage. Proposal:
   * Rust zero-initialises;
   * validate μ=0 contact against the oracle bitwise / tight tolerance;
   * validate friction against analytical sliding-block results plus a
     patched oracle (zero-init patch in `oracle/patches/`).
9. **Q9 — `penalty_quadratic` rotational term** uses `n⊗τ` for both terms,
   whereas linear uses `τ⊗n` for the second (CONT-016). Bug or intended?
   *Review* by the scientific reviewer.
10. **Q10 — DKT18 bending stiffness** = `E t⁴/(12(1−ν²))` (extra `t`,
    STRUCT-005). It is invisible in the only test (t=1). Bug? *Review* with
    Batoz.
11. **Q11 — Structural lumped mass** is unusable (`node2 = conn(0)`; 3-D
    overwrites translations). Port the fixed version (and flag the deviation)
    or keep parity?
12. **Q12 — Heat-transfer tests never built upstream** (`PACKAGE heat_transfer`
    does not exist, DIFF-012). There is no CI evidence that the diffusion model
    passes its patch tests at this commit. *Oracle:* build them with the
    package name patched to `diffusion` (patch in `oracle/patches/`) to get a
    baseline.
13. **Q13 — Unregistered / stale upstream tests**: contact
    `test_coupler/*`, `test_detector`, `test_resolution`, `test_selector`
    (never run); `test_grid_synchronizer*`, `test_dof_synchronizer_communication`,
    `test_petsc_matrix_*`, `test_solver_petsc`, `test_sparse_matrix_profile_parallel`,
    `test_model_solver_dynamic_petsc.cc`, the Kirchhoff patch test. Port with
    the same intent (rewritten to the current API), or record as "not ported:
    stale"? Proposal: port the intent (detection, selection, grid sync, PETSc
    profile) as new Rust tests and record the mapping in `docs/FEATURE_PARITY.md`.
14. **Q14 — `is_distributed = true` with 1 rank** (PART-001). It routes serial
    runs that called `distribute()` through `SolverVectorDistributed` /
    `DOFSynchronizer` (identity gather / scatter). Keep the same switch in Rust
    for parity of code paths, or treat np=1 as serial?
15. **Q15 — Direct-solver backend.** MUMPS (FFI, `unsafe`, LGPL-compatible but
    a native dependency) versus a pure-Rust sparse direct solver (`faer`) for
    the default path. Bitwise parity with MUMPS is impossible anyway (ordering
    and BLAS), so a tolerance-based policy is needed (`docs/NUMERICAL_VALIDATION.md`,
    `docs/DEPENDENCIES.md`). Orchestrator ADR.
16. **Q16 — Partitioner.** Bind Scotch (FFI) or implement a Rust partitioner?
    Either way, differential parallel tests should use partitions dumped from
    the oracle and fed through a `MeshPartitionMeshData` equivalent (PART-005).
17. **Q17 — NR `_residual` early return** skips `afterSolveStep` and the
    corrector (NLS-003): solver-callback hooks (e.g. phase-field history
    updates, contact `afterSolveStep`) are not called on an already-converged
    step. Parity?
18. **Q18 — Contact force assignment semantics** (`=` rather than `+=`, ½ for
    deformable masters, CONT-004) and **node-indexed contact stiffness**
    (CONT-005). Proposal: Rust assembles through the DOF manager with `+=`
    (physically correct), and reproduces the reference only in the
    rigid-master serial tests where both coincide. Needs reviewer approval.
19. **Q19 — Unused / unsupported enum values**: `AnalysisMethod::_explicit_contact`
    and `_implicit_contact`, `SolveConvergenceCriteria::_residual_mass_wgh`,
    `TimeStepSolverType` `_dynamic` for phase field, `SolverParallelMethod::_master_slave_distributed`.
    Keep them as enum variants that return `Error::NotImplemented`?
20. **Q20 — `ContactDetector` construction requires a parsed
    `contact_detector` section** (it dereferences the first one even if
    absent). Python users without input files would crash. Keep as an error
    (`Result`) in Rust.
21. **Q21 — Equation-numbering parity at np>1.** Reproducing first-seen local
    node numbering (PART-009), rank-blocked global numbering (DOF-003) and
    triplet insertion order (SPM-002) gives the best chance of matching the
    oracle with distributed MUMPS. Is bitwise parity at np>1 a goal, or only
    tolerance-based parity? Proposal: exact numbering parity (cheap), and
    tolerance-based solution parity.
22. **Q22 — Uninitialised `k_release`** in `GeneralizedTrapezoidal` (TS-003)
    and `NonLinearSolverTAO::max_iterations` registration conflicts: confirm
    no observable effect in the oracle (expected none).
23. **Q23 — Scope boundary.** `src/io/parser/parsable_petsc.hh` (NLS-008) is
    listed in the PETSc package but lives in `io/` (R1). Here it is documented
    behaviourally. Make sure R1 covers its file entry. Likewise the
    structural FE engine pieces (`shape_structural*`, element classes) are R1
    files that STRUCT depends on.

## Errata (independent completeness review, 2026-11-09)

Factual corrections from an independent completeness review of the inventory. Where they conflict with the text above, **the errata win**.

| Finding | Existing row / location | Evidence and correction |
|---|---|---|
| E2 | BC-004 parallel column, models inventory | No communication contradicts the two periodic consistency calls at `src/model/common/boundary_condition/boundary_condition_tmpl.hh:82-85`. `src/synchronizer/periodic_node_synchronizer_tmpl.hh:47-50,69-76` performs remote synchronization/reduction plus local copies. Correct: node loop itself does not communicate, but application may communicate through distributed periodicity. |
| E4 | NLS-003 note, models inventory | residual_mass_wgh does not fully behave as solution. Both take the solution-norm test at `src/model/common/non_linear_solver/non_linear_solver_newton_raphson.cc:170-176`, but only exact _solution refreshes residual on failed iterations (:178-182) and after the loop (:208-210). Correct to solution-norm with a **stale residual path**, not an alias or a mass-weighted norm. |
| E8 | TS-006 porting note, models inventory | Predictor v half from old a contradicts `src/model/common/integration_scheme/newmark-beta.cc:64-67`, with k=h=0 from `newmark-beta.hh:131-132`: v*=v_n+dt*a_n. Corrector adds dt/2*delta_a, yielding v_(n+1)=v_n+dt/2*(a_n+a_(n+1)) when solving for acceleration increment. A half-step predictor plus this increment corrector loses an old-acceleration contribution. |
