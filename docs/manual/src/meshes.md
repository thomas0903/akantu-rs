<!-- availability: available akantu::mesh -->
# Meshes

This chapter corresponds to the sections *Creating and loading a mesh*
(`getting_started.rst`) and *Mesh* (`basic_types.rst`) of the upstream
manual.

## Creating and reading a mesh

A `Mesh` is created empty with its spatial dimension and an identifier,
then filled by a reader. `read_mesh` infers the format from the file
extension with `MeshIoType::Auto` (`.msh` → Gmsh, `.diana` → Diana); any
other extension needs an explicit format.

```rust
use akantu::core::enums::MeshIoType;
use akantu::io::{detect_format, read_mesh};
use akantu::mesh::Mesh;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");

// upstream: Mesh mesh(2); mesh.read("square.msh");
let mut mesh = Mesh::new(2, "mesh")?;
read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;
assert_eq!(mesh.nb_nodes(), 9);
assert_eq!(mesh.nodes().tuple(4), &[0.5, 0.5]); // node coordinates

assert_eq!(detect_format("bar.msh".as_ref())?, MeshIoType::Gmsh);
assert!(detect_format("bar.mesh".as_ref()).is_err()); // "Cannot guess the type of file"
# Ok::<(), akantu::core::Error>(())
```

A read is transactional: the file is parsed and validated completely
before the mesh is modified, so a malformed file leaves the mesh unchanged
and returns an `Error::Parse` carrying `file:line:column` (upstream may
read uninitialised data or abort, see UD-064, UD-090, UD-095 in
[Differences](differences.md)). `read_mesh_with` returns a `ReadReport`
with the format version and the warnings upstream prints
(`ReadOptions` selects the upstream-compatible behaviours listed below).

### Supported formats

| `MeshIoType` | Read | Write |
|---|---|---|
| `Gmsh` (`_miot_gmsh`) | ASCII MSH 1.0, 2.0–2.2, 4.0, 4.1 | ASCII MSH 2.2 |
| `GmshStruct` (`_miot_gmsh_struct`) | as `Gmsh`, reinterpreting element types for structural elements | ASCII MSH 2.2 |
| `Diana` (`_miot_diana`) | yes | no (`Error::NotImplemented`, as upstream) |
| `Abaqus` (`_miot_abaqus`) | no (`Error::Unsupported`) | no |

Binary MSH files are refused with `Error::Unsupported`. The upstream
manual lists Abaqus among the supported formats, but upstream has no
Abaqus reader (`_miot_abaqus` dereferences a null reader, UD-049); the
port refuses it explicitly (decision D-027).

Gmsh element types that Akantu does not support (e.g. pyramids, 9-node
quadrangles) are skipped with a warning, as upstream.

## Gmsh tags, physical names and groups

The Gmsh reader stores the element tags as mesh data, as upstream:

| Mesh data | Type | Content |
|---|---|---|
| `tag_0` | `Int` | physical tag |
| `tag_1` | `Int` | geometrical (elementary) tag |
| `tag_<j>` | `Int` | further tags of MSH 2 files |
| `physical_names` | `String` | name of the physical group, when the file has `$PhysicalNames` |

When the file has physical names, an element group is created for each
physical name, together with a node group `<name>_nodes` holding the
nodes of its elements (upstream `createGroupsFromMeshData<std::string>
("physical_names")`). These groups are what boundary conditions refer to.

```rust
use akantu::core::enums::MeshIoType;
use akantu::core::{ElementType, GhostType};
use akantu::mesh::Mesh;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");

let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

// mesh data per element type
let seg = ElementType::Segment2;
let physical = mesh.data().ints("tag_0")?.get(seg, GhostType::NotGhost)?;
let names = mesh.data().strings("physical_names")?.get(seg, GhostType::NotGhost)?;
assert_eq!(physical.as_slice()[..2], [1, 1]);
assert_eq!(names.as_slice()[..2], ["left".to_string(), "left".to_string()]);

// element groups created from the physical names
let left = mesh.groups().element_group("left")?;
assert_eq!(left.elements_of(seg, GhostType::NotGhost), &[0, 1]);
assert_eq!(left.nodes(), &[0, 3, 6]);
assert_eq!(left.group().dimension, 1);
assert_eq!(mesh.groups().node_group("left_nodes")?.nodes(), [0, 3, 6]);
# Ok::<(), akantu::core::Error>(())
```

For MSH 4 files, physical groups without a name get a group
`msh_block_<tag>`; upstream creates these groups but leaves them empty
(UD-045). The port fills them by default; `ReadOptions {
fill_unnamed_physical_groups: false, .. }` reproduces upstream.

## Working with groups

Groups are managed by the mesh's `GroupManager` (`mesh.groups()`,
`mesh.groups_mut()`), as upstream's `GroupManager` base class of `Mesh`.
The example below is the Rust version of the two examples of the upstream
manual: a node group of the nodes with a positive $x$ coordinate and an
element group of the elements whose barycenter has a positive $x$
coordinate (here: $x > 0.5$).

```rust
use akantu::core::enums::MeshIoType;
use akantu::core::TypeFilter;
use akantu::mesh::Mesh;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");

let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

// upstream: auto & group = mesh.createNodeGroup("XpositiveNode");
//           for (auto && [node, x] : enumerate(make_view(nodes, dim))) …
let selected: Vec<i32> = mesh
    .nodes()
    .tuples()
    .enumerate()
    .filter(|(_, x)| x[0] > 0.5)
    .map(|(n, _)| n as i32)
    .collect();
let group = mesh.groups_mut().create_node_group("XpositiveNode")?;
for n in selected {
    group.add(n, true);
}
assert_eq!(group.nodes(), [2, 5, 8]);

// upstream: auto & group = mesh.createElementGroup("XpositiveElement");
//           for_each_element(mesh, [&](auto && el) { … group.add(el); });
mesh.groups_mut().create_element_group("XpositiveElement", 2)?;
let elements: Vec<_> = mesh.elements(TypeFilter::new().dim(2)).collect();
for el in elements {
    if mesh.barycenter(el)?[0] > 0.5 {
        mesh.element_group_add("XpositiveElement", el, true, true)?;
    }
}
let group = mesh.groups().element_group("XpositiveElement")?;
assert_eq!(group.group().iter_elements().count(), 2);
// nodes are appended in element order, as upstream (`optimize()` sorts them)
assert_eq!(group.nodes(), &[1, 2, 5, 4, 8, 7]);
# Ok::<(), akantu::core::Error>(())
```

Further group operations (`copy_element_group`, `rename_element_group`,
`append_element_group`, `create_element_group_from_node_group`,
`create_groups_from_mesh_data`, …) keep the upstream names in snake case.
Unlike upstream, copying an element group also copies its nodes (UD-124)
and destroying a node group that an element group still uses is refused
(UD-125).

## Writing a mesh

`write_mesh` writes Gmsh MSH 2.2 (ASCII). Coordinates are written with 17
significant digits so that a written mesh reads back bit for bit (upstream
writes 15, UD-061).

```rust
use akantu::core::enums::MeshIoType;
use akantu::io::{read_mesh_from_str, write_mesh_to_string, ReadOptions, WriteOptions};
use akantu::mesh::Mesh;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");

let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

let text = write_mesh_to_string(&mesh, "out.msh".as_ref(), MeshIoType::Auto, &WriteOptions::default())?;
assert!(text.starts_with("$MeshFormat\n2.2 0 8\n"));

let mut again = Mesh::new(2, "again")?;
read_mesh_from_str(&mut again, "out.msh", &text, MeshIoType::Gmsh, &ReadOptions::default())?;
assert_eq!(again.nodes().as_slice(), mesh.nodes().as_slice());
# Ok::<(), akantu::core::Error>(())
```

## Other mesh utilities

| Upstream | Rust |
|---|---|
| `mesh.getBarycenter(element, x)` | `mesh.barycenter(element)`, `mesh.barycenter_into(element, &mut x)` |
| `mesh.getBBox()`, `getLowerBounds()`, `getUpperBounds()` | `mesh.bbox()`, `lower_bounds()`, `upper_bounds()` |
| `MeshUtils::buildFacets`, `buildNode2Elements`, `purifyMesh` | `akantu::mesh::utils` |
| element/node removal (`eraseElements`) | `akantu::mesh::removal` |
| periodicity (`makePeriodic`) | `akantu::mesh::periodic` |
| `mesh.distribute()` | `akantu::mesh::distribution` ([Parallel computations](parallel.md)) |

Topology changes of a mesh that is used by a model go through
`SharedMesh::mutate` / `SharedMesh::setup`, which notify every registered
consumer (FE engines, DOF manager, model) in upstream's event order
(decision D-019); a plain `Mesh` can be modified directly before it is
shared.

### Consumers say first which changes they cannot take

Upstream sends a mesh event to its handlers after the change; a handler
that cannot follow it throws with the mesh already changed. In the port
every consumer implements `MeshEventHandler::check_mesh_event` (a required
method, decision D-068): before the change is committed, `mutate` asks
every registered consumer, in registration order, whether it accepts each
recorded event. A refusal returns the consumer's error, the mesh is rolled
back (same nodes, same elements, same revision) and nothing is
dispatched; the error is recoverable and the mesh stays usable. The models of
this release refuse what they cannot follow this way: the structural
model refuses every topology change (`Error::NotImplemented`, UD-342).

```rust
use akantu::core::enums::EventPriority;
use akantu::core::{ElementType, Error, GhostType, Result};
use akantu::mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};

/// A consumer that cannot take new nodes.
struct NoNewNodes;

impl MeshEventHandler for NoNewNodes {
    fn check_mesh_event(&self, _mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        match event {
            MeshEvent::NodesAdded { .. } => Err(Error::NotImplemented("new nodes")),
            _ => Ok(()),
        }
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority(50)
    }
    fn on_mesh_event(&mut self, _mesh: &Mesh, _event: &MeshEvent) -> Result<()> {
        unreachable!("refused in the check, never dispatched")
    }
}

let mut mesh = Mesh::new(1, "bar")?;
{
    let mut a = MeshAccessor::new(&mut mesh);
    for x in [0.0, 1.0] {
        a.push_node(&[x])?;
    }
    // write guard on the connectivity, released at the end of the block
    let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
    c.push_tuple(&[0, 1]);
}
let mesh = SharedMesh::new(mesh);
let token = mesh.register_consumer("no new nodes", EventPriority(50), MeshPart::Main)?;
let mut consumer = NoNewNodes;
let revision = mesh.revision()?;

let r = mesh.mutate(
    &mut [(&token, &mut consumer)],
    |tx| {
        let n = tx.push_node(MeshPart::Main, &[2.0])?;
        let event = MeshEvent::NodesAdded {
            nodes: vec![n],
            old_nodes: None,
            origin: "example".into(),
            extension: None,
        };
        tx.record(MeshPart::Main, event);
        Ok(())
    },
    None,
);
assert!(matches!(r, Err(Error::NotImplemented(_)))); // the consumer's error
assert_eq!(mesh.read()?.nb_nodes(), 2); // rolled back
assert_eq!(mesh.revision()?, revision); // nothing committed
# Ok::<(), akantu::core::Error>(())
```

Tests: `crates/akantu-mesh/src/mutation.rs`
(`capability_refusal_rolls_back_before_commit`: refusal, rollback, a
later accepted change) and, for the structural model,
`crates/akantu-structural/tests/structural_model_api.rs::fu2_01_capability_check_matches_the_dispatch`.

### Several changes in one mutation

Upstream handles each event on the mesh state of the moment it is sent;
the port dispatches every event of a mutation after the commit, on the
final mesh. An event names nodes and elements in the numbering of the
moment it was recorded, so a later event of the same mutation that
renumbers them would make a consumer read the wrong entity. Such
combinations are refused before the commit, like any other capability
refusal (recoverable, rolled back):

- node removals before node additions: an addition followed by a
  removal or renumbering of the nodes (`remove_nodes`, `purify`) in the
  same mutation is refused; with a DOF family supported by a group, or on
  a distributed or periodic mesh, a removal must be the only node event
  (on a periodic or distributed mesh the mesh itself refuses a removal
  followed by any other node event, a pure renumbering of the nodes
  included);
- all erasures of a block in one `erase_elements` call, before element
  additions: a second erasure of a `(type, ghost)` block, or an erasure
  after elements were added to the block, is refused when the mesh has
  consumers (erasures of different blocks, and additions after an erasure,
  are fine);
- an upstream sequence that adds entities and then removes or renumbers
  them (the IGFEM interface move) becomes separate mutations.

The variable bounds of a `petsc_tao` solver name equations; after a node
removal they must be set again (the next bounded solve is refused until
then). Details: `docs/ARCHITECTURE.md`, "Event rules", and decision D-095 in `docs/DECISIONS.md`.
