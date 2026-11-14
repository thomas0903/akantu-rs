// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room model v1: Smith core (E1) + Baudin & Smith (2012)
// alternative numerators when |r| <= DBL_MIN (E3, E3b, E3c). No range
// scaling, no special-value recovery.
#pragma once
#include "common.h"

inline C model_v1(double a, double b, double c, double d) {
  if (std::fabs(c) < std::fabs(d)) {
    double r = c / d;
    double den = std::fma(c, r, d);
    if (std::fabs(r) > 0x1p-1022) return C(std::fma(a, r, b) / den, std::fma(b, r, -a) / den);
    return C(std::fma(c, a / d, b) / den, std::fma(c, b / d, -a) / den);
  }
  double r = d / c;
  double den = std::fma(d, r, c);
  if (std::fabs(r) > 0x1p-1022) return C(std::fma(b, r, a) / den, std::fma(-a, r, b) / den);
  return C(std::fma(d, b / c, a) / den, std::fma(-d, a / c, b) / den);
}
