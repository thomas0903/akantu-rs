// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): material-point driver. Replays a prescribed displacement-gradient
// history on a one-element model (one integration point: segment_2,
// triangle_3 or tetrahedron_4) and records, per step, what the material law
// of Akantu (src/model/solid_mechanics/material*.{hh,cc}, materials/) gives:
// the gradient actually seen by the law, stress, tangent moduli (when
// implemented), potential energy and the requested internal variables.
//
// Usage:
//   material_probe <fixtures_root> <case_id> <dim> <material.dat> <grad_u.npy>
//                  [--internal <name>]... [--description <text>] [--wave-speeds]
//                  [--time-steps <dt.npy>] [--internal-init <name> <v.npy>]...
//                  [--internal-set <name> <v.npy>]... [--energy <id>]...
//                  [--post-internal <name>]... [--post-energy <id>]...
//                  [--repeat-stress <n>] [--steady-state-at <k>] [--rho]
//                  [--matrix-changed] [--factory-option <opt>]
//                  [--oracle-variant <patch> <sha256> <defect>]
// grad_u.npy: f64 [step, dim, dim], H_k such that u(X) = H_k X.
// Version-4 options (absent options leave the version-3 data
// unchanged):
//   --time-steps v.npy       f64 [step]: model.setTimeStep(dt_k) before step k
//   --internal-init n v.npy  f64 [ncomp]: internal n of every quadrature point
//                            set to v once, after initFull (initial state)
//   --internal-set n v.npy   f64 [step, ncomp]: internal n set to v_k at step k
//                            after beforeSolveStep, before the stresses (e.g.
//                            the phase-field damage written by the coupler)
//   --energy id              getEnergy(id) after the potential energy
//   --post-internal n        internal n recorded after afterSolveStep(true)
//   --post-energy id         getEnergy(id) recorded after afterSolveStep(true)
//   --repeat-stress n        computeAllStresses n times per step (repeated
//                            residual evaluations of one Newton step)
//   --steady-state-at k      setToSteadyState() after the stresses of step k
//   --rho                    getRho of element 0 (per quadrature point)
//   --matrix-changed         hasMatrixChanged("K") / ("M") after the stresses
//                            (before the tangent), as 0/1
// Version-5 options (absent options leave the version-4 data
// unchanged):
//   --factory-option opt     instantiate the single material section of the
//                            input file through the C++ factory with the
//                            allocator option `opt` (an input file cannot
//                            spell options such as `mazars-drucker-prager`:
//                            section options are identifiers):
//                            registerNewMaterial(name, type, opt) after
//                            initFull, parseSection, initConstitutiveLaws
//                            (the route of upstream's Python initFull)
//   --oracle-variant p s d   label a golden of an oracle VARIANT build (patch
//                            path, its sha256, defect id): written to
//                            parameters.oracle_variant and the description;
//                            repeated, the further patches of the same
//                            variant build go to
//                            parameters.oracle_variant.additional_patches
// The case catalogue (materials, parameters, strain paths) lives in
// oracle/probes/python/material_cases.py.
//
// Step semantics (mirrors a converged static step of SolidMechanicsModel):
//   material.beforeSolveStep()      -> save previous state of internals
//   u_i = H_k X_i at every node
//   material.computeAllStresses()   -> grad_u from u, then the law
//   (record)
//   material.afterSolveStep(true)
//   (record --post-internal / --post-energy)

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <mesh.hh>
#include <parser.hh>
#include <solid_mechanics_model.hh>

#include <array>
#include <cstdint>
#include <typeinfo>
#include <iostream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;

namespace {

constexpr const char * kProbe = "material_probe";
// 2: explicit Voigt order of `tangent`, component labels of tensor internals
//    (metadata only; array contents unchanged).
// 3: optional `--wave-speeds`: per step `celerity` (getCelerity),
//    `wave_speed/push`, `wave_speed/shear` (each omitted, with the exception
//    message in the parameters, when the law throws). Cases without the flag
//    produce exactly the version-2 data.
// 4: optional time steps, initial / prescribed internals, extra energies,
//    post-step records, repeated stress evaluations, steady state, density,
//    matrix-changed flags. Cases without these options produce
//    exactly the version-3 data.
// 5: optional `--factory-option` (C++-factory instantiation with an
//    allocator option) and `--oracle-variant` (variant label); Eigen's cache
//    sizes pinned at start-up (D-050) and recorded in the parameters.
//    Cases without the new options produce exactly the
//    version-4 data (material laws use fixed-size Eigen kernels only).
//    `--oracle-variant` may be repeated (additional_patches);
//    manifest label only, no data change.
constexpr const char * kProbeVersion = "5";

/// Exposes the protected virtual Material::computeTangentModuli through a
/// pointer to member (the call still dispatches virtually).
struct TangentAccess : public Material {
  using Material::computeTangentModuli;
};

std::string one_element_msh(Int dim) {
  std::string nodes, elem;
  switch (dim) {
  case 1:
    nodes = "2\n1 0 0 0\n2 1.3 0 0\n";
    elem = "1 1 2 1 1 1 2\n";
    break;
  case 2:
    nodes = "3\n1 0 0 0\n2 1.1 0.2 0\n3 0.3 0.9 0\n";
    elem = "1 2 2 1 1 1 2 3\n";
    break;
  default:
    nodes = "4\n1 0 0 0\n2 1.1 0.1 0.05\n3 0.2 0.9 0.1\n4 0.1 0.2 1.2\n";
    elem = "1 4 2 1 1 1 2 3 4\n";
    break;
  }
  return "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n" + nodes +
         "$EndNodes\n$Elements\n1\n" + elem + "$EndElements\n";
}

ElementType simplex(Int dim) {
  return dim == 1 ? _segment_2 : (dim == 2 ? _triangle_3 : _tetrahedron_4);
}

/// Quantity-name form of an internal name (fixture names carry no
/// whitespace; upstream names such as "damage dissipated energy" do).
std::string quantity_name(std::string name) {
  for (auto & c : name) {
    if (c == ' ') {
      c = '_';
    }
  }
  return name;
}

} // namespace

int main(int argc, char ** argv) {
  if (argc < 6) {
    std::cerr << "usage: " << argv[0]
              << " <fixtures_root> <case_id> <dim> <material.dat> <grad_u.npy>"
                 " [--internal name]... [--description text]\n";
    return 2;
  }
  const std::filesystem::path root = argv[1];
  const std::string case_id = argv[2];
  const Int dim = std::stoi(argv[3]);
  const std::filesystem::path material_file = argv[4];
  const std::filesystem::path history_file = argv[5];
  std::vector<std::string> internals;
  std::string description = "material point history";
  bool wave_speeds = false;
  // version-4 options
  std::filesystem::path time_steps_file;
  std::vector<std::pair<std::string, std::filesystem::path>> internal_init, internal_set;
  std::vector<std::string> energies, post_internals, post_energies;
  long repeat_stress = 1;
  long steady_state_at = -1;
  bool record_rho = false;
  bool matrix_changed = false;
  // version-5 options
  std::string factory_option;
  std::string variant_patch, variant_sha256, variant_defect;
  // further --oracle-variant labels: patches of the same variant build
  // (the UD-031 variant also carries the UD-814 patch)
  std::vector<std::array<std::string, 3>> variant_extra;
  for (int i = 6; i < argc; ++i) {
    std::string flag = argv[i];
    if (flag == "--wave-speeds") {
      wave_speeds = true;
      continue;
    }
    if (flag == "--rho") {
      record_rho = true;
      continue;
    }
    if (flag == "--matrix-changed") {
      matrix_changed = true;
      continue;
    }
    if (i + 1 >= argc) {
      std::cerr << "option " << flag << " needs a value\n";
      return 2;
    }
    if (flag == "--internal") {
      internals.emplace_back(argv[++i]);
    } else if (flag == "--description") {
      description = argv[++i];
    } else if (flag == "--time-steps") {
      time_steps_file = argv[++i];
    } else if (flag == "--energy") {
      energies.emplace_back(argv[++i]);
    } else if (flag == "--post-internal") {
      post_internals.emplace_back(argv[++i]);
    } else if (flag == "--post-energy") {
      post_energies.emplace_back(argv[++i]);
    } else if (flag == "--repeat-stress") {
      repeat_stress = std::stol(argv[++i]);
    } else if (flag == "--steady-state-at") {
      steady_state_at = std::stol(argv[++i]);
    } else if (flag == "--factory-option") {
      factory_option = argv[++i];
    } else if (flag == "--oracle-variant") {
      if (i + 3 >= argc) {
        std::cerr << "option --oracle-variant needs a patch, a sha256 and a defect id\n";
        return 2;
      }
      if (variant_patch.empty()) {
        variant_patch = argv[++i];
        variant_sha256 = argv[++i];
        variant_defect = argv[++i];
      } else {
        std::array<std::string, 3> extra{argv[i + 1], argv[i + 2], argv[i + 3]};
        variant_extra.push_back(extra);
        i += 3;
      }
    } else if (flag == "--internal-init" or flag == "--internal-set") {
      if (i + 2 >= argc) {
        std::cerr << "option " << flag << " needs a name and a file\n";
        return 2;
      }
      auto & target = flag == "--internal-init" ? internal_init : internal_set;
      std::string name = argv[++i];
      target.emplace_back(name, std::filesystem::path(argv[++i]));
    } else {
      std::cerr << "unknown option " << flag << "\n";
      return 2;
    }
  }
  if (repeat_stress < 1) {
    std::cerr << "--repeat-stress needs n >= 1\n";
    return 2;
  }

  std::vector<std::size_t> hshape;
  auto H = ap::read_npy_f64(history_file, hshape);
  if (hshape.size() != 3 or hshape[1] != static_cast<std::size_t>(dim) or
      hshape[2] != static_cast<std::size_t>(dim)) {
    std::cerr << "grad_u.npy must be [step, dim, dim]\n";
    return 2;
  }
  const auto nsteps = hshape[0];
  std::vector<double> time_steps;
  if (not time_steps_file.empty()) {
    std::vector<std::size_t> tshape;
    time_steps = ap::read_npy_f64(time_steps_file, tshape);
    if (tshape.size() != 1 or tshape[0] != nsteps) {
      std::cerr << "time steps must be [step] with one entry per step\n";
      return 2;
    }
  }
  struct InternalInput {
    std::string name;
    std::filesystem::path file;
    std::vector<std::size_t> shape;
    std::vector<double> values;
  };
  std::vector<InternalInput> inits, sets;
  for (auto & [name, file] : internal_init) {
    InternalInput in{name, file, {}, {}};
    in.values = ap::read_npy_f64(file, in.shape);
    if (in.shape.size() != 1) {
      std::cerr << "--internal-init " << name << ": values must be [ncomp]\n";
      return 2;
    }
    inits.push_back(std::move(in));
  }
  for (auto & [name, file] : internal_set) {
    InternalInput in{name, file, {}, {}};
    in.values = ap::read_npy_f64(file, in.shape);
    if (in.shape.size() != 2 or in.shape[0] != nsteps) {
      std::cerr << "--internal-set " << name << ": values must be [step, ncomp]\n";
      return 2;
    }
    sets.push_back(std::move(in));
  }

  if (not variant_patch.empty()) {
    std::string patches = variant_patch;
    for (auto && extra : variant_extra) {
      patches += " + " + extra[0] + ", " + extra[2];
    }
    description = variant_defect + " zero-initialisation variant of the oracle (" +
                  patches + "): " + description;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  // Factory mode: the input file is parsed into a local parser only, so
  // initFull instantiates no law (the global parser has no parsed file).
  Parser factory_parser;
  if (factory_option.empty()) {
    initialize(material_file.string(), fake_argc, fake_argv);
  } else {
    initialize(fake_argc, fake_argv);
    factory_parser.parse(material_file.string());
  }

  // D-050: Eigen's cache sizes pinned to its own defaults (L1 16 KiB, L2 = L3
  // 512 KiB) on every host, so an install built without the compile-time pin
  // (EIGEN_NO_CPUID) gives the same blocking as the pinned x86_64 and the
  // aarch64 oracles (no effect on those). The material laws use fixed-size
  // kernels only; the pin is recorded for provenance.
  std::cerr << "[material_probe] eigen cache sizes queried l1=" << Eigen::l1CacheSize()
            << " l2=" << Eigen::l2CacheSize() << " l3=" << Eigen::l3CacheSize() << "\n";
  Eigen::setCpuCacheSizes(16384, 524288, 524288);

  ap::FixtureWriter w(root, case_id, description, kProbe, kProbeVersion);
  w.add_tag("material");
  const auto tmp_mesh = w.dir() / ("one_element_" + std::to_string(dim) + "d.msh");
  ap::write_file(tmp_mesh, one_element_msh(dim));

  int status = 0;
  {
    Mesh mesh(dim);
    mesh.read(tmp_mesh.string());
    SolidMechanicsModel model(mesh);
    model.initFull(_analysis_method = _static);
    if (not factory_option.empty()) {
      auto sections = factory_parser.getSubSections(ParserType::_material);
      std::vector<const ParserSection *> found;
      for (auto && section : sections) {
        found.push_back(&section);
      }
      if (found.size() != 1) {
        std::cerr << "--factory-option needs exactly one material section\n";
        return 2;
      }
      const auto & section = *found.front();
      const std::string name = section.getParameter("name");
      model.registerNewMaterial(name, section.getName(), factory_option);
      model.getMaterial(name).parseSection(section);
      model.initConstitutiveLaws();
    }
    auto & mat = model.getMaterial(0);
    const auto type = simplex(dim);
    const auto & X = mesh.getNodes();
    auto & u = model.getDisplacement();

    w.add_input_file("mesh", tmp_mesh, "generated:material_probe one-element mesh");
    w.add_input_file("material", material_file, "generated:material_cases.py");
    w.add_input_file("grad_u", history_file, "generated:material_cases.py");
    if (not time_steps_file.empty()) {
      w.add_input_file("time_steps", time_steps_file, "generated:material_cases.py");
    }
    for (auto & in : inits) {
      w.add_input_file("internal_init/" + quantity_name(in.name), in.file,
                       "generated:material_cases.py");
    }
    for (auto & in : sets) {
      w.add_input_file("internal_set/" + quantity_name(in.name), in.file,
                       "generated:material_cases.py");
    }
    // Writes `values` (one tuple) into every quadrature point of the
    // internal `name` (all element types and ghost types of the law).
    auto write_internal = [&](const std::string & name, const double * values,
                              std::size_t ncomp) {
      if (not mat.isInternal<Real>(name, _ek_regular)) {
        throw std::runtime_error("unknown internal " + name);
      }
      auto & field = mat.getInternal<Real>(name);
      for (auto ghost : ghost_types) {
        for (auto && t : field.elementTypes(dim, ghost)) {
          auto & a = field(t, ghost);
          if (static_cast<std::size_t>(a.getNbComponent()) != ncomp) {
            throw std::runtime_error("internal " + name + ": component count mismatch");
          }
          for (Idx q = 0; q < a.size(); ++q) {
            for (Int c = 0; c < a.getNbComponent(); ++c) {
              a(q, c) = values[c];
            }
          }
        }
      }
    };
    for (auto & in : inits) {
      write_internal(in.name, in.values.data(), in.shape[0]);
    }
    auto & p = w.parameters();
    p["spatial_dimension"] = static_cast<long long>(dim);
    p["element_type"] = [&] {
      std::stringstream ss;
      ss << type;
      return ss.str();
    }();
    p["material_name"] = mat.getName();
    p["material_id"] = mat.getID();
    p["eigen_cache_sizes"] = std::to_string(Eigen::l1CacheSize()) + "," +
                             std::to_string(Eigen::l2CacheSize()) + "," +
                             std::to_string(Eigen::l3CacheSize()) +
                             " (Eigen::setCpuCacheSizes in the probe, D-050)";
    if (not factory_option.empty()) {
      p["factory_option"] = factory_option;
    }
    if (not variant_patch.empty()) {
      ap::Json v = ap::Json::object();
      v["patch"] = variant_patch;
      v["sha256"] = variant_sha256;
      v["defect"] = variant_defect;
      if (not variant_extra.empty()) {
        ap::Json extras = ap::Json::array();
        for (auto && extra : variant_extra) {
          ap::Json e = ap::Json::object();
          e["patch"] = extra[0];
          e["sha256"] = extra[1];
          e["defect"] = extra[2];
          extras.push(e);
        }
        v["additional_patches"] = extras;
      }
      p["oracle_variant"] = v;
    }
    p["nb_steps"] = static_cast<long long>(nsteps);

    const Int voigt = dim * (dim + 1) / 2;
    std::vector<double> grad_u, stress, tangent, epot;
    std::vector<std::vector<double>> extra_energy(energies.size());
    std::vector<std::vector<double>> post_energy(post_energies.size());
    std::vector<std::vector<double>> post_values(post_internals.size());
    std::vector<Int> post_ncomp(post_internals.size(), -1);
    std::vector<double> rho_values, changed_k, changed_m;
    Int rho_nquad = 0;
    std::vector<std::vector<double>> internal_values(internals.size());
    std::vector<Int> internal_ncomp(internals.size(), -1);
    bool has_tangent = true;
    bool has_energy = true;
    std::string tangent_error, energy_error;
    // --wave-speeds: celerity, push and shear wave speeds of element 0
    const char * speed_names[] = {"celerity", "wave_speed/push", "wave_speed/shear"};
    std::vector<double> speeds[3];
    bool has_speed[3] = {wave_speeds, wave_speeds, wave_speeds};
    std::string speed_error[3];
    const auto tangent_fn = &TangentAccess::computeTangentModuli;

    // An exception thrown by the law (e.g. NotImplementedException) is part
    // of the oracle behaviour: it is recorded as an error-path fixture.
    std::string error_kind, error_message;
    std::size_t error_step = nsteps;
    for (std::size_t k = 0; k < nsteps; ++k) {
     try {
      mat.beforeSolveStep();
      if (not time_steps.empty()) {
        model.setTimeStep(time_steps[k]);
      }
      for (auto & in : sets) {
        write_internal(in.name, in.values.data() + k * in.shape[1], in.shape[1]);
      }
      for (Idx n = 0; n < X.size(); ++n) {
        for (Int r = 0; r < dim; ++r) {
          double v = 0.;
          for (Int c = 0; c < dim; ++c) {
            v += H[(k * dim + r) * dim + c] * X(n, c);
          }
          u(n, r) = v;
        }
      }
      for (long r = 0; r < repeat_stress; ++r) {
        mat.computeAllStresses(_not_ghost);
      }
      if (steady_state_at == static_cast<long>(k)) {
        mat.setToSteadyState(_not_ghost);
      }
      if (matrix_changed) {
        changed_k.push_back(mat.hasMatrixChanged("K") ? 1. : 0.);
        changed_m.push_back(mat.hasMatrixChanged("M") ? 1. : 0.);
      }

      // Keep views and iterators alive: `*it` returns a proxy owned by the
      // iterator (src/common/aka_view_iterators.hh), so binding it from a
      // temporary iterator dangles at the semicolon.
      auto g_view = make_view(mat.getGradU(type, _not_ghost), dim, dim);
      auto s_view = make_view(mat.getStress(type, _not_ghost), dim, dim);
      auto g_it = g_view.begin();
      auto s_it = s_view.begin();
      auto && G = *g_it;
      auto && S = *s_it;
      for (Int r = 0; r < dim; ++r) {
        for (Int c = 0; c < dim; ++c) {
          grad_u.push_back(G(r, c));
          stress.push_back(S(r, c));
        }
      }
      if (has_tangent) {
        try {
          Array<Real> T(1, voigt * voigt);
          T.zero();
          (mat.*tangent_fn)(type, T, _not_ghost);
          auto t_view = make_view(T, voigt, voigt);
          auto t_it = t_view.begin();
          auto && Tm = *t_it;
          for (Int r = 0; r < voigt; ++r) {
            for (Int c = 0; c < voigt; ++c) {
              tangent.push_back(Tm(r, c));
            }
          }
        } catch (std::exception & e) {
          has_tangent = false;
          tangent_error = e.what();
          tangent.clear();
        }
      }
      if (has_energy) {
        try {
          epot.push_back(mat.getEnergy("potential"));
        } catch (std::exception & e) {
          has_energy = false;
          energy_error = e.what();
          epot.clear();
        }
      }
      for (std::size_t i = 0; i < energies.size(); ++i) {
        extra_energy[i].push_back(mat.getEnergy(energies[i]));
      }
      if (record_rho) {
        const Element el{type, 0, _not_ghost};
        const auto nq = model.getFEEngine().getNbIntegrationPoints(type);
        Vector<Real> rhos(nq);
        mat.getRho(rhos, el);
        rho_nquad = nq;
        for (Int q = 0; q < nq; ++q) {
          rho_values.push_back(rhos(q));
        }
      }
      for (int q = 0; q < 3; ++q) {
        if (not has_speed[q]) {
          continue;
        }
        try {
          const Element el{type, 0, _not_ghost};
          speeds[q].push_back(q == 0   ? mat.getCelerity(el)
                              : q == 1 ? mat.getPushWaveSpeed(el)
                                       : mat.getShearWaveSpeed(el));
        } catch (std::exception & e) {
          has_speed[q] = false;
          speed_error[q] = e.what();
          speeds[q].clear();
        }
      }
      for (std::size_t i = 0; i < internals.size(); ++i) {
        if (not mat.isInternal<Real>(internals[i], _ek_regular)) {
          continue;
        }
        const auto & a = mat.getInternal<Real>(internals[i])(type, _not_ghost);
        internal_ncomp[i] = a.getNbComponent();
        for (Int c = 0; c < a.getNbComponent(); ++c) {
          internal_values[i].push_back(a(0, c));
        }
      }
      mat.afterSolveStep(true);
      for (std::size_t i = 0; i < post_internals.size(); ++i) {
        if (not mat.isInternal<Real>(post_internals[i], _ek_regular)) {
          continue;
        }
        const auto & a = mat.getInternal<Real>(post_internals[i])(type, _not_ghost);
        post_ncomp[i] = a.getNbComponent();
        for (Int c = 0; c < a.getNbComponent(); ++c) {
          post_values[i].push_back(a(0, c));
        }
      }
      for (std::size_t i = 0; i < post_energies.size(); ++i) {
        post_energy[i].push_back(mat.getEnergy(post_energies[i]));
      }
     } catch (std::exception & e) {
      error_kind = debug::demangle(typeid(e).name());
      error_message = e.what();
      error_step = k;
      break;
     }
    }
    const auto nrec = error_step; // completed steps
    if (error_step < nsteps) {
      w.expect_error(error_kind, error_message.substr(0, 120));
      p["error_step"] = static_cast<long long>(error_step);
      // keep only fully recorded steps
      const auto d2 = static_cast<std::size_t>(dim * dim);
      grad_u.resize(nrec * d2);
      stress.resize(nrec * d2);
      const auto v2 = static_cast<std::size_t>(voigt * voigt);
      if (tangent.size() > nrec * v2) {
        tangent.resize(nrec * v2);
      }
      if (epot.size() > nrec) {
        epot.resize(nrec);
      }
      for (auto & sp : speeds) {
        if (sp.size() > nrec) {
          sp.resize(nrec);
        }
      }
      for (std::size_t i = 0; i < internals.size(); ++i) {
        if (internal_ncomp[i] > 0) {
          internal_values[i].resize(nrec * static_cast<std::size_t>(internal_ncomp[i]));
        }
      }
      for (auto & e : extra_energy) {
        e.resize(std::min(e.size(), nrec));
      }
      for (auto & e : post_energy) {
        e.resize(std::min(e.size(), nrec));
      }
      for (std::size_t i = 0; i < post_internals.size(); ++i) {
        if (post_ncomp[i] > 0) {
          post_values[i].resize(
              std::min(post_values[i].size(), nrec * static_cast<std::size_t>(post_ncomp[i])));
        }
      }
      if (rho_nquad > 0) {
        rho_values.resize(std::min(rho_values.size(), nrec * static_cast<std::size_t>(rho_nquad)));
      }
      changed_k.resize(std::min(changed_k.size(), nrec));
      changed_m.resize(std::min(changed_m.size(), nrec));
    }

    std::vector<long long> steps(nrec);
    for (std::size_t k = 0; k < nrec; ++k) {
      steps[k] = static_cast<long long>(k);
    }
    w.set_steps(steps);
    const auto ns = nrec;
    const auto d = static_cast<std::size_t>(dim);
    w.add("input/grad_u_prescribed", {nsteps, d, d}, {"sample", "row", "col"}, "table", H)
        .description("input: prescribed H_k (u = H_k X) for every requested step")
        .tolerance_class("exact");
    w.add("grad_u", {ns, d, d}, {"step", "row", "col"}, "table", grad_u)
        .description("displacement gradient stored by the material after the step "
                     "(computeAllStresses: FE gradient of u = H X; plastic_mazars stores "
                     "(grad_u - eps_p) + eps_p)")
        .tolerance_class("kernel");
    w.add("stress", {ns, d, d}, {"step", "row", "col"}, "table", stress)
        .description("Cauchy/first-PK stress as stored by the material (getStress)")
        .tolerance_class("kernel");
    // Akantu's Voigt order (details::VoigtHelperData<dim>::vec in
    // src/common/aka_voigthelper.hh): engineering shear strains (factor 2).
    const std::vector<std::vector<std::string>> voigt_order{
        {"xx"}, {"xx", "yy", "xy"}, {"xx", "yy", "zz", "yz", "xz", "xy"}};
    const auto & vlabels = voigt_order[static_cast<std::size_t>(dim - 1)];
    p["voigt_order"] = ap::Json::array_of(vlabels);
    if (has_tangent) {
      const auto v = static_cast<std::size_t>(voigt);
      w.add("tangent", {ns, v, v}, {"step", "row", "col"}, "table", tangent)
          .description("computeTangentModuli: d sigma_row / d epsilon_col in Akantu Voigt "
                       "order (parameters.voigt_order; 1D xx, 2D xx yy xy, 3D xx yy zz "
                       "yz xz xy; shear components act on engineering strains 2 eps_ij); "
                       "stored [row, col] row-major")
          .tolerance_class("kernel");
    } else {
      p["tangent_unavailable"] = tangent_error;
    }
    if (has_energy) {
      w.add("energy/potential", {ns}, {"step"}, "scalar", epot)
          .tolerance_class("kernel");
    } else {
      p["energy_unavailable"] = energy_error;
    }
    for (int q = 0; q < 3 and wave_speeds; ++q) {
      if (has_speed[q]) {
        w.add(speed_names[q], {ns}, {"step"}, "scalar", speeds[q])
            .description("wave speed of element 0 after the step (getCelerity / "
                         "getPushWaveSpeed / getShearWaveSpeed)")
            .tolerance_class("kernel");
      } else {
        std::string key = speed_names[q];
        for (auto & c : key) {
          if (c == '/') {
            c = '_';
          }
        }
        p[key + "_unavailable"] = speed_error[q].substr(0, 200);
      }
    }
    if (not time_steps.empty()) {
      w.add("input/time_step", {nsteps}, {"step"}, "scalar", time_steps)
          .description("input: model.setTimeStep(dt_k) before step k")
          .tolerance_class("exact");
    }
    for (auto & in : inits) {
      w.add("input/internal_init/" + quantity_name(in.name), {in.shape[0]}, {"component"},
            "table", in.values)
          .description("input: initial value of the internal at every quadrature point")
          .tolerance_class("exact");
    }
    for (auto & in : sets) {
      w.add("input/internal_set/" + quantity_name(in.name), {in.shape[0], in.shape[1]},
            {"step", "component"}, "table", in.values)
          .description("input: value of the internal written before the stresses of step k")
          .tolerance_class("exact");
    }
    if (repeat_stress != 1) {
      p["repeat_stress"] = static_cast<long long>(repeat_stress);
    }
    {
      // quantity name -> upstream internal name, where they differ
      ap::Json names = ap::Json::object();
      bool any = false;
      std::vector<std::string> all = internals;
      all.insert(all.end(), post_internals.begin(), post_internals.end());
      for (auto & in : inits) {
        all.push_back(in.name);
      }
      for (auto & in : sets) {
        all.push_back(in.name);
      }
      for (auto & n : all) {
        if (quantity_name(n) != n) {
          names[quantity_name(n)] = n;
          any = true;
        }
      }
      if (any) {
        p["internal_names"] = names;
      }
    }
    if (steady_state_at >= 0) {
      p["steady_state_at"] = static_cast<long long>(steady_state_at);
    }
    for (std::size_t i = 0; i < energies.size(); ++i) {
      w.add("energy/" + energies[i], {extra_energy[i].size()}, {"step"}, "scalar",
            extra_energy[i])
          .description("getEnergy(id) after the stresses of the step (before afterSolveStep)")
          .tolerance_class("kernel");
    }
    for (std::size_t i = 0; i < post_energies.size(); ++i) {
      w.add("post_step/energy/" + post_energies[i], {post_energy[i].size()}, {"step"},
            "scalar", post_energy[i])
          .description("getEnergy(id) after afterSolveStep(true)")
          .tolerance_class("kernel");
    }
    ap::Json missing_post = ap::Json::array();
    std::size_t nb_missing_post = 0;
    for (std::size_t i = 0; i < post_internals.size(); ++i) {
      if (post_ncomp[i] < 0) {
        missing_post.push(post_internals[i]);
        ++nb_missing_post;
        continue;
      }
      w.add("post_step/internal/" + quantity_name(post_internals[i]),
            {ns, static_cast<std::size_t>(post_ncomp[i])}, {"step", "component"}, "table",
            post_values[i])
          .description("internal variable at the integration point after afterSolveStep(true), "
                       "raw Akantu component order")
          .tolerance_class("kernel");
    }
    if (nb_missing_post > 0) {
      p["missing_post_internals"] = missing_post;
    }
    if (record_rho) {
      w.add("rho", {ns, static_cast<std::size_t>(rho_nquad)}, {"step", "component"}, "table",
            rho_values)
          .description("getRho(rhos, element 0) per quadrature point after the stresses")
          .tolerance_class("kernel");
    }
    if (matrix_changed) {
      w.add("matrix_changed/K", {ns}, {"step"}, "scalar", changed_k)
          .description("hasMatrixChanged(\"K\") after the stresses, before the tangent (1 = true)")
          .tolerance_class("exact");
      w.add("matrix_changed/M", {ns}, {"step"}, "scalar", changed_m)
          .description("hasMatrixChanged(\"M\") after the stresses (1 = true)")
          .tolerance_class("exact");
    }
    ap::Json missing = ap::Json::array();
    for (std::size_t i = 0; i < internals.size(); ++i) {
      if (internal_ncomp[i] < 0) {
        missing.push(internals[i]);
        continue;
      }
      auto q = w.add("internal/" + quantity_name(internals[i]),
                       {ns, static_cast<std::size_t>(internal_ncomp[i])},
                       {"step", "component"}, "table", internal_values[i])
                     .description("internal variable at the integration point after the "
                                  "step, raw Akantu component order (a dim x dim tensor "
                                  "is column-major: component r + c*dim is M(r, c))")
                     .tolerance_class("kernel");
      if (dim > 1 and internal_ncomp[i] == dim * dim) {
        // Labels "rc" of M(r, c) in storage (column-major) order (in 1D a
        // tensor and a scalar internal are indistinguishable: no labels).
        const char axes[] = {'x', 'y', 'z'};
        std::vector<std::string> labels;
        for (Int c = 0; c < dim; ++c) {
          for (Int r = 0; r < dim; ++r) {
            labels.push_back(std::string{axes[r], axes[c]});
          }
        }
        q.components(labels);
      }
    }
    if (not missing.is_null()) {
      p["missing_internals"] = missing;
    }
    w.finish();
    std::filesystem::remove(tmp_mesh);
    std::cout << "  " << case_id << "\n";
  }
  finalize();
  return status;
}
