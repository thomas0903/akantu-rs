// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room model v5 (final candidate):
//  scaling (E6/E7, E13-E21, E19; maxima formed as |x| < |y| ? |y| : |x|, E23):
//    max(|c|,|d|) >= DBL_MAX/2           -> a, b, c, d times 1/2
//    else max(|c|,|d|) < 2^-52           -> a, b, c, d times 2^52
//    else (|a| < DBL_MIN or |b| < DBL_MIN) and max(|a|,|b|) < 2^971 and
//         max(|c|,|d|) < 2^971           -> a, b, c, d times 2^52
//  Smith (1962) core, branch r = c/d iff |c| < |d|, fused den/numerators,
//  Baudin & Smith (2012) numerators when |r| <= DBL_MIN (E1, E3*);
//  ISO C11 Annex G (G.5.1 EXAMPLE 2) recovery when both parts are NaN, on the
//  scaled operands (E22, E23).
#pragma once
#include "model_v1.h"

inline double absmax(double x, double y) {
  x = std::fabs(x);
  y = std::fabs(y);
  return x < y ? y : x;
}

inline C model_v5(double a, double b, double c, double d) {
  double ab = absmax(a, b), cd = absmax(c, d);
  if (cd >= 0x1.fffffffffffffp+1022) { a *= 0.5; b *= 0.5; c *= 0.5; d *= 0.5; }
  else if (cd < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < 0x1p971 && cd < 0x1p971) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  C z = model_v1(a, b, c, d);
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
