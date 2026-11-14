// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port), see fe_engine_probe_impl.hh. Translation unit instantiating: _hexahedron_8, _hexahedron_20.
#include "fe_engine_probe_impl.hh"

namespace fe_probe {
template void probe_regular<_hexahedron_8>(const std::filesystem::path &, const std::filesystem::path &);
template void probe_regular<_hexahedron_20>(const std::filesystem::path &, const std::filesystem::path &);
} // namespace fe_probe
