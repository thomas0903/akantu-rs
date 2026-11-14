// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: black-box location of
// the numerator guard of the small-numerator up-scaling, on the construction
// of the 16 grid inputs where only the numerator maximum sits at the guard
// (v110_cdiv_gen.cc grid: a = +-2^-1074, b = +-X, c = +-1.5*2^970,
// d = -+2^-1074, and the mirrored placements). For each X it reports which
// unfused model (S: up-scaled, U: not) the runtime agrees with on the 64
// sign/placement variants. Only results of the compiled division are observed.
#include <cstdio>
#include <vector>

#include "v110_models.h"

C runtime_div(C x, C y);

int main() {
  std::vector<double> xs;
  for (int k = 960; k <= 970; ++k) xs.push_back(std::ldexp(1.0, k));
  for (int k = 960; k <= 970; ++k) xs.push_back(std::ldexp(1.75, k));
  double x = PRED_2_971;
  for (int j = 0; j < 8; ++j) { xs.push_back(x); x = std::nextafter(x, 0.0); }
  xs.push_back(0x1p971);
  for (double X : xs) {
    long s = 0, u = 0, both = 0, neither = 0;
    for (int m = 0; m < 64; ++m) {
      double t = 0x1p-1074, big = X, cc = 0x1.8p970, dd = 0x1p-1074;
      double a = (m & 1) ? -t : t, b = (m & 2) ? -big : big, c = (m & 4) ? -cc : cc, d = (m & 8) ? -dd : dd;
      if (m & 16) std::swap(a, b);
      if (m & 32) std::swap(c, d);
      C z = runtime_div(C(a, b), C(c, d));
      C ws = model_guard(a, b, c, d, INFINITY, INFINITY), wu = model_guard(a, b, c, d, 0.0, 0.0);
      bool es = same(ws.real(), z.real()) && same(ws.imag(), z.imag());
      bool eu = same(wu.real(), z.real()) && same(wu.imag(), z.imag());
      s += es && !eu; u += eu && !es; both += es && eu; neither += !es && !eu;
    }
    std::printf("ABBISECT X=%a S_only=%ld U_only=%ld both=%ld neither=%ld\n", X, s, u, both, neither);
  }
}
