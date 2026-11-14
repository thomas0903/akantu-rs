// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): field names and sources from
// src/model/solid_mechanics/solid_mechanics_model_io.cc (displacement,
// blocked_dofs as a Bool nodal field, material internal fields such as
// damage) and src/io/dumper/dumper_padding_helper.hh (2D → 3D padding).

//! Dumpers hold descriptors; the model resolves live
//! data at dump time (displacement, blocked flags, a law internal), also
//! after the mesh grew — no shadow arrays.

use akantu_comm::Communicator;
use akantu_core::enums::EventPriority;
use akantu_core::{ElementKind, ElementType, Error, GhostType, Result};
use akantu_io::{
    DumpBuffer, Dumper, FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType,
    Padding, TextDumper,
};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu_model::InternalField;
use akantu_solver::DofManager;

const TRI: ElementType = ElementType::Triangle3;

struct ToyModel {
    dofs: DofManager,
    damage: InternalField<f64>,
}

impl FieldResolver for ToyModel {
    fn resolve(
        &self,
        mesh: &Mesh,
        f: &FieldDescriptor,
        ty: Option<ElementType>,
        g: GhostType,
    ) -> Result<DumpBuffer> {
        match (&f.source, ty) {
            (FieldSource::Dof { family, order: 0 }, None) => {
                let a = self.dofs.dofs(family)?;
                debug_assert_eq!(a.len(), mesh.nb_nodes());
                Ok(DumpBuffer::Real {
                    nb_component: a.nb_component(),
                    values: a.as_slice().to_vec(),
                })
            }
            (FieldSource::Blocked { family }, None) => {
                let b = self
                    .dofs
                    .family(family)?
                    .blocked
                    .as_ref()
                    .ok_or_else(|| Error::not_found("blocked dofs", family.clone()))?;
                Ok(DumpBuffer::Bool {
                    nb_component: b.nb_component(),
                    values: b.borrow().as_slice().to_vec(),
                })
            }
            (FieldSource::Internal { name, .. }, Some(t)) if name == "damage" => {
                let a = self.damage.get(t, g)?;
                Ok(DumpBuffer::Real {
                    nb_component: 1,
                    values: a.as_slice().to_vec(),
                })
            }
            _ => Err(Error::not_found("dump field", f.name.clone())),
        }
    }
}

struct Consumer<'a>(&'a mut ToyModel);
impl MeshEventHandler for Consumer<'_> {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority::DOF_MANAGER
    }
    fn on_mesh_event(&mut self, mesh: &Mesh, e: &MeshEvent) -> Result<()> {
        self.0.dofs.on_mesh_event(mesh, e)?;
        if let MeshEvent::ElementsAdded { .. } = e {
            let n = mesh.nb_element(TRI, GhostType::NotGhost);
            self.0.damage.resize(TRI, GhostType::NotGhost, n)?;
        }
        Ok(())
    }
}

fn field(
    name: &str,
    source: FieldSource,
    support: FieldSupport,
    value_type: FieldValueType,
) -> FieldDescriptor {
    FieldDescriptor {
        name: name.into(),
        source,
        support,
        value_type,
        group: None,
        padding: Padding::To3D,
        homogenize: false,
    }
}

#[test]
fn dump_reads_live_model_data_after_a_resize() {
    let mut m = Mesh::new(2, "square").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(TRI, GhostType::NotGhost)
            .push_tuple(&[0, 1, 2]);
    }
    let mesh = SharedMesh::new(m);
    let mut dofs = DofManager::new("dm", Communicator::serial());
    dofs.register_nodal_dofs(&mesh.read().unwrap(), "displacement", 2)
        .unwrap();
    dofs.register_blocked_dofs("displacement").unwrap();
    let mut damage = InternalField::new("damage", 1, 0.0).unwrap();
    damage.resize(TRI, GhostType::NotGhost, 1).unwrap();
    let mut model = ToyModel { dofs, damage };
    let token = mesh
        .register_consumer("model", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();

    let mut dumper = TextDumper::default();
    dumper
        .register_mesh(2, GhostType::NotGhost, ElementKind::Regular)
        .unwrap();
    let disp = FieldSource::Dof {
        family: "displacement".into(),
        order: 0,
    };
    dumper
        .register_field(field(
            "displacement",
            disp,
            FieldSupport::Nodal,
            FieldValueType::Real,
        ))
        .unwrap();
    dumper
        .register_field(field(
            "blocked_dofs",
            FieldSource::Blocked {
                family: "displacement".into(),
            },
            FieldSupport::Nodal,
            FieldValueType::Bool,
        ))
        .unwrap();
    dumper
        .register_field(field(
            "damage",
            FieldSource::Internal {
                law: None,
                name: "damage".into(),
            },
            FieldSupport::QuadraturePoint,
            FieldValueType::Real,
        ))
        .unwrap();

    model.dofs.dofs_mut("displacement").unwrap().as_mut_slice()[2] = 0.5;
    model
        .dofs
        .family_mut("displacement")
        .unwrap()
        .blocked
        .as_mut()
        .unwrap()
        .borrow_mut()
        .as_mut_slice()[0] = true;
    dumper
        .dump(&mesh.read().unwrap(), &model, None, Some(0))
        .unwrap();

    // Topology change: a node and an element are added.
    let mut consumer = Consumer(&mut model);
    let mut handlers: Vec<(&_, &mut dyn MeshEventHandler)> = vec![(&token, &mut consumer)];
    mesh.mutate(
        &mut handlers,
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[1.0, 1.0])?;
            let e = tx.push_element(MeshPart::Main, TRI, GhostType::NotGhost, &[1, n, 2])?;
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsAdded {
                    elements: vec![e],
                    constitutive_law: None,
                    origin: "test".into(),
                    extension: None,
                },
            );
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "test".into(),
                    extension: None,
                },
            );
            Ok(())
        },
        None,
    )
    .unwrap();
    drop(handlers);
    model
        .damage
        .get_mut(TRI, GhostType::NotGhost)
        .unwrap()
        .as_mut_slice()[1] = 0.25;
    dumper
        .dump(&mesh.read().unwrap(), &model, None, Some(1))
        .unwrap();

    let out = dumper.output;
    assert!(out.contains("step 0\ndisplacement [3]: 0 0 0 | 0.5 0 0 | 0 0 0\nblocked_dofs [3]: 1 0 0 0 0 0\ndamage [1]: 0\n"), "{out}");
    assert!(out.contains("step 1\ndisplacement [4]: 0 0 0 | 0.5 0 0 | 0 0 0 | 0 0 0\nblocked_dofs [4]: 1 0 0 0 0 0 0 0\ndamage [2]: 0 | 0.25\n"), "{out}");
}
