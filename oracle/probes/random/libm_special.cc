// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Oracle probe: glibc `pow` on the
// full Cartesian product of a set of special values (signed zeros, ±1, odd and
// even integers, half-integers, subnormals, normal/overflow/underflow
// boundaries, values next to 1, exponent transitions, infinities, quiet and
// signalling NaNs of both signs), and `log`, `exp`, `tan` on the same set, for
// the pure-Rust glibc ports in crates/akantu-core/src/random/libm.rs. glibc
// (LGPL-2.1-or-later) is used as a black box here.
//
// Output: JSON on stdout, committed as
// crates/akantu-core/tests/fixtures/random/libm_special_cartesian.json.
// Results are IEEE-754 bit patterns; NaN results are canonicalised to
// 0x7ff8000000000000 (payloads and signs are not significant).
//
// Built and run by oracle/probes/random/run.sh with the probe flags plus
// -fno-builtin; the function pointers are volatile, so GCC cannot
// constant-fold the calls.

#include <bit>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <gnu/libc-version.h>
#include <vector>

#ifndef PROBE_FLAGS
#define PROBE_FLAGS "unknown"
#endif

namespace {

using u64 = std::uint64_t;

double (*volatile p_log)(double) = std::log;
double (*volatile p_exp)(double) = std::exp;
double (*volatile p_pow)(double, double) = std::pow;
double (*volatile p_tan)(double) = std::tan;

u64 bits(double x) { return std::bit_cast<u64>(x); }
double from_bits(u64 b) { return std::bit_cast<double>(b); }
u64 canon(double x) { return std::isnan(x) ? 0x7ff8000000000000ULL : bits(x); }

} // namespace

int main() {
  // Magnitudes (bit patterns); each is used with both signs.
  const std::vector<u64> magnitudes = {
      0x0000000000000000ULL, // 0
      0x0000000000000001ULL, // smallest subnormal
      0x000fffffffffffffULL, // largest subnormal
      0x0010000000000000ULL, // smallest normal
      0x3c90000000000000ULL, // 2^-54
      0x3fe0000000000000ULL, // 0.5
      0x3fefffffffffffffULL, // 1 - 2^-53
      0x3ff0000000000000ULL, // 1
      0x3ff0000000000001ULL, // 1 + 2^-52
      0x3ff8000000000000ULL, // 1.5
      0x4000000000000000ULL, // 2
      0x4004000000000000ULL, // 2.5
      0x4008000000000000ULL, // 3
      0x4024000000000000ULL, // 10
      0x408ff80000000000ULL, // 1023
      0x4090000000000000ULL, // 1024
      0x4090040000000000ULL, // 1025
      0x4090c80000000000ULL, // 1074
      0x4090cc0000000000ULL, // 1075
      0x433fffffffffffffULL, // 2^53 - 1 (odd)
      0x4340000000000000ULL, // 2^53 (even)
      0x43e0000000000000ULL, // 2^63
      0x43f0000000000000ULL, // 2^64
      0x7fefffffffffffffULL, // largest finite
      0x7ff0000000000000ULL, // inf
      0x7ff8000000000000ULL, // quiet NaN
      0x7ff4000000000000ULL, // signalling NaN
      0x7ff0000000000001ULL, // signalling NaN (smallest payload)
      0x7fffffffffffffffULL, // quiet NaN (all payload bits)
  };
  std::vector<double> xs;
  for (u64 m : magnitudes) {
    xs.push_back(from_bits(m));
    xs.push_back(from_bits(m | 0x8000000000000000ULL));
  }
  std::printf("{\"meta\":{\"glibc\":\"%s\",\"arch\":\"%s\",\"flags\":\"%s\"},\n\"x\":[",
              gnu_get_libc_version(),
#if defined(__aarch64__)
              "aarch64"
#elif defined(__x86_64__)
              "x86_64"
#else
              "other"
#endif
              , PROBE_FLAGS " -fno-builtin");
  for (std::size_t i = 0; i < xs.size(); ++i) {
    std::printf("%s\"%016llx\"", i ? "," : "", (unsigned long long)bits(xs[i]));
  }
  auto row = [&](const char * name, double (*f)(double)) {
    std::printf("],\n\"%s\":[", name);
    for (std::size_t i = 0; i < xs.size(); ++i) {
      std::printf("%s\"%016llx\"", i ? "," : "", (unsigned long long)canon(f(xs[i])));
    }
  };
  row("log", p_log);
  row("exp", p_exp);
  row("tan", p_tan);
  // pow[i][j] = pow(x[i], x[j]).
  std::printf("],\n\"pow\":[\n");
  for (std::size_t i = 0; i < xs.size(); ++i) {
    std::printf("%s[", i ? ",\n" : "");
    for (std::size_t j = 0; j < xs.size(); ++j) {
      std::printf("%s\"%016llx\"", j ? "," : "", (unsigned long long)canon(p_pow(xs[i], xs[j])));
    }
    std::printf("]");
  }
  std::printf("\n]}\n");
  return 0;
}
