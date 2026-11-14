// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: black-box scan of the
// guards of the small-numerator up-scaling (rule 6 / row 8 of
// COMPLEX_CLEANROOM_LOG.md) on the oracle image. For each magnitude X of
// max(|c|, |d|) (mode "cd") or of max(|a|, |b|) (mode "ab") it draws M inputs
// with a zero or subnormal numerator part (the trigger) and counts which of
// two unfused models agrees with the runtime: S = always up-scale on the
// trigger, U = never. "S only" at X means the runtime scaled there, "U only"
// that it did not. Only results of the compiled division are observed (D-030).
// Mode "ab2" isolates the numerator guard: max(|c|, |d|) stays below
// 0x1.ffffffffffffep+970 (inside the denominator guard), c huge, d small.
// usage: v110_guard_scan M [modes, default "cd,ab"]
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include "v110_models.h"

C runtime_div(C x, C y);

struct Sm {  // splitmix64 (own seed)
  std::uint64_t s;
  std::uint64_t next() {
    std::uint64_t z = (s += 0x9e3779b97f4a7c15ULL);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    return z ^ (z >> 31);
  }
  double unit() { return double(next() >> 11) * 0x1.0p-53; }
  int irange(int lo, int hi) { return lo + int(next() % std::uint64_t(hi - lo + 1)); }
  double sgn(double v) { return (next() & 1) ? -v : v; }
};

static double ulps_below(double x, long j) { return from_bits(bits(x) - std::uint64_t(j)); }

int main(int argc, char ** argv) {
  long m = argc > 1 ? std::atol(argv[1]) : 20000;
  std::string modes = argc > 2 ? argv[2] : "cd,ab";
  Sm g{0x6a11d5ca7ULL};
  std::vector<double> xs;
  for (int k = 900; k <= 970; k += 5) xs.push_back(std::ldexp(1.0 + 0.37, k));
  for (int k = 960; k <= 970; ++k) { xs.push_back(std::ldexp(1.0, k)); xs.push_back(std::ldexp(1.75, k)); }
  for (long j : {1L << 40, 1L << 30, 1L << 20, 1L << 10, 64L, 16L, 4L, 3L, 2L, 1L}) xs.push_back(ulps_below(PRED_2_971, j));
  xs.push_back(PRED_2_971);
  xs.push_back(0x1p971);
  xs.push_back(std::nextafter(0x1p971, INFINITY));
  xs.push_back(std::ldexp(1.5, 971));
  xs.push_back(std::nextafter(0x1p972, 0.0));
  xs.push_back(0x1p972);
  auto tiny = [&]() {
    switch (g.irange(0, 2)) {
      case 0: return g.sgn(0.0);
      case 1: return g.sgn(from_bits(g.next() & 0x000fffffffffffffULL));
      default: return g.sgn(std::ldexp(1.0 + g.unit(), -1023 - g.irange(0, 50)));
    }
  };
  for (const char * mode : {"cd", "ab", "ab2"}) {
    if (("," + modes + ",").find(std::string(",") + mode + ",") == std::string::npos) continue;
    for (double x : xs) {
      long s_only = 0, u_only = 0, both = 0, neither = 0;
      for (long i = 0; i < m; ++i) {
        double a, b, c, d;
        if (mode[0] == 'c') {
          c = g.sgn(x);
          d = g.sgn(x * std::ldexp(1.0 + g.unit(), -g.irange(1, 1100)));
          if (std::fabs(d) > std::fabs(c)) d = g.sgn(std::fabs(c));
          a = tiny();
          b = g.sgn(std::ldexp(1.0 + g.unit(), g.irange(-1074, 960)));
        } else if (mode[2] == '2') {
          a = tiny();
          b = g.sgn(x);
          c = g.sgn(std::min(std::ldexp(1.0 + g.unit(), g.irange(940, 970)), 0x1.ffffffffffffep+970));
          d = g.sgn(g.next() & 1 ? std::ldexp(1.0 + g.unit(), g.irange(-1074, -900)) : tiny());
        } else {
          a = tiny();
          b = g.sgn(x);
          c = g.sgn(std::ldexp(1.0 + g.unit(), g.irange(-52, 1000)));
          d = g.sgn(std::ldexp(1.0 + g.unit(), g.irange(-1074, 1000)));
        }
        if (g.next() & 1) std::swap(a, b);
        if (g.next() & 1) std::swap(c, d);
        C z = runtime_div(C(a, b), C(c, d));
        C ws = model_guard(a, b, c, d, INFINITY, INFINITY), wu = model_guard(a, b, c, d, 0.0, 0.0);
        bool es = same(ws.real(), z.real()) && same(ws.imag(), z.imag());
        bool eu = same(wu.real(), z.real()) && same(wu.imag(), z.imag());
        s_only += es && !eu; u_only += eu && !es; both += es && eu; neither += !es && !eu;
      }
      std::printf("SCAN %s X=%a (%016llx) S_only=%ld U_only=%ld both=%ld neither=%ld\n", mode, x,
                  (unsigned long long)bits(x), s_only, u_only, both, neither);
    }
  }
}
