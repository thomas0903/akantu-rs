// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E11 (black-box). Where is the numerator scaled?
// U = model_v2 (numerator as given), S = same with a, b multiplied by 2^52
// and the quotient multiplied back by 2^-52. For random inputs on which U and
// S differ, record whether the oracle equals U or S, bucketed by
// (ilogb max(|a|,|b|), ilogb max(|c|,|d|)) in steps of 16. Prints, per
// bucket, counts "U/S/neither".
#include <array>
#include <map>
#include "model_v2.h"

static C S52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  C w = model_v1(a * 0x1p52, b * 0x1p52, c, d);
  return C(w.real() * 0x1p-52, w.imag() * 0x1p-52);
}
static int lg(double x, double y) { return std::ilogb(std::fmax(std::fabs(x), std::fabs(y))); }

int main() {
  Rng r{0xc0ffee11ULL};
  std::map<std::pair<int, int>, std::array<long, 3>> tab;
  long n = 0;
  for (long k = 0; k < 60000000L; ++k) {
    double a = r.expo(-1074, 1023), b = r.expo(-1074, 1023), c = r.expo(-1074, 1023), d = r.expo(-1074, 1023);
    // bias towards small numerators and subnormal parts
    if (k & 1) a = r.expo(-1074, -1000);
    if (k & 2) d = std::ldexp(c, -r.irange(0, 1100));
    if (c == 0 && d == 0) continue;
    C u = model_v2(a, b, c, d), s = S52(a, b, c, d);
    if (same(u.real(), s.real()) && same(u.imag(), s.imag())) continue;
    if (!std::isfinite(s.real()) || !std::isfinite(s.imag())) continue;
    C z = oracle_div(C(a, b), C(c, d));
    int i = (same(u.real(), z.real()) && same(u.imag(), z.imag())) ? 0
            : (same(s.real(), z.real()) && same(s.imag(), z.imag())) ? 1 : 2;
    int ab = lg(a, b), cd = lg(c, d);
    auto key = std::make_pair((ab < -1022 ? -1023 - 15 : ab) >> 4 << 4, cd >> 4 << 4);
    tab[key][i]++;
    ++n;
  }
  std::printf("E11 discriminating cases=%ld\n", n);
  for (auto & [k, v] : tab)
    std::printf("AB~[%d,%d) CD~[%d,%d): U=%ld S=%ld neither=%ld\n", k.first, k.first + 16, k.second, k.second + 16, v[0], v[1], v[2]);
}
