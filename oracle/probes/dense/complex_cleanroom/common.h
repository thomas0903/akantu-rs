// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probes for std::complex<double> division (D-030
// amendment). Shared helpers: the black-box oracle call, bit printing and a
// deterministic RNG. Only results of the compiled division are observed.
#pragma once
#include <cmath>
#include <complex>
#include <cstdint>
#include <cstdio>
#include <cstring>

using C = std::complex<double>;

// Division under test. noinline + run-time operands: no constant folding.
__attribute__((noinline)) inline C oracle_div(C x, C y) { return x / y; }

inline std::uint64_t bits(double v) {
  std::uint64_t u;
  std::memcpy(&u, &v, 8);
  return u;
}
inline double from_bits(std::uint64_t u) {
  double v;
  std::memcpy(&v, &u, 8);
  return v;
}
// bit equality; any two NaNs compare equal unless `nan_sign` is set, in which
// case their sign bits must agree as well.
inline bool same(double x, double y, bool nan_sign = false) {
  if (std::isnan(x) && std::isnan(y)) return !nan_sign || (std::signbit(x) == std::signbit(y));
  return bits(x) == bits(y);
}

struct Rng {  // splitmix64
  std::uint64_t s;
  std::uint64_t next() {
    std::uint64_t z = (s += 0x9e3779b97f4a7c15ULL);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    return z ^ (z >> 31);
  }
  double uni() { return double(next() >> 11) * 0x1.0p-53 * 2.0 - 1.0; }  // [-1, 1)
  int irange(int lo, int hi) { return lo + int(next() % std::uint64_t(hi - lo + 1)); }
  // random sign, random significand, binary exponent in [lo, hi]
  double expo(int lo, int hi) {
    double m = 1.0 + double(next() >> 11) * 0x1.0p-53;
    double v = std::ldexp(m, irange(lo, hi));
    return (next() & 1) ? -v : v;
  }
  // any finite double (uniform bit pattern, rejecting non-finite)
  double anybits() {
    for (;;) {
      double v = from_bits(next());
      if (std::isfinite(v)) return v;
    }
  }
};

inline void print4(const char * tag, double a, double b, double c, double d) {
  std::printf("%s %016llx %016llx %016llx %016llx", tag, (unsigned long long)bits(a), (unsigned long long)bits(b),
              (unsigned long long)bits(c), (unsigned long long)bits(d));
}
