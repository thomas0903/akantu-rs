// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room fixture generator (black-box). Writes oracle results of
// std::complex<double> division for inputs that discriminate every rule of
// the clean-room model (see COMPLEX_CLEANROOM_LOG.md), one record per line:
//   cdivx <tag> | a b c d | x y          (16-hex-digit IEEE-754 bit patterns)
// Tags: n normal range, t ties/signed zeros, r |r| around DBL_MIN, z r
// underflow, s1 max(|c|,|d|) around 2^-52, s2 small numerator part,
// s3 max(|c|,|d|) around DBL_MAX/2, g guard region, w whole range,
// p special-value grid sample, q special/finite mix.
// stderr: per-tag counts and the number of disagreements with model_v5
// (expected 0).
#include <map>
#include <string>
#include <vector>
#include "model_v5.h"

static std::map<std::string, std::pair<long, long>> stats;

static void rec(const char * tag, double a, double b, double c, double d) {
  C z = oracle_div(C(a, b), C(c, d)), w = model_v5(a, b, c, d);
  auto & s = stats[tag];
  ++s.first;
  if (!(same(w.real(), z.real()) && same(w.imag(), z.imag()))) ++s.second;
  std::printf("cdivx %s | %016llx %016llx %016llx %016llx | %016llx %016llx\n", tag, (unsigned long long)bits(a),
              (unsigned long long)bits(b), (unsigned long long)bits(c), (unsigned long long)bits(d),
              (unsigned long long)bits(z.real()), (unsigned long long)bits(z.imag()));
}

static double sig(Rng & g) { return 1.0 + double(g.next() >> 11) * 0x1p-53; }

int main() {
  Rng g{0x5eedc0de01ULL};
  // n: normal range (E1 distributions; fused vs unfused and branch choice)
  for (int k = 0; k < 600; ++k) {
    double a, b, c, d;
    if (k % 3 == 0) {
      a = g.uni() * std::pow(10.0, g.irange(-12, 12));
      b = g.uni() * std::pow(10.0, g.irange(-12, 12));
      c = g.uni() * std::pow(10.0, g.irange(-12, 12));
      d = g.uni() * std::pow(10.0, g.irange(-12, 12));
    } else {
      a = g.expo(-4, 4); b = g.expo(-4, 4); c = g.expo(-4, 4); d = g.expo(-4, 4);
    }
    if (c == 0 && d == 0) c = 1;
    rec("n", a, b, c, d);
  }
  // t: |c| == |d| ties and small integers (signed zeros)
  for (int k = 0; k < 200; ++k) {
    double a, b, c, d;
    if (k & 1) {
      a = g.irange(-3, 3); b = g.irange(-3, 3); c = g.irange(-3, 3); d = g.irange(-3, 3);
      if (c == 0 && d == 0) c = 2;
    } else {
      a = g.expo(-8, 8); b = g.expo(-8, 8); c = g.expo(-8, 8); d = (g.next() & 1) ? c : -c;
    }
    rec("t", a, b, c, d);
  }
  // r: |r| at, just below and just above DBL_MIN (E3c construction)
  for (int k = 0; k < 400;) {
    double c = g.expo(0, 200), rr;
    switch (k % 4) {
      case 0: rr = 0x1p-1022; break;
      case 1: rr = std::nextafter(0x1p-1022, 0.0); break;
      case 2: rr = std::nextafter(0x1p-1022, 1.0); break;
      default: rr = std::ldexp(sig(g), g.irange(-1024, -1021)); break;
    }
    if (g.next() & 1) rr = -rr;
    double d = rr * c;
    if (!std::isfinite(d) || d == 0) continue;
    double b = g.expo(900, 1000), a = b * (d / c) * (1.0 + g.uni());
    if (g.next() & 1) { std::swap(c, d); std::swap(a, b); }
    rec("r", a, b, c, d);
    ++k;
  }
  // z: r underflows (E3 regime)
  for (int k = 0; k < 300; ++k) {
    double big = g.expo(300, 600), small = g.expo(-700, -500);
    double c = (k & 1) ? big : small, d = (k & 1) ? small : big;
    double p = g.expo(200, 900), q = g.expo(-900, 300);
    if (k & 2) std::swap(p, q);
    double a = (k & 1) ? q : p, b = (k & 1) ? p : q;
    if ((k & 12) == 4) a = 0.0;
    if ((k & 12) == 8) b = 0.0;
    rec("z", a, b, c, d);
  }
  // s1: max(|c|,|d|) around 2^-52; numerator near 2^972 (spurious overflow)
  for (int k = 0; k < 300; ++k) {
    double c;
    switch (k % 5) {
      case 0: c = 0x1p-52; break;
      case 1: c = std::nextafter(0x1p-52, 0.0); break;
      case 2: c = std::ldexp(sig(g), g.irange(-120, -40)); break;
      case 3: c = std::fabs(g.expo(-1074, -53)); break;
      default: c = std::fabs(g.expo(-60, -45)); break;
    }
    double d = c * std::ldexp(sig(g), -g.irange(0, 80));
    double a = (k & 1) ? g.expo(960, 1000) : g.expo(-300, 300), b = g.expo(-1074, 1000);
    if (g.next() & 1) { std::swap(c, d); std::swap(a, b); }
    if (g.next() & 1) c = -c;
    if (c == 0 && d == 0) d = 0x1p-60;
    rec("s1", a, b, c, d);
  }
  // s2: small numerator part (subnormal, zero, DBL_MIN, pred(DBL_MIN)); E16/E17
  for (int k = 0; k < 600; ++k) {
    double c = std::ldexp(3.0 * (1.0 + 0.3 * g.uni()), g.irange(-50, 60));
    double d = c * std::ldexp(sig(g), g.irange(-60, -2)), r = d / c, a;
    switch (k % 6) {
      case 0: a = 0x1p-1022; break;
      case 1: a = std::nextafter(0x1p-1022, 0.0); break;
      case 2: a = 0.0; break;
      case 3: a = g.expo(-1074, -1023); break;
      default: a = g.expo(-1040, -990); break;
    }
    if (g.next() & 1) a = -a;
    double b = (k % 6 == 2) ? g.expo(-1000, -900) : -a / r * (1.0 - std::ldexp(sig(g), g.irange(-60, -1)));
    if (k % 7 == 0) b = g.expo(-1074, 64);
    if (g.next() & 1) { std::swap(a, b); std::swap(c, d); a = -a; }
    rec("s2", a, b, c, d);
  }
  // s3: max(|c|,|d|) around DBL_MAX/2 (E19)
  for (int k = 0; k < 300; ++k) {
    double X;
    switch (k % 4) {
      case 0: X = 0x1.fffffffffffffp+1022; break;
      case 1: X = std::nextafter(0x1.fffffffffffffp+1022, 0.0); break;
      case 2: X = std::fabs(g.expo(1022, 1023)); break;
      default: X = std::fabs(g.expo(1015, 1023)); break;
    }
    double d = X * ((k & 8) ? 1.0 : std::ldexp(sig(g), -g.irange(0, 1100)));
    double a = g.expo(-1074, 1023), b = (k & 16) ? a : g.expo(-1074, 1023);
    double c = X;
    if (g.next() & 1) std::swap(c, d);
    if (g.next() & 1) c = -c;
    if (g.next() & 1) d = -d;
    rec("s3", a, b, c, d);
  }
  // g: guard region (one numerator part tiny/zero, others near 2^971..2^973)
  for (int k = 0; k < 200; ++k) {
    double a = (k % 3 == 0) ? 0.0 : g.expo(-1074, -1023);
    double b = g.expo(960, 1023), c = g.expo(960, 1023), d = (k & 1) ? c : g.expo(-1074, 1023);
    if (k & 2) b = g.expo(-60, 60);
    if (k & 4) { c = g.expo(-60, 60); d = c * 0.75; }
    if (g.next() & 1) std::swap(a, b);
    if (g.next() & 1) std::swap(c, d);
    rec("g", a, b, c, d);
  }
  // w: whole finite range
  for (int k = 0; k < 600; ++k) {
    double a, b, c, d;
    if (k & 1) { a = g.anybits(); b = g.anybits(); c = g.anybits(); d = g.anybits(); }
    else { a = g.expo(-1074, 1023); b = g.expo(-1074, 1023); c = g.expo(-1074, 1023); d = g.expo(-1074, 1023); }
    if (c == 0 && d == 0) c = 1;
    rec("w", a, b, c, d);
  }
  // p: special-value grid sample (E22 grid)
  std::vector<double> base = {0.0, 0x1p-1074, 0x1p-1022, 1.0, 3.0, 0x1p1000, 0x1.fffffffffffffp+1023, INFINITY, NAN};
  std::vector<double> vals;
  for (double v : base) { vals.push_back(v); vals.push_back(-v); }
  const int nv = (int)vals.size();
  for (int k = 0; k < 600; ++k)
    rec("p", vals[g.irange(0, nv - 1)], vals[g.irange(0, nv - 1)], vals[g.irange(0, nv - 1)], vals[g.irange(0, nv - 1)]);
  // q: special/finite mix (E25)
  std::vector<double> sp = {0.0, 0x1p-1074, 0x1p-1022, std::nextafter(0x1p-1022, 0.0), 0x1p-52,
                            std::nextafter(0x1p-52, 0.0), 1.0, 0x1p971, 0x1p972, 0x1.fffffffffffffp+1022,
                            0x1.fffffffffffffp+1023, INFINITY, NAN};
  auto pick = [&]() {
    double v;
    switch (g.irange(0, 2)) {
      case 0: v = sp[g.irange(0, (int)sp.size() - 1)]; break;
      case 1: v = g.expo(-1074, 1023); break;
      default: v = g.expo(-30, 30); break;
    }
    return (g.next() & 1) ? -v : v;
  };
  for (int k = 0; k < 400; ++k) {
    double a = pick(), b = pick(), c = pick(), d = pick();
    rec("q", a, b, c, d);
  }
  for (auto & [t, s] : stats)
    std::fprintf(stderr, "tag %-3s records=%ld model_v5_disagreements=%ld\n", t.c_str(), s.first, s.second);
}
