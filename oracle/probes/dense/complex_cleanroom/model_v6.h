// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room model v6 (x86_64 oracle, D-043): model v5
// (range scaling, Smith core with the strict |c| < |d| branch, Baudin & Smith
// numerators when |r| <= DBL_MIN, ISO C11 Annex G recovery) with every fused
// multiply-add of the core replaced by a rounded product followed by a
// rounded sum (the hypothesis for Debian's x86_64 runtime, whose baseline ISA
// has no FMA). The Baudin & Smith numerators come in two candidate forms:
//   BS = 1: c * (a / d) + b     (v5's fma(c, a / d, b) unfused)
//   BS = 2: (c * a) / d + b
// Its up-scaling guard `< 2^971` (from model v5) is one ulp too high for the
// x86_64 runtime: the port uses `< 0x1.fffffffffffffp+970` for both maxima
// (COMPLEX_CLEANROOM_LOG.md §9).
// Written from the published algorithms ([Smith 1962], [Baudin & Smith 2012],
// ISO C11 G.5.1); only results of the compiled division are observed.
#pragma once
#include "model_v5.h"

template <int BS> inline C core_v6(double a, double b, double c, double d) {
  if (std::fabs(c) < std::fabs(d)) {
    double r = c / d;
    double den = c * r + d;
    if (std::fabs(r) > 0x1p-1022) return C((a * r + b) / den, (b * r - a) / den);
    if (BS == 1) return C((c * (a / d) + b) / den, (c * (b / d) - a) / den);
    return C(((c * a) / d + b) / den, ((c * b) / d - a) / den);
  }
  double r = d / c;
  double den = d * r + c;
  if (std::fabs(r) > 0x1p-1022) return C((b * r + a) / den, (b - a * r) / den);
  if (BS == 1) return C((d * (b / c) + a) / den, (b - d * (a / c)) / den);
  return C(((d * b) / c + a) / den, (b - (d * a) / c) / den);
}

template <int BS> inline C model_v6_bs(double a, double b, double c, double d) {
  double ab = absmax(a, b), cd = absmax(c, d);
  if (cd >= 0x1.fffffffffffffp+1022) { a *= 0.5; b *= 0.5; c *= 0.5; d *= 0.5; }
  else if (cd < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < 0x1p971 && cd < 0x1p971) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  C z = core_v6<BS>(a, b, c, d);
  double x = z.real(), y = z.imag();
  if (std::isnan(x) && std::isnan(y)) {
    if (c == 0.0 && d == 0.0 && (!std::isnan(a) || !std::isnan(b))) {
      x = std::copysign(INFINITY, c) * a;
      y = std::copysign(INFINITY, c) * b;
    } else if ((std::isinf(a) || std::isinf(b)) && std::isfinite(c) && std::isfinite(d)) {
      a = std::copysign(std::isinf(a) ? 1.0 : 0.0, a);
      b = std::copysign(std::isinf(b) ? 1.0 : 0.0, b);
      x = INFINITY * (a * c + b * d);
      y = INFINITY * (b * c - a * d);
    } else if ((std::isinf(c) || std::isinf(d)) && std::isfinite(a) && std::isfinite(b)) {
      c = std::copysign(std::isinf(c) ? 1.0 : 0.0, c);
      d = std::copysign(std::isinf(d) ? 1.0 : 0.0, d);
      x = 0.0 * (a * c + b * d);
      y = 0.0 * (b * c - a * d);
    }
  }
  return C(x, y);
}

inline C model_v6(double a, double b, double c, double d) { return model_v6_bs<1>(a, b, c, d); }
