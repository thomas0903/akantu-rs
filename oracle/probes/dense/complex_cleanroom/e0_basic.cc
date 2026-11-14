// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E0 (black-box): print std::complex<double>
// division results, bit for bit, for operands read from argv (hex bit
// patterns or decimal). No library code is inspected; only outputs.
#include <complex>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>

using C = std::complex<double>;

__attribute__((noinline)) C oracle_div(C x, C y) { return x / y; }

static double parse(const char * s) {
  if (std::strncmp(s, "0x", 2) == 0 && std::strchr(s, 'p') == nullptr) {
    std::uint64_t u = std::strtoull(s + 2, nullptr, 16);
    double v;
    std::memcpy(&v, &u, 8);
    return v;
  }
  return std::strtod(s, nullptr);
}
static std::uint64_t bits(double v) {
  std::uint64_t u;
  std::memcpy(&u, &v, 8);
  return u;
}

int main(int argc, char ** argv) {
  for (int i = 1; i + 3 < argc; i += 4) {
    double a = parse(argv[i]), b = parse(argv[i + 1]), c = parse(argv[i + 2]), d = parse(argv[i + 3]);
    C z = oracle_div(C(a, b), C(c, d));
    std::printf("(%a, %a) / (%a, %a) = (%a, %a)  bits %016llx %016llx\n", a, b, c, d, z.real(), z.imag(),
                (unsigned long long)bits(z.real()), (unsigned long long)bits(z.imag()));
  }
}
