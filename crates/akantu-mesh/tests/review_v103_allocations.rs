// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs design (no upstream counterpart): D-071 (4).
// The validation of an `ElementsAdded` event (`MeshTransaction::record`, run
// for every element of every cohesive insertion pass) read each element's
// connectivity through `SharedElementTypeMapArray::element`, a `Vec` copy
// under the name of the borrowing `ElementTypeMapArray::element`: one heap
// allocation per element, result discarded. It now borrows (`element_ref`)
// and the copying accessor is called `element_to_vec`.

//! Heap allocations of `MeshTransaction::record(ElementsAdded)` for 1 and
//! for 2,000 new elements, unexported and exported (a native section of the
//! runtime the connectivity is exported to, the state of a Python-driven
//! insertion), counted per thread with a global allocator wrapper: the
//! counts must be equal (no allocation per element). On `main` before the
//! fix the large event allocated 1,999 more times.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use akantu_core::{Element, ElementType, ForeignRuntime, GhostType, Idx};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshPart, SharedMesh};

struct Counting;
thread_local! {
    static ON: Cell<bool> = const { Cell::new(false) };
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards every call unchanged to the system allocator; the only
// addition is a thread-local flag check and counter increment (the counting
// allocator of tests/mazars_allocations.rs in akantu-solid), with `realloc`
// counted as an allocation.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was returned by `System` with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

fn count() {
    if ON.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Allocations made by `f` on this thread.
fn counted<R>(f: impl FnOnce() -> R) -> (usize, R) {
    ALLOCS.with(|c| c.set(0));
    ON.with(|o| o.set(true));
    let r = f();
    ON.with(|o| o.set(false));
    (ALLOCS.with(Cell::get), r)
}

const SEG: ElementType = ElementType::Segment2;
const NG: GhostType = GhostType::NotGhost;
const N: usize = 2_000;

fn bar(n: usize) -> Mesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for i in 0..=n {
            a.push_node(&[f64::from(u32::try_from(i).unwrap())])
                .unwrap();
        }
        let mut c = a.connectivity_mut(SEG, NG);
        for i in 0..n {
            let i = Idx::try_from(i).unwrap();
            c.push_tuple(&[i, i + 1]);
        }
    }
    m
}

fn added(n: usize) -> MeshEvent {
    MeshEvent::ElementsAdded {
        elements: (0..n)
            .map(|i| Element::new(SEG, Idx::try_from(i).unwrap(), NG))
            .collect(),
        constitutive_law: None,
        origin: "review_v103".into(),
        extension: None,
    }
}

/// Allocations of recording (and so validating) an `ElementsAdded` event of
/// `n` existing elements, in a fresh transaction that is then rolled back
/// (the event's own vectors are built before counting).
fn record_allocations(mesh: &SharedMesh, n: usize) -> usize {
    let event = added(n);
    let mut allocs = None;
    let r = mesh.mutate(
        &mut [],
        |tx| {
            let (k, ()) = counted(|| tx.record(MeshPart::Main, event));
            allocs = Some(k);
            Err(akantu_core::Error::invalid("roll back"))
        },
        None,
    );
    assert!(r.is_err());
    allocs.unwrap()
}

#[test]
fn elements_added_validation_does_not_allocate_per_element() {
    let mesh = SharedMesh::new(bar(N));
    let one = record_allocations(&mesh, 1);
    let many = record_allocations(&mesh, N);
    assert_eq!(
        many, one,
        "recording {N} new elements allocated {many} times, one element {one} times"
    );
}

#[test]
fn elements_added_validation_does_not_allocate_per_element_when_exported() {
    let mesh = SharedMesh::new(bar(N));
    let rt = ForeignRuntime::new();
    let lease = mesh
        .with_exports(|x| x.connectivity(SEG, NG)?.export(&rt))
        .unwrap();
    let mut interp = rt.enter().unwrap();
    let _section = interp.native().unwrap();
    let one = record_allocations(&mesh, 1);
    let many = record_allocations(&mesh, N);
    assert_eq!(
        many, one,
        "exported: recording {N} new elements allocated {many} times, one element {one} times"
    );
    drop(lease);
}
