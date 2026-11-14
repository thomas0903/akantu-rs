// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E9 (black-box). Scan one operand by powers of two
// (operand_k = operand·2^k) starting from a given input, and classify the
// oracle result per part as: M (= model_v2), E (= exact quotient correctly
// rounded from binary128, and != model), or ? (neither). Runs of equal
// classification are merged.
// usage: e9 a b c d which klo khi
#include <cstdlib>
#include <string>
#include "model_v2.h"

static double parse(const char * s) {
  if (std::strncmp(s, "0x", 2) == 0 && std::strchr(s, 'p') == nullptr) return from_bits(std::strtoull(s + 2, nullptr, 16));
  return std::strtod(s, nullptr);
}

int main(int argc, char ** argv) {
  double v0[4];
  for (int i = 0; i < 4; ++i) v0[i] = parse(argv[1 + i]);
  int w = argv[5][0] - 'a', lo = std::atoi(argv[6]), hi = std::atoi(argv[7]);
  std::string prev;
  int start = lo;
  auto cls = [](double o, double m, double e) { return same(o, m) ? 'M' : same(o, e) ? 'E' : '?'; };
  for (int k = lo; k <= hi + 1; ++k) {
    std::string key = "end";
    if (k <= hi) {
      double v[4] = {v0[0], v0[1], v0[2], v0[3]};
      v[w] = std::ldexp(v0[w], k);
      C z = oracle_div(C(v[0], v[1]), C(v[2], v[3]));
      C m = model_v2(v[0], v[1], v[2], v[3]);
      long double A = v[0], B = v[1], Cc = v[2], D = v[3], den = Cc * Cc + D * D;
      double X = (double)((A * Cc + B * D) / den), Y = (double)((B * Cc - A * D) / den);
      key = std::string(1, cls(z.real(), m.real(), X)) + cls(z.imag(), m.imag(), Y);
    }
    if (key != prev) {
      if (!prev.empty()) std::printf("k in [%d, %d] (%c = %a .. %a): x:%c y:%c\n", start, k - 1, 'a' + w,
                                     std::ldexp(v0[w], start), std::ldexp(v0[w], k - 1), prev[0], prev[1]);
      prev = key;
      start = k;
    }
  }
}
