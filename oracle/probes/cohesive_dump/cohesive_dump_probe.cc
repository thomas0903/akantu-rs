// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) (parity row COH-013): runs the upstream
// TestSMMCFixture scenario (test/test_model/test_solid_mechanics_model/
// test_cohesive/test_cohesive_fixture.hh, mode I) on the frozen meshes and
// dumps the "cohesive elements" dumper (registered by the
// SolidMechanicsModelCohesive constructor) or a user-registered "facets"
// dumper at fixed step counts; every file written by the dumper is recorded
// byte for byte (fixtures/model/cohesive_dump/<case>, roles "dump:<path
// relative to the dump directory>").
//
// Cases (ids model/cohesive_dump/<case>, format v1):
//   intrinsic_tri3_bilinear_base64   2-D, intrinsic cohesive_bilinear
//                                    (material_1.dat), base64, base name
//                                    "cohesive", every nodal field and every
//                                    cohesive internal, dumps after 0/10/20/30
//                                    steps (.pvd: setTimeStep of the fixture).
//   extrinsic_tri3_linear_base64     2-D, extrinsic cohesive_linear
//                                    (material_0.dat), default base name (the
//                                    model id), dumps after 0/25/26/40 steps
//                                    (0 → 3 → 5 → 5 cohesive elements).
//   extrinsic_tri3_linear_text       as above in text mode, base name
//                                    "cohesive_text".
//   extrinsic_tet4_linear_base64     3-D tet4, extrinsic, with material_index,
//                                    dumps after 35/39/50 steps (4 → 6 → 10
//                                    cohesive elements).
//   intrinsic_tet4_bilinear_text     3-D tet4, intrinsic, text, 0/15/30.
//   extrinsic_tri6_linear_text       2-D tri6 (_cohesive_2d_6, VTK polygon
//                                    reorder), extrinsic, text, 0/36/37/45.
//   facets_tri3_extrinsic            2-D extrinsic: a "facets" dumper on the
//                                    facet mesh (dim - 1), displacement and the
//                                    facet internal sigma_c, its own time step,
//                                    dumps after 0 and 10 steps (before any
//                                    insertion).
//   evidence_material_index_before_insertion
//                                    evidence only: material_index on the
//                                    cohesive-element dumper of an extrinsic
//                                    model dumped before the first insertion
//                                    (upstream raises: the law-index map has
//                                    no cohesive block yet, UD-756) and after.
//   evidence_facets_tri3_intrinsic   evidence only (not a golden for byte
//                                    comparison): the "facets" dumper of an
//                                    intrinsic model, sigma_c (no facet
//                                    filter): upstream writes an empty data
//                                    array beside the facet cells (UD-755).
//   evidence_cohesive_unknown_fields evidence only: sigma_c (a facet
//                                    internal), delta_c (a parameter), stress,
//                                    grad_u (regular internals) and strain
//                                    registered one at a time on the
//                                    cohesive-element dumper; per name whether
//                                    the registration and the next dump raise
//                                    (UD-754).
// Diagnostic (COH_DUMP_PROBE_EVIDENCE=1, never promoted):
//   diag_facets_tri3_after_insertion the facets dumper dumped again after the
//                                    insertion of step 24/25 (facets appended
//                                    to the facet mesh, outside every facet
//                                    filter: flattened values from realloc'd
//                                    memory, UD-752).
//
// Usage: cohesive_dump_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: COH_DUMP_PROBE_CASES=<id>[,<id>...] restricts the run.
// D-050: Eigen's cache sizes are pinned at the start of main (for an
// unpinned oracle build); no case uses a
// blocked Eigen kernel or BLAS (explicit lumped-mass dynamics).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <aka_random_generator.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <solid_mechanics_model_cohesive.hh>
// dumper headers after mesh.hh (as oracle/probes/dump/dump_probe.cc)
#include <dumpable_inline_impl.hh>
#include <dumper_iohelper_paraview.hh>

#include <Eigen/Core>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "cohesive_dump_probe";
constexpr const char * kProbeVersion = "1";

using Json = ap::Json;

fs::path g_root, g_up, g_frozen;

bool wanted(const std::string & id) {
  const char * env = std::getenv("COH_DUMP_PROBE_CASES");
  if (env == nullptr || *env == '\0') {
    return true;
  }
  std::stringstream list(env);
  std::string item;
  while (std::getline(list, item, ',')) {
    if (item == id) {
      return true;
    }
  }
  return false;
}

/// Upstream `StrainIncrement` (test_cohesive_fixture.hh).
class StrainIncrement : public BC::Functor {
public:
  StrainIncrement(const Matrix<Real> & strain, BC::Axis dir) : strain_inc(strain), dir(dir) {}
  void operator()(Idx /*node*/, VectorProxy<bool> & flags, VectorProxy<Real> & primal,
                  const VectorProxy<const Real> & coord) {
    if (std::abs(coord(dir)) < 1e-8) {
      return;
    }
    flags.set(true);
    primal += strain_inc * coord;
  }
  static const BC::Functor::Type type = BC::Functor::_dirichlet;

private:
  Matrix<Real> strain_inc;
  BC::Axis dir;
};

/// A registration on the dumper: `vector` = addDumpFieldVectorToDumper.
struct FieldReg {
  std::string name;
  bool vector;
};

struct DumpCase {
  std::string id;
  std::string description;
  ElementType cohesive_type;
  std::string mesh;     // frozen test_cohesive mesh
  bool extrinsic;
  std::string material; // upstream test_cohesive .dat
  std::string dumper;   // "cohesive elements" or "facets"
  std::string base_name; // empty: keep the default (model id / "facets")
  bool text{false};
  std::vector<FieldReg> fields;
  std::vector<std::string> ignored; // registered, upstream registers nothing
  std::vector<unsigned> dumps;      // step counts after which to dump
  bool golden{true};
};

/// D-050 provenance: the numerical environment of the run (Eigen cache
/// sizes pinned in main, OpenBLAS settings of the container; no case uses
/// BLAS).
void record_numerical_env(Json & p) {
  p["eigen_cache_sizes_pinned_in_probe"] = "16384 524288 524288";
  for (const char * name : {"OPENBLAS_CORETYPE", "OPENBLAS_NUM_THREADS", "OMP_NUM_THREADS"}) {
    const char * v = std::getenv(name);
    p[std::string("env_") + name] = v == nullptr ? std::string("<unset>") : std::string(v);
  }
}

void record_dump_tree(ap::FixtureWriter & w, const fs::path & dir) {
  std::vector<fs::path> files;
  for (auto & e : fs::recursive_directory_iterator(dir)) {
    if (e.is_regular_file()) {
      files.push_back(e.path());
    }
  }
  std::sort(files.begin(), files.end());
  Json list = Json::array();
  for (auto & f : files) {
    auto rel = fs::relative(f, dir).generic_string();
    list.push(rel);
    w.add_input_file("dump:" + rel, f, "oracle output");
  }
  w.parameters()["dump_files"] = list;
}

void run_case(const DumpCase & c) {
  const std::string cid = "model/cohesive_dump/" + c.id;
  if (not wanted(c.id)) {
    return;
  }
  const auto dim = Mesh::getSpatialDimension(c.cohesive_type);
  const fs::path tdir = g_up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  const fs::path mdir = g_frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  getStaticParser().parse((tdir / c.material).string());
  RandomGenerator<Idx>::seed(7);

  Mesh mesh(dim);
  mesh.read((mdir / c.mesh).string());
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = c.extrinsic);
  const Real stable_dt = model.getStableTimeStep();
  const Real dt = stable_dt * 0.01;
  model.setTimeStep(dt);

  // TestSMMCFixture::testModeI: initial strain and increment
  auto & mat_el = model.getMaterial("body");
  auto & mat_co = model.getMaterial("insertion");
  Real sigma_c = mat_co.get("sigma_c");
  Real E = mat_el.get("E");
  Real nu = mat_el.get("nu");
  Matrix<Real> strain;
  if (dim == 2) {
    strain = Matrix<Real>{{-nu, 0.}, {0., 1. - nu}};
    strain *= (1. + nu);
  } else {
    strain = Matrix<Real>{{-nu, 0., 0.}, {0., 1., 0.}, {0., 0., -nu}};
  }
  strain *= sigma_c / E;
  const Real init_factor{1. - 1e-5};
  const Real inc_factor{2e-2};
  {
    Matrix<Real> init = init_factor * strain;
    for (auto && [pos, disp] :
         zip(make_view(mesh.getNodes(), dim), make_view(model.getDisplacement(), dim))) {
      disp = init * pos;
    }
  }
  Matrix<Real> inc = (1. / 300) * (inc_factor * strain);

  // the "mass" field can only be registered once the lumped mass exists
  // (as oracle/probes/smm2): assemble it first (the first solveStep does
  // not reassemble it, the results are unchanged)
  model.assembleMassLumped();

  // the dumper
  const fs::path work = fs::temp_directory_path() / ("cohesive_dump_probe_" + c.id);
  fs::remove_all(work);
  fs::create_directories(work);
  if (c.dumper == "facets") {
    mesh.registerDumper<DumperParaview>("facets", "facets");
    mesh.addDumpMeshToDumper("facets", mesh.getMeshFacets(), dim - 1, _not_ghost, _ek_regular);
    mesh.setTimeStepToDumper("facets", dt);
  }
  model.setDirectoryToDumper(c.dumper, work.string());
  if (not c.base_name.empty()) {
    model.setBaseNameToDumper(c.dumper, c.base_name);
  }
  if (c.text) {
    mesh.setTextModeToDumper(c.dumper);
  }
  for (const auto & f : c.fields) {
    if (f.vector) {
      model.addDumpFieldVectorToDumper(c.dumper, f.name);
    } else {
      model.addDumpFieldToDumper(c.dumper, f.name);
    }
  }
  Json ignored_out = Json::array();
  for (const auto & f : c.ignored) {
    try {
      model.addDumpFieldToDumper(c.dumper, f);
      ignored_out.push(f);
    } catch (std::exception & e) {
      ignored_out.push(f + ": " + e.what());
    }
  }

  std::vector<std::int64_t> nb_cohesive_at_dump, nb_facets_at_dump;
  Json dump_errors = Json::array();
  StrainIncrement functor(inc, _y);
  unsigned done = 0;
  for (auto target : c.dumps) {
    for (; done < target; ++done) {
      model.applyBC(functor, "loading");
      model.applyBC(functor, "fixed");
      if (c.extrinsic) {
        model.checkCohesiveStress();
      }
      model.solveStep();
    }
    if (c.golden) {
      model.dump(c.dumper);
    } else {
      try {
        model.dump(c.dumper);
        dump_errors.push("");
      } catch (std::exception & e) {
        dump_errors.push(e.what());
      }
    }
    nb_cohesive_at_dump.push_back(mesh.getNbElement(c.cohesive_type));
    nb_facets_at_dump.push_back(
        mesh.getMeshFacets().getNbElement(mesh.getFacetType(c.cohesive_type)));
  }

  ap::FixtureWriter w(g_root, cid, c.description, kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_tag("dump");
  if (not c.golden) {
    // upstream's output kept as evidence of a defect: not compared byte for
    // byte with the port (which refuses or corrects the case)
    w.add_tag("evidence");
  }
  w.add_input_file("mesh", mdir / c.mesh,
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/" + c.mesh);
  w.add_input_file("material", tdir / c.material,
                   "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/" +
                       c.material);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["cohesive_type"] = std::to_string(c.cohesive_type);
  p["extrinsic"] = c.extrinsic;
  p["random_seed"] = 7LL;
  p["time_step"] = dt;
  p["init_factor"] = init_factor;
  p["inc_factor"] = inc_factor;
  p["dumper"] = c.dumper;
  p["base_name"] = c.base_name;
  p["comparison"] = c.golden ? "bytes" : "evidence";
  p["text_mode"] = c.text;
  Json fields = Json::array();
  for (const auto & f : c.fields) {
    Json j = Json::object();
    j["name"] = f.name;
    j["vector"] = f.vector;
    fields.push(j);
  }
  p["fields"] = fields;
  p["ignored_fields"] = ignored_out;
  Json dumps = Json::array();
  for (auto d : c.dumps) {
    dumps.push(static_cast<long long>(d));
  }
  p["dump_after_steps"] = dumps;
  if (not c.golden) {
    p["dump_errors"] = dump_errors;
  }
  record_numerical_env(p);
  w.add("nb_cohesive_at_dump", {nb_cohesive_at_dump.size()}, {"entry"}, "table",
        nb_cohesive_at_dump)
      .description("mesh.getNbElement(cohesive_type) at each dump");
  w.add("nb_facets_at_dump", {nb_facets_at_dump.size()}, {"entry"}, "table", nb_facets_at_dump)
      .description("number of facets of the cohesive type's facet type at each dump");
  record_dump_tree(w, work);
  w.finish();
  fs::remove_all(work);
  std::cout << "  " << cid << std::endl;
}

/// UD-754 evidence: names that are not a field of the cohesive-element
/// dumper (a facet internal, a parameter, regular-kind internals and a
/// derived field). For each name alone: does the registration raise, and
/// does the next dump raise? (Model::addDumpGroupFieldToDumper falls through
/// to Mesh::createFieldFromAttachedData<Int>, whose non-const getData
/// creates an empty mesh data array of that name.)
void unknown_fields_case() {
  const std::string id = "evidence_cohesive_unknown_fields";
  const std::string cid = "model/cohesive_dump/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path tdir = g_up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  const fs::path mdir = g_frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  const std::string msh = "_cohesive_2d_4_triangle_3.msh";
  const std::string coh = "cohesive elements";
  Json results = Json::array();
  std::int64_t nb_names = 0;
  // each name in a fresh model: re-using the dumper after a raising dump
  // and removeDumpFieldFromDumper crashed (dangling field, observed)
  for (const std::string name : {"sigma_c", "delta_c", "stress", "strain", "grad_u"}) {
    ++nb_names;
    getStaticParser().parse((tdir / "material_0.dat").string());
    RandomGenerator<Idx>::seed(7);
    Mesh mesh(2);
    mesh.read((mdir / msh).string());
    SolidMechanicsModelCohesive model(mesh);
    model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
    model.setTimeStep(model.getStableTimeStep() * 0.01);
    const fs::path work = fs::temp_directory_path() / ("cohesive_dump_probe_" + id + "_" + name);
    fs::remove_all(work);
    fs::create_directories(work);
    model.setDirectoryToDumper(coh, work.string());
    model.addDumpFieldToDumper(coh, "damage");
    Json r = Json::object();
    r["name"] = name;
    bool registered = false;
    try {
      model.addDumpFieldToDumper(coh, name);
      registered = true;
      r["registration_error"] = "";
    } catch (std::exception & e) {
      r["registration_error"] = std::string(e.what());
    }
    r["registered"] = registered;
    r["mesh_data_created"] = mesh.hasData(name);
    if (registered) {
      try {
        model.dump(coh);
        r["dump_error"] = "";
      } catch (std::exception & e) {
        r["dump_error"] = std::string(e.what());
      }
    }
    results.push(r);
    fs::remove_all(work);
  }
  ap::FixtureWriter w(g_root, cid,
                      "UD-754 evidence: names that are not fields of the cohesive-element dumper "
                      "(each registered alone in a fresh model, then a dump)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_tag("dump");
  w.add_tag("evidence");
  w.add_input_file("mesh", mdir / msh,
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/" + msh);
  w.add_input_file("material", tdir / "material_0.dat",
                   "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "material_0.dat");
  auto & p = w.parameters();
  p["comparison"] = "evidence";
  p["dumper"] = coh;
  p["results"] = results;
  record_numerical_env(p);
  std::vector<std::int64_t> n(1, nb_names);
  w.add("nb_names", {1}, {"entry"}, "table", n).description("number of names tried");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

const std::vector<FieldReg> kAllNodal = {
    {"displacement", true},   {"velocity", true}, {"acceleration", true},
    {"internal_force", true}, {"external_force", false}, {"mass", false},
    {"blocked_dofs", false}};

std::vector<FieldReg> all_cohesive() {
  std::vector<FieldReg> v = kAllNodal;
  for (auto n : {"damage", "contact_opening", "contact_tractions", "delta max", "normal",
                 "reversible_energy", "total_energy", "material_index", "partitions"}) {
    v.push_back({n, false});
  }
  v.push_back({"opening", true});
  v.push_back({"tractions", true});
  for (auto n : {"sigma_c_eff", "delta_c_eff", "insertion_stress"}) {
    v.push_back({n, false});
  }
  return v;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: cohesive_dump_probe <fixtures_root> <upstream_src_root> "
                 "<frozen_meshes_root>\n";
    return 2;
  }
  // D-050 pin (no-op on a pinned install)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  g_root = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  const std::string tri3 = "_cohesive_2d_4_triangle_3.msh";
  const std::string tri6 = "_cohesive_2d_6_triangle_6.msh";
  const std::string tet4 = "_cohesive_3d_6_tetrahedron_4.msh";
  const std::string coh = "cohesive elements";
  // no material_index before the first insertion: upstream's dump raises
  // (evidence_material_index_before_insertion, UD-756)
  const std::vector<FieldReg> extrinsic_fields = {
      {"displacement", true}, {"damage", false},      {"opening", true},
      {"tractions", true},    {"sigma_c_eff", false}, {"delta_c_eff", false},
      {"insertion_stress", false}, {"velocity", false}};
  try {
    std::vector<DumpCase> cases;
    cases.push_back({"intrinsic_tri3_bilinear_base64",
                     "TestSMMCFixture IntrinsicModeI (triangle_3, cohesive_bilinear): the "
                     "cohesive-element dumper, every nodal field and cohesive internal, base64, "
                     "four dumps with a .pvd time series",
                     _cohesive_2d_4, tri3, false, "material_1.dat", coh, "cohesive", false,
                     all_cohesive(), {}, {0, 10, 20, 30}});
    cases.push_back({"extrinsic_tri3_linear_base64",
                     "TestSMMCFixture ExtrinsicModeI (triangle_3, cohesive_linear): the "
                     "cohesive-element dumper with its default base name (model id), base64, "
                     "dumps before and after the insertions (0, 3, 5, 5 cohesive elements)",
                     _cohesive_2d_4, tri3, true, "material_0.dat", coh, "", false,
                     extrinsic_fields, {}, {0, 25, 26, 40}});
    cases.push_back({"extrinsic_tri3_linear_text",
                     "as extrinsic_tri3_linear_base64 in text mode (base name cohesive_text)",
                     _cohesive_2d_4, tri3, true, "material_0.dat", coh, "cohesive_text", true,
                     extrinsic_fields, {}, {0, 25, 26, 40}});
    {
      std::vector<FieldReg> f = extrinsic_fields;
      f.push_back({"normal", false});
      f.push_back({"partitions", false});
      f.push_back({"material_index", false});
      cases.push_back({"extrinsic_tet4_linear_base64",
                       "TestSMMCFixture ExtrinsicModeI (tetrahedron_4, cohesive_linear): the "
                       "cohesive-element dumper with material_index, base64, dumps after "
                       "insertions only (4, 6, 10 cohesive elements)",
                       _cohesive_3d_6, tet4, true, "material_0.dat", coh, "cohesive", false,
                       f, {}, {35, 39, 50}});
    }
    cases.push_back({"intrinsic_tet4_bilinear_text",
                     "TestSMMCFixture IntrinsicModeI (tetrahedron_4, cohesive_bilinear): the "
                     "cohesive-element dumper, every nodal field and cohesive internal, text",
                     _cohesive_3d_6, tet4, false, "material_1.dat", coh, "cohesive", true,
                     all_cohesive(), {}, {0, 15, 30}});
    cases.push_back({"extrinsic_tri6_linear_text",
                     "TestSMMCFixture ExtrinsicModeI (triangle_6, _cohesive_2d_6): the "
                     "cohesive-element dumper, text, dumps before and after insertions",
                     _cohesive_2d_6, tri6, true, "material_0.dat", coh, "cohesive", true,
                     extrinsic_fields, {}, {0, 36, 37, 45}});
    cases.push_back({"facets_tri3_extrinsic",
                     "TestSMMCFixture ExtrinsicModeI (triangle_3): a user-registered facets "
                     "dumper (facet mesh, dim - 1) with displacement and the facet internal "
                     "sigma_c, base64, its own time step, dumps before any insertion",
                     _cohesive_2d_4, tri3, true, "material_0.dat", "facets", "", false,
                     {{"displacement", true}, {"sigma_c", false}, {"velocity", false}},
                     {}, {0, 10}});
    {
      DumpCase e{"evidence_material_index_before_insertion",
                 "UD-756 evidence: material_index on the cohesive-element dumper of an "
                 "extrinsic model, dumped before the first insertion (no cohesive block in "
                 "the law-index map) and after it",
                 _cohesive_2d_4, tri3, true, "material_0.dat", coh, "material_index", false,
                 {{"material_index", false}}, {}, {0, 25}};
      e.golden = false;
      cases.push_back(e);
    }
    {
      DumpCase e{"evidence_facets_tri3_intrinsic",
                 "UD-755 evidence: the facets dumper of an intrinsic model with sigma_c (no "
                 "facet filter): upstream writes an empty data array beside the facet cells",
                 _cohesive_2d_4, tri3, false, "material_1.dat", "facets", "", true,
                 {{"displacement", false}, {"sigma_c", false}}, {}, {0}};
      e.golden = false;
      cases.push_back(e);
    }
    const char * ev = std::getenv("COH_DUMP_PROBE_EVIDENCE");
    if (ev != nullptr && std::string(ev) == "1") {
      DumpCase d{"diag_facets_tri3_after_insertion",
                 "UD-752 diagnostic: the facets dumper dumped after the insertions of steps "
                 "24/25 (facets appended outside every facet filter)",
                 _cohesive_2d_4, tri3, true, "material_0.dat", "facets", "", true,
                 {{"sigma_c", false}}, {}, {0, 26, 30}};
      d.golden = false;
      cases.push_back(d);
    }
    for (const auto & c : cases) {
      run_case(c);
    }
    unknown_fields_case();
  } catch (std::exception & e) {
    std::cerr << "cohesive_dump_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
