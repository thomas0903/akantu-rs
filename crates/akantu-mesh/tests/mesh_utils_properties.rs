// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent checks
// of the mesh utilities — not comparisons with the oracle:
// topological invariants of the facet meshes (facet sharing, two-way
// adjacency, Euler characteristics), CSR consistency, renumbering
// invariants after element/node removal, periodic pairing symmetry, and the
// journaled transaction paths (events, rollback) of the removal and facet
// operations. Ported behaviour: src/mesh_utils/mesh_utils.cc,
// src/mesh/mesh.cc (eraseElements, initMeshFacets), src/mesh/mesh_periodic.cc.

use std::collections::{BTreeMap, BTreeSet};

use akantu_core::enums::{EventPriority, MeshIoType, SpatialDirection};
use akantu_core::{Element, ElementKind, ElementType, GhostType, Idx, Result, TypeFilter};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::periodic::{make_periodic, periodic_pairs};
use akantu_mesh::removal::erase_elements_setup;
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT, build_node_to_elements, init_mesh_facets,
    purify_mesh,
};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu_validation::{Fixture, fixtures_root};

const G: GhostType = GhostType::NotGhost;

/// Every input mesh of the `mesh_utils` fixtures: `(case, mesh)`.
fn fixture_meshes() -> Vec<(String, Mesh)> {
    let mut cases = Vec::new();
    let mut stack = vec![fixtures_root().join("mesh_utils")];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.join("manifest.json").exists() {
                cases.push(p);
            } else if p.is_dir() {
                stack.push(p);
            }
        }
    }
    cases.sort();
    cases
        .into_iter()
        .map(|dir| {
            let fx = Fixture::open(&dir).unwrap();
            let p = &fx.manifest().parameters;
            let dim = usize::try_from(p["spatial_dimension"].as_i64().unwrap()).unwrap();
            let mut m = Mesh::new(dim, "m").unwrap();
            if let Ok(path) = fx.input_path("mesh") {
                let opts = ReadOptions {
                    fill_unnamed_physical_groups: false,
                    write_diana_material_file: false,
                };
                read_mesh_with(&mut m, &path, MeshIoType::Auto, &opts).unwrap();
            } else {
                let nodes = fx.load_f64("input/nodes").unwrap();
                {
                    let mut a = MeshAccessor::new(&mut m);
                    for x in nodes.data.chunks_exact(dim) {
                        a.push_node(x).unwrap();
                    }
                    for b in p["input/blocks"].as_array().unwrap() {
                        let ty: ElementType = b.as_str().unwrap().parse().unwrap();
                        let c = fx.load_i64(&format!("input/connectivity/{ty}")).unwrap();
                        let mut conn = a.connectivity_mut(ty, G);
                        for v in c.data.chunks_exact(ty.nb_nodes_per_element()) {
                            let v: Vec<Idx> =
                                v.iter().map(|&n| Idx::try_from(n).unwrap()).collect();
                            conn.push_tuple(&v);
                        }
                    }
                }
                m.compute_bounding_box().unwrap();
            }
            (fx.case_id().to_owned(), m)
        })
        .collect()
}

fn sorted(v: &[Idx]) -> Vec<Idx> {
    let mut s = v.to_vec();
    s.sort_unstable();
    s
}

/// Facets: every facet of dimension − 1 lies on one or two parent elements
/// (cohesive elements count as one side each), boundary facets end with
/// `ElementNull`, the adjacency holds both ways, facet nodes belong to
/// every connected element, no facet is duplicated, and every regular
/// parent element found all its facets.
#[test]
fn facets_are_shared_by_at_most_two_elements_and_adjacency_is_symmetric() {
    let mut checked = 0;
    for (case, mut m) in fixture_meshes() {
        let dim = m.spatial_dimension();
        let _ = init_mesh_facets(&mut m).unwrap();
        let f = m.mesh_facets().unwrap();
        let e2s = f.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
        let s2e = f.data().element_map(SUBELEMENT_TO_ELEMENT).unwrap();
        for level in (0..dim).rev() {
            let parent_mesh: &Mesh = if level + 1 == dim { &m } else { f };
            let mut seen: BTreeSet<(ElementType, Vec<Idx>)> = BTreeSet::new();
            for ty in f.element_types(TypeFilter::new().dim(level)) {
                let conn = f.connectivity(ty, G).unwrap();
                let rows = e2s.get(ty, G).unwrap();
                assert_eq!(rows.len(), conn.len(), "{case} {ty}");
                for (i, (nodes, list)) in conn.tuples().zip(rows.as_slice()).enumerate() {
                    let facet = Element::new(ty, Idx::try_from(i).unwrap(), G);
                    assert!(
                        seen.insert((ty, sorted(nodes))),
                        "{case}: duplicate facet {facet}"
                    );
                    let non_null: Vec<Element> =
                        list.iter().copied().filter(|e| !e.is_null()).collect();
                    assert!(!non_null.is_empty(), "{case}: {facet} has no element");
                    if level + 1 == dim {
                        assert!(
                            non_null.len() <= 2,
                            "{case}: {facet} shared by {non_null:?}"
                        );
                        if non_null.len() == 1 {
                            assert_eq!(list.len(), 2, "{case}: boundary {facet} lacks ElementNull");
                            assert!(list[1].is_null());
                        }
                    }
                    for el in non_null {
                        let en = parent_mesh.element_connectivity(el).unwrap();
                        assert!(
                            nodes.iter().all(|n| en.contains(n)),
                            "{case}: {facet} ⊄ {el}"
                        );
                        assert!(
                            s2e.element(el).unwrap().contains(&facet),
                            "{case}: {el} misses {facet}"
                        );
                    }
                }
            }
            // every regular parent element has all its facets
            for ty in parent_mesh
                .element_types(TypeFilter::new().dim(level + 1).kind(ElementKind::Regular))
            {
                if ty == ElementType::Point1 {
                    continue;
                }
                for row in s2e.get(ty, G).unwrap().tuples() {
                    assert!(
                        row.iter().all(|e| !e.is_null()),
                        "{case}: {ty} lacks a facet"
                    );
                    for &facet in row {
                        assert!(e2s.element(facet).unwrap()[0].iter().any(|x| x.ty() == ty));
                    }
                }
            }
        }
        checked += 1;
    }
    assert!(checked >= 53);
}

/// Euler characteristics of the facet meshes of the (contractible) test
/// domains: `V − E (+ F − C) = 1` over the whole mesh, and every closed
/// boundary has `V − E = 0` (2-D) or `V − E + F = 2` per spherical component
/// (3-D). Only regular meshes without hanging lower-dimensional elements.
#[test]
fn euler_characteristics_of_facet_meshes() {
    let mut checked = 0;
    for (case, mut m) in fixture_meshes() {
        let dim = m.spatial_dimension();
        let mixed_dims = (0..dim).any(|d| m.nb_element_filtered(TypeFilter::new().dim(d)) > 0);
        let regular_only = m
            .element_types(TypeFilter::new())
            .all(|t| t.kind() == ElementKind::Regular);
        if !regular_only || case.contains("generated") || case.contains("segment_nodetype") {
            continue;
        }
        let _ = init_mesh_facets(&mut m).unwrap();
        let f = m.mesh_facets().unwrap();
        let count =
            |mesh: &Mesh, d: usize| mesh.nb_element_filtered(TypeFilter::new().dim(d)) as i64;
        let mut chi = 0i64;
        for d in 0..dim {
            chi += if d % 2 == 0 {
                count(f, d)
            } else {
                -count(f, d)
            };
        }
        chi += if dim % 2 == 0 {
            count(&m, dim)
        } else {
            -count(&m, dim)
        };
        assert_eq!(chi, 1, "{case}: Euler characteristic of the domain");
        // boundary
        let e2s = f.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
        let mut bfacets: Vec<Vec<Idx>> = Vec::new();
        for ty in f.element_types(TypeFilter::new().dim(dim - 1)) {
            let p1 = ty.p1_type().unwrap().nb_nodes_per_element();
            for (nodes, list) in f
                .connectivity(ty, G)
                .unwrap()
                .tuples()
                .zip(e2s.get(ty, G).unwrap().as_slice())
            {
                if list.iter().any(Element::is_null) {
                    bfacets.push(nodes[..p1].to_vec());
                }
            }
        }
        let vertices: BTreeSet<Idx> = bfacets.iter().flatten().copied().collect();
        let v = vertices.len() as i64;
        let fcount = bfacets.len() as i64;
        let bchi = match dim {
            1 => v,
            2 => v - fcount,
            _ => {
                let mut edges = BTreeSet::new();
                for face in &bfacets {
                    for k in 0..face.len() {
                        let (a, b) = (face[k], face[(k + 1) % face.len()]);
                        edges.insert((a.min(b), a.max(b)));
                    }
                }
                v - edges.len() as i64 + fcount
            }
        };
        let expected = match dim {
            1 => 2,
            2 => 0,
            _ => 2,
        };
        if !mixed_dims || dim > 1 {
            assert_eq!(bchi, expected, "{case}: boundary Euler characteristic");
        }
        checked += 1;
    }
    assert!(checked >= 30, "{checked}");
}

/// CSR consistency: element `e` appears in the row of node `n` exactly as
/// many times as `n` occurs in its connectivity.
#[test]
fn node_to_element_csr_matches_connectivity() {
    for (case, m) in fixture_meshes() {
        let dim = m.spatial_dimension();
        let csr = build_node_to_elements(&m, None).unwrap();
        let mut from_conn: BTreeMap<(Idx, Element), usize> = BTreeMap::new();
        for e in m.elements(TypeFilter::new().dim(dim)) {
            for &n in &m.element_connectivity(e).unwrap() {
                *from_conn.entry((n, e)).or_insert(0) += 1;
            }
        }
        let mut from_csr: BTreeMap<(Idx, Element), usize> = BTreeMap::new();
        for n in 0..csr.nb_rows() {
            for &e in csr.get_row(n).unwrap() {
                *from_csr.entry((Idx::try_from(n).unwrap(), e)).or_insert(0) += 1;
            }
        }
        assert_eq!(from_conn, from_csr, "{case}");
    }
}

/// Renumbering invariants of element removal: surviving rows keep their
/// connectivity at their new index, the numbering is a bijection of the
/// survivors onto `0..k`, element references and groups follow; a purify
/// afterwards keeps exactly the nodes still used, coordinates moving with
/// their node.
#[test]
fn removal_renumbering_invariants() {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut rnd = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for (case, mut m) in fixture_meshes() {
        let dim = m.spatial_dimension();
        let _ = init_mesh_facets(&mut m).unwrap();
        let before: Vec<(Element, Vec<Idx>)> = m
            .elements(TypeFilter::new().dim(dim))
            .map(|e| (e, m.element_connectivity(e).unwrap().to_vec()))
            .collect();
        let removed: Vec<Element> = before
            .iter()
            .filter(|_| rnd() % 4 == 0)
            .map(|(e, _)| *e)
            .collect();
        let groups_before: Vec<(String, Vec<Element>)> = m
            .groups()
            .element_groups()
            .map(|g| (g.name().to_owned(), g.iter_elements().collect()))
            .collect();
        let ev = erase_elements_setup(&mut m, &removed).unwrap();
        let MeshEvent::ElementsRemoved {
            new_numbering,
            elements,
            ..
        } = ev
        else {
            panic!("unexpected event")
        };
        assert_eq!(elements, removed);
        m.check_consistency().unwrap();
        let mut survivors: BTreeMap<ElementType, BTreeSet<Idx>> = BTreeMap::new();
        for (e, conn) in &before {
            let new = new_numbering
                .get(e.ty(), G)
                .map(|a| a.as_slice()[e.idx()])
                .unwrap_or(e.index());
            if removed.contains(e) {
                assert_eq!(new, -1, "{case}: {e}");
                continue;
            }
            let ne = Element::new(e.ty(), new, G);
            assert_eq!(
                m.element_connectivity(ne).unwrap(),
                conn.as_slice(),
                "{case}: {e} → {ne}"
            );
            assert!(survivors.entry(e.ty()).or_default().insert(new));
        }
        for (ty, s) in survivors {
            let k = m.nb_element(ty, G);
            assert_eq!(
                s,
                (0..Idx::try_from(k).unwrap()).collect::<BTreeSet<_>>(),
                "{case} {ty}"
            );
        }
        // parent rows of the facet mesh follow, references to removed
        // elements become ElementNull
        let f = m.mesh_facets().unwrap();
        let s2e = f.data().element_map(SUBELEMENT_TO_ELEMENT).unwrap();
        let e2s = f.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
        for e in m.elements(TypeFilter::new().dim(dim).kind(ElementKind::Regular)) {
            for &facet in s2e.element(e).unwrap().iter().filter(|x| !x.is_null()) {
                assert!(
                    e2s.element(facet).unwrap()[0].contains(&e),
                    "{case}: {e} / {facet}"
                );
            }
        }
        for (name, els) in groups_before {
            let after: Vec<Element> = m
                .groups()
                .element_group(&name)
                .unwrap()
                .iter_elements()
                .collect();
            let expected: Vec<Element> = els
                .iter()
                .filter(|e| !removed.contains(e))
                .map(|e| match new_numbering.get(e.ty(), e.ghost_type()) {
                    Ok(a) => Element::new(e.ty(), a.as_slice()[e.idx()], e.ghost_type()),
                    Err(_) => *e,
                })
                .collect();
            assert_eq!(after, expected, "{case}: group {name}");
        }
        // purify: exactly the used nodes survive, coordinates follow
        let coords_of = |m: &Mesh, e: Element| -> Vec<Vec<f64>> {
            m.element_connectivity(e)
                .unwrap()
                .iter()
                .map(|&n| m.nodes().tuple(n as usize).to_vec())
                .collect()
        };
        let elements: Vec<Element> = m.elements(TypeFilter::new()).collect();
        let coords: Vec<Vec<Vec<f64>>> = elements.iter().map(|&e| coords_of(&m, e)).collect();
        // nodes used by the parent or (after the erase) only by facets
        let f = m.mesh_facets().unwrap();
        let used: BTreeSet<Idx> = elements
            .iter()
            .flat_map(|&e| m.element_connectivity(e).unwrap().to_vec())
            .chain(
                f.elements(TypeFilter::new())
                    .flat_map(|e| f.element_connectivity(e).unwrap().to_vec()),
            )
            .collect();
        let _ = purify_mesh(&mut m).unwrap();
        assert_eq!(m.nb_nodes(), used.len(), "{case}");
        for (e, c) in elements.iter().zip(&coords) {
            assert_eq!(&coords_of(&m, *e), c, "{case}: {e}");
        }
        let f = m.mesh_facets().unwrap();
        assert_eq!(f.nb_nodes(), m.nb_nodes());
        f.check_consistency().unwrap();
    }
}

/// Periodic pairing symmetry: each slave has one master that lists it, no
/// master is a slave, flags agree with the maps, paired nodes coincide
/// after the periodic translation(s).
#[test]
fn periodic_pairing_is_symmetric() {
    for (case, mut m) in fixture_meshes() {
        if !case.contains("periodic") {
            continue;
        }
        let dim = m.spatial_dimension();
        let dirs = [
            SpatialDirection::X,
            SpatialDirection::Y,
            SpatialDirection::Z,
        ];
        for &d in dirs.iter().take(dim) {
            let _ = make_periodic(&mut m, d, None).unwrap();
        }
        let pairs = periodic_pairs(&m);
        let masters: BTreeSet<Idx> = pairs.iter().map(|p| p.0).collect();
        let size: Vec<f64> = (0..dim)
            .map(|k| m.upper_bounds()[k] - m.lower_bounds()[k])
            .collect();
        for &(ma, s) in &pairs {
            assert_eq!(m.periodic_master(s), Some(ma), "{case}");
            assert!(m.periodic_slaves(ma).contains(&s));
            assert!(!masters.contains(&s), "{case}: slave {s} is a master");
            assert!(m.is_periodic_slave(s).unwrap() && m.is_periodic_master(ma).unwrap());
            let nodes = m.nodes();
            let (xm, xs) = (nodes.tuple(ma as usize), nodes.tuple(s as usize));
            for k in 0..dim {
                let r = (xs[k] - xm[k]) / size[k];
                let r = r - r.round();
                assert!(r.abs() < 1e-9, "{case}: {ma} ~ {s} in direction {k}");
            }
        }
        for n in 0..m.nb_nodes() {
            let n = Idx::try_from(n).unwrap();
            let is_slave = m.periodic_master(n).is_some();
            assert_eq!(m.is_periodic_slave(n).unwrap(), is_slave, "{case} {n}");
        }
    }
}

// ------------------------------------------------------------- transactions

struct Recorder {
    seen: Vec<String>,
}

impl MeshEventHandler for Recorder {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority(50)
    }
    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        mesh.check_consistency()?;
        self.seen.push(format!("{event:?}"));
        Ok(())
    }
}

fn square(n: usize) -> Mesh {
    let mut m = Mesh::new(2, "sq").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        #[allow(clippy::cast_precision_loss)]
        for j in 0..=n {
            for i in 0..=n {
                a.push_node(&[i as f64, j as f64]).unwrap();
            }
        }
        // one unused node
        a.push_node(&[9.0, 9.0]).unwrap();
        let mut c = a.connectivity_mut(ElementType::Triangle3, G);
        let p = |i: usize, j: usize| Idx::try_from(j * (n + 1) + i).unwrap();
        for j in 0..n {
            for i in 0..n {
                c.push_tuple(&[p(i, j), p(i + 1, j), p(i + 1, j + 1)]);
                c.push_tuple(&[p(i, j), p(i + 1, j + 1), p(i, j + 1)]);
            }
        }
    }
    let mut g = m.groups_mut().create_element_group("left", 2).unwrap();
    g.add(
        Element::new(ElementType::Triangle3, 1, G),
        Some(&[0, 4, 3]),
        false,
    )
    .unwrap();
    m.compute_bounding_box().unwrap();
    m
}

/// Journaled removals: committed events reach the consumer in order on a
/// consistent mesh; a failing closure restores the complete state
/// (fingerprint), including the facet mesh created in the transaction.
#[test]
fn transaction_removals_commit_and_roll_back() {
    let mesh = SharedMesh::new(square(2));
    let token = mesh
        .register_consumer("recorder", EventPriority(50), MeshPart::Main)
        .unwrap();
    let mut rec = Recorder { seen: Vec::new() };
    let before = mesh.read().unwrap().state_fingerprint();
    // rollback: facets + erase + purify, then an error
    let err = mesh
        .mutate(
            &mut [(&token, &mut rec)],
            |tx| {
                tx.init_mesh_facets()?;
                tx.erase_elements(
                    MeshPart::Main,
                    &[Element::new(ElementType::Triangle3, 0, G)],
                )?;
                tx.purify(MeshPart::Main)?;
                Err(akantu_core::Error::invalid("injected failure"))
            },
            None,
        )
        .unwrap_err();
    assert!(err.to_string().contains("injected"));
    assert_eq!(mesh.read().unwrap().state_fingerprint(), before);
    assert!(rec.seen.is_empty());
    // commit
    let committed = mesh
        .mutate(
            &mut [(&token, &mut rec)],
            |tx| {
                tx.init_mesh_facets()?;
                tx.erase_elements(
                    MeshPart::Main,
                    &[
                        Element::new(ElementType::Triangle3, 0, G),
                        Element::new(ElementType::Triangle3, 5, G),
                    ],
                )?;
                tx.purify(MeshPart::Main)
            },
            None,
        )
        .unwrap();
    // the node removal is mirrored into the facet mesh with its own event
    // for facet consumers
    let parts: Vec<MeshPart> = committed.events.iter().map(|(p, _)| *p).collect();
    assert_eq!(
        parts,
        vec![MeshPart::Main, MeshPart::Main, MeshPart::Facets]
    );
    assert_eq!(rec.seen.len(), 2);
    assert!(rec.seen[0].starts_with("ElementsRemoved(2"));
    assert!(rec.seen[1].starts_with("NodesRemoved(1"));
    let m = mesh.read().unwrap();
    m.check_consistency().unwrap();
    assert_eq!(m.nb_element(ElementType::Triangle3, G), 6);
    assert_eq!(m.nb_nodes(), 9);
    let f = m.mesh_facets().unwrap();
    assert_eq!(f.nb_nodes(), 9);
    f.check_consistency().unwrap();
    // element 1 of group "left" survived (it was not removed, not moved)
    assert_eq!(
        m.groups()
            .element_group("left")
            .unwrap()
            .elements_of(ElementType::Triangle3, G),
        &[1]
    );
}

/// Inconsistent removal events recorded by hand are refused at the
/// topology stage (D-037 follow-up), and invalid removal requests are
/// errors, never panics.
#[test]
fn invalid_removals_are_errors() {
    let mesh = SharedMesh::new(square(1));
    let before = mesh.read().unwrap().state_fingerprint();
    let bad = mesh.mutate(
        &mut [],
        |tx| {
            let mut numbering = akantu_core::ElementTypeMapArray::new("n");
            numbering
                .alloc(2, 1, ElementType::Triangle3, G, 0)
                .unwrap()
                .as_mut_slice()
                .copy_from_slice(&[0, -1]);
            tx.record(
                MeshPart::Main,
                MeshEvent::ElementsRemoved {
                    elements: vec![Element::new(ElementType::Triangle3, 1, G)],
                    new_numbering: numbering,
                    origin: "test".into(),
                },
            );
            Ok(())
        },
        None,
    );
    assert!(bad.is_err());
    assert_eq!(mesh.read().unwrap().state_fingerprint(), before);
    for (els, what) in [
        (
            vec![Element::new(ElementType::Triangle3, 7, G)],
            "out of range",
        ),
        (vec![Element::NULL], "null"),
        (
            vec![Element::new(ElementType::Quadrangle4, 0, G)],
            "absent type",
        ),
    ] {
        let r = mesh.mutate(&mut [], |tx| tx.erase_elements(MeshPart::Main, &els), None);
        assert!(r.is_err(), "{what}");
    }
    for numbering in [vec![0, 1], vec![0, 0, 1, 2, 3], vec![3, 2, 1, -1, 0]] {
        let r = mesh.mutate(
            &mut [],
            |tx| tx.remove_nodes(MeshPart::Main, &numbering),
            None,
        );
        assert!(r.is_err(), "{numbering:?}");
    }
    assert!(
        mesh.mutate(&mut [], |tx| tx.purify(MeshPart::Facets), None)
            .is_err()
    );
    assert_eq!(mesh.read().unwrap().state_fingerprint(), before);
}

/// UD-131 (A, MUTIL-004): facet construction on structural meshes. Upstream
/// fails there (oracle probe `oracle/probes/ud131`, x86_64 strict:
/// `AssertException` "position [66,0] is out of range in array
/// rows_offsets" for `_bernoulli_beam_2`, `_bernoulli_beam_3` and
/// `_discrete_kirchhoff_triangle_18`, in `initMeshFacets` and in
/// `buildFacets`): the structural classes declare the 0-node facet type
/// `_not_defined` and the facet loop runs over the unbounded
/// `arange(1, 0)`. The port builds no facet for them: both operations
/// succeed and leave no facet element.
#[test]
fn ud131_structural_meshes_get_no_facets() {
    let cases: [(ElementType, usize, Vec<f64>, Vec<Idx>); 3] = [
        (
            ElementType::BernoulliBeam2,
            2,
            vec![0., 0., 1., 0., 2., 0.5],
            vec![0, 1, 1, 2],
        ),
        (
            ElementType::BernoulliBeam3,
            3,
            vec![0., 0., 0., 1., 0., 0., 2., 0.5, 0.25],
            vec![0, 1, 1, 2],
        ),
        (
            ElementType::DiscreteKirchhoffTriangle18,
            3,
            vec![0., 0., 0., 1., 0., 0., 1., 1., 0., 0., 1., 0.],
            vec![0, 1, 2, 0, 2, 3],
        ),
    ];
    for (ty, dim, nodes, conn) in cases {
        let build = || {
            let mut m = Mesh::new(dim, "structural").unwrap();
            {
                let mut a = MeshAccessor::new(&mut m);
                for x in nodes.chunks(dim) {
                    a.push_node(x).unwrap();
                }
                let npe = ty.nb_nodes_per_element();
                let mut c = a.connectivity_mut(ty, G);
                for t in conn.chunks(npe) {
                    c.push_tuple(t);
                }
            }
            m.compute_bounding_box().unwrap();
            m
        };
        let mut m = build();
        // set-up operations: no consumer to dispatch the events to
        let _events = akantu_mesh::utils::init_mesh_facets(&mut m).unwrap();
        let facets = m.mesh_facets().unwrap();
        let n: usize = facets
            .element_types(TypeFilter {
                spatial_dimension: None,
                ghost_type: G,
                kind: None,
            })
            .map(|t| facets.nb_element(t, G))
            .sum();
        assert_eq!(n, 0, "{ty:?}: facet elements in the facet mesh");
        let mut m = build();
        let _events = akantu_mesh::utils::build_facets(&mut m).unwrap();
        let types: Vec<ElementType> = m
            .element_types(TypeFilter {
                spatial_dimension: None,
                ghost_type: G,
                kind: None,
            })
            .filter(|&t| m.nb_element(t, G) > 0)
            .collect();
        assert_eq!(types, vec![ty], "{ty:?}: buildFacets added elements");
    }
}
