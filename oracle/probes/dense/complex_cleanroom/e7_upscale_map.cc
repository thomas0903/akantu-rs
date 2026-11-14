// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E7 (black-box). Map of the numerator up-scaling
// factor as a function of the denominator magnitude. Operands
// a = A, b = 0, c = 2^q, d = 2^(q-60) (r = 2^-60, c-branch): the true imag
// part y = -A·2^(-60-q)/(1+2^-120) is finite for A < 2^(1084+q). If the
// routine scales the numerator by 2^s, y becomes infinite from A = 2^(1024-s)
// on. For each q, the smallest A (bisection on its bit pattern) with an
// infinite y is reported as s = 1024 - log2(A_crit) (exact power of two
// expected); q values with the same result are merged. Also reported: the
// same with the roles of c and d swapped (d = 2^q, c = 2^(q-60), b = A, a = 0
// -> real part).
#include <limits>
#include "common.h"

static double crit(int q, bool swapped) {
  std::uint64_t lo = bits(0x1p-1074), hi = bits(std::numeric_limits<double>::max());
  auto inf_at = [&](std::uint64_t u) {
    double A = from_bits(u);
    C z = swapped ? oracle_div(C(0.0, A), C(std::ldexp(1.0, q - 60), std::ldexp(1.0, q)))
                  : oracle_div(C(A, 0.0), C(std::ldexp(1.0, q), std::ldexp(1.0, q - 60)));
    return std::isinf(swapped ? z.real() : z.imag());
  };
  if (!inf_at(hi)) return 0;  // never
  if (inf_at(lo)) return -1;
  while (hi - lo > 1) {
    std::uint64_t mid = lo + (hi - lo) / 2;
    if (inf_at(mid)) hi = mid; else lo = mid;
  }
  return from_bits(hi);
}

int main() {
  for (int sw = 0; sw < 2; ++sw) {
    std::printf("# %s\n", sw ? "d = 2^q > c = 2^(q-60), numerator b = A (real part tested)"
                             : "c = 2^q > d = 2^(q-60), numerator a = A (imag part tested)");
    double prev = -2;
    int start = -1014;
    bool prev_true_finite = false;
    for (int q = -1014; q <= 1023; ++q) {
      double A = crit(q, sw);
      // true result finite at A_crit?  y ~ A * 2^(-60-q)
      bool tf = A > 0 && std::ilogb(A) - 60 - q < 1023;
      double key = tf ? A : -3;  // -3: the overflow is genuine (not caused by scaling)
      if (key != prev) {
        if (prev != -2) {
          if (prev == -3) std::printf("q in [%d, %d]: no spurious overflow (true result overflows first)\n", start, q - 1);
          else std::printf("q in [%d, %d]: A_crit = %a  => scale 2^%d\n", start, q - 1, prev, 1024 - std::ilogb(prev));
        }
        prev = key;
        start = q;
      }
      prev_true_finite = tf;
    }
    if (prev == -3) std::printf("q in [%d, 1023]: no spurious overflow (true result overflows first)\n", start);
    else std::printf("q in [%d, 1023]: A_crit = %a  => scale 2^%d\n", start, prev, 1024 - std::ilogb(prev));
    (void)prev_true_finite;
  }
}
