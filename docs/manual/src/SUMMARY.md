# Summary

[Introduction](introduction.md)

# User guide

- [Getting started](getting-started.md)
- [Basic types and iteration](basic-types.md)
- [Meshes](meshes.md)
- [Input files](input-files.md)
- [Solver configuration](solver-configuration.md)
  - [The MUMPS sparse solver](mumps.md)
  - [The PETSc back-end](petsc.md)
- [Materials](materials.md)
  - [Constitutive laws](constitutive-laws.md)
    - [Neo-Hookean hyperelasticity](laws/neohookean.md)
    - [Viscoelastic laws](laws/viscoelastic.md)
    - [Plasticity](laws/plasticity.md)
    - [Damage](laws/damage.md)
    - [Non-local laws](laws/non-local.md)
    - [The phase-field material](laws/phasefield.md)
  - [Writing new constitutive laws](new-constitutive-laws.md)
- [Command-line and environment options](command-line.md)
- [Solid mechanics model](solid-mechanics.md)
- [Structural mechanics model](models/structural.md)
- [Output and dumpers](output.md)
- [Parallel computations](parallel.md)
- [Geometry and the cgal feature](geometry.md)
- [Examples](examples.md)

# Models

- [Cohesive elements](models/cohesive.md)
- [Contact mechanics](models/contact.md)
- [Phase-field fracture](models/phase-field.md)
- [Diffusion (heat transfer)](models/diffusion.md)
- [Model couplers](models/couplers.md)
- [Split-node contact and rate-and-state friction](models/ntn-contact.md)
- [Extra materials](models/extra-materials.md)
- [Embedded interfaces](models/embedded.md)
- [IGFEM](models/igfem.md)
- [Python interface](models/python.md)

# Models not yet available

- [Overview](models/not-yet-available.md)

# Porting from the C++ library

- [Differences from the C++ library](differences.md)
- [Upstream changes](upstream-changes.md)

# Appendix

- [Elements](appendix/elements.md)
- [Coding conventions and design artefacts](appendix/conventions.md)
- [The JOSS paper's claims](appendix/joss.md)
