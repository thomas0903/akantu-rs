// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E22 (black-box). Special operands: every
// combination of a, b, c, d from a set of special and boundary values
// (±0, ±min subnormal, ±DBL_MIN, ±1, ±3, ±2^1000, ±DBL_MAX, ±inf, ±NaN).
// The candidate model is model_v4 followed by the infinity/zero recovery of
// ISO C11 Annex G (G.5.1, EXAMPLE 2) when both parts are NaN, with variants:
//   V0: no recovery:
//   Annex G conditions on the ORIGINAL operands, zero-denominator test
//       c == 0 && d == 0, unfused inner expressions:
//   as V1 on the SCALED operands:
//   as V1 with fused inner expressions;
// Reports mismatches per variant (NaNs equal regardless of sign) and,
// separately, mismatches of the NaN sign bit; prints the first few.
#include <vector>
#include "model_v4.h"

static void scale(double & a, double & b, double & c, double & d) {
  double ab = std::fmax(std::fabs(a), std::fabs(b)), cd = std::fmax(std::fabs(c), std::fabs(d));
  if (cd >= 0x1.fffffffffffffp+1022) { a *= 0.5; b *= 0.5; c *= 0.5; d *= 0.5; }
  else if (cd < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < 0x1p971 && cd < 0x1p971) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
}

static C recover(double a, double b, double c, double d, C z, bool fused) {
  double x = z.real(), y = z.imag();
  if (!(std::isnan(x) && std::isnan(y))) return z;
  auto dot = [&](double p, double q, double s, double t) {  // p*q + s*t
    return fused ? std::fma(p, q, s * t) : p * q + s * t;
  };
  if (c == 0.0 && d == 0.0 && (!std::isnan(a) || !std::isnan(b))) {
    x = std::copysign(INFINITY, c) * a;
    y = std::copysign(INFINITY, c) * b;
  } else if ((std::isinf(a) || std::isinf(b)) && std::isfinite(c) && std::isfinite(d)) {
    a = std::copysign(std::isinf(a) ? 1.0 : 0.0, a);
    b = std::copysign(std::isinf(b) ? 1.0 : 0.0, b);
    x = INFINITY * dot(a, c, b, d);
    y = INFINITY * dot(b, c, -a, d);
  } else if ((std::isinf(c) || std::isinf(d)) && std::isfinite(a) && std::isfinite(b)) {
    c = std::copysign(std::isinf(c) ? 1.0 : 0.0, c);
    d = std::copysign(std::isinf(d) ? 1.0 : 0.0, d);
    x = 0.0 * dot(a, c, b, d);
    y = 0.0 * dot(b, c, -a, d);
  }
  return C(x, y);
}

static C variant(int v, double a, double b, double c, double d) {
  double sa = a, sb = b, sc = c, sd = d;
  scale(sa, sb, sc, sd);
  C z = model_v1(sa, sb, sc, sd);
  switch (v) {
    case 0: return z;
    case 1: return recover(a, b, c, d, z, false);
    case 2: return recover(sa, sb, sc, sd, z, false);
    default: return recover(a, b, c, d, z, true);
  }
}

int main() {
  std::vector<double> base = {0.0, 0x1p-1074, 0x1p-1022, 1.0, 3.0, 0x1p1000, 0x1.fffffffffffffp+1023, INFINITY, NAN};
  std::vector<double> vals;
  for (double v : base) { vals.push_back(v); vals.push_back(-v); }
  const int NV = 4;
  long mis[NV] = {0}, mis_sign[NV] = {0}, total = 0;
  int shown[NV] = {0};
  for (double a : vals) for (double b : vals) for (double c : vals) for (double d : vals) {
    C z = oracle_div(C(a, b), C(c, d));
    ++total;
    for (int v = 0; v < NV; ++v) {
      C w = variant(v, a, b, c, d);
      bool ok = same(w.real(), z.real()) && same(w.imag(), z.imag());
      bool oks = same(w.real(), z.real(), true) && same(w.imag(), z.imag(), true);
      if (!ok) ++mis[v];
      else if (!oks) ++mis_sign[v];
      if (!oks && shown[v] < 6 && v >= 1) {
        ++shown[v];
        std::printf("V%d %s (%a, %a)/(%a, %a): oracle (%a, %a) model (%a, %a)\n", v, ok ? "NaN-sign" : "MISMATCH", a, b, c, d,
                    z.real(), z.imag(), w.real(), w.imag());
      }
    }
  }
  std::printf("E22 total=%ld\n", total);
  for (int v = 0; v < NV; ++v) std::printf("V%d: mismatches=%ld nan-sign-only=%ld\n", v, mis[v], mis_sign[v]);
}
