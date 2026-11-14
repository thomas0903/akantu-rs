// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E3c (black-box). Locate the exact switch point
// between Smith's numerator fma(b, r, a) and the Baudin & Smith form
// fma(d, b/c, a): operands with |r| densely around DBL_MIN = 2^-1022
// (including r = ±DBL_MIN exactly and its neighbours). For each candidate
// threshold rule the mismatch count is reported.
#include "smith_core.h"

enum Rule { LT_MIN, LE_MIN, LT_2MIN, LT_HALFMIN, NRULE };
static const char * rule_name[NRULE] = {"|r| < 2^-1022", "|r| <= 2^-1022", "|r| < 2^-1021", "|r| < 2^-1023"};

static bool alt(double r, Rule u) {
  double m = std::fabs(r);
  switch (u) {
    case LT_MIN: return m < 0x1p-1022;
    case LE_MIN: return m <= 0x1p-1022;
    case LT_2MIN: return m < 0x1p-1021;
    default: return m < 0x1p-1023;
  }
}

static C model(double a, double b, double c, double d, Rule u) {
  if (std::fabs(c) < std::fabs(d)) {
    double r = c / d;
    double den = std::fma(c, r, d);
    if (!alt(r, u)) return C(std::fma(a, r, b) / den, std::fma(b, r, -a) / den);
    return C(std::fma(c, a / d, b) / den, std::fma(c, b / d, -a) / den);
  }
  double r = d / c;
  double den = std::fma(d, r, c);
  if (!alt(r, u)) return C(std::fma(b, r, a) / den, std::fma(-a, r, b) / den);
  return C(std::fma(d, b / c, a) / den, std::fma(-d, a / c, b) / den);
}

int main() {
  Rng g{0xc0ffee04ULL};
  long mism[NRULE] = {0}, total = 0, at_min = 0;
  for (long k = 0; k < 2000000; ++k) {
    // c in [2^0, 2^200]; target r = d/c: either exactly ±2^-1022 times a
    // significand close to 1, or ±2^[-1024,-1020] random.
    double c = g.expo(0, 200);
    double rr;
    switch (k % 4) {
      case 0: rr = std::ldexp(1.0, -1022); break;
      case 1: rr = std::nextafter(std::ldexp(1.0, -1022), 0.0); break;
      case 2: rr = std::nextafter(std::ldexp(1.0, -1022), 1.0); break;
      default: rr = std::ldexp(1.0 + double(g.next() >> 11) * 0x1.0p-53, g.irange(-1024, -1021)); break;
    }
    if (g.next() & 1) rr = -rr;
    double d = rr * c;  // may round; r recomputed by the division anyway
    if (!std::isfinite(d) || d == 0) continue;
    double r = d / c;
    if (std::fabs(r) == 0x1p-1022) ++at_min;
    // numerators: b*r comparable to a so both terms matter
    double b = g.expo(900, 1000) * ((g.next() & 1) ? 1 : -1);
    double a = b * r * (1.0 + g.uni());
    if (g.next() & 1) {  // also exercise the r = c/d branch
      std::swap(c, d);
      std::swap(a, b);
    }
    C z = oracle_div(C(a, b), C(c, d));
    ++total;
    for (int u = 0; u < NRULE; ++u) {
      C w = model(a, b, c, d, Rule(u));
      if (!same(w.real(), z.real()) || !same(w.imag(), z.imag())) ++mism[u];
    }
  }
  std::printf("E3c total=%ld cases_with_|r|==DBL_MIN=%ld\n", total, at_min);
  for (int u = 0; u < NRULE; ++u) std::printf("rule %-16s mismatches=%ld\n", rule_name[u], mism[u]);
}
