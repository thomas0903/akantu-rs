// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E1 (black-box). Compares std::complex<double>
// division with variants of Smith's Algorithm 116 (Smith 1962) written here
// from the published algorithm, on operands in a moderate range (no
// overflow/underflow possible in the intermediates), to establish:
//  * branch choice when |c| == |d|;
//  * whether the denominator / numerators are fused multiply-adds;
//  * whether the quotients are divisions or multiplications by 1/den;
//  * the exact operand order of the d-branch imaginary numerator.
#include "common.h"

struct Cfg {
  bool le;     // d-branch (r = c/d) when |c| <= |d| (else when |c| < |d|)
  bool fden;   // denominator fused
  bool fx;     // real numerator fused
  bool fy;     // imaginary numerator fused
  bool recip;  // multiply by 1/den instead of dividing
  bool negy;   // d-branch imag as -(a - b r) instead of (b r - a)
};

static double mad(bool f, double x, double y, double z) { return f ? std::fma(x, y, z) : x * y + z; }

static C smith(double a, double b, double c, double d, const Cfg & g) {
  bool dbr = g.le ? std::fabs(c) <= std::fabs(d) : std::fabs(c) < std::fabs(d);
  double x, y, den;
  if (dbr) {
    double r = c / d;
    den = mad(g.fden, c, r, d);
    x = mad(g.fx, a, r, b);
    y = g.negy ? -mad(g.fy, -b, r, a) : mad(g.fy, b, r, -a);
  } else {
    double r = d / c;
    den = mad(g.fden, d, r, c);
    x = mad(g.fx, b, r, a);
    y = mad(g.fy, -a, r, b);
  }
  if (g.recip) {
    double t = 1.0 / den;
    return C(x * t, y * t);
  }
  return C(x / den, y / den);
}

int main() {
  Cfg cfgs[64];
  int n = 0;
  for (int m = 0; m < 64; ++m)
    cfgs[n++] = Cfg{bool(m & 1), bool(m & 2), bool(m & 4), bool(m & 8), bool(m & 16), bool(m & 32)};
  long mism[64] = {0};
  long total = 0;
  Rng r{0xc0ffee01ULL};
  for (long k = 0; k < 2000000; ++k) {
    double a, b, c, d;
    switch (k % 4) {
      case 0:  // as the dense fixture: uni * 10^[-12, 12]
        a = r.uni() * std::pow(10.0, r.irange(-12, 12));
        b = r.uni() * std::pow(10.0, r.irange(-12, 12));
        c = r.uni() * std::pow(10.0, r.irange(-12, 12));
        d = r.uni() * std::pow(10.0, r.irange(-12, 12));
        break;
      case 1:  // comparable magnitudes, random significands
        a = r.expo(-4, 4); b = r.expo(-4, 4); c = r.expo(-4, 4); d = r.expo(-4, 4);
        break;
      case 2:  // small integers (exact cases, ties |c| == |d|, signed zeros)
        a = r.irange(-4, 4); b = r.irange(-4, 4); c = r.irange(-4, 4); d = r.irange(-4, 4);
        if (c == 0 && d == 0) c = 1;
        break;
      default:  // |c| == |d| exactly with random significands
        a = r.expo(-8, 8); b = r.expo(-8, 8); c = r.expo(-8, 8);
        d = (r.next() & 1) ? c : -c;
        break;
    }
    C z = oracle_div(C(a, b), C(c, d));
    ++total;
    for (int i = 0; i < n; ++i) {
      C w = smith(a, b, c, d, cfgs[i]);
      if (!same(w.real(), z.real()) || !same(w.imag(), z.imag())) ++mism[i];
    }
  }
  std::printf("E1 total=%ld\n", total);
  for (int i = 0; i < n; ++i)
    std::printf("cfg le=%d fden=%d fx=%d fy=%d recip=%d negy=%d mismatches=%ld\n", cfgs[i].le, cfgs[i].fden,
                cfgs[i].fx, cfgs[i].fy, cfgs[i].recip, cfgs[i].negy, mism[i]);
}
