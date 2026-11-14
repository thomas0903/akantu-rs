// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): verification of the pure-Rust
// ports of the oracle's glibc 2.41 sin/cos/sincos against
// oracle/probes/random/libm_sincos.cc (fixtures/random/libm_sincos.json), and
// of the derived Drucker–Prager constants of
// src/model/solid_mechanics/materials/material_plastic/material_drucker_prager.cc.

//! Bit equality of `akantu_core::glibc_libm::{sin,cos,sincos}_checked` with
//! the oracle's glibc on 2²⁰ inputs per domain (7 domains covering every
//! branch of `__sin`, `__cos`, `__sincos` below the `__branred` range) and
//! on the 8,999 friction angles `phi = i·0.01°` (the angle,
//! sine, cosine and the derived `alpha`, `k` with `fc = 1e7`).

use akantu_core::glibc_libm::{cos_checked, sin_checked, sincos_checked};
use serde_json::Value;

fn fixture() -> Value {
    let path = format!(
        "{}/tests/fixtures/random/libm_sincos.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn h(v: &Value) -> u64 {
    u64::from_str_radix(v.as_str().expect("hex string"), 16).expect("hex")
}

/// NaN-ness only: the documented D-046 exception for the libm
/// special-value tables (NUMERICAL_VALIDATION);
/// every non-NaN value stays bitwise.
fn canon(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

/// splitmix64, identical to libm_sincos.cc.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn signed_range(&mut self, lo: u64, hi: u64) -> f64 {
        let sign = self.next() & 0x8000_0000_0000_0000;
        f64::from_bits((lo + self.next() % (hi - lo + 1)) | sign)
    }
}

/// FNV-1a over the 8 little-endian bytes of each value.
struct Fnv(u64);

impl Fnv {
    fn add(&mut self, v: u64) {
        for i in 0..8 {
            self.0 ^= (v >> (8 * i)) & 0xff;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[test]
fn sin_cos_sincos_hashes_match_glibc() {
    let fx = fixture();
    let n = usize::try_from(fx["n"].as_u64().unwrap()).unwrap();
    let domains = fx["domains"].as_array().unwrap();
    assert_eq!(domains.len(), 7);
    for d in domains {
        let name = d["domain"].as_str().unwrap();
        // the signed_range draw order of the probe: sign first, then range
        let mut rng = SplitMix64(d["seed"].as_u64().unwrap());
        let (lo, hi) = (h(&d["lo"]), h(&d["hi"]));
        let mut hs = [Fnv(0xcbf2_9ce4_8422_2325), Fnv(0xcbf2_9ce4_8422_2325)];
        let mut hc = [Fnv(0xcbf2_9ce4_8422_2325), Fnv(0xcbf2_9ce4_8422_2325)];
        let first: Vec<u64> = d["first"].as_array().unwrap().iter().map(h).collect();
        for i in 0..n {
            let x = rng.signed_range(lo, hi);
            let s = sin_checked(x).unwrap();
            let c = cos_checked(x).unwrap();
            let (ss, sc) = sincos_checked(x).unwrap();
            if i < first.len() / 5 {
                let got = [x, s, c, ss, sc].map(canon);
                assert_eq!(&got[..], &first[i * 5..i * 5 + 5], "{name}: sample {i}");
            }
            hs[0].add(canon(s));
            hc[0].add(canon(c));
            hs[1].add(canon(ss));
            hc[1].add(canon(sc));
        }
        assert_eq!(hs[0].0, h(&d["sin"]), "{name}: sin hash");
        assert_eq!(hc[0].0, h(&d["cos"]), "{name}: cos hash");
        assert_eq!(hs[1].0, h(&d["sincos_sin"]), "{name}: sincos sine hash");
        assert_eq!(hc[1].0, h(&d["sincos_cos"]), "{name}: sincos cosine hash");
    }
}

/// A sweep of 8,999 friction angles (the host libm differs
/// from glibc on 316 `alpha` and 455 `k` values); the port must agree with
/// the oracle on every value, and the Drucker–Prager law must derive the
/// same constants.
#[test]
fn drucker_prager_angle_sweep_matches_glibc() {
    let fx = fixture();
    let sw = &fx["drucker_prager_sweep"];
    let arr = |k: &str| -> Vec<u64> { sw[k].as_array().unwrap().iter().map(h).collect() };
    let fc = f64::from_bits(h(&sw["fc"]));
    let (phi, rad) = (arr("phi"), arr("phi_radian"));
    let (sin, cos, ss, sc) = (arr("sin"), arr("cos"), arr("sincos_sin"), arr("sincos_cos"));
    let (alpha, k) = (arr("alpha"), arr("k"));
    assert_eq!(phi.len(), 8999);
    let mut host_differences = 0;
    for i in 0..8999 {
        #[allow(clippy::cast_precision_loss)] // small integers
        let p = (i + 1) as f64 * 0.01;
        // oracle records: exact under D-046 (finite here)
        let same = |got: f64, want: u64, what: &str| {
            assert!(
                akantu_validation::exact::exact_bits_eq(want, got.to_bits()),
                "{what}, phi {p}: port {got:e} ({:#018x}), oracle {want:#018x}",
                got.to_bits()
            );
        };
        same(p, phi[i], "phi");
        let r = p * std::f64::consts::PI / 180.;
        same(r, rad[i], "phi_radian");
        same(sin_checked(r).unwrap(), sin[i], "sin");
        same(cos_checked(r).unwrap(), cos[i], "cos");
        let (s, c) = sincos_checked(r).unwrap();
        same(s, ss[i], "sincos sine");
        same(c, sc[i], "sincos cosine");
        let a = (6. * s) / (3. - s);
        let cohesion = fc * (1. - s) / (2. * c);
        let kk = (6. * cohesion * c) / (3. - s);
        same(a, alpha[i], "alpha");
        same(kk, k[i], "k");
        if r.sin().to_bits() != s.to_bits() || r.cos().to_bits() != c.to_bits() {
            host_differences += 1;
        }
    }
    // informative: how often the host libm would have differed
    eprintln!("host libm differs from glibc sincos on {host_differences} of 8999 angles");
}
