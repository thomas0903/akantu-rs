// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E26 (black-box). Is the observed division
// independent of the caller's compilation flags? The division is compiled in
// its own translation unit (e26_oracle_tu.cc) with -O0 / -O2 / -O3 / -O2
// -ffp-contract=off -DEIGEN_DONT_VECTORIZE (strict oracle flags) /
// -O2 -ffp-contract=fast, and compared with model_v5 (this file is always
// compiled with -O2 -ffp-contract=off) on 5e6 mixed operands.
#include <vector>
#include "model_v5.h"

std::complex<double> oracle_div_tu(std::complex<double> x, std::complex<double> y);

int main() {
  Rng r{0xc0ffee26ULL};
  std::vector<double> sp = {0.0, 0x1p-1074, 0x1p-1022, 0x1p-52, 1.0, 0x1p971, 0x1.fffffffffffffp+1022,
                            0x1.fffffffffffffp+1023, INFINITY, NAN};
  long total = 0, mis = 0;
  for (long k = 0; k < 5000000L; ++k) {
    double v[4];
    for (double & x : v) {
      int t = r.irange(0, 3);
      x = t == 0 ? sp[r.irange(0, (int)sp.size() - 1)] : t == 1 ? r.expo(-1074, 1023) : r.expo(-30, 30);
      if (r.next() & 1) x = -x;
    }
    C z = oracle_div_tu(C(v[0], v[1]), C(v[2], v[3])), w = model_v5(v[0], v[1], v[2], v[3]);
    ++total;
    mis += !(same(w.real(), z.real()) && same(w.imag(), z.imag()));
  }
  std::printf("total=%ld mismatches=%ld\n", total, mis);
}
