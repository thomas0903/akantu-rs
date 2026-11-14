# Availability table of the user manual

This file is the single source of every statement the user manual
(`docs/manual/src`) and the repository `README.md` make about what this
release of the port contains. It is not a page of the book (it lives
outside `src/`).

How it is used (`crates/akantu/tests/availability/mod.rs`, run by
`cargo test -p akantu --test manual`):

* Generated text. The module table and the placeholder paragraph of
  *Getting started*, the waiting list (`models/not-yet-available.md`),
  the status line at the top of every model page and the "not yet
  available" sentences of `README.md` are generated from this table
  (`<!-- BEGIN GENERATED: availability-… -->` blocks; rewrite them with
  `AKANTU_BLESS_MANUAL=1 cargo test -p akantu --test manual`).
* Checked against the workspace. Every module the facade crate
  re-exports has exactly one row whose status matches its crate
  (`not-yet-available` ⇔ placeholder crate; `opt-in` ⇔ its facade
  feature is not a default feature); every facade feature appears in a
  row; a `crate/feature` exists and is not a default feature of that
  crate; every page and every *Pinned by* test exists; the page
  declarations (`<!-- availability: … -->`) agree with the table; in
  `SUMMARY.md` the part *Models not yet available* lists exactly the
  pages of rows that are not yet available.
* Checked sentence by sentence. Every sentence, list item, heading
  and table row of the manual and of `README.md` that says something is
  not available (not available / unavailable / not part of this release
  / not ported / being ported / a placeholder / a stub / on a branch /
  waiting for / planned / a work-package name `WP-…` / "not … yet", …) must
  name at least one subject of this table — through its module path (or
  an item of it), crate, feature, page link or one of its *Names* — or
  stand on a page whose subjects are all not yet available, and every
  subject it names must be `not-yet-available` or `not-ported` (an
  `opt-in` subject only in a sentence about the build, e.g. "without the
  feature", "in this build"). A work package or branch it names must be
  in the *Waiting for* cell of a subject it names. A sentence that says
  something is available must not name a subject that is not.

Columns: *Id* (unique); *Subject* (as it appears in generated text);
*Status*: `available` (in every default build), `opt-in` (behind a
non-default Cargo feature), `not-yet-available` (being ported or not
started), `not-ported` (deliberately not ported) or `waiting-list` (the
list page itself); *Module* (facade module); *Crate*; *Feature* (facade
feature, or `crate/feature`); *Upstream* (upstream manual chapter);
*Waiting for* (work package and branch, only for `not-yet-available`);
*Page* (the manual page that describes the subject, relative to
`docs/manual/src`); *Pinned by* (a test that shows the availability or
the refusal, `file.rs::test`); *Names* (further names that refer to the
subject: plain words match case-insensitively, `code` matches exactly).

| Id | Subject | Status | Module | Crate | Feature | Upstream | Waiting for | Page | Pinned by | Names |
|---|---|---|---|---|---|---|---|---|---|---|
| core | core types: `Array`, small tensors, element types, enums, errors | available | `akantu::core` | `akantu-core` | — | `basic_types.rst` | — | basic-types.md | — | — |
| parser | input files, parameters, command-line options | available | `akantu::parser` | `akantu-parser` | — | `io.rst` | — | input-files.md | — | — |
| comm | communicators (serial, in-process, MPI) and synchronizers | available | `akantu::comm` | `akantu-comm` | — | — | — | parallel.md | — | — |
| mesh | meshes, groups, mesh data, mesh changes, distribution | available | `akantu::mesh` | `akantu-mesh` | — | `getting_started.rst` | — | meshes.md | — | — |
| fe | element classes, quadrature, FE engine (with the structural element kernels) | available | `akantu::fe` | `akantu-fe` | — | `appendix/elements.rst` | — | appendix/elements.md | — | — |
| linalg | sparse matrices, sparse direct solvers (`eigen`, `faer`), dense algebra | available | `akantu::linalg` | `akantu-linalg` | — | — | — | — | — | — |
| solver | DOF manager, non-linear and time-step solvers | available | `akantu::solver` | `akantu-solver` | — | `io.rst` | — | solver-configuration.md | — | — |
| io | mesh readers and writers, dumpers | available | `akantu::io` | `akantu-io` | — | `io.rst` | — | output.md | — | — |
| model | model base, boundary conditions, constitutive-law framework | available | `akantu::model` | `akantu-model` | — | — | — | — | — | — |
| geometry | geometric queries on meshes (segment intersector), spatial grid | available | `akantu::geometry` | `akantu-geometry` | — | — | — | geometry.md | — | geometry module |
| solid | the solid mechanics model `SolidMechanicsModel` and the solid materials | available | `akantu::solid` | `akantu-solid` | `solid` | `solidmechanicsmodel.rst` | — | solid-mechanics.md | — | solid mechanics model, `SolidMechanicsModel` |
| cohesive | the cohesive model `SolidMechanicsModelCohesive` (one process or a distributed mesh): cohesive laws, element inserter, fragment manager | available | `akantu::cohesive` | `akantu-cohesive` | `cohesive` | `solidmechanicsmodel.rst` | — | models/cohesive.md | — | cohesive model, cohesive models, cohesive elements, `SolidMechanicsModelCohesive` |
| contact | the contact mechanics model `ContactMechanicsModel` (one process): detection, penalty resolutions with Coulomb friction | available | `akantu::contact` | `akantu-contact` | `contact` | `contactmechanicsmodel.rst` | — | models/contact.md | manual_claims_2.rs::doc012_contact_model_is_available | contact mechanics, contact mechanics model, contact model, `ContactMechanicsModel` |
| phase-field | the phase-field fracture model `PhaseFieldModel` | available | `akantu::phase_field` | `akantu-phase-field` | `phase-field` | `phasefieldmodel.rst` | — | models/phase-field.md | — | phase-field model, phase field model, phase-field fracture, `PhaseFieldModel` |
| structural | the structural mechanics model `StructuralMechanicsModel` (beams and plates, one process) | available | `akantu::structural` | `akantu-structural` | `structural` | `structuralmechanicsmodel.rst` | — | models/structural.md | — | structural mechanics model, structural model, `StructuralMechanicsModel` |
| ntn-contact | split-node contact with rate-and-state friction (one process or a distributed mesh) | opt-in | `akantu::ntn_contact` | `akantu-ntn-contact` | `ntn-contact` | `nodetonodecontact.rst` | — | models/ntn-contact.md | — | split-node contact, node-to-node contact |
| cgal | the GPL-3.0-or-later geometry of CGAL's packages: AABB tree, sphere intersector | opt-in | `akantu::geometry_cgal` | `akantu-geometry-cgal` | `cgal` | — | — | geometry.md#the-cgal-feature-gpl-30-or-later | — | CGAL geometry, sphere intersector, AABB tree |
| mumps | the MUMPS sparse solver | opt-in | — | — | `akantu-linalg/mumps` | `io.rst` | — | mumps.md | — | MUMPS |
| mpi | the MPI communicator | opt-in | — | — | `akantu-comm/mpi` | `getting_started.rst` | — | parallel.md | — | MPI back-end, MPI communicator |
| facade-mpi | `runtime::initialize` on the MPI world (the facade's MPI runtime; finalisation at exit per D-067) | opt-in | — | — | `mpi` | `getting_started.rst` | — | parallel.md | — | MPI runtime |
| scotch | Scotch partitioning | opt-in | — | — | `akantu-mesh/scotch` | `getting_started.rst` | — | parallel.md | — | Scotch |
| facade-scotch | Scotch partitioning through the facade (used by the parallel examples) | opt-in | — | — | `scotch` | `getting_started.rst` | — | parallel.md | — | Scotch partitioning of the examples |
| python | the Python interface: the `akantu` package with the extension `py11_akantu` (meshes, groups, parser, FE engine, boundary conditions, solver layer and Python solver callbacks, Python materials and selectors, dumpers, every model and coupler, NumPy views) | opt-in | — | `akantu-python` | `akantu-python/python` | `getting_started.rst` | — | models/python.md | — | Python interface, Python module, Python bindings, Python package, `py11_akantu`, foundation of the Python interface, Python subclasses of materials, Python solver callbacks |
| diffusion | the diffusion (heat transfer) model `DiffusionModel` (serial and distributed) | available | `akantu::diffusion` | `akantu-diffusion` | `diffusion` | `diffusion.rst` | — | models/diffusion.md | manual_claims_2.rs::doc015_diffusion_model_is_available | diffusion model, heat transfer, heat-transfer model, `DiffusionModel`, `HeatTransferModel` |
| couplers | the model couplers (solid–contact, solid–cohesive–contact, solid–phase-field) | available | `akantu::couplers` | `akantu-couplers` | `couplers` | `contactmechanicsmodel.rst`, `phasefieldmodel.rst` | — | models/couplers.md | crates/akantu-couplers/tests/coupler_fixtures.rs::coupler_solid_phase_field_fixtures | model couplers, couplers, coupler, solid–phase-field coupler, solid–contact coupler, `CouplerSolidPhaseField`, `CouplerSolidContact`, `CouplerSolidCohesiveContact` |
| coupler-dumpers | the model-level dumpers of the contact model and of the couplers (upstream's `setBaseName`, `addDumpField*` and `dump` called on them) | available | — | `akantu-couplers` | — | `contactmechanicsmodel.rst` | — | — | crates/akantu-couplers/tests/coupler_dump.rs::solid_contact_default_series_is_the_contact_models_dumper | coupler dumpers, model-level dumpers of the contact model and of the couplers |
| embedded | embedded interfaces (reinforcements) | available | `akantu::embedded` | `akantu-embedded` | `embedded` | — | — | models/embedded.md | crates/akantu-embedded/tests/test_embedded_interface_model.rs::test_embedded_interface_model | embedded interfaces, embedded interface model, `EmbeddedInterfaceModel` |
| igfem | IGFEM (interface-enriched elements, types `_igfem_*`; elastic and damage materials, moving interfaces on a live model, the `igfem elements` dumper; serial; the feature `igfem` implies the GPL-3.0-or-later feature `cgal`) | opt-in | `akantu::igfem` | `akantu-igfem` | `igfem` | — | — | models/igfem.md | crates/akantu-igfem/tests/test_solid_mechanics_model_igfem.rs::test_solid_mechanics_model_igfem | IGFEM |
| extra-materials | the extra materials (damage iterative, Vreepeerlings, FE², …) | opt-in | `akantu::extra_materials` | `akantu-extra-materials` | `extra-materials` | — | — | models/extra-materials.md | crates/akantu-extra-materials/tests/upstream_damage_iterative.rs::test_material_damage_iterative | extra materials, extra-materials package |
| extra-materials-non-local | the non-local extra materials (`damage_iterative_non_local`, `brittle_non_local`, `vreepeerlings_non_local`, `damage_orthotropic_iterative_non_local`; upstream package `extra_materials_non_local`) | opt-in | — | `akantu-extra-materials` | `extra-materials-non-local` | — | — | models/extra-materials.md | crates/akantu-extra-materials/tests/non_local.rs::the_four_wrappers_are_driven_by_the_solid_model_manager | non-local extra materials, extra_materials_non_local |
| non-local | non-local constitutive laws (and the non-local half of upstream's *Adding a new constitutive law*) | available | — | — | — | `constitutive-laws.rst`, `new-constitutive-laws.rst` | — | — | manual_claims_2.rs::doc_non_local_laws_are_available | non-local laws, non-local constitutive laws, non-local |
| petsc | the PETSc back-end (`PetscBackend` DOF-manager matrices, KSP linear solves, the SNES and TAO non-linear solvers), selected by the solid and phase-field models with `dof_manager_type = petsc` / `init_dof_manager("petsc")`; the phase-field model's bounded `petsc_tao` solve | opt-in | — | — | `akantu-solver/petsc` | `io.rst` | — | petsc.md | manual_claims_2.rs::doc_solver_petsc_types_are_refused | PETSc, SNES, TAO, `petsc`, `petsc_snes`, `petsc_tao`, `PetscBackend`, PETSc back-end |
| parallel-cohesive | the cohesive model on a distributed mesh (parallel insertion) | available | — | `akantu-cohesive` | — | — | — | — | manual_claims_2.rs::doc023_cohesive_model_runs_on_a_distributed_mesh | cohesive model on a distributed mesh, parallel cohesive, distributed cohesive, parallel insertion |
| parallel-cohesive-rest | facets-only insertion and the fragment manager on a distributed mesh | available | — | — | — | — | — | — | manual_claims_3.rs::doc023_fragment_manager_on_a_distributed_mesh_is_global | facets-only insertion on a distributed mesh, fragment manager on a distributed mesh |
| parallel-contact | contact mechanics on a distributed mesh | not-yet-available | — | — | — | — | not planned: upstream has no distributed support for it; the port refuses (approved exception, parity row CONT-019) | — | manual_claims_2.rs::doc012_contact_model_refuses_a_distributed_mesh | contact mechanics on a distributed mesh, parallel contact |
| parallel-ntn-contact | split-node contact on a distributed mesh (after `NtnBaseContact::init_parallel`) | opt-in | — | `akantu-ntn-contact` | `ntn-contact` | — | — | — | manual_claims_2.rs::doc013_split_node_contact_on_a_distributed_mesh | split-node contact on a distributed mesh, parallel split-node contact |
| parallel-phase-field | the phase-field model and the solid–phase-field coupler on a distributed mesh | available | — | `akantu-phase-field` | — | — | — | — | manual_claims_2.rs::doc014_phase_field_model_runs_on_a_distributed_mesh | phase-field model on a distributed mesh, distributed phase field |
| parallel-structural | the structural model on a distributed mesh | not-yet-available | — | — | — | — | not planned: upstream has no distributed support for it (UD-902); the port refuses | — | — | structural model on a distributed mesh, parallel structural |
| periodic-parallel | the distribution of periodic meshes and the periodic node synchronizer | available | — | — | — | — | — | parallel.md | — | distribution of periodic meshes, periodic distribution, periodic node synchronizer |
| cohesive-dumper | the cohesive-element dumper (`"cohesive elements"`), the facets dumper (`"facets"`) | available | — | `akantu-cohesive` | — | `io.rst` | — | models/cohesive.md | crates/akantu-cohesive/tests/cohesive_dump.rs::cohesive_dumper_intrinsic_2d_every_field | cohesive-element dumper, cohesive dumper, facets dumper |
| runtime-initialize | the all-in-one `akantu::runtime::initialize` | available | — | — | — | `getting_started.rst` | — | command-line.md | — | `akantu::runtime::initialize` |
| structural-rotary-inertia | the corrected 3-D lumped rotary inertia (exact nodal 3×3 block, D-059) | available | — | `akantu-structural` | — | — | — | models/structural.md | crates/akantu-structural/tests/lumped_rotary_blocks.rs::lumped_block_of_a_two_element_rotated_beam_equals_the_closed_form | corrected 3-D lumped rotary inertia, exact nodal 3×3 block, `Block3x3` |
| tutorials | upstream's external tutorials (Jupyter notebooks of `akantu/tutorials` on GitLab, run on Renku; the manual gives upstream's pointer, D-085) | not-ported | — | — | — | `tutorials.rst` | — | models/python.md | — | external tutorials, tutorials |
| capi | the C API (`akantu-capi` exports only its version string) | not-yet-available | — | `akantu-capi` | — | — | not planned for now: upstream has none (approved exception, D-100) | — | — | C API, C ABI |
| examples | upstream's compiled examples (`examples/c++`) as programs of the port (`crates/akantu/examples`) | available | — | — | — | `examples.rst` | — | examples.md | — | compiled examples, solid-mechanics and cohesive examples, friction examples, examples of the other models |
| laws-chapter | the catalogue of the constitutive laws in this manual (parity row DOC-010) | available | — | — | — | `constitutive-laws.rst` | — | — | — | constitutive-laws catalogue, laws catalogue |
| cohesive-chapter | the cohesive chapter of this manual: insertion, laws, intrinsic and extrinsic use, output (parity rows DOC-023, DOC-007) | available | — | — | — | `solidmechanicsmodel.rst` | — | — | — | manual chapter on the cohesive model, cohesive chapter |
| emacs-mode | upstream's Emacs input mode (`akantu-input-mode.el`) | not-ported | — | — | — | — | — | appendix/conventions.md | — | Emacs mode, Emacs input mode, `akantu-input-mode.el` |
| snippets | upstream's code snippets (yasnippet) | not-ported | — | — | — | — | — | appendix/conventions.md | — | snippets, yasnippets |
| uml | upstream's UML diagram (`doc/uml.dia`) | not-ported | — | — | — | — | — | appendix/conventions.md | — | UML diagram, `doc/uml.dia` |
| mumps-sequential | the sequential MUMPS library (upstream's `_not_parallel` mode; serial runs use `eigen`) | not-ported | — | — | — | — | — | mumps.md | — | sequential MUMPS library, sequential MUMPS |
| waiting-list | the list of features not yet available | waiting-list | — | — | — | — | — | models/not-yet-available.md | — | waiting list, models not yet available |
| python-examples | upstream's Python examples run unchanged with the `akantu` package and compared with the upstream module (`python/examples`), and the example `python_user_defined_bc`, a Rust program embedding CPython (`py11_akantu::embed`) | opt-in | — | `akantu-python` | `python` | — | — | — | — | Python examples, embedded Python, `python_user_defined_bc` |
