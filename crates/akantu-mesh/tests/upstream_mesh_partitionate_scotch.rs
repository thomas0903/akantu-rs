// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_mesh_utils/test_mesh_partitionate/
// test_mesh_partitionate_scotch.cc (+ triangle.geo, frozen triangle.msh).

//! Upstream `test_mesh_partitionate_scotch` in full (feature `scotch`): at
//! debug level `dblDump`, `MeshPartitionScotch(mesh, 2).partitionate(8)`
//! (writing `GraphIniFile.grf`, `GeomIniFile.xyz`, `GraphFile.grf`,
//! `MapFile.map`), a ParaView dump `test-scotch-partition` of the partition
//! field, `reorder()` (writing `ScotchMesh.msh`, `ScotchMesh.xyz`,
//! `ScotchMesh.grf`) and `mesh.write("triangle_reorder.msh")`. Upstream
//! asserts nothing; the port checks the invariants of every step (eight
//! non-empty parts covering every element of the mesh dimension, map file =
//! partition, a node
//! permutation applied consistently, the written mesh reads back to the
//! reordered mesh). Scotch's numbers themselves are compared with the
//! oracle in `mesh2_scotch_oracle.rs` and `parallel_oracle.rs`.
#![cfg(feature = "scotch")]

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementKind, ElementType, Error, GhostType, Idx, Result, TypeFilter};
use akantu_io::{
    DumpBuffer, Dumper, DumperParaview, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, Padding, ReadOptions, read_mesh_with, write_mesh,
};
use akantu_mesh::Mesh;
use akantu_mesh::partition::{MeshPartition, MeshPartitioner};
use akantu_mesh::scotch::ScotchPartitioner;
use akantu_mesh::scotch_reorder::{ReorderMode, reorder};

const NG: GhostType = GhostType::NotGhost;

fn read(path: &std::path::Path) -> Mesh {
    let mut m = Mesh::new(2, "mesh").unwrap();
    let opts = ReadOptions {
        fill_unnamed_physical_groups: false,
        write_diana_material_file: false,
    };
    read_mesh_with(&mut m, path, MeshIoType::Auto, &opts).unwrap();
    m
}

/// Serves the partition of every triangle as the elemental field
/// `partitions` (upstream `ElementalField<Idx>(partition.getPartitions())`).
struct Parts<'a>(&'a MeshPartition);

impl FieldResolver for Parts<'_> {
    fn resolve(
        &self,
        _: &Mesh,
        f: &FieldDescriptor,
        ty: Option<ElementType>,
        _: GhostType,
    ) -> Result<DumpBuffer> {
        match ty {
            Some(t) => Ok(DumpBuffer::Int {
                nb_component: 1,
                values: self.0.partition(t)?.to_vec(),
            }),
            None => Err(Error::not_found("nodal field", f.name.clone())),
        }
    }
}

#[test]
fn test_mesh_partitionate_scotch() {
    let src = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_partitionate/triangle.msh");
    let mut mesh = read(&src);
    let dir = std::env::temp_dir().join(format!("akantu-scotch-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // partitionate(8) at dblDump
    let p = ScotchPartitioner::new(1234)
        .with_loads(&|_, _| 1, &|_| 1)
        .with_debug_dump(&dir)
        .partition(&mesh, 8)
        .unwrap();
    let lin = p.linearized();
    let n = mesh.nb_element_filtered(TypeFilter::new().dim(2));
    assert_eq!(lin.len(), n);
    for part in 0..8 {
        assert!(lin.contains(&part), "part {part} is empty");
    }
    assert!(lin.iter().all(|&x| (0..8).contains(&x)));
    let map = std::fs::read_to_string(dir.join("MapFile.map")).unwrap();
    let mut lines = map.lines();
    assert_eq!(lines.next(), Some(n.to_string().as_str()));
    for (i, l) in lines.enumerate() {
        assert_eq!(l, format!("{i}    {}", lin[i]));
    }
    for f in ["GraphIniFile.grf", "GraphFile.grf", "GeomIniFile.xyz"] {
        assert!(dir.join(f).metadata().unwrap().len() > 0, "{f}");
    }

    // DumperParaview "test-scotch-partition": registerMesh(mesh, dim) and
    // the partition field
    let mut dumper = DumperParaview::with_name("test-scotch-partition");
    dumper.set_directory(&dir.join("paraview")).unwrap();
    dumper.register_mesh(2, NG, ElementKind::Regular).unwrap();
    dumper
        .register_field(FieldDescriptor {
            name: "partitions".into(),
            source: FieldSource::ModelElemental("partitions".into()),
            support: FieldSupport::Elemental,
            value_type: FieldValueType::Int,
            group: None,
            padding: Padding::None,
            homogenize: false,
        })
        .unwrap();
    dumper.dump(&mesh, &Parts(&p), None, None).unwrap();
    let vtu = std::fs::read_to_string(
        dir.join("paraview/test-scotch-partition-VTU/test-scotch-partition_0000.vtu"),
    )
    .unwrap();
    assert!(vtu.contains("Name=\"partitions\""));

    // reorder() at dblDump (upstream renumbering), then mesh.write
    let before = read(&src);
    let out = reorder(&mut mesh, 1234, ReorderMode::Upstream, Some(&dir)).unwrap();
    let perm = &out.permutation;
    let mut seen = vec![false; perm.len()];
    for &q in perm {
        seen[usize::try_from(q).unwrap()] = true;
    }
    assert!(seen.iter().all(|&s| s), "a permutation of the nodes");
    let dim = 2;
    for (i, x) in before.nodes().tuples().enumerate() {
        let j = usize::try_from(perm[i]).unwrap();
        assert_eq!(&mesh.nodes().as_slice()[j * dim..(j + 1) * dim], x);
    }
    for (ty, g, c) in before.connectivities().as_map().iter_all() {
        let new: Vec<Idx> = c
            .read()
            .unwrap()
            .as_slice()
            .iter()
            .map(|&k| perm[usize::try_from(k).unwrap()])
            .collect();
        assert_eq!(mesh.connectivity(ty, g).unwrap().as_slice(), new.as_slice());
    }
    for f in ["ScotchMesh.msh", "ScotchMesh.grf", "ScotchMesh.xyz"] {
        assert!(dir.join(f).metadata().unwrap().len() > 0, "{f}");
    }
    let written = dir.join("triangle_reorder.msh");
    write_mesh(&mesh, &written, MeshIoType::Auto).unwrap();
    let back = read(&written);
    assert_eq!(back.nodes().as_slice(), mesh.nodes().as_slice());
    for ty in mesh.element_types(TypeFilter::new()) {
        assert_eq!(
            back.connectivity(ty, NG).unwrap().as_slice(),
            mesh.connectivity(ty, NG).unwrap().as_slice()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
