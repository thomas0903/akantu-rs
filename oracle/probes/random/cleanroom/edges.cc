// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Clean-room black-box probe (D-030) for the random module
// ported from Akantu (EPFL/LSMS) src/common/aka_random_generator.hh: drives the
// oracle's std::generate_canonical<double, 53> and std::*_distribution<double>
// with a *scripted* uniform random bit generator (same range as minstd_rand0)
// to reach boundary cases that seeded engines practically never produce
// (canonical clamp, polar-method rejections, Cauchy u == 0.5, gamma u == 0,
// signed zeros at u == 0), plus reset() semantics and operator<< of special
// parameter values. Uses only the public <random> API.
//
// Output: JSON on stdout, committed as
// crates/akantu-core/tests/fixtures/random/cleanroom_edges.json.
//
//   oracle/probes/random/cleanroom/run.sh   (see there for the exact command)

#include <bit>
#include <cfloat>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <gnu/libc-version.h>
#include <limits>
#include <random>
#include <sstream>
#include <string>
#include <vector>

namespace {

/// Returns the scripted values, then the outputs of minstd_rand0 seeded 1.
struct Scripted {
  using result_type = unsigned long;
  static constexpr result_type min() { return 1; }
  static constexpr result_type max() { return 2147483646; }
  std::vector<unsigned long> script;
  std::size_t pos = 0;
  std::minstd_rand0 fallback{1};
  unsigned long calls = 0;
  result_type operator()() {
    ++calls;
    if (pos < script.size()) {
      return script[pos++];
    }
    return fallback();
  }
};

std::string hex(double x) {
  char buf[32];
  std::snprintf(buf, sizeof(buf), "\"%016llx\"",
                static_cast<unsigned long long>(std::bit_cast<std::uint64_t>(x)));
  return buf;
}

std::string json_str(const std::string & s) {
  std::string out = "\"";
  for (char c : s) {
    if (c == '"' || c == '\\') {
      out += '\\';
    }
    out += c;
  }
  return out + "\"";
}

struct CanonicalDist {
  template <class G> double operator()(G & g) { return std::generate_canonical<double, 53>(g); }
  void reset() {}
  friend std::ostream & operator<<(std::ostream & os, const CanonicalDist &) { return os; }
};

bool first_case = true;

double canonical_of(unsigned long g0, unsigned long g1) {
  Scripted s;
  s.script = {g0, g1};
  s.fallback.seed(1);
  return std::generate_canonical<double, 53>(s);
}

/// Digits whose canonical value is exactly u (u in [0.5, 1)), or false
/// (checked with the oracle's own generate_canonical).
bool digits_for(double u, unsigned long & g0, unsigned long & g1) {
  constexpr unsigned long R = 2147483646;
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
    if (canonical_of(1 + l, 1 + h) == u) {
      g0 = 1 + l;
      g1 = 1 + h;
      return true;
    }
  }
  return false;
}

/// Scripts for gamma(alpha, 1) whose first draw has log(u) exactly equal to
/// the Marsaglia-Tsang threshold (see exp_gamma_boundary.cc), and the same
/// with the next larger u (log u > threshold).
std::vector<std::vector<unsigned long>> gamma_boundary_scripts(double alpha, int count) {
  std::vector<std::vector<unsigned long>> out;
  const double a1 = alpha - 1.0 / 3.0;
  const double a2 = 1.0 / std::sqrt(9.0 * a1);
  std::minstd_rand0 pick(777);
  for (int trial = 0; trial < 200000 && (int)out.size() < 2 * count; ++trial) {
    std::vector<unsigned long> pair = {pick(), pick(), pick(), pick()};
    Scripted s;
    s.script = pair;
    std::normal_distribution<double> nd(0.0, 1.0);
    double n = nd(s);
    double v = 1.0 + a2 * n;
    if (s.calls != 4 || v <= 0.0) {
      continue;
    }
    v = v * v * v;
    double t = 0.5 * n * n + a1 * (1.0 - v + std::log(v));
    double t1 = 0.5 * n * n + a1 * (std::log(v) - v + 1.0);
    double t2 = 0.5 * n * n + (a1 - a1 * v + a1 * std::log(v));
    if (t != t1 || t != t2 || !(t < 0.0) || t < -0.69) {
      continue;
    }
    double u = -1.0;
    for (double c = std::nextafter(std::exp(t), 0.0), k = 0; k < 3; ++k, c = std::nextafter(c, 2.0)) {
      if (std::log(c) == t) {
        u = c;
      }
    }
    double ua = std::nextafter(u, 2.0);
    unsigned long g0, g1, h0, h1;
    if (u < 0.5 || !(u > 1.0 - 0.0331 * (n * n) * (n * n)) || !(std::log(ua) > t) ||
        !digits_for(u, g0, g1) || !digits_for(ua, h0, h1)) {
      continue;
    }
    out.push_back({pair[0], pair[1], pair[2], pair[3], g0, g1});
    out.push_back({pair[0], pair[1], pair[2], pair[3], h0, h1});
  }
  return out;
}

/// ops: 'd' = draw (records value and cumulative engine calls),
///      'r' = reset(), 'p' = operator<< (records text), 'c' = continue with a
///      copy of the distribution (copy constructor).
template <class D>
void run(const char * name, std::vector<double> params, D d, std::vector<unsigned long> script,
         const std::string & ops, const char * note) {
  Scripted g;
  g.script = std::move(script);
  std::vector<double> values;
  std::vector<unsigned long> calls;
  std::vector<std::string> printed;
  for (char op : ops) {
    switch (op) {
    case 'd':
      values.push_back(d(g));
      calls.push_back(g.calls);
      break;
    case 'r':
      d.reset();
      break;
    case 'p': {
      std::ostringstream os;
      os << d;
      printed.push_back(os.str());
      break;
    }
    case 'c': {
      D copy(d);
      d = copy;
      break;
    }
    default:
      break;
    }
  }
  std::printf("%s{\"name\":%s,\"dist\":\"%s\",\"params\":[", first_case ? "" : ",\n",
              json_str(note).c_str(), name);
  first_case = false;
  for (std::size_t i = 0; i < params.size(); ++i) {
    std::printf("%s%s", i ? "," : "", hex(params[i]).c_str());
  }
  std::printf("],\"script\":[");
  for (std::size_t i = 0; i < g.script.size(); ++i) {
    std::printf("%s%lu", i ? "," : "", g.script[i]);
  }
  std::printf("],\"ops\":\"%s\",\"values\":[", ops.c_str());
  for (std::size_t i = 0; i < values.size(); ++i) {
    std::printf("%s%s", i ? "," : "", hex(values[i]).c_str());
  }
  std::printf("],\"calls\":[");
  for (std::size_t i = 0; i < calls.size(); ++i) {
    std::printf("%s%lu", i ? "," : "", calls[i]);
  }
  std::printf("],\"printed\":[");
  for (std::size_t i = 0; i < printed.size(); ++i) {
    std::printf("%s%s", i ? "," : "", json_str(printed[i]).c_str());
  }
  std::printf("]}");
}

} // namespace

int main() {
  constexpr unsigned long MIN = 1;
  constexpr unsigned long MAX = 2147483646;
  constexpr unsigned long HALF = 1073741824; // digits (MIN, HALF): u == 0.5 exactly
  // Digits giving u ~ 0.75 (x = 2u - 1 ~ 0.5) and u ~ 0.5005 / 0.45.
  constexpr unsigned long Q3 = 1610612735;
  constexpr unsigned long U5005 = 1074815565;
  constexpr unsigned long U45 = 966367642;
  const double nan = std::numeric_limits<double>::quiet_NaN();
  const double inf = std::numeric_limits<double>::infinity();

  std::printf("{\"meta\":{\"compiler\":%s,\"glibcxx\":%d,\"glibcxx_release\":%d,"
              "\"glibc\":%s,\"arch\":%s,\"flags\":%s},\n\"cases\":[\n",
              json_str(__VERSION__).c_str(), __GLIBCXX__, _GLIBCXX_RELEASE,
              json_str(gnu_get_libc_version()).c_str(),
#if defined(__aarch64__)
              "\"aarch64\"",
#else
              "\"other\"",
#endif
              json_str("-std=c++20 -O2 -ffp-contract=off").c_str());

  // generate_canonical boundaries.
  run("canonical", {}, CanonicalDist{}, {MAX, MAX}, "d", "canonical: largest digits clamp below 1");
  run("canonical", {}, CanonicalDist{}, {MAX, MAX - 1, MAX - 1, MAX}, "dd", "canonical: near-1 digits");
  run("canonical", {}, CanonicalDist{}, {MIN, MIN, MIN + 1, MIN, MIN, MIN + 1, MIN, HALF}, "dddd",
      "canonical: zero, 1/R^2, 1/R, 0.5");

  // u == 0 and the clamped u in the inversion formulas (signed zeros, infinities).
  run("uniform_real", {-0.0, 1.0}, std::uniform_real_distribution<double>(-0.0, 1.0), {MIN, MIN},
      "dp", "uniform: u = 0 with a = -0");
  run("uniform_real", {-2.5, 7.25}, std::uniform_real_distribution<double>(-2.5, 7.25),
      {MAX, MAX}, "d", "uniform: clamped u");
  run("exponential", {1.0}, std::exponential_distribution<double>(1.0), {MIN, MIN, MAX, MAX},
      "dd", "exponential: u = 0 and clamped u");
  run("weibull", {1.0, 1.0}, std::weibull_distribution<double>(1.0, 1.0), {MIN, MIN, MAX, MAX},
      "dd", "weibull: u = 0 (pow(-0, 1)) and clamped u");
  run("weibull", {0.5, 2.0}, std::weibull_distribution<double>(0.5, 2.0), {MIN, MIN, MAX, MAX},
      "dd", "weibull: u = 0 (pow(-0, 2)) and clamped u");
  run("extreme_value", {0.0, 1.0}, std::extreme_value_distribution<double>(0.0, 1.0),
      {MIN, MIN, MAX, MAX}, "dd", "extreme_value: u = 0 and clamped u");
  run("cauchy", {0.0, 1.0}, std::cauchy_distribution<double>(0.0, 1.0), {MIN, MIN, MAX, MAX},
      "dd", "cauchy: u = 0 and clamped u");

  // Cauchy: u == 0.5 is rejected (extra engine calls).
  run("cauchy", {2.0, 0.5}, std::cauchy_distribution<double>(2.0, 0.5), {MIN, HALF, MIN, HALF, 7, Q3},
      "dd", "cauchy: u = 0.5 twice, then 0.75");

  // Normal (polar method): r2 == 1 (x = -1, y = 0) and r2 == 0 (x = y = 0).
  run("normal", {0.0, 1.0}, std::normal_distribution<double>(0.0, 1.0),
      {MIN, MIN, MIN, HALF, 5, Q3, 9, Q3}, "dpdp", "normal: r2 == 1 (accepted), then a pair (0.5, 0.5)");
  run("normal", {0.0, 1.0}, std::normal_distribution<double>(0.0, 1.0),
      {MIN, HALF, MIN, HALF, 5, Q3, 9, Q3}, "dpdp", "normal: r2 == 0 (rejected), then a pair (0.5, 0.5)");
  run("normal", {1.5, 2.0}, std::normal_distribution<double>(1.5, 2.0),
      {MIN, MIN, MIN, MIN, MAX, MAX, MAX, MAX, 5, Q3, 3, U45}, "dpdp",
      "normal: r2 == 2 (x = y = -1), r2 ~ 2 (clamped u), accepted");

  // r2 == 1 with x = -1, y = 0 gives the standard value y * mult = 0 * (-0) = -0:
  // the sign of a zero result shows whether nested normals return z * 1 + 0.
  run("student_t", {3.0}, std::student_t_distribution<double>(3.0), {MIN, MIN, MIN, HALF}, "dp",
      "student_t: standard normal value -0 (r2 == 1)");
  run("normal", {-0.0, 1.0}, std::normal_distribution<double>(-0.0, 1.0), {MIN, MIN, MIN, HALF},
      "dpdp", "normal(-0, 1): standard values -0 then +0 (r2 == 1)");

  // Gamma alpha < 1: extra uniform u == 0 is rejected.
  run("gamma", {0.5, 3.0}, std::gamma_distribution<double>(0.5, 3.0),
      {MIN, Q3, MIN, Q3, MIN, MIN + 1, MIN, MIN, MIN, HALF}, "dp",
      "gamma(0.5): accepted pair, tiny u, then extra u = 0 (rejected) and 0.5");
  // Gamma alpha >= 1: v <= 0 (very negative normal value) redraws the normal only.
  run("gamma", {1.0, 1.0}, std::gamma_distribution<double>(1.0, 1.0),
      {MIN, U5005, MIN, U45, MIN, MIN + 1, MIN, MIN + 1}, "dpdp",
      "gamma(1): n ~ -3 (v <= 0), cached n, tiny u");

  // Gamma: acceptance test exactly at log(u) == threshold, and just above.
  {
    auto scripts = gamma_boundary_scripts(2.5, 4);
    for (std::size_t i = 0; i < scripts.size(); ++i) {
      run("gamma", {2.5, 1.0}, std::gamma_distribution<double>(2.5, 1.0), scripts[i], "dp",
          i % 2 == 0 ? "gamma: log u == threshold" : "gamma: log u just above threshold");
    }
  }

  // reset(): drops cached normal values (also of nested distributions).
  const std::string rops = "dpdprpdpcdp";
  run("uniform_real", {0.0, 1.0}, std::uniform_real_distribution<double>(0.0, 1.0), {}, rops, "reset");
  run("exponential", {2.0}, std::exponential_distribution<double>(2.0), {}, rops, "reset");
  run("normal", {10.0, 0.5}, std::normal_distribution<double>(10.0, 0.5), {}, rops, "reset");
  run("lognormal", {1.5, 0.25}, std::lognormal_distribution<double>(1.5, 0.25), {}, rops, "reset");
  run("gamma", {2.5, 1.5}, std::gamma_distribution<double>(2.5, 1.5), {}, rops, "reset");
  run("gamma", {0.3, 1.5}, std::gamma_distribution<double>(0.3, 1.5), {}, rops, "reset");
  run("chi_squared", {3.0}, std::chi_squared_distribution<double>(3.0), {}, rops, "reset");
  run("cauchy", {0.0, 1.0}, std::cauchy_distribution<double>(0.0, 1.0), {}, rops, "reset");
  run("fisher_f", {4.0, 6.0}, std::fisher_f_distribution<double>(4.0, 6.0), {}, rops, "reset");
  run("student_t", {2.5}, std::student_t_distribution<double>(2.5), {}, rops, "reset");
  run("weibull", {2.0, 3.5}, std::weibull_distribution<double>(2.0, 3.5), {}, rops, "reset");
  run("extreme_value", {-1.5, 2.5}, std::extreme_value_distribution<double>(-1.5, 2.5), {}, rops,
      "reset");

  // Gamma alpha < 1 with a non-power-of-two beta (operation order of the
  // final product), and shapes/scales whose products round.
  run("gamma", {0.3, 1.7}, std::gamma_distribution<double>(0.3, 1.7), {}, std::string(200, 'd'),
      "gamma: alpha < 1, beta = 1.7");
  run("gamma", {0.9, 0.1}, std::gamma_distribution<double>(0.9, 0.1), {}, std::string(200, 'd'),
      "gamma: alpha < 1, beta = 0.1");
  run("gamma", {3.3, 7.1}, std::gamma_distribution<double>(3.3, 7.1), {}, std::string(200, 'd'),
      "gamma: alpha > 1, beta = 7.1");
  run("chi_squared", {0.7}, std::chi_squared_distribution<double>(0.7), {}, std::string(200, 'd'),
      "chi_squared: n < 2");
  run("fisher_f", {0.6, 3.3}, std::fisher_f_distribution<double>(0.6, 3.3), {},
      std::string(200, 'd'), "fisher_f: m < 2");
  run("student_t", {0.8}, std::student_t_distribution<double>(0.8), {}, std::string(200, 'd'),
      "student_t: n < 2");
  run("lognormal", {-0.3, 3.7}, std::lognormal_distribution<double>(-0.3, 3.7), {},
      std::string(200, 'd'), "lognormal: s = 3.7");
  run("normal", {-7.0, 0.3}, std::normal_distribution<double>(-7.0, 0.3), {}, std::string(201, 'd') + "p",
      "normal: odd count, printed cache");

  // Parameters outside the standard's preconditions (sampled silently by the
  // oracle, which is built without _GLIBCXX_ASSERTIONS): NaN-sensitive loop
  // conditions and branch tests (see exp_invalid.cc).
  {
    const std::string ops = "dddd";
    run("gamma", {nan, 1.0}, std::gamma_distribution<double>(nan, 1.0), {}, ops, "invalid");
    run("gamma", {-1.0, 1.0}, std::gamma_distribution<double>(-1.0, 1.0), {}, ops, "invalid");
    run("gamma", {-0.5, 1.0}, std::gamma_distribution<double>(-0.5, 1.0), {}, ops, "invalid");
    run("gamma", {0.0, 1.0}, std::gamma_distribution<double>(0.0, 1.0), {}, ops, "invalid");
    run("gamma", {-0.0, 1.0}, std::gamma_distribution<double>(-0.0, 1.0), {}, ops, "invalid");
    run("gamma", {inf, 1.0}, std::gamma_distribution<double>(inf, 1.0), {}, ops, "invalid");
    run("gamma", {-inf, 1.0}, std::gamma_distribution<double>(-inf, 1.0), {}, ops, "invalid");
    run("gamma", {-1e17, 1.0}, std::gamma_distribution<double>(-1e17, 1.0), {}, ops, "invalid");
    run("gamma", {-9007199254740992.0, 1.0},
        std::gamma_distribution<double>(-9007199254740992.0, 1.0), {}, ops, "invalid");
    run("gamma", {0.5, -2.0}, std::gamma_distribution<double>(0.5, -2.0), {}, ops, "invalid");
    run("chi_squared", {-3.0}, std::chi_squared_distribution<double>(-3.0), {}, ops, "invalid");
    run("student_t", {nan}, std::student_t_distribution<double>(nan), {}, ops, "invalid");
    run("fisher_f", {3.0, -7.0}, std::fisher_f_distribution<double>(3.0, -7.0), {}, ops,
        "invalid");
    run("normal", {0.0, -1.0}, std::normal_distribution<double>(0.0, -1.0), {}, ops, "invalid");
    run("lognormal", {nan, 1.0}, std::lognormal_distribution<double>(nan, 1.0), {}, ops,
        "invalid");
    run("exponential", {0.0}, std::exponential_distribution<double>(0.0), {}, ops, "invalid");
    run("weibull", {0.0, 1.0}, std::weibull_distribution<double>(0.0, 1.0), {}, ops, "invalid");
    run("extreme_value", {0.0, nan}, std::extreme_value_distribution<double>(0.0, nan), {}, ops,
        "invalid");
    run("cauchy", {nan, 1.0}, std::cauchy_distribution<double>(nan, 1.0), {}, ops, "invalid");
    run("uniform_real", {1.0, 0.0}, std::uniform_real_distribution<double>(1.0, 0.0), {}, ops,
        "invalid");
    run("uniform_real", {-inf, inf}, std::uniform_real_distribution<double>(-inf, inf), {}, ops,
        "invalid");
  }

  // operator<< of special parameter values.
  run("uniform_real", {-nan, inf}, std::uniform_real_distribution<double>(-nan, inf), {}, "p",
      "print: -nan, inf");
  run("normal", {nan, -inf}, std::normal_distribution<double>(nan, -inf), {}, "p",
      "print: nan, -inf");
  run("exponential", {DBL_TRUE_MIN}, std::exponential_distribution<double>(DBL_TRUE_MIN), {}, "p",
      "print: smallest subnormal");
  std::printf("\n]}\n");
  return 0;
}
