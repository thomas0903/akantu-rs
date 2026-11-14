# Inventory R2 — Solid mechanics, materials, cohesive, extra packages

_Source-level feature inventory of the Akantu reference implementation._
_Reference: upstream Akantu @ `35a851780d0b9471d258ed53a003ae98aaa95ea0`; paths are relative to the upstream repository root._
_Taken 2026-11-09. Status: complete first version (see §7 Completeness)._

All paths are relative to the reference root unless stated otherwise. `path:N`
cites a line in the reference checkout. Upstream tests are under `test/`
(`test/test_model/test_solid_mechanics_model` abbreviated **`T_SMM/`**,
`test/test_model/patch_tests` abbreviated **`T_PATCH/`**).

## Summary

* **139 feature rows**: CL 8 · SMM 32 · MAT 27 · NL 9 · EMB 5 · COH 13 · CMAT 6 · XMAT 16 · IGFEM 11 · TASN 12.
* **Material keywords in `src/`**: 13 always-on (`elastic`, `elastic_anisotropic`, `elastic_orthotropic`, `marigo`, `mazars`, `plastic_mazars`, `anisotropic_damage`, `phasefield`, `plastic_linear_isotropic_hardening`, `plastic_drucker_prager`, `sls_deviatoric`, `viscoelastic_maxwell`, `neohookean`), 3 with `damage_non_local`, 1 with `embedded` (`reinforcement`), 6 cohesive laws; weight functions `base_wf`, `damage_wf`, `remove_wf`.
* **392 source files** (`.hh/.cc/.py`, ~69k lines) mapped in §5; constitutive equations in §2; solver/insertion/fragment algorithms in §3; API in §4; 21 open questions in §6.
* **Headline risks** (details in rows and §6): extra-materials, igfem and the embedded model use removed core APIs (likely not compilable — oracle build must confirm); `neohookean` throws NotImplemented; `marigo_non_local` allocates local Marigo; cohesive `delta_max` history is never enabled though read by `cohesive_exponential` / implicit friction; Maxwell internal variable never updated (test UNSTABLE); anisotropic damage returns σ ≡ 0; finite-strain J2 branch overwritten; several uses of uninitialised data and of Eigen's unsorted eigenvalue order; K always declared symmetric while cohesive blocks are unsymmetric.

## 0. Scope, conventions and reading guide

Scope (from the R2 brief):
`src/model/solid_mechanics/**`, `src/model/common/constitutive_laws/**`,
`src/model/common/non_local_toolbox/**`, `src/mesh_utils/cohesive_element_inserter*`
(from the model's perspective), `extra_packages/{extra-materials,igfem,traction-at-split-node-contact}/**`
and the corresponding tests/fixtures.

Conventions used throughout:

* **Voigt order** (`src/common/aka_voigthelper.hh:44-70`): 1D `(11)`; 2D `(11,22,12)`;
  3D `(11,22,33,23,13,12)`. `strainToVoigt` multiplies shear by 2
  (`factors = {1,1,2}` / `{1,1,1,2,2,2}`), `stressToVoigt` does not.
  Tangent moduli are stored per quadrature point as a row-major
  `voigt×voigt` matrix (`Material::getTangentStiffnessVoigtSize(dim) = dim(dim-1)/2+dim`,
  `material.hh:193`).
* `grad_u` is stored per quadrature point as a `dim×dim` matrix
  `(∇u)_{ij} = ∂u_i/∂X_j`; `ε = ½(∇u + ∇uᵀ)` (`material_inline_impl.hh:113`);
  Green–Lagrange `E = ½(∇uᵀ∇u + ∇uᵀ + ∇u)` (`material_inline_impl.hh:129`);
  `F = I + ∇u` (`material_inline_impl.hh:40`).
* **Internal forces sign**: `internal_force = −∫ Bᵀσ` (assembled with factor `−1`,
  `material.cc:508-509`); the residual is `r = f_ext + f_int`
  (`solid_mechanics_model.cc:254-257`).
* **Element-local vs global numbering**: each constitutive law keeps an
  `element_filter` (law-local index → mesh element index); the handler keeps
  `constitutive_law_index` (mesh element → law index, default −1) and
  `constitutive_law_local_numbering` (mesh element → law-local index).
* `_pat_parsable` = readable from `.dat`; `_pat_modifiable` = settable at run time
  (`set()`), `_pat_parsmod` = both; `_pat_readable` = query only; `_pat_internal` = hidden.
* Material registration: `instantiateMaterial<Mat>("name")` (`material.hh:621-631`)
  registers an allocator in `MaterialFactory` keyed by the `.dat` section keyword;
  it dispatches on spatial dimension 1/2/3 at run time (all three dims are
  always instantiated). The optional section *option* (`material <type> <option> [ … ]`)
  is passed as the allocator's second argument (used by `anisotropic_damage`,
  `phasefield`, `reinforcement`, cohesive none).
* "**Package / build option**" uses the CMake package names of
  `packages/*.cmake` (`solid_mechanics`, `cohesive_element`, `damage_non_local`
  → defines `AKANTU_DAMAGE_NON_LOCAL`, `embedded` (requires CGAL),
  `phase_field`, `implicit`, `parallel`) and the extra-package names
  (`extra_materials`, `igfem`, `traction_at_split_node_contact`).
* "**Risk**" notes flag reference behaviour that looks like a bug or is
  numerically delicate. Per `docs/NUMERICAL_VALIDATION.md` policy, the port must
  reproduce reference behaviour by default and record deviations; those notes
  are inputs for that process, not decisions.

ID prefixes: `CL` constitutive-law infrastructure, `SMM` solid mechanics model
(and generic `Material` machinery), `MAT` bulk material laws, `NL` non-local
toolbox, `EMB` embedded reinforcement, `COH` cohesive model, `CMAT` cohesive
laws, `XMAT` extra-materials package, `IGFEM` igfem package, `TASN`
traction-at-split-node-contact package.

## 1. Main feature table

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| CL-001 | Internal-field registry of a constitutive law | `src/model/common/constitutive_laws/constitutive_law.hh:51-172`, `constitutive_law_tmpl.hh:46-213` (`ConstitutiveLawInternalHandler::registerInternal/unregisterInternal/resizeInternals/savePreviousState/restorePreviousState/getInternal/getSharedPtrInternal/isInternal/getArray/removeIntegrationPoints`) | Owns `map<ID, shared_ptr<InternalFieldBase>>`; `registerInternal(id, nb_comp[, fe_engine_id[, filter]])` creates and initialises an `InternalField` (default FE engine = law's); `savePreviousState`/`restorePreviousState` copy current↔previous only for fields with history; `getArray(id,type,ghost)` throws if missing type. | core | — | indirectly every material test | none directly | Map is keyed by `getRegisterID()` (the plain id); `getInternal` error text uses `id:` prefix. Rust: typed registry keyed by string; preserve "history only if `initializeHistory()` called". |
| CL-002 | `InternalField<T>` per-quadrature-point storage | `internal_field.hh:38-268`, `internal_field_tmpl.hh:31-308` | `ElementTypeMapArray<T>` restricted to the law's element filter, spatial dim and element kind; `nb_component` per quad point; `default_value` used to fill new entries; optional `previous_values` (created by `initializeHistory()`, id `previous_<id>`); `resize()` keeps old values and fills new tail with default (or random draw), increments a per-type `release` counter; `reset()` refills everything; `removeIntegrationPoints(new_numbering)` compacts (`memcpy` of `nb_quad*nb_comp` blocks) when elements are removed. Parameter adaptor: a `Real` `.dat` value sets the default and resets the field (`internal_field_tmpl.hh:291-303`). | core | — | indirect | sizes of ghost types follow the filter | Previous values resized together with current. `release` counters used by dumpers/models to detect changes. Port: SoA arrays per (type, ghost) with explicit history buffer. |
| CL-003 | `RandomInternalField` | `random_internal_field.hh:36-98`, `random_internal_field_tmpl.hh:34-104` | Internal field whose new values are drawn from a `RandomParameter<T>` (base value + distribution, e.g. `uniform [a,b]`, `weibull`); `.dat` syntax `Yd = 94e3 uniform [0, 9.4e3]` (`T_SMM/test_materials/test_material_non_local/material_damage_non_local.dat`). `operator Real()` returns base value. | core | RNG from `aka_random_generator` (seeded globally) | `test_material_damage_non_local` (Yd random), `test_local_material` | random values are **drawn per process**; Marigo re-synchronises `Yd` on ghosts with tag `_clh_init_cl` (`material_marigo_inline_impl.hh:81-118`) | Bit-reproducing random draws requires porting Akantu's generator and seeding/ordering exactly (values drawn in `resize()` order per type/ghost). Flag for numerical validation. |
| CL-004 | Generic `ConstitutiveLaw<Handler>` base | `constitutive_law.hh:176-335`, `constitutive_law_tmpl.hh:216-622` | Parsable (`name` param), `initConstitutiveLaw()` = resize internals + `updateInternalParameters()`; `addElement(s)` (push in filter, update handler maps, resize); `removeElements` (renumber filter + compact internals); mesh-event hooks `onElementsAdded` (resize) / `onElementsRemoved` (renumber filter, update handler arrays); `flattenInternal` (scatter law-local quad data into a mesh-wide `ElementTypeMapArray`) / `inflateInternal` (gather back); `getInternalDataPerElem`; local↔global element conversion; `getParam`/deprecated `setParam`. | core | — | indirect | `packInternalFieldHelper`/`unpackInternalFieldHelper` (`constitutive_law_tmpl.hh:413-431`) used for ghost exchange | `inflateInternal` asserts `field.size()==nb_elem*nb_quad` on the whole map (looks wrong but only debug). `removeElements` loops over *all* filter elements with `std::find` → O(n·m). |
| CL-005 | `ConstitutiveLawsHandler<Law, Model>` (law registry & assignment) | `constitutive_laws_handler.hh:49-347`, `constitutive_laws_handler_tmpl.hh:50-680` | Mixin base of `SolidMechanicsModel` (and phase-field/diffusion-like models). `initFullImpl`: if a file was parsed, instantiate one law per parser subsection of `parser_type` (`material` for SMM) under the model section (or global), then `initConstitutiveLaws()` = assign laws to elements via selector → `initConstitutiveLaw()` on each → sync `_clh_init_cl` → `NonLocalManager::initialize()` if any non-local law. Law id = `<model_id>:<index>:<type>`. `registerNewConstitutiveLaw(name,type,option)` for programmatic creation. `reassignConstitutiveLaw()` re-runs the selector and moves elements. `getConstitutiveLaw(index/name/element)`, `getConstitutiveLawIndex(name)`, `getInternalIndexFromID`. Dumper helpers `isInternal`, `getInternalDataPerElem`, `flattenInternal` (cached per (name,kind)), `flattenAllRegisteredInternals`, `inflateInternal`. | core | — | `test_material_selector`, `test_solid_mechanics_model_reassign_material` (parallel+implicit), `test_multi_material_elastic` | element→law index synchronised with tag `_constitutive_law_id`: ghosts take the owner's index on unpack (`constitutive_laws_handler_tmpl.hh:506-531`); other tags split by law and forwarded to each law's data accessor. Non-local manager created only `#ifdef AKANTU_DAMAGE_NON_LOCAL` (`:240-251`). | `onElementsAdded` re-initialises index arrays with −1 default **only for new entries** and assigns laws only to new elements of model dimension (`:534-564`); comment notes a parallel bug when facets are added between ctor and `initFull`. Assignment iterates `_not_ghost` only, ghosts get indices by communication. |
| CL-006 | Constitutive-law (material) selectors | `constitutive_law_selector.hh:45-150`, `constitutive_law_selector_tmpl.hh:7-27`; aliases `src/model/solid_mechanics/material_selector.hh:33-40` (`DefaultMaterialSelector`, `ElementDataMaterialSelector<T>`, `MeshDataMaterialSelector<T>`) | Chain-of-responsibility: `operator()(element)` returns law index; base returns fallback selector result or `fallback_value` (default 0). `DefaultConstitutiveLawSelector` returns the current index if ≠ −1 else fallback. `ElementDataConstitutiveLawSelector<T>`: for `T=std::string` maps the element data string to a law **by name** (`getConstitutiveLawIndex`), for integer `T` returns the raw value; any exception → fallback. `MeshDataConstitutiveLawSelector<T>(name)` uses `mesh.getData<T>(name)` (e.g. `"physical_names"`, `"tag_0"`). | core | — | `test_material_selector` (`material_selector.dat`, `material_selector.msh`), cohesive tests | selectors only evaluated on non-ghost elements | `first_index` constructor argument is stored but **unused** (`constitutive_law_selector.hh:141`): integer data is not offset. Python exposes `MeshDataMaterialSelectorString` etc. (R4). |
| CL-007 | Non-local constitutive-law interface | `constitutive_law_non_local_interface.hh:41-113`, `constitutive_law_non_local_interface_tmpl.hh:29-122`; `src/model/solid_mechanics/materials/material_non_local.hh:31-44` (`MaterialNonLocalInterface`, `MaterialNonLocal<dim,Parent>`) | Mixin adding: `initConstitutiveLawNonLocal()` = `registerNeighborhood()` (neighborhood id = weight-function id = law `name`) + `registerNonLocalVariables()` (pure); `insertIntegrationPointsInNeighborhoods` inserts every quad point (dim-matching regular types of the filter) with `global_num = element*nb_quad + q`; `updateNonLocalInternals` copies the averaged flattened field back into the law internal. `MaterialNonLocalInterface` adds pure `computeNonLocalStresses(ghost)`. | core (interface); used only by `damage_non_local` laws | — | `test_material_non_local` (`custom_non_local_test_material.*`, `material.dat`), `test_material_damage_non_local` | integration points of ghost elements inserted after grid synchronisation (see NL-001) | `getNeighborhoodName()` returns law name: the `.dat` must contain `non_local <material name> <wf_type> [ … ]`. |
| CL-008 | Handler-level data accessor & sync tags | `constitutive_laws_handler_tmpl.hh:471-531` | `getNbData/packData/unpackData` for `Element` lists: `_constitutive_law_id` packs indices; any other tag is split per law (`splitByConstitutiveLaw`) and delegated. | core | MPI (via synchronizers) | parallel tests (cohesive parallel, reassign) | tags: `_constitutive_law_id`, `_clh_init_cl`, plus law-specific ones | Unpack of `_constitutive_law_id` only adds ghosts whose index is still −1. |
| SMM-001 | `SolidMechanicsModel` construction | `src/model/solid_mechanics/solid_mechanics_model.hh:45-477`, `solid_mechanics_model.cc:61-88` | Inherits `ConstitutiveLawsHandler<Material, Model>`, `BoundaryCondition<SMM>`, `EventHandlerManager<SolidMechanicsModelEventHandler>`. Ctor: init DOF manager (default or given), register FE engine `"SolidMechanicsFEEngine"` = `FEEngineTemplate<IntegratorGauss, ShapeLagrange>` on the model dimension, register Paraview dumper `"solid_mechanics_model"` dumping regular elements of model dim, register element synchronizer for tags `_smm_mass, _smm_stress, _smm_gradu, _smm_density, _for_dump` when the mesh is distributed, set parser type `_material`. Model type `_solid_mechanics_model`. | solid_mechanics | Eigen, IOHelper (dumper) | all SMM tests | synchronizer registration only if `mesh.isDistributed()` | `dim` defaults to mesh dimension (`_all_dimensions`). |
| SMM-002 | Analysis methods → solver configuration | `solid_mechanics_model.cc:115-194`; `SolidMechanicsModelOptions` (in `src/model/model_options.hh`, R3 scope) | `initFull(_analysis_method=…)`. Mapping (`getDefaultSolverID`): `_explicit_lumped_mass` → solver `"explicit_lumped"`, `TimeStepSolverType::_dynamic_lumped` (non-linear solver `_lumped`, scheme `_central_difference`, solution type `_acceleration`); `_explicit_consistent_mass` → `"explicit"`, `_dynamic` (NR solver, central difference, `_acceleration`); `_static` → `"static"`, `_static` (NR, `_pseudo_time`, `_not_defined`); `_implicit_dynamic` → `"implicit"`, `_dynamic` (NR, `_trapezoidal_rule_2`, `_displacement`). Default solver type `_dynamic_lumped`. `initFullImpl` also calls `initBC(*this,"displacement",u,Δu,f_ext)`. | solid_mechanics (+implicit for NR/sparse solve) | sparse solver backend (Eigen/MUMPS/PETSc, R3) | `test_solid_mechanics_model_dynamics` (explicit & implicit, tol 2e-3), patch tests explicit/implicit/static, energies tests | — | Exact behaviour of central difference / trapezoidal schemes and NR lives in `src/model/common/integration_scheme`, `non_linear_solver` (R3); see §3 for the composed algorithm. |
| SMM-003 | Solver initialisation: nodal fields & DOFs | `solid_mechanics_model.cc:197-242` | Allocates `displacement, previous_displacement, displacement_increment, internal_force, external_force, blocked_dofs, current_position` (dim components); `current_position := nodes`; registers DOF `"displacement"` (nodal) with blocked dofs, increment, previous. For dynamic types also `velocity`, `acceleration` registered as 1st/2nd derivatives. | solid_mechanics | — | all | — | `allocNodalField` keeps existing arrays (idempotent). |
| SMM-004 | Residual assembly | `solid_mechanics_model.cc:245-283` | `assembleResidual()`: if `external_work_release ≠ displacement_release` accumulate `external_work += getExternalWorkIncrement()`; `assembleInternalForces()`; residual += `f_ext` and += `f_int`. Split variant `assembleResidual("external" / "internal")`, `canSplitResidual()=true`; unknown part → `SolverCallbackResidualPartUnknown`. | solid_mechanics | — | patch tests (force balance tol 1e-8 relative), energies | — | External work accumulated lazily in residual calls (once per displacement release). |
| SMM-005 | Internal force assembly | `solid_mechanics_model.cc:342-382`; `material.cc:98-154,456-510` | Zero `f_int`; every material `computeAllStresses(_not_ghost)` (∇u = gradient of `u` on quad points minus `eigen_grad_u`, then `computeStress`); if non-local: `NonLocalManager::computeAllNonLocalContribution()`; async sync `_smm_stress`; `assembleInternalForces(_not_ghost)` per material; wait; `assembleInternalForces(_ghost)`; `mesh.makeConsistentForPeriodicity(f_int)`. Small-strain material contribution: `f_int −= Σ_e ∫ Bᵀσ` via `computeBtD` + `integrate` + `assembleElementalArrayLocalArray(…, −1)`. | solid_mechanics | — | all | `_smm_stress` exchanges ghost σ (and S, ∇u when finite deformation); ghost elements contribute to local nodes (overlapping assembly) | Ghost stresses are *communicated*, not recomputed. Periodic BCs: slave/master consistency. |
| SMM-006 | Stiffness matrix assembly & matrix type | `solid_mechanics_model.cc:286-310,385-408`; `material.cc:225-312` | `assembleStiffnessMatrix(need_to_reassemble)`: create `"K"` if missing; reassemble if any material `hasMatrixChanged("K")` (stiffness changed or finite deformation); zero K; each material assembles `∫ Bᵀ D B` (`computeBtDB` with Voigt order `4`, tangent from `computeTangentModuli`) on `_not_ghost` only → `assembleElementalMatricesToMatrix("K","displacement",…,_symmetric)`. `getMatrixType("K")` always returns `_symmetric` (the computed max over materials is discarded — `:292-300`); `"C"` → `_mt_not_defined`; `assembleMatrix("M")` → consistent mass; `assembleLumpedMatrix("M")` → lumped mass. | solid_mechanics, implicit | sparse matrix/solver (R3) | patch tests implicit/static, `test_solid_mechanics_model_material_eigenstrain`, implicit cohesive tests | ghost elements not assembled (owner assembles) | Risk: unsymmetric tangents (Drucker–Prager closest point, friction) are still stored as symmetric (reference bug, keep). Elemental matrices are always assembled as `_symmetric`. |
| SMM-007 | Lumped mass | `solid_mechanics_model_mass.cc:31-118,141-154`; `src/fe_engine/fe_engine_template_tmpl_field.hh:76-215` (R1) | Reassembled if `need_to_reassemble_lumped_mass` or any material `hasMatrixChanged("M")` (phase-field mass degradation). ρ per quad point from `Material::getRho` (functor `ComputeRhoFunctor`). Async `_smm_density`, assemble local then ghost elements, periodic consistency, `assembleToLumpedMatrix("displacement", mass, "M")`, debug-only zeroing of abs(m) < ε / NaN with warning, sync `_smm_mass`. Lumping: **row-sum** `M̃_i = ∫ρφ_i` for linear/serendipity-free types; **diagonal scaling** with fixed nodal factors for `_triangle_6` (1/12 corner, 1/4 mid), `_tetrahedron_10` (1/32, 7/48), `_quadrangle_8` (3/76, 16/76), `_hexahedron_20` (7/248, 16/248), `_pentahedron_15` (51/2358 corners, 192/2358 triangle-edge mids, 300/2358 quad-edge mids) times `∫ρ dV`. | solid_mechanics | — | `T_PATCH/test_lumped_mass.cc` (`data/material_lumped.dat`), dynamics & energy tests | ghost elements assembled into local nodes after density exchange; `_smm_mass` sync of nodal masses | Ghost-element contributions to the lumped mass are included (unlike K). Lumped mass is also stored in `model.getMass()` nodal array (dim components). |
| SMM-008 | Consistent mass matrix | `solid_mechanics_model_mass.cc:121-170` | Creates `"M"` (type from `getMatrixType("M")` = `_symmetric`), assembled only once (`need_to_reassemble_mass`), `fem.assembleFieldMatrix(ρ, "M", "displacement", …)` on `_not_ghost`. | solid_mechanics, implicit | — | implicit dynamics tests, `KineticEnergyImplicit` | owner-only assembly | Not reassembled when material densities change (only flagged on node add). |
| SMM-009 | Stable time step | `solid_mechanics_model.cc:435-485` | `Δt = min_e h_e / c_e`, `h_e = FEEngine::getElementInradius(X_current, type)` using current positions (`X+u`), `c_e = material.getCelerity(elem)` (default P-wave speed). Global `allReduce(min)`. | solid_mechanics | — | `test_solid_mechanics_model_dynamics`, many examples | MPI min-reduction | Risks: element coordinates always extracted for `_not_ghost`; the `Element` passed to `getCelerity` carries the **law-local** index (`elem.element = mat_loc_num`) — harmless for homogeneous laws. No safety factor applied (user multiplies). |
| SMM-010 | Kinetic energy | `solid_mechanics_model.cc:488-569` | If lumped `M` exists: `E_k = ½ Σ_{local/master, non-periodic-slave nodes} Σ_i m_i v_i²` (components with m ≤ ε skipped); else if consistent `M`: `½ Σ_local v·(M v)`; global sum. Per element: `½∫ρ v_h·v_h` with ρ per quad point. | solid_mechanics | — | `test_solid_mechanics_model_kinetic_energy` (`…_kinetic_energy_material.dat`, explicit & implicit, tol 1e-8) | `isLocalOrMasterNode`, allReduce sum | Calls `assembleLumpedMatrix("M")` (may reassemble). |
| SMM-011 | External work (3 variants) | `solid_mechanics_model.cc:572-638` | `"external work"`: per step power: Σ over counted nodes of `(blocked ? −f_int·v : f_ext·v)` (`v` = velocity, or Δu for `_static`), × Δt if not static, allReduce. `"external work new"`: accumulated `external_work` (sum of `(f_ext − f_int)·Δu` at each new displacement release, `getExternalWorkIncrement`), allReduce. | solid_mechanics | — | `test_solid_mechanics_model_work_quasistatic` (tol 1e-12 / 1e-2), `test_solid_mechanics_model_work_dynamics` (tol 5e-2) | allReduce; masters only | Sign convention of `f_int` (negative of ∫Bᵀσ) matters. |
| SMM-012 | Energy dispatch | `solid_mechanics_model.cc:642-701`; `material.cc:599-660` | `getEnergy(id)`: `"kinetic"`, `"external work"`, `"external work new"` handled by model; otherwise Σ over materials `material.getEnergy(id)` (`"potential"` default; `"dissipated"`, `"plastic"`, `"work"`, `"dissipated_sls_deviatoric"`, cohesive `"reversible"`, `"dissipated"`, `"cohesive contact"` …) then allReduce. `getEnergy(id, element)` (kinetic per element or material per element with law-local index); `getEnergy(id, group)` sums over an element group. | solid_mechanics | — | energies tests, cohesive energy tests | allReduce | `Material::getPotentialEnergy(element)` integrates an **uninitialised** vector (never calls `computePotentialEnergyByElement`) — `material.cc:627-639` (reference bug). |
| SMM-013 | Current position | `solid_mechanics_model.cc:411-430` | `current_position = nodes + u`, recomputed lazily when `displacement_release` changed. | solid_mechanics | — | stable time step tests | — | — |
| SMM-014 | Solve-step hooks & release counters | `solid_mechanics_model.cc:320-335` | `predictor()`/`corrector()` increment `displacement_release`; `beforeSolveStep()` → each material `beforeSolveStep()` (= `savePreviousState()`); `afterSolveStep(converged)` → each material (`restorePreviousState()` if not converged, else `updateEnergies(type)` for all types). | solid_mechanics | — | plasticity/viscoelastic/damage tests | — | History semantics: previous values = start-of-step state. |
| SMM-015 | Node add/remove events | `solid_mechanics_model.cc:704-785` | `onNodesAdded`: resize all nodal arrays with zeros (blocked=false), bump displacement release, forward to materials, flag mass reassembly. `onNodesRemoved`: compact nodal arrays (`mesh.removeNodesFromArray`). | solid_mechanics | — | cohesive insertion tests (nodes duplicated) | — | `current_position` not compacted on removal (only resized on add). |
| SMM-016 | Model data accessors | `solid_mechanics_model.cc:849-1026` | Element tags: `_smm_mass` (pack mass), `_smm_for_gradu` (displacement), `_for_dump` (u,v,a,f_int,f_ext), `_smm_boundary` (f_ext,v,blocked) — **packed but never unpacked** in `unpackData(Element)` (only the CLH part is unpacked) — plus CLH tags. DOF tags: `_smm_uv` (u,v), `_smm_res` (f_int), `_smm_mass`, `_for_dump`. | solid_mechanics, parallel | MPI | parallel tests | see behaviour | Risk: element-tag pack/unpack asymmetry (`:923-927`) — would corrupt buffers if those tags were ever synchronised through element synchronizers; in practice nodal data goes through DOF tags. |
| SMM-017 | Dumper fields | `solid_mechanics_model_io.cc:43-216`; dumper classes in `src/io/dumper/` (R1) | Nodal Real: `displacement, mass, velocity, acceleration, external_force, internal_force, increment` (padding to 3 comps if requested); `"force"`/`"residual"` raise rename errors; nodal bool `blocked_dofs`. Elemental: `partitions`, `material_index`, any material internal (flattened, homogenised per element), computed: `strain` / `Green strain` (from `grad_u`), `principal strain` / `principal Green strain`, `Von Mises stress` (from `stress`); 2D padding of `stress`/`strain` to 3×3 (`src/io/dumper/dumper_material_padders.hh`: `StressPadder<2>` sets σ_zz = ν(σ_xx+σ_yy) for plane strain (reads material params `Plane_Stress`, `nu`), `StrainPadder<2>` sets ε_zz = ν/(ν−1)(ε_xx+ε_yy) for plane stress). `dump(...)` sends `BeforeDumpEvent`, `onDump()` (re-flatten registered internals), then `mesh.dump`. `setTimeStep` also sets dumper time step. | solid_mechanics | IOHelper/Paraview | most tests create `paraview/` dirs | `_for_dump` sync tags exist; `partitions` field | Dumper output is not a numerical oracle target (format parity only). |
| SMM-018 | Model events | `solid_mechanics_model_event_handler.hh:29-111` | Events `BeforeSolveStepEvent(method)`, `AfterSolveStepEvent(method)`, `BeforeDumpEvent`, `BeginningOfDamageIterationEvent`, `AfterDamageEvent` → handler callbacks `onBeginningSolveStep`, `onEndSolveStep`, `onDump`, `onDamageIteration`, `onDamageUpdate`. Materials register themselves as handlers (`material.cc:53`): `onDamageIteration` = save previous state, `onDamageUpdate` = `updateEnergiesAfterDamage`, `onDump` = Cauchy stress for finite deformation. | solid_mechanics | — | extra-materials damage-iterative tests (send damage events) | — | Model itself only sends `BeforeDumpEvent`; others are sent by user code / extra packages. |
| SMM-019 | Eigen gradient of displacement | `material.cc:30-88,811-824`; `solid_mechanics_model.cc:1029-1040` | Every material has internal `eigen_grad_u` (dim²) subtracted from ∇u before `computeStress`; `.dat` parameter `eigen_grad_u` (matrix, `_pat_parsable`) copied into the internal by `updateInternalParameters()`; `model.applyEigenGradU(M, material_name, ghost)` sets it on all quad points of the named material. | solid_mechanics | — | `test_solid_mechanics_model_material_eigenstrain` (`cube_3d_tet_4.msh`, `material_elastic_plane_strain.dat`) | — | `updateInternalParameters` only touches `_not_ghost` arrays. |
| SMM-020 | Material access & selection API | `solid_mechanics_model.hh:331-416`, `solid_mechanics_model.cc:1043-1047` | `getMaterials()`, `getMaterial(index/name/element)`, `getMaterialIndex(name)`, `getNbMaterials()`, `reassignMaterial()`, `registerNewMaterial(name,type,option)`, `getMaterialByElement()`, `getMaterialLocalNumbering()`, `getMaterialSelector()`, `setMaterialSelector(shared_ptr)`, `getFEEngineBoundary(name)`. | solid_mechanics | — | `test_material_selector`, reassign test, python tests | — | Thin wrappers over CL-005/CL-006. |
| SMM-021 | Boundary conditions integration | `solid_mechanics_model.hh:47,480-483`, `solid_mechanics_model.cc:124-125` | `BoundaryCondition<SMM>` mixin bound to `displacement`/`increment`/`external_force`; aliases `BC::Neumann::FromStress = FromHigherDim`, `FromTraction = FromSameDim`. Dirichlet functors (`FixedValue`, `IncrementValue`, `Flags`…) live in `src/model/common/boundary_condition*` (R3). | solid_mechanics | — | patch tests (`applyBC(BC::Neumann::FromHigherDim(σ), group)`) | — | — |
| SMM-022 | `Material` base class: generic small-strain pipeline | `material.hh:62-561`, `material.cc:30-154,456-510`, `material_inline_impl.hh` | Registers internals `stress, eigen_grad_u, grad_u` (dim² each) and `potential_energy` (1); params `rho` (Real, default 0, parsable+modifiable), `finite_deformation` (bool, false), `inelastic_deformation` (internal), `eigen_grad_u` (matrix). `initMaterial()`: if finite deformation register `piola_kirchhoff_2` (with history) and `green_strain`; enable history of `stress` and `grad_u`; resize internals. `computeAllStresses`, `assembleInternalForces`, `assembleStiffnessMatrix`, `getPushWaveSpeed`/`getShearWaveSpeed`/`getCelerity` (virtual, default `AKANTU_TO_IMPLEMENT`), `getRho` (constant ρ per quad), `hasMatrixChanged`, `getMatrixType`, `getTangentType` (default `_mt_not_defined` = invalid for implicit), `setToSteadyState`. | solid_mechanics | Eigen | all material tests | — | Virtual hierarchy + CRTP-ish `getArguments` named-tuple zips; Rust port should use traits with per-quad kernels. |
| SMM-023 | Finite-deformation total-Lagrangian assembly (Bathe 1975) | `material.cc:157-191,315-453,512-584`, `material_inline_impl.hh:54-169` | When `finite_deformation=true`: internal force `r = ∫ (B_L0 + B_L1(∇u))ᵀ S_voigt` using 2nd PK `S` (`transferBMatrixToSymVoigtBMatrix` + `transferBMatrixToBL2`); stiffness `K = K_NL + K_L` with `K_NL = ∫ B_NLᵀ Ŝ B_NL` (`setCauchyStressMatrix`: block-diagonal `dim²×dim²` replication of S) and `K_L = ∫ (B_L0+B_L1)ᵀ D (B_L0+B_L1)`. Cauchy stress `σ = J⁻¹ F S Fᵀ` (`J<ε` → 0) computed on dump (`computeAllCauchyStresses`/`StoCauchy`). `hasMatrixChanged("K")` always true. | solid_mechanics | — | patch tests `ExplicitFiniteDeformation`, `ImplicitFiniteDeformation`, `StaticFiniteDeformation` (`material_check_stress_plane_*_finite_deformation.dat`), `test_solid_mechanics_model_material_large_rotation` (UNSTABLE), `test_finite_deformation` (gtest) | `_smm_stress` sends S, ∇u, σ | Risk: `assembleInternalForcesFiniteDeformation` allocates `shapesd_filtered` with `nb_element` rows (`material.cc:547-551`) — relies on `filterElementalData` resizing. Elastic finite deformation = St-Venant–Kirchhoff (MAT-005). |
| SMM-024 | Stress interpolation (for extrinsic cohesive) | `material.cc:663-734` | `initElementalFieldInterpolation(coords)` precomputes per-element interpolation matrices from quad points to arbitrary points (`FEEngine::initElementalFieldInterpolationFromIntegrationPoints`); `interpolateStress(result)`; `interpolateStressOnFacets(result, by_elem_result)` stores, for each facet of each element, the stress at facet quad points into slot 0/1 depending on whether the element is the facet's first neighbour. | solid_mechanics | — | extrinsic cohesive tests | ghost types handled by callers | Used by `SolidMechanicsModelCohesive::interpolateStress` (COH). |
| SMM-025 | Material energies (base) | `material.cc:587-660` | `getPotentialEnergy()` = Σ_types ∫ `potential_energy` after `computePotentialEnergy(type)` (pure virtual-ish); `getEnergy("potential")`; per element variant (buggy, see SMM-012). | solid_mechanics | — | `test_solid_mechanics_model_linear_elastic_potential_energy` (tol 1e-8) | owner-only integration | — |
| SMM-026 | Material history / solve-step behaviour | `material.cc:737-767` | `beforeSolveStep` → `savePreviousState`; `afterSolveStep(false)` → `restorePreviousState`; `afterSolveStep(true)` → `updateEnergies(type)` for all types (any dim/kind). | solid_mechanics | — | plastic/visco/damage tests | — | — |
| SMM-027 | Material parallel data | `material_inline_impl.hh:172-218` | `_smm_stress`: (finite def: S, ∇u,) σ; `_smm_gradu`: ∇u. | parallel | MPI | parallel cohesive tests | sizes from `model.getNbIntegrationPoints(elements)` | — |
| SMM-028 | Static tensor helpers | `material.hh:289-394`, `material_inline_impl.hh:33-169` | `gradUToF`, `gradUToEpsilon`, `gradUToE`, `rightCauchy (FᵀF)`, `leftCauchy`, `StoCauchy(F,S[,C33])` (J = det F·√C33), `computeDeviatoric<dim>` (trace/dim), `stressToVonMises` (deviator with trace/3 regardless of dim, √(3/2 s:s)), `stressToVoigt`, `strainToVoigt`, `voigtToStress`, `setCauchyStressMatrix`. | solid_mechanics | — | indirect | — | Risk: `leftCauchy<dim>(F)` calls `rightCauchy` (returns FᵀF, `material_inline_impl.hh:100-106`). |
| SMM-029 | `extrapolateInternal` | `material.cc:771-808` | "Extrapolation" of an internal to points: copies the first quad-point value with non-zero norm to all output entries (warning printed). | solid_mechanics | — | igfem / FE2 users | — | Placeholder semantics; reproduce as-is. |
| SMM-030 | Quadrature-loop macros | `material.hh:573-616` | `MATERIAL_STRESS_QUADRATURE_POINT_LOOP_BEGIN/END`, `MATERIAL_TANGENT_QUADRATURE_POINT_LOOP_BEGIN/END` (used by extra packages and user materials). | solid_mechanics | — | `test_local_material` | — | Macro API is part of the "user material" extension surface. |
| SMM-031 | Material factory & registration | `material.hh:41-42,621-631`, `material.cc:827-829` | `MaterialFactory = Factory<Material, ID, Int dim, const ID & option, SMM &, const ID & id>`; `Material::getFactory()`; `instantiateMaterial<Mat>(keyword)`; custom allocators for `anisotropic_damage`, `phasefield`, `reinforcement`. User code (and Python) can register new allocators. | solid_mechanics | — | `test_local_material` (user material), python custom materials (R4) | — | Registration happens in static initialisers: Rust needs an explicit registry. |
| SMM-032 | Material registry list (keywords) | see MAT/CMAT/XMAT rows | Keywords registered in `src`: `elastic`, `elastic_anisotropic`, `elastic_orthotropic`, `marigo`, `mazars`, `plastic_mazars`, `anisotropic_damage` (options `mazars` (default) / `mazars-drucker-prager`), `phasefield` (options `no_split` (default) / `volumetric_deviatoric`), `plastic_linear_isotropic_hardening`, `plastic_drucker_prager`, `sls_deviatoric`, `viscoelastic_maxwell`, `neohookean`; with `damage_non_local`: `marigo_non_local` (**allocates local `MaterialMarigo`**), `mazars_non_local`, `plastic_mazars_non_local`; with `embedded`: `reinforcement` (option `elastic`); with `cohesive_element`: `cohesive_linear`, `cohesive_bilinear`, `cohesive_exponential`, `cohesive_linear_fatigue`, `cohesive_linear_friction`, `cohesive_linear_uncoupled`. | various | — | — | — | Risk: `material_marigo_non_local.cc:83-84` registers `MaterialMarigo` (local) under `marigo_non_local`; `MaterialMarigoNonLocal` is compiled but unreachable from input files. |
| MAT-001 | `elastic` — 1D linear elastic | `materials/material_elastic.{hh,cc}`, `material_elastic_inline_impl.hh:52-63` (`MaterialElastic<1>`) | σ₁₁ = E·∂u/∂x + σ_th; tangent `[E]`; `ν` forced to 0 in `initMaterial`; P-wave speed √(E/ρ), shear speed throws. | solid_mechanics | — | `test_elastic_materials` (gtest `TestElasticMaterialFixture/{ComputeStress,EnergyDensity,ComputeTangentModuli,ComputeCelerity}` for `MaterialElastic<1>`), patch tests `_segment_2/_segment_3` | — | See spec §2.1. |
| MAT-002 | `elastic` — 2D plane strain | same, `MaterialElastic<2>` with `Plane_Stress=false` (default) | σ = λ tr(ε) I + 2μ ε + σ_th I (λ, μ from E, ν 3D formulas); tangent 3×3 Voigt `[[λ+2μ, λ, 0],[λ, λ+2μ, 0],[0,0,μ]]`. | solid_mechanics | — | `test_elastic_materials` (`MaterialElastic<2>`), `T_PATCH/patch_test_linear_elastic_{explicit,implicit}.cc` with `data/material_check_stress_plane_strain.dat` (result tol 1e-9, gradient 1e-8, dofs 1e-10, force 1e-8), `test_multi_material_elastic`, `test_material_selector` | — | Out-of-plane σ₃₃ is **not** stored; dumper `StressPadder<2>` reconstructs it for output. |
| MAT-003 | `elastic` — 2D plane stress | same, `Plane_Stress = true` (`plane_stress_toolbox_tmpl.hh:43-47`), `material_elastic.cc:65-78` | λ replaced by λ* = νE/((1+ν)(1−ν)); same σ/tangent formulas; thermal stress uses plane-stress variant (MAT-006). `third_axis_deformation` internal registered (default 1) but not computed for `elastic`. | solid_mechanics | — | `T_PATCH` with `data/material_check_stress_plane_stress.dat`, `test_elastic_materials` | — | `Plane_Stress` param is `_pat_parsmod`; changing it at run time triggers `updateInternalParameters` (recomputes λ). |
| MAT-004 | `elastic` — 3D isotropic | `MaterialElastic<3>` | σ = λ tr(ε) I + 2μ ε + σ_th I; 6×6 Voigt tangent (shear entries μ). Potential energy density ½ σ:∇u. Readable params `lambda`, `mu`, `kapa` (=λ+2μ/3). | solid_mechanics | — | `test_elastic_materials`, patch tests 3D types, energies (`…potential_energy_material.dat`), dynamics, `test_interpolate_stress` (`material_interpolate.dat`, `interpolation.geo`) | — | Stiffness only reassembled when `was_stiffness_assembled` is false (reset by any parameter update). |
| MAT-005 | `elastic` + `finite_deformation = true` (St-Venant–Kirchhoff) | `material_elastic.cc:81-102,137-160` | Stress kernel evaluated with Green–Lagrange `E` in place of `∇u`, output is 2nd PK `S = λ tr(E) I + 2μ E`; tangent = small-strain D; assembled with TL formulation (SMM-023); Cauchy stress computed on dump. | solid_mechanics | — | `T_PATCH` `*FiniteDeformation` cases (`material_check_stress_plane_{strain,stress}_finite_deformation.dat`), `test_finite_deformation` (`material_finite_deformation.dat`, `1_tetrahedron.msh`, `cube.geo`), `test_solid_mechanics_model_material_large_rotation` (UNSTABLE, `material_elastic.dat`) | `_smm_stress` sends S, ∇u, σ | Thermal stress still added isotropically to S. |
| MAT-006 | Thermal contribution (`MaterialThermal`) | `materials/material_thermal.{hh,cc}` | Base of `elastic`. Internals `delta_T` (1, modifiable, uniform default from `.dat` `delta_T`), `sigma_th` (1, history), `epsilon_th` (1, history). 3D & 2D plane strain: ε_th = αΔT(1+ν), σ_th = −E αΔT/(1−2ν); 2D plane stress: ε_th = αΔT, σ_th = −E/(1−ν)·ε_th; 1D: ε_th = αΔT, σ_th = −E ε_th. `sigma_th` is added to the diagonal of σ by `elastic` and descendants that call `MaterialElastic::computeStressOnQuad` with thermal args. Params `E` (0), `nu` (0.5), `alpha` (0), `delta_T`. | solid_mechanics | — | `test_material_thermal` (gtest `ThermalComputeStress`, `material_thermal.dat`) | — | Risk: `epsilon_th` includes (1+ν) in 3D (plane-strain expression) — only used by phase-field split. Thermal stress not recomputed by `marigo`, `mazars` (they skip `MaterialThermal::computeStress`), so σ_th keeps its last value (initially 0). |
| MAT-007 | `elastic_anisotropic` — general linear anisotropic | `materials/material_elastic_linear_anisotropic{.hh,.cc,_inline_impl.hh}` | Params `C11…C66` (upper triangle in Voigt order, material frame; symmetric fill), `n1..n_dim` (material axes, normalised; 3-D must be right-handed within 8ε, 2-D only orthogonality is checked — UD-107). `C = rotator·C'·revrotor` (full dim²×dim² rotation, `rotateCprime`), σ_voigt = C·ε_voigt(with ×2 shears); tangent = C; potential ½σ:∇u (asserts symmetric); celerity √(eig₀(C)/ρ) where eig₀ is the **first unsorted eigenvalue** returned by Eigen's `EigenSolver`. No thermal, no finite deformation. 1D compiles (C11 only). | solid_mechanics | Eigen `EigenSolver` | `test_elastic_materials` (`MaterialElasticLinearAnisotropic<2,3>`), `T_PATCH/patch_test_linear_anisotropic_{explicit,implicit}.cc` (`data/material_anisotropic_{1,2,3}.dat`, tol 1e-11 explicit / 1e-9 static), python `test_patch_linear_anisotropic_*.py` | — | Risk: celerity depends on Eigen eigenvalue ordering (`material_elastic_linear_anisotropic.cc:228`); to reproduce, port must compute the same eigenvalue Eigen would list first, or deviate (document). Only deprecated `computePotentialEnergyByElement(type,index,…)` overridden. |
| MAT-008 | `elastic_orthotropic` | `materials/material_elastic_orthotropic.{hh,cc}` | Params `E1,E2,nu12,G12` (+ `E3,nu13,nu23,G13,G23` in 3D), axes `n1..`. Builds C' from engineering constants (3D: Γ = 1/(1−ν12ν21−ν23ν32−ν31ν13−2ν21ν32ν13), ν21 = ν12E2/E1 …), rotates like MAT-007. **2D is plane stress** (Γ = 1/(1−ν12ν21), C'₁₁ = E1Γ, C'₂₂ = E2Γ, C'₁₂ = E1ν21Γ, C'₃₃ = G12). Errors in 1D. Asserts no finite deformation. | solid_mechanics | — | `test_material_orthotropic` (`orthotropic.dat`, `square.geo`), `test_elastic_materials` (`MaterialElasticOrthotropic<2,3>`) | — | `updateInternalParameters` does not call `Material::updateInternalParameters` (eigen_grad_u from `.dat` ignored) nor reset `was_stiffness_assembled` (`material_elastic_orthotropic.cc:74-128`). |
| MAT-009 | Plane-stress toolbox | `materials/plane_stress_toolbox{.hh,_tmpl.hh}` | Mixin. In 2D adds param `Plane_Stress` (bool, parsmod) and internal `third_axis_deformation` (C33, default 1); its `computeStress` calls parent then `computeThirdAxisDeformation` if plane stress; `computeAllCauchyStresses` uses `computeCauchyStressPlaneStress` in plane stress (finite def only). In 1D/3D provides `C33 = 1` broadcast argument. | solid_mechanics | — | via elastic/neohookean tests | — | Generic (1D/3D) template has no `computeStress`, so explicit calls `PlaneStressToolbox<dim>::computeStress` resolve to `Material::computeStress` = NotImplemented (affects MAT-025). |
| MAT-010 | `MaterialDamage<dim, Parent=elastic>` base | `materials/material_damage/material_damage{.hh,_tmpl.hh}` | Internals `damage` (1, history), `damage dissipated energy` (1), `integral of sigma` (1). Tangent = parent tangent × (1−d). `updateEnergies`: potential energy recomputed; ∫σ:d∇u by trapezoid `½(σ+σ_prev):(∇u−∇u_prev)` accumulated; dissipated = ∫σd∇u − e_pot. `getEnergy("dissipated")`. `hasStiffnessMatrixChanged()` = true. | solid_mechanics | — | via derived | — | Energy uses `grad_u` (not ε) in double contraction (same for symmetric σ). |
| MAT-011 | `marigo` damage | `material_damage/material_marigo{.hh,.cc,_inline_impl.hh}` | Energy-release-rate damage: Y = ½ σ_el:ε (×(1−d) if `damage_in_y`, capped at Yc = ½E ε_c² if `epsilon_c≠0`); if Y − Yd − Sd·d_prev > 0 then d = (Y−Yd)/Sd; d = min(max(d, d_prev), 1); σ = (1−d)σ_el. Params `Yd` (random internal), `Sd` (5000), `epsilon_c` (0), `damage_in_y` (false), internal `Yc limit`. Internal `Y`. | solid_mechanics | — | `test_material_damage_non_local` (uses keyword `marigo_non_local`, which allocates **this** local law), marigo commented out in `test_damage_materials` | `Yd` (random) synchronised on ghosts at init with `_clh_init_cl` | Sd default in header 5000 = registered default. |
| MAT-012 | `MaterialMarigoNonLocal` (`marigo_non_local`) | `material_damage/material_marigo_non_local.{hh,cc}` | Non-local Marigo: local pass computes Y; averaged `Y non local` replaces Y; then damage & stress update. **Unreachable**: the keyword `marigo_non_local` instantiates local `MaterialMarigo` (`material_marigo_non_local.cc:83-84`). | damage_non_local | — | `test_material_damage_non_local` (`material_damage_non_local.dat`, `mesh_section_gap.msh`) — exercises local Marigo + non-local manager with `remove_wf` | non-local averaging (NL-*) | Decide in validation: reproduce the registration bug (default) and keep the non-local class available programmatically. |
| MAT-013 | `mazars` damage | `material_damage/material_mazars{.hh,.cc,_inline_impl.hh}` | Elastic σ_el; equivalent strain ε̂ = √Σ⟨ε_i⟩₊² over principal strains of the 3×3 strain (out-of-plane strain 0 in 2D); if ε̂ > K0: d_t, d_c exponential laws, weights α_t, α_c from positive principal "stresses" with exponent β; d = min(max(d_new, d), 1) (max with **current** d); σ = (1−d)σ_el. Params `K0` (random internal), `At` (0.8), `Bt` (12000), `Ac` (1.4), `Bc` (1900), `beta` (1.06). | solid_mechanics | Eigen `EigenSolver` (principal strains) | `test_damage_materials` gtest `ComputeStress` vs `py_mazars.py` (packages core + python_interface), `test_material_mazars` (UNSTABLE, `material_mazars.dat`) | — | `Ehat` is a single scalar member broadcast to all quad points (overwritten per point). See §2.6. |
| MAT-014 | `mazars_non_local` | `material_damage/material_mazars_non_local{.hh,.cc,_tmpl.hh}`, `material_damage_non_local.hh` | Non-local Mazars: param `average_on_damage` (bool, default **false** → average ε̂; true → average d). Internals `Ehat`, `non_local_variable`. Local pass: elastic stress + ε̂ (+ local damage if averaging d); non-local pass: replace the averaged variable and run damage+stress update. | damage_non_local | — | none upstream | non-local averaging | Member default `damage_in_compute_stress=true` is overridden to false by `registerParam` default. |
| MAT-015 | `plastic_mazars` (von Mises plasticity + Mazars damage) | `material_damage/material_von_mises_mazars{.hh,.cc}` = `MaterialMazars<dim, MaterialLinearIsotropicHardening>`; dead file `material_von_mises_mazars_inline_impl.hh` | Parent stress = J2 linear-hardening return mapping (MAT-020); damage driven by ε̂ of `grad_u − inelastic_strain` (temporarily subtracted, re-added); σ = (1−d)σ_plastic. | solid_mechanics | — | none upstream | — | `material_von_mises_mazars_inline_impl.hh` is an obsolete, unincluded implementation (UInt-templated) — no behaviour. |
| MAT-016 | `plastic_mazars_non_local` | `material_damage/material_von_mises_mazars_non_local.{hh,cc}` | Non-local version of MAT-015 (same mechanism as MAT-014). | damage_non_local | — | none | non-local | — |
| MAT-017 | `anisotropic_damage` (options `mazars`, `mazars-drucker-prager`) | `material_damage/material_anisotropic_damage{.hh,.cc,_tmpl.hh}` | Tensorial damage D (dim² internal, no history), `elastic_stress`, `equivalent_strain`, `trace_damage` (history). Equivalent strain functors `EquivalentStrainMazars` (√(Σ⟨ε_i⟩₊), **sum not sum of squares**) and `…DruckerPrager` (+k·tr ε, param `k`); threshold `DamageThresholdTan` (params `a`=2.93e-4, `A`=5e3, `K0`=5e-5; `DamageThresholdLinear` compiled but unregistered). Damage increment D += Δλ⟨ε⟩₊, σ via `damageStress` (√(1−D) σ̃ √(1−D) split). Param `Dc` (0.99). | solid_mechanics | Eigen eigen-decomposition | **none** upstream | — | High risk / experimental: the elastic kernel writes `sigma` but `damageStress` reads the never-written `elastic_stress` internal (⇒ σ ≡ 0 as coded); writes the new trace into the *previous* history slot; `tensorPlusOp` leaves off-diagonal of `A_diag` uninitialised (UB); "broken" branch mutates material-wide E, ν. See §2.8. |
| MAT-018 | `phasefield` material (options `no_split`, `volumetric_deviatoric`) | `material_damage/material_phasefield{.hh,.cc,_inline_impl.hh}`; energy splits `src/model/phase_field/energy_split*.{hh,cc}`, `energy_splits/*` (phase_field package, R3) | Damage field `d` is written by the phase-field coupler (R3). g(d) = (1−d)² + η; σ = g·σ⁺ + σ⁻ from split (no split: σ⁺ = λ tr(ε−ε_th I) I + 2μ(ε−ε_th I), σ⁻=0; vol-dev: κ⟨tr⟩₊ I + 2μ dev / κ⟨tr⟩₋ I with deviator using trace/3). Tangent from split (volumetric part degraded only if tr ε > 0). Optional mass degradation `degrade_mass` (ρ·g(d), capped at ρ). Params `eta` (0), `is_isotropic` (false, unused in stress), `degrade_mass` (false). | phase_field | — | `test/test_model/test_phase_field_model/*` (`material_*.dat`: coupling, deviatoric, hybrid, optimal, optimal_linear, multiple (UNSTABLE), penalization, linear, linear_tao) | `_smm_density` with `degrade_mass` exchanges damage & ∇u | Uses `epsilon_th` of MaterialThermal (includes (1+ν) factor). |
| MAT-019 | `MaterialPlastic` base | `material_plastic/material_plastic{.hh,.cc,_inline_impl.hh}` | Params `h` (0), `sigma_y` (0). Internals `iso_hardening` (1, history), `inelastic_strain` (dim², history), `plastic_energy`, `d_plastic_energy`. Helper `computeStressAndInelasticStrainOnQuad`: σ = σ_prev + C:(Δ∇u − Δε_p) (no thermal term), ε_p = ε_p,prev + Δε_p. Potential energy from elastic strain ∇u − ε_p. `updateEnergies`: w_p = ½(σ+σ_prev):Δε_p, plastic energy += w_p. `getEnergy("plastic")`. | solid_mechanics | — | via derived | — | `material_plastic_inline_impl.hh` is empty. |
| MAT-020 | `plastic_linear_isotropic_hardening` (small strain J2) | `material_plastic/material_linear_isotropic_hardening{.hh,.cc,_inline_impl.hh}` | Incremental radial return: trial σ_tr = σ_prev + C:Δε + (σ_th,prev − σ_th)I; s = dev₃ₓ₃(σ_tr) (embedded in 3×3, trace/3), σ_eq = √(3/2 s:s); yield if σ_eq − r − σ_y > 0 (**r = current iteration's hardening**); Δp = (σ_eq − σ_y − r_prev)/(3μ+h); r = r_prev + hΔp; Δε_p = 3/2 Δp s/σ_eq (dim block). Tangent = **elastic** (not consistent). | solid_mechanics | — | `test_plastic_materials` (gtest `ComputeStress`, 3D only; energy/tangent/celerity DISABLED), `test_material_standard_linear_isotropic_hardening` (`…linear_isotropic_hardening.dat`, `.geo`, compares with `test_material_standard_linear_isotropic_hardening.verified`) | — | Thermal increment sign `prev − current` (`material_linear_isotropic_hardening_inline_impl.hh:33-36`) and thermal term absent from the final stress — reproduce. |
| MAT-021 | `plastic_linear_isotropic_hardening` with `finite_deformation` | `material_linear_isotropic_hardening.cc:36-53`, `_inline_impl.hh:88-186` | Finite-strain variant (Green strains, trial S, Cauchy-based von Mises with Newton loop ≤50 iterations, tol `Math::getTolerance()`, exit if Δdp < 1e-5) — **then immediately overwritten** by a second call of the small-strain kernel with the original args (`material_linear_isotropic_hardening.cc:47-52`). | solid_mechanics | — | none | — | Reference bug: effective behaviour = small-strain J2 on ∇u with S storage; record and reproduce (or deviate with justification). |
| MAT-022 | `plastic_drucker_prager` | `material_plastic/material_drucker_prager{.hh,.cc,_inline_impl.hh}` | Params `phi` (deg, 0), `fc` (1), `radial_return` (true); α = 6 sinφ/(3−sinφ), c = fc(1−sinφ)/(2cosφ), k = 6c cosφ/(3−sinφ). Yield f = √(3J2) + α tr σ − k (deviator with trace/dim). Trial σ = σ_prev + C:Δε + Δσ_th I (sign current−prev). If f>0: closest-point projection in Voigt (Newton, ≤100 it., tol 1e-10, Hessian of √(3J2)), apex projection if tr σ ≥ k/α, radial-return mode sets α=0 and k=abs(α trσ − k); Δε_p from `Ce·(σ_tr−σ)`, shear components halved. Tangent = elastic (inherited). Finite deformation → NotImplemented. | solid_mechanics | Eigen dense inverse | **none** upstream | — | Risk: apex loop divides by `σ_dev.inverse()` (singular for many states); scaling matrix doubles *normal* components. See §2.12. |
| MAT-023 | `sls_deviatoric` (standard linear solid, deviatoric viscosity) | `material_viscoelastic/material_standard_linear_solid_deviatoric.{hh,cc}` | Params `Eta` (1), `Ev` (1), readable `Einf` = E − Ev. τ = η/Ev; per step: s = 2μ dev(ε); h ← e^{−Δt/τ} h + e^{−Δt/2τ}(s − s_prev); σ = γ∞ κ tr(ε) I + γ∞ s + γ_v h, γ∞ = E∞/E, γ_v = Ev/E. Internals `stress_dev`, `history_integral`, `dissipated_energy`. `setToSteadyState` initialises s, h=0. Energies `dissipated` / `dissipated_sls_deviatoric`. | solid_mechanics | — | `test_material_standard_linear_solid_deviatoric_relaxation` and `…_relaxation_tension` (`material_standard_linear_solid_deviatoric_relaxation.dat`, `.geo`) | — | Dissipated-energy update uses a **matrix product then `.sum()`** instead of a double contraction (`…deviatoric.cc:138`). Uses Δt from model each `computeStress` call (stateful per call). |
| MAT-024 | `viscoelastic_maxwell` (generalised Maxwell) | `material_viscoelastic/material_viscoelastic_maxwell.{hh,cc}` | Params `Einf` (1), vectors `Eta`, `Ev` (equal sizes), readable `previous_dt`. E := Einf + ΣEv. σ = Einf·C·ε + Σ_k [E_k^eff C(ε−ε_prev) + e^{−Δt/λ_k} σ_v,k] + σ_th I (λ_k = η_k/E_k, E_k^eff = (1−e^{−Δt/λ})E_k λ/Δt or E_k if e^{−Δt/λ}=1), C = normalised (E=1) isotropic Voigt stiffness (2D = plane strain). Tangent = E_eff·C. Internal variables `sigma_v`, `epsilon_v` updated after converged steps (`afterSolveStep`). Energies `potential`, `dissipated`, `work`. | solid_mechanics | — | `test_material_viscoelasti_maxwell_relaxation` (**UNSTABLE**, `material_viscoelastic_maxwell.dat`), example `examples/c++/solid_mechanics_model/new_material/viscoelastic_maxwell` | — | Risk: `updateIntVarOnQuad` never writes the new σ_v (computed into `voigt_epsilon_v`, then overwritten; `…maxwell.cc:251-262`), so σ_v stays at its initial value — matches the test being UNSTABLE. `hasStiffnessMatrixChanged` depends on Δt change. |
| MAT-025 | `neohookean` (compressible Neo-Hookean, 1D/3D and 2D plane strain) | `material_finite_deformation/material_neohookean{.hh,.cc,_inline_impl.hh}` | Always `finite_deformation=true`. S = μ I + (λ ln J − μ) C⁻¹ (J = det F·√C33); tangent (Belytschko) C_ijkl = λ C⁻¹_ij C⁻¹_kl + (μ − λ ln J)(C⁻¹_ik C⁻¹_jl + C⁻¹_il C⁻¹_kj); energy ψ = λ(ln J)² + ½μ(−ln J + ½(tr C − dim)) (as coded). Own params `E`, `nu` (0.5), readable `lambda`, `mu`, `kapa`. | solid_mechanics | — | `test_finite_def_materials` — **all cases DISABLED** | — | Risk: `computeStress` first calls `PlaneStressToolbox<dim>::computeStress`, which resolves to `Material::computeStress` → `AKANTU_TO_IMPLEMENT` exception (1D/3D) or via `ParentMaterial::computeStress` (2D) — material appears non-functional at this commit. Energy formula differs from the textbook ½λ(lnJ)² − μ lnJ + ½μ(trC−3). Confirm with the oracle. |
| MAT-026 | `neohookean` 2D plane stress (C33 solve) | `material_neohookean.cc:77-115`, `_inline_impl.hh:51-91` | Plane stress: C33 solved per quad by Newton–Raphson (`Math::NewtonRaphson`, tol 1e-5, ≤100 it.) on f(x) = λ/2 (ln x + ln(C11C22 − C12²)) + μ(x−1); Cauchy stress σ = J⁻¹ F S Fᵀ with J = det F √C33. | solid_mechanics | — | none (disabled) | — | Same NotImplemented risk as MAT-025. |
| MAT-027 | Non-local damage material templates | `materials/material_non_local.hh`, `material_damage/material_damage_non_local.hh:32-66` | `MaterialDamageNonLocal<dim, Local>`: forwards `getArguments` to the local law and implements `computeNonLocalStresses(ghost)` by calling `computeNonLocalStress(type, ghost)` for every non-empty dim-type of the filter. | damage_non_local (headers in solid_mechanics) | — | `test_material_non_local` (custom `CustomNonLocalTestMaterial`) | non-local averaging on ghosts handled by NL | — |
| NL-001 | `NonLocalManager` | `src/model/common/non_local_toolbox/non_local_manager{.hh,.cc,_inline_impl.hh}`, `non_local_manager_callback.hh` | Created by CL-005 when any law implements `ConstitutiveLawNonLocalInterface` (only `#ifdef AKANTU_DAMAGE_NON_LOCAL`). Parses all `non_local <id> <wf_type> [ … ]` sections (parser type `_non_local`) into a map. `registerNeighborhood(neigh, wf_id)` creates neighborhood via `NonLocalNeighborhoodFactory` (key = wf type) and parses its section. `initialize()`: insert local quad points, register mesh event handler, create grid synchronizers (all-gather of neighborhood names; dummy synchronizers for neighborhoods unknown locally), insert ghost quads, build pair lists, **erase irrelevant ghost elements from the mesh**, callback `initializeNonLocal` (sync law ids), set jacobians, allocate non-local variables, compute weights. `computeAllNonLocalContribution()`: flatten local variables (callback `updateLocalInternal`), zero volumes, async `_mnl_for_average`, average (`_not_ghost` incl. weight update), wait, average ghost pairs, copy back (`updateNonLocalInternal`), callback `computeNonLocalContribution(_not_ghost)` (= each material's `computeNonLocalStresses`), `++compute_stress_calls`. Element add/remove events resize/compact `volumes` and `integration_points_positions`. | core (active only with damage_non_local) | MPI (grid synchronizers) | `test_material_non_local`, `test_material_damage_non_local`; extra-materials non-local tests (XMAT) | Grid synchroniser (`GridSynchronizer`, R3) exchanges ghost elements within radius; tags `_mnl_for_average`, `_mnl_weight`; dummy synchronizers keep collective calls matched | `registerNonLocalVariable` looks up by local name but inserts by non-local name (re-creates entry each call). `cleanupExtraGhostElements` mutates the mesh (`eraseElements`). |
| NL-002 | Neighborhoods & pair lists | `neighborhood_base{.hh,.cc,_inline_impl.hh}`, `non_local_neighborhood_base{.hh,.cc}` | `SpatialGrid<IntegrationPoint>` with cell spacing 1.2·R centred on mesh bbox centre; `updatePairList`: for every non-ghost q1 and every q2 in neighbouring cells with ‖x1−x2‖ ≤ R + `Math::getTolerance()`: keep (q1,q2) if q2 ghost, or q2 non-ghost with `q1.global_num ≤ q2.global_num` (half list incl. self-pairs). Param `radius` (default 100). `savePairs`, `saveNeighborCoords` debug writers (`.msh` output when serial). Relevant ghosts = ghosts appearing in pairs. `onElementsRemoved` renumbers ghost quad points. | core | — | non-local tests | ghost pairs list `pair_list[_ghost]` | Risk: the half-list test compares `global_num` across different element types (could duplicate/miss cross-type pairs in mixed meshes). Ghost iteration `break`s at the first ghost in a cell (relies on insertion order). |
| NL-003 | Non-local weighting & averaging | `non_local_neighborhood{.hh,_tmpl.hh}` | Weights per pair: w₁ = (w_q2·J_q2)·W(r,q1,q2), w₂ = (w_q1·J_q1)·W(r,q2,q1) (0 for ghost q2 or self pair); volumes V_q = Σ w; normalise w₁/=V_q1, w₂/=V_q2. Average: ā_q1 += w₁ a_q2 (+ symmetric for non-ghost). Only variables registered in the neighborhood are averaged. `updateWeights()` recomputes when `update_rate ≠ 0` and `compute_stress_calls % update_rate == 0` (after `_mnl_weight` sync). `saveWeights`. | core | — | non-local tests | `_mnl_weight` sync (weight-function data), `_mnl_for_average` (local variables) | Jacobians are integration weight × det J from the FE engine integrator. |
| NL-004 | `base_wf` weight function | `base_weight_function{.hh,.cc,_inline_impl.hh}` | W(r) = (1 − r²/R²)² for r ≤ R else 0. Param `update_rate` (Int 1, parsmod) in sub-section `weight_function weight_parameter [ … ]`. Registered as neighborhood type `base_wf` (`INSTANTIATE_NL_NEIGHBORHOOD`). | core | — | `test_material_non_local` (`material.dat`: `non_local test base_wf [ radius = 0.4 ]`) | — | Neighborhood factory signature `(name, manager, quad_positions, id)`. |
| NL-005 | `damage_wf` (`DamagedWeightFunction`) | `materials/weight_functions/damaged_weight_function{.hh,.cc,_inline_impl.hh}` | Uses manager weight-function internal `"damage"` (flattened from laws). With R2 := R², Radius_init = R⁴, Radius_t = (R²(1−D))² (floored to 0.001·R⁴ if < tol); b = ⌊e⌋ + (⌊e⌋ mod 2), e = 2 ln 0.51 / ln(1 − 0.49 Radius_t/Radius_init); W = max(0, 1 − r²/Radius_init)^b. | damage_non_local | — | none | flattened damage needs ghost values (computed locally from ghosts' damage) | Risk: dimensional inconsistency (r² vs R⁴) — reproduce as coded. |
| NL-006 | `remove_wf` (`RemoveDamagedWeightFunction`) | `materials/weight_functions/remove_damaged_weight_function{.hh,.cc,_inline_impl.hh}` | W = 1 if q1 == q2; else if D(q2) < damage_limit·(1 − tol) → (1 − r²/R²)₊², else 0. Param `damage_limit` (1.0). Exchanges damage of ghosts on `_mnl_weight`. | damage_non_local | — | `test_material_damage_non_local` (`damage_limit = 0.99`, `update_rate = 10`, `radius = 0.01`) | `_mnl_weight` packs flattened damage | — |
| NL-007 | `RemoveDamagedWithDamageRateWeightFunction` | `materials/weight_functions/remove_damaged_with_damage_rate_weight_function{.hh,_inline_impl.hh}` | W = 1 for same global_num; else (1 − (r²/R²)¹)² if D_rate(q2) < `damage_limit` (internal `"damage-rate"`). **Not registered** in the neighborhood factory (no `.cc`). | damage_non_local | — | none | — | Header-only, unreachable from input files. |
| NL-008 | `StressBasedWeightFunction` (Giry et al. 2011) | `materials/weight_functions/stress_based_weight_function{.hh,.cc,_inline_impl.hh}` | Stub: all computations commented out, `operator()` returns 0; not registered. | damage_non_local | — | none | — | Port as stub or omit with parity note (no behaviour). |
| NL-009 | `NeighborhoodMaxCriterion` | `non_local_toolbox/neighborhoods_criterion_evaluation/neighborhood_max_criterion{.hh,.cc,_inline_impl.hh}` | Finds quad points whose criterion (flattened internal named `criterion_id`) is maximal within radius (`findMaxQuads`); own grid sync tag `_nh_criterion`; ghost cleanup with explicit `RemovedElementsEvent`. | core | — | none (no users in tree) | `_nh_criterion` | Stale: data-accessor methods use old names (`getNbDataForElements`), so the class is abstract; dead code compiled into core. |
| EMB-001 | `EmbeddedInterfaceModel` | `src/model/solid_mechanics/solid_mechanics_model_embedded_interface/embedded_interface_model.{hh,cc}`; options `EmbeddedInterfaceModelOptions{has_intersections}` (`src/model/model_options.hh:107-128`) | `SolidMechanicsModel` subclass with a primitive (reinforcement) mesh: uses `MeshDataMaterialSelector<std::string>("physical_names")` for bulk, builds `interface_mesh` (segments) by intersection, registers a 1D FE engine `"EmbeddedInterfaceFEEngine"` on it, registers material allocator `reinforcement` (option `elastic` → `MaterialReinforcement<MaterialElastic<1>, dim>`), assigns reinforcement materials to interface elements by their `physical_names`, adds dumper `"reinforcement"`. | embedded | CGAL | `test_embedded_element_matrix` (`triangle.msh`, `embedded_element.dat`), `test_embedded_interface_model` (`embedded_mesh.msh`, `material.dat`, `matrix`), `test_embedded_interface_model_prestress` (`embedded_mesh_prestress*.msh`, `prestress.dat`); example `examples/c++/embedded_model` | — | Risk: code references removed APIs (`materials[...]`, `MaterialSelector`, `assignMaterialToElements`, private `element_filter`) — likely does **not compile** with `embedded` ON at this commit; oracle build must confirm. |
| EMB-002 | `EmbeddedInterfaceIntersector` | `…/embedded_interface_intersector.{hh,cc}`; uses `MeshSegmentIntersector` (`src/geometry/`, R1) | For each `_segment_2` of the primitive mesh (grouped by `physical_names`) build CGAL `Segment_3` (z=0 in 2D) and intersect with background elements of types `_triangle_3`, `_triangle_6`, `_tetrahedron_4`; output segments appended to `interface_mesh` with element data `associated_element` (background element) and `physical_names`. 1D → error. | embedded | CGAL (`Cartesian` kernel, AABB tree) | embedded tests | not parallel-aware (`ghost_type` ignored) | Requires an exact/consistent geometric predicate implementation in Rust (R1 geometry). |
| EMB-003 | `MaterialReinforcement<Mat, dim>` (`reinforcement elastic`) | `materials/material_embedded/material_reinforcement{.hh,_tmpl.hh}` | 1D constitutive law on interface segments: params `area` (1.0), `pre_stress` (internal). Strain along bar ε_s = C·ε_voigt(background) with directing cosines C = (l², m², [n²,] mn, ln, lm)/‖Δx‖² (`computeDirectingCosinesOnQuad`) evaluated from background shape derivatives at interface quad points (precomputed per (interface type, background type)); stress from `Mat` (1D elastic: σ = Eε) + prestress; residual r = A ∫ Bᵀ Cᵀ σ_s ds assembled into background nodes (factor −1); stiffness K = A ∫ Bᵀ Cᵀ D C B ds (Gomes & Awruch 2001). Potential energy ×area. | embedded | CGAL (via EMB-002) | `test_embedded_element_matrix`, `test_embedded_interface_model*` | ghost interface elements: filters built per ghost type | Risk: `getEnergy("potential")` multiplies the running sum by `area` inside the loop over types. |
| EMB-004 | `InterfaceMeshDataMaterialSelector<T>` | `embedded_interface_model.hh:132-141` | `ElementDataMaterialSelector` reading `interface_mesh` element data (`physical_names`). | embedded | — | embedded tests | — | — |
| EMB-005 | Embedded dumping | `embedded_interface_model.cc:97-99,132-153` | `"reinforcement"` dumper on `interface_mesh` (dim 1); `addDumpGroupFieldToDumper` creates 1D elemental fields for it, other dumpers fall back to SMM (exceptions swallowed). | embedded | IOHelper | embedded tests (paraview dir) | — | — |
| COH-001 | `SolidMechanicsModelCohesive` construction | `src/model/solid_mechanics/solid_mechanics_model_cohesive/solid_mechanics_model_cohesive.{hh,cc}` (`:104-152`); options `SolidMechanicsModelCohesiveOptions{analysis_method, is_extrinsic=false}` (`src/model/model_options.hh:68-84`) | Subclass of SMM (model type `_solid_mechanics_model_cohesive`). Registers FE engines `"CohesiveFEEngine"` (`FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_cohesive>` on the mesh) and `"FacetsFEEngine"` (regular kind on `mesh_facets`, dim−1, integration order = polynomial degree of the corresponding cohesive type — `FacetsCohesiveIntegrationOrderFunctor`); installs `DefaultMaterialCohesiveSelector` with the previous selector as fallback; Paraview dumper `"cohesive elements"` (kind `_ek_cohesive`); creates `CohesiveElementInserter` (which builds `mesh_facets` via `mesh.initMeshFacets()`); in distributed meshes splits a `cohesive_synchronizer` from the element synchronizer (cohesive kind) registered for `_constitutive_law_id`, `_smm_stress`, `_smm_boundary`. | cohesive_element | — | `test_solid_mechanics_model_cohesive` (gtest `test_cohesive.cc`, PARALLEL 1 2: `ExtrinsicModeI/ModeIFiniteDef/ModeII` with `material_0.dat` / `material_0_finite_def.dat` (cohesive_linear), `IntrinsicModeI/ModeII` with `material_1.dat` (cohesive_bilinear); types seg2, tri3, quad4, tri3+quad4, tri6, quad8, tri6+quad8, tet4, tet10 (hex commented); dissipated energy tol 0.5·surface) | see COH-008 | Declared-but-undefined methods `initFacetFilter`, `insertElementsFromMeshData`, `initFacetsCheck` (dead declarations). |
| COH-002 | Cohesive `initFull` / law initialisation | `solid_mechanics_model_cohesive.cc:162-252,259-286` | `initFullImpl`: store `is_extrinsic`, forward to inserter; distributed+extrinsic → `facet_stress_synchronizer` (copy of facet synchronizer with send/recv swapped, tag `_smmc_facets_stress`); register `CohesiveMeshGlobalDataUpdater`; parse optional `cohesive_inserter [ … ]` sub-section of the model section; then `SolidMechanicsModel::initFullImpl`. `initModel`: add cohesive connectivity types (`getCohesiveElementType(getFacetType(type))`) for every non-empty bulk type. `initConstitutiveLaws`: `facet_material` (per facet of `mesh_facets`, default = first cohesive material index) filled by the selector for every non-ghost facet (selector result 0 is treated as "skip"); extrinsic → `material.addFacet(facet)`; then SMM `initConstitutiveLaws`; then extrinsic → `initAutomaticInsertion()` else `insertIntrinsicElements()`. | cohesive_element | — | all cohesive tests | facet material indices propagate to ghost facets via `_constitutive_law_id` (COH-008) | Selector returning index 0 for a facet means "no cohesive material" (`:229-232`): the first material in the input file must not be cohesive for facets to be handled. |
| COH-003 | Intrinsic insertion | `solid_mechanics_model_cohesive.cc:289-293`; `src/mesh_utils/cohesive_element_inserter.cc:93-96,99-225` | At init, `insertion_facets := limitCheckFacets(...)` (all facets with two non-ghost-only neighbours, no cohesive neighbour, barycenter inside `bounding_box` ± tol, both neighbours in `cohesive_zones` if given, facet `physical_names` ∈ `cohesive_surfaces` if given) and insert cohesive elements on all of them (COH-005). | cohesive_element | — | `IntrinsicModeI/II`, `test_cohesive_insertion_along_physical_surfaces` (`3d_spherical_inclusion.geo`, `input_file.dat`: exponential laws on surfaces `coh1..coh5, interface`), `test_cohesive_group_update` (`bar.geo`, `material.dat`) | ghost facets inserted consistently via `_ce_groups` sync | — |
| COH-004 | Extrinsic (dynamic) insertion & stress check | `solid_mechanics_model_cohesive.cc:296-487,586-595` | `initAutomaticInsertion`: `limitCheckFacets()` → `check_facets`; update facet-stress synchronizer; allocate `facet_stress` (per facet quad: 2·dim² — stress of both neighbours); `computeNormals()` (facet normals on `FacetsFEEngine` quad points + tangents via `Math::compute_tangents`); `initStressInterpolation()` (facet quad-point coordinates → per-element/per-facet interpolation matrices for every non-cohesive material). `checkCohesiveStress()` (user calls it each step): `interpolateStress()` (each bulk material `interpolateStressOnFacets`, then sync `_smmc_facets_stress`), each cohesive material `checkInsertion()`, then `inserter->insertElements()`; returns the global number of new elements. `updateAutomaticInsertion()` re-limits check facets. | cohesive_element | — | `ExtrinsicModeI/II(FiniteDef)`; legacy unregistered `test_cohesive_extrinsic*`, `test_cohesive_parallel_extrinsic*` | facet stresses of facets on partition boundaries exchanged (`_smmc_facets_stress`), each rank checks facets it owns | Normals are computed **once** at init (reference configuration); the stress check uses Cauchy stress refreshed after converged steps for finite-deformation materials (`afterSolveStep`, `:554-572`). |
| COH-005 | Cohesive element insertion (topology) | `src/mesh_utils/cohesive_element_inserter.{hh,cc}`, `cohesive_element_inserter_inline_impl.hh`, `cohesive_element_inserter_helper.{hh,cc}` (R1 owns the mesh algorithms) | `insertElements(only_double_facets=false)`: sync `insertion_facets` on `mesh_facets` (`_ce_groups`); `CohesiveElementInserterHelper` doubles facets (dim−1), finds and doubles sub-facets (edges in 3D, then points) and nodes along the crack front, updates element↔subelement maps, quadratic segments and creates cohesive elements (`insertCohesiveElement`) — or only doubles facets (`insertFacetsOnly`); builds `CohesiveNewNodesEvent` (new nodes + their original nodes) and `NewElementsEvent`; `updateInsertionFacets()` (extrinsic: inserted facets get `check=false`; arrays resized for new facets; intrinsic: all false); `MeshAccessor::updateGlobalData` → `CohesiveMeshGlobalDataUpdater` (node flags copied from old node, cohesive synchronizer schemes updated, global ids updated, global counts all-reduced, events sent if > 0). Params of the `cohesive_inserter` section: `cohesive_surfaces` (set of group names), `cohesive_zones` (set of volume names), `bounding_box` (dim×2 matrix); API `setLimit(axis, a, b)`, `addPhysicalSurface`, `addPhysicalVolume`. | cohesive_element | — | `test_cohesive_insertion_along_physical_surfaces`, `test_cohesive_group_update`, `test_cohesive_buildfragments`, `CohesiveElementInserter` gtests in R1's mesh tests | `GlobalIdsUpdater`, facet synchronizer, `_ce_groups` | `cohesive_element_inserter_parallel.cc` contains only includes (no behaviour). Node ordering of new nodes/elements must match for golden comparisons. |
| COH-006 | Nodal field propagation on insertion | `solid_mechanics_model_cohesive.cc:490-551` | `onElementsAdded`: SMM handling (assign laws to new elements via selector → `DefaultMaterialCohesiveSelector` uses the facet material) + resize `facet_stress` if extrinsic. `onNodesAdded` (with `CohesiveNewNodesEvent`): SMM resize then copy, for each new node, the values of its original node in `displacement, blocked_dofs, velocity, acceleration, current_position, previous_displacement, displacement_increment` and the DOF manager solution. | cohesive_element | — | cohesive tests | — | Lumped mass is flagged for reassembly (new nodes get mass by reassembly, not copy). |
| COH-007 | Cohesive residual & stiffness hooks | `solid_mechanics_model_cohesive.cc:398-413`; `materials/material_cohesive.cc:88-277` | `assembleInternalForces()`: every cohesive material `computeTraction(_not_ghost)` (normals from current positions, openings, tractions) **before** the bulk SMM pass (which also calls cohesive `assembleInternalForces` per material; cohesive `computeAllStresses` is a no-op). Cohesive force: for each quad `t_q + t_contact,q` times shape values, integrated, written as −∫(t)N on the first-side nodes and +∫(t)N on the second side (assembled with factor +1). Stiffness: K_e = ∫ (N A)ᵀ Dᵀ (N A), A = [I, −I] (dim·n_side rows), D = ∂t/∂δ (dim×dim) from `computeTangentTraction`, assembled `_unsymmetric`. | cohesive_element (+implicit for K) | — | `test_assembling_K_cohe_elements` (legacy, unregistered: `quadrangle.msh`, `K_matrix_verified.dat`), `test_cohesive_intrinsic_impl` (legacy) | ghost cohesive tractions exchanged with `_smm_stress` | Risks: tangent is used transposed (Dᵀ) in K; the global K is declared `_symmetric` by SMM while cohesive blocks are unsymmetric; `assembleStiffnessMatrix` recomputes normals but not openings (uses openings of the last residual). |
| COH-008 | Cohesive model parallel data | `solid_mechanics_model_cohesive_parallel.cc:45-435` | `updateCohesiveSynchronizers`: append new cohesive elements to cohesive synchronizer schemes (based on facet synchronizer schemes) and prune facet-stress schemes to facets still checked and still shared between the two ranks; `updateFacetStressSynchronizer` likewise. Data accessor: regular elements — `_smmc_facets_stress` packs the facet stress of the side owned by the sender (slot chosen from the ghost type of the facet's first neighbour), `_constitutive_law_id` adds facet material indices for (dim−1) elements (on unpack: register facet in cohesive material if extrinsic, set `facet_material`); cohesive elements — `_smm_boundary` (f_int, v, blocked), CLH tags. | cohesive_element, parallel | MPI | `test_solid_mechanics_model_cohesive` with 2 ranks; legacy parallel tests (unregistered, package name `parallel_cohesive_element` does not exist) | as described | `getNbData` for `_smm_boundary` counts `2·Real+bool` per dof while pack writes f_int, v, blocked — consistent. |
| COH-009 | Cohesive material selectors | `material_selector_cohesive.{hh,cc}` | `DefaultMaterialCohesiveSelector`: cohesive element → material of its facet (`facet_material` of `subelement_to_element(el)[third_dim?1:0]`) else first cohesive material; facet (dim−1) → `facet_material(facet)`; other → fallback (`DefaultMaterialSelector` on current indices). `MeshDataMaterialCohesiveSelector`: facet/cohesive element → material named by the facet's `physical_names` (fallback default cohesive selector; bulk fallback `MeshDataMaterialSelector<string>("physical_names")`). `MaterialCohesiveRulesSelector(rules, mesh_data_id="physical_names")`: facet between bulk regions (id1,id2) → cohesive material `rules[(id1,id2)]` or `rules[(id2,id1)]` (boundary facets use id1 twice). `getDefaultCohesiveMaterial` = first law that is a `MaterialCohesive` (−1 if none). | cohesive_element | — | cohesive tests; python bindings expose `MaterialCohesiveRulesSelector` (R4) | — | Exceptions in lookups silently fall back. |
| COH-010 | `MaterialCohesive` base | `materials/material_cohesive{.hh,.cc,_inline_impl.hh}` | Material on `_ek_cohesive` elements (filter re-initialised to cohesive kind; FE engine `"CohesiveFEEngine"`), with `facet_filter` (extrinsic: facets of `mesh_facets` it may be inserted on). Params `sigma_c` (`FacetRandomInternalField`, parsable/readable, per facet quad), `delta_c` (Real 0). Internals (cohesive kind): `reversible_energy`, `total_energy`, `opening` (dim, hist.), `tractions` (dim, hist.), `contact_tractions`, `contact_opening`, `delta max` (no history — see risk), `damage`, `normal` (dim). Normals: `computeNormalsOnIntegrationPoints<type, CohesiveReduceFunctionMean>` (mean of the two sides, current positions); openings: `interpolateOnIntegrationPoints<type, CohesiveReduceFunctionOpening>` (side 2 − side 1 of displacement). Energies after converged step: E_tot += ½(δ−δ_prev)·(t+t_prev), E_rev = ½ t·δ; `getEnergy("reversible" / "dissipated" (=tot−rev) / "cohesive contact"(½ t_c·δ_c))`. `getEnergy(id, element)` is not overridden (falls back to `Material::getEnergy(id, element)`: `"potential"` → SMM-012 defect, otherwise 0). | cohesive_element | — | cohesive tests, `test_material_cohesive` gtest (`ModeI`, `ModeII` tol 1e-6, `Cycles` tol 2e-3, all dims) | `_smm_stress`: tractions + contact tractions; `_smmc_damage`: damage | Risk: `use_previous_delta_max` is set by derived ctors **after** the base ctor tested it (`material_cohesive.cc:77-79`), so `delta_max` never gets history; laws reading `delta_max.previous(...)` (exponential, linear_friction implicit) dereference a null history. Verify with the oracle. |
| COH-011 | Cohesive internal fields | `materials/cohesive_internal_field{.hh,_tmpl.hh}` | `CohesiveInternalField<T>` (element kind `_ek_cohesive`, FE engine `CohesiveFEEngine`), `FacetInternalField<T>` (spatial dim −1, regular kind, FE engine `FacetsFEEngine`, filter = material facet filter), random variants `CohesiveRandomInternalField`, `FacetRandomInternalField` with `.dat` random-parameter adaptors. | cohesive_element | — | indirect | — | — |
| COH-012 | `FragmentManager` | `fragment_manager.{hh,cc}` | `buildFragments(damage_limit=1)`: (parallel: sync cohesive damage `_smmc_damage`) clusters elements with `GroupManager::createClusters(dim, mesh_facets, "fragment", filter)`, where regular elements always pass and a cohesive element passes (i.e. connects its two sides) iff at least one of its quad points has damage < damage_limit − 1e-14; groups `fragment_<i>`; fragment index parsed from the group name; mass. `computeMass` (∫ρ per component), `computeCenterOfMass` (∫ρx / m), `computeVelocity` (∫ρv / m), `computeInertiaMoments` (∫ρ(tr(rrᵀ)I − rrᵀ), then sorted eigen-decomposition → principal moments & directions), `computeNbElementsPerFragment`, `computeAllData`. ρ per quad = material `rho` (constant). Results all-reduced. Optional dump data stored as mesh element data (`fragments`, `fragments mass`, `moments of inertia`, `elements per fragment`). | cohesive_element | — | `test_cohesive_buildfragments` (`mesh.geo`, `material.dat`: cohesive_linear with random σ_c, `seed = 1`); legacy `test_cohesive_parallel_buildfragments` | all-reduce sums; clustering across ranks handled in `GroupManager` (R1) | Risks: `integrateFieldOnFragments` resets the fragment's accumulator for each element type (`fragment_manager.cc:424`) — multi-type fragments keep only the last type; `createDumpDataArray` writes into a copy (`Vector<T> md_tmp = …`, `:502`) so the mesh data stay unset. |
| COH-013 | Cohesive dumping | `solid_mechanics_model_cohesive.cc:598-623` | `"cohesive elements"` dumper fields use `_ek_cohesive`; `"facets"` dumper uses dim−1; `onDump` flattens cohesive internals then SMM `onDump`. `setTimeStep` also sets the cohesive dumper time step. | cohesive_element | IOHelper | cohesive tests (paraview) | — | — |
| CMAT-001 | `cohesive_linear` (extrinsic & intrinsic linear softening, Camacho–Ortiz type) | `constitutive_laws/material_cohesive_linear{.hh,.cc,_inline_impl.hh}` | Effective opening δ = √(β²/κ² Δt² + Δn²) (Δn excluded on penetration); δ_max = max; d = min(δ_max/δc, 1); T = (β²/κ Δt + Δn)·σc/δ_max·(1−d) (secant unloading to the origin); d = 0 → T = insertion stress (extrinsic) unless penetration; penetration (Δn/δc < −tol and not fully broken unless `contact_after_breaking`) → contact traction = penalty·Δn n. Extrinsic insertion check (`checkInsertion`): facet stress = mean of both sides; effective norm σ_eff = √(⟨σn·n⟩₊² + β⁻²‖tangential part‖²); facet inserted if mean (or max with `max_quad_stress_insertion`) over its quad points > σ_c entry paired with the facet (the loop zips the facet filter with the *per-quad-point* σ_c array, so the k-th facet is compared with the k-th quad entry — aligned only for one quad point per facet); new quad data σc_eff = σ_eff, δc_eff = 2G_c/σ_eff (or δc·σ_c/σ_eff if `delta_c` given), insertion stress = σ·n (sign flipped in 1D/2D), previous traction = insertion stress. Weibull-type size scaling of facet σ_c (`volume_s`, `m_s`). Tangent: see §2.17. Params `beta`, `G_c`, `penalty`, `volume_s`, `m_s`, `kappa`, `contact_after_breaking`, `max_quad_stress_insertion`, `recompute`, plus `sigma_c`, `delta_c`. | cohesive_element | — | `test_material_cohesive` (ModeI/ModeII/Cycles), `ExtrinsicModeI/II(FiniteDef)` (`material_0*.dat`), `test_cohesive_buildfragments`, `test_cohesive_group_update`; legacy extrinsic/implicit tests | facet-level check only on owned facets | Risks: `scaleInsertionTraction` uses an **uninitialised** `base_sigma_c_v` vector (`material_cohesive_linear.cc:161-164`); tangent uses the facet `sigma_c` base value (not σc_eff) (`_inline_impl.hh:219`). |
| CMAT-002 | `cohesive_bilinear` (intrinsic bilinear) | `constitutive_laws/material_cohesive_bilinear.{hh,cc}` | Linear law with initial elastic branch: `delta_max` default = `delta_0`, insertion stress 0; on new elements δc = 2G_c/σc and σc ← σc·δc/(δc − δ0) (error if δc − δ0 < tol), optional volume scaling; traction from CMAT-001 (with d_lin = δ_max/δc), then stored damage overwritten by d = clamp((δ_max − δ0)/(δc − δ0), 0, 1). Param `delta_0` (0). σc_eff takes σ_c's random distribution at init. | cohesive_element | — | `IntrinsicModeI/II` (`material_1.dat`: β=1, G_c=10, σ_c=1e6, δ0=1e-7) | — | Risk: `onElementsAdded` indexes material arrays with the **global** cohesive element index `el.element` (`material_cohesive_bilinear.cc:82-92`), valid only if a single cohesive material holds all cohesive elements of that type. |
| CMAT-003 | `cohesive_exponential` (Xu–Needleman-type exponential) | `constitutive_laws/material_cohesive_exponential.{hh,cc}` | δ = √(β²Δt² + Δn²); δ_max ← max(δ_max,prev, δ); T = (β² δ_vec + (1−β²)Δn n)·e·σc·exp(−δ_max/δc)/δc (zero if δ < tol); compression (Δn < 0 and abs(Δn) > tol): coupled part with δ = βΔt on the tangential opening, plus penalty T_n = n·Δn·e·σc·exp(−Δn/δc)/δc (`exponential_penalty=true`) or n·contact_tangent·e·σc·Δn/δc. Tangent: (I_β + (1−β²)n⊗n − δ_e⊗δ_e/(δ δc))·e^{1−δ/δc}σc/δc + compressive penalty. Params `beta` (0), `exponential_penalty` (true), `contact_tangent` (1; forced to 1 with exponential penalty), `sigma_c`, `delta_c`. | cohesive_element | — | `test_cohesive_insertion_along_physical_surfaces` (insertion only); legacy `test_cohesive_intrinsic_impl` | — | Uses `sigma_c` base value and `delta_c` scalar (no per-quad σc_eff). Reads `delta_max.previous` (null history, COH-010 risk). Unloading branch described in comments is not implemented (always exponential of δ_max). |
| CMAT-004 | `cohesive_linear_fatigue` (Nguyen–Repetto–Ortiz–Radovitzky 2001) | `constitutive_laws/material_cohesive_linear_fatigue.{hh,cc}` | CMAT-001 envelope with unloading/reloading stiffnesses K⁻, K⁺ and 1D traction T₁d: loading (δ̇>0, fatigue regime): K⁺ ← K⁺(1 − δ̇/δ_f), T₁d += K⁺δ̇, capped by envelope σc(1−δ/δc) (then K⁺ = K⁻); switch to normal regime when δ_max > `fatigue_ratio`·δc (δ_max reset to σc/(T₁d/δ + σc/δc)); unloading: K⁺ += (K⁺−K⁻)δ̇/δ_f, T₁d = K⁻δ; traction = (β²/κ Δt + Δn)·K⁻. Params `delta_f` (default δc_eff; error if < δc), `progressive_delta_f` (false), `count_switches` (false; adds internals `switches` (Int), `delta_dot_prec`), `fatigue_ratio` (1). Internals `delta_prec`, `K_plus`, `K_minus`, `T_1d`, `normal_regime` (bool). Tangent inherited from CMAT-001. | cohesive_element | — | legacy `test_cohesive_extrinsic_fatigue` (`material_fatigue.dat`, `fatigue.msh`) — **not registered** | — | `delta_f = this->delta_c_eff` converts the internal to its default value. |
| CMAT-005 | `cohesive_linear_friction` | `constitutive_laws/material_cohesive_linear_friction.{hh,cc}` | CMAT-001 plus Coulomb friction under penetration: τ_max = μ·penalty·abs(Δn); slip = abs(‖Δt‖ − s_res,prev); τ = min(k_f·slip, τ_max) (sign of Δt‖ − s_res,prev); friction force = τ·Δt/‖Δt‖ added to the traction; s_res = ‖Δt‖ − abs(τ)/k_f (or ‖Δt‖ if k_f = 0). Implicit: δ_max reset to previous before each traction computation. Tangent: CMAT-001 tangent + k_f(I − n⊗n) when sticking (τ < τ_max, τ_max > tol), τ_max from previous normal opening. Params `mu` (0), `penalty_for_friction` (0). Internals `residual_sliding` (hist.), `friction_force` (dim). | cohesive_element | — | legacy `test_cohesive_friction` (commented out in CMake; `material.dat`) | — | Risk: implicit path copies `delta_max.previous` (null history, COH-010). |
| CMAT-006 | `cohesive_linear_uncoupled` | `constitutive_laws/material_cohesive_linear_uncoupled.{hh,cc}` | Separate normal/shear damage: δ_n = √(β²/κ²Δt² + Δn²), δ_t = √(β²/κ²Δt² + Δn²·δc²/R²) (as coded both contain the tangential term); d_n = δ_n,max/δc, d_t = δ_t,max/δc; T_n = Δn n σc/δ_n,max (1−d_n) (insertion stress if d_n = 0), T_t = β²/κ Δt σc/δ_t,max (1−d_t) (0 if d_t ∈ {0,1}); penetration → penalty contact. Tangent from previous δ_max values (histories enabled for `delta_n_max`, `delta_t_max`). Param `roughness` R (1). | cohesive_element | — | none upstream | — | Internals resized explicitly at each `computeTraction` call. |
| XMAT-000 | `extra_materials` package status & registration | `extra_packages/extra-materials/package.cmake` (packages `extra_materials` (ADVANCED, depends solid_mechanics) and `extra_materials_non_local` (depends extra_materials, damage_non_local)); `src/material_extra_includes.hh`, `src/material_non_local_extra_includes.hh` (empty file) | Legacy material list macros (`AKANTU_EXTRA_MATERIAL_LIST`, `AKANTU_DAMAGE_NON_LOCAL_MATERIAL_EXTRA_LIST`, guarded by obsolete `AKANTU_CMAKE_LIST_MATERIALS`) list keywords `damage_linear`, `brittle`, `material_FE2`, `damage_iterative`, `iterative_stiffness_reduction`, `vreepeerlings`, `ve_stiffness_prop`, `visco_plastic`, `orthotropic_damage_iterative`, and non-local `brittle_non_local`, `damage_iterative_non_local`, `damage_orthotropic_iterative_non_local`. Actual factory registration is via `INSTANTIATE_MATERIAL(keyword, Class)` in each `.cc`. | extra_materials / extra_materials_non_local (OFF by default; enabled by none of the upstream presets) | — | `extra_packages/extra-materials/test/**` (see rows) | — | **Stale code**: uses the pre-v5 API (`INSTANTIATE_MATERIAL` is not defined anywhere in `src/`; `UInt` templates, `this->model`, `this->element_filter`, `use_previous_stress`, old `MaterialElastic::computeStressOnQuad(grad_u, sigma, …)` and `Matrix::mul/eye/copy` APIs, `InternalField(name, *this)` constructors). Expected not to compile against the reference core; the oracle build must confirm. Port = re-implementation from the formulas below; differential testing impossible unless the package is fixed upstream. |
| XMAT-001 | `damage_iterative` (sequentially linear / saw-tooth damage) | `src/material_damage/material_damage_iterative{.hh,.cc,_inline_impl.hh}` | `MaterialDamage` with: per-quad random strength `Sc`; equivalent stress σ_eq = max principal value of (1−d)σ_el / Sc; global max over the material (`norm_max_equivalent_stress`, all-reduced unless the model is an RVE); `updateDamage()` (called by user loop, returns all-reduced number of damaged quads): if max ≥ 1, every quad with σ_eq ≥ (1−dam_tolerance)·max and reduction_step < max_reductions gets reduction_step+1 and d += prescribed_dam (or d = max_damage at the last reduction); `updateEnergiesAfterDamage` → `updateEnergies`. Params `Sc` (random), `prescribed_dam` (0.1), `dam_threshold` (0.8), `dam_tolerance` (0.01), `max_damage` (0.99999), `max_reductions` (10). Internals `Sc`, `damage_step` (UInt), `equivalent_stress`. Sync tag `_user_2` for damage. | extra_materials | — | `test_material_damage_iterative` (+ `_parallel` variant, same source; `material.dat`, `plate.msh`), used by FE2 tests | `_user_2` damage exchange; max/sum all-reduces | `computeStress` resets the max to 0 per element type (keeps only the last type's max); the alternative `updateDamage(quad, …)` overload uses `dam_threshold`. Stale API (XMAT-000). |
| XMAT-002 | `damage_iterative_non_local` | `material_damage_iterative_non_local{.hh,.cc,_inline_impl.hh}` | Non-local version: registers non-local variable `grad_u` → `grad_u non local` (dim²); `computeStress` empty; non-local pass computes elastic stress, applies (1−d), then σ_eq from the **averaged grad_u**, max reset per type, all-reduce max in `computeNonLocalStresses`. | extra_materials_non_local | — | `test_material_damage_iterative_non_local_serial` (`material_non_local.dat` with `base_wf`, `plate.msh`), `test_material_damage_iterative_non_local_parallel` (`two_materials.dat`, `one_circular_inclusion.msh`) | non-local averaging (NL-*) | Uses removed `NonLocalManager::nonLocalVariableToNeighborhood`. |
| XMAT-003 | `iterative_stiffness_reduction` (Rots & Invernizzi 2004 regularised saw-tooth) | `material_iterative_stiffness_reduction.{hh,cc}` | Derived from XMAT-001. Init per quad: ε_u = 2G_f/(Sc·h_cb), D = Sc/(ε_u − Sc/E). Equivalent stress as XMAT-001. `updateDamage`: same selection; at step k < max: d = 1 − a^{−k}, Sc = ε_u(1−d)E·D/((1−d)E + D); at k = max: d = max_damage. Params `Gf`, `crack_band_width`, `reduction_constant` a (2). Internals `ultimate_strain`, `tangent`. | extra_materials | — | `test_material_iterative_stiffness_reduction` (`material_stiffness_reduction.dat`, `two_elements.msh`, `.verified` reference output) | as XMAT-001 | — |
| XMAT-004 | `damage_linear` | `material_damage_linear{.hh,.cc,_inline_impl.hh}` | ε̂ = √Σ⟨λ_i⟩₊² with λ_i eigenvalues of ∇u read as a 3×3 matrix (`Math::matrix33_eigenvalues`); ε_min = Sigc/E, ε_max = 2Gc/Sigc + ε_min; K (threshold, default ε_min): if ε̂ > K: d = min((ε̂−ε_min)/(ε_max−ε_min)·ε̂/ε_max, 1), K = ε̂; σ = (1−d)σ_el. Params `Sigc` (1e5), `Gc` (2). | extra_materials | — | none | — | Bugs: the K pointer is not advanced in the quad loop (all quads share the first K); eigenvalues of a dim×dim `grad_u` read as 3×3. |
| XMAT-005 | `brittle` (strain-rate dependent brittle) | `material_brittle{.hh,.cc,_inline_impl.hh}` | ε̇_eq = √(2/3 Σ (sym ∇v − tr/3 δ)²) (+2/3(tr/3)² in 2D), ∇v from nodal velocities; fracture stress = S_0 if ε̇_eq ≤ E_0 else **A** (the cubic polynomial A,B,C,D documented is not evaluated); σ_eq = max principal stress; d = 1 if σ_eq > fracture stress; σ = (1−d)σ_el. Params `S_0` (157e6), `E_0` (27e3), `A` (1.622e-5), `B` (−1.3274), `C` (3.6544e4), `D` (−181.38e6). Internal `strain_rate_brittle`. | extra_materials | — | none | — | Code defaults differ from the header documentation. |
| XMAT-006 | `brittle_non_local` | `material_brittle_non_local{.hh,_inline_impl.hh}` | Averages `Sigma max` → `Sigma_max non local`; non-local pass applies the brittle damage criterion with the averaged σ_eq. | extra_materials_non_local | — | none | non-local | No `.cc`; only listed in legacy macro list (not registered). |
| XMAT-007 | `vreepeerlings` (rate-dependent Vree/Peerlings equivalent-strain damage) | `material_vreepeerlings{.hh,_inline_impl.hh,_tmpl.hh}` | Equivalent strain (modified von Mises): ε̂ = (k−1)/(2k(1−2ν)) I₁ + 1/(2k)√(((k−1)/(1−2ν))² I₁² + 12k/(1+ν)² J₂) with I₁, J₂ of ∇u; rate version from strain-rate invariants (error branch if not loading); κ₀ = C(1 + (abs(ε̂̇)A)^B)(1−r) + r·κ_q; κ_dyn = clamp(κ₀, κ_oi, 0.99κ_c); if ε̂ > κ_oi: d₁ = 1 − κ_dyn/ε̂·(κ_c − ε̂)/(κ_c − κ_dyn), d = min(max(d, d₁), 1) (+counters); σ = (1−d)σ_el. Params `Kapaoi` (1e-4), `Kapac` (2e-4), `Arate` (0), `Brate` (1), `Crate` (**bound to Brate**), `Kct` (1), `Kapao_randomness` (0). Internals `Kapa` (random), strain-rate tensor, `fulldam-valstrain(-rate)`, `number-damage`, `equi-strain(-rate)`. | extra_materials | — | none | — | Header-only, **never registered** (no `INSTANTIATE_MATERIAL`); `Crate` param overwrites `Brate` and leaves `Crate` uninitialised. |
| XMAT-008 | `vreepeerlings_non_local` | `material_vreepeerlings_non_local{.hh,.cc,_inline_impl.hh}` | Averages `equi-strain` and `equi-strain-rate`; non-local pass applies the damage update with averaged values. | extra_materials_non_local | — | none | non-local | `.cc` has the registration commented out — unreachable. |
| XMAT-009 | Orthotropic (tensorial) damage base | `material_orthotropic_damage{.hh,_tmpl.hh}` | Second-order damage D (dim², principal-frame) with directions `damage_principal_directions`; Cauchy stress (Ragueneau et al. 2008 eq. 7): σ = √(1−D) σ̃ √(1−D) − ((1−D):σ̃)/(dim − tr D)·(1−D) + (1/dim) tr σ̃ (1 − η/dim·tr D) I; tangent only for 2D (explicit 3×3 formula), 1D (1−D₁₁)·E, 3D left unchanged; dissipated energy like MAT-010. Params `eta` (2), `max_damage` (0.99999). | extra_materials | — | none directly | — | `computeSqrtOneMinusD` assumes D diagonal in its frame (debug assert). |
| XMAT-010 | `orthotropic_damage_iterative` | `material_orthotropic_damage_iterative{.hh,.cc,_inline_impl.hh}` | Iterative variant: per-quad `Sc`, σ_eq = largest principal stress (sorted eig, with directions stored in `equiv_stress_dir`) / Sc; tracks the single global max quad (`q_max`), skipping quads whose damage would exceed `max_damage`; `updateDamage`: rotate D of q_max into the stress frame, add `prescribed_dam` to the (0,0) component, re-diagonalise (new directions), components ≥ `dam_threshold` set to `max_damage`; all-reduce count. Third term uses min(η/dim·tr D, max_damage). Params `Sc`, `prescribed_dam` (0.1), `dam_threshold` (0.8). | extra_materials | — | none | max all-reduce (but q_max is local) | In parallel only the local q_max is damaged on each rank (no global argmax). Relies on legacy proxy semantics of `Matrix<Real> q_dam = dam_it[i]` to write back. |
| XMAT-011 | Orthotropic damage non-local variants | `material_orthotropic_damage_non_local.hh`, `material_orthotropic_damage_iterative_non_local{.hh,_inline_impl.hh}` | Non-local averaging of `grad_u` (→ `grad_u non local`) used for the equivalent stress of XMAT-010. | extra_materials_non_local | — | none | non-local | `material_orthotropic_damage_non_local.hh` uses an even older `MaterialNonLocal<dim>` API (`firstType/lastType`, `getNbDataForElements`). |
| XMAT-012 | `visco_plastic` (Perzyna-type viscoplasticity) | `src/material_plastic/material_viscoplastic{.hh,.cc,_inline_impl.hh}` | Explicit rate update: Δε_p = ts·ε̇₀·‖σ‖^{m−1}/(σ_y + r)^m · σ (‖σ‖ = √(σ:σ) of the *current* stress), r += h‖Δε_p‖, σ += C:(Δ∇u − Δε_p), then `computeStressAndInelasticStrainOnQuad`. Tangent: isotropic elastic with shear correction (−μ on shear diagonal). Params `rate` m (0), `edot0` (0), `ts` (0) + plastic `h`, `sigma_y`. | extra_materials | — | none | — | `previous_sigma_it` not advanced in the loop (bug). |
| XMAT-013 | `ve_stiffness_prop` (stiffness-proportional / Kelvin–Voigt viscosity) | `src/material_viscoelastic/material_stiffness_proportional.{hh,cc}` | σ = σ_el(∇u) + α·σ_el(∇v) (∇v = gradient of nodal velocity; Caughey damping η = αE); potential energy from the elastic part only. Param `Alpha` (0). Internals `stress_viscosity`, `stress_elastic`. | extra_materials | — | none | — | — |
| XMAT-014 | `material_FE2` (FE² multiscale, ASR) | `src/material_FE2/material_FE2{.hh,.cc,_inline_impl.hh}` | Macro material (base `MaterialThermal`) holding one `SolidMechanicsModelRVE` (own mesh read from `mesh_file`) **per quadrature point** of a single element type; stress σ_v = C_q·ε_v + σ_th; tangent = C_q (homogenised Voigt stiffness). `advanceASR(prestrain)`: per quad apply macro ∇u as RVE corner BCs, homogeneous ΔT, grow gel (eigenstrain), iterate damage, homogenise eigen grad u and stiffness. Params `element_type` (`_triangle_3`), `mesh_file`, `nb_gel_pockets`. Internal `material_stiffness` (voigt²). | extra_materials | — | `test_material_FE2` (`material.dat`, `periodic_plate.msh`, `square.msh`) | — | Works for one element type only; 2D-hardcoded homogenisation. |
| XMAT-015 | `SolidMechanicsModelRVE` + `GelMaterialSelector` | `src/material_FE2/solid_mechanics_model_RVE.{hh,cc}` | 2D RVE model (always static): corner nodes found from bounding box (error if missing), removed from `top/left/bottom/right` groups; BCs u = H·x on the 4 corners; volume; `averageTensorField(i,j,"stress" / "strain" / "eigen_grad_u")` = Σ∫/V; `homogenizeStiffness`: zero gel eigenstrain, three virtual tests H = 0.01·e₁⊗e₁, e₂⊗e₂, e₁⊗e₂ (NR max 2 iterations, threshold 1e-6, solution criterion), C = Σ̄·Ē⁻¹ (with crack fill/drain trick: damaged elements temporarily get homogenised E); `homogenizeEigenGradU`; `advanceASR`: set gel eigenstrain, loop solve + `updateDamage` of paste/aggregate (`materials[0..1]`) until no new damage; `GelMaterialSelector` places `nb_gel_pockets` gel elements at random barycenters inside aggregates (`rand()`). Dumps a fixed list of fields. | extra_materials | — | `test_eigenstrain_homogenization` (`mesoscale_materials.dat`, `one_inclusion.msh`), `test_elastic_homogenization` (`material_orthotropic.dat`, `homogenized_plate.geo`), `test_node_selection` (`material_test_boundary.dat`, `periodic_plate.geo`) | — | Volume loop overwrites per type (`=` not `+=`); uses `rand()` (non-reproducible across platforms). |
| IGFEM-000 | `igfem` package status & core hooks | `extra_packages/igfem/package.cmake` (package `IGFEM`, ADVANCED, depends CGAL); core hooks under `#if defined(AKANTU_IGFEM)`: element types `_igfem_segment_3`, `_igfem_triangle_4`, `_igfem_triangle_5`, kind `_ek_igfem`, geometrical/interpolation types (`src/common/aka_element_classes_info.hh:74-87,139-142,176-179`), dumper hooks (`src/io/dumper/dumpable_inline_impl.hh:91-93`, `dumper_iohelper.cc:29-112`, `dumper_internal_material_field.hh:25`, `dumper_element_partition.hh:24`, `dumper_elemental_field.hh:27`) | Interface-enriched generalised FEM (IGFEM, Soghrati–Aragón–Duarte) for 2D (and 1D segment) problems with curved material interfaces (spherical gel pockets) that cut elements; enriched nodes are added on intersected edges, elements become `igfem_*` elements with two sub-elements. | IGFEM (OFF; not in presets) | CGAL (spherical kernel `SK::Sphere_3`, AABB intersection) | `extra_packages/igfem/test/**` | partly (ghost handling in model, parallel tests) | **Stale code**: pre-v5 API throughout (`UInt`, `Matrix::storage/mul/begin_reinterpret`, `firstType/lastType`, `DistributedSynchronizer`, `INSTANTIATE_MATERIAL`, `this->materials`). Expected not to compile; oracle build must confirm. Parity must be re-derived from the formulas. |
| IGFEM-001 | IGFEM element classes | `src/element_class_igfem.{hh,cc}`, `src/element_classes_igfem/element_class_igfem_{segment_3,triangle_4,triangle_5}_inline_impl.hh`, `src/geometrical_element_igfem.hh`, `src/interpolation_element_igfem{.cc,_tmpl.hh}` | Element = parent element + enriched nodes; two sub-elements: `igfem_segment_3` (parent seg2, subs seg2+seg2, 1 enrichment), `igfem_triangle_4` (parent tri3, subs tri3+tri3, 1 enriched node; local sub-connectivity {0,1,3},{0,3,2}), `igfem_triangle_5` (parent tri3, subs tri3+quad4, 2 enriched nodes; {0,3,4},{3,1,2,4}). Shape at a point in sub-element s: parent shapes N₁..N₃ (evaluated via inverse map to the parent) plus enrichment ψ_e = sub-element Lagrange shape of the enriched node `enrichments[s][e]`. `getOrientation(is_inside)` decides which sub-element lies inside the inclusion (4 cases for tri4). `mapToPhysicalDomain`, `mapToParentRefDomain`, `getSubElementCoords`, `getParentCoords`. Min integration order 1. | IGFEM | — | `test_fe_engine/*` (`test_interpolate`, `test_integrate`, `test_fe_engine_precomputation`, `test_gradient`, fixtures `mesh_info_igfem_triangle_{4,5}.txt`, `.verified` outputs, mesh generated by `igfem_mesh_generation.cc`) | — | Static tables use raw `UInt*` arrays; port as const tables. `element_class_igfem.cc` is fully commented out. |
| IGFEM-002 | IGFEM shape functions & integrator | `src/shape_igfem{.hh,.cc,_inline_impl.hh}` (`ShapeLagrange<_ek_igfem>`), `src/integrator_gauss_igfem{.hh,_inline_impl.hh}` (`IntegratorGauss<_ek_igfem>`), `src/fe_engine_template_tmpl_igfem.hh` | Integration points = Gauss points of sub-element 1 followed by those of sub-element 2 (mapped to the parent natural coordinates, stored in `igfem_integration_points`); jacobians per sub-element; shapes and derivatives precomputed per element; shapes also precomputed **at enriched nodes** (`precomputeShapesOnEnrichedNodes`) to recover physical values; `interpolateAtEnrichedNodes`, `extractValuesAtStandardNodes`, `inverseMap` (parent or sub-element), `contains`, `computeShapes/Derivatives` at physical points, `fieldTimesShapes`, integration (`integrate`, `integrateOnIntegrationPoints`); FE engine template specialisations for `_ek_igfem` (gradient, interpolate, assemble). | IGFEM | — | `test_fe_engine/*` | — | Needed only if IGFEM is ported; depends on R1 FE engine abstractions. |
| IGFEM-003 | IGFEM geometry: enrichment & growing spheres | `src/igfem_enrichment{.hh,.cc,_inline_impl.hh}`, `src/mesh_igfem_spherical_growing_gel{.hh,_tmpl.hh}`, `src/igfem_helper.{hh,cc}` | `IGFEMEnrichment`: registry of geometries (spheres) by domain; `isInside(point, domain)`, `update`, `moveInterface(new_radius)` (grow gel pockets). `MeshIgfemSphericalGrowingGel<dim>`: intersect mesh segments with a list of `SK::Sphere_3` (CGAL spherical kernel), create enriched nodes (`NewIGFEMNodesEvent` with node-per-element map), convert cut `_triangle_3` into `_igfem_triangle_4/5` (or `_segment_2` → `_igfem_segment_3`) with `NewIGFEMElementsEvent` (old→new element map), `removeAdditionalNodes`, radius increase factor, parallel node-type assignment from segment connectivity (`buildSegmentConnectivityToNodeType`). `IGFEMHelper`: static queries (nb nodes/quads per sub-element, parent/enriched node counts, sub-element types, orientation). | IGFEM | CGAL (spherical kernel) | `test_geometry/test_spherical_kernel` (CGAL only), `test_generation_igfem_mesh_from_sphere(_static)` (`test_geometry_triangle.msh`) | distributed synchronizer updates for new nodes/elements | Exact sphere–segment intersection predicates must match CGAL for identical topology. |
| IGFEM-004 | `SolidMechanicsModelIGFEM` | `src/solid_mechanics_model_igfem{.hh,.cc,_inline_impl.hh}`; options `SolidMechanicsModelIGFEMOptions` | SMM subclass + `IGFEMEnrichment`: registers FE engine `"IGFEMFEEngine"` (kind `_ek_igfem`); `initMaterials` uses `DefaultMaterialIGFEMSelector` (igfem elements → `fallback_value_igfem`); arrays `real_displacement`, `real_force`, `real_residual` (physical values at enriched nodes); `computeValuesOnEnrichedNodes()` (interpolate u at enriched-node coordinates); `onNodesAdded/Removed`, `onElementsAdded/Removed` (resize, `element_map` old→new for moving interfaces); `transferInternalValues(internal, new_elements, quads, values)` via old material's `extrapolateInternal` (first non-zero quad value); `getSubElementBarycenter`; `applyEigenGradU`; dumping of IGFEM fields (`onDump`, `addDumpGroupFieldToDumper`). | IGFEM | CGAL | `test_solid_mechanics_model_igfem` (`material.dat`, `plate.msh`, `.verified`), `test_transfer_internals`, `test_eigenstrain`, `test_igfem_element_orientation(_parallel)`, `test_volume_computation`; patch tests `test_igfem_triangle_4/5`, `test_interface_position` (`material*.dat`, `test_mesh.msh`, `test_interface_position.msh`) | `initParallel` with `MeshPartition` (old API) | — |
| IGFEM-005 | `MaterialIGFEM` base + `IGFEMInternalField` | `src/material_igfem/material_igfem{.hh,.cc,_inline_impl.hh}`, `igfem_internal_field{.hh,_tmpl.hh}`, `material_igfem_includes.hh` (legacy list `igfem_elastic`, `igfem_saw_tooth_damage`, `igfem_iterative_stiffness_reduction`) | Material on `_ek_igfem` elements combining two sub-materials (params `name_sub_mat_1`, `name_sub_mat_2`); per-quad `sub_material` index set from element orientation w.r.t. sub-material-1 domain (`setSubMaterial<_igfem_triangle_4/5>`); quad-point coordinates from the IGFEM FE engine; `computeAllStresses`, `extrapolateInternal`, `applyEigenGradU` restricted to sub-material quads. | IGFEM | — | IGFEM model tests | packElementData/unpackElementData (old names) | Bug: `setSubMaterial<_igfem_triangle_5>` case 3 assigns `sub_material_index(0)` twice, leaving index 1 stale (`material_igfem.cc:~258`). |
| IGFEM-006 | `igfem_elastic` | `material_igfem_elastic{.hh,.cc,_inline_impl.hh}` | Elastic law with per-quad Lamé fields (`lambda`, `mu`, `kpa` as `IGFEMInternalField`) set from the sub-material elastic parameters (`updateElasticInternals` on element add); stress/tangent/energy as MAT-002/004 with per-quad λ, μ. | IGFEM | — | IGFEM model/patch tests | — | — |
| IGFEM-007 | `igfem_saw_tooth_damage` | `material_igfem_saw_tooth_damage{.hh,.cc,_inline_impl.hh}` | IGFEM analogue of XMAT-001 (sequentially linear saw-tooth damage) with per-quad `Sc`, `equivalent_stress`, global max, damage increments; internals transferred on interface motion. | IGFEM (+extra_materials for tests) | — | `test_ASR_damge` (sic; `material_ASR.dat`, `one_inclusion.msh`) | max all-reduce | — |
| IGFEM-008 | `igfem_iterative_stiffness_reduction` | `material_igfem_iterative_stiffness_reduction.{hh,cc}` | IGFEM analogue of XMAT-003 (Rots–Invernizzi stiffness reduction) with `eps_u`, `reduction_step`, `D` IGFEM internals. | IGFEM (+extra_materials) | — | `test_material_igfem_iterative_stiffness_reduction` (`material_stiffness_reduction.dat`, `regular_mesh.msh`, `igfem_mesh.msh`), `…_damage_step_transfer`, `…_parallel`, `…_tangent_transfer` (`material_stiffness_reduction_2.dat`, `…_weibull.dat`, `test_damage_transfer.msh`) | parallel variant | — |
| IGFEM-009 | `NonLocalManagerIGFEM` | `src/non_local_manager_igfem.{hh,cc}` | Non-local manager variant that inserts IGFEM quadrature points (regular + igfem kinds) into neighborhoods, averages with both kinds, custom ghost cleanup and element-added handling. | IGFEM + damage_non_local | — | none registered | grid synchronizers | Based on the old NonLocalManager API (`init`, `computeAllNonLocalStresses`). |
| IGFEM-010 | IGFEM dumpers | `src/dumper_igfem_{connectivity,element_iterator,element_partition,elemental_field,generic_elemental_field,material_internal_field,quadrature_points_field}.hh` | IOHelper field adaptors that split each IGFEM element into its sub-elements for Paraview output (connectivity, partitions, elemental fields, material internals, quadrature-point fields). | IGFEM | IOHelper | IGFEM tests (paraview) | — | Output-format parity only. |
| TASN-000 | `traction_at_split_node_contact` package | `extra_packages/traction-at-split-node-contact/package.cmake` (depends `iohelper`), `README.txt` (build note only), umbrella header `src/ntn_contact_solver.hh` | Explicit-dynamics node-to-node (split-node) contact and friction for `SolidMechanicsModel` (Kammer-style "traction at split node" method): contact/friction tractions are computed from an impedance-based prediction of the next-step gap and added to the residual. Enabled in the upstream `common` CMake preset (`AKANTU_TRACTION_AT_SPLIT_NODE_CONTACT=TRUE`). | traction_at_split_node_contact | IOHelper (text dumper) | `test_ntn_model` (gtest `test_ntn_basics.cc`), `test_ntn_friction` (gtest `test_ntn_friction.cc`) | MPI-aware (filtered element synchronizer, all-reduces) | Uses the current API (compiles at this commit). |
| TASN-001 | `NTNBaseContact` (common contact machinery) | `src/ntn_contact/ntn_base_contact{.hh,.cc,_inline_impl.hh}` | Per contact node ("slave") synchronized arrays: `slaves`, `normals`, `contact_pressure` (dim), `is_in_contact`, `lumped_boundary_slaves`, `impedance`; `global_contact_pressure` (+old). `updateInternalData()` = normals + lumped boundary (∫N over boundary elements whose nodes are all contact nodes, via boundary FE engine) + impedance. `computeContactPressure()`: sync `_cf_nodal`; a_new = f_m2a(f_int+f_ext)/m; predicted gap g = gₙ + Δt·vₙ + Δt²·aₙ,new − ½Δt²·aₙ,old (relative normal fields); contact iff g ≤ 0, then p = Z·g/(2Δt)·n else 0. `assembleGlobalContactPressure()`: f(slave) = −A_s·p. `savePreviousState/restorePreviousState` of the global pressure. Restart dump/read of all arrays; text dumper of nodal fields (displacement, mass, velocity, acceleration, forces, blocked_dofs, increment, normal, contact_pressure, is_in_contact, lumped_boundary_slave, impedance). `getNbNodesInContact()` (local/master, non-periodic-slave, all-reduced). `initParallel()` filters the element synchronizer to elements touching contact nodes (tags `_cf_nodal`, `_cf_incr`). | traction_at_split_node_contact | — | `test_ntn_model` (`TestNodesPairing`, `TestLumpedBoundary` tol 1e-11, `TestImpedance` tol 1e-11; meshes `ntn_test_2d.msh`, `ntn_test_3d.msh`, `data/material_ntn_model.dat`) | synchronizer filter + `SynchronizedArray` push/erase propagation | Requires time step set before computing pressure (assert). |
| TASN-002 | `NTNContact` (node-to-node pairs) | `src/ntn_contact/ntn_contact.{hh,cc}` | Pairs slave/master nodes of two element groups by projecting coordinates onto the plane normal to `surface_normal_dir` and matching within 0.1·(min slave spacing) (`pairInterfaceNodes`); periodic slaves excluded; `addSurfacePair(slave, master, dir)`, `addNodePairs`, `getNodePairs`. Normals = normalised sum of master boundary-element quad normals (current positions) at each master node. Impedance Z = 2/Δt / (A_m/m_m + A_s/m_s). Gap = relative normal field of current positions (slave − master); relative tangential field = (slave − master) minus normal part. Global pressure: f_m = +A_m p, f_s = −A_s p, made periodic-consistent. | traction_at_split_node_contact | — | `test_ntn_model`, `test_ntn_friction` | — | Normal computation documented as valid only for 1 quad point per boundary element. |
| TASN-003 | `NTRFContact` (node-to-rigid-flat) and `MIIASYMContact` (mode-II antisymmetric) | `src/ntn_contact/ntrf_contact.{hh,cc}`, `src/ntn_contact/mIIasym_contact.{hh,cc}` | NTRF: contact against a rigid plane given by `setReferencePoint(x,y,z)` and `setNormal(x,y,z)` (normalised; all node normals equal); nodes from `addSurface(group)` or `addNodes`; gap = (x − x_ref)·n; Z = 2/Δt/(A_s/m_s); relative fields = absolute nodal fields. MIIASYM: NTRF with Z halved and relative normal/tangential fields doubled (symmetric mode-II setup); `computeContactPressureInEquilibrium()`. | traction_at_split_node_contact | — | none | — | `addNodes(Array<UInt>&)` uses `UInt`. |
| TASN-004 | `NTNBaseFriction` (friction machinery) | `src/ntn_contact/ntn_base_friction.{hh,cc}`, `ntn_friction{.hh,_tmpl.hh}` (`NTNFriction<Law,Reg>`), `ntrf_friction{.hh,_tmpl.hh}` (`NTRFFriction<Law,Reg>`) | Per node arrays: `is_sticking`, `frictional_strength`, `friction_traction` (+old), `slip`, `cumulative_slip`, `slip_velocity` (dim), `slip_velocity_norm`, `friction_state`; global friction traction (+old). `updateSlip()`: relative tangential increment Δ; sticking nodes reset slip, else slip += ‖Δ‖, cumulative += ‖Δ‖, v_slip = Δ/Δt. `computeFrictionTraction()`: stick traction τ_stick = Z/2·(v_t + Δt a_t,new − ½Δt a_t,old) (relative tangential fields) for nodes in contact; then strength from the law; if ‖τ‖ > strength scale τ to the strength (slipping) else sticking. Global: NTN f_m += A_m τ, f_s −= A_s τ; NTRF/base f_s = −A_s τ. `setParam(name, node, value)` per-node parameter edit, `getNbStickingNodes`, restart, dump fields, save/restore. | traction_at_split_node_contact | — | `test_ntn_friction` | synchronized arrays | `addNoiseToStateVariable` in the base class calls itself recursively (infinite recursion if not overridden). |
| TASN-005 | Friction laws: `coulomb`, `linear_slip_weakening`, `linear_slip_weakening_no_healing`, `linear_cohesive` | `src/ntn_contact/friction_laws/ntn_friclaw_{coulomb,linear_slip_weakening,linear_slip_weakening_no_healing,linear_cohesive}{.hh,_tmpl.hh}`; registration in `ntn_initiation_function.cc:67-78` | Coulomb: strength = μ·p_fric (0 if not in contact), per-node param `mu`. Linear slip weakening: μ = μ_s if sticking, else μ_k + (1 − s/d_c)(μ_s − μ_k) for s < d_c, μ_k beyond (s = current slip, reset when sticking); params `mu_s`, `mu_k`, `d_c`. No-healing variant: same with cumulative slip and no sticking reset. Linear cohesive: strength = max(τ_c − (τ_c − τ_r)²/(2G_c)·s_cum, τ_r) (τ_r if G_c = 0); params `G_c`, `tau_c`, `tau_r`. All parameters are per-node synchronized arrays (`_pat_parsmod`). | traction_at_split_node_contact | — | `test_ntn_friction` (`TestCompression` with `material_ntn_friction_slip_weakening.dat`, compares pressure history) | — | — |
| TASN-006 | Rate-and-state friction family | `friction_laws/ntn_friclaw_rate_and_state{.hh,_tmpl.hh}`, `ntn_friclaw_rate_and_state_flashheating.hh` | Params `mu_0`, `a`, `b`, `v_star`, `phi_star`, `friction_state` (θ, per node), `d_c`, `v0`. Coefficient functions: Dieterich μ₀ + a ln(V/V*) + b ln(θ/θ*); revised Dieterich μ₀ + a ln(1+V/V*) + b ln(θ/θ*)·μ₀; regularised Dieterich μ₀ + a ln(1+V/V*) + b ln(1+θ/θ*)·μ₀; n-shaped (1 + b ln(1+θ/θ*))(μ₀/√(1+(V/V*)²) + a ln(V/V*)); revised n-shaped (1 + b ln(1+θ/θ*))(μ₀/√(1+(V*/V)²) + a ln(1+V/V*)). State laws (explicit Euler, only when slipping): ageing θ += (1 − θV/d_c)Δt; slip θ += −θV/d_c·ln(θV/d_c)Δt; regularised ageing θ += (1 − θ√(V²+V₀²)/d_c)Δt. Keywords `rate_and_state:<coef>:<ageingabs(slip)ageing_regularized>` for coef ∈ {dieterich, revised_dieterich, regularized_dieterich, n_shaped, revised_n_shaped}, plus `rate_and_state_n_shaped_{ageing,slip,ageing_regularized}`. Flash heating (`rate_and_state:flashheating`, params `mu_w`, `v_w`): stick μ = μ₀; slip with V < V*: θ = d_c/V*, μ = μ₀ + b ln(θ/θ*); else ageing update, μ_RS (Dieterich), μ = μ_w + (μ_RS − μ_w)/(1 + d_c/(θ v_w)). `addNoiseToStateVariable(map)`: θ *= (1 + perturbation(lower_bound(x))). | traction_at_split_node_contact | — | `test_ntn_friction` param suite `TestRateAndState` over 15 `data/material_ntn_friction_rate_and_state_*.dat` (dieterich/revised/regularized × ageing/slip/ageing_regularized, n_shaped/revised_n_shaped × 3), mesh `ntn_test_ras.geo`; checks base velocity −V/2 tol 1e-10 | — | ln of zero slip velocity when sticking is avoided by evaluating only the coefficient (Dieterich with V=0 would be −∞ — the sticking branch still calls the coefficient function: risk for V = 0). |
| TASN-007 | Friction regularisations | `src/ntn_contact/friction_regularisations/ntn_fricreg_{no_regularisation,rubin_ampuero,simplified_prakash_clifton}.{hh,cc}` | No regularisation: p_fric = ‖p‖ in contact else 0 (param `frictional_contact_pressure`). Rubin–Ampuero: p_fric ← (p_fric + α‖p‖)/(1+α), α = Δt/t* (`t_star`, per node; t* = 0 → ‖p‖). Simplified Prakash–Clifton: regularises the strength itself: s ← (s + α s_spc)/(1+α) with internal `spc_internal`. Factory option (`friction <law> <regularisation> [ … ]`): `rubin_ampuero`, `simplified_prakash_clifton`, anything else (incl. default `no_regularisation`) → **Rubin–Ampuero** (`ntn_initiation_function.hh:58-71`). | traction_at_split_node_contact | — | `test_ntn_friction` | — | Quirk: `NTNFricRegNoRegularisation` is unreachable through the factory. |
| TASN-008 | Friction factory / input parsing | `src/ntn_contact/ntn_initiation_function.{hh,cc}` | `NTNFactory` keyed by law name; `registerFriction<Law>(name)` dispatches on `contact.isNTNContact()` (NTN vs NTRF friction) and regularisation option. `initializeNTNFriction(contact)` reads the **first** `friction` section of the global parser (warning if more), allocates and parses it; overload with explicit law/regularisation strings declared. | traction_at_split_node_contact | — | `test_ntn_friction` | — | Explicit-string overload is declared but not defined in the `.cc` (only the parser-based one). |
| TASN-009 | `NTNContactSolverCallback` | `src/ntn_contact_solvercallback.{hh,cc}` | `InterceptSolverCallback` around the SMM: ctor creates `NTNContact`, `initFull(_explicit_lumped_mass)`, lumped mass, `addSurfacePair(slave, master, dir)`, normals, lumped boundary, Δt = stable Δt × `time_step_factor`, impedance, friction from input. `assembleResidual()`: accumulate external work of contact/friction forces (2-vector, −f·Δu), SMM residual, contact pressure, slip update, friction traction, assemble both global arrays into the residual. `beforeSolveStep`/`afterSolveStep` save/restore contact & friction state. `getExternalWork()` (all-reduced). | traction_at_split_node_contact | — | `test_ntn_friction` | all-reduce | Integration point between TASN and the SMM explicit solver. |
| TASN-010 | Boundary-condition helpers | `src/boundary_conditions/{force_based_dirichlet,spring_bc,inclined_flat_dirichlet}.hh` | `ForceBasedDirichlet(model, axis, target_force, mass)`: rigid-body displacement increment of sub-boundaries driven by (target force + integrated residual): a = F/m, velocity-Verlet half steps, value = vΔt + ½aΔt² applied as `IncrementValue`. `SpringBC(model, axis, stiffness, mass)`: target force = −k·elongation, elongation += value; `setToEquilibrium`, `incrementElongation`. `InclinedFlatDirichlet(value, axis, incl_axis, center, tang)`: u_axis = value + tang·(x_incl − center). | traction_at_split_node_contact | — | none | residual integration all-reduced via `integrateResidual` | — |
| TASN-011 | Utility functions & synchronized arrays | `src/functions/boundary_functions.{hh,cc}` (`integrateResidual(sub_boundary, model, dir)`, `boundaryFix(mesh, names)` — creates missing sub-boundaries on all ranks), `src/functions/node_filter.hh` (`GeometryFilter`, `FilterPositionsGreaterThan/LessThan`), `src/common/synchronized_array{.hh,.cc,_inline_impl.hh}` (`SynchronizedArray<T>`: arrays kept in sync on push_back/erase with registered depending arrays, default values, restart files), `src/common/parameter_reader.{hh,cc}` (key/value input reader with typed maps: string, int, uint, real, bool, element type), `src/common/manual_restart.{hh,cc}` (`dumpArray/loadArray`, `dumpRestart/loadRestart` of SMM nodal fields) | Support code for TASN simulations. | traction_at_split_node_contact | — | indirect | `SynchronizedArray::syncElements(_added/_deleted)` | Text restart formats are package-specific. |
<!-- MAIN-TABLE-END -->

## 2. Constitutive-law specifications

Units: Akantu is unit-agnostic (all parameters in one consistent system).
"hist." = the internal keeps previous values (`initializeHistory()`), i.e.
`x_prev` is the value saved at `beforeSolveStep` (start of the step) — or at
`onDamageIteration` for event-driven damage loops. All laws are instantiated
for dim 1, 2, 3 unless stated. Per-quad-point kernels receive `grad_u`
(already minus `eigen_grad_u`), `sigma` (or `S` when `finite_deformation`),
`previous_grad_u`, `previous_sigma`. Common parameters inherited by every
material: `name` (string, required), `rho` (Real, 0, parsable+modifiable),
`finite_deformation` (bool, false, parsable+readable), `eigen_grad_u`
(dim×dim matrix, parsable), `inelastic_deformation` (internal).

### 2.1 `elastic` (`MaterialElastic<dim>`, base `MaterialThermal<dim>` → `PlaneStressToolbox<dim>` → `Material`)

Files: `src/model/solid_mechanics/materials/material_elastic.{hh,cc}`, `material_elastic_inline_impl.hh`, `material_thermal.{hh,cc}`, `plane_stress_toolbox{.hh,_tmpl.hh}`.

| Param | Type | Default | Flags | Notes |
|---|---|---|---|---|
| `E` | Real | 0 | parsable, modifiable | Young's modulus (`material_thermal.cc:38`) |
| `nu` | Real | 0.5 | parsable, modifiable | Poisson ratio; forced to 0 in 1D at `initMaterial` (`material_elastic.cc:44-46`) |
| `alpha` | Real | 0 | parsable, modifiable | thermal expansion |
| `delta_T` | InternalField (Real) | 0 | parsable, modifiable | uniform ΔT default value of internal `delta_T` |
| `Plane_Stress` | bool | false | parsmod | 2D only (`plane_stress_toolbox_tmpl.hh:44`) |
| `lambda`, `mu`, `kapa` | Real | computed | readable | λ, μ, κ |

Internals: `delta_T` (1), `sigma_th` (1, hist.), `epsilon_th` (1, hist.),
`third_axis_deformation` (1, 2D only, default 1; unused by `elastic`), plus base
`stress`, `grad_u` (hist.), `eigen_grad_u`, `potential_energy`.

Derived parameters (`material_elastic.cc:53-78`), recomputed on every parameter update:
λ = νE/((1+ν)(1−2ν)), μ = E/(2(1+ν)), κ = λ + 2μ/3; 2D plane stress: λ = νE/((1+ν)(1−ν)) (κ uses that λ). Any update sets `was_stiffness_assembled=false`.

Thermal pre-step (`MaterialThermal::computeStress`, called first by `MaterialElastic::computeStress`, `material_thermal.hh:100-128`):
* dim 3 and 2D plane strain: ε_th = ΔT·α·(1+ν); σ_th = −E/(1−2ν)·ΔT·α.
* 2D plane stress: ε_th = ΔT·α; σ_th = −E/(1−ν)·ε_th.
* 1D: ε_th = ΔT·α; σ_th = −E·ε_th.

Stress (`material_elastic_inline_impl.hh:31-63`):
* dim ≥ 2: σ = 2μ·½(∇u+∇uᵀ) + (λ·tr(∇u) + σ_th)·I.
* dim 1: σ₁₁ = E·∇u₁₁ + σ_th.
* `finite_deformation`: the same kernel with ∇u replaced by E_GL; result stored as S (2nd PK) — St-Venant–Kirchhoff.

Tangent (`:66-108`): 1D `[E]`; 2D `[[λ+2μ, λ, 0],[λ, λ+2μ, 0],[0, 0, μ]]`; 3D diag (λ+2μ)×3, off-diagonal normal λ, shear μ (Voigt engineering shear strain ⇒ shear modulus μ). Matrix type `_symmetric`. `hasStiffnessMatrixChanged = !was_stiffness_assembled`.

Energy: ψ = ½ σ:∇u (or ½ S:E_GL in finite deformation) (`:111-116`); per-element version via `computePotentialEnergyByElement(element,…)`.

Wave speeds: c_p = √((λ+2μ)/ρ), c_s = √(μ/ρ); 1D c_p = √(E/ρ), c_s throws (`material_elastic.cc:124-209`).

### 2.2 `elastic_anisotropic` (`MaterialElasticLinearAnisotropic<dim>`)

Files: `material_elastic_linear_anisotropic{.hh,.cc,_inline_impl.hh}`.
Params: `n1`…`n<dim>` (Vector, default unit axes, parsmod), `Cij` for 1 ≤ i ≤ j ≤ voigt_size (Real, 0, parsmod; when `symmetric=false` (never from input) all i,j). `rho`.
Update (`:65-156`): symmetrise C'; fill the extra rows/cols of the dim²×dim² C' by duplicating the shear rows/cols; rotation matrix R with **columns** = normalised n_j (`rot_mat(j) = n_j`, Akantu `Matrix::operator()(j)` is column j; corrected after review); check the axes (2D: only orthogonality/normalisation is verified — ‖n1×n2 − (n1×n2)/‖n1×n2‖‖ > 8ε ⇒ error — so left-handed axes are accepted, UD-107; parallel axes ⇒ error; 3D: ‖n1×n2 − n3‖ > 8ε ⇒ error); `rotator(I,J) = R(k,i)R(l,j)`, `revrotor(I,J) = R(i,k)R(j,l)` with I=mat[i][j], J=mat[k][l]; `C_full = rotator·C'·revrotor`, i.e. `C_ijkl = R_ai R_bj R_ck R_dl C'_abcd` — a rotation with Rᵀ, so the stiffness of material axis a acts along row a of R, not along n_a (UD-104, category B; the port uses R by default); C = top-left voigt block; `eigC = eig(C)` (unsorted real parts).
Stress: σ_voigt = C·[ε₁₁, ε₂₂, (ε₃₃,) 2ε₂₃, 2ε₁₃, 2ε₁₂]; σ = voigtToMatrix(σ_voigt). Tangent = C. Energy ½σ:∇u (debug assert symmetric). Celerity = √(eigC(0)/ρ). No thermal, no plane stress handling (2D C used as given).

### 2.3 `elastic_orthotropic` (`MaterialElasticOrthotropic<dim>`)

Params `E1, E2, nu12, G12` (Real, 0, parsmod); 3D also `E3, nu13, nu23, G13, G23`; axes `n1..` and `Cij` inherited (Cij overwritten). 1D: `AKANTU_ERROR`.
ν21 = ν12E2/E1, ν31 = ν13E3/E1, ν32 = ν23E3/E2.
3D: Γ = 1/(1 − ν12ν21 − ν23ν32 − ν31ν13 − 2ν21ν32ν13); C'11 = E1(1−ν23ν32)Γ, C'22 = E2(1−ν13ν31)Γ, C'33 = E3(1−ν12ν21)Γ, C'12 = E1(ν21+ν31ν23)Γ, C'13 = E1(ν31+ν21ν32)Γ, C'23 = E2(ν32+ν12ν31)Γ, C'44 = G23, C'55 = G13, C'66 = G12.
2D (plane stress, as ν13=ν23=0): Γ = 1/(1−ν12ν21), C'11 = E1Γ, C'22 = E2Γ, C'12 = E1ν21Γ, C'33 = G12.
Then rotation + eig as in 2.2 (`material_elastic_orthotropic.cc:73-128`).

### 2.4 `MaterialDamage<dim, Parent = MaterialElastic>` (base for isotropic damage)

Internals `damage` (1, hist.), `damage dissipated energy` (1), `integral of sigma` (1).
Tangent: D_damaged = (1 − d)·D_parent (`material_damage_tmpl.hh:66-87`).
Energies after converged step (`:44-62`): Δint = ½(σ+σ_prev):(∇u−∇u_prev); ∫σ += Δint; e_diss = ∫σ − ψ (ψ recomputed by `computePotentialEnergy`). `getEnergy("dissipated") = Σ ∫ e_diss`.

### 2.5 `marigo` (`MaterialMarigo<dim>`)

| Param | Type | Default | Flags | Notes |
|---|---|---|---|---|
| `Yd` | RandomInternalField | 0 (base) | parsable | damage threshold energy, may carry a distribution |
| `Sd` | Real | 5000 | parsable, modifiable | damage softening |
| `epsilon_c` | Real | 0 | parsable | critical strain; Yc = ½·E·ε_c² |
| `damage_in_y` | bool | false | parsable | use (1−d)Y |
| `Yc limit` | bool | false | internal | set true iff abs(ε_c) > ε_machine |

Internals: `Y` (1), `Yd` (1, random), + damage base. Kernel (`material_marigo_inline_impl.hh:31-78`):
σ_el from 2.1 (thermal σ_th not refreshed); Y = ½ σ_el:ε; if `damage_in_y`: Y ← (1−d)Y (d = current iterate); if Yc limit: Y ← min(Y, Yc);
local: F = Y − Yd − Sd·d_prev; if F > 0: d = (Y − Yd)/Sd; d ← max(d, d_prev); d ← min(d, 1); σ = (1−d)σ_el.
Non-local variant (if ever reached): Y replaced by averaged `Y non local` before the damage update.

### 2.6 `mazars` / `plastic_mazars` (`MaterialMazars<dim, Parent>`)

| Param | Type | Default | Flags |
|---|---|---|---|
| `K0` | RandomInternalField | 0 | parsable |
| `At` | Real | 0.8 | parsable |
| `Bt` | Real | 12000 | parsable |
| `Ac` | Real | 1.4 | parsable |
| `Bc` | Real | 1900 | parsable |
| `beta` | Real | 1.06 | parsable |
| `average_on_damage` (non-local only) | bool | false | parsable, modifiable |

Kernel (`material_mazars_inline_impl.hh:54-170`):
1. σ = Parent stress (elastic, or J2 return mapping for `plastic_mazars`).
2. If args contain `inelastic_strain` (plastic parent): ∇u ← ∇u − ε_p (restored at the end).
3. ε₃ₓ₃ = embed(sym(∇u)) (zeros out of plane); principal strains ε_i (Eigen `EigenSolver`, unsorted).
4. ε̂ = √(Σ_i ⟨ε_i⟩₊²).
5. If computing damage locally (`damage_in_compute_stress`, true for local `mazars`): if ε̂ − K0 > 0:
   d_t = 1 − K0(1−At)/ε̂ − At·exp(−Bt(ε̂−K0)); d_c = 1 − K0(1−Ac)/ε̂ − Ac·exp(−Bc(ε̂−K0));
   C_d = E(1−ν)/((1+ν)(1−2ν)); σ̃_i = C_d ε_i + λ(ε_j + ε_k) (λ = current Lamé, plane-stress λ in 2D plane stress);
   σ̃⁺_i = max(0, σ̃_i); tr⁺ = ν/E Σσ̃⁺; ε_t,i = (1+ν)/E σ̃⁺_i − tr⁺;
   α_t = min(1, Σ_i ε_t,i ⟨ε_i⟩₊ / ε̂²); α_c = 1 − α_t; α_t ← α_t^β, α_c ← α_c^β;
   d ← min(max(α_t d_t + α_c d_c, d), 1)  (max with the **current** d, not d_prev).
6. σ ← (1 − d)σ (if local).
Non-local (`mazars_non_local`, `plastic_mazars_non_local`): `average_on_damage=false` ⇒ ε̂ stored in internal `Ehat` is averaged and step 5 runs in the non-local pass with averaged ε̂ (principal strains recomputed locally); `true` ⇒ local damage computed in step 5, averaged d used in step 6.

### 2.7 `phasefield` (`MaterialPhaseField<dim, Split>`; option `no_split` | `volumetric_deviatoric`)

Params: `eta` (Real 0, parsable), `is_isotropic` (bool false, parsable/readable, no effect in stress), `degrade_mass` (bool false). Inherits elastic + damage params/internals. `initMaterial` builds the split with (E, ν, plane_stress) — split λ, μ use the same formulas as 2.1 (`src/model/phase_field/energy_split.cc:9-18`).
Stress (after `MaterialThermal::computeStress`): g = (1−d)² + η; ε = sym(∇u) (E_GL if finite def.); σ = g·σ⁺ + σ⁻ with
* no split: ε' = ε − ε_th I; σ⁺ = 2μ ε' + λ tr(ε') I; σ⁻ = 0.
* vol/dev: ε' = ε − ε_th I, t = tr ε', κ = λ + 2μ/3, dev = ε' − t/3·I (trace/3 even in 2D); σ⁺ = κ⟨t⟩₊ I + 2μ dev; σ⁻ = κ⟨t⟩₋ I.
Tangent: no split: g·D_iso (1D: gE); vol/dev: g_h = g if tr(ε) > 0 else 1 (ε without thermal), M_iiii = g_h κ + g·(4/3)μ, M_iijj = g_h κ − g·(2/3)μ, M_ijij = g μ (1D: g_h E).
Energy density used by the phase-field model (R3): no split ψ = ½λ tr² + μ ε:ε; vol/dev ψ⁺ = ½κ⟨tr⟩₊² + μ dev:dev.
Mass: if `degrade_mass`, ρ_q = min(ρ, ρ·((1−d)²+η)); `hasMassMatrixChanged = degrade_mass`.
Damage `d` (internal `damage`) is written by `CouplerSolidPhaseField` (R3), not computed here.

### 2.8 `anisotropic_damage` (`MaterialAnisotropicDamage<dim, EquivStrain, DamageThreshold>`)

Options: `mazars` (default, `EquivalentStrainMazars` + `DamageThresholdTan`), `mazars-drucker-prager` (`EquivalentStrainMazarsDruckerPrager` + Tan). Params: `Dc` (0.99), `a` (2.93e-4), `A` (5e3), `K0` (5e-5), and `k` (0) for the DP variant; all parsable (no defaults passed to `registerParam` — member initialisers apply).
Internals: `damage` (dim², **no history**), `elastic_stress` (dim²), `equivalent_strain` (1), `trace_damage` (1, hist.).
Kernel (`material_anisotropic_damage_tmpl.hh:176-262`), with `TrD` = current internal, `TrD_n_1` = **previous** slot:
1. σ̃ = elastic stress (2.1, written into `sigma`… then `sigma_el` is a separate internal that is never written by the elastic kernel — the elastic kernel writes `args["sigma"]`); ε = sym ∇u; ε̂ = f_eq(ε): Mazars f = √(Σ⟨ε_i⟩₊) (sum of positive eigenvalues, square-rooted); DP: f + k·tr ε.
2. K(TrD) = a·tan(atan2(TrD, a) − atan2(K0, a)); if ε̂ − K > 0: TrD_tmp = K⁻¹(ε̂) = a·A·(atan2(ε̂, a) − atan2(K0, a)); ε⁺ = Σ⟨ε_i⟩₊ n_i⊗n_i; Δλ = (TrD_tmp − TrD)/ε̂²; D_tmp = D + Δλ ε⁺.
3. If D_tmp updated and tr D_tmp > Dc: tensile (tr ε > 0) → κ_b = (1−Dc)κ, set material-wide E = 9κ_b(κ_b−λ)/(3κ_b−λ), ν = λ/(3κ_b−λ), `updateInternalParameters()`, recompute step 1–2; deviatoric (|tr ε| < 1e-10) → computes a clipped spectral version but discards it.
4. TrD_n_1 ← TrD_tmp (if updated) else TrD; D ← D_tmp.
5. σ = √(1−D)·σ̃_el·√(1−D) − ((1−D):σ̃_el)/(dim − TrD_n_1)·(1−D) + (1/dim)·((1−TrD_n_1)·tr⁺σ̃_el − tr⁻σ̃_el)·I, with tr± = sums of positive/negative eigenvalues of `sigma_el` (`damageStress`, `:156-173`).
Notes: as coded, `sigma_el` (internal `elastic_stress`) is passed to `damageStress` but the elastic kernel writes into `sigma`; `elastic_stress` therefore stays 0 unless set elsewhere → σ evaluates with σ̃_el = 0. `tensorPlusOp` leaves off-diagonal entries of `A_diag` uninitialised. K and K⁻¹ are not mutual inverses. Treat as experimental; no upstream test.

### 2.9 `MaterialPlastic<dim>` (base of J2 and DP)

Params `h` (Real 0, parsable/modifiable), `sigma_y` (Real 0, parsable/modifiable). Internals: `iso_hardening` (1, hist.), `inelastic_strain` (dim², hist.), `plastic_energy` (1), `d_plastic_energy` (1).
`computeStressAndInelasticStrainOnQuad` (`material_plastic.hh:63-87`): σ = σ_prev + MaterialElastic(Δ∇u − Δε_p) with **no thermal term**; ε_p = ε_p,prev + Δε_p (Δ∇u = ∇u − ∇u_prev unless supplied).
Potential energy: ½ σ:(∇u − ε_p). After step: w_p = ½(σ+σ_prev):(ε_p − ε_p,prev); plastic_energy += w_p; `getEnergy("plastic")`.

### 2.10 `plastic_linear_isotropic_hardening` (small strain, `MaterialLinearIsotropicHardening<dim>`)

Sequence per element type (`material_linear_isotropic_hardening.cc:28-61`): `MaterialElastic::computeStress` (refresh σ_th, writes elastic σ — overwritten below) then per quad (`_inline_impl.hh:28-84`):
1. Δσ_th = σ_th,prev − σ_th (as coded).
2. σ_tr = σ_prev + [2μ sym(Δ∇u) + (λ tr Δ∇u + Δσ_th) I].
3. s = embed₃ₓ₃(σ_tr) − tr(σ_tr)/3·I₃; σ_eq = √(3/2 s:s).
4. yielding ⇔ σ_eq − r − σ_y > 0 with r = **current** `iso_hardening` value (from the previous Newton iterate, or r_prev at the first iterate since current = prev after the history copy).
5. Δp = yielding ? (σ_eq − σ_y − r_prev)/(3μ + h) : 0; r = r_prev + h·Δp.
6. Δε_p = (3/2)·Δp·s_dim/σ_eq if |σ_eq| > ‖s‖·`Math::getTolerance()` else 0.
7. σ = σ_prev + C:(Δ∇u − Δε_p) (2.9).
Tangent: elastic D (2.1). 2D uses plane-strain or plane-stress λ according to `Plane_Stress` but the deviator is always 3×3 with σ₃₃ = 0 (no plane-strain σ₃₃ tracking).

Finite-deformation branch: see MAT-021 (result overwritten by the small-strain kernel).

### 2.11 `plastic_mazars`: 2.6 with Parent = 2.10.

### 2.12 `plastic_drucker_prager` (`MaterialDruckerPrager<dim>`)

Params: `phi` (deg, 0), `fc` (1), `radial_return` (bool true); plus `h`, `sigma_y` (unused by DP yield). Derived (`material_drucker_prager.cc:41-49`): φ_r = φπ/180, α = 6 sinφ_r/(3 − sinφ_r), c = fc(1 − sinφ_r)/(2 cosφ_r), k = 6c cosφ_r/(3 − sinφ_r).
Yield: f(σ) = √(3 J2(σ)) + α tr σ − k, J2 = ½ dev(σ):dev(σ) with dev using trace/dim.
Per quad (`_inline_impl.hh:278-333`): Δσ_th = σ_th − σ_th,prev; σ_tr = σ_prev + elastic(Δ∇u, Δσ_th). If f(σ_tr) > 0, `computeGradientAndPlasticMultplier(σ_tr, …)` (max 100 iterations, tol 1e-10, `:56-272`):
* Apex branch if tr σ_guess ≥ k/α and α > 0: iterate σ ← σ − J2·dev(σ)⁻¹ until J2 ≤ tol, then σ ← σ − (α tr σ − k)/α·I until ≤ tol; Δε_p = Ce·voigt(σ_tr − σ) (Ce = De⁻¹, De = elastic Voigt tangent), return.
* Otherwise (with α', k' = 0, |α trσ_tr − k| if `radial_return` else α, k): residual f_vec = (σ_tr − σ)_v − De·∇f·Δλ, error = max(‖f_vec‖, |f|); Newton: Ξ = Ce + Δλ·H, H = −9/(4(3J2)^{3/2})·(Sv)(Sv)ᵀ + 3/(2√(3J2))·S·(I − δδᵀ/3) with S = diag(2,…,2 (normals), 1,… (shears)), ∇f = S·dev_v·3/(2√(3J2)) + α'δ; Δλ = (∇f·Ξ⁻¹Δε_p + f)/(∇f·Ξ⁻¹∇f); Δσ = (Ξ⁻¹ − Ξ⁻¹∇f∇fᵀΞ⁻¹/den)Δε_p − Ξ⁻¹∇f f/den.
* Shear components of Δε_p (Voigt) divided by 2 before conversion to a tensor.
Then σ = σ_prev + C:(Δ∇u − Δε_p). Tangent: elastic (inherited). Finite deformation: NotImplemented.

### 2.13 `sls_deviatoric` (`MaterialStandardLinearSolidDeviatoric<dim>`)

Params: `Eta` (Real 1, parsable/modifiable), `Ev` (Real 1, parsable/modifiable), `Einf` (readable, = E − Ev), + elastic params. Internals `stress_dev` (dim²), `history_integral` (dim²), `dissipated_energy` (1) — no history flags.
Per call (`…deviatoric.cc:73-109`): τ = η/Ev, Δt = model time step, e₁ = exp(−Δt/τ), e₂ = exp(−Δt/(2τ)), γ∞ = E∞/E, γ_v = Ev/E; ε_d = ε − tr(ε)/3·I; s = 2μ ε_d; h ← e₁ h + e₂ (s − s_old); s_old ← s; σ = γ∞ κ tr(ε) I + γ∞ s + γ_v h.
Dissipation (`:112-140`): q = γ_v (s − h), q̇ = (γ_v s − q)/τ, α = 1/(2μγ_v); e_diss += sum_entries((ε_d − α q)·q̇·Δt) (matrix product).
Steady state: s = 2μ(sym ∇u − tr(∇u)/3 I), h = 0. Tangent: elastic (inherited, uses total E).

### 2.14 `viscoelastic_maxwell` (`MaterialViscoelasticMaxwell<dim>`)

Params: `Einf` (Real 1, parsable/modifiable), `Eta` (Vector, parsable/modifiable), `Ev` (Vector, parsable/modifiable), `previous_dt` (readable), + elastic params (`E` overwritten: E = Einf + ‖Ev‖₁ at `initMaterial`, before λ, μ are derived). Debug asserts sizes equal and small strain.
Internals: `sigma_v` (dim²·N), `epsilon_v` (dim²·N) (registered in `initMaterial`), `dissipated_energy`, `mechanical_work`.
Normalised stiffness C (E=1, `updateInternalParameters`, `…maxwell.cc:70-124`): pre = 1/((1+ν)(1−2ν)); C_iiii = pre(1−ν), C_iijj = pre·ν, C_ijij = pre·½(1−2ν) (1D C = 1); compliance D: D_iiii = 1, D_iijj = −ν, D_ijij = 2+2ν.
Stress (`:139-178`): σ_v = Einf·C·ε_v + Σ_k [E_k^eff·C·(ε_v − ε_v,prev) + e^{−Δt/λ_k}·voigt(σ_v,k)], λ_k = η_k/E_k, E_k^eff = E_k if e^{−Δt/λ_k} == 1 else (1−e^{−Δt/λ_k})E_k λ_k/Δt; σ = matrix(σ_v) + σ_th I.
Tangent (`:266-298`): E_eff = Einf + Σ E_k^eff; D_tangent = E_eff·C; stores `previous_dt = Δt`.
After converged step (if `update_variable_flag`, default true; `forceUpdateVariable`/`forceNotUpdateVariable`): `updateIntVarOnQuad` — as coded σ_v,k is **not modified**; ε_v,k = D·σ_v,k/E_k. Then energies: ψ = ½ (Einf C ε)·ε + Σ ½ σ_v,k:ε_v,k; work += ½(σ+σ_prev):(∇u−∇u_prev); e_diss = work − ψ.
`hasStiffnessMatrixChanged`: true iff Δt ≠ previous_dt (if equal returns false).

### 2.15 `neohookean` (`MaterialNeohookean<dim>`, base `PlaneStressToolbox<dim>`)

Params: `E` (0), `nu` (0.5) (own members, shadowing nothing in `Material`), readable `lambda`, `mu`, `kapa`; `Plane_Stress` (2D). `finite_deformation` forced true in the ctor. λ = νE/((1+ν)(1−2ν)) (always 3D formula, even in plane stress), μ = E/(2(1+ν)); ν=0 in 1D.
S = μ I + (λ ln J − μ) C⁻¹, C = FᵀF, J = det F·√C33. Tangent C_mn = λ C⁻¹_ij C⁻¹_kl + (μ − λ ln J)(C⁻¹_ik C⁻¹_jl + C⁻¹_il C⁻¹_kj) for Voigt m=(i,j), n=(k,l). Energy as coded ψ = λ(ln J)² + μ(−ln J + (tr C − dim)/2)/2 (J without √C33).
Plane stress C33 Newton (tol 1e-5, 100 it.) on f(x) = λ/2(ln x + ln(C11C22 − C12²)) + μ(x − 1), f' = λ/(2x) + μ, started from the stored C33 (default 1).
Wave speeds as 2.1 with the neohookean λ, μ. **Non-functional at this commit** (see MAT-025).

### 2.16 `reinforcement elastic` (`MaterialReinforcement<MaterialElastic<1>, dim>`) — see EMB-003

Params: `area` (Real 1.0, parsable/modifiable), `pre_stress` (internal, parsable/modifiable), + 1D elastic params (`E`, `rho`, …). Internals (interface FE engine `EmbeddedInterfaceFEEngine`): `gradu_embedded` (dim²), `pre_stress` (1), `directing_cosines` (voigt), base `stress`/`grad_u` are 1×1.
ε_s = C·ε_voigt(u_bg) with ε_voigt using engineering shears; σ_s = E·ε_s + σ_pre; f_int(bg nodes) −= A ∫ Bᵀ Cᵀ σ_s ds; K += A ∫ Bᵀ Cᵀ E C B ds.

### 2.17 Cohesive kinematics and the `MaterialCohesive` base (all cohesive laws)

* Element kinds: cohesive types `_cohesive_1d_2`, `_cohesive_2d_4`, `_cohesive_2d_6`, `_cohesive_3d_6`, `_cohesive_3d_12`, `_cohesive_3d_8`, `_cohesive_3d_16` (element classes in R1). A cohesive element has two facets (sides); its connectivity lists side-1 nodes then side-2 nodes.
* Normal n (per quad, `computeNormal`): normals of the mid-surface computed from **current positions** with `CohesiveReduceFunctionMean` (average of the two sides' nodal positions) — `material_cohesive.cc:311-333`.
* Opening δ (per quad, `computeOpening`): interpolation of `u(side 2) − u(side 1)` (`CohesiveReduceFunctionOpening`) — `:336-356`.
* Decomposition used by all laws: Δn = δ·n, δ_n = Δn n, δ_t = δ − δ_n.
* Internal force: per element, F_side1 = −∫ (t + t_c) N dS, F_side2 = +∫ (t + t_c) N dS (`:88-172`).
* Stiffness (`:175-277`): K_e = ∫ (N A)ᵀ Dᵀ (N A) dS with A = [I | −I] (`dim·n_side × dim·2n_side`), D = tangent (dim×dim row-major per quad).
* History: `opening`, `tractions` with previous values (saved at `beforeSolveStep`). `delta max`: history requested by derived laws but never enabled (COH-010 risk).
* Energies: trapezoidal total energy, reversible ½t·δ, dissipated = total − reversible, contact ½t_c·δ_c.
* Common params: `sigma_c` (FacetRandomInternalField, parsable/readable; random distributions allowed, e.g. `sigma_c = 300e6 uniform [0, 30e6]`), `delta_c` (Real 0, parsable/readable).

### 2.18 `cohesive_linear` (`MaterialCohesiveLinear<dim>`)

| Param | Type | Default | Flags | Notes |
|---|---|---|---|---|
| `sigma_c` | FacetRandomInternalField | 0 | parsable, readable | facet strength (insertion threshold) |
| `delta_c` | Real | 0 | parsable, readable | if ≠ 0 used instead of 2G_c/σ |
| `beta` | Real | 0 | parsable, readable | shear/normal weighting |
| `G_c` | Real | 0 | parsable, readable | fracture energy |
| `penalty` | Real | 0 | parsable, readable | compressive penalty stiffness |
| `volume_s` | Real | 0 | parsable, readable | reference volume for σ_c size scaling (0 = off) |
| `m_s` | Real | 1 | parsable, readable | Weibull exponent for scaling |
| `kappa` | Real | 1 | parsable, readable | κ in β²/κ |
| `contact_after_breaking` | bool | false | parsable, readable | keep penalty contact when d = 1 |
| `max_quad_stress_insertion` | bool | false | parsable, readable | insertion criterion uses max instead of mean over facet quads |
| `recompute` | bool | false | parsable, modifiable | unused flag (legacy load reduction) |

Derived (`updateInternalParameters`, `:107-118`): β²/κ², β²/κ, β⁻² (0 if β = 0).
Internals: `sigma_c_eff` (CohesiveRandomInternalField), `delta_c_eff` (default δc if ≠ 0 else 2G_c/σ_c(base) — `initMaterial`, `:87-104`), `insertion_stress` (dim).
Traction (`_inline_impl.hh:68-151`), per quad:
1. Δn = δ·n; δ_n = Δn n; δ_t = δ − δ_n; Δt = ‖δ_t‖.
2. δ_eff² = (β²/κ²) Δt²; penetration ⇔ Δn/δc_eff < −tol (`Math::getTolerance()`), forced false if d == 1 (float-equal) and not `contact_after_breaking`.
3. If penetration: t_c = penalty·δ_n, δ_c(contact opening) = δ_n, `opening ← δ_t` (the stored opening loses its normal part), δ_n ← 0. Else δ_eff² += Δn², t_c = 0, contact opening = 0.
4. δ_eff = √; δ_max = max(δ_max, δ_eff); d = min(δ_max/δc_eff, 1).
5. d == 1 → t = 0; d == 0 → t = 0 if penetration else `insertion_stress`; else t = ((β²/κ) δ_t + δ_n)·σc_eff/δ_max·(1 − d).
Tangent (`:154-245`), per quad: opening ← opening + contact_opening; recompute δ_n, δ_t, Δt; penetration ⇔ Δn < 0 (same broken rule); if penetration: D = penalty·(D + n⊗n) (D starts at 0), opening ← tangential part, δ_n recomputed (= 0); δ_eff as above; if δ_eff < tol: δ_eff = δc/1000; with σ = **base** facet `sigma_c`: δ_eff ≥ δ_max: (δ_eff ≤ δc: dT/dδ·(1/δ)… `derivative = −σ/δ²`, t = σ(1−δ/δc)) else (0, 0); δ_eff < δ_max: t = σ(1−δ_max/δc)/δ_max·δ_eff, derivative 0. nn = ((1−β²/κ) n⊗n + (β²/κ) I)·t/δ; t̃ = (1−β²/κ²) δ_n + (β²/κ²)·opening; t̂ = δ_n + (β²/κ) δ_t; D += (t̂ ⊗ t̃ · derivative/δ + nn)ᵀ.
Insertion check (extrinsic, `material_cohesive_linear.cc:172-305`): for every facet of the material's facet filter with `check_facets` true: per facet quad, σ̄ = ½(σ_side1 + σ_side2) (from `facet_stress`), t_n = σ̄ n, σ_eff = √(⟨t_n·n⟩₊² + β⁻²·Σ_k (t_n·τ_k)²) (τ_k facet tangents: 1 in 2D, 2 in 3D); criterion value = mean_q σ_eff (or max); if > the σ_c value paired by `zip(facet_filter, sigma_c(type_facet))` (σ_c has one entry per facet quad point, so for facets with >1 quad point the k-th facet is compared with the k-th quad entry — misaligned; exact for linear facets with one quad point) → `insertion_facets(facet)=true` and, unless `check_only`, append per quad: σc_eff = σ_eff,q; δc_eff = 2G_c/σ_eff,q (δc = 0) or σ_c·δc/σ_eff,q; insertion stress = t_n,q (×−1 if dim ≠ 3); previous traction = same. New entries are appended in facet-filter order **before** the inserter creates the elements (so material-local order must match the insertion order).
Size scaling (`scaleInsertionTraction`, only if `volume_s ≠ 0`, at init): σ_c(facet) ← (σ_c − b)·(volume_s/V)^{1/m_s} + b with V = total volume of the facet's neighbour elements and b an uninitialised vector (reference bug).

### 2.19 `cohesive_bilinear` (`MaterialCohesiveBilinear<dim>`)

Param `delta_0` (Real 0, parsable/readable) + all of 2.18. `initMaterial`: σc_eff distribution := σ_c distribution, `delta_max` default = δ0, `insertion_stress` default 0 (both reset). New (non-ghost) cohesive elements: optional scaling `σ ← (σ − σ̄)(volume_s/V)^{1/m_s} + σ̄` (σ̄ = base σc_eff, V = volume of bulk elements around both facets), then per quad δc = 2G_c/σ, error if δc − δ0 < tol, σ ← σ·δc/(δc − δ0). Traction = 2.18 (which uses d = δ_max/δc), then stored damage d = clamp((δ_max − δ0)/(δc − δ0), 0, 1).

### 2.20 `cohesive_exponential` (`MaterialCohesiveExponential<dim>`)

Params: `beta` (0, parsable), `exponential_penalty` (true), `contact_tangent` (1.0; forced to 1 when `exponential_penalty`), `sigma_c`, `delta_c`. Uses σ = base `sigma_c`, δc = `delta_c`.
Traction: Δn, δ_t, Δt as usual; δ = √(β²Δt² + Δn²). If Δn < 0 and abs(Δn) > tol: coupled traction with δ = βΔt and opening δ_t, then add compressive term n·Δn·e·σ·exp(−Δn/δc)/δc (exp. penalty) or n·contact_tangent·e·σ·Δn/δc. Coupled traction: if |δ| < tol → 0; δ_max_new = max(δ_max_prev, δ); t = (β² δ_vec + (1−β²)(δ_vec·n) n)·e·σ·exp(−δ_max_new/δc)/δc.
Tangent: J = β²I; δ_e = (1−β²)(δ·n)n + β²δ; D = ((1−β²)n⊗n + J − δ_e⊗δ_e/(δ δc))·exp(1 − δ/δc)·σ/δc (δ floored at tol); compression adds n⊗n·contact_tangent·e·σ·exp(−Δn'/δc)(1 − Δn'/δc)/δc with Δn' = Δn (exp. penalty) or 0.

### 2.21 `cohesive_linear_fatigue` (`MaterialCohesiveLinearFatigue<dim>`)

Params: `delta_f` (Real −1 ⇒ default δc_eff base; error if 0 ≤ δ_f < δc), `progressive_delta_f` (false; δ_f := δ_max per quad), `count_switches` (false), `fatigue_ratio` (1) + 2.18. Internals: `delta_prec`, `K_plus`, `K_minus`, `T_1d`, `normal_regime` (bool), optional `switches` (Int), `delta_dot_prec`.
Per quad (`material_cohesive_linear_fatigue.cc:84-293`): kinematics and penetration (Δn < −tol) as 2.18 (penalty contact, opening ← δ_t); δ_max, d as 2.18; δ̇ = δ − δ_prec; switch counting when δ̇ changes sign; d == 1 → t = 0; d == 0 → t = 0 (penetration) or insertion stress, T₁d = σc; otherwise if δ ≤ tol → t = 0, else if |δ̇| > tol: loading (δ̇ > 0): if not normal regime: K⁺ ← K⁺(1 − δ̇/δ_f); T₁d += K⁺δ̇; T₁d ← min(T₁d, σc(1 − δ/δc)) (flag); if δ_max > fatigue_ratio·δc: δ_max ← σc/(T₁d/δ + σc/δc), d = min(δ_max/δc, 1), K⁻ = σc/δ_max(1−d), normal regime = true; else K⁻ = T₁d/δ and if capped K⁺ = K⁻; in normal regime K⁻ = σc/δ_max(1−d). Unloading (δ̇ < 0, fatigue regime): K⁺ += (K⁺ − K⁻)δ̇/δ_f; T₁d = K⁻δ. Then t = ((β²/κ)δ_t + δ_n)·K⁻ (only when |δ̇| > tol). Finally δ_prec = δ.

### 2.22 `cohesive_linear_friction` (`MaterialCohesiveLinearFriction<dim>`)

Params: `mu` (0), `penalty_for_friction` k_f (0) + 2.18. Internals: `residual_sliding` (1, hist.), `friction_force` (dim).
Traction: if the solver is not explicit, `delta_max ← delta_max.previous` first; per quad: 2.18 traction; if penetration: τ_max = μ·penalty·abs(Δn) (Δn of the current call), s = ‖δ_t‖ − s_res,prev, τ = sign(s)·min(k_f|s|, τ_max), f = τ δ_t/‖δ_t‖, s_res = ‖δ_t‖ − abs(τ)/k_f (or ‖δ_t‖ if k_f = 0), t += f; else f = 0.
Tangent: 2.18 tangent; if penetration: τ_max from min(δ_prev·n, 0); if τ < τ_max and τ_max > tol: D += k_f(I − n⊗n).

### 2.23 `cohesive_linear_uncoupled` (`MaterialCohesiveLinearUncoupled<dim>`)

Param `roughness` R (Real 1) + 2.18. Internals `delta_n_max` (hist.), `delta_t_max` (hist.), `damage_n`, `damage_t`.
Traction: δ_n,eff² = (β²/κ²)Δt² (+Δn² if no penetration); δ_t,eff² = (β²/κ²)Δt² (+Δn²δc²/R² if no penetration); penetration ⇔ Δn < 0 (unless d_n == 1 and not contact_after_breaking) → t_c = penalty·δ_n, δ_n vector zeroed (opening not modified). Maxima/damages: d_n = min(δ_n,max/δc, 1), d_t = min(δ_t,max/δc, 1). t_n: 0 (d_n = 1), insertion stress or 0 (d_n = 0), else δ_n σc/δ_n,max (1 − d_n). t_t: 0 if d_t ∈ {0, 1} else (β²/κ)δ_t σc/δ_t,max (1 − d_t). t = t_n + t_t.
Tangent: opening += contact_opening; uses previous maxima; normal part like 2.18 with δ_n (penalty n⊗n on penetration); shear part: I_t = (I − n⊗n)·T/δ_t; Δ̃ = (δc²/R² − β²/κ²)δ_n + (β²/κ²)δ; Δ̂ = (β²/κ)δ_t; D += (Δ̂⊗Δ̃·derivative/δ_t + I_t)ᵀ.

### 2.24 Extra-package constitutive laws (XMAT / IGFEM / TASN)

These packages are legacy/optional; equations are given in the corresponding
main-table rows (single source of truth to avoid divergence). Parameter
tables (all `.dat`-parsable unless stated; defaults from `registerParam`):

| Law (keyword) | Row | Parameters (name = default [flags]) | Internals (components, history) |
|---|---|---|---|
| `damage_iterative` | XMAT-001 | `Sc` (RandomInternalField, no default), `prescribed_dam` = 0.1 [parsmod], `dam_threshold` = 0.8 [parsmod], `dam_tolerance` = 0.01 [parsmod], `max_damage` = 0.99999 [parsmod], `max_reductions` = 10 (UInt) [parsmod] + elastic | `Sc` (1), `damage_step` (UInt, 1), `equivalent_stress` (1), `damage` (1, hist.) and MaterialDamage energies; uses previous stress/grad_u |
| `damage_iterative_non_local` | XMAT-002 | as above (+ `non_local <name> <wf>` section) | + `grad_u non local` (dim²) |
| `iterative_stiffness_reduction` | XMAT-003 | `Gf` (no default) [parsmod], `crack_band_width` (no default) [parsmod], `reduction_constant` = 2 [parsmod] + XMAT-001 | + `ultimate_strain` (1), `tangent` (1) |
| `damage_linear` | XMAT-004 | `Sigc` = 1e5, `Gc` = 2 + elastic | `K` (1, default ε_min) + damage |
| `brittle` | XMAT-005 | `S_0` = 157e6 [parsmod], `E_0` = 27e3, `A` = 1.622e-5, `B` = −1.3274, `C` = 3.6544e4, `D` = −181.38e6 + elastic | `strain_rate_brittle` (dim²) + damage |
| `brittle_non_local` | XMAT-006 | as `brittle` | `Sigma max`, `Sigma_max non local`, `Sigma_fracture` (1 each) |
| `vreepeerlings` (unregistered) | XMAT-007 | `Kapaoi` = 1e-4, `Kapac` = 2e-4, `Arate` = 0, `Brate` = 1, `Crate` (→ writes `Brate`, default 1), `Kct` = 1, `Kapao_randomness` = 0 + elastic | `Kapa` (random, 1), `strain-rate-vreepeerlings` (dim²), `fulldam-valstrain`, `fulldam-valstrain-rate`, `number-damage`, `equi-strain`, `equi-strain-rate` (1 each) |
| `vreepeerlings_non_local` (unregistered) | XMAT-008 | as above | + `equi-strain_non_local`, `equi-strain-rate_non_local` |
| `orthotropic_damage_iterative` | XMAT-010 | `Sc` (random), `prescribed_dam` = 0.1, `dam_threshold` = 0.8, `eta` = 2 [parsmod], `max_damage` = 0.99999 [parsmod] + elastic | `damage` (dim²), `damage_principal_directions` (dim²), `damage dissipated energy`, `integral of sigma`, `Sc`, `equivalent_stress`, `equiv_stress_dir` (dim²) |
| `damage_orthotropic_iterative_non_local` | XMAT-011 | as above | + `grad_u non local` |
| `visco_plastic` | XMAT-012 | `rate` = 0, `edot0` = 0, `ts` = 0 [parsmod] + `h`, `sigma_y` + elastic | plastic internals (MAT-019) |
| `ve_stiffness_prop` | XMAT-013 | `Alpha` = 0 [parsmod] + elastic | `stress_viscosity`, `stress_elastic` (dim² each) |
| `material_FE2` | XMAT-014 | `element_type` = `_triangle_3` [parsmod], `mesh_file` [parsmod], `nb_gel_pockets` [parsmod] + thermal (`E`, `nu`, `alpha`, `delta_T`) | `material_stiffness` (voigt²) + thermal internals |
| `igfem_elastic`, `igfem_saw_tooth_damage`, `igfem_iterative_stiffness_reduction` | IGFEM-006..008 | `name_sub_mat_1`, `name_sub_mat_2` (strings) + parameters of the referenced sub-materials and of the non-IGFEM counterparts (XMAT-001/003) | `sub_material` (UInt per quad), per-quad copies (`lambda`, `mu`, `kpa`; `Sc`, `equivalent_stress`; `eps_u`, `reduction_step`, `D`) |
| NTN friction laws | TASN-005..007 | `coulomb`: `mu`; `linear_slip_weakening(_no_healing)`: `mu_s`, `mu_k`, `d_c`; `linear_cohesive`: `G_c`, `tau_c`, `tau_r`; rate & state: `mu_0`, `a`, `b`, `v_star`, `phi_star`, `friction_state`, `d_c`, `v0` (+ `mu_w`, `v_w` for flash heating); regularisation: `frictional_contact_pressure` (none), `t_star` (Rubin–Ampuero, simplified Prakash–Clifton), `spc_internal` (internal). All are per-node `SynchronizedArray<Real>` set uniformly from the `.dat` value ([parsmod]) | `is_sticking`, `frictional_strength`, `friction_traction(_old)`, `slip`, `cumulative_slip`, `slip_velocity(_norm)`, `friction_state` |

<!-- SPECS-END -->


## 3. Model algorithms

The generic solver machinery (`TimeStepSolverDefault`, integration schemes,
non-linear solvers, DOF manager) is R3's scope; this section describes how
`SolidMechanicsModel` plugs into it, with the formulas needed to reproduce the
SMM results. References: `src/model/common/integration_scheme/newmark-beta.{hh,cc}`,
`integration_scheme_2nd_order.cc:70-93`, `non_linear_solver/non_linear_solver_lumped.cc:50-100`,
`non_linear_solver/non_linear_solver_newton_raphson.cc:91-290`.

### 3.1 Initialisation (`model.initFull(_analysis_method = m)`)

1. `Model::initFullImpl` (R3): `initModel()` (FE engines' shape functions/jacobians), then for the
   chosen method `getDefaultSolverID(m)` → create a time-step solver of the type in SMM-002 with
   `getDefaultSolverOptions` (integration scheme on DOF `"displacement"`), which calls
   `initSolver(type)` (SMM-003: nodal arrays, DOF registration).
2. `ConstitutiveLawsHandler::initFullImpl` (CL-005): if a file was parsed, instantiate laws from
   `material …` sections in parser order (law index = order of appearance), assign laws to
   elements (`assignConstitutiveLawToElements`: selector on non-ghost elements → law index;
   ghost indices by `_constitutive_law_id` communication), `initMaterial()` per law
   (internals resized; history enabled for `stress`, `grad_u` (+ law-specific)), sync
   `_clh_init_cl`, initialise the non-local manager (NL-001) if present.
3. `initBC` binds Dirichlet/Neumann functors to `displacement`, `increment`, `external_force`.
4. Cohesive model (COH-002) additionally: facet materials, then intrinsic insertion or
   extrinsic set-up (check facets, facet stress arrays, facet normals, stress interpolation).

### 3.2 Explicit dynamics with lumped mass (`_explicit_lumped_mass`, solver `"explicit_lumped"`)

User loop: `model.setTimeStep(Δt)` (often `α·getStableTimeStep()`), then per step
`model.solveStep()` → `NonLinearSolverLumped::solve`:

1. `beforeSolveStep()` → every material `savePreviousState()` (copy current → previous for
   internals with history).
2. `updateGlobalBlockedDofs()`.
3. `predictor()`: time-step solver applies Newmark predictor (central difference: α = 0, β = ½,
   member k = h = 0) on non-blocked dofs: `u += Δt v + ½Δt² a`; `v += Δt a`; `a` unchanged;
   then SMM `predictor()` → `++displacement_release`.
4. `assembleResidual()`: SMM-004 (external work bookkeeping, `assembleInternalForces` (SMM-005:
   all materials compute stresses from the predicted `u`; non-local averaging; ghost stress
   sync; assembly −∫Bᵀσ; cohesive tractions first for cohesive models), residual
   `r = f_ext + f_int`), then the integration scheme adds `−M_lumped·a` (and `−C·v` if a `C`
   matrix exists) (`integration_scheme_2nd_order.cc:70-93`). The lumped mass is assembled on
   first use (`assembleLumpedMatrix("M")`, SMM-007).
5. Solve `δa_i = α_f · r_i / M_i` per dof (`solveLumped`, α_f = lumped-solver parameter `b_a2x`,
   default 1 — `non_linear_solver_lumped.cc:42`; distinct from SMM's `f_m2a`, also 1, used only by
   TASN's acceleration estimate). Blocked dofs are skipped by the Newmark corrector (their u, v, a
   keep the predicted/imposed values); global blocked-dof handling in the DOF manager is R3.
6. `corrector()` (acceleration solution type, c = 1, d = βΔt = Δt/2, e = αβΔt² = 0):
   `a += δa`, `v += (Δt/2) δa`, `u += 0` ⇒ `v_{n+1} = v_n + ½Δt(a_n + a_{n+1})`; SMM `corrector()`
   → `++displacement_release`.
7. `afterSolveStep(true)` → every material: `updateEnergies(type)` (dissipated/plastic/work
   energies), Maxwell internal-variable update; cohesive model first refreshes Cauchy stresses
   for finite-deformation materials.

Extrinsic cohesive simulations call `checkCohesiveStress()` after `solveStep()` (COH-004);
new elements/nodes trigger `onElementsAdded`/`onNodesAdded` (nodal values copied, mass flagged).

### 3.3 Explicit dynamics with consistent mass (`_explicit_consistent_mass`, solver `"explicit"`)

Same predictor/corrector (central difference, acceleration solution) but solved with the
Newton–Raphson non-linear solver and a sparse linear solve with J = c·M (+ d·C) (e = 0 so K is
not needed): `NewmarkBeta::assembleJacobian` (R3).

### 3.4 Implicit dynamics (`_implicit_dynamic`, solver `"implicit"`)

Trapezoidal rule (α = β = ½), solution type displacement: c = 1/(αβΔt²) = 4/Δt²,
d = 1/(αΔt) = 2/Δt, e = 1. Newton–Raphson (`threshold` 1e-10, `max_iterations` 10,
`convergence_type` default residual — parameters of the solver, R3):
1. `beforeSolveStep`, blocked dofs, predictor (`u += Δt v + ½Δt² a`, `v += Δt a`).
2. Residual r = f_ext + f_int − M a (− C v).
3. Loop: assemble J = c·M + d·C + e·K (K via `assembleStiffnessMatrix`, reassembled when any
   material `hasMatrixChanged("K")`; SMM-006), sparse solve J δ = r, corrector
   (`u += δ`, `v += d δ`, `a += c δ`, `++displacement_release`), new residual, convergence test
   (residual or solution norm). `afterSolveStep(converged)` (non-converged steps restore
   material history).

### 3.5 Static (`_static`, solver `"static"`)

Pseudo-time scheme, solution type not defined (δ is the displacement increment): J = K,
r = f_ext + f_int, Newton–Raphson as above (linear variant `solve_linear` if the solver type is
`_linear`: single solve then residual recomputation). Material tangents (MAT rows) are used as
given — most inelastic laws return the *elastic* tangent (MAT-020, MAT-022), so convergence is
not quadratic.

### 3.6 Lumped mass

See SMM-007: ρ per quad from `Material::getRho` (constant ρ except phase-field mass
degradation); row-sum `∫ρ N_i` for all linear (and other non-listed) element types; for
`_triangle_6`, `_tetrahedron_10`, `_quadrangle_8`, `_hexahedron_20`, `_pentahedron_15` a diagonal
scaling with fixed nodal factors times ∫ρ dV (factors listed in SMM-007); same value on every
spatial component; ghost elements' contributions included after the density exchange; result
stored in `model.getMass()` and the DOF manager lumped matrix `"M"`.

### 3.7 Stable time step

Δt_crit = min over regular elements of dimension `spatial_dimension` (non-ghost) of
`inradius(X_e + u_e) / c_e`, `c_e = material.getCelerity(element)` (P-wave speed for isotropic
laws, √(eig₀(C)/ρ) for anisotropic), global MPI minimum (SMM-009). No safety factor.

### 3.8 Energies

* Kinetic: ½ Σ m_i v_i² over local/master non-periodic-slave nodes (lumped) or ½ vᵀMv (consistent).
* Potential: Σ_materials ∫ ψ (material-specific ψ, e.g. ½σ:∇u).
* External work: per-step power estimate (`"external work"`) or accumulated
  `Σ (f_ext − f_int)·Δu` evaluated once per displacement release in `assembleResidual`
  (`"external work new"`).
* Dissipated: damage (∫σ d∇u − ψ, trapezoidal), plastic (Σ ½(σ+σ_prev):Δε_p), viscoelastic
  (SLS, Maxwell), cohesive (total − reversible).
All global energies are MPI sums.

### 3.9 Extrinsic cohesive insertion (per call of `checkCohesiveStress`)

1. `interpolateStress()`: for each bulk material, interpolate quad-point stresses to the facet
   quadrature points of each element's facets (precomputed interpolation matrices from
   `initStressInterpolation`, based on the reference positions of quad points and facet quad
   points), store both neighbours' stresses in `facet_stress(facet_quad, side)`; exchange with
   `_smmc_facets_stress`.
2. For each cohesive material: `checkInsertion()` (CMAT-001 criterion on facets in its facet
   filter that are still `check_facets`): mark `insertion_facets`, append per-quad σc_eff,
   δc_eff, insertion stress, previous traction.
3. `inserter.insertElements()`: synchronise insertion flags, double facets/nodes, create cohesive
   elements (connectivity side 1 then side 2), update facet bookkeeping (`check=false` for
   inserted facets), update global ids and synchronizers, emit `NewElementsEvent` (laws assigned
   by the cohesive selector → facet material) and `CohesiveNewNodesEvent` (nodal values copied
   from the original nodes).
4. Returns the global number of inserted elements.

Normals for the criterion are the facet normals computed once at initialisation
(`computeNormals`), tangents derived from them (`Math::compute_tangents`).

### 3.10 Intrinsic cohesive insertion

At initialisation only: all facets passing `limitCheckFacets` (bounding box, `cohesive_zones`,
`cohesive_surfaces`, two non-ghost-only neighbours, no cohesive neighbour) get cohesive elements
(COH-003). The bilinear law then supplies the initial elastic branch (CMAT-002).

### 3.11 Fragment computation (`FragmentManager::computeAllData(damage_limit)`)

1. (parallel) sync cohesive damage `_smmc_damage`.
2. Cluster the elements of dimension `spatial_dimension` through `mesh_facets` connectivity
   (`GroupManager::createClusters`): breadth-first search across facets; a cohesive element is
   traversable iff any of its quad points has damage < `damage_limit − 1e-14`; clusters named
   `fragment_<i>` (parallel: temporary names then global renumbering in `GroupManager`).
3. Mass (∫ρ, one value per component), velocity (∫ρv/m), centre of mass (∫ρx/m), inertia tensor
   ∫ρ(‖r‖²I − rrᵀ) about the centre of mass → sorted eigenvalues/eigenvectors (principal
   moments/directions), number of elements per fragment; all summed over ranks.

### 3.12 Non-local averaging (when any non-local law exists)

Inside every `assembleInternalForces`: after local stresses, `computeAllNonLocalContribution`
(NL-001): flatten registered local variables over the mesh, exchange ghost values, recompute
weights if `compute_stress_calls % update_rate == 0` (weight-function internals such as damage
flattened first), average `ā(q1) = Σ w(q1,q2) a(q2)` over the pair lists (normalised by Σw),
copy back into each law's non-local internal, then each non-local law's
`computeNonLocalStresses` (damage update from the averaged variable and final stress).

<!-- ALGO-END -->


## 4. Public API surface

Methods used by user code (examples/tests) and exposed by the Python bindings
(`python/py_solid_mechanics_model.cc`, `py_solid_mechanics_model_cohesive.cc`,
`py_material.cc`, `py_constitutive_law.cc`, `py_constitutive_law_selector.cc`,
`py_constitutive_laws_handler.hh`, `py_fragment_manager.cc`; binding details are R4's scope).
Names marked † are exposed to Python.

**`SolidMechanicsModel`** (SMM-001…021)
* Construction: `SolidMechanicsModel(mesh, dim = _all_dimensions, id = "solid_mechanics_model", dof_manager = nullptr)`†.
* Init/solve (from `Model`, R3): `initFull(_analysis_method = …)`†, `solveStep()`†, `setTimeStep(dt, solver_id = "")`†, `getTimeStep()`, `getNonLinearSolver()`, `getTimeStepSolver()`, `setIntegrationScheme(...)`, `setBaseName`, `addDumpField(Vector)`, `addDumpFieldToDumper`, `dump(...)`†, `applyBC(functor, group)`† (`BC::Dirichlet::FixedValue`, `IncrementValue`, `FlagOnly`, `BC::Neumann::FromStress/FromTraction/FromHigherDim/FromSameDim`).
* Nodal arrays: `getDisplacement()`†, `getPreviousDisplacement()`†, `getIncrement()`†, `getVelocity()`†, `getAcceleration()`†, `getExternalForce()`†, `getInternalForce()`†, `getBlockedDOFs()`†, `getMass()`†, `getCurrentPosition()`†, `getDisplacementRelease()`, `getF_M2A()/setF_M2A()`.
* Assembly: `assembleInternalForces()`†, `assembleStiffnessMatrix(need_to_reassemble = false)`†, `assembleMass()`†, `assembleMassLumped()`†, `getStableTimeStep()`†, `getEnergy(id)`†, `getEnergy(id, element)`†, `getEnergy(id, group)`†.
* Materials: `getMaterial(index/name/element)`†, `getMaterials()`, `getNbMaterials()`†, `getMaterialIndex(name)`†, `getMaterialByElement()`†, `getMaterialLocalNumbering()`, `getMaterialSelector()`†, `setMaterialSelector(selector)`†, `reassignMaterial()`†, `registerNewMaterial(name, type, option)`†, `applyEigenGradU(M, material_name, ghost)`, `getFEEngineBoundary(name)`, constitutive-law generic aliases `getConstitutiveLaw*`†, `registerNewConstitutiveLaw`†, `reassignConstitutiveLaw`†, `flattenInternal`/`inflateInternal` (Python: `flattenInternalReal`†, `inflateInternalReal`†), `isInternal`†, `getNonLocalManager()`, `isNonLocal()`.
* Events: `registerEventHandler(SolidMechanicsModelEventHandler&)`, `EventManager::sendEvent(SolidMechanicsModelEvent::…)`.

**`Material` / `ConstitutiveLaw`** (SMM-022…031, CL-001…004)
* Parameters: `get(name)` / `set(name, value)` (Parsable), deprecated `setParam`, `getParam`; `getName()`, `getID()`.
* Internals: `getInternal<T>(id)`† (`getInternalReal/Int`), `registerInternal<T>(id, nb_comp)`† (`registerInternalReal/Int`), `getArray<T>(id, type, ghost)`, `isInternal<T>(id, kind)`, `InternalField::previous()`†, `initializeHistory()`†, `hasHistory()`†, `setDefaultValue`, `reset`.
* Fields: `getStress(type, ghost)`†, `getGradU(type, ghost)`†, `getPotentialEnergy()`†, `getEnergy(id)`†, `getElementFilter()`†, `getPushWaveSpeed`†, `getShearWaveSpeed`†, `getCelerity`, `getRho/setRho`, `hasMatrixChanged`†, `hasStiffnessMatrixChanged`†, `initMaterial()`† (user materials), `computeAllStresses`, `interpolateStress(OnFacets)`, `extrapolateInternal`.
* Factory: `MaterialFactory::getInstance()`† `.registerAllocator(keyword, fn)`†, `.getPossibleAllocators()`†; `instantiateMaterial<Mat>(keyword)`.
* User-material extension (Python `Material` trampoline `PyMaterial` overrides `computeStress`, `computeTangentModuli`, `initMaterial`, `computePotentialEnergy`, `getPushWaveSpeed`, `getShearWaveSpeed`…).

**Selectors** (CL-006, COH-009): `ConstitutiveLawSelector`† (`operator()`, `setFallback`†), `DefaultMaterialSelector`, `MeshDataMaterialSelector<std::string|Int>`† (`MeshDataMaterialSelectorString`), `ElementDataMaterialSelector<T>`, `DefaultMaterialCohesiveSelector`, `MeshDataMaterialCohesiveSelector`, `MaterialCohesiveRulesSelector(model, rules, mesh_data_id)`†.

**`SolidMechanicsModelCohesive`** (COH-001…013): ctor†, `initFull(_analysis_method, _is_extrinsic)`† (`SolidMechanicsModelCohesiveOptions`†), `checkCohesiveStress()`†, `interpolateStress()`, `updateAutomaticInsertion()`†, `insertIntrinsicElements()`, `getElementInserter()`† (`CohesiveElementInserter`†: `setLimit(axis, a, b)`†, `addPhysicalSurface`†, `addPhysicalVolume`†, `insertIntrinsicElements`, `insertElements(only_double_facets)`, `limitCheckFacets`, `getCheckFacets`†, `getInsertionFacets`†, `getMeshFacets`), `getStressOnFacets(type)`†, `getTangents(type)`†, `getFacetMaterial()`, `getIsExtrinsic()`, `getMeshFacets()`, `getCohesiveSynchronizer()`. `MaterialCohesive`†: `getOpening`†, `getTraction`†, `getDamage`, `getFacetFilter`†, `checkInsertion`, `computeTraction`, `getEnergy("reversible"|"dissipated"|"cohesive contact")`.

**`FragmentManager`**† (COH-012): ctor(model, dump_data = true, id), `buildFragments(damage_limit = 1)`†, `computeAllData(damage_limit = 1)`†, `computeCenterOfMass`†, `computeVelocity`†, `computeInertiaMoments`†, `computeNbElementsPerFragment`†, getters `getNbFragment`†, `getMass`†, `getCenterOfMass`, `getVelocity`†, `getMomentsOfInertia`†, `getPrincipalDirections`†, `getNbElementsPerFragment`†.

**Non-local** (NL-*): `NonLocalManager::registerNeighborhood`, `registerNonLocalVariable`, `registerWeightFunctionInternal`, `getNeighborhood(name)` (`savePairs`, `saveNeighborCoords`, `saveWeights`), `computeAllNonLocalContribution`, `getVolumes`, `getJacobians`; user non-local materials derive from `MaterialNonLocal<dim, Parent>` and implement `registerNonLocalVariables`, `computeNonLocalStresses`.

**Embedded** (EMB-*): `EmbeddedInterfaceModel(mesh, primitive_mesh, dim, id)`, `initFull(_analysis_method, _has_intersections)`, `getInterfaceMesh()`, `getInterfaceAssociatedElements(type)`.

**Extra packages**: `MaterialDamageIterative::updateDamage()`, `getNormMaxEquivalentStress()`; `MaterialFE2::advanceASR(prestrain)`; `SolidMechanicsModelRVE::{applyBoundaryConditions, applyHomogeneousTemperature, advanceASR, homogenizeStiffness, homogenizeEigenGradU, averageTensorField}`; `SolidMechanicsModelIGFEM::{computeValuesOnEnrichedNodes, transferInternalValues, applyEigenGradU, update/moveInterface}`; TASN: `NTNContact::{addSurfacePair, addNodePairs, getNodePairs, updateNormals, updateLumpedBoundary, updateImpedance, computeContactPressure, assembleGlobalContactPressure, getGlobalContactPressure, getNbNodesInContact, dumpRestart, readRestart}`, `NTRFContact::{setReferencePoint, setNormal, addSurface, addNodes}`, `initializeNTNFriction(contact)`, `NTNBaseFriction::{updateSlip, computeFrictionTraction, assembleGlobalFrictionTraction, setParam(name, node, value), getNbStickingNodes, addNoiseToStateVariable}`, `NTNContactSolverCallback(solid, slave, master, normal_dir, time_step_factor)` (+ `getExternalWork`), `ForceBasedDirichlet`, `SpringBC`, `InclinedFlatDirichlet`, `integrateResidual`, `boundaryFix`, `dumpRestart/loadRestart`.

<!-- API-END -->


## 5. Appendix: file coverage

Total: 392 `.hh`/`.cc`/`.py` files (69045 lines), every one mapped. Generated by matching path rules; "build glue" marks files without behaviour. Paths relative to the reference root; within each group the group prefix is shown once.


### `src/model/common/constitutive_laws/` (12 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `constitutive_law.hh` | 351 | CL-001, CL-004 |
| `constitutive_law_non_local_interface.hh` | 120 | CL-007 |
| `constitutive_law_non_local_interface_tmpl.hh` | 124 | CL-007 |
| `constitutive_law_selector.hh` | 156 | CL-006 |
| `constitutive_law_selector_tmpl.hh` | 31 | CL-006 |
| `constitutive_law_tmpl.hh` | 626 | CL-001, CL-004 |
| `constitutive_laws_handler.hh` | 353 | CL-005, CL-008 |
| `constitutive_laws_handler_tmpl.hh` | 684 | CL-005, CL-008 |
| `internal_field.hh` | 282 | CL-002 |
| `internal_field_tmpl.hh` | 312 | CL-002 |
| `random_internal_field.hh` | 102 | CL-003 |
| `random_internal_field_tmpl.hh` | 109 | CL-003 |

### `src/model/common/non_local_toolbox/` (17 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `base_weight_function.cc` | 27 | NL-004 |
| `base_weight_function.hh` | 177 | NL-004 |
| `base_weight_function_inline_impl.hh` | 62 | NL-004 |
| `neighborhood_base.cc` | 254 | NL-002 |
| `neighborhood_base.hh` | 134 | NL-002 |
| `neighborhood_base_inline_impl.hh` | 38 | NL-002 |
| `neighborhoods_criterion_evaluation/neighborhood_max_criterion.cc` | 280 | NL-009 |
| `neighborhoods_criterion_evaluation/neighborhood_max_criterion.hh` | 104 | NL-009 |
| `neighborhoods_criterion_evaluation/neighborhood_max_criterion_inline_impl.hh` | 71 | NL-009 |
| `non_local_manager.cc` | 555 | NL-001 |
| `non_local_manager.hh` | 271 | NL-001 |
| `non_local_manager_callback.hh` | 75 | NL-001 |
| `non_local_manager_inline_impl.hh` | 57 | NL-001 |
| `non_local_neighborhood.hh` | 117 | NL-003 |
| `non_local_neighborhood_base.cc` | 110 | NL-002 |
| `non_local_neighborhood_base.hh` | 123 | NL-002 |
| `non_local_neighborhood_tmpl.hh` | 296 | NL-003 |

### `src/model/solid_mechanics/` (10 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `material.cc` | 831 | SMM-022, SMM-023, SMM-024, SMM-025, SMM-026, SMM-029, SMM-030, SMM-031 |
| `material.hh` | 635 | SMM-022, SMM-023, SMM-024, SMM-025, SMM-026, SMM-029, SMM-030, SMM-031 |
| `material_inline_impl.hh` | 229 | SMM-027, SMM-028, SMM-022 |
| `material_selector.hh` | 43 | CL-006, SMM-020 (`material_selector_tmpl.hh`: empty include guard only — build glue) |
| `material_selector_tmpl.hh` | 28 | CL-006, SMM-020 (`material_selector_tmpl.hh`: empty include guard only — build glue) |
| `solid_mechanics_model.cc` | 1051 | SMM-001..SMM-016, SMM-019, SMM-020 |
| `solid_mechanics_model.hh` | 495 | SMM-001..SMM-021 |
| `solid_mechanics_model_event_handler.hh` | 115 | SMM-018 |
| `solid_mechanics_model_io.cc` | 218 | SMM-017 |
| `solid_mechanics_model_mass.cc` | 172 | SMM-007, SMM-008 |

### `src/model/solid_mechanics/materials/` (67 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `material_damage/material_anisotropic_damage.cc` | 55 | MAT-017 |
| `material_damage/material_anisotropic_damage.hh` | 97 | MAT-017 |
| `material_damage/material_anisotropic_damage_tmpl.hh` | 358 | MAT-017 |
| `material_damage/material_damage.hh` | 109 | MAT-010 |
| `material_damage/material_damage_non_local.hh` | 70 | MAT-027 |
| `material_damage/material_damage_tmpl.hh` | 114 | MAT-010 |
| `material_damage/material_marigo.cc` | 69 | MAT-011 |
| `material_damage/material_marigo.hh` | 118 | MAT-011 |
| `material_damage/material_marigo_inline_impl.hh` | 122 | MAT-011 |
| `material_damage/material_marigo_non_local.cc` | 86 | MAT-012, SMM-032 |
| `material_damage/material_marigo_non_local.hh` | 83 | MAT-012, SMM-032 |
| `material_damage/material_mazars.cc` | 35 | MAT-013 |
| `material_damage/material_mazars.hh` | 115 | MAT-013 |
| `material_damage/material_mazars_inline_impl.hh` | 172 | MAT-013 |
| `material_damage/material_mazars_non_local.cc` | 35 | MAT-014 |
| `material_damage/material_mazars_non_local.hh` | 84 | MAT-014 |
| `material_damage/material_mazars_non_local_tmpl.hh` | 101 | MAT-014 |
| `material_damage/material_phasefield.cc` | 130 | MAT-018 |
| `material_damage/material_phasefield.hh` | 153 | MAT-018 |
| `material_damage/material_phasefield_inline_impl.hh` | 180 | MAT-018 |
| `material_damage/material_von_mises_mazars.cc` | 33 | MAT-015 |
| `material_damage/material_von_mises_mazars.hh` | 38 | MAT-015 |
| `material_damage/material_von_mises_mazars_inline_impl.hh` | 132 | MAT-015 (obsolete, never included: no behaviour) |
| `material_damage/material_von_mises_mazars_non_local.cc` | 34 | MAT-016 |
| `material_damage/material_von_mises_mazars_non_local.hh` | 40 | MAT-016 |
| `material_elastic.cc` | 219 | MAT-001..MAT-005 |
| `material_elastic.hh` | 139 | MAT-001..MAT-005 |
| `material_elastic_inline_impl.hh` | 120 | MAT-001..MAT-005 |
| `material_elastic_linear_anisotropic.cc` | 240 | MAT-007 |
| `material_elastic_linear_anisotropic.hh` | 131 | MAT-007 |
| `material_elastic_linear_anisotropic_inline_impl.hh` | 65 | MAT-007 |
| `material_elastic_orthotropic.cc` | 138 | MAT-008 |
| `material_elastic_orthotropic.hh` | 120 | MAT-008 |
| `material_embedded/material_reinforcement.hh` | 193 | EMB-003 |
| `material_embedded/material_reinforcement_tmpl.hh` | 649 | EMB-003 |
| `material_finite_deformation/material_neohookean.cc` | 164 | MAT-025, MAT-026 |
| `material_finite_deformation/material_neohookean.hh` | 152 | MAT-025, MAT-026 |
| `material_finite_deformation/material_neohookean_inline_impl.hh` | 156 | MAT-025, MAT-026 |
| `material_non_local.hh` | 48 | CL-007, MAT-027 |
| `material_plastic/material_drucker_prager.cc` | 74 | MAT-022 |
| `material_plastic/material_drucker_prager.hh` | 105 | MAT-022 |
| `material_plastic/material_drucker_prager_inline_impl.hh` | 335 | MAT-022 |
| `material_plastic/material_linear_isotropic_hardening.cc` | 87 | MAT-020, MAT-021 |
| `material_plastic/material_linear_isotropic_hardening.hh` | 76 | MAT-020, MAT-021 |
| `material_plastic/material_linear_isotropic_hardening_inline_impl.hh` | 196 | MAT-020, MAT-021 |
| `material_plastic/material_plastic.cc` | 110 | MAT-019 |
| `material_plastic/material_plastic.hh` | 138 | MAT-019 |
| `material_plastic/material_plastic_inline_impl.hh` | 29 | MAT-019 (empty file — build glue) |
| `material_thermal.cc` | 79 | MAT-006 |
| `material_thermal.hh` | 132 | MAT-006 |
| `material_viscoelastic/material_standard_linear_solid_deviatoric.cc` | 210 | MAT-023 |
| `material_viscoelastic/material_standard_linear_solid_deviatoric.hh` | 130 | MAT-023 |
| `material_viscoelastic/material_viscoelastic_maxwell.cc` | 503 | MAT-024 |
| `material_viscoelastic/material_viscoelastic_maxwell.hh` | 220 | MAT-024 |
| `plane_stress_toolbox.hh` | 99 | MAT-009 |
| `plane_stress_toolbox_tmpl.hh` | 129 | MAT-009 |
| `weight_functions/damaged_weight_function.cc` | 29 | NL-005 |
| `weight_functions/damaged_weight_function.hh` | 65 | NL-005 |
| `weight_functions/damaged_weight_function_inline_impl.hh` | 72 | NL-005 |
| `weight_functions/remove_damaged_weight_function.cc` | 29 | NL-006 |
| `weight_functions/remove_damaged_weight_function.hh` | 86 | NL-006 |
| `weight_functions/remove_damaged_weight_function_inline_impl.hh` | 93 | NL-006 |
| `weight_functions/remove_damaged_with_damage_rate_weight_function.hh` | 73 | NL-007 |
| `weight_functions/remove_damaged_with_damage_rate_weight_function_inline_impl.hh` | 58 | NL-007 |
| `weight_functions/stress_based_weight_function.cc` | 112 | NL-008 |
| `weight_functions/stress_based_weight_function.hh` | 88 | NL-008 |
| `weight_functions/stress_based_weight_function_inline_impl.hh` | 184 | NL-008 |

### `src/model/solid_mechanics/solid_mechanics_model_cohesive/` (26 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `fragment_manager.cc` | 515 | COH-012 |
| `fragment_manager.hh` | 158 | COH-012 |
| `material_selector_cohesive.cc` | 169 | COH-009 |
| `material_selector_cohesive.hh` | 92 | COH-009 |
| `materials/cohesive_internal_field.hh` | 77 | COH-011 |
| `materials/cohesive_internal_field_tmpl.hh` | 129 | COH-011 |
| `materials/constitutive_laws/material_cohesive_bilinear.cc` | 198 | CMAT-002 |
| `materials/constitutive_laws/material_cohesive_bilinear.hh` | 96 | CMAT-002 |
| `materials/constitutive_laws/material_cohesive_exponential.cc` | 297 | CMAT-003 |
| `materials/constitutive_laws/material_cohesive_exponential.hh` | 116 | CMAT-003 |
| `materials/constitutive_laws/material_cohesive_linear.cc` | 337 | CMAT-001 |
| `materials/constitutive_laws/material_cohesive_linear.hh` | 181 | CMAT-001 |
| `materials/constitutive_laws/material_cohesive_linear_fatigue.cc` | 303 | CMAT-004 |
| `materials/constitutive_laws/material_cohesive_linear_fatigue.hh` | 121 | CMAT-004 |
| `materials/constitutive_laws/material_cohesive_linear_friction.cc` | 169 | CMAT-005 |
| `materials/constitutive_laws/material_cohesive_linear_friction.hh` | 100 | CMAT-005 |
| `materials/constitutive_laws/material_cohesive_linear_inline_impl.hh` | 251 | CMAT-001 |
| `materials/constitutive_laws/material_cohesive_linear_uncoupled.cc` | 367 | CMAT-006 |
| `materials/constitutive_laws/material_cohesive_linear_uncoupled.hh` | 84 | CMAT-006 |
| `materials/material_cohesive.cc` | 481 | COH-010, COH-007 |
| `materials/material_cohesive.hh` | 242 | COH-010, COH-007 |
| `materials/material_cohesive_inline_impl.hh` | 96 | COH-010, COH-007 |
| `solid_mechanics_model_cohesive.cc` | 627 | COH-001..COH-007, COH-013 |
| `solid_mechanics_model_cohesive.hh` | 298 | COH-001..COH-007, COH-013 |
| `solid_mechanics_model_cohesive_inline_impl.hh` | 295 | COH-001 (entire body commented out: legacy `solveStepCohesive` — no behaviour) |
| `solid_mechanics_model_cohesive_parallel.cc` | 439 | COH-008 |

### `src/model/solid_mechanics/solid_mechanics_model_embedded_interface/` (4 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `embedded_interface_intersector.cc` | 150 | EMB-002 |
| `embedded_interface_intersector.hh` | 88 | EMB-002 |
| `embedded_interface_model.cc` | 155 | EMB-001, EMB-004, EMB-005 |
| `embedded_interface_model.hh` | 145 | EMB-001, EMB-004, EMB-005 |

### `src/mesh_utils/` (6 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `cohesive_element_inserter.cc` | 315 | COH-005, COH-003 |
| `cohesive_element_inserter.hh` | 168 | COH-005, COH-003 |
| `cohesive_element_inserter_helper.cc` | 950 | COH-005, COH-003 |
| `cohesive_element_inserter_helper.hh` | 107 | COH-005, COH-003 |
| `cohesive_element_inserter_inline_impl.hh` | 91 | COH-005, COH-003 |
| `cohesive_element_inserter_parallel.cc` | 25 | COH-005 (includes only — build glue) |

### `extra_packages/extra-materials/src/` (42 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `material_FE2/material_FE2.cc` | 204 | XMAT-014 |
| `material_FE2/material_FE2.hh` | 115 | XMAT-014 |
| `material_FE2/material_FE2_inline_impl.hh` | 20 | XMAT-014 (empty file — build glue) |
| `material_FE2/solid_mechanics_model_RVE.cc` | 596 | XMAT-015 |
| `material_FE2/solid_mechanics_model_RVE.hh` | 224 | XMAT-015 |
| `material_damage/material_brittle.cc` | 128 | XMAT-005 |
| `material_damage/material_brittle.hh` | 116 | XMAT-005 |
| `material_damage/material_brittle_inline_impl.hh` | 106 | XMAT-005 |
| `material_damage/material_brittle_non_local.hh` | 94 | XMAT-006 |
| `material_damage/material_brittle_non_local_inline_impl.hh` | 122 | XMAT-006 |
| `material_damage/material_damage_iterative.cc` | 250 | XMAT-001 |
| `material_damage/material_damage_iterative.hh` | 149 | XMAT-001 |
| `material_damage/material_damage_iterative_inline_impl.hh` | 98 | XMAT-001 |
| `material_damage/material_damage_iterative_non_local.cc` | 47 | XMAT-002 |
| `material_damage/material_damage_iterative_non_local.hh` | 86 | XMAT-002 |
| `material_damage/material_damage_iterative_non_local_inline_impl.hh` | 94 | XMAT-002 |
| `material_damage/material_damage_linear.cc` | 79 | XMAT-004 |
| `material_damage/material_damage_linear.hh` | 90 | XMAT-004 |
| `material_damage/material_damage_linear_inline_impl.hh` | 48 | XMAT-004 |
| `material_damage/material_iterative_stiffness_reduction.cc` | 197 | XMAT-003 |
| `material_damage/material_iterative_stiffness_reduction.hh` | 98 | XMAT-003 |
| `material_damage/material_orthotropic_damage.hh` | 138 | XMAT-009 |
| `material_damage/material_orthotropic_damage_iterative.cc` | 378 | XMAT-010 |
| `material_damage/material_orthotropic_damage_iterative.hh` | 129 | XMAT-010 |
| `material_damage/material_orthotropic_damage_iterative_inline_impl.hh` | 75 | XMAT-010 |
| `material_damage/material_orthotropic_damage_iterative_non_local.hh` | 89 | XMAT-011 |
| `material_damage/material_orthotropic_damage_iterative_non_local_inline_impl.hh` | 145 | XMAT-011 |
| `material_damage/material_orthotropic_damage_non_local.hh` | 97 | XMAT-011 |
| `material_damage/material_orthotropic_damage_tmpl.hh` | 313 | XMAT-009 |
| `material_damage/material_vreepeerlings.hh` | 147 | XMAT-007 |
| `material_damage/material_vreepeerlings_inline_impl.hh` | 190 | XMAT-007 |
| `material_damage/material_vreepeerlings_non_local.cc` | 30 | XMAT-008 |
| `material_damage/material_vreepeerlings_non_local.hh` | 94 | XMAT-008 |
| `material_damage/material_vreepeerlings_non_local_inline_impl.hh` | 163 | XMAT-008 |
| `material_damage/material_vreepeerlings_tmpl.hh` | 111 | XMAT-007 |
| `material_extra_includes.hh` | 75 | XMAT-000 |
| `material_non_local_extra_includes.hh` | 21 | XMAT-000 (empty file — build glue) |
| `material_plastic/material_viscoplastic.cc` | 112 | XMAT-012 |
| `material_plastic/material_viscoplastic.hh` | 102 | XMAT-012 |
| `material_plastic/material_viscoplastic_inline_impl.hh` | 95 | XMAT-012 |
| `material_viscoelastic/material_stiffness_proportional.cc` | 126 | XMAT-013 |
| `material_viscoelastic/material_stiffness_proportional.hh` | 103 | XMAT-013 |

### `extra_packages/igfem/src/` (47 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `dumper_igfem_connectivity.hh` | 139 | IGFEM-010 |
| `dumper_igfem_element_iterator.hh` | 205 | IGFEM-010 |
| `dumper_igfem_element_partition.hh` | 128 | IGFEM-010 |
| `dumper_igfem_elemental_field.hh` | 67 | IGFEM-010 |
| `dumper_igfem_generic_elemental_field.hh` | 158 | IGFEM-010 |
| `dumper_igfem_material_internal_field.hh` | 64 | IGFEM-010 |
| `dumper_igfem_quadrature_points_field.hh` | 162 | IGFEM-010 |
| `element_class_igfem.cc` | 73 | IGFEM-001 (fully commented out — no behaviour) |
| `element_class_igfem.hh` | 310 | IGFEM-001 |
| `element_classes_igfem/element_class_igfem_segment_3_inline_impl.hh` | 91 | IGFEM-001 |
| `element_classes_igfem/element_class_igfem_triangle_4_inline_impl.hh` | 96 | IGFEM-001 |
| `element_classes_igfem/element_class_igfem_triangle_5_inline_impl.hh` | 94 | IGFEM-001 |
| `fe_engine_template_tmpl_igfem.hh` | 122 | IGFEM-002 |
| `geometrical_element_igfem.hh` | 72 | IGFEM-001 |
| `igfem_enrichment.cc` | 107 | IGFEM-003 |
| `igfem_enrichment.hh` | 106 | IGFEM-003 |
| `igfem_enrichment_inline_impl.hh` | 41 | IGFEM-003 |
| `igfem_helper.cc` | 98 | IGFEM-003 |
| `igfem_helper.hh` | 153 | IGFEM-003 |
| `integrator_gauss_igfem.hh` | 125 | IGFEM-002 |
| `integrator_gauss_igfem_inline_impl.hh` | 455 | IGFEM-002 |
| `interpolation_element_igfem.cc` | 97 | IGFEM-001 |
| `interpolation_element_igfem_tmpl.hh` | 108 | IGFEM-001 |
| `material_igfem/igfem_internal_field.hh` | 36 | IGFEM-005 |
| `material_igfem/igfem_internal_field_tmpl.hh` | 39 | IGFEM-005 |
| `material_igfem/material_igfem.cc` | 372 | IGFEM-005 |
| `material_igfem/material_igfem.hh` | 139 | IGFEM-005 |
| `material_igfem/material_igfem_elastic.cc` | 247 | IGFEM-006 |
| `material_igfem/material_igfem_elastic.hh` | 134 | IGFEM-006 |
| `material_igfem/material_igfem_elastic_inline_impl.hh` | 98 | IGFEM-006 |
| `material_igfem/material_igfem_includes.hh` | 33 | IGFEM-005 |
| `material_igfem/material_igfem_inline_impl.hh` | 65 | IGFEM-005 |
| `material_igfem/material_igfem_iterative_stiffness_reduction.cc` | 282 | IGFEM-008 |
| `material_igfem/material_igfem_iterative_stiffness_reduction.hh` | 107 | IGFEM-008 |
| `material_igfem/material_igfem_saw_tooth_damage.cc` | 472 | IGFEM-007 |
| `material_igfem/material_igfem_saw_tooth_damage.hh` | 144 | IGFEM-007 |
| `material_igfem/material_igfem_saw_tooth_damage_inline_impl.hh` | 71 | IGFEM-007 |
| `mesh_igfem_spherical_growing_gel.hh` | 209 | IGFEM-003 |
| `mesh_igfem_spherical_growing_gel_tmpl.hh` | 525 | IGFEM-003 |
| `non_local_manager_igfem.cc` | 300 | IGFEM-009 |
| `non_local_manager_igfem.hh` | 89 | IGFEM-009 |
| `shape_igfem.cc` | 99 | IGFEM-002 |
| `shape_igfem.hh` | 202 | IGFEM-002 |
| `shape_igfem_inline_impl.hh` | 737 | IGFEM-002 |
| `solid_mechanics_model_igfem.cc` | 579 | IGFEM-004 |
| `solid_mechanics_model_igfem.hh` | 201 | IGFEM-004 |
| `solid_mechanics_model_igfem_inline_impl.hh` | 53 | IGFEM-004 |

### `extra_packages/traction-at-split-node-contact/src/` (50 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `boundary_conditions/force_based_dirichlet.hh` | 117 | TASN-010 |
| `boundary_conditions/inclined_flat_dirichlet.hh` | 71 | TASN-010 |
| `boundary_conditions/spring_bc.hh` | 131 | TASN-010 |
| `common/manual_restart.cc` | 125 | TASN-011 |
| `common/manual_restart.hh` | 42 | TASN-011 |
| `common/parameter_reader.cc` | 450 | TASN-011 |
| `common/parameter_reader.hh` | 100 | TASN-011 |
| `common/synchronized_array.cc` | 203 | TASN-011 |
| `common/synchronized_array.hh` | 200 | TASN-011 |
| `common/synchronized_array_inline_impl.hh` | 52 | TASN-011 |
| `functions/boundary_functions.cc` | 62 | TASN-011 |
| `functions/boundary_functions.hh` | 45 | TASN-011 |
| `functions/node_filter.hh` | 103 | TASN-011 |
| `ntn_contact/friction_laws/ntn_friclaw_coulomb.hh` | 87 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_coulomb_tmpl.hh` | 152 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_linear_cohesive.hh` | 104 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_linear_cohesive_tmpl.hh` | 173 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_linear_slip_weakening.hh` | 106 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_linear_slip_weakening_no_healing.hh` | 85 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_linear_slip_weakening_no_healing_tmpl.hh` | 76 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_linear_slip_weakening_tmpl.hh` | 175 | TASN-005 |
| `ntn_contact/friction_laws/ntn_friclaw_rate_and_state.hh` | 362 | TASN-006 |
| `ntn_contact/friction_laws/ntn_friclaw_rate_and_state_flashheating.hh` | 171 | TASN-006 |
| `ntn_contact/friction_laws/ntn_friclaw_rate_and_state_tmpl.hh` | 196 | TASN-006 |
| `ntn_contact/friction_regularisations/ntn_fricreg_no_regularisation.cc` | 121 | TASN-007 |
| `ntn_contact/friction_regularisations/ntn_fricreg_no_regularisation.hh` | 119 | TASN-007 |
| `ntn_contact/friction_regularisations/ntn_fricreg_rubin_ampuero.cc` | 150 | TASN-007 |
| `ntn_contact/friction_regularisations/ntn_fricreg_rubin_ampuero.hh` | 91 | TASN-007 |
| `ntn_contact/friction_regularisations/ntn_fricreg_simplified_prakash_clifton.cc` | 148 | TASN-007 |
| `ntn_contact/friction_regularisations/ntn_fricreg_simplified_prakash_clifton.hh` | 101 | TASN-007 |
| `ntn_contact/mIIasym_contact.cc` | 93 | TASN-003 |
| `ntn_contact/mIIasym_contact.hh` | 81 | TASN-003 |
| `ntn_contact/ntn_base_contact.cc` | 535 | TASN-001 |
| `ntn_contact/ntn_base_contact.hh` | 258 | TASN-001 |
| `ntn_contact/ntn_base_contact_inline_impl.hh` | 115 | TASN-001 |
| `ntn_contact/ntn_base_friction.cc` | 400 | TASN-004 |
| `ntn_contact/ntn_base_friction.hh` | 209 | TASN-004 |
| `ntn_contact/ntn_contact.cc` | 490 | TASN-002 |
| `ntn_contact/ntn_contact.hh` | 159 | TASN-002 |
| `ntn_contact/ntn_friction.hh` | 89 | TASN-004 |
| `ntn_contact/ntn_friction_tmpl.hh` | 85 | TASN-004 |
| `ntn_contact/ntn_initiation_function.cc` | 96 | TASN-008, TASN-005, TASN-006 |
| `ntn_contact/ntn_initiation_function.hh` | 93 | TASN-008, TASN-005, TASN-006 |
| `ntn_contact/ntrf_contact.cc` | 258 | TASN-003 |
| `ntn_contact/ntrf_contact.hh` | 116 | TASN-003 |
| `ntn_contact/ntrf_friction.hh` | 81 | TASN-004 |
| `ntn_contact/ntrf_friction_tmpl.hh` | 93 | TASN-004 |
| `ntn_contact_solver.hh` | 59 | TASN-000 (umbrella include header) |
| `ntn_contact_solvercallback.cc` | 150 | TASN-009 |
| `ntn_contact_solvercallback.hh` | 85 | TASN-009 |

### `test/test_model/test_solid_mechanics_model/` (66 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `test_cohesive/test_cohesive.cc` | 123 | test of COH-001..COH-007, CMAT-001, CMAT-002 |
| `test_cohesive/test_cohesive_1d_element/test_cohesive_1d_element.cc` | 93 | test of COH-001 1D (not registered) |
| `test_cohesive/test_cohesive_buildfragments/test_cohesive_buildfragments.cc` | 223 | test of COH-012 |
| `test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic.cc` | 125 | test of COH-004, CMAT-001 (not registered) |
| `test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic_fatigue.cc` | 233 | test of CMAT-004 (not registered) |
| `test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic_quadrangle.cc` | 118 | test of COH-004, CMAT-001 (not registered) |
| `test_cohesive/test_cohesive_extrinsic/test_cohesive_extrinsic_tetrahedron.cc` | 229 | test of COH-004, CMAT-001 (not registered) |
| `test_cohesive/test_cohesive_extrinsic_implicit/test_assembling_K_cohe_elements.cc` | 160 | test of COH-004, CMAT-001 (not registered) |
| `test_cohesive/test_cohesive_fixture.hh` | 342 | test fixture for COH/CMAT gtests |
| `test_cohesive/test_cohesive_insertion/test_cohesive_insertion_along_physical_surfaces.cc` | 83 | test of COH-003, COH-005, CMAT-003 |
| `test_cohesive/test_cohesive_insertion/test_group_update.cc` | 85 | test of COH-003, COH-005, CMAT-003 |
| `test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic.cc` | 169 | test of COH-003, CMAT-002, COH-012 (not registered) |
| `test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic_quadrangle.cc` | 196 | test of COH-003, CMAT-002, COH-012 (not registered) |
| `test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic_tetrahedron.cc` | 343 | test of COH-003, CMAT-002, COH-012 (not registered) |
| `test_cohesive/test_cohesive_intrinsic/test_cohesive_intrinsic_tetrahedron_fragmentation.cc` | 116 | test of COH-003, CMAT-002, COH-012 (not registered) |
| `test_cohesive/test_cohesive_intrinsic_impl/test_cohesive_intrinsic_impl.cc` | 163 | test of COH-007, CMAT-003 (not registered) |
| `test_cohesive/test_cohesive_linear_friction/test_cohesive_friction.cc` | 229 | test of CMAT-005 (commented out) |
| `test_cohesive/test_materials/test_material_cohesive_fixture.hh` | 303 | test of CMAT-001, COH-010 |
| `test_cohesive/test_materials/test_material_cohesive_linear.cc` | 186 | test of CMAT-001, COH-010 |
| `test_cohesive/test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/test_cohesive_facet_stress_synchronizer.cc` | 203 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_buildfragments/test_cohesive_parallel_buildfragments.cc` | 457 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic.cc` | 177 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic_tetrahedron.cc` | 234 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic/test_cohesive_parallel_extrinsic_tetrahedron_displacement.cc` | 390 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_extrinsic_IG_TG/test_cohesive_parallel_extrinsic_IG_TG.cc` | 303 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/test_cohesive_parallel_insertion_along_physical_surfaces.cc` | 122 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_insertion/test_cohesive_parallel_intrinsic_implicit_insertion.cc` | 295 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/test_cohesive_parallel_intrinsic.cc` | 165 | test of COH-008 parallel (not registered) |
| `test_cohesive/test_parallel_cohesive/test_cohesive_parallel_intrinsic/test_cohesive_parallel_intrinsic_tetrahedron.cc` | 702 | test of COH-008 parallel (not registered) |
| `test_embedded_interface/test_embedded_element_matrix.cc` | 89 | test of EMB-001..EMB-003 |
| `test_embedded_interface/test_embedded_interface_model.cc` | 98 | test of EMB-001..EMB-003 |
| `test_embedded_interface/test_embedded_interface_model_prestress.cc` | 219 | test of EMB-001..EMB-003 |
| `test_energies/test_solid_mechanics_model_kinetic_energy.cc` | 80 | test of SMM-010 |
| `test_energies/test_solid_mechanics_model_linear_elastic_potential_energy.cc` | 106 | test of SMM-025, MAT-004 |
| `test_energies/test_solid_mechanics_model_work_dynamics.cc` | 152 | test of SMM-011 |
| `test_energies/test_solid_mechanics_model_work_quasistatic.cc` | 137 | test of SMM-011 |
| `test_material_selector.cc` | 55 | test of CL-006, SMM-020 |
| `test_materials/local_material_damage.cc` | 91 | user-material example used by `test_local_material` (SMM-030, SMM-031, MAT-010) |
| `test_materials/local_material_damage.hh` | 105 | user-material example used by `test_local_material` (SMM-030, SMM-031, MAT-010) |
| `test_materials/local_material_damage_inline_impl.hh` | 71 | user-material example used by `test_local_material` (SMM-030, SMM-031, MAT-010) |
| `test_materials/py_mazars.py` | 70 | reference implementation for MAT-013 test (python) |
| `test_materials/test_damage_materials.cc` | 247 | test of MAT-013 |
| `test_materials/test_elastic_materials.cc` | 895 | test of MAT-001..MAT-004, MAT-007, MAT-008 |
| `test_materials/test_finite_def_materials.cc` | 78 | test of MAT-025 (all DISABLED) |
| `test_materials/test_finite_deformation.cc` | 127 | test of MAT-005, SMM-023 |
| `test_materials/test_interpolate_stress.cc` | 172 | test of SMM-024 |
| `test_materials/test_local_material.cc` | 119 | test of SMM-030, SMM-031 |
| `test_materials/test_material_elasto_plastic_linear_isotropic_hardening/test_material_elasto_plastic_linear_isotropic_hardening.cc` | 97 | test of MAT-020 |
| `test_materials/test_material_fixtures.hh` | 199 | test fixture for material gtests |
| `test_materials/test_material_mazars.cc` | 278 | test of MAT-013 (UNSTABLE) |
| `test_materials/test_material_non_local/custom_non_local_test_material.cc` | 78 | user non-local material for CL-007/MAT-027 test |
| `test_materials/test_material_non_local/custom_non_local_test_material.hh` | 67 | user non-local material for CL-007/MAT-027 test |
| `test_materials/test_material_non_local/test_material_damage_non_local.cc` | 103 | test of MAT-011/012, NL-001..NL-003, NL-006 |
| `test_materials/test_material_non_local/test_material_non_local.cc` | 92 | test of CL-007, NL-001..NL-004 |
| `test_materials/test_material_orthotropic.cc` | 91 | test of MAT-008 |
| `test_materials/test_material_thermal.cc` | 106 | test of MAT-006 |
| `test_materials/test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation.cc` | 156 | test of MAT-023 |
| `test_materials/test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation_tension.cc` | 169 | test of MAT-023 |
| `test_materials/test_material_viscoelastic_maxwell/test_material_viscoelastic_maxwell_relaxation.cc` | 185 | test of MAT-024 (UNSTABLE) |
| `test_materials/test_multi_material_elastic.cc` | 121 | test of MAT-002, CL-006 |
| `test_materials/test_plastic_materials.cc` | 196 | test of MAT-020 |
| `test_solid_mechanics_model_dynamics.cc` | 322 | test of SMM-002, SMM-009, MAT-001..MAT-004 |
| `test_solid_mechanics_model_fixture.hh` | 138 | test fixture for SMM gtests |
| `test_solid_mechanics_model_material_eigenstrain.cc` | 201 | test of SMM-019 |
| `test_solid_mechanics_model_material_large_rotation.cc` | 109 | test of MAT-005, SMM-023 (UNSTABLE) |
| `test_solid_mechanics_model_reassign_material.cc` | 185 | test of CL-005 (reassign), CL-008 |

### `test/test_model/patch_tests/` (12 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `patch_test_linear_anisotropic_explicit.cc` | 127 | test of MAT-007, SMM-002, SMM-005, SMM-006 |
| `patch_test_linear_anisotropic_implicit.cc` | 121 | test of MAT-007, SMM-002, SMM-005, SMM-006 |
| `patch_test_linear_elastic_explicit.cc` | 97 | test of MAT-001..MAT-005, SMM-002, SMM-005, SMM-006, SMM-023 |
| `patch_test_linear_elastic_implicit.cc` | 156 | test of MAT-001..MAT-005, SMM-002, SMM-005, SMM-006, SMM-023 |
| `patch_test_linear_fixture.hh` | 211 | test fixture for MAT-001..MAT-007 patch tests (SMM-021 BCs) |
| `patch_test_linear_fixture.py` | 142 | test fixture for MAT-001..MAT-007 patch tests (SMM-021 BCs) |
| `patch_test_linear_solid_mechanics_fixture.hh` | 237 | test fixture for MAT-001..MAT-007 patch tests (SMM-021 BCs) |
| `patch_test_linear_solid_mechanics_fixture.py` | 136 | test fixture for MAT-001..MAT-007 patch tests (SMM-021 BCs) |
| `test_lumped_mass.cc` | 92 | test of SMM-007 |
| `test_patch_linear_anisotropic_explicit.py` | 110 | test of MAT-007 (python) |
| `test_patch_linear_elastic_explicit.py` | 39 | test of MAT-001..MAT-004 (python) |
| `test_patch_linear_elastic_static.py` | 36 | test of MAT-001..MAT-004 (python) |

### `extra_packages/extra-materials/test/` (8 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `test_material_FE2/test_eigenstrain_homogenization.cc` | 84 | test of XMAT-015 |
| `test_material_FE2/test_elastic_homogenization.cc` | 82 | test of XMAT-015, MAT-008 |
| `test_material_FE2/test_material_FE2.cc` | 114 | test of XMAT-014 |
| `test_material_FE2/test_node_selection.cc` | 72 | test of XMAT-015 |
| `test_material_damage/test_material_damage_iterative.cc` | 176 | test of XMAT-001 |
| `test_material_damage/test_material_damage_iterative_non_local_parallel.cc` | 353 | test of XMAT-002, NL-001 |
| `test_material_damage/test_material_damage_iterative_non_local_serial.cc` | 229 | test of XMAT-002, NL-001 |
| `test_material_damage/test_material_iterative_stiffness_reduction.cc` | 108 | test of XMAT-003 |

### `extra_packages/igfem/test/` (21 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `patch_tests/test_igfem_triangle_4.cc` | 271 | test of IGFEM-001, IGFEM-004, IGFEM-006 (patch tests) |
| `patch_tests/test_igfem_triangle_5.cc` | 281 | test of IGFEM-001, IGFEM-004, IGFEM-006 (patch tests) |
| `patch_tests/test_interface_position.cc` | 356 | test of IGFEM-001, IGFEM-004, IGFEM-006 (patch tests) |
| `test_fe_engine/igfem_mesh_generation.cc` | 108 | test of IGFEM-001, IGFEM-002 |
| `test_fe_engine/test_fe_engine_precomputation.cc` | 70 | test of IGFEM-001, IGFEM-002 |
| `test_fe_engine/test_gradient.cc` | 171 | test of IGFEM-001, IGFEM-002 |
| `test_fe_engine/test_integrate.cc` | 126 | test of IGFEM-001, IGFEM-002 |
| `test_fe_engine/test_interpolate.cc` | 119 | test of IGFEM-001, IGFEM-002 |
| `test_geometry/test_generation_igfem_mesh_from_sphere.cc` | 138 | test of IGFEM-003 |
| `test_geometry/test_generation_igfem_mesh_from_sphere_static.cc` | 85 | test of IGFEM-003 |
| `test_geometry/test_spherical_kernel.cc` | 76 | test of IGFEM-003 |
| `test_solid_mechanics_model_igfem/test_ASR_damage.cc` | 216 | test of IGFEM-007, XMAT-001 |
| `test_solid_mechanics_model_igfem/test_eigenstrain.cc` | 189 | test of IGFEM-004..IGFEM-006 |
| `test_solid_mechanics_model_igfem/test_igfem_element_orientation.cc` | 153 | test of IGFEM-004..IGFEM-006 |
| `test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction.cc` | 404 | test of IGFEM-008 |
| `test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction_damage_step_transfer.cc` | 354 | test of IGFEM-008 |
| `test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction_parallel.cc` | 429 | test of IGFEM-008 |
| `test_solid_mechanics_model_igfem/test_material_igfem_iterative_stiffness_reduction_tangent_transfer.cc` | 265 | test of IGFEM-008 |
| `test_solid_mechanics_model_igfem/test_solid_mechanics_model_igfem.cc` | 343 | test of IGFEM-004..IGFEM-006 |
| `test_solid_mechanics_model_igfem/test_transfer_internals.cc` | 304 | test of IGFEM-004..IGFEM-006 |
| `test_solid_mechanics_model_igfem/test_volume_computation.cc` | 233 | test of IGFEM-004..IGFEM-006 |

### `extra_packages/traction-at-split-node-contact/test/` (4 files)

| File | Lines | Feature IDs / note |
|---|---|---|
| `test_ntn_model/test_ntn_basics.cc` | 43 | test of TASN-001, TASN-002 |
| `test_ntn_model/test_ntn_fixture.hh` | 258 | test of TASN-001, TASN-002 |
| `test_ntn_model/test_ntn_friction.cc` | 227 | test of TASN-004..TASN-009 |
| `test_ntn_model/test_ntn_friction_fixture.hh` | 184 | test of TASN-004..TASN-009 |

### Non-code files in scope (build glue / fixtures, no behaviour of their own)

* Package declarations: `extra_packages/{extra-materials,igfem,traction-at-split-node-contact}/package.cmake`; `extra_packages/traction-at-split-node-contact/README.txt` (build note).
* Documentation: `extra_packages/extra-materials/manual/*.tex`, `manual/figures/stress_strain_visco.pdf` (manual appendix for extra materials; no code).
* Test fixtures under the test directories in scope: 80 `.dat` (material/input files), 45 `.geo` + 35 `.msh` (meshes), 38 `.txt` (IGFEM mesh info, TASN data), 20 `.sh` (legacy run/verify scripts), 8 `.verified` + 6 `.out` (reference outputs), `test_embedded_interface/matrix` (reference stiffness matrix), test `CMakeLists.txt` files (registration; cited per test in the main table).
* `src/` scope directories contain only `.hh`/`.cc` files.

<!-- FILES-END -->

## 6. Open questions

Items that needed an oracle run, an upstream answer, or a project decision when the inventory was taken.
Numbered for reference from `docs/FEATURE_PARITY.md`.

1. **Stale extra packages / embedded package.** `extra-materials` and `igfem` (XMAT-000, IGFEM-000) and the `embedded` model (EMB-001) use APIs removed from core (`INSTANTIATE_MATERIAL`, `UInt`/old `Matrix` API, `materials[]`, `MaterialSelector`, `assignMaterialToElements`). Do they compile in the oracle build? If not, parity for them can only be "re-implemented from source, unverified against the oracle" — a project decision on scope/priority (D-007, D-032).
2. **Neo-Hookean (MAT-025/026)** appears to throw `NotImplemented` on every `computeStress` (qualified call to `PlaneStressToolbox<dim>::computeStress` → `Material::computeStress`); all its gtests are DISABLED. Confirm with the oracle; decide whether the port reproduces the exception or implements the evident intent (S = μI + (λ ln J − μ)C⁻¹) with a recorded deviation.
3. **`marigo_non_local` registers local Marigo (MAT-012, SMM-032).** Reproduce (default) or fix? `test_material_damage_non_local` currently exercises local Marigo + a non-local manager.
4. **Cohesive `delta_max` history never enabled (COH-010).** `cohesive_exponential` and implicit `cohesive_linear_friction` read `delta_max.previous(...)` (null history). Expected crash/UB in the reference; determine actual oracle behaviour (debug assert vs release UB) before porting those paths.
5. **Maxwell viscoelastic internal variable never updated (MAT-024)** and the upstream test is UNSTABLE: reproduce as coded or implement the intended recursive update? (Also SLS dissipated energy uses a matrix product + `.sum()`, MAT-023.)
6. **Linear isotropic hardening finite-deformation branch overwritten (MAT-021)**: effective behaviour equals small-strain J2 on ∇u. Reproduce?
7. **Anisotropic damage (MAT-017)**: as coded σ ≡ 0 (reads never-written `elastic_stress`), uses uninitialised matrix entries, writes into the history slot, and mutates E/ν material-wide. No test. Port as-is, port the evident intent, or mark "not supported"?
8. **Uninitialised data in the reference**: `scaleInsertionTraction` base vector (CMAT-001), `tensorPlusOp` off-diagonals (MAT-017), `Material::getPotentialEnergy(element)` integrates an uninitialised vector (SMM-012). Oracle outputs depending on these are non-deterministic; exclude from golden fixtures.
9. **Eigen ordering dependence**: anisotropic/orthotropic celerity uses the first *unsorted* eigenvalue returned by `Eigen::EigenSolver` (MAT-007/008) → stable time step depends on Eigen's internal ordering. Port must either replicate Eigen's EigenSolver ordering or deviate (documented). Same for unsorted principal strains (order-independent uses only).
10. **Global stiffness symmetry**: `SolidMechanicsModel::getMatrixType("K")` always returns `_symmetric`, while cohesive (COH-007) and potentially other contributions are assembled `_unsymmetric` and use Dᵀ. How does the DOF manager/solver (R3) treat this mix (lower-triangle storage?) — affects implicit cohesive results.
11. **Element-tag pack/unpack asymmetry in SMM (SMM-016)**: `_smm_mass`, `_for_dump`, `_smm_boundary`, `_smm_for_gradu` are packed for element lists but never unpacked. Are these tags ever synchronised through element synchronizers in practice (only for DOF lists?) — confirm with parallel oracle runs.
12. **Random fields reproducibility (CL-003, cohesive `sigma_c` distributions)**: exact reproduction requires porting Akantu's RNG, seeding (`seed = …` in `.dat`) and draw order (per type/ghost in `resize()` order, per process). Decide whether golden tests with random parameters require bit-equality or statistical checks.
13. **Fragment manager defects (COH-012)**: per-type accumulator reset (multi-type fragments) and dump-data written to copies. Reproduce or fix?
14. **Non-local pair list across element types (NL-002)**: the half-list condition compares `global_num` across types; verify with a mixed-type mesh whether pairs are duplicated/missed in the reference.
15. **Legacy unregistered cohesive tests** (`test_cohesive_extrinsic*`, `test_cohesive_intrinsic*`, `test_cohesive_1d_element`, `test_cohesive_extrinsic_implicit`, `test_parallel_cohesive/*`, `test_cohesive_friction`): port as Rust tests anyway (they document intended behaviour) or only the registered ones? Some reference `.verified` outputs (e.g. `K_matrix_verified.dat`) could become golden fixtures if they still reproduce.
16. **Thermal coupling inconsistencies (MAT-006/MAT-020)**: (1+ν) factor in 3D `epsilon_th`, thermal increment sign `prev − current` in the J2 trial stress and its absence from the final stress, `marigo`/`mazars` not refreshing `sigma_th`. Reproduce (default) and add analytical tests that document the reference behaviour.
17. **TASN rate-and-state with zero slip velocity (TASN-006)**: sticking nodes evaluate ln(V/V*) with V = 0 (−∞) for Dieterich-type laws; check the oracle behaviour in `test_ntn_friction` (maybe never reached because nodes slip from step 0).
18. **TASN default regularisation** is Rubin–Ampuero even when `no_regularisation` is requested (TASN-007). Reproduce?
19. **Python bindings of user materials (`PyMaterial` trampolines)**: R4 owns the binding inventory, but the Rust material trait must allow Python-implemented `computeStress`/`computeTangentModuli` callbacks per element type (SMM-031) — architecture input.
20. **Phase-field material vs model split**: MAT-018 depends on `energy_split*` (phase_field package) and on the coupler writing `damage`; coordinate ownership with R3 (phase-field inventory).

21. **Extrinsic insertion threshold pairing (CMAT-001)**: `checkInsertion` zips the facet filter (one entry per facet) with the per-quad-point `sigma_c` array, so facets with more than one quadrature point are compared with the wrong strength entry. Reproduce exactly (default) — affects quadratic and quadrilateral facets.

<!-- OPENQ-END -->

## 7. Completeness and method

* **Read in full**: all `src/` files in scope (constitutive-law infrastructure, non-local toolbox, SMM, every material, embedded model, cohesive model/materials/laws, cohesive inserter `.hh/.cc/_inline_impl.hh`), the extra-materials damage/plastic/viscoelastic/FE2 sources, TASN contact/friction/regularisation/initiation/solver-callback sources.
* **Read partially (declarations + key algorithm bodies)**: `cohesive_element_inserter_helper.cc` (mesh-level doubling algorithm, R1 scope), `solid_mechanics_model_RVE.cc` (`fillCracks`/`drainCracks` bodies skimmed), `material_orthotropic_damage_iterative_non_local_inline_impl.hh` (second half), all `igfem` sources except the element-class tables, sub-material assignment and enriched-value/transfer functions (shape/integrator/FE-engine internals, mesh generation, IGFEM materials' stress loops, `non_local_manager_igfem.cc`, dumpers were inventoried from declarations and structure), TASN `common/` utilities (`synchronized_array`, `parameter_reader`, `manual_restart`) and `boundary_functions.cc`.
* **Tests**: every test `CMakeLists.txt` in scope was read (registration, package, fixtures, UNSTABLE/disabled flags); test sources were read for their type lists, assertions and tolerances (grep-level for the legacy/unregistered and IGFEM tests).
* Behaviour statements are taken from the code at the pinned commit, not from documentation; where comments/docs and code disagree (e.g. `brittle` polynomial, `cohesive_exponential` unloading, Vree–Peerlings `Crate`), the code behaviour is recorded and the discrepancy flagged.
* Not covered here (other inventories): FE engine/element classes/shape functions, mesh and facet algorithms (incl. the cohesive inserter helper internals) and dumpers (R1); model/DOF manager/solvers/integration schemes/synchronizers, contact mechanics, phase-field model and couplers (R3); Python bindings details, examples and the global test catalogue (R4).
* No oracle runs were performed for this part: statements flagged "risk"/"appears" needed confirmation by the oracle work.

## Errata (independent completeness review, 2026-11-09)

Factual corrections from an independent completeness review of the inventory. Where they conflict with the text above, **the errata win**.

| Finding | Existing row / location | Evidence and correction |
|---|---|---|
| E3 | CMAT-003 formula, solid inventory | The row takes max over current maximum, previous maximum and current opening. `src/model/solid_mechanics/solid_mechanics_model_cohesive/materials/constitutive_laws/material_cohesive_exponential.cc:79-88,119-120,129-154` passes the **previous-history** maximum and assigns delta_max_new=max(delta_max_previous,delta); current trial maximum is excluded. With stored maximum .1 and successive trial openings .3 then .2, the second trial maximum is .2, not .3. Correct trial-versus-converged history semantics; retain the separate missing-history-initialization defect. |
| E6 | TASN-011, solid inventory | dumpRestart/loadRestart of SMM nodal fields implies full state. `extra_packages/traction-at-split-node-contact/src/common/manual_restart.cc:80-124` exclusively gathers/scatters/loads/dumps **displacement**. Correct to displacement-only; velocity, acceleration, forces and material history are not restored by this helper. Other TASN array restarts are separate APIs. |
| E7 | SMM-022 interpretation; solid inventory §0 storage convention | Tangent moduli row-major contradicts `src/model/solid_mechanics/material.cc:429-432`, viewing tangent storage through make_view<tangent_size,tangent_size>. `src/common/aka_types.hh:37,96-111` sets the default to Eigen::ColMajor and defines Matrix/MatrixProxy accordingly. Correct to column-major matrix blocks per quadrature point. Symmetric elastic tangents hide this error; use a nonsymmetric sentinel matrix. Generic Array tuple storage remains contiguous. |
