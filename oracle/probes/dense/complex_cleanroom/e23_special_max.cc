// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E23 (black-box). Follow-up of E22: how are
// max(|c|,|d|) and max(|a|,|b|) formed when an operand is NaN? Variants:
//   F: fmax (NaN ignored);
//   L: |x| < |y| ? |y| : |x|   (a NaN first operand propagates);
//   G: |x| > |y| ? |x| : |y|   (a NaN first operand is ignored).
// Recovery as E22 V2 (Annex G on the scaled operands). Same operand grid as
// E22. Reports mismatches (NaNs equal) and NaN-sign-only mismatches per
// combination (cd form, ab form).
#include <vector>
#include "model_v1.h"

static double mx(int f, double x, double y) {
  x = std::fabs(x); y = std::fabs(y);
  switch (f) {
    case 0: return std::fmax(x, y);
    case 1: return x < y ? y : x;
    default: return x > y ? x : y;
  }
}

static C recover(double a, double b, double c, double d, C z) {
  double x = z.real(), y = z.imag();
  if (!(std::isnan(x) && std::isnan(y))) return z;
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
  return C(x, y);
}

static C model(double a, double b, double c, double d, int fcd, int fab) {
  double ab = mx(fab, a, b), cd = mx(fcd, c, d);
  if (cd >= 0x1.fffffffffffffp+1022) { a *= 0.5; b *= 0.5; c *= 0.5; d *= 0.5; }
  else if (cd < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < 0x1p971 && cd < 0x1p971) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  return recover(a, b, c, d, model_v1(a, b, c, d));
}

int main(int argc, char **) {
  bool show = argc > 1;
  int shown = 0;
  std::vector<double> base = {0.0, 0x1p-1074, 0x1p-1022, 1.0, 3.0, 0x1p1000, 0x1.fffffffffffffp+1023, INFINITY, NAN};
  std::vector<double> vals;
  for (double v : base) { vals.push_back(v); vals.push_back(-v); }
  long mis[3][3] = {{0}}, mis_sign[3][3] = {{0}}, total = 0;
  for (double a : vals) for (double b : vals) for (double c : vals) for (double d : vals) {
    C z = oracle_div(C(a, b), C(c, d));
    ++total;
    for (int i = 0; i < 3; ++i) for (int j = 0; j < 3; ++j) {
      C w = model(a, b, c, d, i, j);
      bool ok = same(w.real(), z.real()) && same(w.imag(), z.imag());
      bool oks = same(w.real(), z.real(), true) && same(w.imag(), z.imag(), true);
      if (!ok) ++mis[i][j];
      else if (!oks) ++mis_sign[i][j];
      if (show && i == 1 && j == 1 && ok && !oks && shown < 40) {
        ++shown;
        std::printf("NANSIGN (%a, %a)/(%a, %a): oracle (%a, %a) model (%a, %a)\n", a, b, c, d, z.real(), z.imag(), w.real(), w.imag());
      }
    }
  }
  const char * nm = "FLG";
  std::printf("E23 total=%ld\n", total);
  for (int i = 0; i < 3; ++i) for (int j = 0; j < 3; ++j)
    std::printf("cd=%c ab=%c: mismatches=%ld nan-sign-only=%ld\n", nm[i], nm[j], mis[i][j], mis_sign[i][j]);
}
