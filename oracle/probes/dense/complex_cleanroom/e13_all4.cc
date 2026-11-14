// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E13 (black-box). Same population as E12 (inputs
// where scaling the numerator matters). Model A = model_v2 with all four
// operands multiplied by 2^52 (no rescaling of the quotient) under a
// candidate trigger; prints, per ilogb max(|c|,|d|) (after the CD < 2^-52
// scaling), how often the oracle equals U (= model_v2), A (= all four
// scaled), both or neither.
#include <map>
#include <array>
#include <cstdlib>
#include "model_v2.h"

static C S52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  C w = model_v1(a * 0x1p52, b * 0x1p52, c, d);
  return C(w.real() * 0x1p-52, w.imag() * 0x1p-52);
}
static C A52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  return model_v1(a * 0x1p52, b * 0x1p52, c * 0x1p52, d * 0x1p52);
}

int main(int argc, char ** argv) {
  long n = argc > 1 ? std::atol(argv[1]) : 20000000L;
  bool dump = argc > 2;
  Rng r{0xc0ffee12ULL};
  std::map<int, std::array<long, 4>> tab;  // U-only, A-only, both, neither
  for (long k = 0; k < n; ++k) {
    double a = r.expo(-1074, 200), b = r.expo(-1074, 200), c = r.expo(-200, 200), d = r.expo(-1074, 200);
    if (k & 1) a = r.expo(-1074, -1000);
    if (k & 2) d = std::ldexp(c, -r.irange(0, 1100));
    if (k & 4) { std::swap(a, b); std::swap(c, d); }
    if (c == 0 && d == 0) continue;
    C u = model_v2(a, b, c, d), s = S52(a, b, c, d);
    if (same(u.real(), s.real()) && same(u.imag(), s.imag())) continue;
    if (!std::isfinite(s.real()) || !std::isfinite(s.imag())) continue;
    C z = oracle_div(C(a, b), C(c, d));
    C w = A52(a, b, c, d);
    bool eu = same(u.real(), z.real()) && same(u.imag(), z.imag());
    bool ea = same(w.real(), z.real()) && same(w.imag(), z.imag());
    double CD = std::fmax(std::fabs(c), std::fabs(d));
    if (CD < 0x1p-52) CD *= 0x1p52;
    tab[std::ilogb(CD)][eu && !ea ? 0 : ea && !eu ? 1 : ea ? 2 : 3]++;
    if (eu && !ea && dump) {
      print4("UONLY", a, b, c, d);
      std::printf(" lAB=%d lCD=%d lminab=%d lmincd=%d\n", std::ilogb(std::fmax(std::fabs(a), std::fabs(b))),
                  std::ilogb(std::fmax(std::fabs(c), std::fabs(d))), std::ilogb(std::fmin(std::fabs(a), std::fabs(b))),
                  std::ilogb(std::fmin(std::fabs(c), std::fabs(d))));
    }
  }
  for (auto & [k, v] : tab)
    if (k >= -60 && k <= 70) std::printf("lCD %d: U-only=%ld A-only=%ld both=%ld neither=%ld\n", k, v[0], v[1], v[2], v[3]);
}
