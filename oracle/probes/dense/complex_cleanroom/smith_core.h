// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room: the Smith (1962) core established by probe E1
// (strict |c| < |d| for the r = c/d branch, fused den/numerators, true
// divisions). Written from the published algorithm; used by later probes as
// the unscaled baseline.
#pragma once
#include "common.h"

inline C smith_fused(double a, double b, double c, double d) {
  if (std::fabs(c) < std::fabs(d)) {
    double r = c / d;
    double den = std::fma(c, r, d);
    return C(std::fma(a, r, b) / den, std::fma(b, r, -a) / den);
  }
  double r = d / c;
  double den = std::fma(d, r, c);
  return C(std::fma(b, r, a) / den, std::fma(-a, r, b) / den);
}
