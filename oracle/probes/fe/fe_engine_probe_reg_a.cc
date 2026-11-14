// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port), see fe_engine_probe_impl.hh. Translation unit instantiating: _segment_2, _segment_3, _triangle_3.
#include "fe_engine_probe_impl.hh"

namespace fe_probe {
template void probe_regular<_segment_2>(const std::filesystem::path &, const std::filesystem::path &);
template void probe_regular<_segment_3>(const std::filesystem::path &, const std::filesystem::path &);
template void probe_regular<_triangle_3>(const std::filesystem::path &, const std::filesystem::path &);
} // namespace fe_probe
