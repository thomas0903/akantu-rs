// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs project tooling: C++ reference of the explicit-step timing harness
// crates/akantu-solid/examples/explicit_step_timing.rs (same problem: elastic
// material rho 2500, E 70e9, nu 0.22, optional finite deformation, explicit
// lumped mass, dt = 0.8 stable time step, the sine-Gaussian pulse of
// examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc along x),
// linked against an Akantu install (EPFL/LSMS, LGPL-3.0-or-later).
//
//   explicit_step_timing MESH STEPS REPS WARMUP FD(0|1) LABEL [DIGEST_STEPS]
//
// Prints the same JSON lines as the Rust harness for the phases step
// (solveStep), fint (assembleInternalForces) and stress (computeAllStresses
// of every material); with DIGEST_STEPS > 0 it runs that many steps and
// prints the FNV-1a digest of the bits of u, v, a and f_int instead.
#include <solid_mechanics_model.hh>

#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <string>

using namespace akantu;

static void fnv(std::uint64_t & h, const Array<Real> & a) {
  const auto * p = reinterpret_cast<const unsigned char *>(a.data());
  std::size_t n = a.size() * a.getNbComponent() * sizeof(Real);
  for (std::size_t i = 0; i < n; ++i) {
    h ^= p[i];
    h *= 0x00000100000001b3ULL;
  }
}

int main(int argc, char ** argv) {
  if (argc < 7) {
    std::fprintf(stderr, "usage: %s MESH STEPS REPS WARMUP FD LABEL [DIGEST_STEPS]\n", argv[0]);
    return 2;
  }
  std::string mesh_file = argv[1];
  int steps = std::atoi(argv[2]), reps = std::atoi(argv[3]), warmup = std::atoi(argv[4]);
  bool fd = std::atoi(argv[5]) != 0;
  std::string label = argv[6];
  int digest_steps = argc > 7 ? std::atoi(argv[7]) : 0;
  {
    std::ofstream m("material.dat");
    m << "material elastic [\n  name = body\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n"
      << "  finite_deformation = " << (fd ? "true" : "false") << "\n]\n";
  }
  int fake_argc = 1;
  initialize("material.dat", fake_argc, argv);
  const Int dim = 3;
  Mesh mesh(dim);
  auto t0 = std::chrono::steady_clock::now();
  mesh.read(mesh_file);
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  Real dt = model.getStableTimeStep() * 0.8;
  model.setTimeStep(dt);
  const auto & X = mesh.getNodes();
  auto & u = model.getDisplacement();
  Real lo = std::numeric_limits<Real>::max(), hi = std::numeric_limits<Real>::lowest();
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    lo = std::min(lo, X(n, 0));
    hi = std::max(hi, X(n, 0));
  }
  Real len = std::max(hi - lo, std::numeric_limits<Real>::min());
  Real mid = 0.5 * (lo + hi), pulse = 0.25 * len;
  Real k = 0.1 * 2. * M_PI * 3. / pulse, amp = 1e-3 * len;
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    Real xx = (X(n, 0) - mid) / len * 4.;
    u(n, 0) = amp * std::sin(k * xx) * std::exp(-(k * xx) * (k * xx) / (pulse * pulse));
  }
  double init_s = std::chrono::duration<double>(std::chrono::steady_clock::now() - t0).count();
  Int nb_el = mesh.getNbElement(dim, _not_ghost, _ek_regular);
  std::printf("{\"kind\":\"meta\",\"label\":\"%s\",\"mesh\":\"%s\",\"types\":[\"cpp\"],\"nb_elements\":%d,"
              "\"nb_nodes\":%d,\"fd\":%s,\"init_s\":%.6f,\"time_step\":%e}\n",
              label.c_str(), mesh_file.c_str(), int(nb_el), int(mesh.getNbNodes()), fd ? "true" : "false",
              init_s, dt);
  if (digest_steps > 0) {
    for (int s = 0; s < digest_steps; ++s)
      model.solveStep();
    std::uint64_t h = 0xcbf29ce484222325ULL;
    fnv(h, model.getDisplacement());
    fnv(h, model.getVelocity());
    fnv(h, model.getAcceleration());
    fnv(h, model.getInternalForce());
    std::printf("{\"kind\":\"digest\",\"label\":\"%s\",\"steps\":%d,\"fnv1a\":\"%016llx\"}\n", label.c_str(),
                digest_steps, (unsigned long long)h);
    return 0;
  }
  for (int w = 0; w < warmup; ++w)
    model.solveStep();
  const char * phases[] = {"step", "fint", "stress"};
  for (int rep = 0; rep < reps; ++rep) {
    for (const char * phase : phases) {
      auto run = [&]() {
        if (std::strcmp(phase, "step") == 0) {
          model.solveStep();
        } else if (std::strcmp(phase, "fint") == 0) {
          model.assembleInternalForces();
        } else {
          for (auto && mat : model.getMaterials())
            mat.computeAllStresses(_not_ghost);
        }
      };
      run();
      auto t = std::chrono::steady_clock::now();
      for (int s = 0; s < steps; ++s)
        run();
      double sec = std::chrono::duration<double>(std::chrono::steady_clock::now() - t).count();
      std::printf("{\"kind\":\"time\",\"label\":\"%s\",\"phase\":\"%s\",\"rep\":%d,\"steps\":%d,"
                  "\"seconds\":%.6f,\"ns_el_step\":%.3f}\n",
                  label.c_str(), phase, rep, steps, sec, sec / (double(nb_el) * steps) * 1e9);
    }
  }
  finalize();
  return 0;
}
