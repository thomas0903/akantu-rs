// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Derived from
// Akantu src/common/aka_extern.cc (copyright EPFL/LSMS, LGPL-3.0-or-later).
//
// ONLY for the "standalone" build of header-only probes (element_probe)
// before the oracle library is installed: the probe is linked with
// src/fe_engine/gauss_integration.cc and src/common/aka_error.cc compiled
// with the oracle flags, plus these three globals, which carry exactly the
// values of aka_extern.cc. The official build links libakantu instead and
// must not compile this file (see oracle/probes/CMakeLists.txt and
// oracle/scripts/generate-fixtures.sh).
#include <aka_common.hh>
#include <aka_error.hh>
#include <aka_math.hh>
#include <aka_random_generator.hh>

#include <limits>

namespace akantu {
namespace debug {
  Debugger debugger;
} // namespace debug
Real Math::tolerance = 1e2 * std::numeric_limits<Real>::epsilon();
template <> long int RandomGenerator<Idx>::_seed = 5489;
} // namespace akantu
