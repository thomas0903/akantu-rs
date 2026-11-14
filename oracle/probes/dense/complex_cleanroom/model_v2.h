// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room model v2 = model v1 + hypothesis H2 (from E6/E7): when
// max(|c|, |d|) < 2^-52 all four operands are multiplied by 2^52 (variant
// SCALE_ALL=1) or only the numerator, with the quotient multiplied back by
// 2^-52 (SCALE_ALL=0 is not consistent with a spurious overflow alone, but is
// kept to discriminate).
#pragma once
#include "model_v1.h"
#ifndef SCALE_ALL
#define SCALE_ALL 1
#endif

inline C model_v2(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) {
    a *= 0x1p52; b *= 0x1p52;
    if (SCALE_ALL) { c *= 0x1p52; d *= 0x1p52; }
  }
  return model_v1(a, b, c, d);
}
