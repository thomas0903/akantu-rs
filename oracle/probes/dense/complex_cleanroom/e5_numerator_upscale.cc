// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E5 (black-box). Detect an up-scaling of the
// numerator by overflow. Operands a = 1.5·2^p, b = 0, c = 1.25·2^q,
// d = 1.75·2^(q-40): the true quotient has y ≈ -1.5·1.75/1.25·2^(p-q-40)
// (finite whenever p - q < 1060), x ≈ 1.2·2^(p-q) (finite when p - q < 1023).
// If the routine multiplies a by 2^s before dividing, y becomes infinite as
// soon as p + s >= 1024. For every q the smallest p with an infinite y
// (while p - q < 1000, so the true y is finite) is printed; ranges of q with
// the same p_crit are merged. p_crit = none means no overflow seen for p<=1023.
#include "common.h"

int main() {
  int prev = -99999, start_q = -1074;
  auto pcrit = [](int q) {
    for (int p = -1074; p <= 1023; ++p) {
      if (p - q >= 1000) break;
      double a = std::ldexp(1.5, p), b = 0.0, c = std::ldexp(1.25, q), d = std::ldexp(1.75, q - 40);
      C z = oracle_div(C(a, b), C(c, d));
      if (std::isinf(z.imag())) return p;
    }
    return 99999;
  };
  for (int q = -1034; q <= 1023; ++q) {
    int pc = pcrit(q);
    if (pc != prev) {
      if (prev != -99999) std::printf("q in [%d, %d]: p_crit = %d\n", start_q, q - 1, prev);
      prev = pc;
      start_q = q;
    }
  }
  std::printf("q in [%d, %d]: p_crit = %d\n", start_q, 1023, prev);
}
