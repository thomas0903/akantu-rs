# Differences from the C++ library

akantu-rs reproduces upstream Akantu's behaviour, input and output
formats and, where possible, its floating-point results bit for bit. This
chapter lists what a user of the C++ (or Python) library has to know when
moving to the port: API conventions, upstream defects that the port
corrects (with the switches that restore upstream behaviour), the
reproducibility guarantees, the known deviations of this release, and the
errors found in the upstream manual.
The complete, reviewed list of upstream defects is
[`docs/UPSTREAM_DEFECTS.md`](../../UPSTREAM_DEFECTS.md).

## API conventions

| Upstream (C++) | akantu-rs |
|---|---|
| exceptions (`debug::Exception`) | `Result<_, akantu::core::Error>`; malformed input never panics |
| `initialize(file, argc, argv)`, static parser, static communicator | explicit `Parser` and `Communicator` arguments; process-global defaults in `akantu::runtime` ([Command-line options](command-line.md)) |
| `finalize()` | not needed (as upstream v5) |
| `Mesh &` shared by models and dumpers | `SharedMesh` handle; topology changes are transactions notifying every consumer |
| dumpers hold references to arrays | dumpers hold field descriptors and read live data through a `FieldResolver` at dump time |
| `camelCase` methods (`getDisplacement`, `applyBC`, `solveStep`) | `snake_case` (`displacement`, `apply_dirichlet_bc` / `apply_neumann_bc`, `solve_step`) |
| `Vector`/`Matrix` (Eigen) | `[f64; N]`, column-major `[[f64; R]; C]`, `MatrixX` ([Basic types](basic-types.md)) |
| enumerations `_static`, `_triangle_3`, … | Rust enums (`AnalysisMethod::Static`, `ElementType::Triangle3`); the upstream spellings parse (`"_triangle_3".parse()`) |
| `Array::find` returns the index or `-1` (`Array<Real>`: the distance from the end, $\varepsilon$ comparison); throws on several components | `Result<Option<usize>>`: the index with upstream's comparison ($\varepsilon$ for `f64`), an error where upstream throws; `find_upstream` returns upstream's raw value (UD-042, D-061; [Basic types](basic-types.md#arrayt)) |

Input files, mesh files, field names, parameter names and output files
keep the upstream names and formats.

## How upstream defects are handled

Decision D-003 classifies every defect found in the reference:

* A, undefined behaviour (uninitialised memory, out-of-bounds
  accesses, crashes): the port implements defined, physically correct
  behaviour, usually an error. No switch.
* B, wrong results: the port computes the correct result by
  default; where practical, a compatibility switch reproduces upstream
  (used by the differential tests against the C++ library).
* C, quirks and API semantics: the port keeps upstream's behaviour
  and documents it. The sub-label *C (documented difference)* (D-115)
  marks a deliberate, documented deviation of the port's API or file
  format where upstream has no numerical behaviour to keep and no bug to
  correct; it counts with C.

### Corrected defaults with a compatibility switch (selection)

This table is a selection. The register has more than 150 entries of
category B with a switch; every one of them is listed there with its
switch (column *Switch or patch* of
[`docs/UPSTREAM_DEFECTS.md`](../../UPSTREAM_DEFECTS.md), and the column
`compat_switch` of `docs/reports/data/upstream-defects.csv`). The rows
below are the defaults a user of the C++ library is most likely to notice
in results, grouped by model, with one row per switch or group of
switches. Each model's `upstream()` constructor (`LawOptions::upstream()`,
`CohesiveUpstreamCompat::upstream()`, `ExtraLawOptions::upstream()`, …)
sets all of its switches at once.

| Defect | Upstream behaviour | Port default | Switch reproducing upstream |
|---|---|---|---|
| UD-038 | 3-D thermal strain $(1 + \nu)\,\alpha\,\Delta T$ | $\alpha\,\Delta T$ | `LawOptions::thermal_strain = ThermalStrain::Upstream` |
| UD-109 | thermal energy $\tfrac{1}{2}\,\boldsymbol{\sigma} : \nabla\boldsymbol{u}$ (not a potential) | stored elastic energy | `LawOptions::thermal_energy = ThermalEnergy::Upstream` |
| UD-033 | anisotropic celerity from Eigen's first (unsorted) eigenvalue | Kelvin/Mandel upper bound | `LawOptions::celerity = CelerityMode::UpstreamFirstEigenvalue` |
| UD-104 | material axes applied with the inverse rotation | axes are the directions `n1…` | `LawOptions::material_axes_as_directions = false` |
| UD-105 | orthotropic update skips the parent's (`eigen_grad_u` ignored) | parent update runs | `LawOptions::orthotropic_parent_update = false` |
| UD-208 | stiffness assembly stores $\nabla\boldsymbol{u}$ instead of $\nabla\boldsymbol{u} - \boldsymbol{\varepsilon}^0$ | $\nabla\boldsymbol{u} - \boldsymbol{\varepsilon}^0$ | `SmmUpstreamCompat::stiffness_grad_u_ignores_eigen_grad_u` |
| UD-010 | Newton–Raphson returns early on a step already converged, skipping the end-of-step callbacks | step completes | `NonLinearSolverNewtonRaphson::set_legacy_hooks(true)` |
| UD-193 | a solver reuses a Jacobian another solver rebuilt | rebuilt when changed | `akantu::solver::UpstreamCompat::stale_shared_jacobian` |
| UD-194 | `convergence_type = residual_mass_wgh` silently runs the solution criterion without residual refresh | refused (`Error::NotImplemented`) | `UpstreamCompat::residual_mass_wgh_unrefreshed` |
| UD-195 | generalized trapezoidal reads `K` when its coefficient is 0 | `K` skipped | `UpstreamCompat::generalized_trapezoidal_reads_k_when_e_is_zero` |
| UD-045 | MSH 4 groups `msh_block_<tag>` created empty | filled | `ReadOptions::fill_unnamed_physical_groups = false` |
| UD-061 | MSH writer keeps 15 significant digits | 17 (exact round trip) | `WriteOptions::upstream_precision()` |
| UD-110, UD-113, UD-114 | ParaView: matrix fields padded to `dim × dim` squares, inconsistent tuple lengths, invalid base64 for < 2 bytes | exact `NumberOfComponents`, checked tuples, valid base64 | `DumperParaview::set_compat_layout(true)` |
| UD-293 | `_bernoulli_beam_3`: the $\theta_y$ row of the shape matrix is $+\mathrm{d}w/\mathrm{d}s$ while the nodal rotations use $\theta_y = -\mathrm{d}w/\mathrm{d}s$ (wrong sign of interpolated $\theta_y$ and of the moment part of line loads) | corrected row $-\mathrm{d}w/\mathrm{d}s$ | `StructuralUpstreamCompat::beam3_theta_y_legacy` (`StructuralFeEngine::new_with_compat`) |
| UD-007, UD-507 | structural model: discrete Kirchhoff triangle bending rigidity $E t^4/(12(1 - \nu^2))$; $\rho A$ on the torsion row of the 3-D beam consistent mass | $E t^3/(12(1 - \nu^2))$; $\rho (I_y + I_z)$ | `StructuralModelUpstreamCompat::dkt_bending`, `StructuralModelUpstreamCompat::beam3_torsional_mass_rho_a` ([Structural mechanics model](models/structural.md#differences-from-upstream)) |
| UD-343 | `_bernoulli_beam_3`: any finite `extra_normal` builds the element frame; a non-unit or skew normal silently gives a non-orthogonal frame | refused (`Error::InvalidArgument`) when $\lvert \boldsymbol{n} \cdot \boldsymbol{n} - 1 \rvert$ or $\lvert \boldsymbol{n} \cdot \hat{\boldsymbol{t}} \rvert$ exceeds `BEAM3_NORMAL_ADMISSIBILITY` = 1e-10 (input validation; admissible normals are used unchanged) | `StructuralUpstreamCompat::beam3_normal_unchecked` |
| UD-216 | MUMPS workspace retry returns $\boldsymbol{A}^{-(k+1)} \boldsymbol{b}$ after $k$ retries | every solve starts from the original right-hand side | `UpstreamCompat::mumps_retry_with_solution_rhs` ([MUMPS](mumps.md#errors-and-the-workspace-retry-ud-216)) |
| UD-407, UD-408 | segment intersector: a tetrahedron becomes one flat triangle; pieces kept in a set with an invalid comparator | four faces; exact lexicographic order, duplicates removed | `SegmentIntersectorOptions::upstream()` ([Geometry](geometry.md#akantugeometry-lgpl)) |
| UD-377…UD-380, UD-385, UD-388 | phase field: unfiltered damage interpolation, stiffness loop stopping early, per-element energies, AT1 penalties | see the chapter | fields of `PhaseFieldUpstreamCompat` ([Phase-field fracture](models/phase-field.md#differences-from-upstream)) |
| UD-039, UD-392, UD-394, UD-395, UD-397, UD-398 | split-node contact: regularisation fallback, non-finite rate-and-state strengths, pairing, restarts, `boundaryFix` | see the chapter | fields of `TasnUpstreamCompat` ([Split-node contact](models/ntn-contact.md#differences-from-upstream)) |
| UD-352, UD-632 | every `initSolver` (each `initNewSolver` or `getNewSolver`) copies $\boldsymbol{X}$ into the current positions: after a static step followed by an explicit one, the stable time step, the cohesive normals and the contact couplers see $\boldsymbol{X}$ instead of $\boldsymbol{X} + \boldsymbol{u}$ (`scalability_test`: $\Delta t = 8.54889 \times 10^{-7}$ in the oracle) | positions copied only when the first solver allocates them | `SmmUpstreamCompat::init_solver_resets_current_position` |
| UD-137 | J2 plasticity (`plastic_linear_isotropic_hardening`, `plastic_mazars`) in 1-D and 2-D: 3-D multiplier and truncated flow, the returned stress is off the yield surface; plane strain ignores $\sigma_{33}$ | consistent return: 1-D $\Delta p = f/(E + h)$, plane strain with $\sigma_{33}$, plane stress with $\sigma_{33} = 0$ | `LawOptions::j2_reduced_return = J2ReducedReturn::UpstreamTruncated` ([Plasticity](laws/plasticity.md)) |
| UD-138 | J2, Mazars and Marigo evaluate each trial from the current (trial) hardening or damage of an earlier evaluation in the same step | every trial starts from the state committed at the last converged step | `LawOptions::committed_trial_state = false` ([Plasticity](laws/plasticity.md), [Damage](laws/damage.md)) |
| UD-139, UD-140, UD-141 | Mazars: the plastic parent of `plastic_mazars` continues from the damaged stress; the equivalent strain uses the total strain (free heating damages); 2-D uses $\varepsilon_{33} = 0$ and the plane-stress $\lambda$ | effective-stress history; mechanical strain; the 3-D strain the 2-D state represents | `LawOptions::plastic_mazars_effective_stress`, `mazars_mechanical_strain`, `mazars_out_of_plane_strain` = `false` ([Damage](laws/damage.md#mazars)) |
| UD-144, UD-145, UD-146 | Drucker–Prager: Newton projection (100 iterations, tolerance $10^{-10}$) that can stop off the surface, give NaN or not terminate | closed-form closest-point projection | `LawOptions::drucker_prager_return = DruckerPragerReturn::UpstreamIteration` ([Plasticity](laws/plasticity.md#druckerprager-plasticity-plastic_drucker_prager)) |
| UD-029, UD-063, UD-161, UD-329, UD-762 | `sls_deviatoric`: dissipation as the sum of a matrix product's entries, history advanced at every evaluation, energy $\tfrac{1}{2}\,\boldsymbol{\sigma} : \nabla\boldsymbol{u}$, degenerate viscous parameters give NaN | tensor contraction, committed history, two-spring free energy, limits for `Ev = 0` / `Eta = 0` | `LawOptions::sls_dissipation`, `sls_committed_state`, `sls_energy`, `sls_degenerate_branch` ([Viscoelastic laws](laws/viscoelastic.md#standard-linear-solid-sls_deviatoric)) |
| UD-029, UD-157, UD-162, UD-163 | `viscoelastic_maxwell`: the stresses of the Maxwell elements never updated, stiffness flagged changed only when $\Delta t$ changes, shear strain doubled, 2-D compliance not the inverse of the stiffness | exponential recurrence after each converged step, stiffness flagged after a parameter change, tensor strains, $\boldsymbol{S} = \boldsymbol{C}^{-1}$ | `LawOptions::maxwell_update = MaxwellUpdate::Upstream`, `maxwell_stiffness_invalidation = false` ([Viscoelastic laws](laws/viscoelastic.md#generalised-maxwell-model-viscoelastic_maxwell)) |
| UD-038, UD-158, UD-160 | inelastic laws: thermal state not refreshed (damage laws), thermal increment with the wrong sign (plasticity); finite-strain damage work pairs the second Piola–Kirchhoff stress with $\Delta\nabla\boldsymbol{u}$; heating an undamaged body counts as dissipation | thermal pre-step, $\Delta\boldsymbol{\sigma}_{\text{th}} = \boldsymbol{\sigma}_{\text{th}} - \boldsymbol{\sigma}_{\text{th,prev}}$, conjugate work $\tfrac{1}{2}(\boldsymbol{S}_{n+1} + \boldsymbol{S}_n) : \Delta\boldsymbol{E}$, reversible thermal work excluded | `LawOptions::damage_thermal_refresh`, `plastic_thermal_increment`, `damage_thermal_work` = `false`; `finite_strain_work = FiniteStrainWork::UpstreamMixed` |
| UD-164, UD-165 | `phasefield` material tangent: `E g_h` in 1-D compression; sign test on the total trace while the stress splits on the thermally corrected one | tangent consistent with the stress | `LawOptions::phasefield_tangent = PhaseFieldTangent::Upstream` ([The phase-field material](laws/phasefield.md)) |
| UD-558 | non-local averaging: two `mazars_non_local` laws averaging different variables share one registration, so every neighbourhood averages the same variable | one registration per neighbourhood | `VariableRegistration::UpstreamShared` (in `NonLocalOptions::upstream()`; [Non-local laws](laws/non-local.md)) |
| UD-219 | a cohesive law listed first in the input never receives a facet (material index 0 is skipped), so extrinsic insertion is disabled without a message | index 0 skipped only when law 0 is not cohesive | `CohesiveUpstreamCompat::skip_material_index_0` ([Cohesive elements](models/cohesive.md#upstream-compatibility-switches)) |
| UD-242, UD-243, UD-244, UD-245 | cohesive tangents that are not the derivative of the traction (penetration predicate, an extra normal stiffness on penetration, the uncoupled law's shear term); `cohesive_exponential` heals when a traction is evaluated outside a step | derivative of the traction; maxima committed at the end of each converged step | `CohesiveUpstreamCompat::tangent_penetration_sign`, `tangent_contact_normal_stiffness`, `uncoupled_tangent`, `maxima_committed_at_step_entry` |
| UD-276, UD-280, UD-281 | diffusion: a law with an empty filter for one type skips its later types; the element energy reads another element; the lumped capacity is gated by the consistent capacity's flag | each type assembled; the law's own element; its own flag | fields of `DiffusionUpstreamCompat` (`empty_filter_returns`, `element_energy_uses_law_local_index`, `lumped_capacity_gated_by_consistent_flag`) |
| UD-595, UD-599, UD-601 | embedded interfaces: a segment whose name matches no law gets law 0; duplicated, overlapping or missing reinforcement pieces where a bar runs along an edge or face or through a vertex | refused with an error; pieces assembled by parameter interval | `EmbeddedUpstreamCompat::interface_selector_falls_back_to_first_law`; `EmbeddedInterfaceIntersector::remove_near_duplicates = false` ([Embedded interface](models/embedded.md)) |
| UD-573, UD-575, UD-577, UD-579, UD-580, UD-581, UD-583, UD-584, UD-585, UD-780, UD-870, UD-1005 | extra materials, among them the `damage_linear` softening curve (upstream's peak stress exceeds `Sigc`: 2.1016 `Sigc` in the case of `docs/NUMERICAL_VALIDATION.md`), per-point state of `damage_linear` and the viscoplastic law, the `vreepeerlings` rate, the orthotropic damage frames and its distributed maximum, the RVE volume | the corrected laws | fields of `ExtraLawOptions`, all set by `ExtraLawOptions::upstream()` ([Extra materials](models/extra-materials.md)) |
| UD-423, UD-424, UD-427, UD-428 | PETSc: KSP divergence ignored; real options written with 6 significant digits; reactions kept in an iterative KSP's right-hand side; SNES and TAO iterates aliased with the solution vector | divergence is an error; options written exactly; blocked rows zeroed; own vectors | `akantu::solver::UpstreamCompat::petsc_ignore_ksp_divergence`, `petsc_six_digit_real_options`, `petsc_ksp_rhs_with_reactions`, `petsc_aliased_solver_vectors` ([PETSc](petsc.md)) |
| UD-1291 | static / implicit solid–contact couplers: the contact search runs in the Newton corrector only, so a step whose first residual converges keeps the previous step's contact state (and every first residual uses it) | search before the first residual of the step, on the predicted positions | `SolidContactCompat::search_in_corrector_only` (`SolidContactCompat::upstream()` also sets the couplers' UD-374 switch, `all()` UD-374 and UD-772; [Model couplers](models/couplers.md#coupling-the-solid-and-the-contact-mechanics-models)) |

Next to UD-1291, three properties of the Newton–Raphson criteria that the
static and implicit contact couplers meet are kept as upstream has them
(category C, D-113; [Model
couplers](models/couplers.md#coupling-the-solid-and-the-contact-mechanics-models)):
the default `solution` criterion declares convergence on the last
increment and never tests the residual of the contact set that the last
correction's search found (UD-1293; prefer `convergence_type = residual`
with contact); the `residual` criterion is relative to the step's first
residual, so a step whose first residual is tiny but above the threshold
may stagnate at the rounding floor, above `threshold` $\times \lVert \boldsymbol{r}_0 \rVert$, until
`max_iterations` (UD-1294); and
the `linear` non-linear solver computes no contact force inside its step,
in either code (UD-1295; use Newton–Raphson).

One more property of the cohesive model is kept as upstream has it (category
C, D-117; [Cohesive elements](models/cohesive.md)): the boundary FE engine
that a Neumann load uses computes its jacobians and normals once and does
not update them when a cohesive insertion rewrites a boundary row, so a load
applied after the rewrite on a non-affine boundary element with a
multi-point rule (a four-node quadrangle that is not a parallelogram, a
curved quadratic segment or triangle) is integrated with the jacobians of
the old node order: $1.7 \times 10^{-3}$ of the largest nodal load on a distorted
hexahedral test mesh (UD-1296). One-point elements (linear triangles,
two-node segments) are not affected.

The cohesive model has its own switches (`CohesiveUpstreamCompat`,
[Cohesive elements](models/cohesive.md)); among them, the friction law
`cohesive_linear_friction` computes by default the residual sliding with
the signed friction stress (UD-249, `friction_residual_sliding_abs`) and,
in implicit runs, decides contact from the converged damage with the exact
tangent (UD-247, `friction_upstream_implicit`); its friction force at zero
tangential opening is zero instead of NaN (UD-225, no switch). The implicit
tangent of the linear-family laws (`cohesive_linear`, `cohesive_bilinear`,
`cohesive_linear_fatigue`, `cohesive_linear_friction`) is zero on a point
failed past $\delta_c$, the derivative of its zero traction, where upstream keeps
a negative secant (UD-1160, `tangent_failed_secant`). When the insertion
doubles a node of a face loaded by a Neumann condition, each copy keeps the
load of its own boundary elements, where upstream leaves the whole
accumulated load on the original copy, on a side that depends on the
element numbering and, on a distributed mesh, on the partition (UD-1280,
`doubled_node_keeps_external_force`; [Cohesive
elements](models/cohesive.md#when-a-crack-reaches-a-loaded-face)). With
this default, Neumann loads must not be rescaled or replaced by writing the
external force (zero it and apply the loads again, or set the switch); a
force written there is taken as added, and a doubling that moves a record
from such a node prints a warning (D-108).
`LawOptions::upstream()` sets every material switch at once
([Materials](materials.md#upstream-compatibility-switches));
`StructuralUpstreamCompat::UPSTREAM` sets both structural switches
(structural element kernels, `akantu::fe::structural`: [Structural
mechanics](models/structural.md)).
`StructuralModelUpstreamCompat::UPSTREAM` sets every switch of the structural model, the kernel switches included.

### Corrected without a switch (selection)

| Defect | Upstream | Port |
|---|---|---|
| UD-080, UD-081 | cyclic or very deep expressions overflow the stack | cyclic references are an error; any depth is evaluated |
| UD-082, UD-083 | empty integer/boolean values or a missing `user` section read uninitialised data | error |
| UD-044, UD-046, UD-047, UD-048, UD-093 | MSH writer drops elemental data; MSH 1.0 unreadable; element-node data truncated; Diana fails in 2-D; written 2nd-order elements read back permuted | data written and read completely; MSH 1.0 and 2-D Diana supported; exact round trips |
| UD-064, UD-090, UD-095 | Gmsh reader corrupts tags, misreads skipped blocks, maps undefined node tags to node 0 | aligned tags, blocks skipped correctly, undefined or duplicate tags are `Error::Parse` |
| UD-049 | `_miot_abaqus` dereferences a null reader | `Error::Unsupported` |
| UD-111, UD-115, UD-116 | uninitialised padded stress components; text dumper reads out of bounds; non-homogeneous fields abort off root | zeros; checked; works on every rank |
| UD-124, UD-125 | copied element groups lose their nodes; destroying a used node group dangles | nodes copied; refused |
| UD-178, UD-183 | singular or non-finite systems "solved" with NaN/inf | `Error::LinearSolver` / `Error::Numerical` |
| UD-003 | parallel matrix–vector product reduction is a no-op | correct reduction (implicit dynamics on several ranks) |
| UD-212 | `initNewSolver` builds the new solver with the previous method's options | the new method's options |
| UD-012 | `hasDOFsDerivatives` is off by one (`false` for the highest registered order; order 0 indexes `−1`), so a second dynamic solver on an order-1 model throws "already registered" | `DofManager::has_dofs_derivative(id, order)` answers whether that order is registered (order 0: `true`); a second dynamic solver of the diffusion model is accepted |
| UD-890 | distributed cohesive insertion overwrites ghost boundary elements of the loaded groups with an interior facet's connectivity, and the next Neumann load reaches an interior node: the distributed run solves another load case | the ghost boundary elements stay on their faces and the Neumann load equals the serial one at every rank count |
| UD-005, UD-518 | structural lumped mass identically zero (`node2 = conn(0)`), explicit update divided by zero masses | `m/2` per translation (rotary inertia: UD-059 below), zero masses and singular rotary blocks on free DOFs refused |
| UD-059 | structural model: lumped rotary inertia from a disc / sphere surrogate (evaluated on upstream's zero masses, UD-005; upstream's lumped values cannot be reproduced) | HRZ coefficients; 3-D: the exact nodal $3 \times 3$ block $\sum_e \boldsymbol{T}_e^T \operatorname{diag}(J_t, J_b, J_b)\, \boldsymbol{T}_e$ (objective); corrected without a switch, `StructuralModelUpstreamCompat::lumped_rotary_inertia` is a model option (`Block3x3` default, `IsotropicMax`: a diagonal alternative) ([Structural mechanics model](models/structural.md#lumped-mass)) |
| UD-096, UD-098, UD-099 | FE engine: no-op or out-of-bounds paths, partial data on error | all elements / errors, nothing stored on error |
| UD-409 | `MeshSphereIntersector` does not compile | implemented (evident intent; feature `cgal`) |
| UD-264 | a failed MUMPS analysis surfaces as an unrelated factorisation error | reported as an analysis error |
| UD-393, UD-396, UD-402 | split-node contact: 1-D pairing yields node 0 or nothing; infinite recursion adding noise; restart arrays with another header read out of bounds | actual nodes; errors |

### Refused by default

Some configurations run in the C++ library but give results that are wrong
or not specified. The port refuses them by default with an error, so an
input file that runs upstream can stop in the port. Each refusal has a
switch that reproduces upstream, for comparisons:

| Configuration | Defect | Error | Switch reproducing upstream |
|---|---|---|---|
| `plastic_linear_isotropic_hardening` with `finite_deformation = true` | UD-030: the finite-strain result is overwritten by the small-strain kernel on the raw $\nabla\boldsymbol{u}$ | `Error::Unsupported("plastic_linear_isotropic_hardening with finite_deformation: no reviewed finite-strain J2 formulation (UD-030); …")` from `computeStress` | `LawOptions::finite_strain_plasticity = FiniteStrainPlasticity::UpstreamLegacy` |
| `marigo`, `mazars` and `plastic_mazars` with `finite_deformation = true` | UD-142: the small-strain kernel on the raw $\nabla\boldsymbol{u}$, not frame indifferent (a pure rotation creates stress) | `Error::Unsupported("marigo with finite_deformation: no reviewed objective finite-strain damage formulation (UD-142); …")`, and the same for `mazars / plastic_mazars` | `LawOptions::finite_strain_damage = FiniteStrainDamage::UpstreamLegacy` |
| `viscoelastic_maxwell` with `alpha ≠ 0` | UD-327: a thermal stress added to a law whose Maxwell elements and stored energy ignore the thermal strain | `Error::Unsupported("viscoelastic_maxwell: thermal expansion (alpha != 0) is not modelled: …")` at `initMaterial` and in the later calls | `LawOptions::maxwell_thermal = MaxwellThermal::Upstream` |
| `anisotropic_damage`, any parameters | UD-031, UD-058: uninitialised entries in upstream's law, and no approved specification | `Error::Unsupported("anisotropic_damage: the default is refused (UD-031/UD-058); …")` | `LawOptions::anisotropic_damage = AnisotropicDamageModel::Upstream` (the reference code with its uninitialised entries set to zero); `Corrected` is an opt-in implementation of the UD-058 specification ([Damage](laws/damage.md#anisotropic-damage-anisotropic_damage)) |
| `convergence_type = residual_mass_wgh` | UD-194: upstream silently runs the solution criterion without refreshing the residual | `Error::NotImplemented` | `akantu::solver::UpstreamCompat::residual_mass_wgh_unrefreshed` |

All five are part of the `upstream()` constructors (`LawOptions::upstream()`,
the solver's `UpstreamCompat`). The model chapters list the other refusals
of input that upstream mishandles (an inadmissible beam normal, UD-343,
above; a PETSc run on a serial mesh with several processes; contact and
structural models on a distributed mesh).

Behaviour kept on purpose (category C) includes the two expression
grammars of the parser (UD-055), the tetrahedron node permutation
(UD-041), the integration-scheme replacement of `init_full` (UD-009),
the default solver chosen before validation (UD-201) and Eigen's unsorted
principal strains.

Upstream's dumps in ParaView (UD-110, UD-111). A matrix-valued field
(`strain`, `Green strain`, and in 2-D the stress padded by
`addDumpFieldTensor`) is written by upstream with $c \times c$ values per tuple
under `NumberOfComponents = c` (each tuple a zero-filled square whose
top-left block is the tensor). ParaView takes the array's first $c \times n$
values as the tuples of the $n$ cells, without a message (cell $i$ shows row
$i \bmod c$ of the square of cell $\lfloor i/c \rfloor$), so upstream's strain and padded
stress cannot be inspected in ParaView; the padded 2-D
stress also holds arbitrary values in its out-of-plane shear entries
(UD-111, up to $8.19 \times 10^{227}$ in the oracle's `parallel_2d` dumps). The port's
files hold each cell's own tensor; `DumperParaview::set_compat_layout(true)`
writes upstream's layout, with zeros in those four entries. The
[ParaView visual check](../../reports/2026-11-12-paraview-visual-check.md)
shows both side by side.

## Deliberate deviations of the port

* Pure-Rust solvers. The `eigen` sparse solver is a translation of
  Eigen 3.4 `SparseLU` (bit-identical to the strict oracle on the
  committed fixtures; see the next section for the platform caveats);
  `faer` is an additional pure-Rust LU without a bitwise claim (decision
  D-040). Both run on several ranks by centralising the system on rank 0,
  where upstream's Eigen solver accepts one process. MUMPS is an opt-in
  feature (`mumps` of `akantu-linalg`, upstream's default implicit
  solver), and `auto` selects it only on an MPI communicator
  ([MUMPS](mumps.md#selecting-it)).
* PETSc (solvers and DOF manager) is behind the opt-in feature
  `petsc` (of `akantu-solver`, and of the solid, phase-field and coupler
  crates), off by default as upstream's `AKANTU_USE_PETSC`; the solid and
  phase-field models select it as upstream (`dof_manager_type = petsc`,
  `init_dof_manager("petsc")`), the contact couplers keep the default DOF
  manager, a serial mesh needs a one-process job (refused on more
  processes, recoverably), and a bounded TAO solve whose bounds exclude a
  zero increment is refused (UD-1022, where upstream returns a wrong
  result) ([The PETSc back-end](petsc.md)).
* Constitutive-law factories per model. Upstream's material factory
  is a process-wide singleton; the port's factories belong to each model
  ([Writing new constitutive laws](new-constitutive-laws.md#differences-from-upstreams-procedure)).
* Geometry split by licence. The CGAL-derived GPL code is an opt-in
  crate (feature `cgal`); a build with it is GPL-3.0-or-later as a whole
  ([Geometry](geometry.md#the-cgal-feature-gpl-30-or-later)).
* Native solver objects (MUMPS) are bound to their communicator and
  agree on every call across processes; misuse is an error on every
  process instead of a crash or a hang ([MUMPS](mumps.md#rules-a-program-must-follow-d-054)).
* Reductions in rank order (D-039): collective sums are folded in rank
  order by the serial, in-process and MPI back-ends alike, so they do not
  depend on the back-end for a given rank count and partition. Upstream
  delegates them to MPI, whose association order may differ (measured:
  explicit energies at 4 ranks differ from the oracle by $\le 1.9 \times 10^{-16}$
  relative); `ReductionPolicy::NativeMpi` uses MPI's own reductions
  instead (oracle comparisons).
* Strict input checks. Mesh distribution refuses inputs upstream
  mishandles (UD-235); files and user sizes are validated before any
  state changes (reads are transactional).
* Partition seeds are recorded with every partition (UD-086).
* Internals on their own support (`registerInternal` with an FE
  engine or an element filter): the field owns a copy of its filter
  (upstream keeps a reference to the caller's array), and element
  removals or reassignments the field cannot follow are refused before
  any change (UD-1083; [Writing new constitutive laws](new-constitutive-laws.md#internals-on-another-fe-engine-or-element-filter)).

## Known deviations of this release

Differences from upstream that are not intended: none known. The 3-D
lumped rotary inertia of the structural model is the exact nodal $3 \times 3$
block (decision D-059); the diagonal rule $\max(J_t, J_b)$, whose torsional
frequencies are far too low on elements longer than $4.42\, r$, is available
as `LumpedRotaryInertia::IsotropicMax` ([Structural mechanics
model](models/structural.md#lumped-mass)).

## Bitwise reproducibility and platforms

What is established, and on which platform (evidence:
[`docs/NUMERICAL_VALIDATION.md`](../../NUMERICAL_VALIDATION.md); the
platforms themselves: `docs/PLATFORMS.md`):

* Reference build. The port is compared value by value with upstream
  built in a strict configuration (`-O2 -ffp-contract=off
  -DEIGEN_DONT_VECTORIZE`: no contraction into fused multiply–add, no
  vectorised kernels). Upstream's default flags (contraction, NEON/SSE
  kernels) change many results in the last bits (measured on the dense
  layer: 1,265 of 2,250 records differ); the port follows the strict
  build, never the default one.
* Reference platform of the committed fixtures: x86_64 Linux. Decision
  D-043 makes x86_64 Linux (glibc, the oracle's Debian base) the
  authoritative platform, and the project's test gate runs on x86_64. The repository holds 2,023 golden fixture cases (the
  directories of `fixtures/` with a `manifest.json`; counted with
  `python3 scripts/docs/validation_counts.py fixtures`), and each
  manifest records the platform in `build.arch`: 1,037 say `x86_64` and
  986 say `aarch64` (counted by reading the `build.arch` field of every
  manifest). The first corpus was generated with the strict oracle on
  aarch64 Linux (Debian 13, glibc 2.41); it was then regenerated on x86_64
  and every difference classified (`NUMERICAL_VALIDATION.md`, section
  "Platform effects"). A case whose x86_64 values are bit-identical to the
  aarch64 ones keeps its aarch64-generated files and its `aarch64`
  manifest (re-stamping identical values would only rewrite the manifest),
  and its data were verified identical by that regeneration on the
  pinned x86_64 oracle and are checked by the test gate on x86_64;
  a case that differs carries the x86_64 values, with the
  aarch64 values kept as a platform variant. Against these fixtures the
  element kernels, quadrature rules, dense products, the Eigen sparse LU,
  the parser's number parsing, the random distributions and the other
  areas marked bitwise in `NUMERICAL_VALIDATION.md` are reproduced bit for
  bit; the remaining comparisons use the tolerances justified there.
  Bitwise statements in this manual are statements against the strict
  oracle on x86_64 unless they say otherwise.
* Known differences on x86_64 Linux (the cross-platform comparison of
  the whole corpus is not complete, so the list may not be either):
  * the sign of the default NaN produced by an invalid operation differs
    (`0x7ff8…` on aarch64, `0xfff8…` on x86_64). Under decision D-046 the
    exact comparisons treat the two default NaNs as equal (every other bit
    pattern, and the sign of zero, D-051, stays strict). With it, two
    tests that would otherwise fail on x86_64 only pass: a parser
    expression, and a finite-deformation explicit replay whose oracle run
    itself diverges to NaN, every finite value being identical;
  * the x86_64 oracle's dense LU case `lu/dense_block` differs from the
    aarch64 golden in the last bits (up to 1024 ulp in the factors; the
    oracle's Eigen kernels size their blocking from the host's cache
    sizes): the port reproduces the aarch64 golden on both
    platforms, but agreement with an x86_64 oracle run is not established
    for that case;
  * results of MUMPS depend on the CPU-specific BLAS kernels (measured
    $\le 1.3 \times 10^{-13}$ between aarch64 and x86_64 oracle runs): with OpenBLAS pinned
    (`OPENBLAS_CORETYPE=Haswell`, one thread) the port's one-process MUMPS
    solutions are bitwise equal to the x86_64 oracle's, and the aarch64
    MUMPS fixtures are kept as a platform variant
    ([MUMPS](mumps.md#bitwise-reproducibility)).
* macOS (arm64) is a supported development platform, with
  enumerated exceptions. The parser's mathematical functions call the
  platform libm, as upstream does, and Apple's libm differs from glibc in
  the last bit for some arguments: the measured differences are listed one
  by one (`LIBM_EXCEPTIONS`, `docs/NUMERICAL_VALIDATION.md`: 18 one-ulp
  results of `acos`, `asin`, `atan`, `atanh`, `erf`, `erfc` and `lgamma`
  in the parser corpus), and any other difference fails the parser's
  comparison. The random distributions use a pure-Rust port of the
  oracle's glibc functions and are bitwise on every platform measured
  (macOS arm64, Linux aarch64, Linux x86_64).
* Threads. The port never uses fast-math, fused multiply–add or
  re-association, and its numerical code runs on one thread, so results
  do not depend on the machine's thread count, with one exception: with
  the `scotch` feature, Scotch 7 partitions with several threads by
  default and is then not reproducible run to run (UD-236); set
  `SCOTCH_PTHREAD_NUMBER=1` (and a fixed seed) for reproducible
  partitions, as the project's fixtures do.
* Several ranks. For a given rank count and partition, reductions
  are folded in rank order (above). The multi-rank workloads measured so
  far (mesh distribution, explicit dynamics at 1, 2 and 4 ranks) give the
  same bits on the local world and on MPI and match the oracle at the
  same rank count and partition bit for bit (the energies at 4 ranks only
  with `ReductionPolicy::NativeMpi`); static solves on several ranks use
  a different solver from the oracle's MUMPS and agree to $1.9 \times 10^{-16}$
  relative. Multi-rank oracle results are trusted per workload only
  (decision D-035; the oracle's parallel implicit dynamics is not used as
  a reference, UD-077). Not guaranteed (as upstream): independence of the
  number of ranks or of the partition, and agreement with libraries other
  than the strict upstream build.

## Questions a C++ developer will ask

* Why do my default results differ from the C++ library's for J2 in 2-D,
  Mazars in plane stress, the standard linear solid and others? The
  defaults correct defects of category B. Bitwise parity with the C++
  library is claimed with the compatibility switches set (for materials
  `LawOptions::upstream()`); the register lists each change with its test.
* `neohookean` runs in the port and throws in the C++ library. Upstream's
  `computeStress` ends in `Material::computeStress`, which is not
  implemented (UD-026); the port implements the documented law, and
  `NeohookeanMode::UpstreamNotImplemented` reproduces the error.
* `neohookean` returns an error on an inverted element. $J \le 0$ is
  `Error::Numerical`, where upstream's `log(J)` gives NaN.
* `plastic_mazars` stores $(\nabla\boldsymbol{u} - \boldsymbol{\varepsilon}_p) + \boldsymbol{\varepsilon}_p$ as its gradient, which differs
  from $\nabla\boldsymbol{u}$ at round-off level. This is reproduced as upstream in both modes
  (UD-150).
* The J2 tangent is the elastic one. As upstream, which makes the Newton
  solve a modified Newton (UD-079); the iteration counts equal the C++
  library's.
* Why is `par_scenarios` public in `akantu-cohesive`? It holds the
  scenarios of the oracle probe, shared by the local-world tests, the
  real-MPI test binary and an example. It is not a user API.
* Why does the `embedded` example need the GPL feature `cgal` when the
  crate is LGPL? To reproduce upstream's AABB traversal order, as upstream
  builds the example with CGAL. The library itself uses the LGPL
  `LinearScan` ([Geometry](geometry.md)).
* What was compared for the extra materials and IGFEM? Neither package
  compiles upstream at the reference commit (UD-025, UD-572), so there is
  no oracle: they are checked against upstream's test criteria, its
  `.verified` files and independent tests.
* The contact model, the contact couplers and the structural model refuse a
  distributed mesh. Upstream has no distributed contact (parity row
  CONT-019), and its distributed structural residual is wrong (UD-902).
* NumPy views have `.base` set to an `_ArrayLease`, and a Python callback
  can raise while native code holds array elements. The lease keeps the
  storage alive and orders NumPy access against native code (UD-085). The
  one rule the port cannot enforce: NumPy releasing the GIL on a view while
  another thread calls akantu.
* `akantu-capi` exports one function. It is a placeholder; upstream has no
  C API (D-100).
* Why the environment variables `AKA_SEED` and `AKA_DEBUG_LEVEL`? They are
  upstream's, read at `runtime::initialize` and by the Python module with
  the same meaning.
* On macOS, can results differ from the oracle's? The parser's functions
  call the platform libm (above), and so do a few other call sites: `powf`
  in the cohesive linear law's volume scaling of the strength and in the
  contact penalty's slip moduli, `atan2`, `atan` and `tan` in the
  anisotropic damage law. On glibc they give the oracle's values; on macOS
  they can differ in the last bit (`docs/NUMERICAL_VALIDATION.md`,
  "Platform effects").

## Upstream manual errata

Statements of the upstream manual (`doc/dev-doc/manual`) that the upstream
source code contradicts. The port follows the code; each item is
checked by a test (doc-tests of the chapters named, or
`crates/akantu/tests/manual_claims.rs`, `manual_claims_3.rs`).

| Upstream manual | Upstream code (and port) | See |
|---|---|---|
| `io.rst`: the exponent operator is `^` | `**` (`algebraic_parser_x3.cc`); `^` is a syntax error | [Input files](input-files.md#algebraic-expressions) |
| `io.rst`: example matrix split over two lines without `\` | rejected by the grammar (values end at the end of the line) | [Input files](input-files.md#structure) |
| `io.rst`: `force_linear_recompute`, `verbose_iteration` in the `non_linear_solver` table | registered `_pat_modifiable`: rejected as "not parsable" in input files | [Solver configuration](solver-configuration.md#non_linear_solver-type---) |
| `io.rst`: dump fields `force`, `residual` | `force` was renamed `external_force`; `residual` was replaced by `internal_force` (not the same quantity); both old names raise an error | [Output](output.md#field-names) |
| `getting_started.rst`: Abaqus is a supported mesh format | no Abaqus reader exists (UD-049) | [Meshes](meshes.md#supported-formats) |
| `basic_types.rst`: `_quadrangle_8` has 9 quadrature points; `_hexahedron_8` is cubic (spelled `_hexadedron_8`) | 4 points (order 3); trilinear, 8 points (order 2) | [Element appendix](appendix/elements.md#element-types) |
| `basic_types.rst`: `find` returns the index of the first occurrence or -1 | true for `Array<T>`; the `Array<Real>` specialisation matches $\lvert a - v \rvert \le \varepsilon$ and returns the distance from the end (`size − index`) when found, -1 otherwise (UD-042); the port returns `Some(index)` / `None` with upstream's comparison (D-061) | [Basic types](basic-types.md#arrayt) |
| `solidmechanicsmodel.rst`: Newton–Raphson allows 100 iterations by default | 10 (`non_linear_solver_newton_raphson.cc:73`) | [Solid mechanics](solid-mechanics.md#static-analysis) |
| `solidmechanicsmodel.rst`: `explicitPred`, `updateResidual`, `updateAcceleration`, `explicitCorr` | removed in v5; `solveStep` does all | [Solid mechanics](solid-mechanics.md#explicit-dynamics) |
| `phasefieldmodel.rst`: an `exponential` phase-field law (`phasefield exponential [ … ]`) | the registered laws are `quadratic` (AT2) and `linear` (AT1); `exponential` is refused | [Phase-field fracture](models/phase-field.md#theory) |
| `phasefieldmodel.rst`: strong form $\ldots (d - l_0^2\, d)$ | the Laplacian $d - l_0^2\, \Delta d$ is assembled | [Phase-field fracture](models/phase-field.md#theory) |
| `phasefieldmodel.rst`: the default phase-field solver "is of linear type" | the `static` default solver is Newton–Raphson with the pseudo-time scheme | [Phase-field fracture](models/phase-field.md#theory) |
| `new-constitutive-laws.rst`: register a custom law with `model.registerNewCustomMaterials<XXX>("name")` after `initFull`, then call `initMaterials()` | neither function exists (only a commented-out binding in `python/py_solid_mechanics_model.cc`); a law is registered with `MaterialFactory::getInstance().registerAllocator("name", …)` before `initFull`: at static initialisation (`examples/c++/solid_mechanics_model/new_material/local_material_damage.cc`) or from Python (`mat_factory.registerAllocator`, `examples/python/solid_mechanics_model/custom-material/custom-material.py`); the port registers in the model's factory before `init_full` | [Writing new constitutive laws](new-constitutive-laws.md#differences-from-upstreams-procedure) |
| `nodetonodecontact.rst`: N-shaped law with $\mu_0/\sqrt{1 + (v^*/v)^2}$ | $\sqrt{1 + (v/v^*)^2}$ for `n_shaped`; $(v^*/v)^2$ only for `revised_n_shaped` | [Split-node contact](models/ntn-contact.md#rate-and-state-coefficients-and-state-laws) |
| `constitutive-laws.rst`: the examples set `Plane_stress = false` (`elastic`, `sls_deviatoric`, `viscoelastic_maxwell`, `marigo`), and the text of `sls_deviatoric` names the viscosity `eta` | parameter names are case-sensitive (`parsable.cc`); the keywords are `Plane_Stress` and `Eta`, and the manual's spellings are unknown parameters, an error (UD-1130; `manual_claims_3.rs::doc010_upstream_manual_spellings_are_refused`) | [Constitutive laws](constitutive-laws.md#reading-the-keyword-tables) |
| `constitutive-laws.rst`: `Plane_stress` selects plane stress for `viscoelastic_maxwell` | the law's 2-D stiffness is plane strain whatever `Plane_Stress` says (UD-1131; `manual_claims_3.rs::doc010_maxwell_ignores_plane_stress`) | [Viscoelastic laws](laws/viscoelastic.md#generalised-maxwell-model-viscoelastic_maxwell) |
| `constitutive-laws.rst`: the internal stresses of the Maxwell elements are updated at the end of each step | upstream's code discards the update (`material_viscoelastic_maxwell.cc:253-260`): the internal stresses stay at their initial zero, so its Maxwell elements keep no stress history (UD-029); the port commits it (corrected default) | [Viscoelastic laws](laws/viscoelastic.md#generalised-maxwell-model-viscoelastic_maxwell) |
| `constitutive-laws.rst`: the Neo-Hookean law and its stress | upstream's stress, tangent and energy raise "not implemented" (UD-026); the port computes the documented law | [Neo-Hookean hyperelasticity](laws/neohookean.md#differences-from-upstream) |
| `constitutive-laws.rst`: Mazars' weight $\alpha_t$ from "the eigenvalues of the strain if the material were undamaged", not capped | the strains of the tensile principal effective stresses, $\alpha_t = \min(\ldots, 1)$ (UD-1132) | [Damage](laws/damage.md#mazars) |
| `constitutive-laws.rst`: `marigo_non_local` is the non-local Marigo law | upstream registers the local `marigo` law under that keyword and ignores the `non_local` section (UD-027); the port registers the non-local law | [Non-local laws](laws/non-local.md#the-non_local-section) |
| `constitutive-laws.rst`: the weight functions `base_weight_function`, `damaged_weight_function`, `remove_damaged_weight_function`, `remove_damaged_with_damage_rate_weight_function`, `stress_based_weight_function`; the non-local model averages the damage | the options are `base_wf`, `damage_wf` and `remove_wf` (the other two classes are not registered); only `average_on_damage = true` of the Mazars laws averages the damage (UD-1133; `manual_claims_3.rs::doc010_weight_function_options`) | [Non-local laws](laws/non-local.md#the-non_local-section) |
| `constitutive-laws.rst`: the friction coefficient of `cohesive_linear_friction` grows linearly with the damage up to `mu` | the damage factor is commented out (`Real mu = mu_max; // * damage;`): the coefficient is `mu` at every damage (UD-1135; the friction cases of the oracle replayed bitwise in `crates/akantu-cohesive/tests/cohesive_fixtures.rs`) | [Cohesive elements](models/cohesive.md#cohesive_linear_friction) |
| `solidmechanicsmodel.rst`: the contact energy of the cohesive laws is `getEnergy("contact")` | the id is `"cohesive contact"`; `"contact"` gives 0 (UD-1134; `manual_claims_3.rs::doc023_cohesive_energy_ids`) | [Cohesive elements](models/cohesive.md#energies) |
