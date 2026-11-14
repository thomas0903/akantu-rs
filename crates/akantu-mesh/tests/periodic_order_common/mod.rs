// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): replay of the oracle probe of
// src/mesh/mesh_periodic.cc (makePeriodic, makeConsistentForPeriodicity),
// src/mesh/mesh_inline_impl.hh (getPeriodicSlaves) and
// src/synchronizer/periodic_node_synchronizer_tmpl.hh written by a
// probe on the oracle; shared by the `periodic_order_oracle` tests
// of akantu-mesh and akantu-model (UD-654).
//
// Fixture `fixtures/mesh_utils/periodic_order` (PROVENANCE.txt): 67 cases
// (structured line/quadrangle/hexahedron grids with natural, reversed and
// shuffled node numbering, periodic in 1–3 directions in several orders,
// plus upstream's frozen periodic meshes). Per case the oracle printed every
// master's `getPeriodicSlaves` iteration order and the bits of a
// 3-component nodal array after `makeConsistentForPeriodicity(_sum)`; the
// array's values span 41 binades with random signs, so every summation
// order of a master's slaves gives different bits with high probability.

use std::collections::BTreeMap;
use std::path::PathBuf;

use akantu_core::enums::{MeshIoType, SpatialDirection};
use akantu_core::{Array, Idx, Real};
use akantu_mesh::Mesh;
use akantu_validation::fixtures_root;

/// Totals of the committed fixture (asserted so that a
/// truncated or edited fixture cannot pass silently).
pub const NB_CASES: usize = 67;
pub const NB_MASTERS: usize = 2_640;
pub const NB_MASTERS_WITH_SEVERAL_SLAVES: usize = 374;
pub const NB_VALUES: usize = 42_465;
/// Values the two former port rules get wrong: the slaves
/// summed in insertion order, and in sorted `(master, slave)` order (UD-132).
pub const INSERTION_ORDER_MISMATCHES: usize = 1_624;
pub const SORTED_ORDER_MISMATCHES: usize = 1_164;

fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Component `c` of node `n` of the probe's nodal array (same decoding as
/// the probe: random mantissa and sign, exponent in `[-20, 20]`).
pub fn value(n: u64, c: u64) -> Real {
    let z = splitmix(n * 8 + c);
    let mant = z & ((1u64 << 52) - 1);
    let e = 1023 - 20 + ((z >> 52) % 41);
    let sign = (z >> 63) & 1;
    f64::from_bits((sign << 63) | (e << 52) | mant)
}

/// One oracle case.
pub struct OracleCase {
    pub name: String,
    pub mesh_file: PathBuf,
    pub dim: usize,
    pub dirs: String,
    pub nb_nodes: usize,
    /// master → slaves in upstream's `getPeriodicSlaves` iteration order.
    pub masters: BTreeMap<Idx, Vec<Idx>>,
    /// bits of the 3 components of every node after the periodic sum.
    pub values: Vec<[u64; 3]>,
}

pub fn fixture_dir() -> PathBuf {
    fixtures_root().join("mesh_utils/periodic_order")
}

#[derive(Default)]
struct Printed {
    nb_nodes: usize,
    masters: BTreeMap<Idx, Vec<Idx>>,
    values: Vec<[u64; 3]>,
}

fn parse_output(text: &str) -> BTreeMap<String, Printed> {
    let mut out = BTreeMap::new();
    let mut cur: Option<(String, Printed)> = None;
    for line in text.lines() {
        let w: Vec<&str> = line.split_whitespace().collect();
        match w.first().copied() {
            Some("CASE") => {
                assert!(cur.is_none(), "unterminated case before {line}");
                let p = Printed {
                    nb_nodes: w[2].parse().unwrap(),
                    ..Printed::default()
                };
                cur = Some((w[1].to_owned(), p));
            }
            Some("M") => {
                let c = &mut cur.as_mut().expect("M outside a case").1;
                let m: Idx = w[1].parse().unwrap();
                let s = w[2..].iter().map(|s| s.parse().unwrap()).collect();
                assert!(c.masters.insert(m, s).is_none(), "master {m} twice");
            }
            Some("V") => {
                let c = &mut cur.as_mut().expect("V outside a case").1;
                assert_eq!(w[1].parse::<usize>().unwrap(), c.values.len());
                let mut v = [0u64; 3];
                for (k, x) in v.iter_mut().enumerate() {
                    *x = u64::from_str_radix(w[2 + k], 16).unwrap();
                }
                c.values.push(v);
            }
            Some("END") => {
                let (n, c) = cur.take().expect("END outside a case");
                assert_eq!(n, w[1]);
                assert_eq!(c.values.len(), c.nb_nodes, "{n}: values");
                assert!(out.insert(n, c).is_none());
            }
            Some("ERROR") => panic!("oracle error: {line}"),
            _ => {}
        }
    }
    assert!(cur.is_none(), "unterminated case at the end");
    out
}

/// The cases of `cases.txt` with their oracle output.
pub fn load_cases() -> Vec<OracleCase> {
    let dir = fixture_dir();
    let mut printed =
        parse_output(&std::fs::read_to_string(dir.join("oracle_output.txt")).unwrap());
    let list = std::fs::read_to_string(dir.join("cases.txt")).unwrap();
    let mut out = Vec::new();
    for line in list
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let w: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(w.len(), 4, "case line: {line}");
        let p = printed
            .remove(w[0])
            .unwrap_or_else(|| panic!("{}: no oracle output", w[0]));
        out.push(OracleCase {
            name: w[0].to_owned(),
            mesh_file: dir.join("meshes").join(w[1]),
            dim: w[2].parse().unwrap(),
            dirs: w[3].to_owned(),
            nb_nodes: p.nb_nodes,
            masters: p.masters,
            values: p.values,
        });
    }
    assert!(printed.is_empty(), "oracle cases missing from cases.txt");
    out
}

/// The case's mesh made periodic in its directions, in the listed order
/// (upstream `makePeriodic` per direction).
pub fn periodic_mesh(case: &OracleCase) -> Mesh {
    let mut mesh = Mesh::new(case.dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, &case.mesh_file, MeshIoType::Gmsh).unwrap();
    for d in case.dirs.chars() {
        let sd = match d {
            'x' => SpatialDirection::X,
            'y' => SpatialDirection::Y,
            'z' => SpatialDirection::Z,
            _ => panic!("{}: direction {d}", case.name),
        };
        let _events = akantu_mesh::periodic::make_periodic(&mut mesh, sd, None).unwrap();
    }
    assert_eq!(mesh.nb_nodes(), case.nb_nodes, "{}: nb_nodes", case.name);
    mesh
}

/// The probe's nodal array (3 components).
pub fn input_array(nb_nodes: usize) -> Array<Real> {
    let mut a = Array::<Real>::zeros(nb_nodes, 3);
    for (i, t) in a
        .as_mut_slice()
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .enumerate()
    {
        for (c, x) in t.iter_mut().enumerate() {
            *x = value(i as u64, c as u64);
        }
    }
    a
}

/// Periodic sum with the slaves of every master accumulated in `order`
/// (slave values taken before any reduction: a slave has one master), then
/// copied back: the model of the former rules, to show that the cases
/// discriminate them.
pub fn sum_in_order(mesh: &Mesh, order: &dyn Fn(Idx) -> Vec<Idx>) -> Vec<[u64; 3]> {
    let n = mesh.nb_nodes();
    let snapshot: Vec<[Real; 3]> = (0..n)
        .map(|i| [value(i as u64, 0), value(i as u64, 1), value(i as u64, 2)])
        .collect();
    let mut a = snapshot.clone();
    for m in 0..n {
        let mi = Idx::try_from(m).unwrap();
        let slaves = mesh.periodic_slaves(mi);
        if slaves.is_empty() {
            continue;
        }
        for s in order(mi) {
            for c in 0..3 {
                a[m][c] += snapshot[s as usize][c];
            }
        }
        for &s in slaves {
            a[s as usize] = a[m];
        }
    }
    a.iter()
        .map(|v| [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()])
        .collect()
}

/// Counts of a replay.
#[derive(Debug, Default)]
pub struct Replay {
    pub cases: usize,
    pub masters: usize,
    pub masters_with_several_slaves: usize,
    pub values: usize,
    /// masters whose `periodic_slaves_upstream_order` differs from the
    /// oracle's `getPeriodicSlaves` order.
    pub order_mismatches: usize,
    /// values of the port's periodic sum differing from the oracle's bits.
    pub value_mismatches: usize,
    pub insertion_order_mismatches: usize,
    pub sorted_order_mismatches: usize,
}

/// Replays every case: the port's masters, slave sets and upstream-order
/// accessor against the oracle, and `periodic_sum` (the port's
/// `makeConsistentForPeriodicity(_sum)` under test) bit for bit.
pub fn replay(periodic_sum: &dyn Fn(&Mesh, &mut Array<Real>)) -> Replay {
    let mut r = Replay::default();
    for case in load_cases() {
        let name = &case.name;
        let mesh = periodic_mesh(&case);
        let mut port_masters = BTreeMap::new();
        for m in 0..Idx::try_from(mesh.nb_nodes()).unwrap() {
            if mesh.is_periodic_master(m).unwrap() {
                let s: Vec<Idx> = mesh.periodic_slaves_upstream_order(m).collect();
                assert_eq!(s.len(), mesh.periodic_slaves(m).len());
                port_masters.insert(m, s);
            }
        }
        assert_eq!(
            port_masters.keys().collect::<Vec<_>>(),
            case.masters.keys().collect::<Vec<_>>(),
            "{name}: masters"
        );
        for (m, s) in &case.masters {
            r.masters += 1;
            if s.len() > 1 {
                r.masters_with_several_slaves += 1;
            }
            let mut ps = port_masters[m].clone();
            if &ps != s {
                r.order_mismatches += 1;
                if r.order_mismatches <= 10 {
                    eprintln!("{name}: master {m}: oracle {s:?}, port {ps:?}");
                }
            }
            let mut os = s.clone();
            ps.sort_unstable();
            os.sort_unstable();
            assert_eq!(ps, os, "{name}: master {m}: slave sets");
        }
        let mut a = input_array(mesh.nb_nodes());
        periodic_sum(&mesh, &mut a);
        let ins = sum_in_order(&mesh, &|m| mesh.periodic_slaves(m).to_vec());
        let sorted = sum_in_order(&mesh, &|m| {
            let mut s = mesh.periodic_slaves(m).to_vec();
            s.sort_unstable();
            s
        });
        for n in 0..mesh.nb_nodes() {
            for c in 0..3 {
                r.values += 1;
                let o = case.values[n][c];
                let p = a.as_slice()[3 * n + c].to_bits();
                if p != o {
                    r.value_mismatches += 1;
                    if r.value_mismatches <= 10 {
                        eprintln!("{name}: node {n} component {c}: oracle {o:016x}, port {p:016x}");
                    }
                }
                r.insertion_order_mismatches += usize::from(ins[n][c] != o);
                r.sorted_order_mismatches += usize::from(sorted[n][c] != o);
            }
        }
        r.cases += 1;
    }
    r
}

/// The assertions shared by both tests.
pub fn assert_replay(r: &Replay, what: &str) {
    eprintln!("{what}: {r:?}");
    assert_eq!(r.cases, NB_CASES, "cases");
    assert_eq!(r.masters, NB_MASTERS, "masters");
    assert_eq!(
        r.masters_with_several_slaves, NB_MASTERS_WITH_SEVERAL_SLAVES,
        "masters with several slaves"
    );
    assert_eq!(r.values, NB_VALUES, "values");
    // the cases discriminate the rules: the two former orders fail
    assert_eq!(r.insertion_order_mismatches, INSERTION_ORDER_MISMATCHES);
    assert_eq!(r.sorted_order_mismatches, SORTED_ORDER_MISMATCHES);
    assert_eq!(
        r.order_mismatches, 0,
        "getPeriodicSlaves order (UD-654: reverse insertion order)"
    );
    assert_eq!(r.value_mismatches, 0, "{what}: periodic sums (bitwise)");
}
