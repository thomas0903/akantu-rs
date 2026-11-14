// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E15 (black-box). Inputs with a LARGE numerator
// component whose quotient is still sensitive to a subnormal intermediate:
// c = ±2^[0,60]·m, d subnormal, a subnormal, b = ±2^[0, p+52]·m (so that
// d·(b/c) stays below DBL_MIN), in both branch orientations. Counts, per
// ilogb max(|a|,|b|), whether the oracle equals U (= model_v2), A (all four
// operands times 2^52) or both/neither, on inputs where U != A.
#include <array>
#include <map>
#include "model_v2.h"

static C A52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  return model_v1(a * 0x1p52, b * 0x1p52, c * 0x1p52, d * 0x1p52);
}

int main() {
  Rng r{0xc0ffee15ULL};
  std::map<int, std::array<long, 3>> tab;
  for (long k = 0; k < 30000000L; ++k) {
    int p = r.irange(-52, 900);
    double c = r.expo(p, p);
    double d = r.expo(-1074, -1023);
    double a = r.expo(-1074, -1023);
    int qmax = std::min(1023, p + 52);
    double b = r.expo(std::max(-1074, qmax - 120), qmax);
    if (k & 1) { std::swap(a, b); std::swap(c, d); }
    C u = model_v2(a, b, c, d), s = A52(a, b, c, d);
    if (same(u.real(), s.real()) && same(u.imag(), s.imag())) continue;
    C z = oracle_div(C(a, b), C(c, d));
    bool eu = same(u.real(), z.real()) && same(u.imag(), z.imag());
    bool ea = same(s.real(), z.real()) && same(s.imag(), z.imag());
    int lab = std::ilogb(std::fmax(std::fabs(a), std::fabs(b)));
    tab[lab >> 4 << 4][eu ? 0 : ea ? 1 : 2]++;
  }
  for (auto & [k, v] : tab) std::printf("lAB~[%d,%d): U=%ld A=%ld neither=%ld\n", k, k + 16, v[0], v[1], v[2]);
}
