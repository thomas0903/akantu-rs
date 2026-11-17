// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of
// DOFManagerDefault::assembleElementalMatricesToMatrix
// (src/model/common/dof_manager/dof_manager_default.cc) and of the matrix
// operations of an implicit step on threads (no upstream counterpart:
// D-123).

//! The element matrix assembly of the DOF manager and the whole-matrix
//! operations of an implicit step give the same matrices, bit for bit, at
//! 1, 2, 3, 4 and 8 threads, finest and automatic split: two filters
//! (materials) on one element type and a second element type, symmetric
//! blocks into a symmetric matrix and unsymmetric blocks into an
//! unsymmetric one, several assembly rounds (profile, positions stored,
//! cached), entries appended between rounds (contact couplings), a block
//! whose equations change, Dirichlet rows, `J = copy_profile(K); J += c M;
//! J += d K` and products.

use akantu_comm::Communicator;
use akantu_core::enums::MatrixType;
use akantu_core::exec::{Exec, ExecPolicy, Grain};
use akantu_core::{Array, ElementType, GhostType, GlobalIdx, Idx, Real};
use akantu_linalg::terms::TermsToAssemble;
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::DofManager;

/// An `n³` cube: cells with `i + j + k` even as hexahedra, the others as
/// six tetrahedra.
fn cube(n: usize) -> Mesh {
    let mut m = Mesh::new(3, "cube").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    a.push_node(&[i as Real, j as Real, k as Real]).unwrap();
                }
            }
        }
        let id = |i: usize, j: usize, k: usize| (i + (n + 1) * (j + (n + 1) * k)) as Idx;
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
                    if (i + j + k) % 2 == 0 {
                        a.connectivity_mut(ElementType::Hexahedron8, GhostType::NotGhost)
                            .push_tuple(&v);
                    } else {
                        for t in [
                            [0, 1, 2, 6],
                            [0, 2, 3, 6],
                            [0, 3, 7, 6],
                            [0, 7, 4, 6],
                            [0, 4, 5, 6],
                            [0, 5, 1, 6],
                        ] {
                            a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost)
                                .push_tuple(&[v[t[0]], v[t[1]], v[t[2]], v[t[3]]]);
                        }
                    }
                }
            }
        }
    }
    m
}

/// Deterministic values (signed zeros and subnormals now and then).
struct Rng(u64);

impl Rng {
    fn value(&mut self) -> Real {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        let r = self.0.wrapping_mul(0x2545_f491_4f6c_dd1d);
        match r % 32 {
            0 => -0.0,
            1 => 0.0,
            2 => 5e-324,
            _ => (r >> 11) as Real / (1u64 << 53) as Real * 2.0 - 1.0,
        }
    }
}

/// Elemental matrices of `nb` elements of size `n × n` (symmetric when
/// asked), for assembly round `round`.
fn blocks(nb: usize, n: usize, symmetric: bool, seed: u64) -> Array<Real> {
    let mut rng = Rng(seed | 1);
    let mut out = Array::new(n * n);
    let mut b = vec![0.0; n * n];
    for _ in 0..nb {
        for i in 0..n {
            for j in 0..n {
                if symmetric && j < i {
                    b[i + j * n] = b[j + i * n];
                } else {
                    b[i + j * n] = rng.value();
                }
            }
        }
        out.push_tuple(&b);
    }
    out
}

/// Every matrix of the run: coordinates and value bits, value release.
type Snapshot = Vec<(String, Vec<GlobalIdx>, Vec<GlobalIdx>, Vec<u64>, u64)>;

fn snapshot(dm: &DofManager, ids: &[&str]) -> Snapshot {
    ids.iter()
        .map(|id| {
            let m = dm.matrix(id).unwrap();
            let (i, j, a) = m.coo();
            (
                (*id).to_owned(),
                i.to_vec(),
                j.to_vec(),
                a.iter().map(|v| v.to_bits()).collect(),
                m.value_release(),
            )
        })
        .collect()
}

/// A sequence of implicit-step-like operations; returns the matrices after
/// every round, the products and the number of threaded assemblies.
fn run(exec: &Exec, symmetric: bool, cached: bool) -> (Vec<Snapshot>, Vec<Vec<u64>>, u64) {
    let mut mesh = cube(4);
    let g = GhostType::NotGhost;
    let ty_k = if symmetric {
        MatrixType::Symmetric
    } else {
        MatrixType::Unsymmetric
    };
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.set_exec(exec.clone());
    dm.set_assembly_cache(cached);
    dm.register_nodal_dofs(&mesh, "u", 3).unwrap();
    dm.register_blocked_dofs("u").unwrap();
    dm.new_matrix("K", ty_k).unwrap();
    dm.new_matrix("M", ty_k).unwrap();
    dm.new_matrix("J", ty_k).unwrap();
    let tets = mesh.nb_element(ElementType::Tetrahedron4, g);
    let hexes = mesh.nb_element(ElementType::Hexahedron8, g);
    // two materials on the tetrahedra (every third element, the others)
    let mat_a: Vec<Idx> = (0..tets as Idx).filter(|e| e % 3 == 0).collect();
    let mat_b: Vec<Idx> = (0..tets as Idx).filter(|e| e % 3 != 0).collect();
    let mut snaps = Vec::new();
    let mut products = Vec::new();
    let nb_dofs = 3 * mesh.nb_nodes();
    let x: Vec<Real> = {
        let mut r = Rng(99);
        (0..nb_dofs).map(|_| r.value()).collect()
    };
    for round in 0..6u64 {
        if round == 4 {
            // a mesh event: the node order of a tetrahedron and of a
            // hexahedron permuted (the same pairs, other equations per
            // local index): the stored slots and the scatter plan must not
            // be trusted after it
            let mut a = MeshAccessor::new(&mut mesh);
            a.connectivity_mut(ElementType::Tetrahedron4, g)
                .tuple_mut(7)
                .swap(0, 2);
            a.connectivity_mut(ElementType::Hexahedron8, g)
                .tuple_mut(1)
                .rotate_left(3);
        }
        dm.zero_matrix("K").unwrap();
        let assemble = |dm: &mut DofManager, matrix: &str, seed: u64| {
            for (k, filter) in [&mat_a, &mat_b].into_iter().enumerate() {
                let b = blocks(filter.len(), 12, symmetric, seed + k as u64);
                dm.assemble_elemental_matrices_to_matrix(
                    &mesh,
                    matrix,
                    "u",
                    b.view(),
                    ElementType::Tetrahedron4,
                    g,
                    ty_k,
                    Some(filter),
                )
                .unwrap();
            }
            let b = blocks(hexes, 24, symmetric, seed + 7);
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                matrix,
                "u",
                b.view(),
                ElementType::Hexahedron8,
                g,
                ty_k,
                None,
            )
            .unwrap();
        };
        assemble(&mut dm, "K", 1000 * round);
        if round == 0 {
            dm.zero_matrix("M").unwrap();
            assemble(&mut dm, "M", 777);
        }
        if round == 2 {
            // contact couplings: entries appended to K's profile (the next
            // round's batch must not reuse the plan of the shorter profile)
            let mut t = TermsToAssemble::new("u", "u");
            t.term(0, (nb_dofs - 1) as Idx).value = 2.5;
            t.term(5, (nb_dofs - 7) as Idx).value = -1.25;
            dm.assemble_preassembled_matrix("K", &t).unwrap();
        }
        // J = copy_profile(K); J += c M; J += d K; Dirichlet rows
        dm.copy_matrix_profile("J", "K").unwrap();
        dm.add_matrix("J", "M", 0.125).unwrap();
        dm.add_matrix("J", "K", 1.0 / 3.0).unwrap();
        {
            let mut blocked = dm.blocked_dofs_mut("u").unwrap();
            for (k, b) in blocked.as_mut_slice().iter_mut().enumerate() {
                *b = (k + round as usize).is_multiple_of(11);
            }
        }
        dm.apply_boundary("J").unwrap();
        let mut y: Vec<Real> = x.iter().map(|v| v * 0.5).collect();
        dm.mat_vec(&mesh, "K", &x, &mut y, 1.5, -0.25).unwrap();
        products.push(y.iter().map(|v| v.to_bits()).collect());
        snaps.push(snapshot(&dm, &["K", "M", "J"]));
    }
    (snaps, products, dm.threaded_assemblies())
}

#[test]
fn matrix_assembly_and_operations_are_bitwise_for_every_thread_count() {
    for symmetric in [true, false] {
        // the reference: the uncached loop (`add_values` per element)
        let (reference, ref_products, serial_batches) = run(&Exec::serial(), symmetric, false);
        assert_eq!(serial_batches, 0, "no threaded batch on one thread");
        for threads in [1, 2, 3, 4, 8] {
            for grain in [Grain::Finest, Grain::Auto] {
                let p = ExecPolicy::serial()
                    .with_threads(threads)
                    .unwrap()
                    .with_grain(grain);
                let exec = Exec::new(p).unwrap();
                let (snaps, products, batches) = run(&exec, symmetric, true);
                let what = format!("symmetric {symmetric} threads {threads} {grain:?}");
                // every assembly from the second round on is threaded (3
                // per round: two materials and the hexahedra), and in the
                // first round the second material of K and of M (it
                // continues the position cache of the first, as the serial
                // loop does: its positions are stored in that round)
                let expected = if threads == 1 { 0 } else { 5 * 3 + 2 };
                assert_eq!(batches, expected, "{what}: threaded assemblies");
                assert_eq!(products, ref_products, "{what}: products");
                for (round, (a, b)) in reference.iter().zip(&snaps).enumerate() {
                    for (ma, mb) in a.iter().zip(b) {
                        assert!(ma == mb, "{what}: matrix {} differs in round {round}", ma.0);
                    }
                }
            }
        }
    }
}

/// `assemble_elemental_matrices_by_chunks` with chunks of a few elements
/// gives, round after round, the matrix of the uncached loop bit for bit
/// at 1, 2, 3, 4 and 8 threads (finest and automatic grain): two materials
/// on the tetrahedra and the hexahedra, couplings appended to the profile,
/// a mesh event (an element's node order permuted), and the chunked path
/// really taken once a plan exists.
#[test]
fn chunked_assembly_is_the_uncached_loop() {
    fn rounds(exec: &Exec, cached: bool) -> (Vec<Snapshot>, usize) {
        let mut mesh = cube(4);
        let g = GhostType::NotGhost;
        let mut dm = DofManager::new("dm", Communicator::serial());
        dm.set_exec(exec.clone());
        dm.set_assembly_cache(cached);
        dm.set_assembly_chunk_values(3 * 144 + 17);
        dm.register_nodal_dofs(&mesh, "u", 3).unwrap();
        dm.new_matrix("K", MatrixType::Symmetric).unwrap();
        let tets = mesh.nb_element(ElementType::Tetrahedron4, g);
        let hexes = mesh.nb_element(ElementType::Hexahedron8, g);
        let mat_a: Vec<Idx> = (0..tets as Idx).filter(|e| e % 3 == 0).collect();
        let mat_b: Vec<Idx> = (0..tets as Idx).filter(|e| e % 3 != 0).collect();
        let all_hex: Vec<Idx> = (0..hexes as Idx).collect();
        let nb_dofs = 3 * mesh.nb_nodes();
        let mut snaps = Vec::new();
        let mut chunked = 0;
        let (mut full, mut chunk) = (Array::new(1), Array::new(1));
        for round in 0..6u64 {
            if round == 4 {
                let mut a = MeshAccessor::new(&mut mesh);
                a.connectivity_mut(ElementType::Tetrahedron4, g)
                    .tuple_mut(4)
                    .swap(1, 3);
            }
            dm.zero_matrix("K").unwrap();
            for (k, (ty, filter, n)) in [
                (ElementType::Tetrahedron4, &mat_a, 12),
                (ElementType::Tetrahedron4, &mat_b, 12),
                (ElementType::Hexahedron8, &all_hex, 24),
            ]
            .into_iter()
            .enumerate()
            {
                let b = blocks(filter.len(), n, true, 100 * round + k as u64);
                let mut produce = |r: std::ops::Range<usize>, out: &mut Array<Real>| {
                    *out = Array::from_vec(
                        b.as_slice()[r.start * n * n..r.end * n * n].to_vec(),
                        n * n,
                    )?;
                    Ok(())
                };
                if dm
                    .assemble_elemental_matrices_by_chunks(
                        &mesh,
                        "K",
                        "u",
                        n * n,
                        &mut produce,
                        &mut full,
                        &mut chunk,
                        ty,
                        g,
                        MatrixType::Symmetric,
                        Some(filter),
                    )
                    .unwrap()
                {
                    chunked += 1;
                }
            }
            if round == 2 {
                let mut t = TermsToAssemble::new("u", "u");
                t.term(2, (nb_dofs - 4) as Idx).value = 0.75;
                dm.assemble_preassembled_matrix("K", &t).unwrap();
            }
            snaps.push(snapshot(&dm, &["K"]));
        }
        (snaps, chunked)
    }
    let (reference, none) = rounds(&Exec::serial(), false);
    assert_eq!(none, 0, "no chunked path without the cache");
    for threads in [1, 2, 3, 4, 8] {
        for grain in [Grain::Finest, Grain::Auto] {
            let exec = Exec::new(
                ExecPolicy::serial()
                    .with_threads(threads)
                    .unwrap()
                    .with_grain(grain),
            )
            .unwrap();
            let (snaps, chunked) = rounds(&exec, true);
            let what = format!("threads {threads} {grain:?}");
            assert!(chunked >= 6, "{what}: the chunked path ran {chunked} times");
            for (round, (a, b)) in reference.iter().zip(&snaps).enumerate() {
                assert!(a == b, "{what}: K differs in round {round}");
            }
        }
    }
}
