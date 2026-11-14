// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E3 (black-box). The ratio r = d/c (or c/d)
// underflows to zero while all operands stay far from overflow and from the
// subnormal range. Smith (1962) then gives x = a/c, y = b/c; Baudin & Smith
// (2012, robust_internal) instead keep the d·(b/c) term. Candidates for the
// r == 0 numerators (fused or not, operand grouping) are compared with the
// oracle.
#include "smith_core.h"

static double mad(bool f, double x, double y, double z) { return f ? std::fma(x, y, z) : x * y + z; }

// variant v: 0 = Smith (r used as is); 1 = BS grouping d*(b/c) unfused;
// 2 = BS grouping fused; 3 = (d*b)/c grouping unfused; 4 = (d*b)/c fused
static C model(double a, double b, double c, double d, int v) {
  bool dbr = std::fabs(c) < std::fabs(d);
  if (dbr) {  // swap roles: (a,b,c,d) -> (b,a,d,c) with y negated by construction
    double r = c / d;
    double den = std::fma(c, r, d);
    if (r != 0 || v == 0) return C(std::fma(a, r, b) / den, std::fma(b, r, -a) / den);
    double x, y;
    switch (v) {
      case 1: x = mad(false, c, a / d, b); y = mad(false, c, b / d, -a); break;
      case 2: x = mad(true, c, a / d, b); y = mad(true, c, b / d, -a); break;
      case 3: x = (c * a) / d + b; y = (c * b) / d - a; break;
      default: x = std::fma(c / d, a, b); y = std::fma(c / d, b, -a); break;
    }
    return C(x / den, y / den);
  }
  double r = d / c;
  double den = std::fma(d, r, c);
  if (r != 0 || v == 0) return C(std::fma(b, r, a) / den, std::fma(-a, r, b) / den);
  double x, y;
  switch (v) {
    case 1: x = mad(false, d, b / c, a); y = mad(false, -d, a / c, b); break;
    case 2: x = mad(true, d, b / c, a); y = mad(true, -d, a / c, b); break;
    case 3: x = (d * b) / c + a; y = b - (d * a) / c; break;
    default: x = std::fma(d / c, b, a); y = std::fma(-(d / c), a, b); break;
  }
  return C(x / den, y / den);
}

int main() {
  Rng r{0xc0ffee03ULL};
  const int NV = 5;
  long mism[NV] = {0}, total = 0;
  int shown = 0;
  for (long k = 0; k < 1000000; ++k) {
    // big part in [2^300, 2^600], small part in [2^-700, 2^-500]: |ratio| <
    // 2^-800... underflows; b/c, a/d etc. stay normal.
    double big = r.expo(300, 600), small = r.expo(-700, -500);
    double c, d;
    if (k & 1) { c = big; d = small; } else { c = small; d = big; }
    // numerators: one of them comparable to big, the other to small*(big/big)
    double p = r.expo(200, 900), q = r.expo(-900, 300);
    if (k & 2) std::swap(p, q);
    double a = (k & 1) ? q : p, b = (k & 1) ? p : q;
    if ((k & 12) == 4) a = 0.0;
    if ((k & 12) == 8) b = 0.0;
    C z = oracle_div(C(a, b), C(c, d));
    ++total;
    for (int v = 0; v < NV; ++v) {
      C w = model(a, b, c, d, v);
      bool ok = same(w.real(), z.real()) && same(w.imag(), z.imag());
      if (!ok) ++mism[v];
      if (!ok && v == 2 && shown < 8) {
        ++shown;
        print4("MIS(v2)", a, b, c, d);
        std::printf(" | oracle %a %a | model %a %a\n", z.real(), z.imag(), w.real(), w.imag());
      }
    }
  }
  std::printf("E3 total=%ld\n", total);
  const char * names[NV] = {"smith(r as is)", "d*(b/c) unfused", "d*(b/c) fused", "(d*b)/c unfused",
                            "fma(d/c,b,a)"};
  for (int v = 0; v < NV; ++v) std::printf("variant %d %-18s mismatches=%ld\n", v, names[v], mism[v]);
}
