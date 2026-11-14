// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E19 (black-box). Down-scaling for huge
// denominators. Detector: a = b = 1.5·2^1023 makes the numerator a + b·r
// (r = 1) overflow unless the numerator is scaled down; c = d = X. The true
// quotient 1.5·2^1023/X is finite for the X searched. Bisection on X finds
// the smallest denominator magnitude at which the oracle returns a finite
// real part (scaling active). Repeated with c = X, d = X/2 and c = X/2, d = X
// (so that the threshold is seen to apply to max(|c|,|d|)), and with
// d = X, c tiny.
#include <limits>
#include "common.h"

template <class F>
static void bisect(const char * name, F make) {
  auto fin = [&](std::uint64_t u) {
    double v[4];
    make(from_bits(u), v);
    C z = oracle_div(C(v[0], v[1]), C(v[2], v[3]));
    return std::isfinite(z.real()) && std::isfinite(z.imag());
  };
  std::uint64_t lo = bits(0x1p1000), hi = bits(std::numeric_limits<double>::max());
  bool fl = fin(lo), fh = fin(hi);
  if (fl == fh) { std::printf("%s: finite at 2^1000: %d, at DBL_MAX: %d\n", name, fl, fh); return; }
  while (hi - lo > 1) {
    std::uint64_t mid = lo + (hi - lo) / 2;
    if (fin(mid) == fl) lo = mid; else hi = mid;
  }
  double v[4];
  make(from_bits(hi), v);
  C z = oracle_div(C(v[0], v[1]), C(v[2], v[3]));
  std::printf("%s: finite=%d up to X=%a (%016llx); from X=%a (%016llx) finite=%d, result (%a, %a)\n", name, fl,
              from_bits(lo), (unsigned long long)lo, from_bits(hi), (unsigned long long)hi, fh, z.real(), z.imag());
}

int main() {
  const double A = 0x1.8p1023;
  bisect("c=d=X      ", [&](double X, double * v) { v[0] = A; v[1] = A; v[2] = X; v[3] = X; });
  bisect("c=X, d=X/2 ", [&](double X, double * v) { v[0] = A; v[1] = A; v[2] = X; v[3] = X * 0.5; });
  bisect("c=X/2, d=X ", [&](double X, double * v) { v[0] = A; v[1] = A; v[2] = X * 0.5; v[3] = X; });
  bisect("c=X, d=-X  ", [&](double X, double * v) { v[0] = A; v[1] = -A; v[2] = X; v[3] = -X; });
}
