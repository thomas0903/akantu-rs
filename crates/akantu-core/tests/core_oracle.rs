// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests of
// the core utilities against the oracle fixtures fixtures/core/common/*
// written by oracle/probes/core/core_probe.cc: SpatialGrid
// (src/common/aka_grid_dynamic.hh), BBox (aka_bbox.hh), Math::reduce /
// NewtonRaphson / pow / are_float_equal / intersects / is_in_range
// (aka_math*), printMemorySize / to_lower / trim (aka_common_inline_impl.hh),
// getVersion (aka_config.cc.in), CSR (aka_csr.hh), CircularArray
// (aka_circular_array*.hh).
//
// Every case is replayed from the recorded inputs and every recorded
// output is compared (exact rules of fixtures/core/common/tolerances.json).
// The inputs the probe echoes into the fixture are read with
// `Case::input_f64` / `Case::input_i64`, which mark them as consumed without
// comparing them; a quantity that is neither compared nor read as an input
// is a failure. The `strings` case records one input array only: its outputs
// are text parameters of the manifest, compared with `assert_eq!`.

use std::collections::BTreeSet;

use akantu_core::array_print::ArrayPrint;
use akantu_core::bbox::BBox;
use akantu_core::circular_array::CircularArray;
use akantu_core::csr::Csr;
use akantu_core::debug::{DebugLevel, Debugger, format_backtrace};
use akantu_core::math::{
    self, DEFAULT_TOLERANCE, NewtonRaphson, are_float_equal, is_in_range, pow_n,
};
use akantu_core::spatial_grid::SpatialGrid;
use akantu_core::strings::{print_memory_size, to_lower, trim, trim_char};
use akantu_core::{Array, Element, ElementType, GhostType, Int, Real, UInt, version};
use akantu_validation::DiffTest;
use serde_json::Value;

/// A differential case that refuses unconsumed quantities.
struct Case {
    t: DiffTest,
    seen: BTreeSet<String>,
}

impl Case {
    fn open(test: &str, id: &str) -> Self {
        Self {
            t: DiffTest::new(test, id).unwrap(),
            seen: BTreeSet::new(),
        }
    }

    fn f64(&mut self, q: &str, a: &[f64]) {
        self.seen.insert(q.to_owned());
        self.t.check_f64(q, a);
    }

    fn i64(&mut self, q: &str, a: &[i64]) {
        self.seen.insert(q.to_owned());
        self.t.check_i64(q, a);
    }

    fn ragged(&mut self, q: &str, rows: &[Vec<f64>]) {
        self.seen.insert(q.to_owned());
        self.t.check_ragged_f64(q, rows);
    }

    /// An input the probe echoed into the fixture: the replay feeds it to
    /// the port, so it is consumed here and not compared (a comparison of
    /// the recorded values with themselves could never fail).
    fn input_f64(&mut self, q: &str) -> Vec<f64> {
        self.seen.insert(q.to_owned());
        self.ref_f64(q)
    }

    /// As [`Case::input_f64`] for an integer input.
    fn input_i64(&mut self, q: &str) -> Vec<i64> {
        self.seen.insert(q.to_owned());
        self.ref_i64(q)
    }

    fn ref_f64(&self, q: &str) -> Vec<f64> {
        self.t.reference_f64(q).unwrap().data
    }

    fn ref_i64(&self, q: &str) -> Vec<i64> {
        self.t.reference_i64(q).unwrap().data
    }

    /// The exact rule ignores the sign of zero (by design of the harness):
    /// quantities where `±0.0` is observable are also compared bit for bit.
    fn bits(&self, q: &str, mine: &[f64]) {
        let r = self.ref_f64(q);
        assert_eq!(r.len(), mine.len(), "{q} length");
        for (k, (a, b)) in r.iter().zip(mine).enumerate() {
            // D-046 (bit identity, or default NaNs of either sign)
            assert!(
                akantu_validation::exact_eq(*a, *b),
                "{q}[{k}] bits: oracle {a:e} ({:#018x}), port {b:e} ({:#018x})",
                a.to_bits(),
                b.to_bits()
            );
        }
    }

    fn params(&self) -> &Value {
        &self.t.fixture().manifest().parameters
    }

    fn assert_all_consumed(&self) {
        let all: BTreeSet<String> = self
            .t
            .fixture()
            .quantity_names("*")
            .into_iter()
            .map(str::to_owned)
            .collect();
        let missing: Vec<_> = all.difference(&self.seen).collect();
        assert!(missing.is_empty(), "unchecked quantities: {missing:?}");
    }

    fn finish(self) {
        self.assert_all_consumed();
        self.t.assert_pass();
    }

    /// Finish a case whose outputs are all text parameters of the manifest,
    /// compared with `assert_eq!` by the caller, and whose recorded arrays
    /// are all inputs. The harness report counts array comparisons only, so
    /// it must hold none here (it would refuse to pass without evidence).
    fn finish_parameters_only(self) {
        self.assert_all_consumed();
        let report = self.t.report();
        assert!(
            report.quantities.is_empty(),
            "a case finished as parameters-only compared arrays"
        );
    }
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_owned())
        .collect()
}

fn b(x: bool) -> i64 {
    i64::from(x)
}

// --- SpatialGrid -----------------------------------------------------------------

fn grid_points(dim: usize) {
    grid_case(
        &format!("core_oracle_grid_points_{dim}d"),
        &format!("core/common/grid_points_{dim}d"),
    );
}

/// Replay of a `SpatialGrid<Int>` case (points, spacing and center are
/// inputs recorded in the fixture; the dimension is the spacing's length).
fn grid_case(test: &str, id: &str) {
    let mut c = Case::open(test, id);
    let spacing = c.input_f64("spacing");
    let center = c.input_f64("center");
    let pts = c.input_f64("points");
    let dim = spacing.len();
    let mut grid: SpatialGrid<Int> = SpatialGrid::with_spacing(dim, &spacing, &center).unwrap();
    let mut pids = Vec::new();
    for (p, x) in pts.chunks_exact(dim).enumerate() {
        let id = grid.cell_id(x).unwrap();
        pids.extend(id.ids().iter().map(|&i| i64::from(i)));
        grid.insert(Int::try_from(p).unwrap(), x).unwrap();
    }
    c.i64("point_cell_ids", &pids);
    check_grid(&mut c, &grid, |&i| i64::from(i));
    let p = c.params().clone();
    assert_eq!(p["type_name"], "int");
    assert_eq!(grid.print_self("int", 0), p["printself"].as_str().unwrap());
    assert_eq!(
        grid.print_self("int", 3),
        p["printself_indent3"].as_str().unwrap()
    );
    c.finish();
}

/// Cells, neighbours, bounds and saveAsMesh arrays (shared with the
/// geometry crate's test_grid replay, which duplicates this helper).
fn check_grid<T>(c: &mut Case, grid: &SpatialGrid<T>, item: impl Fn(&T) -> i64) {
    let mut ids = Vec::new();
    let mut offsets = vec![0i64];
    let mut items = Vec::new();
    let mut neighbors = Vec::new();
    for (id, cell) in grid.cells() {
        ids.extend(id.ids().iter().map(|&i| i64::from(i)));
        items.extend(cell.iter().map(&item));
        offsets.push(i64::try_from(items.len()).unwrap());
        for n in id.neighbors() {
            neighbors.extend(n.ids().iter().map(|&i| i64::from(i)));
        }
    }
    c.i64("cells/ids", &ids);
    c.i64("cells/offsets", &offsets);
    c.i64("cells/items", &items);
    c.i64("cells/neighbors", &neighbors);
    c.f64("lower_bounds", grid.lower_bounds());
    c.f64("upper_bounds", grid.upper_bounds());
    c.bits("lower_bounds", grid.lower_bounds());
    c.bits("upper_bounds", grid.upper_bounds());
    let m = grid.mesh_arrays(0).unwrap();
    c.f64("mesh/nodes", &m.nodes);
    c.bits("mesh/nodes", &m.nodes);
    let conn: Vec<i64> = m.connectivity.iter().map(|&i| i64::from(i)).collect();
    c.i64("mesh/connectivity", &conn);
    let tags: Vec<i64> = m.tags.iter().map(|&i| i64::from(i)).collect();
    c.i64("mesh/tag_1", &tags);
}

#[test]
fn grid_points_1d() {
    grid_points(1);
}

#[test]
fn grid_points_2d() {
    grid_points(2);
}

#[test]
fn grid_points_3d() {
    grid_points(3);
}

/// Non-integral bounding-box extents in every direction
/// (3.0000000000000004 cells), so `print_self`'s `ceil` is discriminated in
/// the directions after the first (mutant M17).
#[test]
fn grid_extent_2d() {
    grid_case("core_oracle_grid_extent_2d", "core/common/grid_extent_2d");
}

/// As [`grid_extent_2d`] with extents just below an integer
/// (6.999999999999999, 6, 5.999999999999999 cells).
#[test]
fn grid_extent_3d() {
    grid_case("core_oracle_grid_extent_3d", "core/common/grid_extent_3d");
}

/// Negative spacing and center −0.0 give a lower bound
/// and saved node coordinates equal to −0.0 (checked bit for bit).
#[test]
fn grid_signed_zero_2d() {
    grid_case(
        "core_oracle_grid_signed_zero_2d",
        "core/common/grid_signed_zero_2d",
    );
}

// --- BBox --------------------------------------------------------------------------

#[test]
fn bbox_operations() {
    let mut c = Case::open("core_oracle_bbox", "core/common/bbox");
    let offsets = c.input_i64("sets/offsets");
    let points = c.input_f64("sets/points");
    let mut boxes = Vec::new();
    for w in offsets.windows(2) {
        let (s, e) = (
            usize::try_from(w[0]).unwrap(),
            usize::try_from(w[1]).unwrap(),
        );
        let mut bb = BBox::empty(2);
        for p in points[2 * s..2 * e].as_chunks::<2>().0 {
            bb.add_point(p);
        }
        boxes.push(bb);
    }
    let flat = |f: &dyn Fn(&BBox) -> Vec<Real>| -> Vec<Real> { boxes.iter().flat_map(f).collect() };
    let lower = flat(&|x| x.lower.clone());
    let upper = flat(&|x| x.upper.clone());
    c.f64("boxes/lower", &lower);
    c.f64("boxes/upper", &upper);
    c.bits("boxes/lower", &lower);
    c.bits("boxes/upper", &upper);
    c.f64("boxes/size", &flat(&BBox::size));
    c.f64("boxes/center", &flat(&BBox::center));
    let empty: Vec<i64> = boxes.iter().map(|x| b(x.is_empty())).collect();
    c.i64("boxes/empty", &empty);

    let (mut inter, mut inter_dir, mut inter_empty, mut il, mut iu) =
        (vec![], vec![], vec![], vec![], vec![]);
    for a in &boxes {
        for o in &boxes {
            inter.push(b(a.intersects(o)));
            for d in 0..2 {
                inter_dir.push(b(a.intersects_in(o, d)));
            }
            let i = a.intersection(o);
            inter_empty.push(b(i.is_empty()));
            il.extend_from_slice(&i.lower);
            iu.extend_from_slice(&i.upper);
        }
    }
    c.i64("pairs/intersects", &inter);
    c.i64("pairs/intersects_dir", &inter_dir);
    c.i64("pairs/intersection_empty", &inter_empty);
    c.f64("pairs/intersection_lower", &il);
    c.f64("pairs/intersection_upper", &iu);
    // box 7 (signed zeros) gives −0.0 intersection bounds
    c.bits("pairs/intersection_lower", &il);
    c.bits("pairs/intersection_upper", &iu);

    let pts = c.input_f64("contains/points");
    let contains: Vec<i64> = boxes
        .iter()
        .flat_map(|x| pts.as_chunks::<2>().0.iter().map(move |p| b(x.contains(p))))
        .collect();
    c.i64("contains/result", &contains);

    let p = c.params().clone();
    let prints: Vec<String> = boxes.iter().map(ToString::to_string).collect();
    assert_eq!(prints, strs(&p["printself"]));
    let mut r = boxes[1].clone();
    r.reset();
    assert_eq!(r.to_string(), p["printself_after_reset"].as_str().unwrap());
    c.finish();
}

/// Upstream `%g` (6 digits) of the bounds that occur in the flagged boxes
/// of `bbox_emptiness` (test-only; the general formatter is crate-private).
fn g6(x: Real) -> &'static str {
    match x {
        x if x == Real::MAX => "1.79769e+308",
        x if x == -Real::MAX => "-1.79769e+308",
        1.0 => "1",
        _ => panic!("unexpected bound {x:e} in a flagged box"),
    }
}

/// UD-272: boxes that upstream flags non-empty although
/// their bounds keep the inverted empty-box values ±`DBL_MAX` in some
/// direction — `setLowerBounds`/`setUpperBounds` with a margin on an empty
/// box (contact detector, empty node list), the box of a grid without
/// cells (grid synchroniser), NaN points, `allSum` of an empty box — and
/// every query on them. The port derives emptiness (`is_empty()` true for
/// these five), which changes only `empty()`, the printed text, and
/// `intersects` against a box of **infinite** extent; every other query
/// (against finite boxes and among themselves) must equal the oracle's.
#[test]
fn bbox_emptiness() {
    let mut c = Case::open("core_oracle_bbox_emptiness", "core/common/bbox_emptiness");
    let p = c.params().clone();
    let max_bb = p["max_bb"].as_f64().unwrap();
    let flagged = 5; // boxes 0..5 are the constructions
    // 0: ContactDetector::constructBoundingBox with an empty node list
    let mut b0 = BBox::empty(2);
    for l in &mut b0.lower {
        *l -= max_bb;
    }
    for u in &mut b0.upper {
        *u += max_bb;
    }
    // 1: GridSynchronizer's box of a grid without cells
    let g: SpatialGrid<Int> = SpatialGrid::with_spacing(2, &[0.2, 0.3], &[0.1, -0.2]).unwrap();
    let b1 = BBox {
        lower: g
            .lower_bounds()
            .iter()
            .zip(g.spacing())
            .map(|(l, s)| l - s)
            .collect(),
        upper: g
            .upper_bounds()
            .iter()
            .zip(g.spacing())
            .map(|(u, s)| u + s)
            .collect(),
    };
    // 2, 3: NaN points (std::min/std::max keep the bounds)
    let mut b2 = BBox::empty(2);
    b2.add_point(&[f64::NAN, f64::NAN]);
    let mut b3 = BBox::empty(2);
    b3.add_point(&[f64::NAN, 1.0]);
    // 4: allSum of an empty box on one rank: Min of [lower, −1·upper], then
    // −1· again (aka_bbox.hh:172-185)
    let e = BBox::empty(2);
    let b4 = BBox {
        lower: e.lower.clone(),
        // −1·(−1·u) = −(−u) exactly for the finite ±DBL_MAX bounds
        upper: e.upper.iter().map(|&u| -(-u)).collect(),
    };
    let mut boxes = vec![b0, b1, b2, b3, b4];
    let offsets = c.input_i64("sets/offsets");
    let points = c.input_f64("sets/points");
    for w in offsets.windows(2) {
        let (s, e) = (
            usize::try_from(w[0]).unwrap(),
            usize::try_from(w[1]).unwrap(),
        );
        let mut bb = BBox::empty(2);
        for q in points[2 * s..2 * e].as_chunks::<2>().0 {
            bb.add_point(q);
        }
        boxes.push(bb);
    }
    let nb = boxes.len();
    assert_eq!(strs(&p["kinds"]).len(), nb);
    let flat = |f: &dyn Fn(&BBox) -> Vec<Real>| -> Vec<Real> { boxes.iter().flat_map(f).collect() };
    for (q, v) in [
        ("boxes/lower", flat(&|x| x.lower.clone())),
        ("boxes/upper", flat(&|x| x.upper.clone())),
        ("boxes/size", flat(&BBox::size)),
        ("boxes/center", flat(&BBox::center)),
    ] {
        c.f64(q, &v);
        c.bits(q, &v);
    }
    // documented difference (UD-272): upstream's flag says non-empty for
    // the five constructions, the port's derived emptiness says empty
    for b in &boxes[..flagged] {
        assert!(b.is_empty(), "the port derives emptiness for {b:?}");
    }
    let empty_as_upstream: Vec<i64> = boxes
        .iter()
        .enumerate()
        .map(|(k, x)| if k < flagged { 0 } else { b(x.is_empty()) })
        .collect();
    c.i64("boxes/empty", &empty_as_upstream);
    let prints = strs(&p["printself"]);
    for (k, x) in boxes.iter().enumerate() {
        if k < flagged {
            // upstream prints the inverted bounds of a flagged box
            assert_eq!(x.to_string(), "BBox[]");
            let v = |a: &[Real]| {
                let items: Vec<String> = a.iter().map(|&y| format!("[{}]", g6(y))).collect();
                format!("[{}]", items.join(", "))
            };
            assert_eq!(
                prints[k],
                format!("BBox[{} - {}]", v(&x.lower), v(&x.upper))
            );
        } else {
            assert_eq!(x.to_string(), prints[k]);
        }
    }

    let (mut inter, mut inter_dir, mut inter_empty, mut il, mut iu) =
        (vec![], vec![], vec![], vec![], vec![]);
    for a in &boxes {
        for o in &boxes {
            inter.push(b(a.intersects(o)));
            for d in 0..2 {
                inter_dir.push(b(a.intersects_in(o, d)));
            }
            let i = a.intersection(o);
            inter_empty.push(b(i.is_empty()));
            il.extend_from_slice(&i.lower);
            iu.extend_from_slice(&i.upper);
        }
    }
    c.i64("pairs/intersects", &inter);
    c.i64("pairs/intersects_dir", &inter_dir);
    c.i64("pairs/intersection_empty", &inter_empty);
    c.f64("pairs/intersection_lower", &il);
    c.f64("pairs/intersection_upper", &iu);
    c.bits("pairs/intersection_lower", &il);
    c.bits("pairs/intersection_upper", &iu);

    let pts = c.input_f64("contains/points");
    let contains: Vec<i64> = boxes
        .iter()
        .flat_map(|x| pts.as_chunks::<2>().0.iter().map(move |q| b(x.contains(q))))
        .collect();
    c.i64("contains/result", &contains);

    // the box [−∞, ∞]²: per-direction tests ignore emptiness on both
    // sides (equal); the full test differs for the five constructions
    // (upstream: both flagged non-empty and every inverted ±DBL_MAX
    // interval lies inside (−∞, ∞), so true; port: empty, so false —
    // UD-272)
    let mut f = BBox::empty(2);
    f.add_point(&[f64::NEG_INFINITY, f64::NEG_INFINITY]);
    f.add_point(&[f64::INFINITY, f64::INFINITY]);
    let mut fi = Vec::new();
    let mut fi_dir = Vec::new();
    for (k, x) in boxes.iter().enumerate() {
        let (fx, xf) = (f.intersects(x), x.intersects(&f));
        if k < flagged {
            assert!(!fx && !xf, "the port's empty box intersects nothing");
            fi.extend([1, 1]);
        } else {
            fi.extend([b(fx), b(xf)]);
        }
        for d in 0..2 {
            fi_dir.extend([b(f.intersects_in(x, d)), b(x.intersects_in(&f, d))]);
        }
    }
    c.i64("infinite/intersects", &fi);
    c.i64("infinite/intersects_dir", &fi_dir);
    c.finish();
}

// --- Math --------------------------------------------------------------------------

#[test]
fn math_helpers() {
    let mut c = Case::open("core_oracle_math", "core/common/math");
    // reduce
    let offsets = c.input_i64("reduce/offsets");
    let inputs = c.ref_f64("reduce/inputs");
    let rows: Vec<Vec<Real>> = offsets
        .windows(2)
        .map(|w| inputs[usize::try_from(w[0]).unwrap()..usize::try_from(w[1]).unwrap()].to_vec())
        .collect();
    let results: Vec<Real> = rows.iter().map(|r| math::reduce(&mut r.clone())).collect();
    c.ragged("reduce/inputs", &rows);
    c.f64("reduce/results", &results);

    // Newton-Raphson
    let sqrt_a = c.input_f64("newton/sqrt_a");
    let cubic_b = c.input_f64("newton/cubic_b");
    let mut xs = Vec::new();
    let mut threw = Vec::new();
    let mut push = |r: akantu_core::Result<Real>| match r {
        Ok(x) => {
            xs.push(x);
            threw.push(0);
        }
        Err(_) => {
            xs.push(f64::NAN);
            threw.push(1);
        }
    };
    for &a in &sqrt_a {
        push(NewtonRaphson::new(1e-12, 100).solve(|x| x * x - a, |x| 2. * x, 1.));
    }
    for &bb in &cubic_b {
        push(NewtonRaphson::new(1e-5, 100).solve(
            |x| x * x * x - 2. * x - bb,
            |x| 3. * x * x - 2.,
            2.,
        ));
    }
    push(NewtonRaphson::new(0., 3).solve(|x| x * x - 2., |x| 2. * x, 1.));
    c.f64("newton/x", &xs);
    c.i64("newton/threw", &threw);

    // pow
    let px = c.input_f64("pow/x");
    let pv: Vec<Real> = px
        .iter()
        .flat_map(|&x| [0, 1, 2, 3, 4, 7, 5, 9].map(|p| pow_n(x, p)))
        .collect();
    c.f64("pow/values", &pv);

    // are_float_equal (default tolerance, then 1e-15)
    assert_eq!(
        c.params()["default_tolerance"].as_f64().unwrap().to_bits(),
        DEFAULT_TOLERANCE.to_bits()
    );
    let pairs = c.input_f64("float_equal/pairs");
    let eq: Vec<i64> = [DEFAULT_TOLERANCE, 1e-15]
        .iter()
        .flat_map(|&tol| {
            pairs
                .as_chunks::<2>()
                .0
                .iter()
                .map(move |p| b(are_float_equal(p[0], p[1], tol)))
        })
        .collect();
    c.i64("float_equal/result", &eq);

    // intersects / is_in_range
    let iv = c.input_f64("intervals/values");
    let ir: Vec<i64> = iv
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| b(math::intersects(v[0], v[1], v[2], v[3])))
        .collect();
    let rr: Vec<i64> = iv
        .as_chunks::<4>()
        .0
        .iter()
        .map(|v| b(is_in_range(v[0], v[2], v[3])))
        .collect();
    c.i64("intervals/intersects", &ir);
    c.i64("intervals/is_in_range", &rr);
    c.finish();
}

// --- strings and version -----------------------------------------------------

#[test]
fn strings_and_version() {
    let mut c = Case::open("core_oracle_strings", "core/common/strings");
    let sizes = c.input_i64("memory/sizes");
    let p = c.params().clone();
    let su: Vec<usize> = sizes.iter().map(|&s| usize::try_from(s).unwrap()).collect();
    let real: Vec<String> = su.iter().map(|&s| print_memory_size::<Real>(s)).collect();
    let chr: Vec<String> = su.iter().map(|&s| print_memory_size::<u8>(s)).collect();
    let uint: Vec<String> = su.iter().map(|&s| print_memory_size::<UInt>(s)).collect();
    assert_eq!(real, strs(&p["memory_real"]));
    assert_eq!(chr, strs(&p["memory_char"]));
    assert_eq!(uint, strs(&p["memory_uint"]));
    let input = strs(&p["strings"]);
    let low: Vec<String> = input.iter().map(|s| to_lower(s)).collect();
    let tr: Vec<String> = input.iter().map(|s| trim(s).to_owned()).collect();
    let trq: Vec<String> = input.iter().map(|s| trim_char(s, '"').to_owned()).collect();
    assert_eq!(low, strs(&p["to_lower"]));
    assert_eq!(tr, strs(&p["trim"]));
    assert_eq!(trq, strs(&p["trim_quote"]));
    // version: the oracle compiles a patched copy of the tree, hence its
    // `.dirty` build metadata (akantu_core::version module documentation)
    let oracle = p["version"].as_str().unwrap();
    assert_eq!(oracle.strip_suffix(".dirty"), Some(version::get_version()));
    assert_eq!(p["version_number"], u64::from(version::AKANTU_VERSION));
    assert_eq!(p["version_major"], u64::from(version::AKANTU_VERSION_MAJOR));
    assert_eq!(p["version_minor"], u64::from(version::AKANTU_VERSION_MINOR));
    assert_eq!(p["version_patch"], u64::from(version::AKANTU_VERSION_PATCH));
    assert_eq!(p["version_prerelease"], version::AKANTU_VERSION_PRERELEASE);
    c.finish_parameters_only();
}

// --- CSR and CircularArray -----------------------------------------------------

#[test]
fn csr_and_circular_array() {
    let mut c = Case::open("core_oracle_csr_circular", "core/common/csr_circular");
    let counts = c.input_i64("csr/counts");
    let mut csr: Csr<i64> = Csr::new(counts.len());
    for (r, &k) in counts.iter().enumerate() {
        for _ in 0..k {
            *csr.row_offset_mut(r).unwrap() += 1;
        }
    }
    csr.count_to_csr();
    let after: Vec<i64> = csr
        .offsets()
        .iter()
        .map(|&o| i64::try_from(o).unwrap())
        .collect();
    csr.resize_cols();
    csr.begin_insertions();
    let mut left = counts.clone();
    let (mut rows, mut pos) = (vec![], vec![]);
    let mut val = 100;
    let mut any = true;
    while any {
        any = false;
        for (r, l) in left.iter_mut().enumerate() {
            if *l > 0 {
                rows.push(i64::try_from(r).unwrap());
                pos.push(i64::try_from(csr.insert_in_row(r, val).unwrap()).unwrap());
                val += 1;
                *l -= 1;
                any = true;
            }
        }
    }
    csr.end_insertions();
    let offsets: Vec<i64> = csr
        .offsets()
        .iter()
        .map(|&o| i64::try_from(o).unwrap())
        .collect();
    let nb_cols: Vec<i64> = (0..csr.nb_rows())
        .map(|r| i64::try_from(csr.nb_cols(r).unwrap()).unwrap())
        .collect();
    c.i64("csr/offsets_after_count", &after);
    c.i64("csr/insert_rows", &rows);
    c.i64("csr/insert_positions", &pos);
    c.i64("csr/offsets", &offsets);
    c.i64("csr/values", csr.values());
    c.i64("csr/nb_cols", &nb_cols);

    let mut ca = CircularArray::new(4, 2, 0.0, "ca").unwrap();
    for i in 0..4 {
        for j in 0..2 {
            *ca.get_mut(i, j).unwrap() = 10. * i as Real + j as Real + 0.5;
        }
    }
    let mut seen = Vec::new();
    for _ in 0..6 {
        for i in 0..4 {
            for j in 0..2 {
                seen.push(*ca.get(i, j).unwrap());
            }
        }
        ca.make_step();
    }
    c.f64("circular/values", &seen);
    c.finish();
}

// --- debug output (aka_error) and Array::printself ------------------------------

/// Replace the trailing `(<function>(): <file>:<line>)` of every message
/// line by `(LOC)` (locations are the oracle's C++ files and lines here,
/// Rust's in the port; their shape is checked separately).
fn mask_locations(text: &str) -> String {
    text.lines()
        .map(|l| match l.rfind(" (") {
            Some(i) if l.ends_with(')') && l[i..].contains("(): ") => {
                let loc = &l[i + 2..l.len() - 1];
                let (func, rest) = loc.split_once("(): ").unwrap();
                let (_file, line) = rest.rsplit_once(':').unwrap();
                assert!(
                    !func.is_empty() && line.bytes().all(|b| b.is_ascii_digit()),
                    "{l}"
                );
                format!("{} (LOC)\n", &l[..i])
            }
            _ => format!("{l}\n"),
        })
        .collect()
}

/// The port's process id and time stamps masked as the probe masks them.
fn mask_pid_ts(text: &str) -> String {
    let pid = format!("<{}>", std::process::id());
    let mut out = String::new();
    for l in text.split_inclusive('\n') {
        let l = l.replacen(&pid, "<PID>", 1);
        match (l.find('{'), l.find("} ")) {
            (Some(a), Some(b)) if a < b && l[a + 1..b].bytes().all(|c| c.is_ascii_digit()) => {
                out.push_str(&l[..=a]);
                out.push_str("TS");
                out.push_str(&l[b..]);
            }
            _ => out.push_str(&l),
        }
    }
    out
}

/// Upstream locations `(<function>(): <file>:<line>)` with the file and
/// line masked; the function name is kept (it is part of the contract).
fn mask_files(text: &str) -> String {
    text.lines()
        .map(|l| match l.rfind(" (") {
            Some(i) if l.ends_with(')') && l[i..].contains("(): ") => {
                let loc = &l[i + 2..l.len() - 1];
                let (func, rest) = loc.split_once("(): ").unwrap();
                let (_file, line) = rest.rsplit_once(':').unwrap();
                assert!(line.bytes().all(|b| b.is_ascii_digit()), "{l}");
                format!("{} ({func}(): FILE:LINE)\n", &l[..i])
            }
            _ => format!("{l}\n"),
        })
        .collect()
}

/// The probe's message sequence (`core_probe.cc` `case_debug_output` §2)
/// emitted **through the macros**, in a function of the same name as the
/// probe's (the macros print it in `AKANTU_DEBUG_IN`/`OUT` and in every
/// location). The debugger in effect is the caller's thread scope.
fn case_debug_output() {
    use akantu_core::debug::{self, DebugLevel};
    debug::set_parallel_context(3, 12);
    debug::set_debug_level(DebugLevel::INFO);
    akantu_core::aka_debug_warning!("value {} too large", 3);
    akantu_core::aka_debug_info!("plain info");
    akantu_core::aka_debug_trace!("hidden trace");
    akantu_core::aka_debug_in!();
    debug::set_debug_level(DebugLevel::DUMP);
    akantu_core::aka_debug_trace!("visible trace");
    akantu_core::aka_debug_in!();
    akantu_core::aka_debug_out!();
    akantu_core::aka_debug!(DebugLevel::ACCESSORY, "accessory {}", 9);
    debug::set_debug_level(DebugLevel::WARNING);
    // AKANTU_EXCEPTION("boom " << 1): the `###` line, then the throw
    akantu_core::aka_exception_message!("boom {}", 1);
    // AKANTU_SILENT_EXCEPTION("quiet"): nothing printed
    // AKANTU_ERROR("fatal " << 2): the `!!! ` line, then the throw
    akantu_core::aka_debug_error!("fatal {}", 2);
    debug::add_module_to_debug("mesh");
    debug::set_debug_level(DebugLevel::DUMP);
    akantu_core::aka_debug_info!("filtered out");
    akantu_core::aka_debug!(DebugLevel::CRITICAL, "critical ignores the filter");
    debug::remove_module_to_debug("mesh");
    debug::set_debug_level(DebugLevel::WARNING);
}

/// The oracle's message sequence through the port's
/// macros (`aka_debug_warning!`, `aka_debug_info!`, `aka_debug_trace!`,
/// `aka_debug_in!`, `aka_debug_out!`, `aka_debug!`, `aka_exception_message!`,
/// `aka_debug_error!`), each at the probe's debugger levels, compared with
/// the recorded text with only process ids, time stamps and source files
/// and lines masked: a changed prefix or level of any macro changes the
/// text (e.g. `"!!! "` → `"!!!"`, `"<=="` → `"==>"`, `aka_debug_trace!` at
/// `dblInfo` prints "hidden trace", `aka_exception_message!` at `dblInfo`
/// drops "boom 1"). The parallel contexts are emitted through
/// `aka_debug_warning!` too.
#[test]
fn debug_output_through_the_macros() {
    use akantu_core::debug::{self, DebuggerScope};
    let c = Case::open("core_oracle_debug_macros", "core/common/debug_output");
    let p = c.params().clone();
    let mut d = Debugger::new();
    let out = d.set_out_buffer();
    let scope = DebuggerScope::new(d);
    case_debug_output();
    let messages = out.take();
    // the contexts: one `AKANTU_DEBUG_WARNING("ctx")` per (rank, size)
    let rs: Vec<i64> = p["contexts_rank_size"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let mut contexts = Vec::new();
    for w in rs.as_chunks::<2>().0 {
        debug::set_parallel_context(i32::try_from(w[0]).unwrap(), usize::try_from(w[1]).unwrap());
        akantu_core::aka_debug_warning!("ctx");
        let line = mask_pid_ts(&out.take());
        contexts.push(line[..line.find("{TS}").unwrap()].to_owned());
    }
    drop(scope);
    let expected = mask_files(p["messages"].as_str().unwrap());
    assert!(expected.contains("(case_debug_output(): FILE:LINE)"));
    assert_eq!(mask_files(&mask_pid_ts(&messages)), expected);
    assert_eq!(contexts, strs(&p["contexts"]));
}

/// Upstream's debug output (message prefixes,
/// parallel contexts, levels, module filter, exception and error lines,
/// `testLevel`, `printBacktrace` layout, `setLogFile`) and
/// `Array::printself` texts, compared as text (process ids, time stamps,
/// addresses and source locations masked).
#[test]
fn debug_output() {
    let mut c = Case::open("core_oracle_debug_output", "core/common/debug_output");
    let p = c.params().clone();

    // 1. Array::printself: same arrays, levels, indents and precisions
    let mut real = Array::from_vec(
        vec![
            1. / 3.,
            -0.,
            2.5e-7,
            1e20,
            f64::NAN,
            f64::NEG_INFINITY,
            123_456_789.,
            1e-5,
        ],
        2,
    )
    .unwrap();
    real.set_id("real");
    let mut ints = Array::from_vec(vec![3_i32, -4, 5], 1).unwrap();
    ints.set_id("ints");
    let mut uints = Array::from_vec(vec![0_u32, 1, 2, 3, 4, UInt::MAX], 3).unwrap();
    uints.set_id("uints");
    let mut elements: Array<Element> = Array::with_id(1, "elements");
    elements.push_value(Element::new(ElementType::Triangle3, 0, GhostType::NotGhost));
    let empty: Array<Real> = Array::with_id(3, "empty");
    let mut pushed: Array<Real> = Array::with_id(2, "pushed");
    for k in 0..3_u8 {
        pushed.push_tuple(&[Real::from(k), Real::from(k) + 0.5]);
    }
    for rec in p["printself"].as_array().unwrap() {
        let text = rec["text"].as_str().unwrap();
        // upstream's allocation (std::vector growth) is an input of the
        // printer: read it from the recorded text
        let allocated: usize = text
            .lines()
            .find_map(|l| l.trim().strip_prefix("+ allocated size : "))
            .unwrap()
            .parse()
            .unwrap();
        let opts = ArrayPrint {
            indent: usize::try_from(rec["indent"].as_u64().unwrap()).unwrap(),
            debug_level: Some(DebugLevel(
                i32::try_from(rec["level"].as_i64().unwrap()).unwrap(),
            )),
            allocated_size: Some(allocated),
            precision: usize::try_from(rec["precision"].as_u64().unwrap()).unwrap(),
            ..ArrayPrint::default()
        };
        let mine = match rec["name"].as_str().unwrap() {
            "real" => real.print_self(&opts),
            "ints" => ints.print_self(&opts),
            "uints" => uints.print_self(&opts),
            "elements" => elements.print_self(&opts),
            "empty" => empty.print_self(&opts),
            "pushed" => pushed.print_self(&opts),
            other => panic!("unknown array {other}"),
        };
        let mine: String = mine
            .split_inclusive('\n')
            .map(|l| match l.split_once(" + address        : ") {
                Some((sp, _)) => format!("{sp} + address        : ADDR\n"),
                None => l.to_owned(),
            })
            .collect();
        assert_eq!(mine, text, "{rec}");
    }

    // 2. messages: the same sequence through the port's debugger
    let module = p["message_module"].as_str().unwrap().to_owned();
    let mut d = Debugger::new();
    let out = d.set_out_buffer();
    let emit = |d: &Debugger, prefix: &str, level: DebugLevel, msg: &str, module: &str| {
        d.print_message(prefix, level, &format!("{msg} (LOC)"), module);
    };
    d.set_parallel_context(3, 12);
    d.set_debug_level(DebugLevel::INFO);
    emit(
        &d,
        "/!\\",
        DebugLevel::WARNING,
        "value 3 too large",
        &module,
    );
    emit(&d, "---", DebugLevel::INFO, "plain info", &module);
    emit(&d, ">>>", DebugLevel::TRACE, "hidden trace", &module);
    emit(&d, "==>", DebugLevel::IN, "case_debug_output()", &module);
    d.set_debug_level(DebugLevel::DUMP);
    emit(&d, ">>>", DebugLevel::TRACE, "visible trace", &module);
    emit(&d, "==>", DebugLevel::IN, "case_debug_output()", &module);
    emit(&d, "<==", DebugLevel::OUT, "case_debug_output()", &module);
    emit(&d, "   ", DebugLevel::ACCESSORY, "accessory 9", &module);
    d.set_debug_level(DebugLevel::WARNING);
    emit(&d, "###", DebugLevel::WARNING, "boom 1", &module); // AKANTU_EXCEPTION
    emit(&d, "!!! ", DebugLevel::ERROR, "fatal 2", &module); // AKANTU_ERROR
    d.add_module_to_debug("mesh");
    d.set_debug_level(DebugLevel::DUMP);
    emit(&d, "---", DebugLevel::INFO, "filtered out", &module);
    emit(
        &d,
        "   ",
        DebugLevel::CRITICAL,
        "critical ignores the filter",
        &module,
    );
    d.remove_module_to_debug("mesh");
    d.set_debug_level(DebugLevel::WARNING);
    assert_eq!(
        mask_pid_ts(&out.take()),
        mask_locations(p["messages"].as_str().unwrap())
    );
    assert_eq!(p["exception_what"], "boom 1");
    assert!(
        p["exception_info"]
            .as_str()
            .unwrap()
            .starts_with("akantu::debug::Exception : boom 1 [")
    );
    let rs: Vec<i64> = p["contexts_rank_size"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    let mut contexts = Vec::new();
    for w in rs.as_chunks::<2>().0 {
        d.set_parallel_context(i32::try_from(w[0]).unwrap(), usize::try_from(w[1]).unwrap());
        contexts.push(mask_pid_ts(d.parallel_context()));
    }
    assert_eq!(contexts, strs(&p["contexts"]));

    // 3. testLevel
    let levels = c.input_i64("test_level/levels");
    let dlevels = c.input_i64("test_level/debugger_levels");
    let mut tl = Vec::new();
    for &dl in &dlevels {
        let mut t = Debugger::new();
        t.set_debug_level(DebugLevel(i32::try_from(dl).unwrap()));
        for filter in [false, true] {
            if filter {
                t.add_module_to_debug("core");
            }
            for m in ["core", "mesh"] {
                for &l in &levels {
                    tl.push(b(t.test_level(DebugLevel(i32::try_from(l).unwrap()), m)));
                }
            }
            if filter {
                t.remove_module_to_debug("core");
            }
        }
    }
    c.i64("test_level/result", &tl);

    // 4. printBacktrace layouts
    let bts: Vec<String> = [0usize, 1, 10]
        .iter()
        .map(|&n| format_backtrace(&(1..=n).map(|k| format!("frame{k}")).collect::<Vec<_>>()))
        .collect();
    assert_eq!(bts, strs(&p["backtraces"]));

    // 5. setLogFile
    let dir = std::env::temp_dir().join(format!("akantu-rs-core-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("log.txt");
    let mut f = Debugger::new();
    f.set_parallel_context(0, 1);
    f.set_log_file(&path).unwrap();
    emit(&f, "/!\\", DebugLevel::WARNING, "to the log file", &module);
    let logged = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(
        mask_pid_ts(&logged),
        mask_locations(p["log_file"].as_str().unwrap())
    );
    c.finish();
}
