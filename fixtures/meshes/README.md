# Frozen Gmsh-generated meshes (decision D-009)

The reference generates these meshes at **build time** from `.geo` files with
`add_mesh()` (`cmake/Modules/FindGMSH.cmake`):
`gmsh -<DIM> -order <ORDER> -o <binary dir>/<OUTPUT> <geo>`. Their content
depends on the Gmsh version, so they are frozen here, generated with the
Gmsh of the oracle image (`akantu-oracle:dev`, Gmsh 4.13.1, Debian package
`4.13.1+ds1-6`; all files are MSH 4.1 ASCII).

* `gmsh-4.13.1/<path>` — one file per `add_mesh()` call of `test/`,
  `examples/` and `extra_packages/` of the reference
  (`35a851780d0b9471d258ed53a003ae98aaa95ea0`), at the path of its build
  directory (e.g. `gmsh-4.13.1/test/test_mesh_utils/test_mesh_io/cube.msh`).
* `frozen_meshes.json` — per mesh: CMake file and target, `.geo` path and
  SHA-256, dimension, order, output path, SHA-256 and size of the mesh, MSH
  format, and the comparison with the copy in the oracle build tree:
  87 are **byte-identical** to the meshes the oracle build generated, 33 belong
  to targets the oracle does not build (generated with the same command), and
  for one (`extra_packages/.../test_material_FE2/periodic_plate.msh`) the build
  tree holds the committed file of the same name (copied by another test's
  `FILES_TO_COPY`; the Gmsh target was not built).
* Every file is also indexed in `fixtures/SHA256SUMS`.

Regenerate (and re-verify against the oracle build tree):
`oracle/probes/mesh_io/freeze-meshes.sh`. Golden `Mesh::read` fixtures of these
meshes are the `fixtures/mesh_io/generated/...` cases (`mesh_probe`).
