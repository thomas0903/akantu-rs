// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: entry point wrapping an unmodified upstream Akantu
// (EPFL/LSMS) test driver (compiled with -Dmain=akantu_upstream_main), e.g.
// test/test_model/test_solid_mechanics_model/test_cohesive/
// test_cohesive_extrinsic_implicit/test_assembling_K_cohe_elements.cc.
//
// D-050: the authoritative x86_64 oracle pins Eigen's cache sizes to Eigen's
// own defaults (L1 16 KiB, L2 = L3 512 KiB) so that blocked dense kernels do
// not depend on the host CPU; set before anything else runs.
#include <Eigen/Core>

int akantu_upstream_main(int argc, char * argv[]);

int main(int argc, char * argv[]) {
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  return akantu_upstream_main(argc, argv);
}
