// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): verification of the pure-Rust
// ports of the oracle's glibc 2.41 log/exp/pow/tan (used by the libstdc++
// distributions behind src/common/aka_random_generator.hh) against reference
// hashes produced by oracle/probes/random/libm_vectors.cc on the oracle.

//! Bit equality of `random::glibc_libm` with the oracle's glibc.
//!
//! * Always: the 2¹⁶-input hash of every (function, domain), the first
//!   input/output pairs and the special inputs of `libm.json`.
//! * `AKANTU_RNG_LIBM_LONG=1`: also the 2²²-input hashes.
//! * `AKANTU_RNG_LIBM_EXHAUSTIVE=<n>` on Linux/glibc 2.41: compares the ports
//!   with the host libm directly on `n` inputs per domain (meaningful only on
//!   the oracle platform, Debian trixie aarch64).

use akantu_core::random::glibc_libm::{exp, log, pow, tan_checked};
use serde_json::Value;

fn fixture() -> Value {
    let path = format!(
        "{}/tests/fixtures/random/libm.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn h(v: &Value) -> u64 {
    u64::from_str_radix(v.as_str().expect("hex string"), 16).expect("hex")
}

/// NaN-ness only, by design: the documented **D-046 exception** for the
/// libm special-value tables (docs/NUMERICAL_VALIDATION.md "Default-NaN
/// convention (D-046)"). The probe records every NaN
/// result as the canonical `0x7ff8000000000000`, and glibc's NaN
/// propagation for NaN *inputs* (which operand's payload, the quieting of
/// signalling NaNs, the sign) is platform-specific, so these tables compare
/// NaN as a class; every non-NaN result stays bitwise.
fn canon(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

/// splitmix64, identical to libm_vectors.cc.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next() % (hi - lo + 1)
    }
    fn signed_range(&mut self, lo: u64, hi: u64) -> f64 {
        let sign = self.next() & 0x8000_0000_0000_0000;
        f64::from_bits(self.range(lo, hi) | sign)
    }
}

fn b(x: f64) -> u64 {
    x.to_bits()
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn add(&mut self, v: u64) {
        for i in 0..8 {
            self.0 ^= (v >> (8 * i)) & 0xff;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

type Sample = (f64, f64, f64);
type Generator = fn(&mut SplitMix64, &Impl) -> Sample;

/// The functions under test (ports or, for the exhaustive mode, host libm).
struct Impl {
    log: fn(f64) -> f64,
    exp: fn(f64) -> f64,
    pow: fn(f64, f64) -> f64,
    tan: fn(f64) -> f64,
}

/// Adapter for the checked port: every generated tan domain lies inside the
/// ported range |x| <= 25 (see libm_vectors.cc), so `None` is a test failure.
fn tan_port(x: f64) -> f64 {
    tan_checked(x).expect("tan test domain inside the ported range")
}

const PORTS: Impl = Impl {
    log,
    exp,
    pow,
    tan: tan_port,
};

/// Same domains (name, seed, generator) as libm_vectors.cc.
fn domains() -> Vec<(&'static str, &'static str, u64, Generator)> {
    vec![
        ("log", "unit", 1, |r, f| {
            let x = f64::from_bits(r.range(b(f64::from_bits(0x3c3_0000000000000)), b(1.0)));
            (x, 0.0, (f.log)(x))
        }),
        ("log", "near_one", 2, |r, f| {
            let x = f64::from_bits(r.range(0x3fee_0000_0000_0000, 0x3ff1_0900_0000_0000));
            (x, 0.0, (f.log)(x))
        }),
        ("log", "positive", 3, |r, f| {
            let x = f64::from_bits(r.range(1, 0x7fef_ffff_ffff_ffff));
            (x, 0.0, (f.log)(x))
        }),
        ("log", "one_minus_u", 4, |r, f| {
            #[allow(clippy::cast_precision_loss)] // < 2^53: exact
            let u = (r.next() >> 11) as f64 * f64::from_bits(0x3ca0_0000_0000_0000);
            let x = 1.0 - u;
            (x, 0.0, (f.log)(x))
        }),
        ("exp", "range", 11, |r, f| {
            let x = r.signed_range(0x3c3_0000000000000, b(745.2));
            (x, 0.0, (f.exp)(x))
        }),
        ("exp", "moderate", 12, |r, f| {
            let x = r.signed_range(0, b(40.0));
            (x, 0.0, (f.exp)(x))
        }),
        ("exp", "any", 13, |r, f| {
            let x = f64::from_bits(r.next());
            (x, 0.0, (f.exp)(x))
        }),
        ("pow", "weibull", 21, |r, f| {
            let x = f64::from_bits(r.range(b(1e-17), b(40.0)));
            let a = f64::from_bits(r.range(b(0.05), b(100.0)));
            let y = 1.0 / a;
            (x, y, (f.pow)(x, y))
        }),
        ("pow", "gamma_small_alpha", 22, |r, f| {
            let x = f64::from_bits(r.range(b(1e-17), b(1.0) - 1));
            let a = f64::from_bits(r.range(b(1e-3), b(1.0) - 1));
            let y = 1.0 / a;
            (x, y, (f.pow)(x, y))
        }),
        ("pow", "positive_general", 23, |r, f| {
            let x = f64::from_bits(r.range(1, 0x7fef_ffff_ffff_ffff));
            let y = r.signed_range(0x3e1_0000000000000, 0x409_0000000000000);
            (x, y, (f.pow)(x, y))
        }),
        ("pow", "near_one", 24, |r, f| {
            let x = f64::from_bits(r.range(b(0.99), b(1.01)));
            let y = r.signed_range(b(1.0), b(1e6));
            (x, y, (f.pow)(x, y))
        }),
        ("pow", "any", 25, |r, f| {
            let x = f64::from_bits(r.next());
            let y = f64::from_bits(r.next());
            (x, y, (f.pow)(x, y))
        }),
        ("tan", "cauchy", 31, |r, f| {
            #[allow(clippy::cast_precision_loss)] // < 2^53: exact
            let u = (r.next() >> 11) as f64 * f64::from_bits(0x3ca0_0000_0000_0000);
            let x = std::f64::consts::PI * u;
            (x, 0.0, (f.tan)(x))
        }),
        ("tan", "small", 32, |r, f| {
            let x = r.signed_range(0x3d7_0000000000000, b(25.0));
            (x, 0.0, (f.tan)(x))
        }),
    ]
}

fn run_domain(gen_fn: Generator, seed: u64, n: usize, f: &Impl) -> (u64, Vec<Sample>) {
    let mut r = SplitMix64(seed);
    let mut hash = Fnv::new();
    let mut first = Vec::new();
    for i in 0..n {
        let s = gen_fn(&mut r, f);
        hash.add(canon(s.2));
        if i < 8 {
            first.push(s);
        }
    }
    (hash.0, first)
}

#[test]
fn generator_constants_match_probe() {
    // The bit patterns used above for 0x1p-60, 0x1p-30, 0x1p10, 0x1p-40, 0x1p-53.
    assert_eq!(f64::from_bits(0x3c3_0000000000000), 2f64.powi(-60));
    assert_eq!(f64::from_bits(0x3e1_0000000000000), 2f64.powi(-30));
    assert_eq!(f64::from_bits(0x409_0000000000000), 2f64.powi(10));
    assert_eq!(f64::from_bits(0x3d7_0000000000000), 2f64.powi(-40));
    assert_eq!(f64::from_bits(0x3ca0_0000_0000_0000), 2f64.powi(-53));
    assert_eq!(0x3fee_0000_0000_0000, b(1.0 - 2f64.powi(-4)));
    assert_eq!(
        0x3ff1_0900_0000_0000,
        b(1.0 + f64::from_bits(0x3fb0_9000_0000_0000))
    );
}

#[test]
fn libm_hashes_match_glibc() {
    let fx = fixture();
    assert_eq!(fx["meta"]["glibc"], "2.41");
    // Either authoritative oracle platform (D-043, D-073): the x86-64-v3
    // regeneration reproduces every value of these fixtures bit for bit
    // (class X0); only the metadata names the platform.
    assert!(
        matches!(fx["meta"]["arch"].as_str(), Some("aarch64" | "x86_64")),
        "{}",
        fx["meta"]["arch"]
    );
    let short = usize::try_from(fx["short"].as_u64().unwrap()).unwrap();
    let long = usize::try_from(fx["long"].as_u64().unwrap()).unwrap();
    let run_long = std::env::var_os("AKANTU_RNG_LIBM_LONG").is_some();
    let doms = domains();
    let refs = fx["domains"].as_array().unwrap();
    assert_eq!(doms.len(), refs.len());
    for ((func, dom, seed, g), reference) in doms.into_iter().zip(refs) {
        assert_eq!(reference["func"], func);
        assert_eq!(reference["domain"], dom);
        assert_eq!(reference["seed"], seed);
        let (hash, first) = run_domain(g, seed, short, &PORTS);
        for (i, (s, e)) in first
            .iter()
            .zip(reference["first"].as_array().unwrap())
            .enumerate()
        {
            let e = e.as_array().unwrap();
            assert_eq!(s.0.to_bits(), h(&e[0]), "{func}/{dom}: input {i}");
            assert_eq!(s.1.to_bits(), h(&e[1]), "{func}/{dom}: input {i}");
            assert_eq!(
                canon(s.2),
                h(&e[2]),
                "{func}/{dom}: {func}({:e}, {:e})",
                s.0,
                s.1
            );
        }
        assert_eq!(
            format!("{hash:016x}"),
            reference["hash_short"],
            "{func}/{dom}: 2^16 hash"
        );
        if run_long {
            let (hash, _) = run_domain(g, seed, long, &PORTS);
            assert_eq!(
                format!("{hash:016x}"),
                reference["hash_long"],
                "{func}/{dom}: 2^22 hash"
            );
        }
    }
}

#[test]
fn libm_special_inputs_match_glibc() {
    let fx = fixture();
    for e in fx["special_log"].as_array().unwrap() {
        let x = f64::from_bits(h(&e[0]));
        assert_eq!(canon(log(x)), h(&e[1]), "log({x:e})");
    }
    for e in fx["special_exp"].as_array().unwrap() {
        let x = f64::from_bits(h(&e[0]));
        assert_eq!(canon(exp(x)), h(&e[1]), "exp({x:e})");
    }
    for e in fx["special_pow"].as_array().unwrap() {
        let (x, y) = (f64::from_bits(h(&e[0])), f64::from_bits(h(&e[1])));
        assert_eq!(canon(pow(x, y)), h(&e[2]), "pow({x:e}, {y:e})");
    }
    for e in fx["special_tan"].as_array().unwrap() {
        let x = f64::from_bits(h(&e[0]));
        assert_eq!(
            canon(tan_checked(x).expect("vector inside the ported tan domain")),
            h(&e[1]),
            "tan({x:e})"
        );
    }
}

/// Direct comparison with the host libm, only meaningful on the oracle
/// platform (Debian trixie aarch64, glibc 2.41).
#[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "aarch64"))]
#[test]
fn libm_exhaustive_against_host_glibc() {
    let Some(n) = std::env::var("AKANTU_RNG_LIBM_EXHAUSTIVE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    else {
        return;
    };
    let host = Impl {
        log: f64::ln,
        exp: f64::exp,
        pow: f64::powf,
        tan: f64::tan,
    };
    for (func, dom, seed, g) in domains() {
        let mut r_port = SplitMix64(seed ^ 0x5eed);
        let mut r_host = SplitMix64(seed ^ 0x5eed);
        let mut mismatches = 0usize;
        for _ in 0..n {
            let p = g(&mut r_port, &PORTS);
            let q = g(&mut r_host, &host);
            if canon(p.2) != canon(q.2) {
                if mismatches < 10 {
                    eprintln!(
                        "{func}/{dom}: {func}({:e} [{:016x}], {:e}) port {:016x} host {:016x}",
                        p.0,
                        p.0.to_bits(),
                        p.1,
                        canon(p.2),
                        canon(q.2)
                    );
                }
                mismatches += 1;
            }
        }
        eprintln!("{func}/{dom}: {n} inputs, {mismatches} mismatches");
        assert_eq!(mismatches, 0, "{func}/{dom}");
    }
}
