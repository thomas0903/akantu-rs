// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E21 (black-box). Are the CD < 2^-52 scaling (R1)
// and the small-numerator scaling (R2) exclusive? After R1 every non-zero
// operand is normal, so only an exact zero numerator part could re-trigger
// R2. Inputs: max(|c|,|d|) < 2^-52, one numerator part ±0, the other chosen
// so that after one 2^52 scaling an intermediate is subnormal (a second
// scaling would change the result). Compares the oracle with
//   X = exclusive (else-if, model_v4) and Q = sequential (R2 tested again on
//   the R1-scaled operands, guard 2^971).
#include "model_v4.h"

static C sequential(double a, double b, double c, double d) {
  double cd = std::fmax(std::fabs(c), std::fabs(d));
  if (cd < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  double ab = std::fmax(std::fabs(a), std::fabs(b));
  cd = std::fmax(std::fabs(c), std::fabs(d));
  if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < 0x1p971 && cd < 0x1p971) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  return model_v1(a, b, c, d);
}

int main() {
  Rng g{0xc0ffee21ULL};
  long n[4] = {0};  // X only, Q only, both, neither
  for (long k = 0; k < 5000000; ++k) {
    double c = g.expo(-200, -53), d = c * std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, -g.irange(1, 900));
    double a = g.expo(-1074, -900), b = (k & 1) ? 0.0 : -0.0;
    if (k & 2) std::swap(a, b);
    if (k & 4) std::swap(c, d);
    C z = oracle_div(C(a, b), C(c, d));
    C x = model_v4(a, b, c, d), q = sequential(a, b, c, d);
    bool ex = same(x.real(), z.real()) && same(x.imag(), z.imag());
    bool eq = same(q.real(), z.real()) && same(q.imag(), z.imag());
    n[ex && !eq ? 0 : eq && !ex ? 1 : ex ? 2 : 3]++;
  }
  std::printf("E21 exclusive-only=%ld sequential-only=%ld both=%ld neither=%ld\n", n[0], n[1], n[2], n[3]);
}
