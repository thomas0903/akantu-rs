# Reference implementation facts

The port follows one commit of the C++ library. Every comparison, inventory
and fixture in this repository refers to it.

| Item | Value |
|---|---|
| Upstream | https://gitlab.com/akantu/akantu |
| Commit | `35a851780d0b9471d258ed53a003ae98aaa95ea0` |
| `git describe` | `v5.0.7-post1-554-g35a851780` |
| Commit date | 2026-10-30 (merge of `features/agents-md-and-lint-config`) |
| License | LGPL-3.0-or-later (`COPYING`, `COPYING.lesser`; file headers say "version 3 ... or any later version") |
| Language | C++20 (`AKANTU_CXX_STANDARD 20`), CMake ≥ 3.16.3 |
| Size | 535 `.hh/.cc` files, ~107k lines in `src/`; 215 test sources; 123 registered tests |

The scripts that read the reference (the oracle build, the probes, the
upstream data manifest) take the path of a local checkout from the
environment variable `AKANTU_SRC_HOST`. They refuse a checkout that is not
exactly this commit or that has modified or untracked files.
`docs/ORACLE_BUILD.md` describes the build.

## CMake packages (`packages/*.cmake`)

| Package | Default | Depends | Notes |
|---|---|---|---|
| core | mandatory | akantu_iterators, Boost, Eigen3 | |
| akantu_iterators | ON | | header-only, `third-party/akantu_iterators` |
| Boost | mandatory | | preprocessor, Spirit (parser), graph |
| Eigen3 | mandatory | | dense linear algebra, also used as sparse solver backend |
| IOHelper / dumpers | ON | | `third-party/iohelper`, ParaView/VTU output |
| solid_mechanics | ON | core | |
| cohesive_element | ON | solid_mechanics | |
| contact_mechanics | ON | model_couplers, cohesive_element | |
| diffusion | ON | | heat transfer / diffusion |
| phase_field | ON | model_couplers | |
| model_couplers | advanced | | |
| structural_mechanics | OFF | implicit | beams, Kirchhoff shell |
| damage_non_local | OFF | | non-local damage laws |
| embedded | OFF | CGAL | embedded reinforcement model |
| CGAL | external | | geometry (AABB trees) |
| implicit | META | | |
| parallel | META | MPI, Scotch | |
| MPI | external | | |
| Scotch | external | | mesh partitioning |
| Mumps | external | | sparse direct solver |
| PETSc | external | parallel | sparse solvers |
| ScaLAPACK | external | MPI | |
| python_interface | OFF | | pybind11 |
| debug_tools | OFF | | |
| documentation | OFF | | |
| gtest | external | | |

Extra packages (`extra_packages/`): `extra-materials`, `igfem`,
`traction-at-split-node-contact`. At this commit `igfem`, `extra-materials`
and `embedded` do not compile (`docs/ORACLE_BUILD.md`, "Known upstream build
problems").

The upstream CMake preset `common` enables COHESIVE_ELEMENT, IMPLICIT,
PARALLEL, STRUCTURAL_MECHANICS, DIFFUSION, DAMAGE_NON_LOCAL, PHASE_FIELD,
CONTACT_MECHANICS and TRACTION_AT_SPLIT_NODE_CONTACT; `devcontainer` adds
EXAMPLES, `IMPLICIT_SOLVER=Mumps+PETSc` and PYTHON_INTERFACE. How the port's
cargo features and `ci/run-all.sh` presets correspond to them is in
`ci/MAPPING.md`.

The upstream CI image `test/ci/debian:stable/Dockerfile` (Debian trixie,
OpenMPI, MUMPS, Scotch, PETSc, Boost, Eigen, HDF5, Python3/numpy/scipy/mpi4py,
gmsh) is the basis of the oracle image (`docs/ORACLE_BUILD.md`).

The feature-by-feature status of the port is `docs/FEATURE_PARITY.md`; the
file-by-file reading of the upstream source is `docs/inventory/`.

## External reference material

| Repository | Commit | Licence | Role |
|---|---|---|---|
| https://gitlab.com/akantu/performance-testing-cohesive | `483bace8a03850ae49ee2feb68636d2f64eae3ee` | the repository carries no licence file; the driver `cohesive_extrinsic.cc` has an LGPL-3.0-or-later header, which `benchmarks/cohesive-perf/src/` keeps; the copied data files (`results/*.csv`, `cube.geo`, the `material*.dat` inputs) carry no notice, and no licence is asserted for them: they are reproduced with attribution as published reference measurements and will be removed at the authors' request (`NOTICE.md`) | Akantu's JOSS scalability test: `SolidMechanicsModelCohesive` extrinsic, 3-D cube (`cube.msh` 209 MB, `h = 0.002` in `cube.geo`), finite-deformation elastic + `cohesive_linear`, a static solve then 5000 explicit lumped steps with `checkCohesiveStress`, per-phase Chrono CSV; results for Akantu v4.0.1 and v5.0.4 on EPFL *jed* at 1 to 720 ranks (`results/*.csv`). The port reproduces this benchmark (`docs/BENCHMARKS.md`). |
