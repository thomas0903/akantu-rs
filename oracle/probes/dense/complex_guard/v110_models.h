// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: the
// clean-room unfused model (oracle/probes/dense/complex_cleanroom/model_v6.h,
// core_v6<1>) with the two guards of the small-numerator up-scaling as
// parameters, so that black-box runs can tell guard values apart. Written
// from the author's clean-room model and the published algorithms only.
#pragma once
#include "model_v6.h"

// Same steps as model_v6_bs<1>, with `cd_guard` / `ab_guard` in place of the
// fixed 2^971 of the small-numerator rule (rule 6 of COMPLEX_CLEANROOM_LOG.md).
// cd_guard = +inf: always scale on the trigger; cd_guard = 0: never.
inline C model_guard(double a, double b, double c, double d, double cd_guard, double ab_guard) {
  double ab = absmax(a, b), cd = absmax(c, d);
  if (cd >= 0x1.fffffffffffffp+1022) { a *= 0.5; b *= 0.5; c *= 0.5; d *= 0.5; }
  else if (cd < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < ab_guard && cd < cd_guard) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  C z = core_v6<1>(a, b, c, d);
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

// The largest double below 2^971 (= DBL_MAX / 2^53).
constexpr double PRED_2_971 = 0x1.fffffffffffffp+970;
