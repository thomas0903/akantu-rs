// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E14 (black-box). Like E9, but the oracle result
// is classified against U (= model_v2) and A (= model_v2 after multiplying
// all four operands by 2^52): u = U only, A = A only, = both, ? neither.
// usage: e14 a b c d which klo khi     (operand_k = operand * 2^k)
#include <cstdlib>
#include <string>
#include "model_v2.h"

static double parse(const char * s) {
  if (std::strncmp(s, "0x", 2) == 0 && std::strchr(s, 'p') == nullptr) return from_bits(std::strtoull(s + 2, nullptr, 16));
  return std::strtod(s, nullptr);
}
static C A52(double a, double b, double c, double d) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  return model_v1(a * 0x1p52, b * 0x1p52, c * 0x1p52, d * 0x1p52);
}

int main(int argc, char ** argv) {
  double v0[4];
  for (int i = 0; i < 4; ++i) v0[i] = parse(argv[1 + i]);
  int w = argv[5][0] - 'a', lo = std::atoi(argv[6]), hi = std::atoi(argv[7]);
  std::string prev;
  int start = lo;
  for (int k = lo; k <= hi + 1; ++k) {
    std::string key = "end";
    if (k <= hi) {
      double v[4] = {v0[0], v0[1], v0[2], v0[3]};
      v[w] = std::ldexp(v0[w], k);
      C z = oracle_div(C(v[0], v[1]), C(v[2], v[3]));
      C u = model_v2(v[0], v[1], v[2], v[3]), s = A52(v[0], v[1], v[2], v[3]);
      bool eu = same(u.real(), z.real()) && same(u.imag(), z.imag());
      bool ea = same(s.real(), z.real()) && same(s.imag(), z.imag());
      key = eu && ea ? "=" : eu ? "u" : ea ? "A" : "?";
    }
    if (key != prev) {
      if (!prev.empty())
        std::printf("k in [%d, %d] (%c = %a .. %a): %s\n", start, k - 1, 'a' + w, std::ldexp(v0[w], start),
                    std::ldexp(v0[w], k - 1), prev.c_str());
      prev = key;
      start = k;
    }
  }
}
