// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E28 (black-box, D-043). Run in the
// x86_64 oracle image: compares std::complex<double> division with the fused
// model v5 (aarch64 runtime) and the unfused model v6 (both Baudin & Smith
// forms) on E27's mix (whole finite range, special/finite mix, small-numerator
// cancellations, |r| around DBL_MIN) and on the dense-fixture range of E1.
// Prints the disagreement counts (NaN sign bits ignored, as in E4/E27).
// usage: e28 N
#include <cstdlib>
#include <vector>
#include "model_v6.h"

int main(int argc, char ** argv) {
  long n = argc > 1 ? std::atol(argv[1]) : 1000000L;
  Rng g{0xb01c0de28ULL};
  std::vector<double> sp = {0.0, 0x1p-1074, 0x1p-1022, std::nextafter(0x1p-1022, 0.0), 0x1p-52,
                            std::nextafter(0x1p-52, 0.0), 1.0, 0x1p971, 0x1p972, 0x1.fffffffffffffp+1022,
                            std::nextafter(0x1.fffffffffffffp+1022, 0.0), 0x1.fffffffffffffp+1023, INFINITY, NAN};
  auto pick = [&]() {
    double v;
    switch (g.irange(0, 3)) {
      case 0: v = sp[g.irange(0, (int)sp.size() - 1)]; break;
      case 1: v = g.expo(-1074, 1023); break;
      case 2: v = g.anybits(); break;
      default: v = g.expo(-60, 60); break;
    }
    return (g.next() & 1) ? -v : v;
  };
  const char * cls[7] = {"whole", "whole", "special", "special", "cancel", "r_min", "dense"};
  long bad5[7] = {0}, bad61[7] = {0}, bad62[7] = {0}, tot[7] = {0}, branch_bs[7] = {0};
  for (long k = 0; k < n; ++k) {
    double a, b, c, d;
    int m = int(k % 7);
    switch (m) {
      case 0:
      case 1:
        a = g.expo(-1074, 1023); b = g.expo(-1074, 1023); c = g.expo(-1074, 1023); d = g.expo(-1074, 1023);
        if (k % 14 == 1) { a = g.expo(900, 1023); c = g.expo(900, 1023); }
        if (k % 14 == 8) { b = g.expo(-1074, -900); d = g.expo(-1074, -900); }
        break;
      case 2:
      case 3:
        a = pick(); b = pick(); c = pick(); d = pick();
        break;
      case 4: {
        c = std::ldexp(3.0 * (1.0 + 0.3 * g.uni()), g.irange(-50, 60));
        d = c * std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, g.irange(-60, -2));
        a = g.expo(-1074, -990);
        double r = d / c;
        b = -a / r * (1.0 - std::ldexp(1.0 + double(g.next() >> 11) * 0x1p-53, g.irange(-60, -1)));
        if (g.next() & 1) { std::swap(a, b); std::swap(c, d); a = -a; }
        break;
      }
      case 5: {
        c = g.expo(0, 200);
        double rr = std::ldexp(1.0 + double(g.next() >> 11) * 0x1.0p-53, g.irange(-1060, -1000));
        d = rr * c;
        b = g.expo(800, 1000);
        a = b * (d / c) * (1.0 + g.uni());
        if (g.next() & 1) { std::swap(c, d); std::swap(a, b); }
        break;
      }
      default:  // the dense fixture's range (E1 case 0)
        a = g.uni() * std::pow(10.0, g.irange(-12, 12));
        b = g.uni() * std::pow(10.0, g.irange(-12, 12));
        c = g.uni() * std::pow(10.0, g.irange(-12, 12));
        d = g.uni() * std::pow(10.0, g.irange(-12, 12));
        if (c == 0 && d == 0) c = 1;
        break;
    }
    C z = oracle_div(C(a, b), C(c, d));
    C w5 = model_v5(a, b, c, d), w61 = model_v6_bs<1>(a, b, c, d), w62 = model_v6_bs<2>(a, b, c, d);
    ++tot[m];
    if (!(same(w5.real(), z.real()) && same(w5.imag(), z.imag()))) ++bad5[m];
    if (!(same(w61.real(), z.real()) && same(w61.imag(), z.imag()))) ++bad61[m];
    if (!(same(w62.real(), z.real()) && same(w62.imag(), z.imag()))) ++bad62[m];
    if (!(same(w61.real(), w62.real()) && same(w61.imag(), w62.imag()))) ++branch_bs[m];
  }
  long t = 0, b5 = 0, b61 = 0, b62 = 0;
  for (int m = 0; m < 7; ++m) {
    std::printf("E28 class %-7s (k mod 7 = %d) total=%ld model_v5=%ld model_v6_bs1=%ld model_v6_bs2=%ld bs1_ne_bs2=%ld\n",
                cls[m], m, tot[m], bad5[m], bad61[m], bad62[m], branch_bs[m]);
    t += tot[m]; b5 += bad5[m]; b61 += bad61[m]; b62 += bad62[m];
  }
  std::printf("E28 total=%ld model_v5_disagreements=%ld model_v6_bs1_disagreements=%ld model_v6_bs2_disagreements=%ld\n",
              t, b5, b61, b62);
}
