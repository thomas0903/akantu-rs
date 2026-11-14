// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: the input stream of
// the clean-room experiment E25 (oracle/probes/dense/complex_cleanroom/
// e25_special_random.cc: same RNG, seed and value mix; 126
// of its 2e7 inputs on which the unfused model disagrees with the x86_64
// runtime) written as records for the Rust replay: every input on which
// model_v6 or the guard variant v6p disagrees with the runtime, and every
// 64th input. Black-box only (D-030).
// usage: v110_e25_dump OUT.bin   (record: 7 little-endian u64, class 9)
#include <vector>

#include "v110_models.h"

C runtime_div(C x, C y);

int main(int argc, char ** argv) {
  if (argc < 2) return 2;
  FILE * out = std::fopen(argv[1], "wb");
  if (!out) return 1;
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
  long total = 0, m6 = 0, mp = 0, written = 0, pred_cd = 0;
  for (long k = 0; k < 20000000L; ++k) {
    double a = pick(), b = pick(), c = pick(), d = pick();
    C z = runtime_div(C(a, b), C(c, d));
    C w6 = model_v6(a, b, c, d), wp = model_guard(a, b, c, d, PRED_2_971, 0x1p971);
    bool e6 = same(w6.real(), z.real()) && same(w6.imag(), z.imag());
    bool ep = same(wp.real(), z.real()) && same(wp.imag(), z.imag());
    ++total;
    m6 += !e6;
    mp += !ep;
    if (!e6 && absmax(c, d) == PRED_2_971) ++pred_cd;
    if (!e6 || !ep || k % 64 == 0) {
      std::uint64_t rec[7] = {9, bits(a), bits(b), bits(c), bits(d), bits(z.real()), bits(z.imag())};
      std::fwrite(rec, 8, 7, out);
      ++written;
    }
  }
  std::fclose(out);
  std::printf("E25-dump total=%ld v6_mismatches=%ld (with max(|c|,|d|) = 0x1.fffffffffffffp+970: %ld) "
              "v6p_mismatches=%ld written=%ld\n", total, m6, pred_cd, mp, written);
}
