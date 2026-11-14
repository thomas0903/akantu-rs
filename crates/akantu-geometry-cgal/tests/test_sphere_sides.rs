// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042) because it links akantu-geometry-cgal;
// it contains no CGAL-derived code. Checks of the spherical kernel's sphere
// side predicates used by Akantu's IGFEM
// (extra_packages/igfem/src/igfem_enrichment_inline_impl.hh, `isInside`:
// `not sphere.has_on_unbounded_side(SK::Point_3(x, y, 0))`).
// No upstream test exists for them.

//! `SkSphere::{bounded_side, has_on_bounded_side, has_on_boundary,
//! has_on_unbounded_side}` against an independent exact reference in
//! scaled 128-bit integers, on points within a few ulps of the sphere,
//! points exactly on it and points where the naive `f64` evaluation of
//! `|p − c|² ⋚ r²` gives the wrong side.

use std::cmp::Ordering;

use akantu_geometry_cgal::spherical::{BoundedSide, SkPoint, SkSphere};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        // 53 random bits in [0, 1).
        #[allow(clippy::cast_precision_loss)] // exact: < 2^53
        let m = (self.next() >> 11) as f64;
        m * f64::from_bits(0x3CA0_0000_0000_0000) // 2^-53
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// `x · 2^60` as an exact integer; `x` must be 0 or have `2^-8 <= |x| < 4`
/// (then its ulp is at least `2^-60`).
fn fixed60(x: f64) -> i128 {
    assert!(x == 0.0 || (x.abs() >= 1.0 / 256.0 && x.abs() < 4.0), "{x}");
    let s = x * f64::from_bits(0x43B0_0000_0000_0000); // 2^60, exact scaling
    assert_eq!(s.fract(), 0.0);
    #[allow(clippy::cast_possible_truncation)] // exact: integral, |s| < 2^62
    let v = s as i128;
    v
}

/// `r² · 2^120` as an exact integer (`2^-68 <= r² < 16`).
fn fixed120(r2: f64) -> i128 {
    assert!((f64::from_bits(0x3BB0_0000_0000_0000)..16.0).contains(&r2)); // 2^-68
    let s = r2 * f64::from_bits(0x4770_0000_0000_0000); // 2^120
    assert_eq!(s.fract(), 0.0);
    #[allow(clippy::cast_possible_truncation)] // exact: integral, < 2^124
    let v = s as i128;
    v
}

/// Exact `compare(r², |p − c|²)` in units of `2^-120`.
fn reference(c: [f64; 3], r2: f64, p: [f64; 3]) -> Ordering {
    let d2: i128 = (0..3)
        .map(|k| {
            let d = fixed60(c[k]) - fixed60(p[k]);
            d * d
        })
        .sum();
    fixed120(r2).cmp(&d2)
}

fn naive(c: [f64; 3], r2: f64, p: [f64; 3]) -> Ordering {
    let d2 = (c[0] - p[0]) * (c[0] - p[0])
        + (c[1] - p[1]) * (c[1] - p[1])
        + (c[2] - p[2]) * (c[2] - p[2]);
    r2.partial_cmp(&d2).unwrap()
}

/// Snap `x` to 0 or to `[2^-8, 4)` in magnitude (the reference's domain).
fn domain(x: f64) -> f64 {
    if x.abs() < 1.0 / 256.0 { 0.0 } else { x }
}

fn check(c: [f64; 3], r2: f64, p: [f64; 3], counts: &mut [usize; 4]) {
    let s = SkSphere::try_from_f64(c, r2).unwrap();
    let q = SkPoint::try_from_f64(p).unwrap();
    let want = reference(c, r2, p);
    let side = s.bounded_side(&q).unwrap();
    let expected_side = match want {
        Ordering::Less => BoundedSide::OnUnboundedSide,
        Ordering::Equal => BoundedSide::OnBoundary,
        Ordering::Greater => BoundedSide::OnBoundedSide,
    };
    assert_eq!(side, expected_side, "c {c:?} r² {r2:e} p {p:?}");
    assert_eq!(s.has_on_unbounded_side(&q).unwrap(), want == Ordering::Less);
    assert_eq!(s.has_on_boundary(&q).unwrap(), want == Ordering::Equal);
    assert_eq!(
        s.has_on_bounded_side(&q).unwrap(),
        want == Ordering::Greater
    );
    counts[match want {
        Ordering::Less => 0,
        Ordering::Equal => 1,
        Ordering::Greater => 2,
    }] += 1;
    if naive(c, r2, p) != want {
        counts[3] += 1;
    }
}

#[test]
fn sphere_sides_are_exact() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut counts = [0usize; 4]; // outside, on, inside, naive f64 wrong
    for i in 0..20_000 {
        let c = [
            domain(rng.unit() - 0.5),
            domain(rng.unit() - 0.5),
            if i % 2 == 0 {
                0.0
            } else {
                domain(rng.unit() - 0.5)
            },
        ];
        let r = 0.05 + rng.unit();
        let r2 = r * r;
        // A point at distance ≈ r along a random axis-aligned or diagonal
        // direction, then nudged by -3..=3 ulps in one coordinate.
        let (y, z) = (domain(0.6 * r * (rng.unit() - 0.5)), 0.0);
        let x = (r2 - y * y).sqrt();
        let mut p = [domain(c[0] + x), domain(c[1] + y), c[2] + z];
        let k = i64::try_from(rng.below(7)).unwrap() - 3;
        for _ in 0..k.unsigned_abs() {
            p[0] = if k > 0 {
                p[0].next_up()
            } else {
                p[0].next_down()
            };
        }
        p[0] = domain(p[0]);
        check(c, r2, p, &mut counts);
    }
    // Exact boundary points (Pythagorean triples scaled by powers of two).
    for (a, b, h, scale) in [
        (3.0, 4.0, 5.0, 0.125),
        (5.0, 12.0, 13.0, 0.0625),
        (8.0, 15.0, 17.0, 0.03125),
    ] {
        let c = [0.25, -0.5, 0.0];
        let r2 = (h * scale) * (h * scale);
        for (dx, dy) in [(a, b), (b, a), (-a, b), (a, -b), (h, 0.0), (0.0, -h)] {
            check(
                c,
                r2,
                [c[0] + dx * scale, c[1] + dy * scale, 0.0],
                &mut counts,
            );
        }
    }
    eprintln!(
        "outside {}, on {}, inside {}; naive f64 gives the wrong side in {} cases",
        counts[0], counts[1], counts[2], counts[3]
    );
    assert!(counts[0] > 1000 && counts[1] >= 18 && counts[2] > 1000);
    // The cases discriminate an exact predicate from the naive formula.
    assert!(counts[3] > 0);
}

/// IGFEM's `isInside`: inside or on the sphere is "inside"
/// (`!has_on_unbounded_side`).
#[test]
fn igfem_is_inside_pattern() {
    let s = SkSphere::try_from_f64([0.5, 0.5, 0.0], 0.0625).unwrap();
    let inside = |x: f64, y: f64| {
        !s.has_on_unbounded_side(&SkPoint::try_from_f64([x, y, 0.0]).unwrap())
            .unwrap()
    };
    assert!(inside(0.5, 0.5));
    assert!(inside(0.75, 0.5), "on the sphere counts as inside");
    assert!(!inside(0.75 + f64::EPSILON, 0.5));
    assert!(inside(0.75 - f64::EPSILON / 2.0, 0.5));
    assert!(!inside(0.0, 0.0));
}
