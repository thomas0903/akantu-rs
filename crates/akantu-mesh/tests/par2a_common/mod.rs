// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/{test_facet_synchronizer.cc,
// test_synchronizers_fixture.hh, test_data_accessor.hh, test_grid_synchronizer.cc,
// test_grid_synchronizer_check_neighbors.cc, test_grid_tools.hh},
// test/test_mesh_utils/test_mesh_periodic/{test_mesh_periodic.cc,
// test_mesh_periodic_fixture.hh}.

//! scenarios (distributed facets, facet / grid / periodic
//! synchronizers) shared by the local-world tests (`upstream_par2a.rs`,
//! `parallel_oracle_par2a.rs`; 1/2/4 ranks as threads) and the real-MPI
//! binary (`mpi_synchronizers.rs`). Every scenario takes the communicator
//! and returns `Err(message)` on failure. Uses `par_common` (declared by
//! the including binary).

#![allow(dead_code)]
// each binary uses a subset
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
// test indices are small
// `ensure!(a <= b, …)` negates a float comparison on purpose (NaN fails)
#![allow(clippy::neg_cmp_op_on_partial_ord)]

use std::collections::{BTreeMap, BTreeSet};

use akantu_comm::Communicator;
use akantu_core::enums::SynchronizationTag;
use akantu_core::{Element, ElementType, GhostType, GlobalIdx, Idx, Rank, Real, TypeFilter};
use akantu_mesh::Mesh;
use akantu_mesh::facet_synchronizer::{DistributedFacetsOptions, FacetGlobalConnectivityAccessor};
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, FacetFlipMode, MESH_TO_MESH_FACET, SUBELEMENT_TO_ELEMENT,
    init_mesh_facets_with,
};

use crate::ensure;
use crate::par_common::{
    BarycenterAccessor, Outcome, barycenters, distribute_prescribed, distribute_rcb, ok,
    oracle_partition, read_on_root,
};

// ------------------------------------------------------------------ meshes

/// Structured Tet4 cube `[0,1]^3` (`n^3` cells, Kuhn split), node and
/// element order of the oracle probe's `cube_tet4`.
pub fn cube_tet4_on_root(comm: &Communicator, n: usize) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(3, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        {
            let mut a = akantu_mesh::MeshAccessor::new(&mut m);
            let h = 1.0 / n as Real;
            let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as Idx;
            for k in 0..=n {
                for j in 0..=n {
                    for i in 0..=n {
                        ok(
                            a.push_node(&[i as Real * h, j as Real * h, k as Real * h]),
                            "node",
                        )?;
                    }
                }
            }
            let kuhn: [[usize; 4]; 6] = [
                [0, 1, 2, 6],
                [0, 2, 3, 6],
                [0, 3, 7, 6],
                [0, 7, 4, 6],
                [0, 4, 5, 6],
                [0, 5, 1, 6],
            ];
            let mut c = a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
            for k in 0..n {
                for j in 0..n {
                    for i in 0..n {
                        let v = [
                            id(i, j, k),
                            id(i + 1, j, k),
                            id(i + 1, j + 1, k),
                            id(i, j + 1, k),
                            id(i, j, k + 1),
                            id(i + 1, j, k + 1),
                            id(i + 1, j + 1, k + 1),
                            id(i, j + 1, k + 1),
                        ];
                        for t in &kuhn {
                            c.push_tuple(&[v[t[0]], v[t[1]], v[t[2]], v[t[3]]]);
                        }
                    }
                }
            }
        }
        ok(m.compute_bounding_box(), "bbox")?;
    }
    Ok(m)
}

/// Structured Tri3 square `[0,1]^2` (`n^2` cells split along the diagonal),
/// order of the oracle probe's `square_tri3`.
pub fn square_tri3_on_root(comm: &Communicator, n: usize) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 {
        {
            let mut a = akantu_mesh::MeshAccessor::new(&mut m);
            let h = 1.0 / n as Real;
            let id = |i: usize, j: usize| (j * (n + 1) + i) as Idx;
            for j in 0..=n {
                for i in 0..=n {
                    ok(a.push_node(&[i as Real * h, j as Real * h]), "node")?;
                }
            }
            let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
            for j in 0..n {
                for i in 0..n {
                    c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                    c.push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
                }
            }
        }
        ok(m.compute_bounding_box(), "bbox")?;
    }
    Ok(m)
}

/// Distribute with the oracle's partition of `scenario` at this rank count
/// when it exists (1/2/4 ranks), otherwise with the bisection partitioner.
pub fn distribute_like_oracle(comm: &Communicator, m: &mut Mesh, scenario: &str) -> Outcome {
    let np = comm.size();
    let have = crate::par_common::oracle_rank_counts(scenario).contains(&np);
    if np > 1 && have {
        let parts = if comm.rank() == 0 {
            oracle_partition(scenario, np)?
        } else {
            Vec::new()
        };
        distribute_prescribed(m, parts)
    } else {
        distribute_rcb(m)
    }
}

/// `cube.msh` distributed like `facets_cube_msh`, with its facet mesh.
pub fn facets_cube_msh(
    comm: &Communicator,
    flip: FacetFlipMode,
) -> std::result::Result<Mesh, String> {
    let mut m = read_on_root(comm, "test_synchronizer/cube.msh", 3)?;
    distribute_like_oracle(comm, &mut m, "facets_cube_msh")?;
    let _ = ok(
        init_mesh_facets_with(&mut m, DistributedFacetsOptions { flip }),
        "init_mesh_facets",
    )?;
    Ok(m)
}

// ------------------------------------------------- TEST-170 (upstream test)

fn facet_mesh(m: &Mesh) -> std::result::Result<&Mesh, String> {
    m.mesh_facets().ok_or_else(|| "no facet mesh".to_owned())
}

/// Upstream `TestFacetSynchronizerFixture.{SynchroneOnce, Synchrone,
/// Asynchrone}` (`test_facet_synchronizer.cc`): after `initMeshFacets` on
/// the distributed `cube.msh`, the barycentres of the ghost facets received
/// through the facet synchronizer equal the local ones (upstream
/// `EXPECT_NEAR(0, dist, 1e-15)`; bitwise here with the default
/// `RemoteOrder` flip, since a ghost facet has its owner's node order).
pub fn test_facet_synchronizer(comm: Communicator) -> Outcome {
    let m = facets_cube_msh(&comm, FacetFlipMode::RemoteOrder)?;
    let f = facet_mesh(&m)?;
    ensure!(f.is_distributed(), "facet mesh not distributed");
    let b = barycenters(f)?;
    let sync = f
        .synchronizers()
        .element
        .clone()
        .ok_or("no facet synchronizer")?;
    let nb_recv: usize = sync.communications.recv.values().map(Vec::len).sum();
    let mut acc = BarycenterAccessor {
        dim: 3,
        bary: &b,
        max_dist: 0.0,
        unpacked: 0,
    };
    // SynchroneOnce / Synchrone (same call here)
    ok(
        sync.synchronize(&mut acc, SynchronizationTag::Test),
        "synchronize",
    )?;
    ensure!(
        acc.max_dist <= 1e-15,
        "facet barycenter distance {}",
        acc.max_dist
    );
    ensure!(
        acc.max_dist == 0.0,
        "facet barycenters not bitwise: {}",
        acc.max_dist
    );
    ensure!(
        acc.unpacked == nb_recv,
        "unpacked {} of {nb_recv}",
        acc.unpacked
    );
    // Asynchrone (random delay 10..100 µs upstream)
    ok(
        sync.asynchronous_synchronize(&acc, SynchronizationTag::Test),
        "asynchronous",
    )?;
    std::thread::sleep(std::time::Duration::from_micros(
        10 + 29 * comm.rank() as u64,
    ));
    ok(
        sync.wait_end_synchronize(&mut acc, SynchronizationTag::Test),
        "wait_end",
    )?;
    ensure!(
        acc.max_dist == 0.0,
        "facet barycenters not bitwise: {}",
        acc.max_dist
    );
    // the upstream-compatible flip gives the same facets here (every
    // ghost/owner pair differs by a reflection, an involution)
    let mu = facets_cube_msh(&comm, FacetFlipMode::UpstreamInverse)?;
    ensure!(
        facet_mesh(&mu)?.state_fingerprint() == f.state_fingerprint(),
        "UpstreamInverse and RemoteOrder facet meshes differ on cube.msh"
    );
    Ok(())
}

// ------------------------------------------------------ independent checks

/// Sorted global connectivity of a facet.
fn sorted_gconn(f: &Mesh, e: Element) -> std::result::Result<Vec<GlobalIdx>, String> {
    let mut g: Vec<GlobalIdx> = ok(f.element_connectivity(e), "conn")?
        .iter()
        .map(|&n| f.node_global_id(n))
        .collect();
    g.sort_unstable();
    Ok(g)
}

/// Flat `(sorted global connectivity…)` of every facet of dimension − 1 of
/// one ghost type, with the facet type codes, for gathering.
fn pack_facets(f: &Mesh, ghost: GhostType) -> std::result::Result<Vec<i64>, String> {
    let dim = f.spatial_dimension();
    let mut v = Vec::new();
    for ty in f.element_types(TypeFilter::new().dim(dim - 1).ghost(ghost)) {
        for e in 0..f.nb_element(ty, ghost) {
            let g = sorted_gconn(f, Element::new(ty, e as Idx, ghost))?;
            v.push(ty as u8 as i64);
            v.push(g.len() as i64);
            v.extend(g);
        }
    }
    Ok(v)
}

fn unpack_facets(v: &[i64]) -> Vec<(i64, Vec<i64>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < v.len() {
        let n = v[i + 1] as usize;
        out.push((v[i], v[i + 2..i + 2 + n].to_vec()));
        i += 2 + n;
    }
    out
}

/// Independent invariants of a distributed facet mesh (gathered on every
/// rank; test-only all-gathers):
/// * every facet of dimension − 1 is local on **exactly one** rank, and the
///   union of the local facets is the serial facet set of the global mesh;
/// * every ghost facet is in the receive scheme of exactly one rank, which
///   holds it as a local facet; the owner is the minimum owner rank of the
///   ghost elements it bounds;
/// * facet schemes are symmetric (the sender's list equals the receiver's,
///   compared by sorted global connectivity, in order);
/// * with the default flip, every ghost facet's global connectivity equals
///   its owner's in order (node and facet global ids consistent);
/// * `element_to_subelement` / `subelement_to_element` are reciprocal and
///   the ghost type of a facet follows upstream's rule.
pub fn facet_invariants(comm: Communicator) -> Outcome {
    let rank = comm.rank();
    let size = comm.size();
    // serial facet set of the global mesh, on every rank
    let serial = {
        let mut s = ok(Mesh::new(3, "serial"), "mesh")?;
        let opts = akantu_io::ReadOptions {
            fill_unnamed_physical_groups: false,
            write_diana_material_file: false,
        };
        ok(
            akantu_io::read_mesh_with(
                &mut s,
                &crate::par_common::frozen("test_synchronizer/cube.msh"),
                akantu_core::enums::MeshIoType::Auto,
                &opts,
            ),
            "read",
        )?;
        let _ = ok(
            akantu_mesh::utils::init_mesh_facets(&mut s),
            "serial facets",
        )?;
        let f = facet_mesh(&s)?;
        let mut set = BTreeSet::new();
        for (_, g) in unpack_facets(&pack_facets(f, GhostType::NotGhost)?) {
            set.insert(g);
        }
        set
    };
    let m = facets_cube_msh(&comm, FacetFlipMode::RemoteOrder)?;
    let f = facet_mesh(&m)?;
    let local = pack_facets(f, GhostType::NotGhost)?;
    let all = ok(comm.all_gather_v(&local), "gather")?;
    let mut owner: BTreeMap<Vec<i64>, Rank> = BTreeMap::new();
    for (r, v) in all.iter().enumerate() {
        for (_, g) in unpack_facets(v) {
            if let Some(prev) = owner.insert(g.clone(), r as Rank) {
                return Err(format!("facet {g:?} local on ranks {prev} and {r}"));
            }
        }
    }
    let owned: BTreeSet<Vec<i64>> = owner.keys().cloned().collect();
    ensure!(
        owned == serial,
        "union of the local facets ({}) is not the serial facet set ({})",
        owned.len(),
        serial.len()
    );
    // ghost facets
    let fs = f
        .synchronizers()
        .element
        .as_ref()
        .ok_or("no facet synchronizer")?;
    let mut in_scheme: BTreeMap<Element, Rank> = BTreeMap::new();
    for (&q, facets) in &fs.communications.recv {
        for &e in facets {
            if let Some(p) = in_scheme.insert(e, q) {
                return Err(format!("ghost facet {e} received from {p} and {q}"));
            }
        }
    }
    let dim = f.spatial_dimension();
    let e2s = ok(f.data().element_lists(ELEMENT_TO_SUBELEMENT), "e2s")?;
    let es = m
        .synchronizers()
        .element
        .as_ref()
        .ok_or("no element synchronizer")?;
    let mut element_owner: BTreeMap<Element, Rank> = BTreeMap::new();
    for (&q, els) in &es.communications.recv {
        for &e in els {
            element_owner.insert(e, q);
        }
    }
    for ty in f.element_types(TypeFilter::new().dim(dim - 1).ghost(GhostType::Ghost)) {
        for i in 0..f.nb_element(ty, GhostType::Ghost) {
            let e = Element::new(ty, i as Idx, GhostType::Ghost);
            let q = *in_scheme
                .get(&e)
                .ok_or_else(|| format!("ghost facet {e} in no receive scheme"))?;
            let g = sorted_gconn(f, e)?;
            ensure!(
                owner.get(&g) == Some(&q),
                "ghost facet {e} received from {q}, local on {:?}",
                owner.get(&g)
            );
            // owner = minimum owner of its ghost neighbours
            let row = &ok(e2s.element(e), "e2s row")?[0];
            let min = row
                .iter()
                .filter(|n| !n.is_null() && n.ghost_type() == GhostType::Ghost)
                .filter_map(|n| element_owner.get(n))
                .min()
                .copied();
            ensure!(
                min == Some(q),
                "ghost facet {e}: owner {q}, minimum neighbour owner {min:?}"
            );
        }
    }
    // ghost-type rule for facets with one local and one ghost neighbour
    for ghost in GhostType::ALL {
        for ty in f.element_types(TypeFilter::new().dim(dim - 1).ghost(ghost)) {
            for i in 0..f.nb_element(ty, ghost) {
                let e = Element::new(ty, i as Idx, ghost);
                let row = &ok(e2s.element(e), "e2s row")?[0];
                let real: Vec<Element> = row.iter().copied().filter(|n| !n.is_null()).collect();
                let nl = real
                    .iter()
                    .filter(|n| n.ghost_type() == GhostType::NotGhost)
                    .count();
                let expect = match (real.len(), nl) {
                    (_, 0) => GhostType::Ghost,
                    (2, 1) => {
                        let other = real
                            .iter()
                            .find(|n| n.ghost_type() == GhostType::Ghost)
                            .and_then(|n| element_owner.get(n))
                            .copied()
                            .ok_or("ghost neighbour without owner")?;
                        if other > rank {
                            GhostType::NotGhost
                        } else {
                            GhostType::Ghost
                        }
                    }
                    _ => GhostType::NotGhost,
                };
                ensure!(
                    expect == ghost,
                    "facet {e}: ghost type {ghost}, rule says {expect}"
                );
            }
        }
    }
    // reciprocity: every element listed by a facet lists the facet
    let s2e = ok(f.data().element_map(SUBELEMENT_TO_ELEMENT), "s2e")?;
    for ghost in GhostType::ALL {
        for ty in f.element_types(TypeFilter::new().dim(dim - 1).ghost(ghost)) {
            for i in 0..f.nb_element(ty, ghost) {
                let e = Element::new(ty, i as Idx, ghost);
                for n in &ok(e2s.element(e), "e2s row")?[0] {
                    if n.is_null() {
                        continue;
                    }
                    ensure!(
                        ok(s2e.element(*n), "s2e row")?.contains(&e),
                        "{n} does not list its facet {e}"
                    );
                }
            }
        }
    }
    // symmetric schemes + equal ordered global connectivities (owner →
    // ghost exchange of the global connectivities)
    let mut acc = ok(FacetGlobalConnectivityAccessor::new(f), "accessor")?;
    let before = acc.global_connectivity.clone();
    ok(
        fs.synchronize(&mut acc, SynchronizationTag::SmmcFacetsConn),
        "conn sync",
    )?;
    for (ty, ghost, a) in acc.global_connectivity.as_map().iter_all() {
        let b = ok(before.get(ty, ghost), "before")?;
        ensure!(
            a.as_slice() == b.as_slice(),
            "a ghost facet's global connectivity ({ty}:{ghost}) differs from its owner's after \
             the flip"
        );
    }
    // node synchronizer consistency: the nodes of every facet carry the
    // owner's global ids (node synchronizer exchange of global ids)
    let mut gids: Vec<i64> = (0..m.nb_nodes())
        .map(|n| m.node_global_id(n as Idx))
        .collect();
    let ns = m
        .synchronizers()
        .node
        .as_ref()
        .ok_or("no node synchronizer")?;
    let before = gids.clone();
    ok(ns.synchronize_array(&mut gids, 1), "node gids")?;
    ensure!(gids == before, "node global ids differ from their owners'");
    let _ = size;
    Ok(())
}

// --------------------------------------------------------- oracle compare

/// `(type code, element, ghost)` encoding of the probe (`push_element`).
fn encode(e: Element, table: &[String]) -> std::result::Result<[i64; 3], String> {
    if e.is_null() {
        return Ok([-1, -1, -1]);
    }
    let code = if e.ty() == ElementType::NotDefined {
        -2
    } else {
        table
            .iter()
            .position(|t| t == e.ty().name())
            .ok_or_else(|| format!("type {} not in the oracle table", e.ty()))? as i64
    };
    Ok([
        code,
        i64::from(e.index()),
        i64::from(e.ghost_type() == GhostType::Ghost),
    ])
}

fn ghost_key(ghost: GhostType) -> &'static str {
    if ghost == GhostType::NotGhost {
        "not_ghost"
    } else {
        "ghost"
    }
}

/// Compare a distributed facet mesh (and the parent's `mesh_to_mesh_facet`)
/// with the oracle's record (probe `record_facets`): blocks, connectivities,
/// `element_to_subelement` rows, `subelement_to_element`, facet synchronizer
/// schemes. Exact.
pub fn compare_facets_with_oracle(m: &Mesh, fx: &akantu_validation::Fixture) -> Outcome {
    let p = &fx.manifest().parameters;
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let strs = |key: &str| -> Vec<String> {
        p[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let table = strs("type_table");
    let f = facet_mesh(m)?;
    ensure!(
        p["facets/is_distributed"].as_bool() == Some(f.is_distributed()),
        "facet mesh distribution flag"
    );
    // blocks + connectivities
    let mut blocks = Vec::new();
    for ghost in GhostType::ALL {
        for ty in f.element_types(TypeFilter::new().ghost(ghost)) {
            let key = format!("{}/{}", ty.name(), ghost_key(ghost));
            let conn: Vec<i64> = ok(f.connectivity(ty, ghost), "conn")?
                .as_slice()
                .iter()
                .map(|&v| i64::from(v))
                .collect();
            ensure!(
                conn == li(&format!("facets/conn/{key}"))?,
                "facet connectivity {key} differs"
            );
            blocks.push(key);
        }
    }
    ensure!(
        blocks == strs("facets/blocks"),
        "facet blocks {blocks:?} vs {:?}",
        strs("facets/blocks")
    );
    // element_to_subelement
    let e2s = ok(f.data().element_lists(ELEMENT_TO_SUBELEMENT), "e2s")?;
    let mut keys = Vec::new();
    for ghost in GhostType::ALL {
        for ty in e2s.element_types(TypeFilter::new().ghost(ghost)) {
            let key = format!("{}/{}", ty.name(), ghost_key(ghost));
            let rows = ok(e2s.get(ty, ghost), "e2s block")?;
            let mut len = Vec::new();
            let mut v = Vec::new();
            for row in rows.as_slice() {
                len.push(row.len() as i64);
                for &e in row {
                    v.extend(encode(e, &table)?);
                }
            }
            ensure!(
                len == li(&format!("facets/e2s_len/{key}"))?,
                "e2s row lengths {key} differ"
            );
            ensure!(v == li(&format!("facets/e2s/{key}"))?, "e2s {key} differs");
            keys.push(key);
        }
    }
    ensure!(
        keys == strs("facets/e2s_blocks"),
        "e2s blocks {keys:?} vs {:?}",
        strs("facets/e2s_blocks")
    );
    // subelement_to_element
    let s2e = ok(f.data().element_map(SUBELEMENT_TO_ELEMENT), "s2e")?;
    let mut keys = Vec::new();
    for ghost in GhostType::ALL {
        for ty in s2e.element_types(TypeFilter::new().ghost(ghost)) {
            let key = format!("{}/{}", ty.name(), ghost_key(ghost));
            let a = ok(s2e.get(ty, ghost), "s2e block")?;
            ensure!(
                p[format!("facets/s2e_nb_component/{key}")].as_u64()
                    == Some(a.nb_component() as u64),
                "s2e components {key}"
            );
            let mut v = Vec::new();
            for &e in a.as_slice() {
                v.extend(encode(e, &table)?);
            }
            ensure!(v == li(&format!("facets/s2e/{key}"))?, "s2e {key} differs");
            keys.push(key);
        }
    }
    ensure!(
        keys == strs("facets/s2e_blocks"),
        "s2e blocks {keys:?} vs {:?}",
        strs("facets/s2e_blocks")
    );
    // facet synchronizer schemes
    let fs = f
        .synchronizers()
        .element
        .as_ref()
        .ok_or("no facet synchronizer")?;
    let mut keys = Vec::new();
    for (dir, sch) in [
        ("send", &fs.communications.send),
        ("recv", &fs.communications.recv),
    ] {
        for (q, els) in sch {
            let key = format!("{dir}/{q}");
            let mut v = Vec::new();
            for &e in els {
                v.extend(encode(e, &table)?);
            }
            ensure!(
                v == li(&format!("facets/scheme/{key}"))?,
                "facet scheme {key} differs ({} entries)",
                els.len()
            );
            keys.push(key);
        }
    }
    ensure!(
        keys == strs("facets/schemes"),
        "facet schemes {keys:?} vs {:?}",
        strs("facets/schemes")
    );
    // parent mesh_to_mesh_facet
    let m2f_blocks = strs("m2f_blocks");
    match m.data().element_map(MESH_TO_MESH_FACET) {
        Ok(m2f) => {
            let mut keys = Vec::new();
            for ghost in GhostType::ALL {
                for ty in m2f.element_types(TypeFilter::new().ghost(ghost)) {
                    let key = format!("{}/{}", ty.name(), ghost_key(ghost));
                    let mut v = Vec::new();
                    for &e in ok(m2f.get(ty, ghost), "m2f")?.as_slice() {
                        v.extend(encode(e, &table)?);
                    }
                    // UD-128: upstream's unmatched rows hold the default
                    // `Element()` (code -2), normalised to the canonical NULL
                    let mut want = li(&format!("m2f/{key}"))?;
                    for t in want.as_chunks_mut::<3>().0 {
                        if t[0] == -2 {
                            *t = [-1, -1, -1];
                        }
                    }
                    ensure!(v == want, "m2f {key} differs");
                    keys.push(key);
                }
            }
            ensure!(keys == m2f_blocks, "m2f blocks {keys:?} vs {m2f_blocks:?}");
        }
        Err(_) => ensure!(
            m2f_blocks.is_empty(),
            "no mesh_to_mesh_facet, oracle has {m2f_blocks:?}"
        ),
    }
    Ok(())
}

fn open_case(
    scenario: &str,
    comm: &Communicator,
) -> std::result::Result<akantu_validation::Fixture, String> {
    akantu_validation::Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{}/rank{}",
        comm.size(),
        comm.rank()
    ))
    .map_err(|e| e.to_string())
}

/// `facets_cube_msh` equals the oracle (both flip modes; parent first).
pub fn facets_cube_msh_matches_oracle(comm: Communicator) -> Outcome {
    let fx = open_case("facets_cube_msh", &comm)?;
    for flip in [FacetFlipMode::UpstreamInverse, FacetFlipMode::RemoteOrder] {
        let m = facets_cube_msh(&comm, flip)?;
        crate::par_common::compare_with_oracle(&m, &fx)?;
        compare_facets_with_oracle(&m, &fx).map_err(|e| format!("{flip:?}: {e}"))?;
    }
    Ok(())
}

/// `facets_cube_n4` (structured Tet4 cube) equals the oracle.
pub fn facets_cube_n4_matches_oracle(comm: Communicator) -> Outcome {
    let fx = open_case("facets_cube_n4", &comm)?;
    for flip in [FacetFlipMode::UpstreamInverse, FacetFlipMode::RemoteOrder] {
        let mut m = cube_tet4_on_root(&comm, 4)?;
        distribute_like_oracle(&comm, &mut m, "facets_cube_n4")?;
        let _ = ok(
            init_mesh_facets_with(&mut m, DistributedFacetsOptions { flip }),
            "facets",
        )?;
        crate::par_common::compare_with_oracle(&m, &fx)?;
        compare_facets_with_oracle(&m, &fx).map_err(|e| format!("{flip:?}: {e}"))?;
    }
    Ok(())
}

/// `facets_adv2d` (2-D mixed Quad4/Tri3, compat orphan placement as the
/// oracle) equals the oracle.
pub fn facets_adv2d_matches_oracle(comm: Communicator) -> Outcome {
    let fx = open_case("facets_adv2d", &comm)?;
    for flip in [FacetFlipMode::UpstreamInverse, FacetFlipMode::RemoteOrder] {
        let mut m = crate::par_common::adv2d_distributed(&comm, true)?;
        let _ = ok(
            init_mesh_facets_with(&mut m, DistributedFacetsOptions { flip }),
            "facets",
        )?;
        crate::par_common::compare_with_oracle(&m, &fx)?;
        compare_facets_with_oracle(&m, &fx).map_err(|e| format!("{flip:?}: {e}"))?;
    }
    Ok(())
}

// ------------------------------------------------- bounding boxes

/// `[local lower, local upper, global lower, global upper]` of a mesh, the
/// layout of the probe's `record_bbox` (`mesh/bbox_before`, `mesh/bbox_after`).
pub fn bbox_values(m: &Mesh) -> Vec<Real> {
    let mut v = m.local_bbox().lower.clone();
    v.extend_from_slice(&m.local_bbox().upper);
    v.extend_from_slice(m.lower_bounds());
    v.extend_from_slice(m.upper_bounds());
    v
}

/// Bitwise comparison of recorded box values with the oracle's `name`.
pub fn compare_bbox_values(have: &[Real], fx: &akantu_validation::Fixture, name: &str) -> Outcome {
    let want = fx
        .load_f64(name)
        .map(|f| f.data)
        .map_err(|e| format!("{name}: {e}"))?;
    ensure!(
        have.len() == want.len()
            && have
                .iter()
                .zip(&want)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
        "{name}: port {have:?} vs oracle {want:?} (local lower, local upper, global lower, global upper)"
    );
    Ok(())
}

// ------------------------------------------- scenarios (facets)

/// A structured mesh of the adversarial scenarios: Tri3 square (2-D) or Tet4
/// cube (3-D) with `n` cells per direction, on rank 0.
pub fn v101_mesh_on_root(
    comm: &Communicator,
    dim: usize,
    n: usize,
) -> std::result::Result<Mesh, String> {
    if dim == 2 {
        square_tri3_on_root(comm, n)
    } else {
        cube_tet4_on_root(comm, n)
    }
}

/// `v101_facets_{tri,tet}_quadrants` (quadrant partition: facets seen as
/// ghosts of two other ranks) equal the oracle, both flip modes; the boxes
/// are unchanged by `initMeshFacets`.
pub fn v101_facets_matches_oracle_of(scenario: &str, dim: usize, comm: &Communicator) -> Outcome {
    let fx = open_case(scenario, comm)?;
    for flip in [FacetFlipMode::UpstreamInverse, FacetFlipMode::RemoteOrder] {
        let mut m = v101_mesh_on_root(comm, dim, 4)?;
        distribute_like_oracle(comm, &mut m, scenario)?;
        compare_bbox_values(&bbox_values(&m), &fx, "mesh/bbox_before")?;
        let _ = ok(
            init_mesh_facets_with(&mut m, DistributedFacetsOptions { flip }),
            "facets",
        )?;
        crate::par_common::compare_with_oracle(&m, &fx)?;
        compare_facets_with_oracle(&m, &fx).map_err(|e| format!("{flip:?}: {e}"))?;
        compare_bbox_values(&bbox_values(&m), &fx, "mesh/bbox_after")?;
    }
    Ok(())
}

pub fn v101_facets_tri_quadrants_matches_oracle(comm: Communicator) -> Outcome {
    v101_facets_matches_oracle_of("v101_facets_tri_quadrants", 2, &comm)
}

pub fn v101_facets_tet_quadrants_matches_oracle(comm: Communicator) -> Outcome {
    v101_facets_matches_oracle_of("v101_facets_tet_quadrants", 3, &comm)
}

// --------------------------------------------------------- failure paths

/// A rank-specific failure while building the distributed facet mesh is
/// an error on every rank (agreed, far below the timeout), no rank
/// installs a facet mesh, and the communicator stays usable (a second,
/// correct construction succeeds).
pub fn facet_failure_agreed(comm: Communicator) -> Outcome {
    let t0 = std::time::Instant::now();
    let rank = comm.rank();
    let size = comm.size();
    let fresh = || -> std::result::Result<Mesh, String> {
        let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
        distribute_rcb(&mut m)?;
        Ok(m)
    };
    // (a) the last rank's parent has a wrongly typed `mesh_to_mesh_facet`
    // (refused in the local build, before any collective); (b) its
    // `physical_names` have the wrong type (refused in the last local step,
    // after the exchanges)
    for case in ["m2f", "names"] {
        let mut bad = fresh()?;
        if rank == size as Rank - 1 {
            let wrong = || {
                akantu_mesh::MeshDataValues::Int(akantu_core::SharedElementTypeMapArray::new("bad"))
            };
            bad.data_mut().insert_elemental("physical_names", wrong());
            if case == "m2f" {
                bad.data_mut().insert_elemental(MESH_TO_MESH_FACET, wrong());
            }
        }
        let before = bad.state_fingerprint();
        let r = akantu_mesh::utils::init_mesh_facets(&mut bad);
        if size > 1 {
            ensure!(
                r.is_err(),
                "rank {rank} ({case}): a rank-specific failure must fail everywhere"
            );
            ensure!(
                bad.mesh_facets().is_none(),
                "rank {rank} ({case}): facet mesh installed"
            );
            ensure!(
                bad.state_fingerprint() == before,
                "rank {rank} ({case}): mesh changed"
            );
        }
    }
    // the communicator is still usable
    let mut m = fresh()?;
    let _ = ok(
        akantu_mesh::utils::init_mesh_facets(&mut m),
        "facets after the failure",
    )?;
    ensure!(m.mesh_facets().is_some(), "facet mesh missing");
    let dt = t0.elapsed();
    ensure!(
        dt < std::time::Duration::from_secs(10),
        "took {dt:?}: a rank waited for a timeout"
    );
    Ok(())
}

/// The facet mesh of a mesh shared with consumers: prepared collectively
/// before a mutation and installed by the transaction (the topology closure
/// cannot communicate; the plain `init_mesh_facets` of a transaction is
/// refused on more than one rank).
pub fn facet_transaction_install(comm: Communicator) -> Outcome {
    let mut m = read_on_root(&comm, "test_synchronizer/cube.msh", 3)?;
    distribute_rcb(&mut m)?;
    let shared = akantu_mesh::SharedMesh::new(m);
    let prepared = {
        let g = ok(shared.read(), "read")?;
        ok(
            akantu_mesh::facet_synchronizer::create_distributed_mesh_facets(
                &g,
                "mesh_facets",
                DistributedFacetsOptions::default(),
            ),
            "prepare",
        )?
    };
    let reference = prepared.mesh_facets.state_fingerprint();
    struct NoopHook;
    impl akantu_mesh::DistributedHook for NoopHook {
        fn prepare(
            &mut self,
            _: &Mesh,
            _: &[(akantu_mesh::MeshPart, akantu_mesh::MeshEvent)],
        ) -> akantu_core::Result<()> {
            Ok(())
        }
        fn apply(
            &mut self,
            _: &mut akantu_mesh::MeshTransaction,
            _: &[(akantu_mesh::MeshPart, akantu_mesh::MeshEvent)],
            _: &mut akantu_comm::phase::Phases,
        ) -> akantu_core::Result<()> {
            Ok(())
        }
    }
    if comm.size() > 1 {
        // the transaction variant refuses to communicate
        let r = shared.mutate(&mut [], |tx| tx.init_mesh_facets(), Some(&mut NoopHook));
        ensure!(
            r.is_err(),
            "init_mesh_facets inside a mutation must be refused"
        );
    }
    let mut slot = Some(prepared);
    ok(
        shared.mutate(
            &mut [],
            |tx| {
                tx.install_mesh_facets(
                    slot.take().ok_or_else(|| {
                        akantu_core::Error::Mesh("prepared facets used twice".into())
                    })?,
                )
            },
            Some(&mut NoopHook),
        ),
        "install",
    )?;
    let g = ok(shared.read(), "read")?;
    ensure!(
        g.mesh_facets().map(Mesh::state_fingerprint) == Some(reference),
        "installed facet mesh differs from the prepared one"
    );
    Ok(())
}

// ------------------------------------------------------- grid synchronizer

use akantu_core::spatial_grid::SpatialGrid;
use akantu_mesh::grid_synchronizer::{
    GridCells, GridSynchronizerOptions, create_grid_synchronizer, create_grid_synchronizer_shared,
};

/// Radius of a grid scenario (probe `run_grid`).
pub fn grid_radius(scenario: &str) -> Real {
    if scenario == "grid_tri_n8" { 0.2 } else { 0.3 }
}

/// The global mesh of a grid scenario, built on rank 0 of `comm` (or on
/// every rank with a serial communicator).
pub fn grid_mesh_on_root(comm: &Communicator, scenario: &str) -> std::result::Result<Mesh, String> {
    if scenario == "grid_tri_n8" {
        square_tri3_on_root(comm, 8)
    } else {
        cube_tet4_on_root(comm, 4)
    }
}

/// Upstream test: grid of the local barycentres, spacing `1.2 R`, centred
/// on the mesh's (global) box.
pub fn local_barycenter_grid(
    m: &Mesh,
    radius: Real,
) -> std::result::Result<SpatialGrid<Element>, String> {
    let dim = m.spatial_dimension();
    let center: Vec<Real> = m
        .lower_bounds()
        .iter()
        .zip(m.upper_bounds())
        .map(|(l, u)| 0.5 * (u + l))
        .collect();
    let spacing = vec![radius * 1.2; dim];
    let mut grid = ok(SpatialGrid::with_spacing(dim, &spacing, &center), "grid")?;
    for ty in m.element_types(TypeFilter::new().dim(dim)) {
        for e in 0..m.nb_element(ty, GhostType::NotGhost) {
            let el = Element::new(ty, e as Idx, GhostType::NotGhost);
            let b = ok(m.barycenter(el), "barycenter")?;
            ok(grid.insert(el, &b), "insert")?;
        }
    }
    Ok(grid)
}

/// Distributed mesh + grid + grid synchronizer of a scenario.
pub fn grid_scenario(
    comm: &Communicator,
    scenario: &str,
    options: GridSynchronizerOptions,
) -> std::result::Result<
    (
        Mesh,
        akantu_mesh::synchronizers::ElementSynchronizer,
        SpatialGrid<Element>,
    ),
    String,
> {
    let mut m = grid_mesh_on_root(comm, scenario)?;
    distribute_like_oracle(comm, &mut m, scenario)?;
    let grid = local_barycenter_grid(&m, grid_radius(scenario))?;
    let (sync, _events) = ok(
        create_grid_synchronizer(&mut m, &grid, "grid_synchronizer", options),
        "grid synchronizer",
    )?;
    Ok((m, sync, grid))
}

fn sorted_element_key(m: &Mesh, e: Element) -> std::result::Result<Vec<GlobalIdx>, String> {
    let mut k: Vec<GlobalIdx> = ok(m.element_connectivity(e), "conn")?
        .iter()
        .map(|&n| m.node_global_id(n))
        .collect();
    k.sort_unstable();
    Ok(k)
}

fn distance(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<Real>()
        .sqrt()
}

/// Port of the stale `test_grid_synchronizer.cc` +
/// `test_grid_synchronizer_check_neighbors.cc` (same intent, modern API,
/// generated meshes instead of `bar.msh`): after the grid synchronizer,
/// the neighbours within `R` of every local element found through the grid
/// (local and ghost barycentres inserted, 3^d neighbour cells) are exactly
/// the neighbours a brute-force search finds in the serial mesh (the
/// parallel pair list equals the serial one); the ghost barycentres sent
/// through the grid synchronizer equal the owners' (bitwise); new pure
/// ghost nodes carry their owner rank (UD-252 default).
pub fn test_grid_synchronizer(comm: Communicator) -> Outcome {
    for scenario in ["grid_tri_n8", "grid_cube_n4"] {
        let radius = grid_radius(scenario);
        // serial reference on every rank (global ids = serial node ids)
        let serial = grid_mesh_on_root(&Communicator::serial(), scenario)?;
        let dim = serial.spatial_dimension();
        let mut serial_bary: Vec<(Vec<GlobalIdx>, Vec<Real>)> = Vec::new();
        for ty in serial.element_types(TypeFilter::new().dim(dim)) {
            for e in 0..serial.nb_element(ty, GhostType::NotGhost) {
                let el = Element::new(ty, e as Idx, GhostType::NotGhost);
                serial_bary.push((
                    sorted_element_key(&serial, el)?,
                    ok(serial.barycenter(el), "barycenter")?,
                ));
            }
        }
        let n_ghost_before =
            |m: &Mesh| m.nb_element_filtered(TypeFilter::new().dim(dim).ghost(GhostType::Ghost));
        let mut m = grid_mesh_on_root(&comm, scenario)?;
        distribute_like_oracle(&comm, &mut m, scenario)?;
        let ghosts_before = n_ghost_before(&m);
        let mut grid = local_barycenter_grid(&m, radius)?;
        let (sync, _events) = ok(
            create_grid_synchronizer(
                &mut m,
                &grid,
                "grid_synchronizer",
                GridSynchronizerOptions::default(),
            ),
            "grid synchronizer",
        )?;
        ensure!(n_ghost_before(&m) >= ghosts_before, "ghost elements lost");
        // insert the ghost barycentres (upstream test, after the creation)
        for ty in m.element_types(TypeFilter::new().dim(dim).ghost(GhostType::Ghost)) {
            for e in 0..m.nb_element(ty, GhostType::Ghost) {
                let el = Element::new(ty, e as Idx, GhostType::Ghost);
                let b = ok(m.barycenter(el), "barycenter")?;
                ok(grid.insert(el, &b), "insert ghost")?;
            }
        }
        // pair list through the grid vs brute force on the serial mesh
        for ty in m.element_types(TypeFilter::new().dim(dim)) {
            for e in 0..m.nb_element(ty, GhostType::NotGhost) {
                let el = Element::new(ty, e as Idx, GhostType::NotGhost);
                let b = ok(m.barycenter(el), "barycenter")?;
                let mut found = BTreeSet::new();
                let center = ok(grid.cell_id(&b), "cell id")?;
                for c in center.neighbors() {
                    for &f in grid.cell(&c) {
                        let bf = ok(m.barycenter(f), "barycenter")?;
                        if distance(&b, &bf) <= radius {
                            found.insert(sorted_element_key(&m, f)?);
                        }
                    }
                }
                let brute: BTreeSet<Vec<GlobalIdx>> = serial_bary
                    .iter()
                    .filter(|(_, bf)| distance(&b, bf) <= radius)
                    .map(|(k, _)| k.clone())
                    .collect();
                ensure!(
                    found == brute,
                    "{scenario}: {el}: {} neighbours through the grid, {} by brute force",
                    found.len(),
                    brute.len()
                );
            }
        }
        // ghost barycentres through the grid synchronizer
        let bary = barycenters(&m)?;
        let nb_recv: usize = sync.communications.recv.values().map(Vec::len).sum();
        let mut acc = BarycenterAccessor {
            dim,
            bary: &bary,
            max_dist: 0.0,
            unpacked: 0,
        };
        ok(
            sync.synchronize(&mut acc, SynchronizationTag::Test),
            "grid synchronize",
        )?;
        ensure!(
            acc.max_dist == 0.0,
            "{scenario}: ghost barycenters differ by {}",
            acc.max_dist
        );
        ensure!(
            acc.unpacked == nb_recv,
            "{scenario}: unpacked {} of {nb_recv}",
            acc.unpacked
        );
        // the distribution's element synchronizer still works
        let es = m
            .synchronizers()
            .element
            .clone()
            .ok_or("no element synchronizer")?;
        let mut acc = BarycenterAccessor {
            dim,
            bary: &bary,
            max_dist: 0.0,
            unpacked: 0,
        };
        ok(
            es.synchronize(&mut acc, SynchronizationTag::Test),
            "element synchronize",
        )?;
        ensure!(
            acc.max_dist == 0.0,
            "{scenario}: element synchronizer after the grid"
        );
        // every pure ghost node has an owner that holds it as local/master
        let mine: Vec<i64> = (0..m.nb_nodes())
            .filter(|&n| m.node_flag(n as Idx).is_local_or_master())
            .map(|n| m.node_global_id(n as Idx))
            .collect();
        let all = ok(comm.all_gather_v(&mine), "gather")?;
        for n in 0..m.nb_nodes() {
            let n = n as Idx;
            let f = m.node_flag(n);
            if f.is_local_or_master() {
                continue;
            }
            let p = m.node_prank(n);
            ensure!(
                p >= 0 && p as usize != comm.rank() as usize,
                "{scenario}: node {n} without owner"
            );
            ensure!(
                all[p as usize].contains(&m.node_global_id(n)),
                "{scenario}: node {n} (global {}) not owned by its prank {p}",
                m.node_global_id(n)
            );
        }
    }
    Ok(())
}

/// Compare the mesh and grid synchronizer of a grid scenario with the
/// oracle (UD-252 compat: no owner rank on the new nodes).
pub fn grid_matches_oracle_of(scenario: &str, comm: &Communicator) -> Outcome {
    let mut m = grid_mesh_on_root(comm, scenario)?;
    distribute_like_oracle(comm, &mut m, scenario)?;
    grid_matches_oracle_with(scenario, comm, m, grid_radius(scenario))
}

/// The grid synchronizer on the distributed mesh `m` (grid of the local
/// barycentres, spacing `1.2 radius`) equals the oracle's `scenario`: grid
/// bounds and centre, the mesh after the new ghost layer, the grid schemes,
/// and the local and global boxes before and after (exact).
pub fn grid_matches_oracle_with(
    scenario: &str,
    comm: &Communicator,
    mut m: Mesh,
    radius: Real,
) -> Outcome {
    let fx = open_case(scenario, comm)?;
    compare_bbox_values(&bbox_values(&m), &fx, "mesh/bbox_before")?;
    let grid = local_barycenter_grid(&m, radius)?;
    let (sync, _events) = ok(
        create_grid_synchronizer(
            &mut m,
            &grid,
            "grid_synchronizer",
            GridSynchronizerOptions {
                upstream_node_pranks: true,
            },
        ),
        "grid synchronizer",
    )?;
    compare_bbox_values(&bbox_values(&m), &fx, "mesh/bbox_after")?;
    let p = &fx.manifest().parameters;
    let lf = |name: &str| {
        fx.load_f64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let mut bounds = grid.lower_bounds().to_vec();
    bounds.extend_from_slice(grid.upper_bounds());
    let want = lf("grid/bounds")?;
    ensure!(
        bounds.len() == want.len()
            && bounds
                .iter()
                .zip(&want)
                .all(|(a, b)| a.to_bits() == b.to_bits()),
        "grid bounds {bounds:?} vs oracle {want:?}"
    );
    let want = lf("grid/center")?;
    ensure!(
        grid.center()
            .iter()
            .zip(&want)
            .all(|(a, b)| a.to_bits() == b.to_bits()),
        "grid center {:?} vs oracle {want:?}",
        grid.center()
    );
    crate::par_common::compare_with_oracle(&m, &fx)?;
    let table: Vec<String> = p["type_table"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let keys_want: Vec<String> = p["grid_schemes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    let mut keys = Vec::new();
    for (dir, sch) in [
        ("send", &sync.communications.send),
        ("recv", &sync.communications.recv),
    ] {
        for (q, els) in sch {
            let key = format!("{dir}/{q}");
            let mut v = Vec::new();
            for &e in els {
                v.extend(encode(e, &table)?);
            }
            ensure!(
                v == li(&format!("grid_scheme/{key}"))?,
                "grid scheme {key} differs ({} entries)",
                els.len()
            );
            keys.push(key);
        }
    }
    ensure!(keys == keys_want, "grid schemes {keys:?} vs {keys_want:?}");
    Ok(())
}

pub fn grid_tri_n8_matches_oracle(comm: Communicator) -> Outcome {
    grid_matches_oracle_of("grid_tri_n8", &comm)
}

/// Case `v101_grid_wide`: Tri3 n = 8 in four stripes, R = 0.45 (a
/// neighbourhood wider than a rank's halo: elements from non-adjacent
/// ranks).
pub fn v101_grid_wide_matches_oracle(comm: Communicator) -> Outcome {
    let scenario = "v101_grid_wide";
    let mut m = square_tri3_on_root(&comm, 8)?;
    distribute_like_oracle(&comm, &mut m, scenario)?;
    grid_matches_oracle_with(scenario, &comm, m, 0.45)
}

/// Case `v101_grid_adv2d`: the grid synchronizer (R = 0.6) on the
/// mixed Quad4/Tri3 adv2d mesh (cell-rule partition, upstream's orphan
/// placement).
pub fn v101_grid_adv2d_matches_oracle(comm: Communicator) -> Outcome {
    let m = crate::par_common::adv2d_distributed(&comm, true)?;
    grid_matches_oracle_with("v101_grid_adv2d", &comm, m, 0.6)
}

pub fn grid_cube_n4_matches_oracle(comm: Communicator) -> Outcome {
    grid_matches_oracle_of("grid_cube_n4", &comm)
}

/// The grid synchronizer through a mesh mutation (consumers present)
/// produces the same mesh content and schemes as the set-up path (the
/// mutation recomputes the local bounding box; the global one is
/// unchanged).
pub fn grid_synchronizer_through_mutation(comm: Communicator) -> Outcome {
    // R = 0.2 adds nodes on every rank; R = 0.01 leaves some ranks without
    // new nodes or elements (identical event schedules nonetheless)
    for radius in [0.2, 0.01] {
        grid_through_mutation_with(&comm, radius)?;
    }
    Ok(())
}

fn grid_through_mutation_with(comm: &Communicator, radius: Real) -> Outcome {
    let comm = comm.clone();
    let scenario = "grid_tri_n8";
    let reference_of = || -> std::result::Result<_, String> {
        let mut m = grid_mesh_on_root(&comm, scenario)?;
        distribute_like_oracle(&comm, &mut m, scenario)?;
        let grid = local_barycenter_grid(&m, radius)?;
        let (sync, _) = ok(
            create_grid_synchronizer(
                &mut m,
                &grid,
                "grid_synchronizer",
                GridSynchronizerOptions::default(),
            ),
            "grid synchronizer",
        )?;
        Ok((m, sync))
    };
    let (reference, ref_sync) = reference_of()?;
    let mut m = grid_mesh_on_root(&comm, scenario)?;
    distribute_like_oracle(&comm, &mut m, scenario)?;
    let grid = local_barycenter_grid(&m, radius)?;
    let global_box = m.bbox().clone();
    let shared = akantu_mesh::SharedMesh::new(m);
    let (sync, committed) = ok(
        create_grid_synchronizer_shared(
            &shared,
            &mut [],
            &grid,
            "grid_synchronizer",
            GridSynchronizerOptions::default(),
        ),
        "grid synchronizer (mutation)",
    )?;
    let _ = committed;
    let g = ok(shared.read(), "read")?;
    ensure!(
        g.nodes().as_slice() == reference.nodes().as_slice(),
        "nodes differ"
    );
    ensure!(
        g.nodes_flags().as_slice() == reference.nodes_flags().as_slice(),
        "flags differ"
    );
    for n in 0..g.nb_nodes() {
        let n = n as Idx;
        ensure!(
            g.node_global_id(n) == reference.node_global_id(n),
            "global id of {n}"
        );
        ensure!(g.node_prank(n) == reference.node_prank(n), "prank of {n}");
    }
    for ghost in GhostType::ALL {
        for ty in reference.element_types(TypeFilter::new().ghost(ghost)) {
            ensure!(
                ok(g.connectivity(ty, ghost), "conn")?.as_slice()
                    == ok(reference.connectivity(ty, ghost), "conn")?.as_slice(),
                "connectivity {ty}:{ghost} differs"
            );
        }
        for ty in reference.element_types(TypeFilter::new().ghost(GhostType::Ghost)) {
            let a = g
                .ghosts_counters()
                .get(ty, GhostType::Ghost)
                .map(|a| a.as_slice().to_vec())
                .ok();
            let b = reference
                .ghosts_counters()
                .get(ty, GhostType::Ghost)
                .map(|a| a.as_slice().to_vec())
                .ok();
            ensure!(a == b, "ghost counters {ty} differ");
        }
    }
    ensure!(
        sync.communications.send == ref_sync.communications.send,
        "send schemes differ"
    );
    ensure!(
        sync.communications.recv == ref_sync.communications.recv,
        "recv schemes differ"
    );
    let (gn, rn) = (
        g.synchronizers().node.as_ref(),
        reference.synchronizers().node.as_ref(),
    );
    ensure!(
        gn.map(|s| &s.communications.send) == rn.map(|s| &s.communications.send)
            && gn.map(|s| &s.communications.recv) == rn.map(|s| &s.communications.recv),
        "node schemes differ (R = {radius})"
    );
    ensure!(g.bbox() == &global_box, "global box changed");
    // The set-up path installs the boxes upstream's
    // sendEvent<NewNodesEvent> computes; the mutation's stage 3 the same
    ensure!(
        bbox_values(&g) == bbox_values(&reference),
        "boxes differ between the mutation and the set-up path (R = {radius}): {:?} vs {:?}",
        bbox_values(&g),
        bbox_values(&reference)
    );
    ensure!(
        g.local_bbox() == &g.local_bounding_box(),
        "local box is not the box of the local nodes (R = {radius})"
    );
    Ok(())
}

/// A rank-specific failure of the grid synchronizer (a grid of another
/// dimension on the last rank; a ghost element in a sent cell, UD-253) is
/// an error on every rank, every mesh unchanged, communicator usable.
pub fn grid_failure_agreed(comm: Communicator) -> Outcome {
    let t0 = std::time::Instant::now();
    let rank = comm.rank();
    let size = comm.size();
    if size == 1 {
        return Ok(());
    }
    let scenario = "grid_tri_n8";
    for case in ["dimension", "ghost"] {
        let mut m = grid_mesh_on_root(&comm, scenario)?;
        distribute_like_oracle(&comm, &mut m, scenario)?;
        let mut grid = local_barycenter_grid(&m, grid_radius(scenario))?;
        if rank == size as Rank - 1 {
            if case == "dimension" {
                grid = ok(SpatialGrid::with_spacing(3, &[0.24; 3], &[0.5; 3]), "grid")?;
            } else {
                // a ghost element in a cell (upstream would send another
                // element's nodes)
                let ty = m
                    .element_types(TypeFilter::new().dim(2).ghost(GhostType::Ghost))
                    .next()
                    .ok_or("no ghost type")?;
                let el = Element::new(ty, 0, GhostType::Ghost);
                let b = ok(m.barycenter(el), "barycenter")?;
                ok(grid.insert(el, &b), "insert")?;
            }
        }
        let before = m.state_fingerprint();
        let r = create_grid_synchronizer(&mut m, &grid, "grid", GridSynchronizerOptions::default());
        ensure!(
            r.is_err(),
            "rank {rank} ({case}): a rank-specific failure must fail everywhere"
        );
        ensure!(
            m.state_fingerprint() == before,
            "rank {rank} ({case}): mesh changed"
        );
    }
    // the communicator is still usable
    let _ = grid_scenario(&comm, scenario, GridSynchronizerOptions::default())?;
    let dt = t0.elapsed();
    ensure!(
        dt < std::time::Duration::from_secs(10),
        "took {dt:?}: a rank waited for a timeout"
    );
    Ok(())
}

/// A [`GridCells`] delegating to `inner` whose `cell_index`
/// (`fail_cell_index`) or `elements_in_cell_range` (otherwise) fails: the
/// fallible paths of the grid synchronizer's send planning.
/// With the core grid these errors cannot be
/// reached from a valid grid (the intersection bounds are cell bounds of
/// inserted cells ± one spacing), so only such a wrapper reaches them.
struct FailingGridCells<'a> {
    inner: &'a dyn GridCells,
    fail_cell_index: bool,
}

/// Message of the injected [`FailingGridCells`] errors.
const INJECTED_GRID_FAILURE: &str = "injected GridCells failure";

impl GridCells for FailingGridCells<'_> {
    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
    fn lower_bounds(&self) -> &[Real] {
        self.inner.lower_bounds()
    }
    fn upper_bounds(&self) -> &[Real] {
        self.inner.upper_bounds()
    }
    fn spacing(&self) -> &[Real] {
        self.inner.spacing()
    }
    fn cell_index(&self, position: Real, direction: usize) -> akantu_core::Result<Idx> {
        if self.fail_cell_index {
            return Err(akantu_core::Error::Numerical(format!(
                "{INJECTED_GRID_FAILURE}: cell_index"
            )));
        }
        self.inner.cell_index(position, direction)
    }
    fn elements_in_cell_range(
        &self,
        first: &[Idx],
        last: &[Idx],
        visit: &mut dyn FnMut(Element),
    ) -> akantu_core::Result<()> {
        if !self.fail_cell_index {
            return Err(akantu_core::Error::Numerical(format!(
                "{INJECTED_GRID_FAILURE}: elements_in_cell_range"
            )));
        }
        self.inner.elements_in_cell_range(first, last, visit)
    }
}

/// A failing `GridCells::cell_index` or
/// `GridCells::elements_in_cell_range` on the last rank inside
/// `create_grid_synchronizer` (a mock `GridCells`) is an agreed error on
/// every rank — the failing rank reports its own error, every other rank
/// "failed on another rank" — every mesh is unchanged, no rank waits for a
/// timeout, and the communicator stays usable (a regular grid synchronizer
/// afterwards).
pub fn grid_cells_failure_agreed(comm: Communicator) -> Outcome {
    let t0 = std::time::Instant::now();
    let rank = comm.rank();
    let size = comm.size();
    if size == 1 {
        return Ok(());
    }
    let scenario = "grid_tri_n8";
    for fail_cell_index in [true, false] {
        let what = if fail_cell_index {
            "cell_index"
        } else {
            "elements_in_cell_range"
        };
        let mut m = grid_mesh_on_root(&comm, scenario)?;
        distribute_like_oracle(&comm, &mut m, scenario)?;
        let grid = local_barycenter_grid(&m, grid_radius(scenario))?;
        let failing = FailingGridCells {
            inner: &grid,
            fail_cell_index,
        };
        let cells: &dyn GridCells = if rank == size as Rank - 1 {
            &failing
        } else {
            &grid
        };
        let before = m.state_fingerprint();
        let r = create_grid_synchronizer(&mut m, cells, "grid", GridSynchronizerOptions::default());
        let Err(e) = r else {
            return Err(format!(
                "rank {rank} ({what}): a failure on rank {} must fail everywhere",
                size - 1
            ));
        };
        let e = e.to_string();
        if rank == size as Rank - 1 {
            ensure!(
                e.contains(INJECTED_GRID_FAILURE) && e.contains(what),
                "rank {rank} ({what}): not the injected error: {e}"
            );
        } else {
            ensure!(
                e.contains("failed on another rank"),
                "rank {rank} ({what}): not an agreed error: {e}"
            );
        }
        ensure!(
            m.state_fingerprint() == before,
            "rank {rank} ({what}): mesh changed"
        );
    }
    // the communicator is still usable
    let _ = grid_scenario(&comm, scenario, GridSynchronizerOptions::default())?;
    let dt = t0.elapsed();
    ensure!(
        dt < std::time::Duration::from_secs(10),
        "took {dt:?}: a rank waited for a timeout"
    );
    Ok(())
}

// ------------------------------------------- periodic node synchronizer

use akantu_core::enums::{ReduceOperation, SpatialDirection};
use akantu_mesh::periodic::{make_consistent_for_periodicity_real, make_periodic, periodic_pairs};
use akantu_mesh::periodic_synchronizer::periodic_sum_contributions_real;

/// The six `TestMeshPeriodic` types `(dimension, periodic directions)`.
pub const PERIODIC_TYPES: [(usize, usize); 6] = [(1, 1), (2, 1), (2, 2), (3, 1), (3, 2), (3, 3)];

pub fn periodic_scenario_name(dim: usize, pdim: usize) -> String {
    format!("periodic_{dim}d_{}", &"xyz"[..pdim])
}

/// Upstream `TestMeshPeriodic::SetUp`: read line/square/cube.msh on rank 0,
/// nodal data `shared` = number of periodic groups holding the node,
/// distribute (the oracle's partition at 1/2/4 ranks), then `makePeriodic`
/// for every periodic direction **on the distributed mesh**.
pub fn periodic_mesh(
    comm: &Communicator,
    dim: usize,
    pdim: usize,
) -> std::result::Result<Mesh, String> {
    periodic_mesh_with_box(comm, dim, pdim).map(|(m, _)| m)
}

/// [`periodic_mesh`] and the mesh's boxes before `makePeriodic`
/// ([`bbox_values`]).
pub fn periodic_mesh_with_box(
    comm: &Communicator,
    dim: usize,
    pdim: usize,
) -> std::result::Result<(Mesh, Vec<Real>), String> {
    let name = ["line.msh", "square.msh", "cube.msh"][dim - 1];
    let mut m = read_on_root(
        comm,
        &format!("test_mesh_utils/test_mesh_periodic/{name}"),
        dim,
    )?;
    if comm.rank() == 0 {
        let groups = [
            "x-minus", "x-plus", "y-minus", "y-plus", "z-minus", "z-plus",
        ];
        let mut shared = akantu_core::Array::filled(m.nb_nodes(), 1, 0.0);
        for g in &groups[..2 * pdim] {
            let nodes = ok(m.groups().element_group(g), "group")?.nodes().to_vec();
            for n in nodes {
                shared.as_mut_slice()[n as usize] += 1.0;
            }
        }
        m.data_mut()
            .insert_nodal("shared", akantu_mesh::NodalDataValues::Real(shared));
    }
    distribute_like_oracle(comm, &mut m, &periodic_scenario_name(dim, pdim))?;
    let before = bbox_values(&m);
    for s in 0..pdim {
        let dir = [
            SpatialDirection::X,
            SpatialDirection::Y,
            SpatialDirection::Z,
        ][s];
        let _ = ok(make_periodic(&mut m, dir, None), "makePeriodic")?;
    }
    Ok((m, before))
}

/// Non-dyadic nodal data of the probe's `record_periodic_nondyadic`:
/// `(1 / (3 + gid), 0.1 (gid + 1))`.
pub fn periodic_nondyadic_input(m: &Mesh) -> akantu_core::Array<Real> {
    let mut a = akantu_core::Array::filled(m.nb_nodes(), 2, 0.0);
    for n in 0..m.nb_nodes() {
        let g = m.node_global_id(n as Idx);
        let t = a.tuple_mut(n);
        t[0] = 1. / (3. + g as Real);
        t[1] = 0.1 * ((g + 1) as Real);
    }
    a
}

/// The comparisons every periodic case shares, after `makePeriodic`: boxes
/// before (`before`) and after, the mesh (nodes incl. the new pure
/// ghosts, flags, owners, node schemes after the filter), sorted periodic
/// pairs, periodic synchronizer schemes, the per-master slave order of the
/// oracle's `unordered_multimap` against
/// [`Mesh::periodic_slaves_upstream_order`] (D-061 / UD-654), and
/// `makeConsistentForPeriodicity` `_id` / `_sum` of non-dyadic data,
/// and the per-rank contributions and run-dependent
/// entries of that `_sum` ([`compare_run_dependent_sum`]).
/// Everything exact.
pub fn periodic_common_matches_oracle(
    scenario: &str,
    m: &Mesh,
    before: &[Real],
    fx: &akantu_validation::Fixture,
) -> Outcome {
    compare_bbox_values(before, fx, "mesh/bbox_before").map_err(|e| format!("{scenario}: {e}"))?;
    compare_bbox_values(&bbox_values(m), fx, "mesh/bbox_after")
        .map_err(|e| format!("{scenario}: {e}"))?;
    crate::par_common::compare_with_oracle(m, fx).map_err(|e| format!("{scenario}: {e}"))?;
    let p = &fx.manifest().parameters;
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let lf = |name: &str| {
        fx.load_f64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    ensure!(
        p["is_periodic"].as_bool() == Some(m.is_periodic()),
        "{scenario}: periodic flag"
    );
    let pairs: Vec<i64> = periodic_pairs(m)
        .into_iter()
        .flat_map(|(a, b)| [i64::from(a), i64::from(b)])
        .collect();
    ensure!(
        pairs == li("periodic/pairs")?,
        "{scenario}: periodic pairs differ: {pairs:?}"
    );
    let ps = m
        .synchronizers()
        .periodic_node
        .as_ref()
        .ok_or("no periodic synchronizer")?;
    let mut keys = Vec::new();
    for (dir, sch) in [
        ("send", &ps.communications.send),
        ("recv", &ps.communications.recv),
    ] {
        for (q, nodes) in sch {
            let key = format!("{dir}/{q}");
            let have: Vec<i64> = nodes.iter().map(|&v| i64::from(v)).collect();
            ensure!(
                have == li(&format!("periodic_scheme/{key}"))?,
                "{scenario}: periodic scheme {key} differs: {have:?}"
            );
            keys.push(key);
        }
    }
    let want: Vec<String> = p["periodic_schemes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    ensure!(
        keys == want,
        "{scenario}: periodic schemes {keys:?} vs {want:?}"
    );
    // per-master slave order of the oracle's multimap (equal keys adjacent)
    let raw = li("periodic/multimap_order")?;
    let mut groups: Vec<(i64, Vec<i64>)> = Vec::new();
    for pair in raw.as_chunks::<2>().0 {
        match groups.last_mut() {
            Some((master, slaves)) if *master == pair[0] => slaves.push(pair[1]),
            _ => groups.push((pair[0], vec![pair[1]])),
        }
    }
    let masters: BTreeSet<Idx> = periodic_pairs(m).into_iter().map(|(ms, _)| ms).collect();
    ensure!(
        groups.len() == masters.len(),
        "{scenario}: {} slave groups in the oracle's multimap order, {} masters here",
        groups.len(),
        masters.len()
    );
    for (master, slaves) in &groups {
        let have: Vec<i64> = m
            .periodic_slaves_upstream_order(*master as Idx)
            .map(i64::from)
            .collect();
        ensure!(
            &have == slaves,
            "{scenario}: slaves of master {master} in upstream order {have:?} vs oracle {slaves:?}"
        );
    }
    let exact = |a: &[Real], b: &[f64]| {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
    };
    let input = periodic_nondyadic_input(m);
    ensure!(
        exact(input.as_slice(), &lf("periodic/nondyadic_input")?),
        "{scenario}: non-dyadic input"
    );
    // version-3 records: nodes whose master has >= 2 remote contributors
    // (a structural superset of the run-dependent entries); the oracle's
    // mask must equal the one derived here from the port's periodic
    // schemes; those entries are compared by `compare_run_dependent_sum`
    let mask = order_dependent_sum_nodes(m)?;
    ensure!(
        mask == li("periodic/nondyadic_sum_order_dependent")?,
        "{scenario}: order-dependent nodes {mask:?} differ from the oracle's"
    );
    for (op, name) in [
        (ReduceOperation::Id, "periodic/nondyadic_after_id"),
        (ReduceOperation::Sum, "periodic/nondyadic_after_sum"),
    ] {
        let mut a = input.clone();
        ok(make_consistent_for_periodicity_real(m, &mut a, op), name)?;
        let want = lf(name)?;
        ensure!(
            want.len() == a.as_slice().len(),
            "{scenario}: {name} length"
        );
        let masked = |k: usize| op == ReduceOperation::Sum && mask[k / 2] != 0;
        let diffs: Vec<String> = a
            .as_slice()
            .iter()
            .zip(&want)
            .enumerate()
            .filter(|&(k, (x, y))| {
                if masked(k) {
                    y.to_bits() != 0f64.to_bits()
                } else {
                    x.to_bits() != y.to_bits()
                }
            })
            .take(6)
            .map(|(k, (x, y))| format!("[{}:{}] {x:e} vs {y:e}", k / 2, k % 2))
            .collect();
        ensure!(
            diffs.is_empty(),
            "{scenario}: {name} differs: {}",
            diffs.join(", ")
        );
    }
    compare_run_dependent_sum(scenario, m, fx, &input)
}

/// The rank-order view of one periodic master's distributed `_sum`: the
/// owner, the owner's own contribution and the remote contributions by
/// rank (ascending: the port's fold order, D-039).
struct MasterContributions {
    owner: Rank,
    own: Vec<Real>,
    remote: BTreeMap<Rank, Vec<Real>>,
}

impl MasterContributions {
    /// `own + c_q1 + c_q2 + …` in the given order of the remote ranks.
    fn fold(&self, order: &[Rank], c: usize) -> Real {
        let mut s = self.own[c];
        for q in order {
            s += self.remote[q][c];
        }
        s
    }

    /// The rank-order fold (the port's order, D-039).
    fn rank_order_fold(&self, c: usize) -> Real {
        let order: Vec<Rank> = self.remote.keys().copied().collect();
        self.fold(&order, c)
    }

    /// Distinct results (bit patterns) over every arrival order of the
    /// remote contributions (upstream adds them in `waitAny` order).
    fn possible_results(&self, c: usize) -> BTreeSet<u64> {
        let mut order: Vec<Rank> = self.remote.keys().copied().collect();
        let mut out = BTreeSet::new();
        permutations(&mut order, 0, &mut |o| {
            out.insert(self.fold(o, c).to_bits());
        });
        out
    }
}

/// Every permutation of `v[k..]` (Heap-free recursive swap; `v.len()` is at
/// most `P - 1` remote ranks).
fn permutations(v: &mut [Rank], k: usize, f: &mut dyn FnMut(&[Rank])) {
    if k + 1 >= v.len() {
        f(v);
        return;
    }
    for i in k..v.len() {
        v.swap(k, i);
        permutations(v, k + 1, f);
        v.swap(k, i);
    }
}

/// The contributions `values` (one `nbc`-tuple per local node) of every
/// periodic master with remote contributors, keyed by global id, gathered
/// from every rank through the **port's** periodic schemes (slave reduction:
/// the owner receives on its send schemes, every other rank sends its
/// receive-scheme entries). Collective. Errors if the schemes are not
/// symmetric.
fn gather_master_contributions(
    m: &Mesh,
    values: &[Real],
    nbc: usize,
) -> std::result::Result<BTreeMap<i64, MasterContributions>, String> {
    let ps = m
        .synchronizers()
        .periodic_node
        .as_ref()
        .ok_or("no periodic synchronizer")?;
    let me = i64::from(m.communicator().rank());
    let (mut keys, mut vals): (Vec<i64>, Vec<Real>) = (Vec::new(), Vec::new());
    for (kind, schemes) in [
        (0i64, &ps.communications.send),
        (1, &ps.communications.recv),
    ] {
        for (&other, nodes) in schemes {
            for &n in nodes {
                keys.extend([kind, m.node_global_id(n), me, i64::from(other)]);
                let k = n as usize * nbc;
                let t = values
                    .get(k..k + nbc)
                    .ok_or_else(|| format!("node {n} outside the contributions"))?;
                vals.extend_from_slice(t);
            }
        }
    }
    let keys = ok(m.communicator().all_gather_v(&keys), "gather keys")?;
    let vals = ok(m.communicator().all_gather_v(&vals), "gather values")?;
    let mut out: BTreeMap<i64, MasterContributions> = BTreeMap::new();
    let mut expected: BTreeMap<i64, BTreeSet<Rank>> = BTreeMap::new();
    let mut sent_to: BTreeMap<i64, BTreeMap<Rank, Rank>> = BTreeMap::new();
    for (k, v) in keys.iter().zip(&vals) {
        ensure!(k.len() / 4 * nbc == v.len(), "gathered rows");
        for (row, x) in k.as_chunks::<4>().0.iter().zip(v.chunks_exact(nbc)) {
            let [kind, gid, rank, other] = *row;
            let (rank, other) = (rank as Rank, other as Rank);
            let e = out.entry(gid).or_insert_with(|| MasterContributions {
                owner: -1,
                own: Vec::new(),
                remote: BTreeMap::new(),
            });
            if kind == 0 {
                ensure!(
                    (e.owner == -1 || e.owner == rank)
                        && (e.own.is_empty()
                            || e.own.iter().zip(x).all(|(a, b)| a.to_bits() == b.to_bits())),
                    "gid {gid}: inconsistent owner rows"
                );
                e.owner = rank;
                e.own = x.to_vec();
                expected.entry(gid).or_default().insert(other);
            } else {
                ensure!(
                    e.remote.insert(rank, x.to_vec()).is_none(),
                    "gid {gid}: two contributions of rank {rank}"
                );
                sent_to.entry(gid).or_default().insert(rank, other);
            }
        }
    }
    for (gid, e) in &out {
        let senders: BTreeSet<Rank> = e.remote.keys().copied().collect();
        ensure!(
            e.owner >= 0
                && expected.get(gid) == Some(&senders)
                && sent_to
                    .get(gid)
                    .is_some_and(|t| t.values().all(|&o| o == e.owner)),
            "gid {gid}: periodic schemes not symmetric (owner {}, expected {:?}, senders {senders:?})",
            e.owner,
            expected.get(gid)
        );
    }
    Ok(out)
}

/// Version-4 records of the non-dyadic periodic `_sum`
/// (D-039 amendment), every comparison exact:
/// 1. the port's per-rank contributions
///    ([`periodic_sum_contributions_real`]) equal the oracle's
///    (`periodic/nondyadic_sum_contributions`) bitwise;
/// 2. the run-dependent entries derived here from the port's contributions
///    and periodic schemes (≥ 2 distinct results over the arrival orders of
///    a master's remote contributions) equal the oracle's mask
///    (`periodic/nondyadic_sum_run_dependent`, per node and component);
/// 3. every entry of the port's `_sum` equals the rank-order model of the
///    **recorded** contributions: the rank-order fold of its master's
///    contributions (a node takes the value of its local master, or of its
///    own copy of a master), or its own contribution where no remote rank
///    contributes — at masked entries this is the D-039 check; at the
///    other entries the oracle's recorded value
///    (`periodic/nondyadic_after_sum_deterministic`) must equal the same
///    model (consistency of the recorded contributions) and the port's
///    value bitwise; masked entries are recorded as `+0.0`.
///
/// Collective.
pub fn compare_run_dependent_sum(
    scenario: &str,
    m: &Mesh,
    fx: &akantu_validation::Fixture,
    input: &akantu_core::Array<Real>,
) -> Outcome {
    let li = |name: &str| {
        fx.load_i64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{scenario}: {name}: {e}"))
    };
    let lf = |name: &str| {
        fx.load_f64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{scenario}: {name}: {e}"))
    };
    let nbc = input.nb_component();
    let nb = m.nb_nodes();
    // 1. contributions
    let mut contrib = input.clone();
    ok(
        periodic_sum_contributions_real(m, &mut contrib),
        "contributions",
    )?;
    let recorded = lf("periodic/nondyadic_sum_contributions")?;
    ensure!(
        recorded.len() == nb * nbc,
        "{scenario}: {} recorded contributions for {nb} nodes",
        recorded.len()
    );
    let diffs: Vec<String> = contrib
        .as_slice()
        .iter()
        .zip(&recorded)
        .enumerate()
        .filter(|(_, (x, y))| x.to_bits() != y.to_bits())
        .take(6)
        .map(|(k, (x, y))| format!("[{}:{}] {x:e} vs {y:e}", k / nbc, k % nbc))
        .collect();
    ensure!(
        diffs.is_empty(),
        "{scenario}: contributions differ from the oracle's: {}",
        diffs.join(", ")
    );
    // the source of every node's value: its local master, else itself
    let mut source: Vec<usize> = (0..nb).collect();
    for (ms, sl) in periodic_pairs(m) {
        source[sl as usize] = ms as usize;
    }
    // 2. run-dependent entries derived from the port's own contributions
    let port_masters = gather_master_contributions(m, contrib.as_slice(), nbc)?;
    let mut mask = vec![0i64; nb * nbc];
    for n in 0..nb {
        for c in 0..nbc {
            let dependent = |node: usize| {
                port_masters
                    .get(&m.node_global_id(node as Idx))
                    .is_some_and(|e| e.possible_results(c).len() > 1)
            };
            if dependent(n) || dependent(source[n]) {
                mask[n * nbc + c] = 1;
            }
        }
    }
    let want_mask = li("periodic/nondyadic_sum_run_dependent")?;
    ensure!(
        mask == want_mask,
        "{scenario}: run-dependent entries {:?} differ from the oracle's {:?}",
        mask.iter()
            .enumerate()
            .filter(|e| *e.1 != 0)
            .map(|e| e.0)
            .collect::<Vec<_>>(),
        want_mask
            .iter()
            .enumerate()
            .filter(|e| *e.1 != 0)
            .map(|e| e.0)
            .collect::<Vec<_>>()
    );
    // 3. the port's `_sum` against the rank-order model of the recorded
    // contributions; the oracle's deterministic entries against the same
    let rec_masters = gather_master_contributions(m, &recorded, nbc)?;
    let mut sum = input.clone();
    ok(
        make_consistent_for_periodicity_real(m, &mut sum, ReduceOperation::Sum),
        "sum",
    )?;
    let det = lf("periodic/nondyadic_after_sum_deterministic")?;
    ensure!(
        det.len() == nb * nbc,
        "{scenario}: deterministic sum length"
    );
    let mut errors = Vec::new();
    for (n, &s) in source.iter().enumerate() {
        let master = rec_masters.get(&m.node_global_id(s as Idx));
        for c in 0..nbc {
            let k = n * nbc + c;
            let model = master.map_or(recorded[s * nbc + c], |e| e.rank_order_fold(c));
            let port = sum.as_slice()[k];
            if port.to_bits() != model.to_bits() {
                errors.push(format!(
                    "[{n}:{c}] port {port:e} vs rank-order fold of the recorded contributions \
                     {model:e}{}",
                    if mask[k] != 0 {
                        " (run-dependent entry)"
                    } else {
                        ""
                    }
                ));
            }
            if mask[k] != 0 {
                if det[k].to_bits() != 0f64.to_bits() {
                    errors.push(format!("[{n}:{c}] masked entry recorded as {:e}", det[k]));
                }
            } else if det[k].to_bits() != model.to_bits() {
                errors.push(format!(
                    "[{n}:{c}] oracle {:e} vs rank-order fold of its contributions {model:e}",
                    det[k]
                ));
            }
        }
    }
    ensure!(
        errors.is_empty(),
        "{scenario}: non-dyadic _sum: {} difference(s): {}",
        errors.len(),
        errors
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .join("; ")
    );
    Ok(())
}

/// One flag per local node: `1` if its periodic `_sum` comes from a master
/// whose owner accumulates contributions from two or more remote ranks
/// (the master is in the periodic send schemes toward two or more ranks):
/// upstream adds them in arrival order (`waitAny`), the port in rank order
/// (D-039). The probe's version-3 mask `periodic/nondyadic_sum_order_dependent`
/// is this structural rule, a superset of the run-dependent entries;
/// the exact entries and the port's values there are checked
/// by [`compare_run_dependent_sum`]. Collective.
pub fn order_dependent_sum_nodes(m: &Mesh) -> std::result::Result<Vec<i64>, String> {
    let ps = m
        .synchronizers()
        .periodic_node
        .as_ref()
        .ok_or("no periodic synchronizer")?;
    let mut count: BTreeMap<Idx, usize> = BTreeMap::new();
    for nodes in ps.communications.send.values() {
        for &n in nodes {
            *count.entry(n).or_default() += 1;
        }
    }
    let mine: Vec<i64> = count
        .iter()
        .filter(|(_, c)| **c >= 2)
        .map(|(&n, _)| m.node_global_id(n))
        .collect();
    let all = ok(m.communicator().all_gather_v(&mine), "gather")?;
    let dependent: BTreeSet<i64> = all.into_iter().flatten().collect();
    let mut mask = vec![0i64; m.nb_nodes()];
    for (n, f) in mask.iter_mut().enumerate() {
        if dependent.contains(&m.node_global_id(n as Idx)) {
            *f = 1;
        }
    }
    for (ms, sl) in periodic_pairs(m) {
        if dependent.contains(&m.node_global_id(ms)) {
            mask[sl as usize] = 1;
        }
    }
    Ok(mask)
}

fn shared_of(m: &Mesh) -> std::result::Result<Vec<Real>, String> {
    match ok(m.data().nodal("shared"), "shared")? {
        akantu_mesh::NodalDataValues::Real(a) => Ok(a.as_slice().to_vec()),
        _ => Err("shared is not Real".into()),
    }
}

/// Upstream `TestMeshPeriodic.MakeConsistentForPeriodicityIdentity` input.
pub fn periodic_identity_input(m: &Mesh) -> akantu_core::Array<Real> {
    let mut a = akantu_core::Array::filled(m.nb_nodes(), 1, 10.0);
    for n in 0..m.nb_nodes() {
        if m.node_flag(n as Idx).is_periodic_slave() {
            a.as_mut_slice()[n] = -1.0;
        }
    }
    a
}

/// Upstream `TestMeshPeriodic.MakeConsistentForPeriodicitySum` input.
pub fn periodic_sum_input(m: &Mesh) -> std::result::Result<akantu_core::Array<Real>, String> {
    let shared = shared_of(m)?;
    let mut a = akantu_core::Array::filled(m.nb_nodes(), 1, 100.0);
    let masters: BTreeSet<Idx> = periodic_pairs(m).into_iter().map(|(ms, _)| ms).collect();
    for master in masters {
        if m.node_flag(master).is_local_or_master() {
            a.as_mut_slice()[master as usize] = 100.0 / 2f64.powf(shared[master as usize]);
        }
    }
    Ok(a)
}

/// Port of `test_mesh_periodic.cc` (`SimpleMakePeriodic`,
/// `MakeConsistentForPeriodicityIdentity`, `MakeConsistentForPeriodicitySum`)
/// on every type, distributed: the mesh is periodic; `_id` makes every
/// value 10; `_id` then `_sum` makes every local-or-master value 100
/// (upstream `EXPECT_NEAR(…, 1e-7)`; exact here — dyadic data). Plus the
/// global consistency of the periodic relation: every rank that holds a
/// periodic slave maps it to the same global master, and masters are never
/// slaves.
pub fn test_mesh_periodic(comm: Communicator) -> Outcome {
    for (dim, pdim) in PERIODIC_TYPES {
        let m = periodic_mesh(&comm, dim, pdim)?;
        let tag = periodic_scenario_name(dim, pdim);
        ensure!(m.is_periodic(), "{tag}: not periodic");
        let mut id = periodic_identity_input(&m);
        ok(
            make_consistent_for_periodicity_real(&m, &mut id, ReduceOperation::Id),
            "identity",
        )?;
        for (n, v) in id.as_slice().iter().enumerate() {
            ensure!(
                *v == 10.0,
                "{tag}: identity node {n} (flag {:#x}) = {v}",
                m.node_flag(n as Idx).bits()
            );
        }
        let mut sum = periodic_sum_input(&m)?;
        ok(
            make_consistent_for_periodicity_real(&m, &mut sum, ReduceOperation::Id),
            "sum id",
        )?;
        ok(
            make_consistent_for_periodicity_real(&m, &mut sum, ReduceOperation::Sum),
            "sum",
        )?;
        for (n, v) in sum.as_slice().iter().enumerate() {
            if m.node_flag(n as Idx).is_local_or_master() {
                ensure!(
                    *v == 100.0,
                    "{tag}: sum node {n} (flag {:#x}) = {v}",
                    m.node_flag(n as Idx).bits()
                );
            }
        }
        // global consistency of the periodic relation
        let mut mine = Vec::new();
        for (ms, s) in periodic_pairs(&m) {
            mine.push(m.node_global_id(s));
            mine.push(m.node_global_id(ms));
        }
        let all = ok(comm.all_gather_v(&mine), "gather")?;
        let mut master_of: BTreeMap<i64, i64> = BTreeMap::new();
        for v in &all {
            for p in v.as_chunks::<2>().0 {
                if let Some(prev) = master_of.insert(p[0], p[1]) {
                    ensure!(
                        prev == p[1],
                        "{tag}: global slave {} has masters {prev} and {}",
                        p[0],
                        p[1]
                    );
                }
            }
        }
        for (s, ms) in &master_of {
            ensure!(
                !master_of.contains_key(ms),
                "{tag}: master {ms} of {s} is a slave"
            );
        }
    }
    Ok(())
}

/// Compare a periodic scenario with the oracle: the mesh after
/// `makePeriodic` (nodes incl. the new pure ghosts, flags, owners, node
/// schemes after the filter), sorted periodic pairs, periodic synchronizer
/// schemes, and the `makeConsistentForPeriodicity` arrays (exact).
pub fn periodic_matches_oracle_of(dim: usize, pdim: usize, comm: &Communicator) -> Outcome {
    let scenario = periodic_scenario_name(dim, pdim);
    let fx = open_case(&scenario, comm)?;
    let (m, before) = periodic_mesh_with_box(comm, dim, pdim)?;
    periodic_common_matches_oracle(&scenario, &m, &before, &fx)?;
    let lf = |name: &str| {
        fx.load_f64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let exact = |a: &[Real], b: &[f64]| {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
    };
    let mut id = periodic_identity_input(&m);
    ensure!(
        exact(id.as_slice(), &lf("periodic/identity_input")?),
        "{scenario}: identity input"
    );
    ok(
        make_consistent_for_periodicity_real(&m, &mut id, ReduceOperation::Id),
        "identity",
    )?;
    ensure!(
        exact(id.as_slice(), &lf("periodic/identity_output")?),
        "{scenario}: identity output {:?}",
        id.as_slice()
    );
    let mut sum = periodic_sum_input(&m)?;
    ensure!(
        exact(sum.as_slice(), &lf("periodic/sum_input")?),
        "{scenario}: sum input"
    );
    ok(
        make_consistent_for_periodicity_real(&m, &mut sum, ReduceOperation::Id),
        "sum id",
    )?;
    ensure!(
        exact(sum.as_slice(), &lf("periodic/sum_after_id")?),
        "{scenario}: sum after id {:?}",
        sum.as_slice()
    );
    ok(
        make_consistent_for_periodicity_real(&m, &mut sum, ReduceOperation::Sum),
        "sum",
    )?;
    ensure!(
        exact(sum.as_slice(), &lf("periodic/sum_output")?),
        "{scenario}: sum output {:?}",
        sum.as_slice()
    );
    Ok(())
}

/// The TestMeshPeriodic cases ([`PERIODIC_TYPES`]) the probe recorded at
/// the rank count of `comm` (all six at 1, 2 and 4 ranks; `periodic_3d_xyz`
/// also at 8); an error if none is recorded there.
pub fn periodic_matches_oracle(comm: Communicator) -> Outcome {
    let mut compared = 0usize;
    for (dim, pdim) in PERIODIC_TYPES {
        let name = periodic_scenario_name(dim, pdim);
        if crate::par_common::oracle_rank_counts(&name).contains(&comm.size()) {
            periodic_matches_oracle_of(dim, pdim, &comm)?;
            compared += 1;
        }
    }
    if compared == 0 {
        return Err(format!(
            "no periodic oracle case recorded at {} ranks",
            comm.size()
        ));
    }
    Ok(())
}

/// Case `v101_periodic_<dim>d_<dirs>`: `makePeriodic` in the first
/// `ndir` directions on a structured mesh (`n` cells per direction) with a
/// parity partition (every periodic pair crosses a rank boundary), then
/// the shared periodic comparisons (boxes, pairs, schemes, multimap order,
/// non-dyadic `_id` / `_sum`).
pub fn v101_periodic_matches_oracle_of(
    dim: usize,
    ndir: usize,
    n: usize,
    comm: &Communicator,
) -> Outcome {
    let scenario = format!("v101_periodic_{dim}d_{}", &"xyz"[..ndir]);
    let fx = open_case(&scenario, comm)?;
    let mut m = v101_mesh_on_root(comm, dim, n)?;
    distribute_like_oracle(comm, &mut m, &scenario)?;
    let before = bbox_values(&m);
    for dir in &[
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ][..ndir]
    {
        let _ = ok(make_periodic(&mut m, *dir, None), "makePeriodic")?;
    }
    periodic_common_matches_oracle(&scenario, &m, &before, &fx)
}

pub fn v101_periodic_matches_oracle(comm: Communicator) -> Outcome {
    for (dim, ndir, n) in V101_PERIODIC {
        v101_periodic_matches_oracle_of(dim, ndir, n, &comm)?;
    }
    Ok(())
}

/// The periodic scenarios `(dimension, directions, cells)` of the `v101_*` fixtures.
pub const V101_PERIODIC: [(usize, usize, usize); 3] = [(2, 1, 4), (2, 2, 4), (3, 3, 2)];

/// A rank-specific failure of the distributed `makePeriodic` (a node list
/// with an out-of-range node on the last rank) is an error on every rank,
/// every mesh unchanged; the communicator stays usable.
pub fn periodic_failure_agreed(comm: Communicator) -> Outcome {
    let t0 = std::time::Instant::now();
    let rank = comm.rank();
    let size = comm.size();
    let mut m = read_on_root(&comm, "test_mesh_utils/test_mesh_periodic/square.msh", 2)?;
    distribute_rcb(&mut m)?;
    let before = m.state_fingerprint();
    let bad = if rank == size as Rank - 1 {
        vec![m.nb_nodes() as Idx + 5]
    } else {
        Vec::new()
    };
    let r = akantu_mesh::periodic::make_periodic_lists(&mut m, SpatialDirection::X, &bad, &[]);
    if size > 1 {
        ensure!(
            r.is_err(),
            "rank {rank}: a rank-specific failure must fail everywhere"
        );
        ensure!(m.state_fingerprint() == before, "rank {rank}: mesh changed");
    }
    let _ = ok(
        make_periodic(&mut m, SpatialDirection::X, None),
        "makePeriodic after the failure",
    )?;
    ensure!(m.is_periodic(), "not periodic");
    let dt = t0.elapsed();
    ensure!(
        dt < std::time::Duration::from_secs(10),
        "took {dt:?}: a rank waited for a timeout"
    );
    Ok(())
}

// ---------------------------------------------- UD-251 (mixed facet types)

/// A hexahedron next to two prisms (probe `scenario_facets_hex_prism`):
/// prisms on rank 0, the hexahedron on the last rank, so the ranks' local
/// facet types differ ({triangle, quadrangle} vs {quadrangle}). Upstream's
/// per-type synchronous announcements deadlock here (UD-251); the port's
/// one-message-per-rank announcement builds consistent schemes: every ghost
/// facet is received from its owner and the ghost barycentres equal the
/// owners'.
pub fn ud251_mixed_facet_types(comm: Communicator) -> Outcome {
    let mut m = ok(Mesh::with_communicator(3, "mesh", comm.clone()), "mesh")?;
    let rank = comm.rank();
    let size = comm.size();
    if rank == 0 {
        {
            let mut a = akantu_mesh::MeshAccessor::new(&mut m);
            for k in 0..=1 {
                for j in 0..=1 {
                    for i in 0..=2 {
                        ok(
                            a.push_node(&[Real::from(i), Real::from(j), Real::from(k)]),
                            "node",
                        )?;
                    }
                }
            }
            let id = |i: Idx, j: Idx, k: Idx| (k * 2 + j) * 3 + i;
            let mut p = a.connectivity_mut(ElementType::Pentahedron6, GhostType::NotGhost);
            p.push_tuple(&[
                id(0, 0, 0),
                id(1, 0, 0),
                id(1, 1, 0),
                id(0, 0, 1),
                id(1, 0, 1),
                id(1, 1, 1),
            ]);
            p.push_tuple(&[
                id(0, 0, 0),
                id(1, 1, 0),
                id(0, 1, 0),
                id(0, 0, 1),
                id(1, 1, 1),
                id(0, 1, 1),
            ]);
            drop(p);
            let mut h = a.connectivity_mut(ElementType::Hexahedron8, GhostType::NotGhost);
            h.push_tuple(&[
                id(1, 0, 0),
                id(2, 0, 0),
                id(2, 1, 0),
                id(1, 1, 0),
                id(1, 0, 1),
                id(2, 0, 1),
                id(2, 1, 1),
                id(1, 1, 1),
            ]);
        }
        ok(m.compute_bounding_box(), "bbox")?;
    }
    if size > 1 {
        // types in enum order: Pentahedron6 then Hexahedron8 (linearised)
        let parts = if rank == 0 {
            vec![0, 0, (size - 1) as Idx]
        } else {
            Vec::new()
        };
        let mut ordered = parts;
        if rank == 0 {
            let types: Vec<ElementType> = m.element_types(TypeFilter::new().dim(3)).collect();
            ensure!(
                types == vec![ElementType::Hexahedron8, ElementType::Pentahedron6]
                    || types == vec![ElementType::Pentahedron6, ElementType::Hexahedron8],
                "types {types:?}"
            );
            if types[0] == ElementType::Hexahedron8 {
                ordered = vec![(size - 1) as Idx, 0, 0];
            }
        }
        distribute_prescribed(&mut m, ordered)?;
    } else {
        distribute_rcb(&mut m)?;
    }
    let _ = ok(
        akantu_mesh::utils::init_mesh_facets(&mut m),
        "init_mesh_facets",
    )?;
    let f = facet_mesh(&m)?;
    let fs = f
        .synchronizers()
        .element
        .as_ref()
        .ok_or("no facet synchronizer")?;
    // every ghost facet of dimension 2 is in a receive scheme
    let nb_ghost: usize = f
        .element_types(TypeFilter::new().dim(2).ghost(GhostType::Ghost))
        .map(|ty| f.nb_element(ty, GhostType::Ghost))
        .sum();
    let nb_recv: usize = fs.communications.recv.values().map(Vec::len).sum();
    ensure!(
        nb_recv == nb_ghost,
        "{nb_recv} received ghost facets of {nb_ghost}"
    );
    let mixed = fs
        .communications
        .send
        .values()
        .chain(fs.communications.recv.values())
        .flatten()
        .any(|e| e.ty() == ElementType::Triangle3);
    let b = barycenters(f)?;
    let mut acc = BarycenterAccessor {
        dim: 3,
        bary: &b,
        max_dist: 0.0,
        unpacked: 0,
    };
    ok(
        fs.synchronize(&mut acc, SynchronizationTag::Test),
        "synchronize",
    )?;
    // the ranks holding the elements exchange triangles (prism faces)
    if size > 1 && (rank == 0 || rank == size as Rank - 1) {
        ensure!(
            mixed,
            "rank {rank}: no triangle facet in the schemes (test not exercising UD-251)"
        );
    }
    ensure!(
        acc.max_dist == 0.0,
        "ghost facet barycentres differ by {}",
        acc.max_dist
    );
    ensure!(
        acc.unpacked == nb_recv,
        "unpacked {} of {nb_recv}",
        acc.unpacked
    );
    Ok(())
}

/// UD-254: a mesh made periodic on the root, then distributed, has no
/// periodic synchronizer on more than one rank: `makeConsistentForPeriodicity`
/// is refused (upstream dereferences a null pointer on non-root ranks)
/// until `update_periodic_synchronizer` builds it; then the upstream test
/// expectations hold.
pub fn ud254_periodic_after_distribution(comm: Communicator) -> Outcome {
    let size = comm.size();
    let mut m = read_on_root(&comm, "test_mesh_utils/test_mesh_periodic/square.msh", 2)?;
    if comm.rank() == 0 {
        let groups = ["x-minus", "x-plus", "y-minus", "y-plus"];
        let mut shared = akantu_core::Array::filled(m.nb_nodes(), 1, 0.0);
        for g in &groups {
            let nodes = ok(m.groups().element_group(g), "group")?.nodes().to_vec();
            for n in nodes {
                shared.as_mut_slice()[n as usize] += 1.0;
            }
        }
        m.data_mut()
            .insert_nodal("shared", akantu_mesh::NodalDataValues::Real(shared));
        let _ = ok(make_periodic(&mut m, SpatialDirection::X, None), "x")?;
        let _ = ok(make_periodic(&mut m, SpatialDirection::Y, None), "y")?;
    }
    distribute_rcb(&mut m)?;
    ensure!(m.is_periodic(), "periodicity lost by the distribution");
    let mut id = periodic_identity_input(&m);
    let r = make_consistent_for_periodicity_real(&m, &mut id, ReduceOperation::Id);
    if size > 1 {
        ensure!(
            r.is_err(),
            "makeConsistent without a periodic synchronizer must be refused"
        );
    }
    ok(
        akantu_mesh::periodic_synchronizer::update_periodic_synchronizer(&mut m),
        "update",
    )?;
    let mut id = periodic_identity_input(&m);
    ok(
        make_consistent_for_periodicity_real(&m, &mut id, ReduceOperation::Id),
        "identity",
    )?;
    ensure!(
        id.as_slice().iter().all(|&v| v == 10.0),
        "identity: {:?}",
        id.as_slice()
    );
    let mut sum = periodic_sum_input(&m)?;
    ok(
        make_consistent_for_periodicity_real(&m, &mut sum, ReduceOperation::Id),
        "sum id",
    )?;
    ok(
        make_consistent_for_periodicity_real(&m, &mut sum, ReduceOperation::Sum),
        "sum",
    )?;
    for (n, v) in sum.as_slice().iter().enumerate() {
        if m.node_flag(n as Idx).is_local_or_master() {
            ensure!(*v == 100.0, "sum node {n} = {v}");
        }
    }
    Ok(())
}

/// The grid synchronizer on a distributed mesh **with a facet mesh**: the
/// new pure-ghost nodes are mirrored into the facet mesh and the new ghost
/// elements get adjacency rows (ARCHITECTURE), so the facet/parent
/// invariants hold after the set-up path and the mutation path (which
/// checks them before commit).
pub fn grid_synchronizer_keeps_facet_mesh_consistent(comm: Communicator) -> Outcome {
    let scenario = "grid_tri_n8";
    let radius = grid_radius(scenario);
    // set-up path
    let mut m = grid_mesh_on_root(&comm, scenario)?;
    distribute_like_oracle(&comm, &mut m, scenario)?;
    let _ = ok(akantu_mesh::utils::init_mesh_facets(&mut m), "facets")?;
    let grid = local_barycenter_grid(&m, radius)?;
    let nb_before = m.nb_nodes();
    let _ = ok(
        create_grid_synchronizer(&mut m, &grid, "grid", GridSynchronizerOptions::default()),
        "grid synchronizer",
    )?;
    ok(
        akantu_mesh::utils::check_facet_mesh_consistency(&m),
        "facet consistency (set-up)",
    )?;
    ensure!(
        facet_mesh(&m)?.nb_nodes() == m.nb_nodes(),
        "facet mesh has {} nodes, parent {} (before {nb_before})",
        facet_mesh(&m)?.nb_nodes(),
        m.nb_nodes()
    );
    // mutation path
    let mut m = grid_mesh_on_root(&comm, scenario)?;
    distribute_like_oracle(&comm, &mut m, scenario)?;
    let _ = ok(akantu_mesh::utils::init_mesh_facets(&mut m), "facets")?;
    let grid = local_barycenter_grid(&m, radius)?;
    let shared = akantu_mesh::SharedMesh::new(m);
    let _ = ok(
        create_grid_synchronizer_shared(
            &shared,
            &mut [],
            &grid,
            "grid",
            GridSynchronizerOptions::default(),
        ),
        "grid synchronizer (mutation)",
    )?;
    let g = ok(shared.read(), "read")?;
    ok(
        akantu_mesh::utils::check_facet_mesh_consistency(&g),
        "facet consistency (mutation)",
    )?;
    Ok(())
}

// ------------------------------------------------ UD-813: an empty part

/// Scheme sizes `[(peer, nb sent), …]` and `[(peer, nb received), …]` of a
/// synchronizer, all-gathered: every rank's send toward `q` has the length
/// of `q`'s receive from it.
fn schemes_are_symmetric<E>(
    comm: &Communicator,
    c: &akantu_comm::Communications<E>,
    what: &str,
) -> Outcome {
    let mut mine = Vec::new();
    for (&q, l) in &c.send {
        mine.extend([0i64, i64::from(q), l.len() as i64]);
    }
    for (&q, l) in &c.recv {
        mine.extend([1i64, i64::from(q), l.len() as i64]);
    }
    let all = ok(comm.all_gather_v(&mine), "gather")?;
    let me = i64::from(comm.rank());
    let mut sends: BTreeMap<(i64, i64), i64> = BTreeMap::new();
    let mut recvs: BTreeMap<(i64, i64), i64> = BTreeMap::new();
    for (r, v) in all.iter().enumerate() {
        for t in v.as_chunks::<3>().0 {
            let key = if t[0] == 0 {
                (r as i64, t[1])
            } else {
                (t[1], r as i64)
            };
            if t[0] == 0 {
                sends.insert(key, t[2]);
            } else {
                recvs.insert(key, t[2]);
            }
        }
    }
    let nonempty = |m: &BTreeMap<(i64, i64), i64>| -> BTreeMap<(i64, i64), i64> {
        m.iter()
            .filter(|(_, n)| **n > 0)
            .map(|(k, n)| (*k, *n))
            .collect()
    };
    ensure!(
        nonempty(&sends) == nonempty(&recvs),
        "rank {me}: {what} schemes are not symmetric: sends {sends:?}, receives {recvs:?}"
    );
    Ok(())
}

/// A prescribed partition that
/// leaves the last rank of 4 without elements (three stripes `floor(3 x)`
/// on a Tri3 square n = 6; at 8 ranks ranks 3–7 are empty) is accepted;
/// `initMeshFacets`, the grid
/// synchronizer (R = 0.2) and `makePeriodic` x, y then complete on every
/// rank (upstream's `MeshPartitionMeshData` refuses the partition by a
/// debug assertion, oracle run `v101_empty_rank_*`), with symmetric schemes
/// and a consistent periodic relation. Other rank counts: every part
/// non-empty (the same checks).
pub fn ud813_empty_part(comm: Communicator) -> Outcome {
    let size = comm.size();
    let rank = comm.rank();
    let stripes = |m: &Mesh| -> std::result::Result<Vec<Idx>, String> {
        let mut p = Vec::new();
        for e in 0..m.nb_element(ElementType::Triangle3, GhostType::NotGhost) {
            let el = Element::new(ElementType::Triangle3, e as Idx, GhostType::NotGhost);
            let b = ok(m.barycenter(el), "barycenter")?;
            let s = ((b[0] * 3.).floor() as Idx).min(2);
            p.push(s % (size.min(3) as Idx));
        }
        Ok(p)
    };
    for what in ["facets", "grid", "periodic"] {
        let mut m = square_tri3_on_root(&comm, 6)?;
        if size > 1 {
            let parts = if rank == 0 { stripes(&m)? } else { Vec::new() };
            crate::par_common::distribute_prescribed(&mut m, parts)?;
        } else {
            crate::par_common::distribute_rcb(&mut m)?;
        }
        // parts 0..min(P, 3): ranks 3.. are empty when P > 3 (agreed check:
        // a rank-local refusal here would leave the others in a collective)
        let nb_local = m.nb_element(ElementType::Triangle3, GhostType::NotGhost);
        let expect_empty = size > 3 && rank >= 3;
        let mut bad = [u64::from(expect_empty != (nb_local == 0))];
        ok(
            comm.all_reduce(&mut bad, akantu_comm::ReduceOp::Sum),
            "all_reduce",
        )?;
        ensure!(
            bad[0] == 0,
            "rank {rank}/{size}: {nb_local} local elements (empty expected: {expect_empty}; {} rank(s) differ)",
            bad[0]
        );
        match what {
            "facets" => {
                let _ = ok(
                    init_mesh_facets_with(&mut m, DistributedFacetsOptions::default()),
                    "initMeshFacets",
                )?;
                let f = facet_mesh(&m)?;
                if let Some(fs) = &f.synchronizers().element {
                    schemes_are_symmetric(&comm, &fs.communications, "facet")?;
                } else {
                    ensure!(size == 1, "rank {rank}: no facet synchronizer");
                }
            }
            "grid" => {
                let grid = local_barycenter_grid(&m, 0.2)?;
                let (sync, _) = ok(
                    create_grid_synchronizer(
                        &mut m,
                        &grid,
                        "grid_synchronizer",
                        GridSynchronizerOptions::default(),
                    ),
                    "grid synchronizer",
                )?;
                schemes_are_symmetric(&comm, &sync.communications, "grid")?;
                ensure!(
                    m.local_bbox() == &m.local_bounding_box(),
                    "rank {rank}: local box is not the box of the local nodes"
                );
            }
            _ => {
                for dir in [SpatialDirection::X, SpatialDirection::Y] {
                    let _ = ok(make_periodic(&mut m, dir, None), "makePeriodic")?;
                }
                ensure!(m.is_periodic(), "rank {rank}: not periodic");
                if let Some(ps) = &m.synchronizers().periodic_node {
                    schemes_are_symmetric(&comm, &ps.communications, "periodic")?;
                }
                // every rank maps a global slave to the same global master
                let mut mine = Vec::new();
                for (ms, sl) in periodic_pairs(&m) {
                    mine.push(m.node_global_id(sl));
                    mine.push(m.node_global_id(ms));
                }
                let all = ok(comm.all_gather_v(&mine), "gather")?;
                let mut master_of: BTreeMap<i64, i64> = BTreeMap::new();
                for v in &all {
                    for p in v.as_chunks::<2>().0 {
                        if let Some(prev) = master_of.insert(p[0], p[1]) {
                            ensure!(
                                prev == p[1],
                                "global slave {} has masters {prev} and {}",
                                p[0],
                                p[1]
                            );
                        }
                    }
                }
                // the square has (n + 1)^2 = 49 nodes, 13 of them periodic
                // slaves (6 on x = 1, 6 on y = 1, the corner (1, 1))
                ensure!(
                    master_of.len() == 13,
                    "{} global periodic slaves (13 expected)",
                    master_of.len()
                );
            }
        }
    }
    Ok(())
}
