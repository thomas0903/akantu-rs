// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs (UD-654): adversarial oracle probe of
// the periodic summation order. Exercises src/mesh/mesh_periodic.cc
// (makePeriodic, makeConsistentForPeriodicity), src/mesh/mesh_inline_impl.hh
// (addPeriodicSlave, getPeriodicSlaves) and
// src/synchronizer/periodic_node_synchronizer{.cc,_tmpl.hh} of Akantu
// (EPFL/LSMS); not a port of any file.
//
// usage: v90_periodic_order_probe <case-list> <mesh-dir>
//   case-list lines: <name> <mesh-file> <dim> <dirs>   (dirs e.g. "xyz", "zx")
// Output per case (stdout):
//   CASE <name> <nb_nodes> <nb_masters>
//   M <master> <slave>...          getPeriodicSlaves(master) iteration order
//   V <node> <hex bits of every component after the periodic sum>
//   END <name>
// The nodal array (3 components) is filled with values decoded from a
// splitmix64 stream (wide exponent range, random signs) so that every
// summation order of a master's slaves gives different bits with high
// probability; the Rust replay builds the same values bit for bit.
#include <aka_common.hh>
#include <mesh.hh>

#include <Eigen/Core>

#include <bit>
#include <cstdint>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;

namespace {

std::uint64_t splitmix(std::uint64_t x) {
  std::uint64_t z = x + 0x9e3779b97f4a7c15ULL;
  z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
  z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
  return z ^ (z >> 31);
}

// value of component c of node n (same decoding in the Rust replay)
double value(std::uint64_t n, std::uint64_t c) {
  std::uint64_t z = splitmix(n * 8 + c);
  std::uint64_t mant = z & ((std::uint64_t(1) << 52) - 1);
  std::uint64_t e = 1023 - 20 + ((z >> 52) % 41);
  std::uint64_t sign = (z >> 63) & 1;
  return std::bit_cast<double>((sign << 63) | (e << 52) | mant);
}

void run_case(const std::string & name, const std::string & file, Int dim,
              const std::string & dirs) {
  Mesh mesh(dim);
  mesh.read(file);
  for (char d : dirs) {
    SpatialDirection sd = d == 'x' ? _x : (d == 'y' ? _y : _z);
    mesh.makePeriodic(sd);
  }
  const Int nc = 3;
  Array<Real> arr(mesh.getNbNodes(), nc);
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    for (Int c = 0; c < nc; ++c) {
      arr(n, c) = value(std::uint64_t(n), std::uint64_t(c));
    }
  }
  Int nb_masters = 0;
  std::ostringstream ms;
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    if (not mesh.isPeriodicMaster(n)) {
      continue;
    }
    ++nb_masters;
    ms << "M " << n;
    for (auto s : mesh.getPeriodicSlaves(n)) {
      ms << ' ' << s;
    }
    ms << '\n';
  }
  mesh.makeConsistentForPeriodicity(arr);
  std::cout << "CASE " << name << ' ' << mesh.getNbNodes() << ' ' << nb_masters << '\n'
            << ms.str();
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    std::cout << "V " << n;
    for (Int c = 0; c < nc; ++c) {
      std::cout << ' ' << std::hex << std::setw(16) << std::setfill('0')
                << std::bit_cast<std::uint64_t>(arr(n, c)) << std::dec;
    }
    std::cout << '\n';
  }
  std::cout << "END " << name << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 3) {
    std::cerr << "usage: v90_periodic_order_probe <case-list> <mesh-dir>\n";
    return 2;
  }
  // D-050 pin, as the project's probes (no solve here; recorded anyway)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  std::ifstream list(argv[1]);
  std::string line;
  int status = 0;
  while (std::getline(list, line)) {
    if (line.empty() or line[0] == '#') {
      continue;
    }
    std::istringstream is(line);
    std::string name, file, dirs;
    Int dim = 0;
    is >> name >> file >> dim >> dirs;
    try {
      run_case(name, std::string(argv[2]) + "/" + file, dim, dirs);
    } catch (std::exception & e) {
      std::cout << "ERROR " << name << ' ' << e.what() << std::endl;
      status = 1;
    }
  }
  finalize();
  return status;
}
