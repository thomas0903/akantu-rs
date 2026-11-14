// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room probe E8 (black-box). For given operands print the
// oracle result, model_v2, the exact quotient rounded from binary128
// (long double on aarch64 Linux), and the intermediates of model_v2.
#include <cstdlib>
#include "model_v2.h"

static double parse(const char * s) {
  if (std::strncmp(s, "0x", 2) == 0 && std::strchr(s, 'p') == nullptr) return from_bits(std::strtoull(s + 2, nullptr, 16));
  return std::strtod(s, nullptr);
}

int main(int argc, char ** argv) {
  for (int i = 1; i + 3 < argc; i += 4) {
    double a = parse(argv[i]), b = parse(argv[i + 1]), c = parse(argv[i + 2]), d = parse(argv[i + 3]);
    C z = oracle_div(C(a, b), C(c, d));
    C m = model_v2(a, b, c, d);
    long double A = a, B = b, Cc = c, D = d;
    long double den = Cc * Cc + D * D;
    long double X = (A * Cc + B * D) / den, Y = (B * Cc - A * D) / den;
    std::printf("in  a=%a b=%a c=%a d=%a\n", a, b, c, d);
    std::printf("  oracle (%a, %a)\n  model  (%a, %a)\n  exact  (%a, %a)\n", z.real(), z.imag(), m.real(), m.imag(),
                (double)X, (double)Y);
    bool dbr = std::fabs(c) < std::fabs(d);
    double r = dbr ? c / d : d / c;
    std::printf("  branch=%s r=%a\n", dbr ? "r=c/d" : "r=d/c", r);
    {  // numerator scaled by 2^52, quotient scaled back (after the CD scaling)
      double sa = a, sb = b, sc = c, sd = d;
      if (std::fmax(std::fabs(c), std::fabs(d)) < 0x1p-52) { sa *= 0x1p52; sb *= 0x1p52; sc *= 0x1p52; sd *= 0x1p52; }
      C w = model_v1(sa * 0x1p52, sb * 0x1p52, sc, sd);
      std::printf("  S52    (%a, %a)\n", w.real() * 0x1p-52, w.imag() * 0x1p-52);
      double den = dbr ? std::fma(sc, r, sd) : std::fma(sd, r, sc);
      double nx = dbr ? std::fma(sa, r, sb) : std::fma(sb, r, sa);
      double ny = dbr ? std::fma(sb, r, -sa) : std::fma(-sa, r, sb);
      std::printf("  den=%a numx=%a numy=%a\n", den, nx, ny);
    }
  }
}
