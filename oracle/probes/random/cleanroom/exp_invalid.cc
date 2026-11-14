// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Clean-room black-box experiment (D-030): behaviour of the
// oracle's distributions (built without _GLIBCXX_ASSERTIONS, like Akantu) for
// parameters outside the standard's preconditions -- NaN-sensitive loop
// conditions and branch tests become observable (termination, engine calls,
// NaN vs number). Each case runs in a forked child with a 2 s alarm.
//
//   oracle/probes/random/cleanroom/run.sh   (see there for the exact command)
#include <bit>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <limits>
#include <random>
#include <sys/wait.h>
#include <unistd.h>

struct Counting {
  using result_type = std::minstd_rand0::result_type;
  static constexpr result_type min() { return std::minstd_rand0::min(); }
  static constexpr result_type max() { return std::minstd_rand0::max(); }
  std::minstd_rand0 e{5489};
  unsigned long calls = 0;
  result_type operator()() { ++calls; return e(); }
};

template <class D> void probe(const char * label, D d) {
  std::fflush(stdout);
  pid_t pid = fork();
  if (pid == 0) {
    alarm(2);
    Counting g;
    std::printf("%-26s", label);
    for (int i = 0; i < 4; ++i) {
      double x = d(g);
      std::printf(" %016llx/%lu", (unsigned long long)std::bit_cast<std::uint64_t>(x), g.calls);
    }
    std::printf("\n");
    std::fflush(stdout);
    _exit(0);
  }
  int st = 0;
  waitpid(pid, &st, 0);
  if (WIFSIGNALED(st)) {
    std::printf("%-26s HANG/KILLED (signal %d)\n", label, WTERMSIG(st));
  }
}

int main() {
  const double nan = std::numeric_limits<double>::quiet_NaN();
  const double inf = std::numeric_limits<double>::infinity();
  probe("gamma(nan, 1)", std::gamma_distribution<double>(nan, 1.0));
  probe("gamma(-nan, 1)", std::gamma_distribution<double>(-nan, 1.0));
  probe("gamma(-1, 1)", std::gamma_distribution<double>(-1.0, 1.0));
  probe("gamma(-5, 1)", std::gamma_distribution<double>(-5.0, 1.0));
  probe("gamma(-0.5, 1)", std::gamma_distribution<double>(-0.5, 1.0));
  probe("gamma(0, 1)", std::gamma_distribution<double>(0.0, 1.0));
  probe("gamma(-0, 1)", std::gamma_distribution<double>(-0.0, 1.0));
  probe("gamma(inf, 1)", std::gamma_distribution<double>(inf, 1.0));
  // alpha + 1 == alpha: distinguishes "alpha == malpha" from "alpha >= 1".
  probe("gamma(-1e17, 1)", std::gamma_distribution<double>(-1e17, 1.0));
  probe("gamma(-inf, 1)", std::gamma_distribution<double>(-inf, 1.0));
  probe("gamma(-2^53, 1)", std::gamma_distribution<double>(-9007199254740992.0, 1.0));
  probe("gamma(-2^52, 1)", std::gamma_distribution<double>(-4503599627370496.0, 1.0));
  probe("gamma(2, nan)", std::gamma_distribution<double>(2.0, nan));
  probe("gamma(0.5, -2)", std::gamma_distribution<double>(0.5, -2.0));
  probe("chi_squared(nan)", std::chi_squared_distribution<double>(nan));
  probe("chi_squared(-3)", std::chi_squared_distribution<double>(-3.0));
  probe("student_t(nan)", std::student_t_distribution<double>(nan));
  probe("student_t(-4)", std::student_t_distribution<double>(-4.0));
  probe("fisher_f(nan, 1)", std::fisher_f_distribution<double>(nan, 1.0));
  probe("fisher_f(3, -7)", std::fisher_f_distribution<double>(3.0, -7.0));
  probe("normal(0, nan)", std::normal_distribution<double>(0.0, nan));
  probe("normal(0, -1)", std::normal_distribution<double>(0.0, -1.0));
  probe("lognormal(nan, 1)", std::lognormal_distribution<double>(nan, 1.0));
  probe("exponential(0)", std::exponential_distribution<double>(0.0));
  probe("exponential(nan)", std::exponential_distribution<double>(nan));
  probe("weibull(nan, 1)", std::weibull_distribution<double>(nan, 1.0));
  probe("weibull(0, 1)", std::weibull_distribution<double>(0.0, 1.0));
  probe("extreme_value(0, nan)", std::extreme_value_distribution<double>(0.0, nan));
  probe("cauchy(nan, 1)", std::cauchy_distribution<double>(nan, 1.0));
  probe("uniform(1, 0)", std::uniform_real_distribution<double>(1.0, 0.0));
  probe("uniform(nan, 1)", std::uniform_real_distribution<double>(nan, 1.0));
  probe("uniform(-inf, inf)", std::uniform_real_distribution<double>(-inf, inf));
  return 0;
}
