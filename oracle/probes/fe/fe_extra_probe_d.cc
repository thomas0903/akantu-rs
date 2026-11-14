// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port), see fe_extra_probe_impl.hh. Translation unit instantiating: _pentahedron_15 _hexahedron_8 _hexahedron_20.
#include "fe_extra_probe_impl.hh"

namespace fe_extra {
template void probe_extra<_pentahedron_15>(const std::filesystem::path &, const std::filesystem::path &);
template void probe_extra<_hexahedron_8>(const std::filesystem::path &, const std::filesystem::path &);
template void probe_extra<_hexahedron_20>(const std::filesystem::path &, const std::filesystem::path &);
} // namespace fe_extra
