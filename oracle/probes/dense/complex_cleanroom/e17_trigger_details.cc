// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E17 (black-box). Details of the small-numerator
// scaling trigger found in E16:
//  (a) does an exact zero numerator part trigger it? b = ±0, a normal,
//      y-numerator -a·r subnormal (U and A differ there);
//  (b) boundary: |a| = DBL_MIN exactly vs its predecessor, with the E16
//      cancellation construction;
//  (c) zero with the CD < 2^-52 scaling active: is the scaling applied twice
//      (overflow of a·2^104)?
#include "model_v2.h"

static C A52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  return model_v1(a * 0x1p52, b * 0x1p52, c * 0x1p52, d * 0x1p52);
}
static char cls(double a, double b, double c, double d) {
  C z = oracle_div(C(a, b), C(c, d)), u = model_v2(a, b, c, d), s = A52(a, b, c, d);
  bool eu = same(u.real(), z.real()) && same(u.imag(), z.imag());
  bool ea = same(s.real(), z.real()) && same(s.imag(), z.imag());
  return eu && ea ? '=' : eu ? 'U' : ea ? 'A' : '?';
}

int main() {
  Rng g{0xc0ffee17ULL};
  long cnt[2][4] = {{0}};  // [zero sign][U, A, =, ?]
  for (long k = 0; k < 2000000; ++k) {
    double c = std::ldexp(3.0 * (1.0 + 0.3 * g.uni()), g.irange(-2, 40));
    double d = c * std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, g.irange(-60, -2));
    double a = g.expo(-1022, -960);
    double b = (k & 1) ? -0.0 : 0.0;
    double A_ = a, B_ = b, C_ = c, D_ = d;
    if (k & 2) { std::swap(A_, B_); std::swap(C_, D_); }
    char ch = cls(A_, B_, C_, D_);
    cnt[k & 1][ch == 'U' ? 0 : ch == 'A' ? 1 : ch == '=' ? 2 : 3]++;
  }
  std::printf("(a) zero part, +0: U=%ld A=%ld both=%ld neither=%ld\n", cnt[0][0], cnt[0][1], cnt[0][2], cnt[0][3]);
  std::printf("(a) zero part, -0: U=%ld A=%ld both=%ld neither=%ld\n", cnt[1][0], cnt[1][1], cnt[1][2], cnt[1][3]);

  long cb[2][4] = {{0}};
  for (long k = 0; k < 2000000; ++k) {
    double c = std::ldexp(3.0 * (1.0 + 0.3 * g.uni()), g.irange(-2, 40));
    double t = std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, g.irange(-40, -20));
    double d = c * t, r = d / c;
    int which = k & 1;  // 0: |a| = DBL_MIN, 1: |a| = pred(DBL_MIN)
    double a = which ? std::nextafter(0x1p-1022, 0.0) : 0x1p-1022;
    if (g.next() & 1) a = -a;
    double u = std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, g.irange(-60, -1));
    double b = -a / r * (1.0 - u);
    double A_ = a, B_ = b, C_ = c, D_ = d;
    if (k & 2) { std::swap(A_, B_); std::swap(C_, D_); A_ = -A_; }
    char ch = cls(A_, B_, C_, D_);
    cb[which][ch == 'U' ? 0 : ch == 'A' ? 1 : ch == '=' ? 2 : 3]++;
  }
  std::printf("(b) |a| = DBL_MIN      : U=%ld A=%ld both=%ld neither=%ld\n", cb[0][0], cb[0][1], cb[0][2], cb[0][3]);
  std::printf("(b) |a| = pred(DBL_MIN): U=%ld A=%ld both=%ld neither=%ld\n", cb[1][0], cb[1][1], cb[1][2], cb[1][3]);

  // (c) CD < 2^-52 and b = 0: a = 1.5*2^930 overflows if scaled by 2^104 but
  // not by 2^52 (c = 2^-60, d = 2^-70: true result ~2^990, finite).
  for (double b : {0.0, 0x1p-1074, 0x1p-1022}) {
    double a = 0x1.8p930, c = 0x1p-60, d = 0x1.4p-70;
    C z = oracle_div(C(a, b), C(c, d));
    C u = model_v2(a, b, c, d);
    std::printf("(c) a=%a b=%a c=%a d=%a: oracle (%a, %a) model_v2 (%a, %a)\n", a, b, c, d, z.real(), z.imag(), u.real(),
                u.imag());
  }
}
