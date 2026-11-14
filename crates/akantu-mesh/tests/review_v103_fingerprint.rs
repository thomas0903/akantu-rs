// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs design (no upstream counterpart): regression tests (D-071 (3))
// for `Mesh::state_fingerprint`, the "complete state"
// oracle of the rollback tests. The first three tests failed on the
// version before the fix, where
// group lists and numeric mesh data (exportable `SharedArray`s, whose `Debug`
// is metadata-only) were formatted through `Debug`, so a change of their
// values that kept the lengths left the fingerprint unchanged.

use akantu_core::{Array, Element, ElementType, ForeignRuntime, GhostType, Idx};
use akantu_mesh::{Mesh, MeshAccessor};

const SEG: ElementType = ElementType::Segment2;
const NG: GhostType = GhostType::NotGhost;

fn bar() -> Mesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for i in 0..5 {
            a.push_node(&[f64::from(i)]).unwrap();
        }
        let mut c = a.connectivity_mut(SEG, NG);
        for i in 0..4 {
            c.push_tuple(&[i as Idx, (i + 1) as Idx]);
        }
    }
    let g = m.groups_mut();
    g.create_node_group("ng").unwrap().set_nodes(vec![0, 1, 2]);
    let mut eg = g.create_element_group("eg", 1).unwrap();
    eg.add_element(Element::new(SEG, 0, NG)).unwrap();
    eg.add_element(Element::new(SEG, 1, NG)).unwrap();
    m.data_mut()
        .reals_mut_or_insert("weights")
        .unwrap()
        .as_map_mut()
        .replace(
            SEG,
            NG,
            Array::from_vec(vec![1.0, 2.0, 3.0, 4.0], 1).unwrap().into(),
        );
    m
}

#[test]
fn node_group_values_change_the_fingerprint() {
    let mut m = bar();
    let before = m.state_fingerprint();
    m.groups_mut()
        .node_group_mut("ng")
        .unwrap()
        .set_nodes(vec![0, 1, 4]); // same length
    assert_ne!(
        m.state_fingerprint(),
        before,
        "node group values are not in the fingerprint"
    );
}

#[test]
fn element_group_values_change_the_fingerprint() {
    let mut m = bar();
    let before = m.state_fingerprint();
    {
        let mut eg = m.groups_mut().element_group_mut("eg").unwrap();
        eg.elements.get_mut(SEG, NG).unwrap().as_mut_slice()[1] = 3; // same length
    }
    assert_ne!(
        m.state_fingerprint(),
        before,
        "element group values are not in the fingerprint"
    );
}

#[test]
fn mesh_data_values_change_the_fingerprint() {
    let mut m = bar();
    let before = m.state_fingerprint();
    m.data_mut()
        .reals_mut("weights")
        .unwrap()
        .get_mut(SEG, NG)
        .unwrap()
        .as_mut_slice()[2] = -3.0; // same length
    assert_ne!(
        m.state_fingerprint(),
        before,
        "mesh data values are not in the fingerprint"
    );
}

/// D-071 (3), second half: export metadata is not state. A real shrink of
/// an exported block retires it (generation + 1, fresh allocation); growing
/// back to the same values must give the same fingerprint (in export-all
/// mode, five rollback tests failed on `generation` alone before the fix).
#[test]
fn export_metadata_is_not_in_the_fingerprint() {
    let mut m = bar();
    let before = m.state_fingerprint();
    let rt = ForeignRuntime::new();
    let lease = m
        .data_mut()
        .reals_mut("weights")
        .unwrap()
        .shared_mut(SEG, NG)
        .unwrap()
        .export(&rt)
        .unwrap();
    let mut interp = rt.enter().unwrap();
    {
        let _section = interp.native().unwrap();
        {
            let data = m.data_mut().reals_mut("weights").unwrap();
            let mut w = data.get_mut(SEG, NG).unwrap();
            w.truncate(3);
            w.push_value(4.0);
        }
        let block = m.data().reals("weights").unwrap().shared(SEG, NG).unwrap();
        assert!(
            block.generation() > 0,
            "the shrink retired the exported block"
        );
        assert_eq!(m.state_fingerprint(), before);
    }
    assert!(
        !m.data()
            .reals("weights")
            .unwrap()
            .shared(SEG, NG)
            .unwrap()
            .is_attached(&lease)
    );
}
