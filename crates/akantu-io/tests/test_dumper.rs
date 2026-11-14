// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_io/test_dumper/test_dumper.cc
// (+ test_dumper.msh, input_file.dat).

//! Port of upstream `test_dumper`: on the `Bottom` group of
//! `test_dumper.msh`, dump `displacement = (d+1)·|x|` and `blocked_dofs =
//! bool(n % 2 + d)` with the group's ParaView dumper, a ParaView dumper
//! with three simulated ranks (filtered mesh re-registered per rank) and a
//! CSV text dumper with precision 8, a time step and two variables. The
//! files are compared byte for byte with the output of the upstream test
//! binary (`fixtures/dump/test_dumper`, `oracle/probes/dump/run.sh`). The
//! mesh comes from the oracle's mesh fixture
//! (`fixtures/mesh_io/test_io/test_dumper`); the solid-mechanics model is
//! replaced by a resolver serving the two nodal arrays (the test only
//! dumps them).

mod dump_common;

use std::path::Path;

use akantu_core::{ElementKind, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_io::{
    DumpBuffer, Dumpable, Dumper, DumperParaview, DumperText, FieldDescriptor, FieldResolver,
    FieldSource, MeshFilter, MeshSelection, ParallelContext, TextDumpMode, solid_mechanics_field,
};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::npy::read_npy;
use dump_common::{assert_same_tree, fixtures, scratch};

const NG: GhostType = GhostType::NotGhost;

fn npy(name: &str) -> Vec<f64> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/mesh_io/test_io/test_dumper/test_dumper/data")
        .join(format!("{name}.npy"));
    read_npy(&p).unwrap().to_f64().unwrap()
}

#[allow(clippy::cast_possible_truncation)] // small exact integers from the fixture
fn ids(name: &str) -> Vec<Idx> {
    npy(name).into_iter().map(|x| x as Idx).collect()
}

fn test_dumper_mesh() -> Mesh {
    let mut m = Mesh::new(3, "test_dumper").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in npy("nodes").chunks(3) {
            a.push_node(x).unwrap();
        }
        for (ty, name, n) in [
            (ElementType::Quadrangle4, "connectivity___quadrangle_4", 4),
            (ElementType::Hexahedron8, "connectivity___hexahedron_8", 8),
        ] {
            let mut c = a.connectivity_mut(ty, NG);
            for t in ids(name).chunks(n) {
                c.push_tuple(t);
            }
        }
    }
    let bottom = ids("element_groups__Bottom___quadrangle_4");
    let nodes = ids("element_groups__Bottom__nodes");
    let mut g = m.groups_mut().create_element_group("Bottom", 2).unwrap();
    g.elements
        .alloc(bottom.len(), 1, ElementType::Quadrangle4, NG, 0)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&bottom);
    g.node_group.set_nodes(nodes);
    m
}

/// The model of the test: displacement, blocked flags, two variables.
struct Model {
    disp: Vec<Real>,
    bound: Vec<bool>,
    pot_energy: Real,
    gforces: [Real; 2],
}

impl Model {
    fn new(mesh: &Mesh) -> Self {
        let dim = 3;
        let coord = mesh.nodes();
        let coord = coord.as_slice();
        let n = mesh.nb_nodes();
        let mut disp = vec![0.0; n * dim];
        let mut bound = vec![false; n * dim];
        for i in 0..n {
            let mut dist = 0.;
            for d in 0..dim {
                dist += coord[i * dim + d] * coord[i * dim + d];
            }
            dist = dist.sqrt();
            for d in 0..dim {
                #[allow(clippy::cast_precision_loss)]
                let f = (d + 1) as Real;
                disp[i * dim + d] = f * dist;
                bound[i * dim + d] = (i % 2) + d != 0;
            }
        }
        Self {
            disp,
            bound,
            pot_energy: 1.234_556_789_1,
            gforces: [1.0, 1.0],
        }
    }
}

impl FieldResolver for Model {
    fn resolve(
        &self,
        _: &Mesh,
        f: &FieldDescriptor,
        ty: Option<ElementType>,
        _: GhostType,
    ) -> Result<DumpBuffer> {
        match (&f.source, ty) {
            (FieldSource::Dof { order: 0, .. }, None) => Ok(DumpBuffer::Real {
                nb_component: 3,
                values: self.disp.clone(),
            }),
            (FieldSource::Blocked { .. }, None) => Ok(DumpBuffer::Bool {
                nb_component: 3,
                values: self.bound.clone(),
            }),
            _ => Err(Error::not_found("field", f.name.clone())),
        }
    }

    fn variable(&self, name: &str) -> Result<DumpBuffer> {
        match name {
            "potential_energy" => Ok(DumpBuffer::Real {
                nb_component: 1,
                values: vec![self.pot_energy],
            }),
            "global_forces" => Ok(DumpBuffer::Real {
                nb_component: 2,
                values: self.gforces.to_vec(),
            }),
            _ => Err(Error::not_found("variable", name)),
        }
    }
}

fn group_field(name: &str) -> FieldDescriptor {
    solid_mechanics_field(name, Some("Bottom"), false, |_| false).unwrap()
}

#[test]
fn test_dumper() {
    let mesh = test_dumper_mesh();
    let mut model = Model::new(&mesh);
    let dir = scratch("test_dumper");
    let paraview = dir.join("paraview");
    let bottom = MeshSelection::new(None, NG, ElementKind::NotDefined)
        .filtered(MeshFilter::ElementGroup("Bottom".into()));

    // Group dumper of Bottom (ElementGroup: registerDumper<DumperParaview>
    // ("paraview_Bottom", "Bottom", true) + addDumpFilteredMesh).
    let mut group = Dumpable::new();
    group.register_paraview("paraview_Bottom", Some("Bottom"), true);
    group.add_dump_mesh(bottom.clone()).unwrap();
    // model.setGroupDirectory / setGroupBaseName / addDumpGroupField
    group.set_directory(&paraview).unwrap();
    group.set_base_name("paraview_bottom").unwrap();
    group.add_dump_field(group_field("displacement")).unwrap();
    group.add_dump_field(group_field("blocked_dofs")).unwrap();

    // ParaView dumper with three simulated ranks.
    let nbp = 3;
    let mut prv = DumperParaview::new(
        "paraview_bottom_parallel",
        paraview.to_str().unwrap(),
        false,
    );
    for p in 0..nbp {
        prv.set_parallel_context(ParallelContext::Explicit {
            rank: p,
            size: nbp,
            root: 0,
        });
        if p != 0 {
            prv.unregister_field("connectivities").unwrap();
            // never registered: upstream warns, akantu-rs reports NotFound
            assert!(prv.unregister_field("element_type").is_err());
            prv.unregister_field("positions").unwrap();
            prv.unregister_field("displacement").unwrap();
        }
        prv.register_mesh_selection(bottom.clone()).unwrap();
        prv.register_field(group_field("displacement")).unwrap();
        prv.dump(&mesh, &model, None, Some(0)).unwrap();
    }

    let mut txt = DumperText::new("text_bottom", TextDumpMode::Csv, true);
    txt.set_directory(&paraview).unwrap();
    txt.set_precision(8).unwrap();
    txt.set_time_step(0.1).unwrap();
    txt.register_mesh_selection(bottom.clone()).unwrap();
    txt.register_field(group_field("displacement")).unwrap();
    txt.register_field(group_field("blocked_dofs")).unwrap();
    txt.register_variable("potential_energy").unwrap();
    txt.register_variable("global_forces").unwrap();

    // dump a first time before the main loop
    group.dump(&mesh, &model, None, None).unwrap();
    txt.dump(&mesh, &model, None, None).unwrap();

    let time_step = 0.1;
    let mut time = 0.;
    for i in 1..5 {
        model.pot_energy += 2.;
        model.gforces[0] += 0.1;
        model.gforces[1] += 0.2;
        time += time_step;
        if i % 2 == 0 {
            txt.dump(&mesh, &model, Some(time), Some(i)).unwrap();
            group.dump(&mesh, &model, None, None).unwrap();
            for p in 0..nbp {
                prv.set_parallel_context(ParallelContext::Explicit {
                    rank: p,
                    size: nbp,
                    root: 0,
                });
                prv.dump(&mesh, &model, None, Some(i)).unwrap();
            }
        }
    }
    assert_same_tree(&paraview, &fixtures().join("test_dumper/paraview"));
}
