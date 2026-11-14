// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Clean-room black-box experiment (D-030): which parameter
// values make the oracle's std::*_distribution<double> constructors fail their
// debug assertions when compiled with -D_GLIBCXX_ASSERTIONS (each construction
// runs in a forked child; an abort means "precondition asserted").
//
//   oracle/probes/random/cleanroom/run.sh   (see there for the exact command)

#include <cmath>
#include <cstdio>
#include <functional>
#include <limits>
#include <random>
#include <string>
#include <sys/wait.h>
#include <unistd.h>
#include <vector>

static bool aborts(const std::function<void()> & f) {
  std::fflush(stdout);
  pid_t pid = fork();
  if (pid == 0) {
    // Silence the assertion message.
    std::freopen("/dev/null", "w", stderr);
    f();
    _exit(0);
  }
  int status = 0;
  waitpid(pid, &status, 0);
  return !(WIFEXITED(status) && WEXITSTATUS(status) == 0);
}

int main() {
  const double nan = std::numeric_limits<double>::quiet_NaN();
  const double inf = std::numeric_limits<double>::infinity();
  const double big = std::numeric_limits<double>::max();
  const double tiny = std::numeric_limits<double>::denorm_min();
  const std::vector<double> vals = {-inf, -1.0, -tiny, -0.0, 0.0, tiny, 1.0, 2.0, big, inf, nan};
  auto name = [](double x) {
    char b[32];
    std::snprintf(b, sizeof b, "%g", x);
    return std::string(b);
  };
  std::mt19937 dummy;
#define ONE(D)                                                                 \
  for (double p : vals) {                                                      \
    bool a = aborts([&] { std::D<double> d(p); (void)d; });                    \
    std::printf("%-28s (%s): %s\n", #D, name(p).c_str(), a ? "ASSERT" : "ok"); \
  }
#define TWO(D)                                                                 \
  for (double p : vals)                                                        \
    for (double q : vals) {                                                    \
      bool a = aborts([&] { std::D<double> d(p, q); (void)d; });               \
      std::printf("%-28s (%s, %s): %s\n", #D, name(p).c_str(),                 \
                  name(q).c_str(), a ? "ASSERT" : "ok");                       \
    }
  TWO(uniform_real_distribution)
  ONE(exponential_distribution)
  TWO(normal_distribution)
  TWO(lognormal_distribution)
  TWO(gamma_distribution)
  TWO(weibull_distribution)
  TWO(extreme_value_distribution)
  ONE(chi_squared_distribution)
  TWO(cauchy_distribution)
  TWO(fisher_f_distribution)
  ONE(student_t_distribution)
  // Halving underflow of the nested gamma shape (n/2 == 0 for n = tiny).
  std::printf("chi_squared(tiny) n/2 = %g\n", tiny / 2);
  (void)dummy;
  return 0;
}
