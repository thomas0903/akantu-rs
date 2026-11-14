// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E4 (black-box). Residual analysis: finite
// operands over the whole double range compared with a model (selected at
// compile time with -DMODEL=model_vN, header model_vN.h). Prints mismatch
// counts by class and samples, used to drive the next hypothesis.
#include <map>
#include <string>
#define STR2(x) #x
#define STR(x) STR2(x)
#include STR(MODEL_HEADER)

static int lg(double x, double y) {
  double m = std::fmax(std::fabs(x), std::fabs(y));
  return m == 0 ? -2000 : std::ilogb(m);
}

int main(int argc, char ** argv) {
  long n = argc > 1 ? std::atol(argv[1]) : 4000000;
  int max_show = argc > 2 ? std::atoi(argv[2]) : 40;
  Rng r{0xc0ffee02ULL};
  long total = 0, mism = 0, mism_nansign = 0;
  std::map<std::string, long> cls;
  int shown = 0;
  for (long k = 0; k < n; ++k) {
    double a, b, c, d;
    switch (k % 4) {
      case 0: a = r.anybits(); b = r.anybits(); c = r.anybits(); d = r.anybits(); break;
      case 1: a = r.expo(-1074, 1023); b = r.expo(-1074, 1023); c = r.expo(-1074, 1023); d = r.expo(-1074, 1023); break;
      case 2:  // near the top of the range
        a = r.expo(900, 1023); b = r.expo(900, 1023); c = r.expo(900, 1023); d = r.expo(900, 1023);
        if (r.next() & 1) a = r.expo(-1074, 1023);
        if (r.next() & 1) d = r.expo(-1074, 1023);
        break;
      default:  // near the bottom of the range
        a = r.expo(-1074, -900); b = r.expo(-1074, -900); c = r.expo(-1074, -900); d = r.expo(-1074, -900);
        if (r.next() & 1) b = r.expo(-1074, 1023);
        if (r.next() & 1) c = r.expo(-1074, 1023);
        break;
    }
    if (c == 0 && d == 0) continue;
    C z = oracle_div(C(a, b), C(c, d));
    C w = MODEL(a, b, c, d);
    ++total;
    bool ok = same(w.real(), z.real()) && same(w.imag(), z.imag());
    bool ok_sign = same(w.real(), z.real(), true) && same(w.imag(), z.imag(), true);
    if (ok && !ok_sign) ++mism_nansign;
    if (!ok) {
      ++mism;
      int ab = lg(a, b), cd = lg(c, d);
      std::string key = std::string(ab >= 1000 ? "AB_huge " : ab <= -950 ? "AB_tiny " : "AB_mid ") +
                        (cd >= 1000 ? "CD_huge " : cd <= -950 ? "CD_tiny " : "CD_mid ") +
                        ((std::isnan(z.real()) || std::isnan(z.imag())) ? "oracleNaN" : std::isinf(z.real()) || std::isinf(z.imag()) ? "oracleInf" : "finite");
      cls[key]++;
      if (shown < max_show) {
        ++shown;
        print4("MIS", a, b, c, d);
        std::printf(" | oracle %a %a | model %a %a | lgAB %d lgCD %d\n", z.real(), z.imag(), w.real(), w.imag(), ab, cd);
      }
    }
  }
  std::printf("E4 total=%ld mismatches=%ld nan_sign_only_mismatches=%ld\n", total, mism, mism_nansign);
  for (auto & [k, v] : cls) std::printf("class %-40s %ld\n", k.c_str(), v);
}
