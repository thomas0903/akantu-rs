// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle tooling (not ported from Akantu). LD_PRELOAD library
// pinning Eigen's CPU cache sizes
// (decision D-050: Eigen::setCpuCacheSizes(16384, 524288, 524288), Eigen's
// own defaults, the aarch64 values) before main() of the **unmodified**
// upstream example binaries, which cannot call it themselves. Eigen keeps the
// sizes in the function-local static of the inline
// Eigen::internal::manage_caching_sizes, a default-visibility symbol that
// libakantu.so exports, so the preloaded copy is the one every object of the
// process binds to and the setting reaches the SparseLU/GEBP code compiled
// into libakantu.so. The oracle variants (oracle/probes/examples/*.cc) also
// pin at the start of main. The effective sizes are written to stderr.
#include <Eigen/Core>
#include <cstdio>
#include <cstdlib>

__attribute__((constructor)) static void akantu_rs_eigen_cache_pin() {
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  const long l1 = static_cast<long>(Eigen::l1CacheSize());
  const long l2 = static_cast<long>(Eigen::l2CacheSize());
  const long l3 = static_cast<long>(Eigen::l3CacheSize());
  std::fprintf(stderr, "akantu-rs eigen_cache_pin: l1=%ld l2=%ld l3=%ld\n", l1,
               l2, l3);
  if (l1 != 16384 || l2 != 524288 || l3 != 524288) {
    std::fprintf(stderr, "akantu-rs eigen_cache_pin: pin not effective\n");
    std::abort();
  }
}
