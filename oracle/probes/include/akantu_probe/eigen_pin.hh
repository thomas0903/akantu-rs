// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct Akantu counterpart: it enforces the D-050 Eigen cache-size pin of the
// x86_64 oracle inside every C++ probe (D-073).
//
// Include it AFTER the Akantu headers (they include Eigen with Akantu's
// plugins; including <Eigen/Core> first would break them), e.g. as the last
// #include of the probe:
//
//   #include "akantu_probe/fixture_writer.hh"
//   #include <solid_mechanics_model.hh>
//   ...
//   #include "akantu_probe/eigen_pin.hh"
//
// Why: Eigen 3.4.0 keeps its L1/L2/L3 cache sizes (which set the blocking of
// GEMM and of the triangular solves, also inside SparseLU) in the
// function-local static of the inline `Eigen::internal::manage_caching_sizes`.
// libakantu.so exports that static, so ONE instance is shared by every module
// of the process and the first module that calls into Eigen initialises it:
// a probe translation unit compiled without the pin (or a preloaded shim)
// can silently re-introduce the host-dependent CPUID sizes inside the pinned
// library (a 64 KiB shim re-creates class X1). The compile-time
// pin of the library is therefore necessary but not sufficient.
//
// What this header does (x86_64 only; nothing on other architectures, so the
// aarch64 manifests and outputs stay unchanged):
//  - compile time: refuses to compile a probe TU that does not carry the
//    D-050 flags (-DEIGEN_NO_CPUID and the three default cache sizes),
//    unless the diagnostic define -DAKANTU_PROBE_ALLOW_UNPINNED_EIGEN is given;
//  - run time: registers a query of the EFFECTIVE sizes (Eigen::l1CacheSize()
//    etc. read the shared static) with the fixture writer, which refuses to
//    write a fixture when they are not 16384 / 524288 / 524288 and records
//    the measured values in the manifest (`build.libraries.
//    eigen_cache_sizes_measured`); probes that write their own text output
//    call `akantu_probe::require_eigen_pin()` and print
//    `akantu_probe::eigen_cache_sizes_measured()`.

#ifndef AKANTU_PROBE_EIGEN_PIN_HH_
#define AKANTU_PROBE_EIGEN_PIN_HH_

#if !defined(EIGEN_WORLD_VERSION)
#error "akantu_probe/eigen_pin.hh must be included after the Akantu headers (Eigen not included yet)"
#endif

#include <cstdio>
#include <cstdlib>
#include <string>

#include "akantu_probe/fixture_writer.hh"

#if defined(__x86_64__) && !defined(AKANTU_PROBE_ALLOW_UNPINNED_EIGEN)
#if !defined(EIGEN_NO_CPUID) || !defined(EIGEN_DEFAULT_L1_CACHE_SIZE) ||       \
    !defined(EIGEN_DEFAULT_L2_CACHE_SIZE) ||                                   \
    !defined(EIGEN_DEFAULT_L3_CACHE_SIZE)
#error "x86_64 probe compiled without the D-050 Eigen pin: take the flags of the oracle install (build-info.txt / oracle-flags.cmake)"
#endif
static_assert(EIGEN_DEFAULT_L1_CACHE_SIZE == 16384 &&
                  EIGEN_DEFAULT_L2_CACHE_SIZE == 524288 &&
                  EIGEN_DEFAULT_L3_CACHE_SIZE == 524288,
              "D-050: the x86_64 oracle pins Eigen's cache sizes to "
              "16384 / 524288 / 524288 bytes");
#endif

namespace akantu_probe {

/// Effective Eigen cache sizes of this process, read from Eigen's shared
/// static (initialised by whichever module calls Eigen first).
inline void eigen_cache_sizes(long & l1, long & l2, long & l3) {
  l1 = static_cast<long>(Eigen::l1CacheSize());
  l2 = static_cast<long>(Eigen::l2CacheSize());
  l3 = static_cast<long>(Eigen::l3CacheSize());
}

/// "l1,l2,l3" as measured in this process.
inline std::string eigen_cache_sizes_measured() {
  long l1 = 0;
  long l2 = 0;
  long l3 = 0;
  eigen_cache_sizes(l1, l2, l3);
  return std::to_string(l1) + "," + std::to_string(l2) + "," +
         std::to_string(l3);
}

/// x86_64: abort (exit 3) unless the effective sizes are the D-050 values
/// (or AKANTU_PROBE_ALLOW_UNPINNED_EIGEN=1 in the environment). No-op
/// elsewhere.
inline void require_eigen_pin() {
#if defined(__x86_64__)
  std::string err = detail::eigen_pin_error();
  if (not err.empty()) {
    std::fprintf(stderr, "%s\n", err.c_str());
    std::exit(3);
  }
#endif
}

namespace detail {
/// Registers the query with the fixture writer before main() runs (a C++17
/// inline variable: one registration per program, whatever the number of
/// TUs including this header).
inline const bool eigen_query_registered = [] {
  eigen_cache_query() = &eigen_cache_sizes;
  return true;
}();
} // namespace detail

} // namespace akantu_probe

#endif // AKANTU_PROBE_EIGEN_PIN_HH_
