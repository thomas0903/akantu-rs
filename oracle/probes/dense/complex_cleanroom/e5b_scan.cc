// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E5b (black-box). One-parameter scans: operands
// (a, b, c, d) = (ma·2^pa, mb·2^pb, mc·2^pc, md·2^pd) with one exponent swept
// over a range; prints oracle and model_v1 results side by side so that the
// switch points of the oracle's behaviour can be read off.
// usage: e5b ma pa mb pb mc pc md pd which lo hi   (which in a,b,c,d)
#include <cstdlib>
#include "model_v1.h"

int main(int argc, char ** argv) {
  if (argc < 12) return 1;
  double m[4];
  int e[4];
  for (int i = 0; i < 4; ++i) {
    m[i] = std::strtod(argv[1 + 2 * i], nullptr);
    e[i] = std::atoi(argv[2 + 2 * i]);
  }
  int w = argv[9][0] - 'a', lo = std::atoi(argv[10]), hi = std::atoi(argv[11]);
  for (int x = lo; x <= hi; ++x) {
    e[w] = x;
    double v[4];
    for (int i = 0; i < 4; ++i) v[i] = std::ldexp(m[i], e[i]);
    C z = oracle_div(C(v[0], v[1]), C(v[2], v[3]));
    C y = model_v1(v[0], v[1], v[2], v[3]);
    bool ok = same(z.real(), y.real()) && same(z.imag(), y.imag());
    std::printf("%c=%d oracle (%a, %a) model (%a, %a) %s\n", 'a' + w, x, z.real(), z.imag(), y.real(), y.imag(),
                ok ? "same" : "DIFF");
  }
}
