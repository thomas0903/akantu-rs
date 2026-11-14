// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_mesh_data.cc,
// registered twice by test/test_fe_engine/CMakeLists.txt
// (`register_typed_test(mesh_data string "5" "10")` and
// `register_typed_test(mesh_data UInt 5 10)`).
//! Ports of upstream `test_mesh_data_string` and `test_mesh_data_UInt`
//! (TEST-010, TEST-011): a fresh `MeshData`, the elemental dataset
//! `<TYPE>_data` of `_triangle_6` allocated with zero tuples
//! (`getElementalDataArrayAlloc<TYPE>`), two values pushed, read back
//! through the dataset, and the dataset's type code. Upstream checks the
//! values with `AKANTU_DEBUG_ASSERT` (compiled out in release builds, so
//! the upstream release test only prints); the port asserts them in every
//! build. The upstream wrong-type debug assertion of
//! `getElementalDataArrayAlloc` is checked as an error here.

use akantu_core::{ElementType, GhostType, UInt};
use akantu_mesh::data::{MeshData, MeshDataTypeCode};

const ELEMENT: ElementType = ElementType::Triangle6;
const G: GhostType = GhostType::NotGhost;

/// `test_mesh_data_string`: `TYPE = string`, values `"5"`, `"10"`.
#[test]
fn test_mesh_data_string() {
    let mut mesh_data = MeshData::default();
    let name = "string_data";
    let vec = mesh_data
        .strings_mut_or_insert(name)
        .unwrap()
        .alloc(0, 1, ELEMENT, G, String::new())
        .unwrap();
    assert_eq!(vec.len(), 0);
    let value = ["5".to_owned(), "10".to_owned()];
    vec.push_value(value[0].clone());
    vec.push_value(value[1].clone());

    let read = mesh_data.strings(name).unwrap().get(ELEMENT, G).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read.nb_component(), 1);
    for (i, v) in value.iter().enumerate() {
        assert_eq!(&read.as_slice()[i], v, "value {i}");
    }
    assert_eq!(mesh_data.type_code(name).unwrap(), MeshDataTypeCode::String);
    // Upstream debug assertion "getElementalDataArrayAlloc called with the
    // wrong type": a typed access with another type is an error.
    assert!(mesh_data.uints_mut_or_insert(name).is_err());
    assert!(mesh_data.uints(name).is_err());
}

/// `test_mesh_data_UInt`: `TYPE = UInt`, values `5`, `10`.
#[test]
fn test_mesh_data_uint() {
    let mut mesh_data = MeshData::default();
    let name = "UInt_data";
    let mut vec = mesh_data
        .uints_mut_or_insert(name)
        .unwrap()
        .alloc(0, 1, ELEMENT, G, 0)
        .unwrap();
    assert_eq!(vec.len(), 0);
    let value: [UInt; 2] = [5, 10];
    vec.push_value(value[0]);
    vec.push_value(value[1]);
    drop(vec);
    let read = mesh_data.uints(name).unwrap().get(ELEMENT, G).unwrap();
    assert_eq!(read.as_slice(), &value);
    assert_eq!(read.nb_component(), 1);
    drop(read);
    assert_eq!(mesh_data.type_code(name).unwrap(), MeshDataTypeCode::UInt);
    assert!(mesh_data.strings_mut_or_insert(name).is_err());
    assert!(mesh_data.reals(name).is_err());
    // A second `getElementalDataArrayAlloc` on an existing dataset returns
    // the same array (no reallocation, values kept).
    let again = mesh_data
        .uints_mut_or_insert(name)
        .unwrap()
        .get(ELEMENT, G)
        .unwrap();
    assert_eq!(again.as_slice(), &value);
}
