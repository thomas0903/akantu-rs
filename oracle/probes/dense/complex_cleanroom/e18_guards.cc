// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E18 (black-box). Guards of the small-numerator
// scaling (trigger: |a| < DBL_MIN or |b| < DBL_MIN, E16/E17). Overflow
// detectors, bisection on the bit pattern of one large operand X:
//  G1: a = 2^-1060 (trigger), b = X, c = 1.25, d = 0.75: if scaled by 2^52,
//      b·2^52 overflows for X >= 2^972 -> infinite parts.
//  G2: a = 2^-1060, b = 1, c = X, d = X·(1-2^-20): if scaled, the denominator
//      c·2^52(1 + r^2) overflows from X ~ 2^971 on -> zero quotient (or NaN
//      once c·2^52 itself overflows).
//  G3: as G2 with the roles of c and d exchanged (d = X large, c smaller).
//  G4: a = 0 (trigger), b = X: as G1.
// Each prints the first X at which the oracle result stops being equal to the
// unscaled model_v2 (searching X in [2^960, DBL_MAX]).
#include <limits>
#include "model_v2.h"

template <class F>
static void bisect(const char * name, F make) {
  auto eq = [&](std::uint64_t u) {
    double v[4];
    make(from_bits(u), v);
    C z = oracle_div(C(v[0], v[1]), C(v[2], v[3])), m = model_v2(v[0], v[1], v[2], v[3]);
    return same(z.real(), m.real()) && same(z.imag(), m.imag());
  };
  std::uint64_t lo = bits(0x1p960), hi = bits(std::numeric_limits<double>::max());
  bool el = eq(lo), eh = eq(hi);
  if (el == eh) {
    // also scan coarse powers of two to show where/if it differs
    std::printf("%s: equal-to-model at 2^960: %d, at DBL_MAX: %d (no single switch found)\n", name, el, eh);
    for (int e = 960; e <= 1023; ++e)
      if (!eq(bits(std::ldexp(1.0, e)))) std::printf("  differs at X = 2^%d\n", e);
    return;
  }
  while (hi - lo > 1) {
    std::uint64_t mid = lo + (hi - lo) / 2;
    if (eq(mid) == el) lo = mid; else hi = mid;
  }
  double v[4];
  make(from_bits(hi), v);
  C z = oracle_div(C(v[0], v[1]), C(v[2], v[3])), m = model_v2(v[0], v[1], v[2], v[3]);
  std::printf("%s: model-equal=%d up to X=%a (%016llx); from X=%a (%016llx): oracle (%a,%a) model (%a,%a)\n", name, el,
              from_bits(lo), (unsigned long long)lo, from_bits(hi), (unsigned long long)hi, z.real(), z.imag(), m.real(),
              m.imag());
}

int main() {
  bisect("G1", [](double X, double * v) { v[0] = 0x1p-1060; v[1] = X; v[2] = 1.25; v[3] = 0.75; });
  bisect("G2", [](double X, double * v) { v[0] = 0x1p-1060; v[1] = 1.0; v[2] = X; v[3] = X * (1.0 - 0x1p-20); });
  bisect("G3", [](double X, double * v) { v[0] = 0x1p-1060; v[1] = 1.0; v[2] = X * (1.0 - 0x1p-20); v[3] = X; });
  bisect("G4", [](double X, double * v) { v[0] = 0.0; v[1] = X; v[2] = 1.25; v[3] = 0.75; });
  bisect("G5 (no trigger: a=1)", [](double X, double * v) { v[0] = 1.0; v[1] = 1.0; v[2] = X; v[3] = X * (1.0 - 0x1p-20); });
}
