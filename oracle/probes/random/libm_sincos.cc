// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS). Reference values of the
// oracle's glibc sin/cos/sincos for the Rust ports in
// crates/akantu-core/src/glibc_libm/sincos.rs, and the friction-angle sweep of
// MaterialDruckerPrager::updateInternalParameters
// (src/model/solid_mechanics/materials/material_plastic/material_drucker_prager.cc:
// `phi_radian = phi * M_PI / 180.`, `alpha = (6. * sin) / (3. - sin)`,
// `cohesion = fc * (1. - sin) / (2. * cos)`, `k = (6. * cohesion * cos) /
// (3. - sin)`; GCC 14 compiles the sin/cos pair of the library into one
// `sincos` call).
//
// Output (stdout, JSON): FNV-1a hashes of the outputs over splitmix64 inputs
// per (function, domain), the first samples, and the sweep phi = i * 0.01
// degrees, i = 1 .. 8999, fc = 1e7, as 16-hex-digit bit patterns.
#include <bit>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <gnu/libc-version.h>
#include <vector>

#ifndef PROBE_FLAGS
#define PROBE_FLAGS "unknown"
#endif

namespace {

using u64 = std::uint64_t;

double (*volatile p_sin)(double) = ::sin;
double (*volatile p_cos)(double) = ::cos;
void (*volatile p_sincos)(double, double *, double *) = ::sincos;

u64 bits(double x) { return std::bit_cast<u64>(x); }
double from_bits(u64 b) { return std::bit_cast<double>(b); }
u64 canon(double x) { return std::isnan(x) ? 0x7ff8000000000000ULL : bits(x); }

struct SplitMix64 {
  u64 s;
  u64 next() {
    u64 z = (s += 0x9e3779b97f4a7c15ULL);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    return z ^ (z >> 31);
  }
  u64 range(u64 lo, u64 hi) { return lo + next() % (hi - lo + 1); }
  double signed_range(u64 lo, u64 hi) {
    u64 sign = next() & 0x8000000000000000ULL;
    return from_bits(range(lo, hi) | sign);
  }
};

struct Fnv {
  u64 h = 0xcbf29ce484222325ULL;
  void add(u64 v) {
    for (int i = 0; i < 8; ++i) {
      h ^= (v >> (8 * i)) & 0xff;
      h *= 0x100000001b3ULL;
    }
  }
};

constexpr std::size_t N = 1u << 20;
constexpr std::size_t FIRST = 4;

// One domain: inputs of magnitude bit pattern in [lo, hi] with random sign;
// hashes sin, cos, sincos-sine, sincos-cosine (in this order per input).
void domain(bool & first_domain, const char * name, u64 seed, u64 lo, u64 hi) {
  SplitMix64 rng{seed};
  Fnv h_sin, h_cos, h_ss, h_sc;
  std::vector<double> first;
  for (std::size_t i = 0; i < N; ++i) {
    double x = rng.signed_range(lo, hi);
    double s = p_sin(x), c = p_cos(x), ss, sc;
    p_sincos(x, &ss, &sc);
    h_sin.add(canon(s));
    h_cos.add(canon(c));
    h_ss.add(canon(ss));
    h_sc.add(canon(sc));
    if (i < FIRST) {
      first.insert(first.end(), {x, s, c, ss, sc});
    }
  }
  std::printf("%s{\"domain\":\"%s\",\"seed\":%llu,\"lo\":\"%016llx\",\"hi\":\"%016llx\","
              "\"sin\":\"%016llx\",\"cos\":\"%016llx\",\"sincos_sin\":\"%016llx\","
              "\"sincos_cos\":\"%016llx\",\"first\":[",
              first_domain ? "" : ",\n", name, (unsigned long long)seed,
              (unsigned long long)lo, (unsigned long long)hi,
              (unsigned long long)h_sin.h, (unsigned long long)h_cos.h,
              (unsigned long long)h_ss.h, (unsigned long long)h_sc.h);
  for (std::size_t i = 0; i < first.size(); ++i) {
    std::printf("\"%016llx\"%s", (unsigned long long)canon(first[i]),
                i + 1 == first.size() ? "" : ",");
  }
  std::printf("]}");
  first_domain = false;
}

void array(const char * name, const std::vector<double> & v, bool last) {
  std::printf("\"%s\":[", name);
  for (std::size_t i = 0; i < v.size(); ++i) {
    std::printf("\"%016llx\"%s", (unsigned long long)canon(v[i]),
                i + 1 == v.size() ? "" : ",");
  }
  std::printf("]%s\n", last ? "" : ",");
}

} // namespace

int main() {
  std::printf("{\"meta\":{\"glibc\":\"%s\",\"compiler\":\"%s\",\"flags\":\"%s\","
#if defined(__aarch64__)
              "\"arch\":\"aarch64\"},\n",
#elif defined(__x86_64__)
              "\"arch\":\"x86_64\"},\n",
#else
              "\"arch\":\"other\"},\n",
#endif
              gnu_get_libc_version(), __VERSION__, PROBE_FLAGS);
  std::printf("\"n\":%zu,\n\"domains\":[\n", N);
  bool first = true;
  // the branches of __sin/__cos/__sincos (high words 0x3e400000,
  // 0x3e500000, 0x3feb6000, 0x400368fd, 0x419921fb); 0.126 = do_sin's
  // Taylor limit
  domain(first, "tiny", 1, bits(0x1p-60), bits(0x1p-26));
  domain(first, "taylor", 2, bits(0x1p-27), bits(0.126));
  domain(first, "table", 3, bits(0.12), bits(0.855469));
  domain(first, "middle", 4, bits(0.855), bits(2.426266));
  domain(first, "reduce_small", 5, bits(2.426), bits(100.0));
  domain(first, "reduce_large", 6, bits(100.0), bits(105414349.0));
  domain(first, "angle_radians", 7, bits(1e-6), bits(1.5707963267948966));
  std::printf("],\n");

  // MaterialDruckerPrager::updateInternalParameters sweep
  const double fc = 1e7;
  std::vector<double> phi, x, s_sin, s_cos, sc_s, sc_c, alpha, k;
  for (int i = 1; i <= 8999; ++i) {
    double p = i * 0.01;
    double r = p * M_PI / 180.;
    double s, c;
    p_sincos(r, &s, &c);
    phi.push_back(p);
    x.push_back(r);
    s_sin.push_back(p_sin(r));
    s_cos.push_back(p_cos(r));
    sc_s.push_back(s);
    sc_c.push_back(c);
    alpha.push_back((6. * s) / (3. - s));
    double cohesion = fc * (1. - s) / (2. * c);
    k.push_back((6. * cohesion * c) / (3. - s));
  }
  std::printf("\"drucker_prager_sweep\":{\"fc\":\"%016llx\",\n",
              (unsigned long long)bits(fc));
  array("phi", phi, false);
  array("phi_radian", x, false);
  array("sin", s_sin, false);
  array("cos", s_cos, false);
  array("sincos_sin", sc_s, false);
  array("sincos_cos", sc_c, false);
  array("alpha", alpha, false);
  array("k", k, true);
  std::printf("}}\n");
  return 0;
}
