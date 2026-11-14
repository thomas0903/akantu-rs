// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E2 (black-box). Finite operands over the whole
// double range: where does std::complex<double> division depart from the
// unscaled fused Smith core of E1? Prints a histogram of the binary exponents
// (ilogb of max(|a|,|b|) and max(|c|,|d|)) of mismatching cases plus samples.
#include "smith_core.h"
#include <map>

static int lg(double x, double y) {
  double m = std::fmax(std::fabs(x), std::fabs(y));
  return m == 0 ? -2000 : std::ilogb(m);
}

int main() {
  Rng r{0xc0ffee02ULL};
  long total = 0, mism = 0, nan_res = 0;
  std::map<std::pair<int, int>, long> hist;  // (ilogb AB /64, ilogb CD /64)
  int shown = 0;
  for (long k = 0; k < 4000000; ++k) {
    double a, b, c, d;
    if (k % 2 == 0) {
      a = r.anybits(); b = r.anybits(); c = r.anybits(); d = r.anybits();
    } else {
      a = r.expo(-1074, 1023); b = r.expo(-1074, 1023); c = r.expo(-1074, 1023); d = r.expo(-1074, 1023);
    }
    if (c == 0 && d == 0) continue;
    C z = oracle_div(C(a, b), C(c, d));
    C w = smith_fused(a, b, c, d);
    ++total;
    if (std::isnan(z.real()) || std::isnan(z.imag())) ++nan_res;
    if (!same(w.real(), z.real()) || !same(w.imag(), z.imag())) {
      ++mism;
      int ab = lg(a, b), cd = lg(c, d);
      hist[{ab < -1022 ? -1023 : (ab >> 6) << 6, cd < -1022 ? -1023 : (cd >> 6) << 6}]++;
      if (shown < 30) {
        ++shown;
        print4("MIS", a, b, c, d);
        std::printf(" | oracle %a %a | smith %a %a | lgAB %d lgCD %d\n", z.real(), z.imag(), w.real(), w.imag(), ab, cd);
      }
    }
  }
  std::printf("E2 total=%ld mismatches=%ld oracle_nan=%ld\n", total, mism, nan_res);
  for (auto & [k, v] : hist) std::printf("hist lgAB~%d lgCD~%d : %ld\n", k.first, k.second, v);
}
