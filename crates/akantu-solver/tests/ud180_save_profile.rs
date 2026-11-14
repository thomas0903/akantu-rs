// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of
// SparseMatrixAIJ::saveProfile (src/model/common/sparse_matrix/
// sparse_matrix_aij.cc:85-120) on several processes: UD-180, category B.

//! UD-180 (SPM-007): upstream's
//! multi-rank `saveProfile` lets every rank truncate and rewrite one file
//! (a corrupt file); the port writes one valid Matrix Market pattern per
//! rank (`_rank-<rank>.mtx`, as `saveMatrix`), whose union is the global
//! profile. Checked at 2 ranks of the local world on the distributed bar
//! of the solver tests (Jacobian "J" after a Newton solve) against the
//! serial run, entries compared through the node positions (the global
//! equation numbering of the distributed run is not the serial one).

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_linalg::direct::SparseSolverFactory;
use akantu_solver::{AijBackend, DofManager};
use common::{Springs, newton, serial_bar, two_rank_bar};

fn backend() -> Box<AijBackend> {
    let mut aij = AijBackend::default();
    aij.set_solver(
        "J",
        SparseSolverFactory::default()
            .create_by_name("eigen")
            .unwrap(),
    );
    Box::new(aij)
}

/// `(order, entries)` of a Matrix Market pattern file; the header's entry
/// count must equal the number of entry lines.
fn parse_pattern(text: &str, symmetry: &str) -> (usize, Vec<(usize, usize)>) {
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some(format!("%%MatrixMarket matrix coordinate pattern {symmetry}").as_str())
    );
    let dims: Vec<usize> = lines
        .next()
        .unwrap()
        .split_whitespace()
        .map(|t| t.parse().unwrap())
        .collect();
    assert_eq!(dims.len(), 3);
    assert_eq!(dims[0], dims[1]);
    let entries: Vec<(usize, usize)> = lines
        .map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            assert_eq!(t.len(), 3, "{l}");
            assert_eq!(t[2], "1");
            (t[0].parse().unwrap(), t[1].parse().unwrap())
        })
        .collect();
    assert_eq!(entries.len(), dims[2], "header count = entry lines");
    for &(i, j) in &entries {
        assert!(i >= 1 && i <= dims[0] && j >= 1 && j <= dims[0]);
    }
    (dims[0], entries)
}

/// Global equation number → node position of the DOFs of "u" on `dm`.
fn equation_positions(dm: &DofManager, xs: &[f64]) -> BTreeMap<u64, u64> {
    dm.global_equation_numbers("u")
        .unwrap()
        .into_iter()
        .zip(xs)
        .filter_map(|(g, &x)| g.map(|g| (g as u64, x as u64)))
        .collect()
}

fn scratch() -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ud180_save_profile");
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn ud180_save_profile_writes_one_valid_file_per_rank() {
    let dir = scratch();
    // serial reference: the name as given, upstream's bytes
    let mut dm = DofManager::with_backend("dm", Communicator::serial(), backend());
    let mut p = Springs::new(serial_bar(4), 4.0, true);
    p.setup(&mut dm);
    newton(&mut dm, &mut p).unwrap();
    let j = dm.matrix("J").unwrap();
    let written = j.save_profile_on(&dir.join("serial"), 0, 1).unwrap();
    assert_eq!(written, dir.join("serial"));
    let text = std::fs::read_to_string(&written).unwrap();
    assert_eq!(text, j.profile_text());
    let symmetry = text
        .lines()
        .next()
        .unwrap()
        .rsplit(' ')
        .next()
        .unwrap()
        .to_owned();
    let (order, serial) = parse_pattern(&text, &symmetry);
    let xs: Vec<f64> = p.mesh.nodes().as_slice().to_vec();
    let pos = equation_positions(&dm, &xs);
    let serial_pairs: BTreeSet<(u64, u64)> = serial
        .iter()
        .map(|&(i, j)| (pos[&(i as u64 - 1)], pos[&(j as u64 - 1)]))
        .collect();

    // two ranks: one file per rank
    let out = run_local(2, Duration::from_secs(20), |comm| {
        let mut dm = DofManager::with_backend("dm", comm.clone(), backend());
        let mut p = Springs::new(two_rank_bar(&comm), 4.0, true);
        p.setup(&mut dm);
        newton(&mut dm, &mut p).map_err(|e| e.to_string())?;
        let file = dm
            .matrix("J")
            .map_err(|e| e.to_string())?
            .save_profile_on(&dir.join("profile.mtx"), comm.rank(), comm.size())
            .map_err(|e| e.to_string())?;
        let xs: Vec<f64> = p.mesh.nodes().as_slice().to_vec();
        Ok::<_, String>((file, equation_positions(&dm, &xs)))
    });
    let mut positions = BTreeMap::new();
    let mut files = Vec::new();
    for (rank, o) in out.into_iter().enumerate() {
        let (file, pos) = o.unwrap_or_else(|e| panic!("rank {rank}: {e}"));
        assert_eq!(file, dir.join(format!("profile_rank-{rank}.mtx")));
        for (g, x) in pos {
            assert_eq!(*positions.entry(g).or_insert(x), x, "equation {g}");
        }
        files.push(file);
    }
    assert!(
        !dir.join("profile.mtx").exists(),
        "no shared file is written"
    );
    let mut union = BTreeSet::new();
    for file in &files {
        let (o, entries) = parse_pattern(&std::fs::read_to_string(file).unwrap(), &symmetry);
        assert_eq!(o, order, "{}: global order", file.display());
        assert!(!entries.is_empty(), "{}: rank entries", file.display());
        for (i, j) in entries {
            union.insert((positions[&(i as u64 - 1)], positions[&(j as u64 - 1)]));
        }
    }
    assert_eq!(
        union, serial_pairs,
        "the rank files together hold the serial profile"
    );
}
