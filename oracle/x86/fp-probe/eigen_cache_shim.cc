// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: root-cause probe (docs/NUMERICAL_VALIDATION.md,
// "x86_64 corpus").
//
// LD_PRELOAD shim: sets Eigen's cache sizes (Eigen::setCpuCacheSizes) before
// main() from EIGEN_CACHE_SIZES="l1,l2,l3" (bytes). Eigen 3.4.0 keeps them in
// the function-local static of the inline Eigen::internal::manage_caching_sizes,
// which libakantu.so exports, so the preloaded definition is shared with the
// library (checked by EIGEN_CACHE_SIZES_REPORT=1, which prints what Eigen sees).
// On x86_64 Eigen otherwise queries CPUID; on aarch64 it uses its built-in
// defaults 16 KiB / 512 KiB / 512 KiB.
#include <Eigen/Core>
#include <cstdio>
#include <cstdlib>

__attribute__((constructor)) static void akantu_x86_set_eigen_caches() {
  const char * spec = std::getenv("EIGEN_CACHE_SIZES");
  if (spec != nullptr) {
    long l1 = 0, l2 = 0, l3 = 0;
    if (std::sscanf(spec, "%ld,%ld,%ld", &l1, &l2, &l3) == 3 && l1 > 0 && l2 > 0 && l3 > 0) {
      Eigen::setCpuCacheSizes(l1, l2, l3);
    }
  }
  if (std::getenv("EIGEN_CACHE_SIZES_REPORT") != nullptr) {
    std::fprintf(stderr, "[eigen_cache_shim] l1=%ld l2=%ld l3=%ld\n",
                 static_cast<long>(Eigen::l1CacheSize()), static_cast<long>(Eigen::l2CacheSize()),
                 static_cast<long>(Eigen::l3CacheSize()));
  }
}
