# Test inputs of the model-coupler tests

Verbatim copies of upstream test inputs (Akantu, EPFL/LSMS, LGPL-3.0-or-later)
from the reference tree at `35a851780d0b9471d258ed53a003ae98aaa95ea0`:

| File | Upstream path |
|---|---|
| `test_phase_field_model/material_coupling.dat` | `test/test_model/test_phase_field_model/material_coupling.dat` |
| `test_phase_field_model/material_deviatoric.dat` | `test/test_model/test_phase_field_model/material_deviatoric.dat` |
| `test_phase_field_model/material_linear.dat` | `test/test_model/test_phase_field_model/material_linear.dat` |
| `test_phase_field_model/material_multiple.dat` | `test/test_model/test_phase_field_model/material_multiple.dat` |
| `test_phase_field_model/material_penalization.dat` | `test/test_model/test_phase_field_model/material_penalization.dat` |
| `test_phase_field_model/test_one_element.{geo,msh}` | `test/test_model/test_phase_field_model/test_one_element.{geo,msh}` (committed upstream) |
| `test_phase_field_model/test_two_element.{geo,msh}` | `test/test_model/test_phase_field_model/test_two_element.{geo,msh}` (committed upstream) |
| `test_phase_field_model/test_four_elements.{geo,msh}` | `test/test_model/test_phase_field_model/test_four_elements.{geo,msh}` (committed upstream) |

The contact meshes are read from the frozen corpus `fixtures/meshes/gmsh-4.13.1`.

`v80_oracle/<mode>.run1.txt` are outputs of the oracle probe
`oracle/probes/couplers_v80/` (first of three identical runs per mode), read by
`tests/v80_review.rs`. Regenerate them with `oracle/probes/couplers_v80/run.sh`.
