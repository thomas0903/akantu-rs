<!-- availability: available akantu::io -->
# Output and dumpers

This chapter corresponds to *Output data* (`io.rst`) of the upstream
manual. Models write their fields as ParaView VTK files (`.pvtu`,
`.vtu`, `.pvd`) with upstream's names and layout, so existing
post-processing scripts keep working. The files are byte-identical to
upstream's IOHelper output for the deterministic cases of the oracle
corpus (`fixtures/dump`, parity row DUMP-004), except where upstream's
layout is defective: by default matrix-valued fields are written with
their exact component count and short base64 arrays are encoded validly
(UD-110, UD-113, UD-114), and only `DumperParaview::set_compat_layout(true)`
reproduces upstream's bytes there ([below](#low-level-dumpers-and-external-data)).
Text (CSV) and LAMMPS writers are available through the low-level API.

## Dumping a model

The solid mechanics model owns a default ParaView dumper. As upstream, the
files go to `./paraview/` unless another directory is set, and the file
base name is the model id unless another is set:

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
# let parser = Parser::parse_str(
#     "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#     "material.dat",
# )?;
# let mut model =
#     SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
# model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
# let output_dir = std::env::temp_dir().join(format!("akantu-manual-output-{}", std::process::id()));
// `model` is an explicit solid mechanics model of the unit square
model.set_directory(&output_dir)?; // default: ./paraview
model.set_base_name("square")?; // default: the model id

// upstream: model.addDumpFieldVector("displacement"); model.addDumpField("stress");
model.add_dump_field_vector("displacement")?; // padded to 3 components for ParaView
model.add_dump_field("stress")?; // averaged over the quadrature points of each element
model.add_dump_field("Von Mises stress")?;
model.add_dump_field("material_index")?;

model.set_time_step(1e-6, None)?; // also gives the dumper its time step (.pvd)
model.dump(None, None)?; // step 0
model.solve_step(None)?;
model.dump(None, None)?; // step 1, t = 1e-6

for f in ["square_0000.pvtu", "square_0001.pvtu", "square-VTU/square_0000.vtu", "square.pvd"] {
    assert!(output_dir.join(f).exists(), "{f}");
}
let pvd = std::fs::read_to_string(output_dir.join("square.pvd"))?;
assert!(pvd.contains(r#"timestep="1e-06""#) && pvd.contains(r#"file="square_0001.pvtu""#));
# std::fs::remove_dir_all(&output_dir)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

Files: `<dir>/<base>_<step>.pvtu` (one per dump, listing the pieces of
every rank), `<dir>/<base>-VTU/<base>_<step>[.proc<rank>].vtu` (the
data), and `<dir>/<base>.pvd` (the time series, written once the dumper
knows its time step). `dump(time, step)` overrides the time and the step
counter (upstream `dump(time, step)`).

As upstream, quadrature-point fields are written as element fields
averaged over the quadrature points of each element;
`add_dump_field_vector` and `add_dump_field_tensor` pad vectors to 3
components and 2-D tensors to $3 \times 3$ (with the out-of-plane stress or
strain of plane strain/plane stress), because ParaView handles 3-D data.
A 2-D stress, for example, is written as

$$
\begin{pmatrix}
\sigma_{xx} & \sigma_{xy} & 0 \\
\sigma_{yx} & \sigma_{yy} & 0 \\
0 & 0 & \sigma_{zz}
\end{pmatrix},
$$

$\sigma_{zz}$ being the out-of-plane stress of plane strain (zero in
plane stress); the out-of-plane shear components are zero (UD-111,
below).

## Field names

The names of the upstream manual's table, checked against
`SolidMechanicsModel::createNodalFieldReal/Bool` and
`createElementalField` of upstream and against the port by
`crates/akantu/tests/manual_claims.rs`:

| Name | Support | Values | Note |
|---|---|---|---|
| `displacement` | nodes | `dim` (3 padded) | |
| `velocity`, `acceleration` | nodes | `dim` | dynamic analyses only |
| `external_force`, `internal_force` | nodes | `dim` | upstream manual: `force`, `residual` |
| `mass` | nodes | `dim` | once the lumped mass has been assembled (explicit lumped analyses) |
| `increment` | nodes | `dim` | |
| `blocked_dofs` | nodes | `dim` booleans | |
| `partitions` | elements | 1 | rank owning the element |
| `material_index` | elements | 1 | index of the element's material |
| `stress`, `grad_u`, `eigen_grad_u` | quadrature points | `dim²` | material internals |
| `strain`, `Green strain` | quadrature points | `dim²` | computed from `grad_u` |
| `principal strain`, `principal Green strain` | quadrature points | `dim` | eigenvalues (unsorted, as upstream) |
| `Von Mises stress` | quadrature points | 1 | computed from `stress` |
| `potential_energy` and any internal of the materials (`sigma_th`, `epsilon_th`, `delta_T`, …) | quadrature points | per internal | zero for elements whose material lacks it |
| `tag_0`, `tag_1`, … (mesh data) | elements | per dataset | numeric mesh data only |

The names `force` and `residual` of the upstream manual's table are no
longer accepted by upstream's code (`force` was renamed `external_force`;
`residual` was replaced by `internal_force`, which is not the same
quantity); both upstream and the port reject them with
"The 'force' field has been renamed in 'external_force'" and "The
'residual' field has been replaced by 'internal_force'". A name that is
none of the above must be numeric elemental mesh data; any other name (an
unknown name, a material internal before `init_full` creates the
materials, string mesh data such as `physical_names`, which has no VTK
representation) is refused when it is registered (`Error::NotFound`), as
upstream's `addDumpField` refuses it, and the dumper stays usable
(UD-683). A known model array that is created only later (`mass`, at the
first lumped-mass assembly) is accepted and read when the model dumps;
upstream refuses it at registration.

## Group dumpers

A dumper restricted to an element group (upstream
`ElementGroup::registerDumper` / `model.dumpGroup`):

```rust
# use akantu::core::enums::{AnalysisMethod, MeshIoType};
# use akantu::mesh::{Mesh, SharedMesh};
# use akantu::model::ModelOptions;
# use akantu::parser::Parser;
# use akantu::solid::SolidMechanicsModel;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let mut mesh = Mesh::new(2, "mesh")?;
# akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
# let parser = Parser::parse_str(
#     "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n",
#     "material.dat",
# )?;
# let mut model =
#     SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "solid_mechanics_model", parser)?;
# model.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
# let output_dir = std::env::temp_dir().join(format!("akantu-manual-group-{}", std::process::id()));
model.register_group_dumper("right", Some("right_side"))?;
model.set_directory_to_dumper("right", &output_dir)?;
model.add_dump_group_field_to_dumper("right", "displacement", "right", true)?;
model.dump_named("right", None, None)?;
assert!(output_dir.join("right_side_0000.pvtu").exists());
# std::fs::remove_dir_all(&output_dir)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Cohesive elements and fragments

The cohesive model registers a second dumper, `"cohesive elements"`,
which writes the cohesive elements with their own fields (`opening`,
`tractions`, `damage`, …), and its fragment manager computes per-fragment
data (mass, centre of mass, mean velocity, principal moments of inertia,
number of elements) that the program reads from public arrays. Both are
described with the cohesive model:
[Cohesive elements' data](models/cohesive.md#cohesive-elements-data) and
[Fragmentation data](models/cohesive.md#fragmentation-data).

## Low-level dumpers and external data

The dumpers of `akantu::io` can be used directly, for example to write
data that does not belong to a model (upstream `DumperParaview`,
`DumperText` and `addDumpFieldExternal`). A dumper stores field
descriptors and reads the values at dump time from a `FieldResolver`
that the caller implements over its own arrays (upstream keeps references
to the arrays instead, which dangle when the mesh changes):

```rust
use akantu::core::enums::MeshIoType;
use akantu::core::{ElementKind, ElementType, Error, GhostType, Result};
use akantu::io::dumper::{DumpBuffer, Dumper};
use akantu::io::{DumperText, FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType, Padding, TextDumpMode};
use akantu::mesh::Mesh;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");
# let output_dir = std::env::temp_dir().join(format!("akantu-manual-text-{}", std::process::id()));

/// A nodal temperature computed by the application.
struct Temperature(Vec<f64>);

impl FieldResolver for Temperature {
    fn resolve(&self, _: &Mesh, field: &FieldDescriptor, ty: Option<ElementType>, _: GhostType) -> Result<DumpBuffer> {
        match ty {
            None => Ok(DumpBuffer::Real { nb_component: 1, values: self.0.clone() }),
            Some(_) => Err(Error::not_found("field", field.name.clone())),
        }
    }
}

let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
let temperature = Temperature(mesh.nodes().tuples().map(|x| 300.0 + 10.0 * x[0]).collect());

// upstream: DumperText dumper("temperature", _tdm_csv); dumper.registerMesh(mesh, 2);
let mut dumper = DumperText::new("temperature", TextDumpMode::Csv, false);
dumper.set_directory(&output_dir)?;
dumper.register_mesh(2, GhostType::NotGhost, ElementKind::NotDefined)?;
dumper.register_field(FieldDescriptor {
    name: "temperature".into(),
    source: FieldSource::ModelNodal("temperature".into()),
    support: FieldSupport::Nodal,
    value_type: FieldValueType::Real,
    group: None,
    padding: Padding::None,
    homogenize: false,
})?;
dumper.dump(&mesh, &temperature, None, None)?;
assert!(std::fs::read_dir(&output_dir)?.count() > 0);
# std::fs::remove_dir_all(&output_dir)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`DumperParaview::new(base, directory, parallel)` is used the same way.
The models' own resolvers are what `model.dump` passes to its dumpers.

Corrected by default, with `DumperParaview::set_compat_layout(true)`
reproducing upstream's bytes: matrix-valued fields are written with
exactly `NumberOfComponents` values per tuple (upstream pads `strain` to
`dim × dim` zero-filled squares, e.g. 81 values for a 9-component tensor,
UD-110), base64 arrays shorter than 2 bytes are encoded correctly
(UD-114); out-of-plane shear components of padded 2-D stresses are zero
instead of uninitialised (UD-111).
