// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Clean-room black-box experiment (D-030): is the
// Marsaglia-Tsang acceptance test of the oracle's std::gamma_distribution
// strict (log u < t) or not (log u <= t) when log(u) equals the threshold
// t = n^2/2 + d (1 - v + log v) exactly? Uses a scripted generator: a polar
// pair giving the normal value n (obtained from the oracle's own
// std::normal_distribution on the same digits), then digits giving exactly a
// uniform u with log(u) == t (found by search; u is checked with the oracle's
// own std::generate_canonical). Accepted => 6 engine calls, rejected => more.
// The squeeze test u < 1 - 0.0331 n^4 is made to fail (u above it).
//
//   oracle/probes/random/cleanroom/run.sh   (see there for the exact command)

#include <cmath>
#include <cstdio>
#include <random>
#include <vector>

namespace {
constexpr unsigned long MIN = 1;
constexpr unsigned long R = 2147483646;

struct Scripted {
  using result_type = unsigned long;
  static constexpr result_type min() { return MIN; }
  static constexpr result_type max() { return R; }
  std::vector<unsigned long> script;
  std::size_t pos = 0;
  std::minstd_rand0 fallback{1};
  unsigned long calls = 0;
  result_type operator()() {
    ++calls;
    return pos < script.size() ? script[pos++] : fallback();
  }
};

double canonical_of(unsigned long g0, unsigned long g1) {
  Scripted s;
  s.script = {g0, g1};
  return std::generate_canonical<double, 53>(s);
}

/// Digits whose canonical value is exactly u (u in [0.5, 1)), or false.
bool digits_for(double u, unsigned long & g0, unsigned long & g1) {
  long double s = (long double)u * (long double)R * (long double)R;
  unsigned long hi = (unsigned long)(s / R);
  long lo = (long)(s - (long double)hi * R);
  for (long d = -2000; d <= 2000; ++d) {
    long l = lo + d;
    unsigned long h = hi;
    if (l < 0) {
      l += R;
      h -= 1;
    }
    if (l >= (long)R) {
      l -= R;
      h += 1;
    }
    if (canonical_of(MIN + l, MIN + h) == u) {
      g0 = MIN + l;
      g1 = MIN + h;
      return true;
    }
  }
  return false;
}
} // namespace

int main() {
  const double alpha = 2.5;
  const double a1 = alpha - 1.0 / 3.0;
  const double a2 = 1.0 / std::sqrt(9.0 * a1);
  std::minstd_rand0 pick(777);
  int found = 0, accepted_eq = 0, rejected_eq = 0, control_ok = 0;
  for (int trial = 0; trial < 200000 && found < 12; ++trial) {
    std::vector<unsigned long> pair = {pick(), pick(), pick(), pick()};
    Scripted s;
    s.script = pair;
    std::normal_distribution<double> nd(0.0, 1.0);
    double n = nd(s);
    if (s.calls != 4) {
      continue; // pair rejected by the polar method
    }
    double v = 1.0 + a2 * n;
    if (v <= 0.0) {
      continue;
    }
    v = v * v * v;
    // All plausible groupings of the threshold must agree.
    double t0 = 0.5 * n * n + a1 * (1.0 - v + std::log(v));
    double t1 = 0.5 * n * n + a1 * (std::log(v) - v + 1.0);
    double t2 = 0.5 * n * n + (a1 - a1 * v + a1 * std::log(v));
    double t3 = 0.5 * (n * n) + a1 * (1.0 - v) + a1 * std::log(v);
    if (!(t0 == t1 && t0 == t2 && t0 == t3) || !(t0 < 0.0) || t0 < -0.69) {
      continue;
    }
    double sq = 1.0 - 0.0331 * (n * n) * (n * n);
    double u0 = std::exp(t0);
    double u = -1.0;
    for (int k = -8; k <= 8; ++k) {
      double c = u0;
      for (int j = 0; j < (k < 0 ? -k : k); ++j) {
        c = std::nextafter(c, k < 0 ? 0.0 : 2.0);
      }
      if (std::log(c) == t0) {
        u = c;
        break;
      }
    }
    if (u < 0.5 || u >= 1.0 || !(u > sq)) {
      continue;
    }
    unsigned long g0, g1;
    if (!digits_for(u, g0, g1)) {
      continue;
    }
    ++found;
    auto calls_for = [&](unsigned long d0, unsigned long d1) {
      Scripted g;
      g.script = {pair[0], pair[1], pair[2], pair[3], d0, d1};
      std::gamma_distribution<double> gd(alpha, 1.0);
      (void)gd(g);
      return g.calls;
    };
    unsigned long eq = calls_for(g0, g1);
    // Controls: the next smaller u (log u < t) must be accepted.
    double ub = std::nextafter(u, 0.0);
    unsigned long b0, b1;
    unsigned long below = digits_for(ub, b0, b1) ? calls_for(b0, b1) : 0;
    double ua = std::nextafter(u, 2.0);
    unsigned long a0, a1d;
    unsigned long above = (digits_for(ua, a0, a1d) && std::log(ua) > t0) ? calls_for(a0, a1d) : 0;
    std::printf("n=%a t=%a u=%a: calls at log u == t: %lu; next below: %lu; next above "
                "(log u > t): %lu\n",
                n, t0, u, eq, below, above);
    (eq == 6 ? accepted_eq : rejected_eq) += 1;
    control_ok += (below == 6) && (above == 0 || above > 6);
  }
  std::printf("equality cases: %d accepted, %d rejected; controls ok: %d/%d\n", accepted_eq,
              rejected_eq, control_ok, found);
  return 0;
}
