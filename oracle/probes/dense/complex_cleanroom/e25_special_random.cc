// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E25 (black-box). Randomised mix of special
// values (±0, ±inf, ±NaN, ±DBL_MAX, ±DBL_MIN, subnormals, values at the
// scaling thresholds and their neighbours) and random finite values over the
// whole range, compared with model_v5 (NaNs equal regardless of sign).
// Also re-runs the exhaustive E22 grid against model_v5.
#include <vector>
#include "model_v5.h"

int main() {
  std::vector<double> sp = {0.0, 0x1p-1074, 0x1p-1022, std::nextafter(0x1p-1022, 0.0), 0x1p-52,
                            std::nextafter(0x1p-52, 0.0), 1.0, 0x1p971, std::nextafter(0x1p971, 0.0), 0x1p972,
                            0x1.fffffffffffffp+1022, std::nextafter(0x1.fffffffffffffp+1022, 0.0),
                            0x1.fffffffffffffp+1023, INFINITY, NAN};
  Rng r{0xc0ffee25ULL};
  auto pick = [&]() {
    double v;
    switch (r.irange(0, 3)) {
      case 0: v = sp[r.irange(0, (int)sp.size() - 1)]; break;
      case 1: v = r.expo(-1074, 1023); break;
      case 2: v = r.anybits(); break;
      default: v = r.expo(-60, 60); break;
    }
    return (r.next() & 1) ? -v : v;
  };
  long total = 0, mis = 0;
  int shown = 0;
  for (long k = 0; k < 20000000L; ++k) {
    double a = pick(), b = pick(), c = pick(), d = pick();
    C z = oracle_div(C(a, b), C(c, d)), w = model_v5(a, b, c, d);
    ++total;
    if (!(same(w.real(), z.real()) && same(w.imag(), z.imag()))) {
      ++mis;
      if (shown++ < 20) {
        print4("MIS", a, b, c, d);
        std::printf(" | oracle (%a, %a) model (%a, %a)\n", z.real(), z.imag(), w.real(), w.imag());
      }
    }
  }
  std::printf("E25 random total=%ld mismatches=%ld\n", total, mis);
  std::vector<double> base = {0.0, 0x1p-1074, 0x1p-1022, 1.0, 3.0, 0x1p1000, 0x1.fffffffffffffp+1023, INFINITY, NAN};
  std::vector<double> vals;
  for (double v : base) { vals.push_back(v); vals.push_back(-v); }
  long gt = 0, gm = 0;
  for (double a : vals) for (double b : vals) for (double c : vals) for (double d : vals) {
    C z = oracle_div(C(a, b), C(c, d)), w = model_v5(a, b, c, d);
    ++gt;
    gm += !(same(w.real(), z.real()) && same(w.imag(), z.imag()));
  }
  std::printf("E25 grid total=%ld mismatches=%ld\n", gt, gm);
}
