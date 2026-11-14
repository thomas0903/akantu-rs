// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): does upstream's HeatTransferModel run on a distributed
// mesh? DiffusionModel::getNbData/packData(elements, _diffusion) forward the
// tag to ConstitutiveLawsHandler, which forwards it to every law, and
// DiffusionLaw::getNbData raises AKANTU_ERROR("Unknown ghost synchronization
// tag") for every tag but _diffusion_gradient (UD-275). The probe distributes
// a mesh, builds the model and records, per rank and per analysis, whether
// initFull / solveStep / getEnergy succeed and the exception text.
//
// Usage: mpirun -np P diffusion_par_probe <out_prefix> <mesh> <material>
// (writes <out_prefix>.<rank>)

#include <aka_common.hh>
#include <communicator.hh>
#include <heat_transfer_model.hh>
#include <mesh.hh>

#include <fstream>
#include <functional>
#include <memory>
#include <iostream>
#include <sstream>
#include <string>

using namespace akantu;

namespace {
std::string attempt(const std::string & what, const std::function<void()> & f) {
  try {
    f();
    return what + ": ok";
  } catch (debug::Exception & e) {
    return what + ": akantu exception: " + e.info();
  } catch (std::exception & e) {
    return what + ": std::exception: " + e.what();
  }
}
} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: diffusion_par_probe <out_file> <mesh> <material>\n";
    return 2;
  }
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  const auto & comm = Communicator::getStaticCommunicator();
  const auto prank = comm.whoAmI();
  const auto psize = comm.getNbProc();
  std::ostringstream report;
  getStaticParser().parse(argv[3]);
  for (const auto method : {_explicit_lumped_mass, _static}) {
    Mesh mesh(2);
    if (prank == 0) {
      mesh.read(argv[2]);
    }
    mesh.distribute();
    const std::string tag = method == _static ? "static" : "explicit";
    std::unique_ptr<HeatTransferModel> model;
    report << "[" << tag << "] rank " << prank << "/" << psize << ": "
           << attempt("construct", [&]() { model = std::make_unique<HeatTransferModel>(mesh); })
           << "\n";
    if (not model) {
      continue;
    }
    report << "[" << tag << "] rank " << prank << ": " << attempt("initFull", [&]() {
      // default sparse solver (_auto: MUMPS in the oracle; Eigen refuses
      // more than one rank)
      model->initFull(_analysis_method = method);
    }) << "\n";
    report << "[" << tag << "] rank " << prank << ": " << attempt("solveStep", [&]() {
      if (method != _static) {
        model->setTimeStep(0.5 * model->getStableTimeStep());
      }
      model->solveStep();
    }) << "\n";
  }
  // one report file per rank (<out_file>.<rank>)
  auto text = report.str();
  {
    std::ofstream out(std::string(argv[1]) + "." + std::to_string(prank),
                      std::ios::binary | std::ios::trunc);
    out << "nb_proc = " << psize << "\n" << text;
  }
  std::cout << text << std::flush;
  finalize();
  return 0;
}
