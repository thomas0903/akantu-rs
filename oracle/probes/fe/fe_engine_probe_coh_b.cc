// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port), see fe_engine_probe_impl.hh. Translation unit instantiating: _cohesive_3d_6, _cohesive_3d_12.
#include "fe_engine_probe_impl.hh"

namespace fe_probe {
template void probe_cohesive<_cohesive_3d_6>(const std::filesystem::path &);
template void probe_cohesive<_cohesive_3d_12>(const std::filesystem::path &);
} // namespace fe_probe
