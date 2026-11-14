// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ported behaviour:
// src/fe_engine/fe_engine_template_tmpl.hh (`onElementsAdded`), dispatched
// as upstream `Mesh::sendEvent` does (here through the v4 staged mutation of
// `SharedMesh`, docs/ARCHITECTURE.md).

//! The FE engine as a consumer of the v4 mesh domain: registered at
//! upstream priority `FE_ENGINE`, it is updated by the phased dispatch of a
//! committed mutation (it does not communicate, so it keeps the default
//! `on_mesh_event_phased`). Its incrementally updated caches equal a
//! from-scratch precomputation on the final mesh bit for bit, its recorded
//! revision follows the domain, and an elemental interpolation built before
//! the mutation is refused afterwards.

use akantu_core::enums::EventPriority;
use akantu_core::{ElementKind, ElementType, ElementTypeMapArray, Error, GhostType};
use akantu_fe::FeEngine;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};

const S2: ElementType = ElementType::Segment2;
const G: GhostType = GhostType::NotGhost;

fn fe(mesh: &Mesh) -> FeEngine {
    FeEngine::with_precomputation(
        mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap()
}

#[test]
fn fe_engine_follows_committed_mutations() {
    let mut mesh = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for x in [0.0, 1.0, 2.0] {
            a.push_node(&[x]).unwrap();
        }
        let mut c = a.connectivity_mut(S2, G);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[1, 2]);
    }
    let mut engine = fe(&mesh);
    let mut points = ElementTypeMapArray::new("points");
    points.alloc(2, 1, S2, G, 0.5).unwrap();
    let interp = engine
        .init_elemental_field_interpolation(&mesh, &points, None)
        .unwrap();

    let shared = SharedMesh::new(mesh);
    let token = shared
        .register_consumer("fe", EventPriority::FE_ENGINE, MeshPart::Main)
        .unwrap();
    let committed = shared
        .mutate(
            &mut [(&token, &mut engine as &mut dyn MeshEventHandler)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[3.5])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                let e = tx.push_element(MeshPart::Main, S2, G, &[2, n])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsAdded {
                        elements: vec![e],
                        constitutive_law: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    shared.check_current(&token).unwrap();

    let mesh = shared.read().unwrap();
    assert_eq!(committed.revision, shared.revision().unwrap());
    assert_eq!(engine.mesh_revision(), mesh.revision());
    assert_eq!(mesh.nb_element(S2, G), 3);
    let reference = fe(&mesh);
    for (got, want) in [
        (engine.jacobians(S2, G), reference.jacobians(S2, G)),
        (engine.shapes(S2, G), reference.shapes(S2, G)),
        (
            engine.shapes_derivatives(S2, G),
            reference.shapes_derivatives(S2, G),
        ),
    ] {
        let (got, want) = (got.unwrap(), want.unwrap());
        assert_eq!(got.nb_component(), want.nb_component());
        assert_eq!(
            got.as_slice(),
            want.as_slice(),
            "incremental != from scratch"
        );
    }

    // The interpolation built before the mutation is stale.
    let mut field = ElementTypeMapArray::new("field");
    field.alloc(2, 1, S2, G, 1.0).unwrap();
    let mut out = ElementTypeMapArray::new("out");
    let mut filter = ElementTypeMapArray::new("filter");
    let f = filter.alloc(0, 1, S2, G, 0).unwrap();
    f.push_value(0);
    f.push_value(1);
    let r = engine.interpolate_elemental_field(&mesh, &interp, &field, &mut out, G, Some(&filter));
    assert!(
        matches!(&r, Err(Error::InvalidArgument(m)) if m.contains("revision")),
        "{r:?}"
    );
}
