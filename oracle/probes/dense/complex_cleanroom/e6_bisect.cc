// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E6 (black-box). Bisection on the IEEE bit
// pattern of one operand: given the other three operands and a bracket
// [lo, hi] (positive doubles, hex bit patterns) in which the oracle's result
// "class" changes, find the adjacent pair of doubles where it changes. The
// class of a result is "imag part infinite" (mode inf_y), "real part
// infinite" (inf_x) or "equal to model_v1" (model).
// usage: e6 a b c d which lo hi mode      (a..d as hex bits or decimal)
#include <cstdlib>
#include <string>
#include "model_v1.h"

static double parse(const char * s) {
  if (std::strncmp(s, "0x", 2) == 0 && std::strchr(s, 'p') == nullptr) return from_bits(std::strtoull(s + 2, nullptr, 16));
  return std::strtod(s, nullptr);
}

int main(int argc, char ** argv) {
  if (argc < 9) return 1;
  double v[4];
  for (int i = 0; i < 4; ++i) v[i] = parse(argv[1 + i]);
  int w = argv[5][0] - 'a';
  std::uint64_t lo = bits(parse(argv[6])), hi = bits(parse(argv[7]));
  std::string mode = argv[8];
  auto cls = [&](std::uint64_t u) {
    double t[4] = {v[0], v[1], v[2], v[3]};
    t[w] = std::signbit(v[w]) ? -from_bits(u) : from_bits(u);
    C z = oracle_div(C(t[0], t[1]), C(t[2], t[3]));
    if (mode == "inf_y") return std::isinf(z.imag());
    if (mode == "inf_x") return std::isinf(z.real());
    C m = model_v1(t[0], t[1], t[2], t[3]);
    return same(z.real(), m.real()) && same(z.imag(), m.imag());
  };
  bool cl = cls(lo), ch = cls(hi);
  std::printf("class(lo=%a)=%d class(hi=%a)=%d\n", from_bits(lo), cl, from_bits(hi), ch);
  if (cl == ch) { std::printf("no change in bracket\n"); return 0; }
  while (hi - lo > 1) {
    std::uint64_t mid = lo + (hi - lo) / 2;
    if (cls(mid) == cl) lo = mid; else hi = mid;
  }
  std::printf("switch between %a (%016llx, class %d) and %a (%016llx, class %d)\n", from_bits(lo),
              (unsigned long long)lo, cl, from_bits(hi), (unsigned long long)hi, ch);
}
