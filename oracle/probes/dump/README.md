# Dumper probe

`dump_probe.cc` drives the dumpers of the installed oracle
(`/build/install-strict`, image `akantu-oracle:dev`) — Akantu's
`DumperParaview`, `DumperText`, element/node-group dumpers, the
solid-mechanics field pipeline, and the raw IOHelper ParaView, text and
LAMMPS writers — on small hand-built meshes, and records the files they
write as `fixtures/dump/<case>/`. The Rust differential tests
(`crates/akantu-io/tests/dump_oracle.rs`, `dump_smm.rs`, `test_dumper.rs`)
rebuild each case and compare the files byte for byte.

```sh
oracle/probes/dump/run.sh      # from the repository root (Docker, read-only oracle volume)
```

| Case | What | Upstream outcome |
|---|---|---|
| `m2` | 2-D mixed mesh (tri3, quad4, seg2): padded/strided/bool/int/uint nodal fields, real/int elemental fields, `material_index`, `partitions`, homogenised quadrature field, mesh data `tag_0`; base64 + ASCII; `.pvd` time series; `dump()`, `dump(step)`, `dump(time, step)`; all-dimension registration | deterministic |
| `m2_raw_qp` | non-homogenised quadrature field over types with 3 and 4 points | writes 9 or 12 values per tuple under `NumberOfComponents="9"` |
| `all_types` | one element of each of the 21 dumpable types (VTK codes, node reorders) | deterministic |
| `coh3d16`, `dkt18` | `_cohesive_3d_16`, `_discrete_kirchhoff_triangle_18` | exception `getIOHelperType : not implemented yet !` (UD-053) |
| `groups` | element-group dumper (filtered mesh and fields, base64/ASCII), node-group dumper (point set) | deterministic |
| `parallel` | `setParallelContext(p, n)` for n = 3 and 11 (rank widths 1, 2) | deterministic |
| `text` | `DumperText` space + CSV, filtered mesh, fields, scalar/vector/int variables, precision, time file, second `setTimeStep` | deterministic |
| `text_mpi_mesh` | 2 MPI ranks, `registerMesh` | `std::bad_cast` (UD-052) |
| `text_mpi_filtered` | 2 MPI ranks, `registerFilteredMesh`, every `NodeFlag` | deterministic |
| `lammps` | IOHelper `DumperLammps<atomic/bond>`, bounds, two dumps per writer | deterministic (stream state carries over) |
| `iohelper` | IOHelper's own tests (`test_paraview_dumper`, `test_text_dumper`) on `cube.msh` | deterministic |
| `smm2d` | 2-D solid mechanics (plane strain + plane stress elastic laws): derived fields, padded stress | `smm2d_ascii_pad` VTU nondeterministic (uninitialised padding) |
| `smm2d_strain_pad` | padded 2-D strain | aborts at field registration (bare `throw;` in `FieldComputeProxy::connectToFunctor`, `dumper_compute.hh:354`; UD-112) |
| `smm3d` | 3-D solid mechanics (hex8 + tet4) | deterministic; matrix fields written as 81/9 values per cell |

Each case writes `inputs.txt` (exact input arrays as IEEE-754 bit patterns,
fed to the Rust resolvers), `status.txt` (exit code, stdout, stderr) and,
when two runs differ, `NONDETERMINISTIC`. `fixtures/dump/PROVENANCE.txt`
records the oracle, image and compiler; `fixtures/dump/SHA256SUMS` indexes
every file.
