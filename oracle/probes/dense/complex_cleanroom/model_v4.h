// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room model v4 = model v3 + R3 (E19): max(|c|,|d|) >=
// DBL_MAX/2 -> all four operands times 1/2 (the factor is not observable for
// any power of two >= 2^-970; 1/2 as in Baudin & Smith 2012). Finite inputs
// only (no Annex G recovery yet).
#pragma once
#include "model_v1.h"
#ifndef GUARD_EXP
#define GUARD_EXP 970
#endif

inline C model_v4(double a, double b, double c, double d) {
  double ab = std::fmax(std::fabs(a), std::fabs(b)), cd = std::fmax(std::fabs(c), std::fabs(d));
  const double G = std::ldexp(1.0, GUARD_EXP);
  if (cd >= 0x1.fffffffffffffp+1022) {
    a *= 0.5; b *= 0.5; c *= 0.5; d *= 0.5;
  } else if (cd < 0x1p-52) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  } else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < G && cd < G) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  return model_v1(a, b, c, d);
}
