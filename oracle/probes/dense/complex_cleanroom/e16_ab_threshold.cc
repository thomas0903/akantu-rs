// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E16 (black-box). Is the all-operand 2^52 scaling
// triggered by the magnitude of the numerator parts? c-branch with c = 3·2^p
// (p in [-2, 40]), d = c·t (t random in [2^-40, 2^-20]) and a, b chosen so
// that fma(b, r, a) cancels to a subnormal value: a = ±m·2^e (e swept around
// -1022), b = -a/r·(1 - u). On inputs where U (= model_v2) and A (all four
// times 2^52) differ, counts which one the oracle equals, per ilogb(|a|)
// (and separately per ilogb(min(|a|,|b|))).
#include <array>
#include <map>
#include "model_v2.h"

static C A52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  return model_v1(a * 0x1p52, b * 0x1p52, c * 0x1p52, d * 0x1p52);
}

int main() {
  Rng g{0xc0ffee16ULL};
  std::map<int, std::array<long, 3>> tab;
  for (long k = 0; k < 20000000L; ++k) {
    double c = std::ldexp(3.0 * (1.0 + 0.3 * g.uni()), g.irange(-2, 40));
    double t = std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, g.irange(-40, -20));
    double d = c * t;
    double r = d / c;
    int e = g.irange(-1040, -990);
    double a = g.expo(e, e);
    double u = std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, g.irange(-60, -1));
    double b = -a / r * (1.0 - u);
    bool sw = g.next() & 1;
    double A_ = a, B_ = b, C_ = c, D_ = d;
    if (sw) { std::swap(A_, B_); std::swap(C_, D_); A_ = -A_; }  // mirrored: same structure in the r = c/d branch
    C uu = model_v2(A_, B_, C_, D_), s = A52(A_, B_, C_, D_);
    if (same(uu.real(), s.real()) && same(uu.imag(), s.imag())) continue;
    C z = oracle_div(C(A_, B_), C(C_, D_));
    bool eu = same(uu.real(), z.real()) && same(uu.imag(), z.imag());
    bool ea = same(s.real(), z.real()) && same(s.imag(), z.imag());
    tab[std::ilogb(std::fmin(std::fabs(A_), std::fabs(B_)))][eu ? 0 : ea ? 1 : 2]++;
  }
  for (auto & [k, v] : tab) std::printf("ilogb min(|a|,|b|)=%d: U=%ld A=%ld neither=%ld\n", k, v[0], v[1], v[2]);
}
