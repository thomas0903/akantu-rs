// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): drives the dumpers of the installed Akantu oracle
// (src/io/dumper/*, third-party/iohelper) on small hand-built meshes and
// fields so that the Rust dumpers (crates/akantu-io/src/dumper*) can be
// compared byte for byte with the files written here.
//
// Usage: dump_probe <case> <output_dir> [material.dat]
// Cases (one process per case, so that an upstream abort only loses that
// case; run.sh records the exit status):
//   m2            2D mixed mesh (tri3 + quad4 + seg2): nodal/elemental/
//                 quadrature/mesh-data/partition fields, base64 + ASCII,
//                 time description (pvd), dump()/dump(step)/dump(time, step)
//   m2_raw_qp     non-homogenised quadrature field over types with different
//                 numbers of points (upstream writes inconsistent tuples)
//   all_types     one element of every dumpable type (type map, reorders)
//   coh3d16       mesh containing _cohesive_3d_16 (upstream throws)
//   dkt18         mesh containing _discrete_kirchhoff_triangle_18 (throws)
//   groups        element-group and node-group dumpers, filtered fields
//   parallel      simulated parallel contexts (3 and 11 pieces)
//   text          DumperText space/csv, filtered, variables, time file
//   text_mpi_mesh / text_mpi_filtered  (2 MPI ranks) nodes_type, UD-052
//   lammps        IOHelper LAMMPS atomic and bond writers
//   iohelper      the two IOHelper tests (tetra4 ParaView + text, raw API)
//   smm2d/smm3d   solid-mechanics derived fields (needs material.dat);
//                 smm2d_strain_pad: padded 2D strain alone (upstream aborts
//                 while registering the field, UD-112)
// Every case also writes <output_dir>/inputs.txt with the exact (hex) input
// arrays the Rust tests feed to their resolvers.

#include <aka_common.hh>
#include <mesh.hh>
#include <dumpable_inline_impl.hh>
#include <dumper_compute.hh>
#include <dumper_element_partition.hh>
#include <dumper_elemental_field.hh>
#include <dumper_homogenizing_field.hh>
#include <dumper_internal_material_field.hh>
#include <dumper_iohelper_paraview.hh>
#include <dumper_nodal_field.hh>
#include <akantu/dumper_text.hh>
#include <dumper_variable.hh>
#include <element_group.hh>
#include <group_manager_inline_impl.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <node_group.hh>
#include <solid_mechanics_model.hh>

#include <iohelper/dumper_lammps.hh>
#include <iohelper/dumper_paraview.hh>
#include <iohelper/dumper_text.hh>

#include <cmath>
#include <cstdint>
#include <cstring>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <sstream>
#include <string>
#include <sys/stat.h>
#include <vector>

using namespace akantu;

namespace {

std::ofstream inputs;

void mkdirs(const std::string & path) {
  std::string cur;
  std::stringstream ss(path);
  std::string part;
  if (!path.empty() && path[0] == '/') {
    cur = "/";
  }
  while (std::getline(ss, part, '/')) {
    if (part.empty()) {
      continue;
    }
    cur += part + "/";
    mkdir(cur.c_str(), 0755);
  }
}

std::string hex(double v) {
  std::uint64_t b;
  std::memcpy(&b, &v, 8);
  std::stringstream s;
  s << std::hex << std::setw(16) << std::setfill('0') << b;
  return s.str();
}

template <typename T>
void put_array(const std::string & name, const Array<T> & a) {
  inputs << "array " << name << " " << a.size() << " " << a.getNbComponent()
         << "\n";
  for (Int i = 0; i < a.size(); ++i) {
    for (Int c = 0; c < a.getNbComponent(); ++c) {
      if constexpr (std::is_same_v<T, Real>) {
        inputs << hex(a(i, c));
      } else {
        inputs << static_cast<long long>(a(i, c));
      }
      inputs << (c + 1 == a.getNbComponent() ? "\n" : " ");
    }
  }
}

void put_value(const std::string & name, double v) {
  inputs << "value " << name << " " << hex(v) << "\n";
}

/* ---------------------------------------------------------------------- */
/* The 2D mixed mesh M2 (also built by the Rust tests).                    */
/* ---------------------------------------------------------------------- */
// clang-format off
const double m2_nodes[8][2] = {
    {0.0, 0.0}, {1.0, 0.0}, {2.0, 0.0},
    {0.0, 1.0}, {1.0 / 3.0, 1.0}, {2.0, 1.1},
    {0.5, 2.0}, {1.5, -2.5e-7}};
const Idx m2_quad4[2][4] = {{0, 1, 4, 3}, {1, 2, 5, 4}};
const Idx m2_tri3[3][3] = {{3, 4, 6}, {4, 7, 5}, {4, 7, 6}};
const Idx m2_seg2[2][2] = {{0, 1}, {1, 2}};
// clang-format on

void build_m2(Mesh & mesh, bool with_segments = true) {
  MeshAccessor ma(mesh);
  auto & nodes = ma.getNodes();
  nodes.resize(8);
  for (Int n = 0; n < 8; ++n) {
    for (Int d = 0; d < 2; ++d) {
      nodes(n, d) = m2_nodes[n][d];
    }
  }
  auto & q = ma.getConnectivity(_quadrangle_4);
  q.resize(2);
  for (Int e = 0; e < 2; ++e) {
    for (Int k = 0; k < 4; ++k) {
      q(e, k) = m2_quad4[e][k];
    }
  }
  auto & t = ma.getConnectivity(_triangle_3);
  t.resize(3);
  for (Int e = 0; e < 3; ++e) {
    for (Int k = 0; k < 3; ++k) {
      t(e, k) = m2_tri3[e][k];
    }
  }
  if (with_segments) {
    auto & s = ma.getConnectivity(_segment_2);
    s.resize(2);
    for (Int e = 0; e < 2; ++e) {
      for (Int k = 0; k < 2; ++k) {
        s(e, k) = m2_seg2[e][k];
      }
    }
  }
  ma.makeReady();
}

/// Deterministic test values (exact binary fractions plus a few awkward
/// decimal values so that the text formats are exercised).
double val(Int i, Int c, Int seed) {
  static const double odd[] = {0.1,     -1.0 / 3.0, 2.5e-7, -1.0e20,
                               123.456, 1.0e-300,   -0.0,   7.0};
  return odd[(i * 3 + c + seed) % 8] * (1 + i) + 0.25 * c - seed;
}

/* ---------------------------------------------------------------------- */
struct M2Fields {
  Array<Real> disp{8, 2, "disp"};
  Array<Real> velo{8, 2, "velo"};
  Array<bool> blocked{8, 2, "blocked"};
  Array<Int> nint{8, 1, "nint"};
  Array<UInt> nuint{8, 1, "nuint"};
  ElementTypeMapArray<Real> ereal{"ereal"};
  ElementTypeMapArray<Int> eint{"eint"};
  ElementTypeMapArray<Idx> mat{"material_index"};
  ElementTypeMapArray<Real> qp{"qp"};
  ElementTypeMap<Int> qp_nb_data;

  void fill(Int seed) {
    for (Int n = 0; n < 8; ++n) {
      for (Int c = 0; c < 2; ++c) {
        disp(n, c) = val(n, c, seed);
        velo(n, c) = val(n, c, seed + 3) * 0.5;
        blocked(n, c) = ((n + c + seed) % 3) == 0;
      }
      nint(n) = (n * 7 + seed) % 5 - 2;
      nuint(n) = 4000000000U - UInt(n + seed);
    }
    for (auto && [type, nel, nq] :
         {std::tuple<ElementType, Int, Int>{_triangle_3, 3, 3},
          std::tuple<ElementType, Int, Int>{_quadrangle_4, 2, 4}}) {
      if (!ereal.exists(type)) {
        ereal.alloc(nel, 1, type, _not_ghost);
        eint.alloc(nel, 2, type, _not_ghost);
        mat.alloc(nel, 1, type, _not_ghost);
        qp.alloc(nel * nq, 3, type, _not_ghost);
        qp_nb_data(type) = nq * 3;
      }
      for (Int e = 0; e < nel; ++e) {
        ereal(type)(e) = val(e, Int(type), seed) + 0.5;
        eint(type)(e, 0) = e - Int(type) + seed;
        eint(type)(e, 1) = 100 * e + seed;
        mat(type)(e) = (e + seed) % 2;
      }
      for (Int q = 0; q < nel * nq; ++q) {
        for (Int c = 0; c < 3; ++c) {
          qp(type)(q, c) = val(q, c, seed + Int(type)) / 3.0;
        }
      }
    }
  }

  void put(const std::string & tag) {
    put_array("disp" + tag, disp);
    put_array("velo" + tag, velo);
    put_array("blocked" + tag, blocked);
    put_array("nint" + tag, nint);
    put_array("nuint" + tag, nuint);
    for (auto type : {_triangle_3, _quadrangle_4}) {
      std::string t = std::string(":") + std::to_string(Int(type));
      put_array("ereal" + tag + t, ereal(type));
      put_array("eint" + tag + t, eint(type));
      put_array("mat" + tag + t, mat(type));
      put_array("qp" + tag + t, qp(type));
    }
  }
};

void add_tag_data(Mesh & mesh) {
  MeshAccessor ma(mesh);
  for (auto type : {_segment_2, _triangle_3, _quadrangle_4}) {
    auto & tag = ma.getData<Int>("tag_0", type);
    for (Int e = 0; e < tag.size(); ++e) {
      tag(e) = 10 * Int(type) + e;
    }
  }
}

std::shared_ptr<dumpers::Field> homogenized_qp(M2Fields & f, Int dim) {
  auto raw = std::make_shared<dumpers::InternalMaterialField<Real>>(
      f.qp, dim, _not_ghost, _ek_regular);
  raw->setNbDataPerElem(f.qp_nb_data);
  std::shared_ptr<dumpers::Field> field = raw;
  auto h = dumpers::HomogenizerProxy::createHomogenizer(*field);
  return dumpers::FieldComputeProxy::createFieldCompute(field, std::move(h));
}

void register_m2_fields(DumperIOHelper & d, Mesh & mesh, M2Fields & f) {
  auto disp = std::make_shared<dumpers::NodalField<Real>>(f.disp);
  disp->setPadding(3);
  d.registerField("disp", disp);
  d.registerField("velo", std::make_shared<dumpers::NodalField<Real>>(f.velo));
  d.registerField("blocked",
                  std::make_shared<dumpers::NodalField<bool>>(f.blocked));
  d.registerField("nint", std::make_shared<dumpers::NodalField<Int>>(f.nint));
  d.registerField("nuint",
                  std::make_shared<dumpers::NodalField<UInt>>(f.nuint));
  d.registerField("disp_y",
                  std::make_shared<dumpers::NodalField<Real>>(f.disp, 1, 1));
  d.registerField("ereal", std::make_shared<dumpers::ElementalField<Real>>(
                               f.ereal, 2, _not_ghost, _ek_regular));
  d.registerField("eint", std::make_shared<dumpers::ElementalField<Int>>(
                              f.eint, 2, _not_ghost, _ek_regular));
  d.registerField("material_index",
                  std::make_shared<dumpers::ElementalField<Idx>>(
                      f.mat, 2, _not_ghost, _ek_regular));
  d.registerField("partitions",
                  std::make_shared<dumpers::ElementPartitionField<>>(
                      mesh.getConnectivities(), 2, _not_ghost, _ek_regular));
  d.registerField("qp", homogenized_qp(f, 2));
  d.registerField("tag_0", mesh.createFieldFromAttachedData<Int>(
                               "tag_0", "all", _ek_regular));
}

void case_m2(const std::string & out) {
  Mesh mesh(2);
  build_m2(mesh);
  add_tag_data(mesh);
  M2Fields f;
  f.fill(0);
  f.put(":0");

  DumperParaview b64("m2", out + "/paraview", false);
  b64.registerMesh(mesh, 2, _not_ghost, _ek_regular);
  register_m2_fields(b64, mesh, f);
  b64.setTimeStep(0.1);

  DumperParaview asc("m2_ascii", out + "/paraview", false);
  asc.getDumper().setMode(iohelper::TEXT);
  asc.registerMesh(mesh, 2, _not_ghost, _ek_regular);
  register_m2_fields(asc, mesh, f);
  asc.setTimeStep(0.1);

  // all dimensions / all kinds: segments become cells too
  DumperParaview all("m2_all", out + "/paraview", false);
  all.registerMesh(mesh, _all_dimensions, _not_ghost, _ek_not_defined);
  all.registerField("velo",
                    std::make_shared<dumpers::NodalField<Real>>(f.velo));

  b64.dump();
  asc.dump();
  all.dump();
  f.fill(1);
  f.put(":1");
  b64.dump(5);
  asc.dump(5);
  f.fill(2);
  f.put(":2");
  b64.dump(0.35, 7);
  asc.dump(0.35, 7);
  b64.dump();
  asc.dump();
}

void case_m2_raw_qp(const std::string & out) {
  Mesh mesh(2);
  build_m2(mesh);
  M2Fields f;
  f.fill(0);
  f.put(":0");
  for (auto mode : {iohelper::BASE64, iohelper::TEXT}) {
    DumperParaview d(mode == iohelper::TEXT ? "raw_ascii" : "raw",
                     out + "/paraview", false);
    d.getDumper().setMode(mode);
    d.registerMesh(mesh, 2, _not_ghost, _ek_regular);
    auto raw = std::make_shared<dumpers::InternalMaterialField<Real>>(
        f.qp, 2, _not_ghost, _ek_regular);
    raw->setNbDataPerElem(f.qp_nb_data);
    d.registerField("qp_raw", raw);
    d.dump();
  }
}

/* ---------------------------------------------------------------------- */
const ElementType dumpable_types[] = {
    _point_1,         _segment_2,        _segment_3,      _triangle_3,
    _triangle_6,      _quadrangle_4,     _quadrangle_8,   _tetrahedron_4,
    _tetrahedron_10,  _pentahedron_6,    _pentahedron_15, _hexahedron_8,
    _hexahedron_20,   _cohesive_1d_2,    _cohesive_2d_4,  _cohesive_2d_6,
    _cohesive_3d_12,  _cohesive_3d_6,    _cohesive_3d_8,  _bernoulli_beam_2,
    _bernoulli_beam_3};

void build_types_mesh(Mesh & mesh, const std::vector<ElementType> & types) {
  MeshAccessor ma(mesh);
  auto & nodes = ma.getNodes();
  nodes.resize(20);
  for (Int n = 0; n < 20; ++n) {
    nodes(n, 0) = n;
    nodes(n, 1) = 0.5 * n * n;
    nodes(n, 2) = -1.0 / (n + 1);
  }
  Int t = 0;
  for (auto type : types) {
    auto nnodes = Mesh::getNbNodesPerElement(type);
    auto & c = ma.getConnectivity(type);
    c.resize(1);
    for (Int k = 0; k < nnodes; ++k) {
      c(0, k) = (k * 3 + t) % 20;
    }
    ++t;
  }
  ma.makeReady();
}

void case_all_types(const std::string & out) {
  Mesh mesh(3);
  std::vector<ElementType> types(std::begin(dumpable_types),
                                 std::end(dumpable_types));
  build_types_mesh(mesh, types);
  for (auto mode : {iohelper::BASE64, iohelper::TEXT}) {
    DumperParaview d(mode == iohelper::TEXT ? "types_ascii" : "types",
                     out + "/paraview", false);
    d.getDumper().setMode(mode);
    d.registerMesh(mesh, _all_dimensions, _not_ghost, _ek_not_defined);
    d.dump();
  }
}

void case_throwing_type(const std::string & out, ElementType type,
                        const std::string & name) {
  Mesh mesh(3);
  build_types_mesh(mesh, {_triangle_3, type});
  DumperParaview d(name, out + "/paraview", false);
  d.registerMesh(mesh, _all_dimensions, _not_ghost, _ek_not_defined);
  try {
    d.dump();
    std::cout << "dump-succeeded\n";
  } catch (std::exception & e) {
    std::string what = e.what();
    std::cout << "exception: " << what.substr(0, what.find('\n')) << "\n";
  }
}

/* ---------------------------------------------------------------------- */
void case_groups(const std::string & out) {
  Mesh mesh(2);
  build_m2(mesh);
  M2Fields f;
  f.fill(0);
  f.put(":0");

  auto & grp = mesh.createElementGroup("grp", 2);
  grp.add(Element{_quadrangle_4, 1, _not_ghost}, true);
  grp.add(Element{_triangle_3, 2, _not_ghost}, true);
  grp.add(Element{_triangle_3, 0, _not_ghost}, true);
  grp.optimize();
  put_array("grp_nodes", grp.getNodeGroup().getNodes());
  put_array("grp_tri3", grp.getElements(_triangle_3));
  put_array("grp_quad4", grp.getElements(_quadrangle_4));

  auto & ngrp = mesh.createNodeGroup("ngrp");
  ngrp.add(5);
  ngrp.add(1);
  ngrp.add(3);
  put_array("ngrp_nodes", ngrp.getNodes());

  grp.setDirectory(out + "/paraview");
  grp.addDumpFieldExternal("disp", mesh.createNodalField(&f.disp, "grp", 3));
  grp.addDumpFieldExternal("blocked",
                           mesh.createNodalField(&f.blocked, "grp"));
  grp.addDumpFieldExternal(
      "disp_y", mesh.createStridedNodalField(&f.disp, "grp", 1, 1, 0));
  grp.addDumpFieldExternal(
      "ereal",
      mesh.createElementalField<Real, Vector<Real>, dumpers::ElementalField>(
          f.ereal, "grp", 2, _ek_regular));
  grp.addDumpFieldExternal(
      "eint",
      mesh.createElementalField<Int, Vector<Int>, dumpers::ElementalField>(
          f.eint, "grp", 2, _ek_regular));
  {
    auto field =
        mesh.createElementalField<Real, dumpers::InternalMaterialField>(
            f.qp, "grp", 2, _ek_regular, f.qp_nb_data);
    auto h = dumpers::HomogenizerProxy::createHomogenizer(*field);
    field = dumpers::FieldComputeProxy::createFieldCompute(field,
                                                           std::move(h));
    grp.addDumpFieldExternal("qp", field);
  }
  grp.dump();
  f.fill(1);
  f.put(":1");
  grp.dump();

  // ASCII variant of the group dumper (second dumper on the group)
  grp.registerDumper<DumperParaview>("grp_ascii", "grp_ascii");
  grp.setDirectoryToDumper("grp_ascii", out + "/paraview");
  grp.addDumpFilteredMeshToDumper("grp_ascii", mesh, grp.getElements(),
                                  grp.getNodeGroup().getNodes(),
                                  _all_dimensions);
  grp.setTextModeToDumper("grp_ascii");
  grp.addDumpFieldExternalToDumper("grp_ascii", "disp",
                                   mesh.createNodalField(&f.disp, "grp", 3));
  grp.dump("grp_ascii");

  ngrp.setDirectory(out + "/paraview");
  ngrp.dump();
  ngrp.setTextModeToDumper();
  ngrp.setBaseName("ngrp_ascii");
  ngrp.dump();
}

/* ---------------------------------------------------------------------- */
void case_parallel(const std::string & out) {
  Mesh mesh(2);
  build_m2(mesh);
  M2Fields f;
  f.fill(0);
  f.put(":0");
  for (Int nbp : {3, 11}) {
    DumperParaview d("par" + std::to_string(nbp), out + "/paraview", false);
    auto & io = d.getDumper();
    d.registerMesh(mesh, 2, _not_ghost, _ek_regular);
    d.registerField("velo",
                    std::make_shared<dumpers::NodalField<Real>>(f.velo));
    d.setTimeStep(0.5);
    for (Int step : {0, 2}) {
      for (Int p = 0; p < nbp; ++p) {
        io.setParallelContext(p, nbp, 0);
        d.dump(step);
      }
    }
  }
}

/* ---------------------------------------------------------------------- */
void case_text(const std::string & out) {
  mkdirs(out + "/text");
  Mesh mesh(2);
  build_m2(mesh);
  M2Fields f;
  f.fill(0);
  f.put(":0");
  auto & grp = mesh.createElementGroup("grp", 2);
  grp.add(Element{_quadrangle_4, 1, _not_ghost}, true);
  grp.add(Element{_triangle_3, 0, _not_ghost}, true);
  grp.optimize();
  put_array("grp_nodes", grp.getNodeGroup().getNodes());

  Real scalar = 1.2345567891;
  Int iscalar = -3;
  Vector<Real> vec(2);
  vec(0) = 1.0;
  vec(1) = -2.0 / 3.0;

  DumperText space("txt", iohelper::_tdm_space);
  space.setDirectory(out + "/text");
  space.registerMesh(mesh);
  space.registerField("disp",
                      std::make_shared<dumpers::NodalField<Real>>(f.disp));
  space.registerField("blocked",
                      std::make_shared<dumpers::NodalField<bool>>(f.blocked));
  space.registerField("nint",
                      std::make_shared<dumpers::NodalField<Int>>(f.nint));
  space.registerField("ereal", std::make_shared<dumpers::ElementalField<Real>>(
                                   f.ereal, 2, _not_ghost, _ek_regular));
  space.registerVariable("scalar",
                         std::make_shared<dumpers::Variable<Real>>(scalar));
  space.registerVariable("iscalar",
                         std::make_shared<dumpers::Variable<Int>>(iscalar));
  space.registerVariable("vec",
                         std::make_shared<dumpers::Variable<Vector<Real>>>(vec));

  DumperText csv("txt_csv", iohelper::_tdm_csv);
  csv.setDirectory(out + "/text");
  csv.setPrecision(8);
  csv.setTimeStep(0.25);
  csv.registerFilteredMesh(mesh, grp.getElements(),
                           grp.getNodeGroup().getNodes());
  csv.registerField("disp",
                    std::make_shared<dumpers::NodalField<Real, true>>(
                        f.disp, 0, 0, &grp.getNodeGroup().getNodes()));
  csv.registerField("blocked",
                    std::make_shared<dumpers::NodalField<bool, true>>(
                        f.blocked, 0, 0, &grp.getNodeGroup().getNodes()));
  csv.registerVariable("scalar",
                       std::make_shared<dumpers::Variable<Real>>(scalar));
  csv.registerVariable("vec",
                       std::make_shared<dumpers::Variable<Vector<Real>>>(vec));

  space.dump();
  csv.dump();
  f.fill(1);
  f.put(":1");
  scalar += 2.;
  iscalar += 1;
  vec(0) += 0.1;
  space.dump(0.5, 3);
  csv.dump(0.5, 3);
  f.fill(2);
  f.put(":2");
  scalar *= -1e-12;
  vec(1) = 1e300;
  space.dump();
  csv.dump();
  csv.setTimeStep(0.125); // upstream re-activates and restarts the clock
  csv.dump();
}

/* ---------------------------------------------------------------------- */
// UD-052: in parallel DumperText::registerMesh pairs a ContactState functor
// with the NodeFlag nodal field; registerFilteredMesh uses NodeFlag. Run
// with 2 MPI ranks (undistributed mesh, node flags set by hand).
void case_text_mpi(const std::string & out, bool filtered) {
  auto & comm = Communicator::getStaticCommunicator();
  auto rank = comm.whoAmI();
  std::string dir = out + "/text";
  mkdirs(dir);
  Mesh mesh(2);
  build_m2(mesh);
  {
    MeshAccessor ma(mesh);
    auto & flags = ma.getNodesFlags();
    const NodeFlag f[8] = {NodeFlag::_normal,          NodeFlag::_master,
                           NodeFlag::_slave,           NodeFlag::_pure_ghost,
                           NodeFlag::_periodic_master, NodeFlag::_periodic_slave,
                           NodeFlag::_pbc_slave_dist_ghost,
                           NodeFlag::_distributed};
    for (Int n = 0; n < 8; ++n) {
      flags(n) = f[n];
    }
  }
  Array<Idx> all_nodes(0, 1);
  for (Int n = 0; n < 8; ++n) {
    all_nodes.push_back(Idx((n * 3) % 8));
  }
  put_array("filter_nodes", all_nodes);
  DumperText d(filtered ? "txt_filtered" : "txt_mesh", iohelper::_tdm_space);
  d.setDirectory(dir);
  try {
    if (filtered) {
      d.registerFilteredMesh(mesh, mesh.getConnectivities(), all_nodes);
    } else {
      d.registerMesh(mesh);
    }
    std::cout << "rank " << rank << " registered\n" << std::flush;
    d.dump();
    std::cout << "rank " << rank << " dump-succeeded\n";
  } catch (std::exception & e) {
    std::string what = e.what();
    std::cout << "rank " << rank
              << " exception: " << what.substr(0, what.find('\n')) << "\n";
  }
}

/* ---------------------------------------------------------------------- */
template <iohelper::LammpsAtomStyle style>
void lammps_case(const std::string & out, const std::string & name,
                 std::vector<double> & points, int dim, double * bounds) {
  iohelper::DumperLammps<style> d(bounds, out + "/lammps");
  d.setPoints(points.data(), dim, int(points.size() / dim), name);
  d.init();
  d.dump();
  // second dump: the stream keeps the scientific format of the first one
  std::rename((out + "/lammps/" + name).c_str(),
              (out + "/lammps/" + name + ".first").c_str());
  for (auto & p : points) {
    p *= -2.0;
  }
  d.dump();
}

void case_lammps(const std::string & out) {
  mkdirs(out + "/lammps");
  std::vector<double> p3 = {0.0, 0.1, -1.0 / 3.0, 1e20, 2.5, -7.0,
                            123.456, 1e-300, 0.0};
  std::vector<double> p2 = {0.0, 0.1, -1.0 / 3.0, 1e20, 2.5, -7.0};
  double bounds[6] = {-1.5, 1.0 / 3.0, 0.0, 1e7, -2e-5, 12.0};
  inputs << "lammps p3 " << hex(p3[0]) << "\n";
  lammps_case<iohelper::atomic>(out, "atomic3", p3, 3, nullptr);
  p3 = {0.0, 0.1, -1.0 / 3.0, 1e20, 2.5, -7.0, 123.456, 1e-300, 0.0};
  lammps_case<iohelper::atomic>(out, "atomic3_bounds", p3, 3, bounds);
  lammps_case<iohelper::atomic>(out, "atomic2", p2, 2, nullptr);
  p3 = {0.0, 0.1, -1.0 / 3.0, 1e20, 2.5, -7.0, 123.456, 1e-300, 0.0};
  lammps_case<iohelper::bond>(out, "bond3", p3, 3, nullptr);
  p3 = {0.0, 0.1, -1.0 / 3.0, 1e20, 2.5, -7.0, 123.456, 1e-300, 0.0};
  lammps_case<iohelper::bond>(out, "bond3_bounds", p3, 3, bounds);
}

/* ---------------------------------------------------------------------- */
void case_iohelper(const std::string & out) {
  // third-party/iohelper/test/test_{paraview,text}_dumper.cc on cube.msh
  Mesh mesh(3);
  mesh.read("cube.msh");
  put_array("nodes", mesh.getNodes());
  put_array("tet4", mesh.getConnectivity(_tetrahedron_4));
  std::vector<double> nodes(mesh.getNodes().data(),
                            mesh.getNodes().data() + mesh.getNbNodes() * 3);
  const auto & conn_a = mesh.getConnectivity(_tetrahedron_4);
  std::vector<int> conn(conn_a.data(), conn_a.data() + conn_a.size() * 4);
  int nb_elements = conn_a.size();
  mkdirs(out + "/iohelper");
  {
    iohelper::DumperParaview dumper;
    dumper.setPoints(nodes.data(), 3, int(nodes.size() / 3), "coordinates2");
    dumper.setConnectivity(conn.data(), iohelper::TETRA1, nb_elements,
                           iohelper::C_MODE);
    dumper.addNodeDataField("test", nodes.data(), 3, nodes.size() / 3);
    dumper.setPrefix(out + "/iohelper/");
    dumper.setVTUSubDirectory("test_tetra4-VTUs");
    dumper.init();
    dumper.dump("test_tetra4");
  }
  {
    iohelper::DumperText dumper;
    dumper.setPoints(nodes.data(), 3, int(nodes.size() / 3), "coordinates2");
    dumper.setConnectivity(conn.data(), iohelper::TETRA1, nb_elements,
                           iohelper::C_MODE);
    dumper.addNodeDataField("test", nodes.data(), 3, nodes.size() / 3);
    dumper.setPrefix(out + "/iohelper/");
    dumper.init();
    dumper.dump("test_text_tetra4", 0);
  }
}

/* ---------------------------------------------------------------------- */
class AlternatingSelector : public ConstitutiveLawSelector {
public:
  Idx operator()(const Element & element) override {
    return element.element % 2;
  }
};

void smm_inputs(SolidMechanicsModel & model, Int dim) {
  auto & grad_u = model.flattenInternal("grad_u", _ek_regular);
  auto & stress = model.flattenInternal("stress", _ek_regular);
  for (auto type : model.getMesh().elementTypes(dim, _not_ghost,
                                                 _ek_regular)) {
    std::string t = std::string(":") + std::to_string(Int(type));
    put_array("grad_u" + t, grad_u(type));
    put_array("stress" + t, stress(type));
    put_array("mat" + t, model.getMaterialByElement(type));
  }
  for (Int m = 0; m < Int(model.getNbMaterials()); ++m) {
    auto & mat = model.getMaterial(m);
    put_value("nu:" + std::to_string(m), Real(mat.getParam("nu")));
    if (dim == 2) {
      put_value("plane_stress:" + std::to_string(m),
                Real(bool(mat.getParam("Plane_Stress"))));
    }
  }
  put_array("displacement", model.getDisplacement());
  put_array("blocked_dofs", model.getBlockedDOFs());
}

void smm_setup(Mesh & mesh, SolidMechanicsModel & model, Int dim) {
  model.setMaterialSelector(std::make_shared<AlternatingSelector>());
  model.initFull(_analysis_method = _static);
  auto & u = model.getDisplacement();
  auto & b = model.getBlockedDOFs();
  const auto & x = mesh.getNodes();
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    for (Int d = 0; d < dim; ++d) {
      double xs = x(n, 0), ys = x(n, 1), zs = dim == 3 ? x(n, 2) : 0.0;
      u(n, d) = 0.01 * (d + 1) * xs - 0.02 * ys + 0.003 * xs * ys * (d + 1) +
                0.004 * zs * zs - 0.001 * d;
      b(n, d) = ((n + d) % 2) == 0;
    }
  }
  model.assembleInternalForces();
}

void smm_dumpers(Mesh & mesh, SolidMechanicsModel & model,
                 const std::string & out, const std::string & base, Int dim,
                 bool include_strain_tensor) {
  // default model dumper (base64, padded vectors/tensors)
  model.setBaseName(base);
  mesh.setDirectory(out + "/paraview");
  model.addDumpFieldVector("displacement");
  model.addDumpField("blocked_dofs");
  model.addDumpFieldTensor("stress");
  if (include_strain_tensor) {
    model.addDumpFieldTensor("strain");
    model.addDumpFieldTensor("Green strain");
  }
  model.addDumpField("principal strain");
  model.addDumpField("principal Green strain");
  model.addDumpField("Von Mises stress");
  model.addDumpField("grad_u");
  model.addDumpField("material_index");
  model.addDumpField("partitions");
  model.dump();

  // ASCII, unpadded
  mesh.registerDumper<DumperParaview>("ascii", base + "_ascii");
  mesh.addDumpMeshToDumper("ascii", mesh, dim, _not_ghost, _ek_regular);
  mesh.setDirectoryToDumper("ascii", out + "/paraview");
  mesh.setTextModeToDumper("ascii");
  for (std::string name :
       {"displacement", "stress", "strain", "Green strain", "principal strain",
        "principal Green strain", "Von Mises stress", "grad_u"}) {
    model.addDumpFieldToDumper("ascii", name);
  }
  model.dump("ascii");

  // ASCII, padded (2D: stress padder; strain padder omitted, see the
  // smm2d_strain_pad case)
  mesh.registerDumper<DumperParaview>("ascii_pad", base + "_ascii_pad");
  mesh.addDumpMeshToDumper("ascii_pad", mesh, dim, _not_ghost, _ek_regular);
  mesh.setDirectoryToDumper("ascii_pad", out + "/paraview");
  mesh.setTextModeToDumper("ascii_pad");
  model.addDumpFieldVectorToDumper("ascii_pad", "displacement");
  model.addDumpFieldTensorToDumper("ascii_pad", "stress");
  if (include_strain_tensor) {
    model.addDumpFieldTensorToDumper("ascii_pad", "strain");
  }
  model.dump("ascii_pad");
}

void case_smm2d(const std::string & out, bool strain_pad_only) {
  Mesh mesh(2);
  build_m2(mesh, false);
  SolidMechanicsModel model(mesh);
  smm_setup(mesh, model, 2);
  smm_inputs(model, 2);
  if (strain_pad_only) {
    mesh.registerDumper<DumperParaview>("pad", "smm2d_strain_pad");
    mesh.addDumpMeshToDumper("pad", mesh, 2, _not_ghost, _ek_regular);
    mesh.setDirectoryToDumper("pad", out + "/paraview");
    model.addDumpFieldTensorToDumper("pad", "strain");
    std::cout << "strain padder registered\n" << std::flush;
    model.dump("pad");
    std::cout << "dump-succeeded\n";
    return;
  }
  smm_dumpers(mesh, model, out, "smm2d", 2, false);
}

void case_smm3d(const std::string & out) {
  Mesh mesh(3);
  {
    MeshAccessor ma(mesh);
    auto & nodes = ma.getNodes();
    const double x[9][3] = {{0, 0, 0}, {1, 0, 0}, {1, 1, 0},
                            {0, 1, 0}, {0, 0, 1}, {1, 0, 1},
                            {1, 1, 1}, {0, 1, 1}, {0.5, 0.5, 1.6}};
    nodes.resize(9);
    for (Int n = 0; n < 9; ++n) {
      for (Int d = 0; d < 3; ++d) {
        nodes(n, d) = x[n][d];
      }
    }
    auto & h = ma.getConnectivity(_hexahedron_8);
    h.resize(1);
    for (Int k = 0; k < 8; ++k) {
      h(0, k) = k;
    }
    auto & t = ma.getConnectivity(_tetrahedron_4);
    t.resize(2);
    const Idx tc[2][4] = {{4, 5, 7, 8}, {5, 6, 7, 8}};
    for (Int e = 0; e < 2; ++e) {
      for (Int k = 0; k < 4; ++k) {
        t(e, k) = tc[e][k];
      }
    }
    ma.makeReady();
  }
  SolidMechanicsModel model(mesh);
  smm_setup(mesh, model, 3);
  smm_inputs(model, 3);
  put_array("nodes", mesh.getNodes());
  smm_dumpers(mesh, model, out, "smm3d", 3, true);
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 3) {
    std::cerr << "usage: dump_probe <case> <output_dir> [material.dat]\n";
    return 2;
  }
  std::string kase = argv[1];
  std::string out = argv[2];
  mkdirs(out + "/paraview");
  inputs.open(out + "/inputs.txt");
  inputs << "# dump_probe " << kase
         << " inputs: 'array <name> <tuples> <components>' then rows "
            "(reals as IEEE-754 bit patterns in hex), 'value <name> <hex>'\n";
  if (argc > 3) {
    initialize(argv[3], argc, argv);
  } else {
    initialize(argc, argv);
  }
  if (kase == "m2") {
    case_m2(out);
  } else if (kase == "m2_raw_qp") {
    case_m2_raw_qp(out);
  } else if (kase == "all_types") {
    case_all_types(out);
  } else if (kase == "coh3d16") {
    case_throwing_type(out, _cohesive_3d_16, "coh3d16");
  } else if (kase == "dkt18") {
    case_throwing_type(out, _discrete_kirchhoff_triangle_18, "dkt18");
  } else if (kase == "groups") {
    case_groups(out);
  } else if (kase == "parallel") {
    case_parallel(out);
  } else if (kase == "text") {
    case_text(out);
  } else if (kase == "text_mpi_mesh") {
    case_text_mpi(out, false);
  } else if (kase == "text_mpi_filtered") {
    case_text_mpi(out, true);
  } else if (kase == "lammps") {
    case_lammps(out);
  } else if (kase == "iohelper") {
    case_iohelper(out);
  } else if (kase == "smm2d") {
    case_smm2d(out, false);
  } else if (kase == "smm2d_strain_pad") {
    case_smm2d(out, true);
  } else if (kase == "smm3d") {
    case_smm3d(out);
  } else {
    std::cerr << "unknown case " << kase << "\n";
    return 2;
  }
  inputs.close();
  finalize();
  return 0;
}
