// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E24 (black-box). NaN sign bits: how does the
// oracle propagate the sign of a NaN operand into each part? For a = ±NaN
// or b = ±NaN with ordinary c, d (both branches), print the sign bits of the
// oracle's parts, next to the sign produced on this machine by the scalar
// operations fma(-a, r, b) (negated NaN operand) and fma(a, r, -b).
#include "common.h"

__attribute__((noinline)) double fma_neg_first(double a, double r, double b) { return std::fma(-a, r, b); }
__attribute__((noinline)) double neg(double a) { return -a; }

int main() {
  const double nan_p = NAN, nan_n = -NAN;
  struct T { const char * n; double a, b, c, d; } ts[] = {
      {"a=+nan,c-branch", nan_p, 0.0, 1.0, 0.5}, {"a=-nan,c-branch", nan_n, 0.0, 1.0, 0.5},
      {"b=+nan,c-branch", 0.0, nan_p, 1.0, 0.5}, {"b=-nan,c-branch", 0.0, nan_n, 1.0, 0.5},
      {"a=+nan,d-branch", nan_p, 0.0, 0.5, 1.0}, {"a=-nan,d-branch", nan_n, 0.0, 0.5, 1.0},
      {"b=+nan,d-branch", 0.0, nan_p, 0.5, 1.0}, {"b=-nan,d-branch", 0.0, nan_n, 0.5, 1.0},
      {"c=+nan", 1.0, 1.0, nan_p, 1.0}, {"c=-nan", 1.0, 1.0, nan_n, 1.0},
      {"d=+nan", 1.0, 1.0, 1.0, nan_p}, {"d=-nan", 1.0, 1.0, 1.0, nan_n},
  };
  for (auto & t : ts) {
    C z = oracle_div(C(t.a, t.b), C(t.c, t.d));
    std::printf("%-16s oracle x=%016llx y=%016llx\n", t.n, (unsigned long long)bits(z.real()), (unsigned long long)bits(z.imag()));
  }
  std::printf("fma(-(+nan), 1, 0) = %016llx ; -(+nan) = %016llx\n", (unsigned long long)bits(fma_neg_first(nan_p, 1.0, 0.0)),
              (unsigned long long)bits(neg(nan_p)));
}
