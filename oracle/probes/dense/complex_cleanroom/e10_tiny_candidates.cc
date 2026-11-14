// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E10 (black-box). Collect the cases where the
// oracle departs from model_v2 with a finite result and a moderate
// denominator (|c|,|d| < 2^1000), then score candidate rules for small
// numerator terms on that population: per candidate, the number of cases
// where both parts match bit for bit.
#include <vector>
#include "model_v2.h"

struct Case { double a, b, c, d; C z; };

// Candidates: numerator scaled by 2^s (and result multiplied by 2^-s) when
// max(|a|,|b|) < 2^t, applied after the model_v2 denominator scaling.
static C scaled_num(double a, double b, double c, double d, int t, int s) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  bool sc = std::fmax(std::fabs(a), std::fabs(b)) < std::ldexp(1.0, t);
  if (sc) { a = std::ldexp(a, s); b = std::ldexp(b, s); }
  C w = model_v1(a, b, c, d);
  if (sc) w = C(std::ldexp(w.real(), -s), std::ldexp(w.imag(), -s));
  return w;
}
// Candidate: BS (2012) compreal-like split when the product term is not
// normal: x = a/den + (b/den)*r (fused or unfused).
static C split(double a, double b, double c, double d, bool fused) {
  if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { a *= 0x1p52; b *= 0x1p52; c *= 0x1p52; d *= 0x1p52; }
  auto comb = [&](double p, double q, double r, double den) {  // p + q*r over den
    double qr = q * r;
    if (std::fabs(qr) >= 0x1p-1022 || qr == 0) return std::fma(q, r, p) / den;
    return fused ? std::fma(q / den, r, p / den) : p / den + (q / den) * r;
  };
  if (std::fabs(c) < std::fabs(d)) {
    double r = c / d, den = std::fma(c, r, d);
    if (std::fabs(r) <= 0x1p-1022) return model_v1(a, b, c, d);
    return C(comb(b, a, r, den), comb(-a, b, r, den));
  }
  double r = d / c, den = std::fma(d, r, c);
  if (std::fabs(r) <= 0x1p-1022) return model_v1(a, b, c, d);
  return C(comb(a, b, r, den), comb(b, -a, r, den));
}

int main() {
  Rng r{0xc0ffee10ULL};
  std::vector<Case> pop;
  long tried = 0;
  while (pop.size() < 3000 && tried < 400000000L) {
    ++tried;
    double a, b, c, d;
    switch (tried % 3) {
      case 0: a = r.expo(-1074, -900); b = r.expo(-400, 100); c = r.expo(-60, 60); d = r.expo(-1074, 0); break;
      case 1: a = r.expo(-1074, 100); b = r.expo(-1074, 100); c = r.expo(-200, 200); d = r.expo(-1074, 200); break;
      default: a = r.expo(-1074, -1000); b = r.expo(-1074, -1000); c = r.expo(-50, 50); d = r.expo(-1074, 50); break;
    }
    if (r.next() & 1) { std::swap(a, b); std::swap(c, d); }
    C z = oracle_div(C(a, b), C(c, d));
    if (!std::isfinite(z.real()) || !std::isfinite(z.imag())) continue;
    C m = model_v2(a, b, c, d);
    if (same(m.real(), z.real()) && same(m.imag(), z.imag())) continue;
    pop.push_back({a, b, c, d, z});
  }
  std::printf("E10 population=%zu (from %ld random inputs)\n", pop.size(), tried);
  auto score = [&](auto f) {
    long ok = 0;
    for (auto & k : pop) {
      C w = f(k.a, k.b, k.c, k.d);
      ok += same(w.real(), k.z.real()) && same(w.imag(), k.z.imag());
    }
    return ok;
  };
  std::printf("split unfused: %ld\n", score([](double a, double b, double c, double d) { return split(a, b, c, d, false); }));
  std::printf("split fused:   %ld\n", score([](double a, double b, double c, double d) { return split(a, b, c, d, true); }));
  for (int t : {-1022, -969, -900, -800, -600, -511, -300, -200, -100, -52, -26, 0, 52, 100})
    for (int s : {52, 53, 54, 64, 104, 105, 106, 512, 1022}) {
      long ok = score([&](double a, double b, double c, double d) { return scaled_num(a, b, c, d, t, s); });
      if (ok > long(pop.size()) / 4) std::printf("scale num by 2^%d if AB < 2^%d: %ld\n", s, t, ok);
    }
  for (int i = 0; i < 12 && i < (int)pop.size(); ++i) {
    print4("CASE", pop[i].a, pop[i].b, pop[i].c, pop[i].d);
    std::printf(" | oracle %a %a\n", pop[i].z.real(), pop[i].z.imag());
  }
}
