// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E27 (black-box). Bulk oracle results for the
// Rust-side check (crates/akantu-linalg/tests/complex_div_cleanroom.rs,
// test `bulk_oracle_file`, enabled by AKANTU_CDIV_BULK=<file>). Writes
// N records of six little-endian u64 (a b c d re im bit patterns) to stdout.
// Mix: whole finite range (E4 distributions), special/finite mix (E25),
// small-numerator cancellations (E16), |r| around DBL_MIN (E3c).
// usage: e27 N > file      (stderr: disagreements with model_v5, expected 0)
#include <cstdlib>
#include <vector>
#include "model_v5.h"

int main(int argc, char ** argv) {
  long n = argc > 1 ? std::atol(argv[1]) : 1000000L;
  Rng g{0xb01c0de27ULL};
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
  long bad = 0;
  for (long k = 0; k < n; ++k) {
    double a, b, c, d;
    switch (k % 6) {
      case 0:
      case 1:
        a = g.expo(-1074, 1023); b = g.expo(-1074, 1023); c = g.expo(-1074, 1023); d = g.expo(-1074, 1023);
        if (k % 12 == 1) { a = g.expo(900, 1023); c = g.expo(900, 1023); }
        if (k % 12 == 7) { b = g.expo(-1074, -900); d = g.expo(-1074, -900); }
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
      default: {
        c = g.expo(0, 200);
        double rr = std::ldexp(1.0 + double(g.next() >> 11) * 0x1.0p-53, g.irange(-1060, -1000));
        d = rr * c;
        b = g.expo(800, 1000);
        a = b * (d / c) * (1.0 + g.uni());
        if (g.next() & 1) { std::swap(c, d); std::swap(a, b); }
        break;
      }
    }
    C z = oracle_div(C(a, b), C(c, d)), w = model_v5(a, b, c, d);
    if (!(same(w.real(), z.real()) && same(w.imag(), z.imag()))) ++bad;
    std::uint64_t rec[6] = {bits(a), bits(b), bits(c), bits(d), bits(z.real()), bits(z.imag())};
    std::fwrite(rec, sizeof rec, 1, stdout);  // aarch64: little-endian
  }
  std::fprintf(stderr, "E27 records=%ld model_v5_disagreements=%ld\n", n, bad);
}
