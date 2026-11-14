# Upstream PETSc reference outputs

Copied verbatim from the reference Akantu tree (LGPL-3.0-or-later, EPFL/LSMS),
at commit `35a851780d0b9471d258ed53a003ae98aaa95ea0`:

| File | Upstream path | Upstream test |
|---|---|---|
| `test_model_solver_petsc.verified` | `test/test_model/test_common/test_model_solver/test_model_solver_petsc.verified` | `test_model_solver_petsc` (`test_model_solver.cc`, `DOF_MANAGER_TYPE="petsc"`) |
| `test_model_solver_dynamic_petsc.verified` | `test/test_model/test_common/test_model_solver/test_model_solver_dynamic_petsc.verified` | `test_model_solver_dynamic_petsc` (`test_model_solver_dynamic.cc`, `EXPLICIT=false`, `DOF_MANAGER_TYPE="petsc"`) |

These are the printed tables upstream compares (7 resp. 6 significant digits);
the Rust ports (`crates/akantu-solver/tests/petsc_mpi.rs`) compare their own
printed tables with them, ignoring white space, as upstream's `.verified`
mechanism does. Both pass in the oracle build (`oracle/results/ctest-strict.json`,
aarch64, PETSc 3.22.5).
