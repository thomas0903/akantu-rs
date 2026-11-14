// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room black-box experiments (D-030) for the geometry module:
// shared helpers. Only the public C++ standard-library API is used; no
// library header or source is read.
#pragma once
#include <cstdint>
#include <cstdio>
#include <vector>

namespace bb {
// xorshift64* (deterministic input generator, not the subject of the test)
struct Rng {
  std::uint64_t s;
  explicit Rng(std::uint64_t seed) : s(seed) {}
  std::uint64_t next() {
    s ^= s >> 12;
    s ^= s << 25;
    s ^= s >> 27;
    return s * 0x2545F4914F6CDD1DULL;
  }
  int below(int n) { return static_cast<int>(next() % static_cast<std::uint64_t>(n)); }
};

inline void print_ints(const char * name, const std::vector<int> & v) {
  std::printf("\"%s\":[", name);
  for (std::size_t i = 0; i < v.size(); ++i)
    std::printf(i ? ",%d" : "%d", v[i]);
  std::printf("]");
}
} // namespace bb
