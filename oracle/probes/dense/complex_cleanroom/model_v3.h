// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room model v3 = model v1 core + scalings:
//  R1 (E6/E7): max(|c|,|d|) < 2^-52 -> all four operands times 2^52;
//  R2 (E13-E17), else-if: |a| < DBL_MIN or |b| < DBL_MIN (zero included),
//     and the scaling cannot overflow (max(|a|,|b|) and max(|c|,|d|) below
//     the guard G = 2^GUARD_EXP) -> all four operands times 2^52.
// No handling yet of huge operands (denominator overflow).
#pragma once
#include "model_v1.h"
#ifndef GUARD_EXP
#define GUARD_EXP 970
#endif

inline C model_v3(double a, double b, double c, double d) {
  double ab = std::fmax(std::fabs(a), std::fabs(b)), cd = std::fmax(std::fabs(c), std::fabs(d));
  const double G = std::ldexp(1.0, GUARD_EXP);
  if (cd < 0x1p-52) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  } else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < G && cd < G) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  return model_v1(a, b, c, d);
}
