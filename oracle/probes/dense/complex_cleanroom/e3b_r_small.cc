// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E3b (black-box). Follow-up of E3: the
// Baudin & Smith (2012) alternative numerator d·(b/c) may be used not only
// when r == 0 but whenever r is tiny (e.g. subnormal). Compare switch
// conditions {r == 0, |r| < DBL_MIN} x {fused, unfused} on the E3 regime.
#include "smith_core.h"

static double mad(bool f, double x, double y, double z) { return f ? std::fma(x, y, z) : x * y + z; }

static C model(double a, double b, double c, double d, bool sub, bool fused) {
  auto alt = [&](double r) { return sub ? std::fabs(r) < 0x1p-1022 : r == 0; };
  if (std::fabs(c) < std::fabs(d)) {
    double r = c / d;
    double den = std::fma(c, r, d);
    if (!alt(r)) return C(std::fma(a, r, b) / den, std::fma(b, r, -a) / den);
    return C(mad(fused, c, a / d, b) / den, mad(fused, c, b / d, -a) / den);
  }
  double r = d / c;
  double den = std::fma(d, r, c);
  if (!alt(r)) return C(std::fma(b, r, a) / den, std::fma(-a, r, b) / den);
  return C(mad(fused, d, b / c, a) / den, mad(fused, -d, a / c, b) / den);
}

int main() {
  Rng r{0xc0ffee03ULL};  // same stream as E3
  long mism[4] = {0}, total = 0;
  int shown = 0;
  for (long k = 0; k < 1000000; ++k) {
    double big = r.expo(300, 600), small = r.expo(-700, -500);
    double c, d;
    if (k & 1) { c = big; d = small; } else { c = small; d = big; }
    double p = r.expo(200, 900), q = r.expo(-900, 300);
    if (k & 2) std::swap(p, q);
    double a = (k & 1) ? q : p, b = (k & 1) ? p : q;
    if ((k & 12) == 4) a = 0.0;
    if ((k & 12) == 8) b = 0.0;
    C z = oracle_div(C(a, b), C(c, d));
    ++total;
    for (int v = 0; v < 4; ++v) {
      C w = model(a, b, c, d, v & 1, v & 2);
      bool ok = same(w.real(), z.real()) && same(w.imag(), z.imag());
      if (!ok) ++mism[v];
      if (!ok && v == 3 && shown < 10) {
        ++shown;
        print4("MIS(sub,fused)", a, b, c, d);
        std::printf(" | oracle %a %a | model %a %a | r=%a\n", z.real(), z.imag(), w.real(), w.imag(),
                    std::fabs(c) < std::fabs(d) ? c / d : d / c);
      }
    }
  }
  std::printf("E3b total=%ld\n", total);
  for (int v = 0; v < 4; ++v)
    std::printf("cond=%s grouping=%s mismatches=%ld\n", (v & 1) ? "|r|<DBL_MIN" : "r==0", (v & 2) ? "fused" : "unfused",
                mism[v]);
}
