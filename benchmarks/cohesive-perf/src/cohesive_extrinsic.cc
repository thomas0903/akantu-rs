/**
 * @file   cohesive_extrinsic.cc
 *
 * @author Zineb Fouad <zineb.fouad@epfl.ch>
 * @author Nicolas Richart <nicolas.richart@epfl.ch>
 * @author Seyedeh Mohadeseh Taheri Mousavi <mohadeseh.taherimousavi@epfl.ch>
 * @author Marco Vocialta <marco.vocialta@epfl.ch>
 *
 * @date creation: Tue May 08 2012
 * @date last modification: Wed Feb 06 2019
 *
 * @brief  Cohesive element examples in extrinsic
 *
 *
 * @section LICENSE
 *
 * Copyright (©) 2015-2021 EPFL (Ecole Polytechnique Fédérale de Lausanne)
 * Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
 *
 * Akantu is free software: you can redistribute it and/or modify it under the
 * terms of the GNU Lesser General Public License as published by the Free
 * Software Foundation, either version 3 of the License, or (at your option) any
 * later version.
 *
 * Akantu is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR
 * A PARTICULAR PURPOSE. See the GNU Lesser General Public License for more
 * details.
 *
 * You should have received a copy of the GNU Lesser General Public License
 * along with Akantu. If not, see <http://www.gnu.org/licenses/>.
 *
 */

/* -------------------------------------------------------------------------- */
/* akantu-rs harness copy (benchmarks/cohesive-perf, SPDX LGPL-3.0-or-later)  */
/* Source: gitlab.com/akantu/performance-testing-cohesive @ 483bace8a038      */
/* (cohesive_extrinsic.cc). Every change is marked "akantu-rs:" and is either */
/* inert with the upstream input files or outside the timed regions:          */
/*  1. optional user parameters `export_partition` (default false: upstream   */
/*     `mesh.distribute()`; true: same Scotch partition built explicitly with */
/*     the same unit weights and written to partition.csv, D-009),            */
/*     `energy_precision` (default 6 = the stream default of upstream) and    */
/*     `blocked_node` (default "local" = upstream `blocked_dofs(3, _y)` on    */
/*     the local node 3 of every rank; "global" = the node of global id 3,    */
/*     i.e. node 3 of the mesh file, on every rank that holds it, so that the */
/*     problem does not depend on the rank count);                            */
/*  2. the return value of checkCohesiveStress() (global number of inserted   */
/*     elements, discarded upstream) is kept and logged to insertions.csv     */
/*     after the loop (one integer comparison per step inside the loop);      */
/*  3. after the upstream Chrono output: the same CSV in chrono.csv (rank 0), */
/*     summary.json (rank 0) and summary_rank<r>.json with physical results   */
/*     and partition statistics.                                              */
/* The timed phases and the Chrono CSV format are unchanged.                  */
/* -------------------------------------------------------------------------- */

/* -------------------------------------------------------------------------- */
#include <non_linear_solver.hh>
#include <solid_mechanics_model_cohesive.hh>
/* -------------------------------------------------------------------------- */
#include <chrono>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <map>
// akantu-rs: headers for the optional explicit partition and the summary
#include <cmath>
#include <mesh_partition_scotch.hh>
#include <utility>
#include <vector>

// akantu-rs: statistics of the fillNodesToElements interposer
// (joss_fnte_interposer.cc, JOSS-patch emulation)
extern "C" void akantu_rs_fnte_stats(int * mode, unsigned long * event_calls,
                                     unsigned long * event_skipped,
                                     unsigned long * other_calls,
                                     double * event_time, double * other_time);
/* -------------------------------------------------------------------------- */

using clk = std::chrono::high_resolution_clock;
using seconds = std::chrono::duration<double>;
using milliseconds = std::chrono::duration<double, std::milli>;

//#define AKANTU_VERSION_MAJOR 2
class Chrono {
public:
  Chrono(int prank, int psize) : prank(prank), psize(psize) {}

  inline void start() { _start = clk::now(); };
  inline void store_time(const std::string &type) {
    clk::time_point _end = clk::now();
    if (measures.find(type) == measures.end()) {
      measures[type] = _end - _start;
      nb_measures[type] = 1;
      if (prank == 0) {
        std::cout << "Passing the first " << type << " chrono! [" << measures[type].count() << "]" << std::endl;
      }
    } else {
      measures[type] += _end - _start;
      ++nb_measures[type];
    }

    _start = clk::now();
  }

  virtual void printself(std::ostream &stream, int indent = 0) const {
    std::string space(AKANTU_INDENT, indent);

    stream << space << "Chrono [" << std::endl;
    for (auto && measure : measures) {
      const unsigned int &nb_measure = nb_measures.find(measure.first)->second;
      stream << space << " + " << measure.first << "\t: " << std::setw(25)
             << std::fixed << std::setprecision(16)
             << measure.second.count()
             << "us - nb_repetition: " << nb_measure << std::endl;
    }
    stream << space << "]" << std::endl;
  }

  virtual void printself_csv(std::ostream &stream, int indent = 0) const {
    std::string space(AKANTU_INDENT, indent);
    stream << "\"psize\"";
    for (auto && measure : measures) {
      stream << ", \"" << measure.first << "\""
             << ", \"" << measure.first << " nb_rep" << "\"";
    }
    stream << std::endl;

    stream << psize;
    for (auto && measure : measures) {
      const unsigned int &nb_measure = nb_measures.find(measure.first)->second;
      stream << ", "  << measure.second.count() << ", " << nb_measure;
    }
    stream << std::endl;
  }

private:
  clk::time_point _start;
  std::map<std::string, seconds> measures;
  std::map<std::string, unsigned int> nb_measures;
    int prank, psize;
};

inline std::ostream &operator<<(std::ostream &stream, const Chrono &_this) {
  _this.printself_csv(stream);
  return stream;
}

using namespace akantu;

int main(int argc, char *argv[]) {
  initialize("material.dat", argc, argv);

  const UInt spatial_dimension = 3;

  auto prank = Communicator::getStaticCommunicator().whoAmI();
  auto psize = Communicator::getStaticCommunicator().getNbProc();
  Chrono chrono(prank, psize);

  const auto &usersect = getUserParser();

  const Real c = usersect.getParameter("compression");
  const Real s = usersect.getParameter("shear");
  const Real inc_s = usersect.getParameter("inc_shear");
  const bool output_energy = usersect.getParameter("output_energy", true);
  const bool output_paraview = usersect.getParameter("output_paraview", true);
  const bool cohesive_insertion = usersect.getParameter("cohesive_insertion", true);
  const UInt max_steps = usersect.getParameter("max_steps");
  const std::string mesh_filename = usersect.getParameter("mesh");
  // akantu-rs: optional parameters (defaults reproduce upstream behaviour)
  const bool export_partition =
      usersect.getParameter("export_partition", false);
  const Int energy_precision = usersect.getParameter("energy_precision", 6);
  const std::string blocked_node =
      usersect.getParameter("blocked_node", std::string("local"));
  if (blocked_node != "local" and blocked_node != "global") {
    AKANTU_EXCEPTION("blocked_node must be \"local\" or \"global\"");
  }

  if(prank == 0) {
    std::cout << "Paramters:\n"
              << " - output_energy: " << output_energy << "\n"
              << " - output_paraview: " << output_paraview << "\n"
              << " - cohesive_insertion: " << cohesive_insertion << "\n"
              << " - max_steps: " << max_steps << "\n"
              << " - mesh_filename: " << mesh_filename << "\n";
  }
  chrono.start();

  clk::time_point start_time = clk::now();

  Mesh mesh(spatial_dimension);

  if (prank == 0) {
    mesh.read(mesh_filename);
    chrono.store_time("read_mesh");
  }
  if (not export_partition) {
    mesh.distribute();
  } else {
    // akantu-rs: what Mesh::distributeImpl does for psize > 1 (Scotch, unit
    // edge/vertex weights), with the partition written out (D-009).
    std::shared_ptr<MeshPartition> partition;
    if (prank == 0 and psize > 1) {
      partition =
          std::make_shared<MeshPartitionScotch>(mesh, spatial_dimension);
      partition->partitionate(
          psize, [](auto &&, auto &&) { return 1; }, [](auto &&) { return 1; });
      std::ofstream pout("partition.csv");
      pout << "type, element, partition" << std::endl;
      for (auto type : partition->getPartitions().elementTypes(
               _all_dimensions, _not_ghost, _ek_not_defined)) {
        const auto & parts = partition->getPartition(type, _not_ghost);
        for (Idx el = 0; el < parts.size(); ++el) {
          pout << type << ", " << el << ", " << parts(el) << std::endl;
        }
      }
    }
    mesh.distribute(partition);
  }

  chrono.store_time("dist_mesh");

  SolidMechanicsModelCohesive model(mesh);

  /// model initialization
  model.initFull(_analysis_method = _static,
                 _is_extrinsic = true);

  chrono.store_time("init_full");

  auto &blocked_dofs = model.getBlockedDOFs();
  auto &force = model.getExternalForce();

  /// boundary conditions
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _z), "bottom");     // face
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "right line"); // line
  // akantu-rs: `blocked_node` selects the upstream (local) or global rule
  std::vector<Idx> blocked_y_nodes;
  if (blocked_node == "local") {
    blocked_dofs(3, _y) = true; // point
    blocked_y_nodes.push_back(3);
  } else {
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      if (mesh.getNodeGlobalId(n) == 3) {
        blocked_dofs(n, _y) = true;
        blocked_y_nodes.push_back(n);
      }
    }
  }


  Matrix<Real> compression{{c, 0., 0.}, {0., c, 0.}, {0., 0., c}};
  Matrix<Real> shear{{0., 0., s}, {0., 0., 0.}, {s, 0., 0.}};

  force.zero();

  model.applyBC(BC::Neumann::FromHigherDim(compression), "top");
  model.applyBC(BC::Neumann::FromHigherDim(compression), "bottom");

  model.applyBC(BC::Neumann::FromHigherDim(shear), "top");
  model.applyBC(BC::Neumann::FromHigherDim(shear), "bottom");
  model.applyBC(BC::Neumann::FromHigherDim(shear), "side left");
  model.applyBC(BC::Neumann::FromHigherDim(shear), "side right");

  if (output_paraview) {
    model.setBaseName("extrinsic");
    model.addDumpFieldVector("displacement");
    model.addDumpField("internal_force");
    model.addDumpField("external_force");
    model.addDumpField("stress");
    model.addDumpField("blocked_dofs");
    model.addDumpField("grad_u");

    model.addDumpFieldToDumper("cohesive elements", "displacement");
    model.addDumpFieldToDumper("cohesive elements", "tractions");

    model.dump();
    model.dump("cohesive elements");
  }

  chrono.store_time("initial_conditons");

  model.solveStep("static");
  chrono.store_time("static_solve");

  model.initNewSolver(_explicit_lumped_mass);

  std::ofstream fout;
  if(output_energy and prank == 0) {
    fout.open("energies.csv", std::ofstream::out | std::ofstream::trunc);
    fout << std::setprecision(energy_precision); // akantu-rs: default 6
    fout << "step, ed, ep, ek, ew, et" << std::endl;
  }

  Real Ed{0}, Ep{0}, Ek{0}, Ew{0};
  if (output_energy) {
    Ep = model.getEnergy("potential");
    Ek = model.getEnergy("kinetic");
    Ew += model.getEnergy("external work");
  }
  auto Et = Ed + Ep + Ek - Ew;

  if (output_energy and prank == 0) {
    fout << 0 << ", " << Ed << ", " << Ep << ", " << Ek << ", " << Ew << ", "
         << Et << std::endl;
  }

  Real time_step = model.getStableTimeStep() * 0.05;
  model.setTimeStep(time_step);
  if (prank == 0) {
    std::cout << "Time step: " << time_step << std::endl;
  }

  if (output_paraview) {
    model.addDumpField("velocity");
    model.addDumpField("acceleration");
    model.addDumpFieldToDumper("cohesive elements", "velocity");

    model.dump();
    model.dump("cohesive elements");
  }

  Matrix<Real> new_shear{{0., 0., inc_s}, {0., 0., 0.}, {inc_s, 0., 0.}};

  seconds init_time = clk::now() - start_time;
  chrono.store_time("before_step");

  auto nb_dofs_start = mesh.getNbGlobalNodes() * spatial_dimension;
  // akantu-rs: (step, global number of inserted elements) of insertion steps
  std::vector<std::pair<UInt, UInt>> insertions;

  start_time = clk::now();
  /// Main loop
  for (auto s : arange(1, max_steps + 1)) {
    if (s % 100 == 0 and s < 10000) {
      model.applyBC(BC::Neumann::FromHigherDim(new_shear), "top");
      model.applyBC(BC::Neumann::FromHigherDim(new_shear), "bottom");
      model.applyBC(BC::Neumann::FromHigherDim(new_shear), "side left");
      model.applyBC(BC::Neumann::FromHigherDim(new_shear), "side right");
      chrono.store_time("boundary_conditions");
    }
    if (cohesive_insertion) {
      auto nb_new = model.checkCohesiveStress(); // akantu-rs: keep result
      if (nb_new > 0) {
        insertions.emplace_back(s, nb_new);
      }
      chrono.store_time("check_cohesive_stress");
    }
    model.solveStep("explicit_lumped");
    chrono.store_time("solve_step");

    if (output_energy) {
      Ed = model.getEnergy("dissipated");
      Ep = model.getEnergy("potential");
      Ek = model.getEnergy("kinetic");
      Ew += model.getEnergy("external work");

      Et = Ed + Ep + Ek - Ew;

      if (prank == 0) {
        fout << s << ", " << Ed << ", " << Ep << ", " << Ek << ", " << Ew
             << ", " << Et << std::endl;
      }
      chrono.store_time("energies");
    }

    if (output_paraview and (s % 100 == 0)) {
      model.dump();
      model.dump("cohesive elements");

      if (prank == 0) {
        milliseconds loop_time = clk::now() - start_time;
        std::cout << "passing step " << s << "/" << max_steps
                  << " - nb_cohesive_element: "
                  << mesh.getNbElement(spatial_dimension, _not_ghost,
                                       _ek_cohesive)
                  << " - nb_dofs: "
                  << mesh.getNbGlobalNodes() * spatial_dimension << " - "
                  << loop_time.count() / s << "\t\t \r";
        std::cout.flush();
      }
      chrono.store_time("dumpers");
    }
  }
  auto nb_dofs_end = mesh.getNbGlobalNodes() * spatial_dimension;

  seconds loop_time = clk::now() - start_time;
  auto nb_cohesive_elements = mesh.getNbElement(spatial_dimension, _not_ghost,
                                                _ek_cohesive);
  Communicator::getStaticCommunicator().allReduce(nb_cohesive_elements);
  Ed = model.getEnergy("dissipated");
  if (prank == 0) {
    std::cout << std::endl;
    std::cout << "Cohesive info: dissipated energy: " << Ed
              << " - nb_cohesive_element: "
              << nb_cohesive_elements
              << std::endl;
    std::cout << "Nb proc: "
              << Communicator::getStaticCommunicator().getNbProc()
              << std::endl;
    std::cout << "Full time: " << (init_time + loop_time).count() << std::endl;
    std::cout << "Init time: " << init_time.count() << std::endl;
    std::cout << "Step time: " << loop_time.count()
              << " - nb_steps: " << max_steps
              << " - time_per_step: " << (loop_time.count() / max_steps)
              << std::endl;
    std::cout << "Ns DOFs - start: " << nb_dofs_start
	      << " - end: " << nb_dofs_end << std::endl;
  }

  if (prank == 0) {
    std::cerr << chrono << std::endl;
    // akantu-rs: same CSV in a file (OpenMPI's stderr forwarding can lose
    // part of the many small unbuffered writes of std::cerr)
    std::ofstream cout_csv("chrono.csv");
    cout_csv << chrono;
  }

  if (output_energy and prank == 0) {
    fout.close();
  }

  /* akantu-rs: summary of the physical state after the timed loop ---------- */
  {
    auto & comm = Communicator::getStaticCommunicator();
    const auto & u = model.getDisplacement();
    Real u2{0.};
    Real u_max{0.};
    Real u_sum[3] = {0., 0., 0.};
    Int nb_owned_nodes{0};
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      if (not mesh.isLocalOrMasterNode(n)) {
        continue;
      }
      ++nb_owned_nodes;
      for (Int d = 0; d < Int(spatial_dimension); ++d) {
        u2 += u(n, d) * u(n, d);
        u_sum[d] += u(n, d);
        u_max = std::max(u_max, std::abs(u(n, d)));
      }
    }
    comm.allReduce(u2, SynchronizerOperation::_sum);
    comm.allReduce(u_max, SynchronizerOperation::_max);
    for (auto & v : u_sum) {
      comm.allReduce(v, SynchronizerOperation::_sum);
    }
    auto nb_owned_global = nb_owned_nodes;
    comm.allReduce(nb_owned_global, SynchronizerOperation::_sum);
    auto Ep_end = model.getEnergy("potential");
    auto Ek_end = model.getEnergy("kinetic");
    auto nb_bulk = mesh.getNbElement(spatial_dimension, _not_ghost);
    auto nb_bulk_ghost = mesh.getNbElement(spatial_dimension, _ghost);
    auto nb_coh_local =
        mesh.getNbElement(spatial_dimension, _not_ghost, _ek_cohesive);
    auto nb_bulk_regular =
        mesh.getNbElement(spatial_dimension, _not_ghost, _ek_regular);
    auto nb_bulk_global = nb_bulk_regular;
    comm.allReduce(nb_bulk_global, SynchronizerOperation::_sum);

    int fnte_mode{0};
    unsigned long fnte_event{0}, fnte_skipped{0}, fnte_other{0};
    double fnte_event_time{0.}, fnte_other_time{0.};
    akantu_rs_fnte_stats(&fnte_mode, &fnte_event, &fnte_skipped, &fnte_other,
                         &fnte_event_time, &fnte_other_time);
    const auto & X = mesh.getNodes();
    std::ofstream rout("summary_rank" + std::to_string(prank) + ".json");
    rout << std::setprecision(17) << "{\"rank\": " << prank
         << ", \"blocked_node_rule\": \"" << blocked_node << "\""
         << ", \"blocked_y_nodes\": [";
    for (std::size_t i = 0; i < blocked_y_nodes.size(); ++i) {
      auto n = blocked_y_nodes[i];
      rout << (i == 0 ? "" : ", ") << "{\"local\": " << n
           << ", \"global\": " << mesh.getNodeGlobalId(n) << ", \"X\": ["
           << X(n, 0) << ", " << X(n, 1) << ", " << X(n, 2) << "]}";
    }
    rout << "]"
         << ", \"fill_nodes_to_elements\": {\"mode\": \""
         << (fnte_mode == 1 ? "joss" : "upstream")
         << "\", \"event_calls\": " << fnte_event
         << ", \"event_calls_skipped\": " << fnte_skipped
         << ", \"event_time_s\": " << fnte_event_time
         << ", \"other_calls\": " << fnte_other
         << ", \"other_time_s\": " << fnte_other_time << "}"
         << ", \"nb_nodes\": " << mesh.getNbNodes()
         << ", \"nb_owned_nodes\": " << nb_owned_nodes
         << ", \"nb_regular_elements_not_ghost\": " << nb_bulk_regular
         << ", \"nb_elements_dim3_not_ghost\": " << nb_bulk
         << ", \"nb_elements_dim3_ghost\": " << nb_bulk_ghost
         << ", \"nb_cohesive_not_ghost\": " << nb_coh_local << "}"
         << std::endl;

    if (prank == 0) {
      std::ofstream iout("insertions.csv");
      iout << "step, nb_inserted, nb_cohesive_total" << std::endl;
      UInt total{0};
      for (auto && [step, nb] : insertions) {
        total += nb;
        iout << step << ", " << nb << ", " << total << std::endl;
      }
      std::ofstream sout("summary.json");
      sout << std::setprecision(17) << "{\n"
           << "  \"psize\": " << psize << ",\n"
           << "  \"mesh\": \"" << mesh_filename << "\",\n"
           << "  \"max_steps\": " << max_steps << ",\n"
           << "  \"cohesive_insertion\": "
           << (cohesive_insertion ? "true" : "false") << ",\n"
           << "  \"blocked_node\": \"" << blocked_node << "\",\n"

           << "  \"fill_nodes_to_elements\": \""
           << (fnte_mode == 1 ? "joss" : "upstream") << "\",\n"
           << "  \"time_step\": " << time_step << ",\n"
           << "  \"nb_global_nodes_start\": "
           << nb_dofs_start / spatial_dimension << ",\n"
           << "  \"nb_global_nodes_end\": "
           << nb_dofs_end / spatial_dimension << ",\n"
           << "  \"nb_owned_nodes_sum\": " << nb_owned_global << ",\n"
           << "  \"nb_regular_elements_dim3\": " << nb_bulk_global << ",\n"
           << "  \"nb_cohesive_elements\": " << nb_cohesive_elements << ",\n"
           << "  \"nb_insertion_steps\": " << insertions.size() << ",\n"
           << "  \"first_insertion_step\": "
           << (insertions.empty() ? 0 : insertions.front().first) << ",\n"
           << "  \"energy_dissipated\": " << Ed << ",\n"
           << "  \"energy_potential\": " << Ep_end << ",\n"
           << "  \"energy_kinetic\": " << Ek_end << ",\n"
           << "  \"displacement_l2_norm\": " << std::sqrt(u2) << ",\n"
           << "  \"displacement_max_abs\": " << u_max << ",\n"
           << "  \"displacement_sum\": [" << u_sum[0] << ", " << u_sum[1]
           << ", " << u_sum[2] << "],\n"
           << "  \"loop_time_s\": " << loop_time.count() << ",\n"
           << "  \"init_time_s\": " << init_time.count() << "\n"
           << "}" << std::endl;
    }
  }

  finalize();

  return EXIT_SUCCESS;
}
