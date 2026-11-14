// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS; distributions behind
// src/common/aka_random_generator.hh). Clean-room variant harness (D-030):
// replays the committed oracle fixtures through a copy of
// crates/akantu-core/src/random/distributions_impl.rs in which alternative
// operation orders can be switched on (make_variants.py), and reports which
// fixture sequences each alternative breaks. Usage: see README.md.

#![allow(dead_code, unused_imports, clippy::all)] // throw-away experiment harness
#[path = "../../../../../../crates/akantu-core/src/random/engine.rs"] mod engine;
#[path = "../../../../../../crates/akantu-core/src/random/libm.rs"] mod libm;
#[path = "../../../../../../crates/akantu-core/src/random/libm_tables.rs"] mod libm_tables;
#[path = "../../../../../../crates/akantu-core/src/random/fmt_c.rs"] mod fmt_c;
mod dist;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;
use dist::*;
use engine::*;
use serde_json::Value;

static VARIANTS: OnceLock<HashMap<String, u32>> = OnceLock::new();
pub fn v(key: &str) -> u32 { VARIANTS.get().unwrap().get(key).copied().unwrap_or(0) }

#[derive(Clone)]
enum D { U(UniformRealDistribution), E(ExponentialDistribution), G(GammaDistribution), W(WeibullDistribution), X(ExtremeValueDistribution), N(NormalDistribution), L(LognormalDistribution), C(ChiSquaredDistribution), Y(CauchyDistribution), F(FisherFDistribution), T(StudentTDistribution) }
impl D {
  fn sample(&mut self, g: &mut MinstdRand0) -> f64 { match self { D::U(d)=>d.sample(g), D::E(d)=>d.sample(g), D::G(d)=>d.sample(g), D::W(d)=>d.sample(g), D::X(d)=>d.sample(g), D::N(d)=>d.sample(g), D::L(d)=>d.sample(g), D::C(d)=>d.sample(g), D::Y(d)=>d.sample(g), D::F(d)=>d.sample(g), D::T(d)=>d.sample(g) } }
  fn printed(&self) -> String { let mut s = String::new(); match self { D::U(d)=>d.write_params(&mut s), D::E(d)=>d.write_params(&mut s), D::G(d)=>d.write_params(&mut s), D::W(d)=>d.write_params(&mut s), D::X(d)=>d.write_params(&mut s), D::N(d)=>d.write_params(&mut s), D::L(d)=>d.write_params(&mut s), D::C(d)=>d.write_params(&mut s), D::Y(d)=>d.write_params(&mut s), D::F(d)=>d.write_params(&mut s), D::T(d)=>d.write_params(&mut s) }; s }
}
fn std_dist(name: &str, p: &[f64]) -> D { match name {
  "uniform_real"|"uniform"|"not_defined" => D::U(UniformRealDistribution::new(p[0], p[1])), "exponential" => D::E(ExponentialDistribution::new(p[0])),
  "gamma" => D::G(GammaDistribution::new(p[0], p[1])), "weibull" => D::W(WeibullDistribution::new(p[0], p[1])),
  "extreme_value" => D::X(ExtremeValueDistribution::new(p[0], p[1])), "normal" => D::N(NormalDistribution::new(p[0], p[1])),
  "lognormal" => D::L(LognormalDistribution::new(p[0], p[1])), "chi_squared" => D::C(ChiSquaredDistribution::new(p[0])),
  "cauchy" => D::Y(CauchyDistribution::new(p[0], p[1])), "fisher_f" => D::F(FisherFDistribution::new(p[0], p[1])),
  "student_t" => D::T(StudentTDistribution::new(p[0])), o => panic!("{o}") } }
fn hex(v: &Value) -> f64 { f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap()) }
fn hexes(v: &Value) -> Vec<u64> { v.as_array().unwrap().iter().map(|x| u64::from_str_radix(x.as_str().unwrap(), 16).unwrap()).collect() }
fn fixture(n: &str) -> Value { serde_json::from_str(&std::fs::read_to_string(format!("{}/../../../../../crates/akantu-core/tests/fixtures/random/{n}", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap() }

fn main() {
  let arg = std::env::args().nth(1).unwrap_or_default();
  let m: HashMap<String, u32> = arg.split(',').filter(|s| !s.is_empty()).map(|kv| { let (k, v) = kv.split_once('=').unwrap(); (k.to_owned(), v.parse().unwrap()) }).collect();
  VARIANTS.set(m).unwrap();
  // name -> (failed sequences, total sequences, failed values, total values)
  let mut stats: BTreeMap<String, (u32, u32, u64, u64)> = BTreeMap::new();
  let mut rec = |k: &str, got: &[u64], exp: &[u64], ok_extra: bool| { let e = stats.entry(k.to_owned()).or_default(); let bad = got.iter().zip(exp).filter(|(a, b)| a != b).count() as u64; e.1 += 1; e.3 += exp.len() as u64; e.2 += bad; if bad > 0 || !ok_extra { e.0 += 1; } };
  let f = fixture("distributions.json");
  for c in f["cases"].as_array().unwrap() {
    let name = c["dist"].as_str().unwrap(); let p: Vec<f64> = c["params"].as_array().unwrap().iter().map(hex).collect();
    let seed: i64 = c["seed"].as_str().unwrap().parse().unwrap();
    let mut d = std_dist(name, &p); let mut e = MinstdRand0::with_seed_long(seed); let exp = hexes(&c["values"]);
    let got: Vec<u64> = (0..exp.len()).map(|_| d.sample(&mut e).to_bits()).collect();
    let extra = c["state_after"] == e.state() && c["printed"] == d.printed();
    rec(name, &got, &exp, extra);
  }
  let f = fixture("random_parameter.json");
  for c in f["make"].as_array().unwrap() {
    let ty = c["type"].as_str().unwrap(); let base = hex(&c["base"]); let (a, b) = (hex(&c["a"]), hex(&c["b"]));
    let (a, b) = match c["nparams"].as_u64().unwrap() { 0 => (0.0, 0.0), 1 => (a, 0.0), _ => (a, b) };
    let mut d = match ty { "not_defined" => std_dist("uniform", &[0.0, 0.0]), "weibull" => std_dist("weibull", &[b, a]), t => std_dist(t, &[a, b]) };
    let mut e = MinstdRand0::with_seed_long(1);
    let (mut got, mut exp, mut extra) = (vec![], vec![], true);
    for ch in c["chunks"].as_array().unwrap() { let x = hexes(&ch["values"]); for _ in 0..x.len() { got.push((base + d.sample(&mut e)).to_bits()); } exp.extend(x); extra &= ch["state"] == e.state(); }
    rec(&format!("param:{ty}"), &got, &exp, extra);
  }
  // interleave script (same as dump_random.cc)
  {
    let evs = f["interleave"].as_array().unwrap(); let mut i = 0usize;
    let mut e = MinstdRand0::with_seed_long(2024);
    let mut pn = (10.0, std_dist("normal", &[0.0, 1.0])); let mut pg = (0.0, std_dist("gamma", &[0.7, 2.0])); let mut pu = (3.0, std_dist("uniform", &[0.0, 0.0])); let mut pl = (0.0, std_dist("lognormal", &[0.0, 0.3]));
    let mut ok = true;
    let mut check = |p: &mut (f64, D), e: &mut MinstdRand0| { let ev = &evs[i]; i += 1; let x = hexes(&ev["values"]); for xv in x { ok &= (p.0 + p.1.sample(e)).to_bits() == xv; } ok &= ev["state"] == e.state(); };
    check(&mut pn, &mut e); check(&mut pg, &mut e); check(&mut pu, &mut e);
    let mut nc = pn.clone(); check(&mut pn, &mut e); check(&mut nc, &mut e);
    check(&mut pl, &mut e); let mut lc = pl.clone(); check(&mut lc, &mut e); check(&mut pl, &mut e);
    pn = pg.clone(); check(&mut pn, &mut e); check(&mut pg, &mut e); check(&mut pu, &mut e);
    let s = stats.entry("interleave".into()).or_default(); s.1 += 1; if !ok { s.0 += 1; }
  }
  let mut line = vec![];
  for (k, (fs, ts, fv, tv)) in &stats { if *fs > 0 { line.push(format!("{k}: {fs}/{ts} seq, {fv}/{tv} values")); } }
  println!("[{arg}] {}", if line.is_empty() { "ALL MATCH".to_string() } else { format!("FAIL: {}", line.join("; ")) });
}
