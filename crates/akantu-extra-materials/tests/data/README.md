# Upstream test inputs of the extra-materials package

Verbatim copies (LGPL-3.0-or-later, © EPFL/LSMS) of the committed input files
of `extra_packages/extra-materials/test/` of the reference
(`35a851780d0b9471d258ed53a003ae98aaa95ea0`), used by the ported tests of
`crates/akantu-extra-materials/tests/`:

| File | Upstream |
|---|---|
| `test_material_damage/*` | `extra_packages/extra-materials/test/test_material_damage/` (meshes, `.dat`, the `.verified` reference output) |
| `test_material_FE2/*` | `extra_packages/extra-materials/test/test_material_FE2/` (committed meshes and `.dat`) |

Gmsh-generated meshes (`homogenized_plate.msh`, the generated
`periodic_plate.msh`, `energy_study.msh`) are the frozen copies of
`fixtures/meshes/gmsh-4.13.1/extra_packages/extra-materials/` (D-009).
`glibc_rand_srand_0_1_12345.txt` holds the first 64 values of the C library's
`rand()` after `srand(0)`, `srand(1)` and `srand(12345)` (black-box output of
a 10-line C program, `rand_probe.c`, compiled with glibc 2.39 on an x86_64 host
Ubuntu 24.04 and with the oracle image's Debian trixie glibc: identical),
the reference values of the port's gel-pocket generator (UD-586, D-030: no
library source was read).

`mixed/mixed_t3_q4.msh` is not an upstream file: a hand-written 6-node unit
square (one `_quadrangle_4` on `[0, 0.25] × [0, 1]`, two `_triangle_3` on
the rest, boundary lines `bottom`/`right`/`top`/`left`) written by
for the multi-type tests of UD-573 and UD-585;
no upstream test uses a mesh with several element types.
