// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E12 (black-box). Dump the inputs on which the
// unscaled-numerator model U (= model_v2) and the scaled-numerator model S
// (a, b times 2^52, quotient times 2^-52) differ, together with which one the
// oracle matched, for offline analysis of the trigger condition.
// Output: one line per case: class(U|S|N) a b c d (hex bits).
#include <cstdlib>
#include <string>
#include "model_v2.h"

static C S52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  C w = model_v1(a * 0x1p52, b * 0x1p52, c, d);
  return C(w.real() * 0x1p-52, w.imag() * 0x1p-52);
}

int main(int argc, char ** argv) {
  long n = argc > 1 ? std::atol(argv[1]) : 20000000L;
  Rng r{0xc0ffee12ULL};
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
    char cl = (same(u.real(), z.real()) && same(u.imag(), z.imag())) ? 'U'
              : (same(s.real(), z.real()) && same(s.imag(), z.imag())) ? 'S' : 'N';
    print4(std::string(1, cl).c_str(), a, b, c, d);
    std::printf("\n");
  }
}
