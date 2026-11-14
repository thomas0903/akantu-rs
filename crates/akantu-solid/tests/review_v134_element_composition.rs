// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS).
// Element events composed in one mutation
// (upstream mesh.cc eraseElements; upstream IGFEM's buildIGFEMMesh sends
// NewElements, then RemovedElements of the old elements,
// mesh_igfem_spherical_growing_gel_tmpl.hh:380-391; upstream
// ConstitutiveLaw::onElementsRemoved, constitutive_law_tmpl.hh:344-420,
// resizes the handler maps of the renumbered types only). The two `poison`
// tests assert the corrected behaviour.

//! Consumers receive every event of a mutation with the **final** mesh
//! (`docs/ARCHITECTURE.md`). Two `ElementsRemoved` of one block, or an
//! `ElementsAdded` followed by an `ElementsRemoved` that moves the added
//! elements into the holes, cannot be followed on the final mesh (before
//! the fix they failed at the solid model's dispatch and poisoned the
//! domain): the mesh domain refuses them in the capability stage, before
//! the commit — recoverable, the revision and the model unchanged, the
//! domain usable afterwards. One call with both cells, an erasure followed
//! by an addition, and erasures of different blocks commit; an erasure
//! leaves the constitutive-law maps of the other blocks untouched.

mod node_removal_common;
mod par_smm;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshPart, MeshTransaction, SharedMesh};
use akantu_model::ModelOptions;
use akantu_solid::SolidMechanicsModel;
use node_removal_common::{model_on, scrambled_square};

const NX: usize = 6;

fn cell(i: usize, j: usize) -> Vec<Element> {
    let e = 2 * (j * NX + i);
    (e..e + 2)
        .map(|k| Element::new(ElementType::Triangle3, k as Idx, GhostType::NotGhost))
        .collect()
}

fn explicit() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)
}

/// The model's element → law maps agree with its laws' filters on every
/// block of the mesh's dimension: each element has a law, its law-local
/// number points back to it, and each filter entry is mapped to its law.
fn check_law_maps(m: &SolidMechanicsModel, mesh: &Mesh) {
    let laws = &m.physics.materials;
    let index = laws.law_index_map();
    let local = laws.local_numbering_map();
    for ghost in GhostType::ALL {
        for ty in [ElementType::Triangle3, ElementType::Quadrangle4] {
            let n = mesh.nb_element(ty, ghost);
            if n == 0 {
                continue;
            }
            let idx = index.get(ty, ghost).unwrap().as_slice().to_vec();
            let loc = local.get(ty, ghost).unwrap().as_slice().to_vec();
            assert_eq!(idx.len(), n, "{ty:?} {ghost:?}: law index map size");
            assert_eq!(loc.len(), n, "{ty:?} {ghost:?}: local numbering size");
            for e in 0..n {
                let law = usize::try_from(idx[e]).unwrap_or_else(|_| {
                    panic!("{ty:?} {ghost:?} element {e}: no law ({})", idx[e])
                });
                let filter = laws
                    .law(law)
                    .unwrap()
                    .element_filter()
                    .get(ty, ghost)
                    .unwrap()
                    .as_slice()
                    .to_vec();
                let k = usize::try_from(loc[e]).unwrap();
                assert_eq!(filter[k], e as Idx, "{ty:?} {ghost:?} element {e}");
            }
            for l in 0..laws.nb_laws() {
                let f = laws.law(l).unwrap().element_filter();
                if let Ok(f) = f.get(ty, ghost) {
                    for (k, &e) in f.as_slice().iter().enumerate() {
                        let e = usize::try_from(e).unwrap();
                        assert_eq!(idx[e], l as Idx, "{ty:?} {ghost:?} filter entry {k}");
                        assert_eq!(loc[e], k as Idx, "{ty:?} {ghost:?} filter entry {k}");
                    }
                }
            }
        }
    }
}

fn run(
    change: impl FnOnce(&mut MeshTransaction<'_>) -> akantu_core::Result<()>,
) -> akantu_core::Result<()> {
    let shared = SharedMesh::new(scrambled_square(NX, 4, false));
    let mut a = model_on(&shared).unwrap();
    a.init_full(&explicit()).unwrap();
    a.mutate_mesh(change, None).map(|_| ())
}

/// The old cell (2, 1) replaced by two new triangles with the same nodes.
fn replace_cell(tx: &mut MeshTransaction<'_>, erase_first: bool) -> akantu_core::Result<()> {
    let old = cell(2, 1);
    let conn: Vec<Vec<Idx>> = {
        let c = tx
            .mesh()
            .connectivity(ElementType::Triangle3, GhostType::NotGhost)?;
        old.iter().map(|e| c.tuple(e.idx()).to_vec()).collect()
    };
    if erase_first {
        tx.erase_elements(MeshPart::Main, &old)?;
    }
    let mut added = Vec::new();
    for t in &conn {
        added.push(tx.push_element(
            MeshPart::Main,
            ElementType::Triangle3,
            GhostType::NotGhost,
            t,
        )?);
    }
    tx.record(
        MeshPart::Main,
        MeshEvent::ElementsAdded {
            elements: added,
            constitutive_law: None,
            origin: "v134".into(),
            extension: None,
        },
    );
    if !erase_first {
        tx.erase_elements(MeshPart::Main, &old)?;
    }
    Ok(())
}

/// A composed mutation through the live model is refused before the
/// commit: recoverable (`Unsupported`), the revision, the
/// mesh and the model's state unchanged; the domain stays usable (one
/// erasure call with both cells then commits and the model runs on).
fn refused_before_commit(change: impl FnOnce(&mut MeshTransaction<'_>) -> akantu_core::Result<()>) {
    let shared = SharedMesh::new(scrambled_square(NX, 4, false));
    let mut a = model_on(&shared).unwrap();
    a.init_full(&explicit()).unwrap();
    let dt = a.stable_time_step().unwrap() * 0.5;
    a.set_time_step(dt, None).unwrap();
    a.solve_step(None).unwrap();
    let revision = shared.revision().unwrap();
    let nb_el = shared
        .read()
        .unwrap()
        .nb_element(ElementType::Triangle3, GhostType::NotGhost);
    let u = a.displacement().unwrap().as_slice().to_vec();
    let r = a.mutate_mesh(change, None);
    let e = r.expect_err("refused before the commit");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
    assert!(format!("{e}").contains("of the same mutation names"), "{e}");
    assert_eq!(shared.revision().unwrap(), revision);
    assert_eq!(
        shared
            .read()
            .unwrap()
            .nb_element(ElementType::Triangle3, GhostType::NotGhost),
        nb_el
    );
    assert_eq!(a.displacement().unwrap().as_slice(), &u[..]);
    check_law_maps(&a, &shared.read().unwrap());
    // the domain stays usable
    let mut both = cell(5, 0);
    both.extend(cell(2, 1));
    a.mutate_mesh(|tx| tx.erase_elements(MeshPart::Main, &both), None)
        .unwrap();
    check_law_maps(&a, &shared.read().unwrap());
    a.solve_step(None).unwrap();
    assert!(
        a.displacement()
            .unwrap()
            .as_slice()
            .iter()
            .all(|x| x.is_finite())
    );
}

#[test]
fn v134_one_erasure_call_with_two_cells_commits() {
    let mut els = cell(5, 0);
    els.extend(cell(2, 1));
    run(|tx| tx.erase_elements(MeshPart::Main, &els)).unwrap();
}

/// Flipped after the fix (was: fatal at the solid model's
/// dispatch, `OutOfRange { "array tuple", 44, 44 }`).
#[test]
fn v134_two_erasure_calls_in_one_mutation_are_refused_before_commit() {
    refused_before_commit(|tx| {
        tx.erase_elements(MeshPart::Main, &cell(5, 0))?;
        tx.erase_elements(MeshPart::Main, &cell(2, 1))
    });
}

/// Flipped after the fix (was: fatal at the FE engines'
/// dispatch, `OutOfRange { "added element", 48, 48 }`).
#[test]
fn v134_elements_added_then_erased_are_refused_before_commit() {
    refused_before_commit(|tx| replace_cell(tx, false));
}

/// Two elements added, an erasure of two older ones that moves them into
/// the holes, two more added (copies of cell (0, 0)): every index of both
/// additions is valid in the final mesh, so before the fix nothing failed
/// at dispatch, and the FE engines kept the shapes of the second pair in
/// the rows of the first (silently wrong); refused like (b).
#[test]
fn v134_added_erased_added_is_refused_before_commit() {
    refused_before_commit(|tx| {
        replace_cell(tx, false)?;
        let conn: Vec<Vec<Idx>> = {
            let c = tx
                .mesh()
                .connectivity(ElementType::Triangle3, GhostType::NotGhost)?;
            (0..2).map(|e| c.tuple(e).to_vec()).collect()
        };
        let mut added = Vec::new();
        for t in &conn {
            added.push(tx.push_element(
                MeshPart::Main,
                ElementType::Triangle3,
                GhostType::NotGhost,
                t,
            )?);
        }
        tx.record(
            MeshPart::Main,
            MeshEvent::ElementsAdded {
                elements: added,
                constitutive_law: None,
                origin: "v134".into(),
                extension: None,
            },
        );
        Ok(())
    });
}

#[test]
fn v134_elements_erased_then_added_commit() {
    run(|tx| replace_cell(tx, true)).unwrap();
}

/// Square `[0, 2] × [0, 1]`: two triangles on the left, one quadrangle on
/// the right.
fn mixed_mesh() -> Mesh {
    let mut mesh = Mesh::new(2, "mixed").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for p in [
            [0.0, 0.0],
            [1.0, 0.0],
            [1.0, 1.0],
            [0.0, 1.0],
            [2.0, 0.0],
            [2.0, 1.0],
        ] {
            a.push_node(&p as &[Real]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[0, 2, 3]);
        drop(c);
        let mut q = a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
        q.push_tuple(&[1, 4, 5, 2]);
        q.push_tuple(&[4, 5, 2, 1]);
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

/// Port defect, outside upstream's
/// behaviour): an erasure in one block used to reset the law maps of
/// **every** block of the mesh's dimension and rewrite only the renumbered
/// ones, so the quadrangles lost their law (index `-1`) when a triangle was
/// erased. Upstream resizes the maps of the renumbered types only. Also:
/// erasures of two different blocks in two calls of one mutation compose.
#[test]
fn v134_erasure_keeps_the_law_maps_of_the_other_blocks() {
    let shared = SharedMesh::new(mixed_mesh());
    let mut m = model_on(&shared).unwrap();
    m.init_full(&explicit()).unwrap();
    check_law_maps(&m, &shared.read().unwrap());
    m.mutate_mesh(
        |tx| {
            tx.erase_elements(
                MeshPart::Main,
                &[Element::new(ElementType::Triangle3, 0, GhostType::NotGhost)],
            )
        },
        None,
    )
    .unwrap();
    check_law_maps(&m, &shared.read().unwrap());
    m.mutate_mesh(
        |tx| {
            tx.erase_elements(
                MeshPart::Main,
                &[Element::new(
                    ElementType::Quadrangle4,
                    1,
                    GhostType::NotGhost,
                )],
            )?;
            tx.erase_elements(
                MeshPart::Main,
                &[Element::new(ElementType::Triangle3, 0, GhostType::NotGhost)],
            )
        },
        None,
    )
    .unwrap();
    let g = shared.read().unwrap();
    assert_eq!(g.nb_element(ElementType::Triangle3, GhostType::NotGhost), 0);
    assert_eq!(
        g.nb_element(ElementType::Quadrangle4, GhostType::NotGhost),
        1
    );
    check_law_maps(&m, &g);
}
