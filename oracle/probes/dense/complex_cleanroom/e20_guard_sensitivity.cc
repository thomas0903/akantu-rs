// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E20 (black-box). Sensitivity of the results to
// the overflow guard of the small-numerator scaling (R2). Inputs: one
// numerator part subnormal or zero (R2 trigger), the other parts with
// magnitudes up to DBL_MAX/2 (biased to 2^[900,1023]), all orientations.
// For guard exponents gab (on max(|a|,|b|)) and gcd (on max(|c|,|d|)) the
// number of mismatches with the oracle is printed; plus the exact boundary
// for c = d = X (den = 2X) found by bisection over X in [2^969, 2^973].
#include <limits>
#include "model_v1.h"

static C model(double a, double b, double c, double d, double Gab, double Gcd) {
  double ab = std::fmax(std::fabs(a), std::fabs(b)), cd = std::fmax(std::fabs(c), std::fabs(d));
  if (cd >= 0x1.fffffffffffffp+1022) { a *= 0.5; b *= 0.5; c *= 0.5; d *= 0.5; }
  else if (cd < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  else if ((std::fabs(a) < 0x1p-1022 || std::fabs(b) < 0x1p-1022) && ab < Gab && cd < Gcd) {
    a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52;
  }
  return model_v1(a, b, c, d);
}

int main() {
  const int G[] = {60, 120, 500, 900, 969, 970, 971, 972, 973, 1000, 1024};
  const int NG = sizeof G / sizeof *G;
  long mab[NG] = {0}, mcd[NG] = {0}, total = 0;
  Rng r{0xc0ffee20ULL};
  for (long k = 0; k < 20000000L; ++k) {
    double a = (k % 3 == 0) ? 0.0 : r.expo(-1074, -1023);
    double b = r.expo((k & 4) ? 900 : -1074, 1023);
    double c = r.expo((k & 8) ? 900 : -60, 1023), d = r.expo((k & 16) ? 900 : -1074, 1023);
    if (k & 32) d = (r.next() & 1) ? c : -c;
    if (k & 64) d = c * (1.0 - std::ldexp(r.uni() + 1.0, -r.irange(1, 60)));
    if (k & 128) { std::swap(a, b); }
    if (k & 256) { std::swap(c, d); }
    if (c == 0 && d == 0) continue;
    C z = oracle_div(C(a, b), C(c, d));
    ++total;
    for (int i = 0; i < NG; ++i) {
      C w1 = model(a, b, c, d, std::ldexp(1.0, G[i]), 0x1p970);
      C w2 = model(a, b, c, d, 0x1p970, std::ldexp(1.0, G[i]));
      mab[i] += !(same(w1.real(), z.real()) && same(w1.imag(), z.imag()));
      mcd[i] += !(same(w2.real(), z.real()) && same(w2.imag(), z.imag()));
    }
  }
  std::printf("E20 total=%ld\n", total);
  for (int i = 0; i < NG; ++i)
    std::printf("guard 2^%d: on AB (CD guard 2^970) mismatches=%ld ; on CD (AB guard 2^970) mismatches=%ld\n", G[i], mab[i],
                mcd[i]);
  // c = d = X, a = 0, b = 1: scaled den 2X·2^52 overflows from X = 2^971
  auto scaled_like = [](double X) {
    C z = oracle_div(C(0.0, 1.0), C(X, X));
    return z.real() == 0.0 && z.imag() == 0.0;
  };
  int sw = 0;
  for (double X = 0x1p969; X <= 0x1p973; X = std::nextafter(X, INFINITY)) {
    if (scaled_like(X)) { std::printf("c=d=X: oracle result zero (scaled-like) at X=%a\n", X); sw = 1; break; }
    if (X > 0x1.0000000001p969 && X < 0x1.fffffffffp972) X = std::nextafter(X * (1 + 0x1p-12), 0.0);  // coarse walk
  }
  if (!sw) std::printf("c=d=X, a=0, b=1: oracle never zero for X in [2^969, 2^973] (no scaling there)\n");
}
