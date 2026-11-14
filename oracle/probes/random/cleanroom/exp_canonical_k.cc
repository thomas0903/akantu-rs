// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Clean-room black-box experiment (D-030): number of
// generator calls made by the oracle's std::generate_canonical<double, 53>
// for generators whose range R = max - min + 1 is 2^k, 2^k - 1 and 2^k + 1.
#include <cstdio>
#include <random>
#include <utility>

static unsigned long calls = 0;
template <unsigned long MIN, unsigned long MAX> struct G {
  using result_type = unsigned long;
  static constexpr result_type min() { return MIN; }
  static constexpr result_type max() { return MAX; }
  result_type operator()() { ++calls; return MIN; }
};
template <unsigned K> void probe() {
  constexpr unsigned long p = (K == 64) ? 0UL : (1UL << K);
  G<0, p - 1> a;   // R = 2^k
  calls = 0; (void)std::generate_canonical<double, 53>(a); unsigned long ca = calls;
  unsigned long cb = 0, cc = 0;
  if constexpr (K >= 2 && K < 64) {
    G<0, p - 2> b; // R = 2^k - 1
    calls = 0; (void)std::generate_canonical<double, 53>(b); cb = calls;
  }
  if constexpr (K < 64) {
    G<0, p> c;     // R = 2^k + 1
    calls = 0; (void)std::generate_canonical<double, 53>(c); cc = calls;
  }
  std::printf("k=%2u  R=2^k: %2lu   R=2^k-1: %2lu   R=2^k+1: %2lu\n", K, ca, cb, cc);
}
template <unsigned... K> void all(std::integer_sequence<unsigned, K...>) { (probe<K + 1>(), ...); }
int main() { all(std::make_integer_sequence<unsigned, 64>{}); }
