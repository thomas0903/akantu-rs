// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records the law-level allocation and draw order of random internal
// fields in the installed reference library
// (rows CL-002/CL-003):
//
// A probe material (derived from upstream `Material`) registers, in this
// order, the random internal fields `z_rand` (1 component), `a_rand` (2
// components), `m_rand` (1 component, with history), `k_coh` (1 component,
// element kind set to _ek_cohesive after registration) and the plain field
// `p_plain`; all are `.dat` parameters (src/model/common/constitutive_laws/
// {internal_field,random_internal_field}_tmpl.hh, constitutive_law_tmpl.hh,
// constitutive_laws_handler_tmpl.hh). A SolidMechanicsModel on a small
// mixed _triangle_3/_quadrangle_4 mesh (with ghost elements and
// _segment_2 boundary elements) then goes through:
//
//   s1 initFull (assignment of the not-ghost elements, initConstitutiveLaw)
//   s2 Material::addElements of three ghost elements
//   s3 z_rand.setDefaultValue(4.0)               (reset: redraw everything)
//   s4 a_rand.setRandomDistribution(normal ...)  (reset with a new law)
//   s5 NewElementsEvent of three not-ghost elements (model assignment +
//      resizeInternals)
//   s6 Mesh::eraseElements of two not-ghost elements (RemovedElementsEvent,
//      law compaction; the event's new numbering is recorded)
//   s7 NewElementsEvent of one more not-ghost element
//
// After every step: the element filter, the block layout of every internal
// (which (type, ghost) arrays exist and their sizes) and the values of the
// random fields and of the history of m_rand. At the end: the next raw
// values of RandomGenerator<Idx> (total consumption check).
//
// Second case, smm_2d_mixed: one
// resize that grows the not-ghost **and** the ghost blocks at once. A fresh
// model on the same mesh with an `elastic` material first (index 0, which
// the default selector gives every element) and the probe material second;
// the engine is re-seeded with the same seed:
//
//   s1 initFull (the probe material gets no element: zero-size blocks)
//   s2 Material::addElements of every not-ghost and ghost element in one
//      interleaved list (one resizeInternals)
//
// recorded as above, then the next raw engine values.
//
// Output: fixtures/cl/random_internal/{smm_2d,smm_2d_mixed}
// Usage: law_random_probe <fixtures_root> <work_dir>

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_random_generator.hh>
#include <material.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_events.hh>
#include <parser.hh>
#include <random_internal_field.hh>
#include <solid_mechanics_model.hh>

#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;

namespace {

class MaterialRandomProbe : public Material {
public:
  MaterialRandomProbe(SolidMechanicsModel & model, const ID & id)
      : Material(model, id),
        z_rand(this->registerInternal<Real, DefaultRandomInternalField>("z_rand", 1)),
        a_rand(this->registerInternal<Real, DefaultRandomInternalField>("a_rand", 2)),
        m_rand(this->registerInternal<Real, DefaultRandomInternalField>("m_rand", 1)),
        k_coh(this->registerInternal<Real, DefaultRandomInternalField>("k_coh", 1)),
        p_plain(this->registerInternal<Real>("p_plain", 1)) {
    this->registerParam("z_rand", z_rand, _pat_parsmod, "random, 1 component");
    this->registerParam("a_rand", a_rand, _pat_parsmod, "random, 2 components");
    this->registerParam("m_rand", m_rand, _pat_parsmod, "random, history");
    this->registerParam("k_coh", k_coh, _pat_parsmod, "random, cohesive kind");
    this->registerParam("p_plain", p_plain, _pat_parsmod, "plain field");
    m_rand.initializeHistory();
    k_coh.setElementKind(_ek_cohesive);
  }

  void computeStress(ElementType /*type*/, GhostType /*ghost*/) override {}

  RandomInternalField<Real> & z_rand;
  RandomInternalField<Real> & a_rand;
  RandomInternalField<Real> & m_rand;
  RandomInternalField<Real> & k_coh;
  InternalField<Real> & p_plain;
};

class RemovalRecorder : public MeshEventHandler {
public:
  void onElementsRemoved(const Array<Element> & elements_list,
                         const ElementTypeMapArray<Idx> & new_numbering,
                         const RemovedElementsEvent & /*event*/) override {
    removed.clear();
    for (const auto & el : elements_list) {
      removed.push_back(el);
    }
    numbering.clear();
    for (auto gt : ghost_types) {
      for (auto type : new_numbering.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
        const auto & a = new_numbering(type, gt);
        std::vector<std::int64_t> v;
        for (Int i = 0; i < a.size(); ++i) {
          v.push_back(a(i));
        }
        numbering.push_back({type, gt, v});
      }
    }
  }
  struct Block {
    ElementType type;
    GhostType ghost;
    std::vector<std::int64_t> values;
  };
  std::vector<Element> removed;
  std::vector<Block> numbering;
};

std::string tname(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::string gname(GhostType g) { return g == _ghost ? "ghost" : "not_ghost"; }

const std::vector<ElementType> kTypes{_segment_2, _triangle_3, _quadrangle_4,
                                      _cohesive_2d_4};
const std::vector<std::string> kFields{"a_rand", "k_coh", "m_rand", "p_plain", "z_rand"};

std::vector<double> flat(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

void record_step(ap::FixtureWriter & w, ap::Json & steps, const std::string & step,
                 MaterialRandomProbe & mat) {
  ap::Json s = ap::Json::object();
  s["step"] = step;
  ap::Json filters = ap::Json::array();
  const auto & cmat = mat; // the const accessors are the public ones
  for (auto gt : ghost_types) {
    for (auto type : kTypes) {
      if (not cmat.getElementFilter().exists(type, gt)) {
        continue;
      }
      const auto & f = cmat.getElementFilter(type, gt);
      std::vector<long long> v;
      for (Int i = 0; i < f.size(); ++i) {
        v.push_back(f(i));
      }
      ap::Json e = ap::Json::object();
      e["type"] = tname(type);
      e["ghost"] = gname(gt);
      e["elements"] = ap::Json::array_of(v);
      filters.push(std::move(e));
    }
  }
  s["filter"] = filters;
  ap::Json layout = ap::Json::array();
  for (const auto & id : kFields) {
    auto & field = mat.getInternal<Real>(id);
    for (auto gt : ghost_types) {
      for (auto type : kTypes) {
        if (not field.exists(type, gt)) {
          continue;
        }
        const auto & a = field(type, gt);
        ap::Json e = ap::Json::object();
        e["field"] = id;
        e["type"] = tname(type);
        e["ghost"] = gname(gt);
        e["size"] = static_cast<long long>(a.size());
        e["nb_component"] = static_cast<long long>(a.getNbComponent());
        layout.push(std::move(e));
        if (a.size() > 0 and id != "p_plain") {
          w.add(step + "/" + id + "/" + tname(type) + "/" + gname(gt),
                {static_cast<std::size_t>(a.size()),
                 static_cast<std::size_t>(a.getNbComponent())},
                {"quad", "component"}, "quadrature", flat(a))
              .element_type(tname(type))
              .ghost_type(gname(gt))
              .tolerance_class("exact");
        }
      }
    }
    if (field.hasHistory()) {
      const auto & prev = field.previous();
      for (auto gt : ghost_types) {
        for (auto type : kTypes) {
          if (not prev.exists(type, gt)) {
            continue;
          }
          const auto & a = prev(type, gt);
          ap::Json e = ap::Json::object();
          e["field"] = "previous_" + id;
          e["type"] = tname(type);
          e["ghost"] = gname(gt);
          e["size"] = static_cast<long long>(a.size());
          e["nb_component"] = static_cast<long long>(a.getNbComponent());
          layout.push(std::move(e));
        }
      }
    }
  }
  s["layout"] = layout;
  steps.push(std::move(s));
}

/// The probe mesh: 4 + 2 not-ghost triangles/quadrangles, 2 + 1 ghost ones,
/// 2 boundary segments.
void build_mesh(Mesh & mesh) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  const double xy[12][2] = {{0, 0}, {1, 0}, {2, 0}, {0, 1}, {1, 1}, {2, 1},
                            {0, 2}, {1, 2}, {2, 2}, {3, 0}, {3, 1}, {3, 2}};
  nodes.resize(12);
  for (Int n = 0; n < 12; ++n) {
    nodes(n, 0) = xy[n][0];
    nodes(n, 1) = xy[n][1];
  }
  auto push = [](Array<Idx> & conn, std::initializer_list<Idx> c) {
    Vector<Idx> v(static_cast<Int>(c.size()));
    Int k = 0;
    for (auto x : c) {
      v(k++) = x;
    }
    conn.push_back(v);
  };
  auto & tri = acc.getConnectivity(_triangle_3, _not_ghost);
  push(tri, {0, 1, 4});
  push(tri, {0, 4, 3});
  push(tri, {3, 4, 7});
  push(tri, {3, 7, 6});
  auto & quad = acc.getConnectivity(_quadrangle_4, _not_ghost);
  push(quad, {1, 2, 5, 4});
  push(quad, {4, 5, 8, 7});
  auto & gtri = acc.getConnectivity(_triangle_3, _ghost);
  push(gtri, {2, 9, 10});
  push(gtri, {2, 10, 5});
  acc.getGhostsCounters(_triangle_3, _ghost).resize(2, 1);
  auto & gquad = acc.getConnectivity(_quadrangle_4, _ghost);
  push(gquad, {5, 10, 11, 8});
  acc.getGhostsCounters(_quadrangle_4, _ghost).resize(1, 1);
  auto & seg = acc.getConnectivity(_segment_2, _not_ghost);
  push(seg, {0, 1});
  push(seg, {1, 2});
  acc.makeReady();
}

/// Case smm_2d_mixed (see the file header): one resize growing both ghost
/// types. Must run after the first case (it re-reads the static parser).
int mixed_case(const std::filesystem::path & root, const std::filesystem::path & work) {
  const auto dat = work / "law_random_probe_mixed.dat";
  {
    std::ofstream f(dat);
    f << "seed = 1234\n"
         "material elastic [\n"
         "  name = other\n"
         "  rho = 1\n"
         "  E = 1\n"
         "  nu = 0.3\n"
         "]\n"
         "material random_probe [\n"
         "  name = probe\n"
         "  rho = 1\n"
         "  z_rand = 2.5 uniform [-1, 1]\n"
         "  a_rand = 1 weibull [2, 3]\n"
         "  m_rand = 0.5 normal [0, 0.25]\n"
         "  k_coh = 1 uniform [0, 1]\n"
         "  p_plain = 7\n"
         "]\n";
  }
  readInputFile(dat.string());
  const long seed = getStaticParser().getParameter("seed", _ppsc_current_scope);
  RandomGenerator<Idx>::seed(seed);

  Mesh mesh(2);
  build_mesh(mesh);
  ap::FixtureWriter w(root, "cl/random_internal/smm_2d_mixed",
                      "Random internal fields of a probe Material in a SolidMechanicsModel: "
                      "one addElements of interleaved not-ghost and ghost elements into an "
                      "empty material (a single resize growing both ghost types)",
                      "law_random_probe", "1");
  w.add_tag("cl");
  w.add_tag("wp-verify-1");
  w.add_input_file("material", dat);
  ap::Json steps = ap::Json::array();
  try {
    SolidMechanicsModel model(mesh);
    model.initFull(_analysis_method = _static);
    auto & mat = dynamic_cast<MaterialRandomProbe &>(model.getMaterial(1));
    w.parameters()["seed"] = static_cast<long long>(RandomGenerator<Idx>::seed());
    w.parameters()["material_index"] = 1LL;
    record_step(w, steps, "s1", mat);

    Array<Element> mixed;
    mixed.push_back(Element{_triangle_3, 1, _ghost});
    mixed.push_back(Element{_triangle_3, 0, _not_ghost});
    mixed.push_back(Element{_quadrangle_4, 0, _ghost});
    mixed.push_back(Element{_triangle_3, 1, _not_ghost});
    mixed.push_back(Element{_quadrangle_4, 0, _not_ghost});
    mixed.push_back(Element{_triangle_3, 0, _ghost});
    mixed.push_back(Element{_triangle_3, 2, _not_ghost});
    mixed.push_back(Element{_quadrangle_4, 1, _not_ghost});
    mixed.push_back(Element{_triangle_3, 3, _not_ghost});
    {
      ap::Json list = ap::Json::array();
      for (const auto & el : mixed) {
        ap::Json e = ap::Json::object();
        e["type"] = tname(el.type);
        e["element"] = static_cast<long long>(el.element);
        e["ghost"] = gname(el.ghost_type);
        list.push(std::move(e));
      }
      w.parameters()["added"] = list;
    }
    mat.addElements(mixed);
    record_step(w, steps, "s2", mat);

    RandomGenerator<Idx> gen;
    std::vector<std::int64_t> next;
    for (int k = 0; k < 4; ++k) {
      next.push_back(static_cast<std::int64_t>(gen()));
    }
    w.add("next_draws", {next.size()}, {"entry"}, "table", next)
        .description("next raw values of RandomGenerator<Idx> after s2")
        .tolerance_class("exact");
    w.parameters()["steps"] = steps;
    w.finish();
    std::cout << "  cl/random_internal/smm_2d_mixed\n";
  } catch (std::exception & e) {
    std::cerr << "law_random_probe (mixed) failed: " << e.what() << "\n";
    return 1;
  }
  return 0;
}

} // namespace

int main(int argc, char ** argv) {
  if (argc != 3) {
    std::cerr << "usage: law_random_probe <fixtures_root> <work_dir>\n";
    return 2;
  }
  const std::filesystem::path root(argv[1]);
  const std::filesystem::path work(argv[2]);
  std::filesystem::create_directories(work);
  const auto dat = work / "law_random_probe.dat";
  {
    std::ofstream f(dat);
    f << "seed = 1234\n"
         "material random_probe [\n"
         "  name = probe\n"
         "  rho = 1\n"
         "  z_rand = 2.5 uniform [-1, 1]\n"
         "  a_rand = 1 weibull [2, 3]\n"
         "  m_rand = 0.5 normal [0, 0.25]\n"
         "  k_coh = 1 uniform [0, 1]\n"
         "  p_plain = 7\n"
         "]\n";
  }
  unsetenv("AKA_SEED");
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(dat.string(), fake_argc, fake_argv);

  MaterialFactory::getInstance().registerAllocator(
      "random_probe",
      [](Int /*dim*/, const ID & /*option*/, SolidMechanicsModel & model,
         const ID & id) -> std::unique_ptr<Material> {
        return std::make_unique<MaterialRandomProbe>(model, id);
      });

  const Int dim = 2;
  Mesh mesh(dim);
  build_mesh(mesh);

  ap::FixtureWriter w(root, "cl/random_internal/smm_2d",
                      "Random internal fields of a probe Material in a SolidMechanicsModel: "
                      "allocation and draw order on init, ghost addElements, setDefaultValue, "
                      "setRandomDistribution, NewElementsEvent, eraseElements; block layout "
                      "per internal (spatial dimension / element kind restriction)",
                      "law_random_probe", "1");
  w.add_tag("cl");
  w.add_tag("wp-verify-1");
  w.add_input_file("material", dat);
  ap::Json steps = ap::Json::array();
  int status = 0;
  try {
    SolidMechanicsModel model(mesh);
    model.initFull(_analysis_method = _static);
    auto & mat = dynamic_cast<MaterialRandomProbe &>(model.getMaterial(0));
    RemovalRecorder recorder;
    mesh.registerEventHandler(recorder, _ehp_lowest);
    w.parameters()["seed"] = static_cast<long long>(RandomGenerator<Idx>::seed());
    {
      std::vector<long long> nq;
      for (auto type : {_triangle_3, _quadrangle_4}) {
        nq.push_back(model.getFEEngine().getNbIntegrationPoints(type));
      }
      w.parameters()["nb_quadrature_points"] = ap::Json::array_of(nq);
    }

    record_step(w, steps, "s1", mat);

    Array<Element> ghosts;
    ghosts.push_back(Element{_triangle_3, 1, _ghost});
    ghosts.push_back(Element{_quadrangle_4, 0, _ghost});
    ghosts.push_back(Element{_triangle_3, 0, _ghost});
    mat.addElements(ghosts);
    record_step(w, steps, "s2", mat);

    mat.z_rand.setDefaultValue(4.0);
    record_step(w, steps, "s3", mat);

    mat.a_rand.setRandomDistribution(
        RandomParameter<Real>(-0.5, std::normal_distribution<Real>(1.0, 0.125)));
    record_step(w, steps, "s4", mat);

    auto add_elements = [&](const std::vector<std::pair<ElementType, std::vector<Idx>>> & els) {
      MeshAccessor acc(mesh);
      NewElementsEvent event("law_random_probe");
      for (const auto & [type, c] : els) {
        auto & conn = acc.getConnectivity(type, _not_ghost);
        Vector<Idx> v(static_cast<Int>(c.size()));
        for (std::size_t k = 0; k < c.size(); ++k) {
          v(static_cast<Int>(k)) = c[k];
        }
        conn.push_back(v);
        event.getList().push_back(Element{type, conn.size() - 1, _not_ghost});
      }
      mesh.sendEvent(event);
    };
    add_elements({{_triangle_3, {2, 9, 10}}, {_quadrangle_4, {5, 10, 11, 8}},
                  {_triangle_3, {2, 10, 5}}});
    record_step(w, steps, "s5", mat);

    Array<Element> to_erase;
    to_erase.push_back(Element{_triangle_3, 1, _not_ghost});
    to_erase.push_back(Element{_quadrangle_4, 0, _not_ghost});
    mesh.eraseElements(to_erase);
    {
      ap::Json rem = ap::Json::object();
      ap::Json list = ap::Json::array();
      for (const auto & el : recorder.removed) {
        ap::Json e = ap::Json::object();
        e["type"] = tname(el.type);
        e["element"] = static_cast<long long>(el.element);
        e["ghost"] = gname(el.ghost_type);
        list.push(std::move(e));
      }
      rem["removed"] = list;
      ap::Json num = ap::Json::array();
      for (const auto & b : recorder.numbering) {
        ap::Json e = ap::Json::object();
        e["type"] = tname(b.type);
        e["ghost"] = gname(b.ghost);
        std::vector<long long> v(b.values.begin(), b.values.end());
        e["new_numbering"] = ap::Json::array_of(v);
        num.push(std::move(e));
      }
      rem["numbering"] = num;
      w.parameters()["removal"] = rem;
    }
    record_step(w, steps, "s6", mat);

    add_elements({{_triangle_3, {0, 1, 3}}});
    record_step(w, steps, "s7", mat);

    RandomGenerator<Idx> gen;
    std::vector<std::int64_t> next;
    for (int k = 0; k < 4; ++k) {
      next.push_back(static_cast<std::int64_t>(gen()));
    }
    w.add("next_draws", {next.size()}, {"entry"}, "table", next)
        .description("next raw values of RandomGenerator<Idx> after s7")
        .tolerance_class("exact");
    w.parameters()["steps"] = steps;
    w.finish();
    std::cout << "  cl/random_internal/smm_2d\n";
  } catch (std::exception & e) {
    std::cerr << "law_random_probe failed: " << e.what() << "\n";
    status = 1;
  }
  if (status == 0) {
    status = mixed_case(root, work);
  }
  finalize();
  return status;
}
