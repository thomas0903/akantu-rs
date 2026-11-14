// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Clean-room black-box experiment (D-030): observes the
// behaviour of the oracle's std::generate_canonical<double, 53> through
// scripted uniform random bit generators (call counts, digit order, rounding,
// clamping) and compares candidate formulas written from the C++ standard
// ([rand.util.canonical]) against it. Uses only the public <random> API.
//
//   oracle/probes/random/cleanroom/run.sh   (see there for the exact command)

#include <bit>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <deque>
#include <random>

static std::deque<unsigned long> script;
static unsigned long calls = 0;

template <unsigned long MIN, unsigned long MAX> struct Scripted {
  using result_type = unsigned long;
  static constexpr result_type min() { return MIN; }
  static constexpr result_type max() { return MAX; }
  result_type operator()() {
    ++calls;
    if (script.empty()) {
      return MIN;
    }
    auto v = script.front();
    script.pop_front();
    return v;
  }
};

static unsigned long long bits(double x) { return std::bit_cast<std::uint64_t>(x); }

// Candidate A: S and R^k accumulated in double, digit 0 first (standard text).
static double cand_a(unsigned long g0, unsigned long g1) {
  const double r = 2147483646.0;
  double sum = 0.0;
  double t = 1.0;
  sum += double(g0 - 1) * t;
  t *= r;
  sum += double(g1 - 1) * t;
  t *= r;
  return sum / t;
}
// Candidate B: same in long double (binary128 on aarch64).
static double cand_b(unsigned long g0, unsigned long g1) {
  const long double r = 2147483646.0L;
  long double sum = 0.0L;
  long double t = 1.0L;
  sum += (long double)(g0 - 1) * t;
  t *= r;
  sum += (long double)(g1 - 1) * t;
  t *= r;
  return double(sum / t);
}
// Candidate C: exact integer S, one conversion, divided by double(R^2).
static double cand_c(unsigned long g0, unsigned long g1) {
  const unsigned long r = 2147483646UL;
  unsigned long s = (g0 - 1) + (g1 - 1) * r;
  return double(s) / double(r * r);
}
// Candidate D: digit order reversed (g0 most significant), double.
static double cand_d(unsigned long g0, unsigned long g1) { return cand_a(g1, g0); }

int main() {
  using M = Scripted<1, 2147483646>;
  M g;
  // 1. Number of calls per draw.
  calls = 0;
  (void)std::generate_canonical<double, 53>(g);
  std::printf("calls per draw (minstd range): %lu\n", calls);

  // 2. Digit order: (g0, g1) = (min+1, min) vs (min, min+1).
  script = {2, 1};
  double a = std::generate_canonical<double, 53>(g);
  script = {1, 2};
  double b = std::generate_canonical<double, 53>(g);
  std::printf("order: (min+1,min) -> %a ; (min,min+1) -> %a ; 1/R = %a ; 1/R^2 = %a\n", a,
              b, 1.0 / 2147483646.0, 1.0 / (2147483646.0 * 2147483646.0));

  // 3. Largest digits: (max, max) -> S/R^2 rounds to 1.0 in double.
  script = {2147483646, 2147483646};
  double c = std::generate_canonical<double, 53>(g);
  std::printf("(max,max) -> %a (%016llx); cand_a = %a\n", c, bits(c),
              cand_a(2147483646, 2147483646));
  script = {2147483646, 2147483645};
  double c2 = std::generate_canonical<double, 53>(g);
  std::printf("(max,max-1) -> %a; cand_a = %a\n", c2, cand_a(2147483646, 2147483645));
  script = {1, 1};
  double z = std::generate_canonical<double, 53>(g);
  std::printf("(min,min) -> %a (%016llx)\n", z, bits(z));

  // 4. Rounding: compare candidates on engine-produced digit pairs.
  std::minstd_rand0 e(5489);
  long mism[4] = {0, 0, 0, 0};
  long clamp_cases = 0;
  const long n = 20000000;
  for (long i = 0; i < n; ++i) {
    unsigned long g0 = e();
    unsigned long g1 = e();
    script = {g0, g1};
    double v = std::generate_canonical<double, 53>(g);
    double ca = cand_a(g0, g1), cb = cand_b(g0, g1), cc = cand_c(g0, g1), cd = cand_d(g0, g1);
    if (ca >= 1.0) {
      ++clamp_cases;
    }
    mism[0] += bits(v) != bits(ca);
    mism[1] += bits(v) != bits(cb);
    mism[2] += bits(v) != bits(cc);
    mism[3] += bits(v) != bits(cd);
  }
  std::printf("%ld pairs: mismatches A(double, low first)=%ld B(long double)=%ld "
              "C(exact S)=%ld D(high first)=%ld; A>=1 cases=%ld\n",
              n, mism[0], mism[1], mism[2], mism[3], clamp_cases);

  // 5. Number of calls for other ranges (k rule).
#define CALLS(LO, HI)                                                          \
  do {                                                                         \
    Scripted<LO, HI> h;                                                        \
    calls = 0;                                                                 \
    (void)std::generate_canonical<double, 53>(h);                              \
    std::printf("range [%lu, %lu] (R = %.6g, log2 R = %.6f): %lu calls\n",     \
                (unsigned long)(LO), (unsigned long)(HI),                      \
                double(HI) - double(LO) + 1.0,                                 \
                std::log2(double(HI) - double(LO) + 1.0), calls);              \
  } while (0)
  CALLS(0UL, 1UL);
  CALLS(0UL, 255UL);
  CALLS(0UL, (1UL << 26) - 1);
  CALLS(0UL, 110000000UL - 1);   // log2 R ~ 26.71
  CALLS(0UL, (1UL << 27) - 2);   // log2 R just below 27
  CALLS(0UL, (1UL << 27) - 1);
  CALLS(0UL, (1UL << 31) - 1);
  CALLS(0UL, (1UL << 32) - 1);
  CALLS(0UL, (1UL << 53) - 1);
  CALLS(0UL, ~0UL);
  CALLS(5UL, 5UL + 999UL);
  return 0;
}
