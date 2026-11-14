// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: black-box probe of the std::complex<double> division of the
// oracle image, with an independent generator (xoshiro256**, seeds and classes independent of the author's
// E1/E4/E25/E28). Only the results of the compiled division are observed
// (D-030); no library source or compiled library code is read or inspected.
//
// usage: v110_cdiv_gen OUT.bin SCALE
//   writes records of 7 little-endian u64 (class, a, b, c, d, re, im) for the
//   random classes (SCALE * the per-class counts below) and, for the
//   exhaustive special-value grid, every record on which a model disagrees
//   plus every 16th record;
//   prints per class the disagreements of
//     v6     the author's unfused model (oracle/probes/dense/complex_cleanroom/model_v6.h, = Unfused)
//     v5     the author's fused model (= Fused, the aarch64 runtime)
//     v6p    v6 with the denominator guard cd < 0x1.fffffffffffffp+970
//     v6q    v6 with both guards (cd, ab) < 0x1.fffffffffffffp+970
//   NaN sign bits are ignored, as in the author's experiments and the port's
//   tests (D-046 canonicalisation).
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <vector>

#include "v110_models.h"

C runtime_div(C x, C y);

struct Xo {  // xoshiro256** (Blackman & Vigna), seeded by splitmix64
  std::uint64_t s[4];
  explicit Xo(std::uint64_t seed) {
    for (auto & w : s) {
      std::uint64_t z = (seed += 0x9e3779b97f4a7c15ULL);
      z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
      z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
      w = z ^ (z >> 31);
    }
  }
  static std::uint64_t rotl(std::uint64_t x, int k) { return (x << k) | (x >> (64 - k)); }
  std::uint64_t next() {
    std::uint64_t r = rotl(s[1] * 5, 7) * 9, t = s[1] << 17;
    s[2] ^= s[0]; s[3] ^= s[1]; s[1] ^= s[2]; s[0] ^= s[3]; s[2] ^= t; s[3] = rotl(s[3], 45);
    return r;
  }
  int irange(int lo, int hi) { return lo + int(next() % std::uint64_t(hi - lo + 1)); }
  double unit() { return double(next() >> 11) * 0x1.0p-53; }  // [0, 1)
  bool coin() { return next() & 1; }
  double sgn(double v) { return coin() ? -v : v; }
  double expo(int lo, int hi) { return sgn(std::ldexp(1.0 + unit(), irange(lo, hi))); }
  double bitsany() {
    for (;;) {
      double v = from_bits(next());
      if (std::isfinite(v)) return v;
    }
  }
  // v moved by up to +-2^k ulps (stays finite and of the same sign)
  double near(double v, int k) {
    std::int64_t off = std::int64_t(next() % (2ULL << k)) - (std::int64_t(1) << k);
    std::uint64_t u = bits(v);
    std::uint64_t w = std::uint64_t(std::int64_t(u) + off);
    double x = from_bits(w);
    if (!std::isfinite(x) || std::signbit(x) != std::signbit(v)) return v;
    return x;
  }
};

static const double TH[] = {0x1p-1074, 0x1p-1073, 0x1p-1022, 0x1p-1021, 0x1p-970, 0x1p-52, 1.0, 0x1p60,
                            0x1p969,   0x1p970,   0x1p971,   0x1p972,   0x1p1022, 0x1p1023};

static std::vector<double> specials() {
  std::vector<double> v = {0.0, INFINITY, NAN, 0x1.fffffffffffffp+1023, 0x1.fffffffffffffp+1022, 3.0};
  for (double t : TH) {
    v.push_back(t);
    v.push_back(std::nextafter(t, 0.0));
    v.push_back(std::nextafter(t, INFINITY));
  }
  return v;
}

enum { K_WHOLE, K_BITS, K_MOD, K_SPECIAL, K_TIE, K_GUARD, K_ZERO, K_RMIN, K_GRID, NK };
static const char * NAME[NK] = {"whole", "bits", "moderate", "special", "tie", "guard", "zero_sub", "r_min", "grid"};
static const long COUNT[NK - 1] = {4000000, 3000000, 3000000, 4000000, 2000000, 3000000, 2000000, 2000000};

struct Tally {
  long tot = 0, v6 = 0, v5 = 0, v6p = 0, v6q = 0, written = 0;
};

int main(int argc, char ** argv) {
  if (argc < 3) { std::fprintf(stderr, "usage: %s OUT.bin SCALE\n", argv[0]); return 2; }
  FILE * out = std::fopen(argv[1], "wb");
  if (!out) { std::perror("fopen"); return 1; }
  double scale = std::atof(argv[2]);
  const std::vector<double> sp = specials();
  Xo g{0x5110c0deULL};
  auto special = [&]() { return g.sgn(sp[g.irange(0, int(sp.size()) - 1)]); };
  auto pick = [&]() {
    switch (g.irange(0, 5)) {
      case 0: return special();
      case 1: return g.expo(-1074, 1023);
      case 2: return g.bitsany();
      case 3: return g.expo(-60, 60);
      case 4: return g.sgn(g.near(TH[g.irange(0, 13)], g.irange(0, 12)));
      default: return g.sgn(g.unit() * std::pow(10.0, g.irange(-12, 12)));
    }
  };
  auto tiny = [&]() {  // zero, subnormal or a normal next to DBL_MIN
    switch (g.irange(0, 3)) {
      case 0: return g.sgn(0.0);
      case 1: return g.sgn(from_bits(g.next() & 0x000fffffffffffffULL));
      case 2: return g.sgn(g.near(0x1p-1022, g.irange(0, 20)));
      default: return g.expo(-1074, -1000);
    }
  };
  Tally t[NK];
  auto check = [&](int k, double a, double b, double c, double d, bool always_write) {
    C z = runtime_div(C(a, b), C(c, d));
    C w6 = model_v6(a, b, c, d), w5 = model_v5(a, b, c, d);
    C wp = model_guard(a, b, c, d, PRED_2_971, 0x1p971);
    C wq = model_guard(a, b, c, d, PRED_2_971, PRED_2_971);
    auto eq = [&](C w) { return same(w.real(), z.real()) && same(w.imag(), z.imag()); };
    bool m6 = !eq(w6), m5 = !eq(w5), mp = !eq(wp), mq = !eq(wq);
    Tally & s = t[k];
    ++s.tot; s.v6 += m6; s.v5 += m5; s.v6p += mp; s.v6q += mq;
    if (m6 && s.v6 <= 12) {
      std::printf("MIS6 %-8s %016llx %016llx %016llx %016llx | rt (%a, %a) v6 (%a, %a) v6p %s\n", NAME[k],
                  (unsigned long long)bits(a), (unsigned long long)bits(b), (unsigned long long)bits(c),
                  (unsigned long long)bits(d), z.real(), z.imag(), w6.real(), w6.imag(), mp ? "MIS" : "ok");
    }
    if (mp && s.v6p <= 12) {
      std::printf("MISP %-8s %016llx %016llx %016llx %016llx | rt (%a, %a) v6p (%a, %a)\n", NAME[k],
                  (unsigned long long)bits(a), (unsigned long long)bits(b), (unsigned long long)bits(c),
                  (unsigned long long)bits(d), z.real(), z.imag(), wp.real(), wp.imag());
    }
    if (always_write || m6 || m5 || mp || (s.tot % 16 == 0)) {
      std::uint64_t rec[7] = {std::uint64_t(k), bits(a), bits(b), bits(c), bits(d), bits(z.real()), bits(z.imag())};
      std::fwrite(rec, 8, 7, out);
      ++s.written;
    }
  };
  for (int k = 0; k < K_GRID; ++k) {
    long n = long(COUNT[k] * scale);
    for (long i = 0; i < n; ++i) {
      double a, b, c, d;
      switch (k) {
        case K_WHOLE:
          a = g.expo(-1074, 1023); b = g.expo(-1074, 1023); c = g.expo(-1074, 1023); d = g.expo(-1074, 1023);
          break;
        case K_BITS:
          a = g.bitsany(); b = g.bitsany(); c = g.bitsany(); d = g.bitsany();
          break;
        case K_MOD:
          a = g.sgn(g.unit() * std::pow(10.0, g.irange(-12, 12)));
          b = g.sgn(g.unit() * std::pow(10.0, g.irange(-12, 12)));
          c = g.sgn(g.unit() * std::pow(10.0, g.irange(-12, 12)));
          d = g.sgn(g.unit() * std::pow(10.0, g.irange(-12, 12)));
          if (c == 0.0 && d == 0.0) c = 1.0;
          break;
        case K_SPECIAL:
          a = pick(); b = pick(); c = pick(); d = pick();
          break;
        case K_TIE: {  // |c| = |d| (or one ulp apart), any signs
          a = pick(); b = pick(); c = pick();
          d = g.sgn(std::fabs(c));
          if (g.irange(0, 3) == 0) d = g.sgn(std::nextafter(std::fabs(c), g.coin() ? INFINITY : 0.0));
          if (g.coin()) std::swap(c, d);
          break;
        }
        case K_GUARD: {  // the small-numerator rule near its guards (2^970 ... 2^972)
          double big;
          switch (g.irange(0, 4)) {
            case 0: big = PRED_2_971; break;
            case 1: big = g.near(PRED_2_971, g.irange(0, 16)); break;
            case 2: big = std::ldexp(1.0 + g.unit(), g.irange(968, 972)); break;
            case 3: big = g.near(0x1p972, g.irange(0, 16)); break;
            default: big = g.near(0x1p970, g.irange(0, 16)); break;
          }
          big = g.sgn(big);
          if (g.irange(0, 2) != 0) {  // denominator guard: max(|c|, |d|) = big
            c = big;
            d = g.coin() ? pick() : g.sgn(std::fabs(big) * std::ldexp(1.0 + g.unit(), -g.irange(1, 1100)));
            if (std::fabs(d) > std::fabs(c) && std::isfinite(d)) d = g.sgn(std::fabs(c) * g.unit());
            a = tiny();
            b = g.coin() ? pick() : g.expo(-1074, 970);
          } else {  // numerator guard: max(|a|, |b|) = big
            a = tiny();
            b = big;
            c = pick();
            d = pick();
          }
          if (g.coin()) std::swap(a, b);
          if (g.coin()) std::swap(c, d);
          break;
        }
        case K_ZERO:
          a = tiny(); b = g.coin() ? tiny() : pick(); c = g.expo(-1074, 1023); d = g.expo(-1074, 1023);
          if (g.coin()) std::swap(a, b);
          break;
        default: {  // K_RMIN: |r| around DBL_MIN, both branch orientations
          c = g.expo(-200, 200);
          d = c * std::ldexp(1.0 + g.unit(), -g.irange(1000, 1080));
          a = g.coin() ? g.expo(-1074, 1023) : tiny();
          b = g.expo(-1074, 1023);
          if (g.coin()) { std::swap(c, d); }
          if (g.coin()) { std::swap(a, b); }
          break;
        }
      }
      check(k, a, b, c, d, true);
    }
  }
  // exhaustive grid of signed specials (30 magnitudes x 2 signs)^4
  std::vector<double> gv = {0.0, 0x1p-1074, 0x1p-1022, std::nextafter(0x1p-1022, 0.0), 0x1p-52,
                            std::nextafter(0x1p-52, 0.0), 1.0, 3.0, 0x1p60, 0x1p970,
                            PRED_2_971, 0x1p971, std::nextafter(0x1p972, 0.0), 0x1p972, 0x1.fffffffffffffp+1022,
                            std::nextafter(0x1.fffffffffffffp+1022, 0.0), 0x1p1023, 0x1.fffffffffffffp+1023, INFINITY, NAN,
                            0x1p-970, 0x1p-1000, 0x1.8p-1060, 0x1.4p100, 0x1.4p-100, 0x1p500, 0x1p-500, 0x1p-53,
                            0x1.8p970, 0x1.8p969};
  std::vector<double> vals;
  for (double v : gv) { vals.push_back(v); vals.push_back(-v); }
  for (double a : vals) for (double b : vals) for (double c : vals) for (double d : vals) check(K_GRID, a, b, c, d, false);
  std::fclose(out);
  Tally all;
  for (int k = 0; k < NK; ++k) {
    std::printf("V110 class %-8s total=%ld v6_unfused=%ld v5_fused=%ld v6p_cdguard_pred=%ld v6q_both_pred=%ld written=%ld\n",
                NAME[k], t[k].tot, t[k].v6, t[k].v5, t[k].v6p, t[k].v6q, t[k].written);
    all.tot += t[k].tot; all.v6 += t[k].v6; all.v5 += t[k].v5; all.v6p += t[k].v6p; all.v6q += t[k].v6q;
    all.written += t[k].written;
  }
  std::printf("V110 total=%ld v6_unfused=%ld v5_fused=%ld v6p_cdguard_pred=%ld v6q_both_pred=%ld written=%ld\n",
              all.tot, all.v6, all.v5, all.v6p, all.v6q, all.written);
}
