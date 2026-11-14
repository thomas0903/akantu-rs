// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): SparseMatrixAIJ assembly through DOFManagerDefault
// (addToProfile + addValues for every symmetry combination), pre-assembled
// terms, matVecMul, applyBoundary, copyProfile/add(B, alpha) as in the
// integration schemes, Matrix Market text (saveMatrix, saveProfile,
// saveVector, std::ostream formatting), release counters, and
// SparseSolverEigen / Eigen::SparseLU solutions together with the internal
// factor arrays of Eigen's SparseLU (read through a subclass exposing its
// protected members, for step-by-step comparison of the Rust translation).
//
// Usage: solver_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Writes the cases solver/aij/*, solver/lu/*, solver/text/* (fixture format
// v1, docs/NUMERICAL_VALIDATION.md).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <dof_manager_default.hh>
#include <mesh.hh>
#include <solver_vector_default.hh>
#include <sparse_matrix_aij.hh>
#include <sparse_solver_eigen.hh>
#include <terms_to_assemble.hh>

#include <Eigen/Sparse>
#include <Eigen/SparseLU>

#include <cmath>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "solver_probe";
constexpr const char * kProbeVersion = "1";

using I64 = std::int64_t;
using SpMat = Eigen::SparseMatrix<double>;

/// Exposes the protected state of Eigen::SparseLU (read-only use).
struct LuSpy : public Eigen::SparseLU<SpMat> {
  const auto & glu() const { return this->m_glu; }
  const auto & perm_c() const { return this->m_perm_c; }
  const auto & perm_r() const { return this->m_perm_r; }
  const auto & etree() const { return this->m_etree; }
};

/// Deterministic 64-bit LCG (Knuth MMIX constants), uniform in [0, 1).
struct Lcg {
  std::uint64_t s;
  explicit Lcg(std::uint64_t seed) : s(seed) {}
  double next() {
    s = s * 6364136223846793005ULL + 1442695040888963407ULL;
    return static_cast<double>(s >> 11) * (1.0 / 9007199254740992.0);
  }
};

template <class V> std::vector<I64> to_i64(const V & v, Int n) {
  std::vector<I64> out(n);
  for (Int i = 0; i < n; ++i) {
    out[i] = static_cast<I64>(v(i));
  }
  return out;
}

void add_vec(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
             const std::string & axis, const std::string & desc) {
  w.add(name, {v.size()}, {axis}, axis == "dof" ? "dof" : "table", v).description(desc);
}

void add_ivec(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
              const std::string & axis, const std::string & desc) {
  w.add(name, {v.size()}, {axis}, "table", v).description(desc);
}

void add_scalar_i(ap::FixtureWriter & w, const std::string & name, I64 v, const std::string & desc) {
  std::vector<I64> one{v};
  w.add(name, {1}, {"entry"}, "table", one).description(desc);
}

/// How much of an AIJ matrix is stored.
enum class Coo {
  Full,       ///< rows, cols (1-based i64) and values
  ValuesOnly, ///< values; coordinates as SHA-256 parameter
  Releases,   ///< releases and size only; everything else as SHA-256
};

/// SHA-256 of the concatenation of `parts` (each an array of little-endian
/// 8-byte words: i64 rows, i64 cols, f64 values).
std::string coo_digest(const std::vector<I64> & a, const std::vector<I64> & b) {
  std::string bytes(reinterpret_cast<const char *>(a.data()), a.size() * sizeof(I64));
  bytes.append(reinterpret_cast<const char *>(b.data()), b.size() * sizeof(I64));
  return ap::Sha256::of(bytes);
}

/// COO of an AIJ matrix (1-based, as getIRN/getJCN) and its releases.
void record_aij(ap::FixtureWriter & w, const std::string & prefix, const SparseMatrixAIJ & A,
                Coo mode = Coo::Full) {
  const auto nnz = A.getNbNonZero();
  std::vector<I64> rows(nnz), cols(nnz);
  std::vector<double> vals(nnz);
  for (Int k = 0; k < nnz; ++k) {
    rows[k] = A.getIRN()(k);
    cols[k] = A.getJCN()(k);
    vals[k] = A.getA()(k);
  }
  auto & p = w.parameters();
  p[prefix + "/nnz"] = static_cast<long long>(nnz);
  p[prefix + "/coords_sha256"] = coo_digest(rows, cols);
  if (mode == Coo::Full) {
    w.add(prefix + "/rows", {rows.size()}, {"nnz"}, "matrix", rows)
        .description("SparseMatrixAIJ::getIRN (1-based global rows, storage order)");
    w.add(prefix + "/cols", {cols.size()}, {"nnz"}, "matrix", cols)
        .description("SparseMatrixAIJ::getJCN (1-based global columns, storage order)");
  }
  if (mode != Coo::Releases) {
    w.add(prefix + "/values", {vals.size()}, {"nnz"}, "matrix", vals)
        .description("SparseMatrixAIJ::getA (storage order)");
  } else {
    // default NaNs canonicalised (D-046): the same bytes
    // as the raw little-endian values whenever no NaN is present
    ap::Sha256 hv;
    ap::update_doubles_d046(hv, vals.data(), vals.size());
    p[prefix + "/values_sha256"] = hv.hex();
  }
  add_scalar_i(w, prefix + "/profile_release", A.getProfileRelease(), "getProfileRelease()");
  add_scalar_i(w, prefix + "/value_release", A.getValueRelease(), "getValueRelease()");
  add_scalar_i(w, prefix + "/size", A.size(), "size()");
  add_scalar_i(w, prefix + "/symmetric", A.getMatrixType() == _symmetric ? 1 : 0,
               "1 if getMatrixType() == _symmetric");
}

/// Eigen::SparseLU on triplets built like SparseSolverEigen::setA, solve
/// with a Map destination like SparseSolverEigen::solve; records the
/// solution and the internal arrays. Returns the solution.
std::vector<double> record_sparse_lu(ap::FixtureWriter & w, const std::string & prefix, Int n,
                                     const std::vector<Eigen::Triplet<double>> & triplets,
                                     const std::vector<double> & b_in,
                                     bool internals = true) {
  SpMat A;
  A.resize(n, n);
  A.setZero();
  A.setFromTriplets(triplets.begin(), triplets.end());
  LuSpy lu;
  lu.analyzePattern(A);
  lu.factorize(A);
  auto & p = w.parameters();
  const bool ok = lu.info() == Eigen::Success;
  p[prefix + "_info_success"] = ok;
  p[prefix + "_last_error"] = lu.lastErrorMessage();
  add_ivec(w, prefix + "/perm_c", to_i64(lu.perm_c().indices(), n), "entry",
           "colsPermutation().indices() after analyzePattern (COLAMD + etree postorder)");
  add_ivec(w, prefix + "/etree", to_i64(lu.etree(), n), "entry",
           "column elimination tree, renumbered in postorder");
  if (!ok) {
    return {};
  }
  add_ivec(w, prefix + "/perm_r", to_i64(lu.perm_r().indices(), n), "entry",
           "rowsPermutation().indices() after factorize");
  add_scalar_i(w, prefix + "/nnz_l", lu.nnzL(), "nnzL()");
  add_scalar_i(w, prefix + "/nnz_u", lu.nnzU(), "nnzU()");
  const auto & g = lu.glu();
  const Int nsuper = g.supno(n);
  w.parameters()[prefix + "_nsuper"] = static_cast<long long>(nsuper);
  if (internals) {
  add_ivec(w, prefix + "/xsup", to_i64(g.xsup, nsuper + 2), "entry", "glu.xsup[0..=nsuper+1]");
  add_ivec(w, prefix + "/supno", to_i64(g.supno, n + 1), "entry", "glu.supno[0..=n]");
  add_ivec(w, prefix + "/xlsub", to_i64(g.xlsub, n + 1), "entry", "glu.xlsub (after fixupL)");
  add_ivec(w, prefix + "/lsub", to_i64(g.lsub, g.xlsub(n)), "entry",
           "glu.lsub[0..xlsub[n]] (after fixupL: rows of P*A)");
  add_ivec(w, prefix + "/xlusup", to_i64(g.xlusup, n + 1), "entry", "glu.xlusup");
  {
    std::vector<double> lusup(g.xlusup(n));
    for (Int i = 0; i < g.xlusup(n); ++i) {
      lusup[i] = g.lusup(i);
    }
    add_vec(w, prefix + "/lusup", lusup, "entry", "glu.lusup[0..xlusup[n]] (supernodal L and U blocks)");
  }
  add_ivec(w, prefix + "/xusub", to_i64(g.xusub, n + 1), "entry", "glu.xusub");
  add_ivec(w, prefix + "/usub", to_i64(g.usub, g.xusub(n)), "entry", "glu.usub[0..xusub[n]]");
  {
    std::vector<double> ucol(g.xusub(n));
    for (Int i = 0; i < g.xusub(n); ++i) {
      ucol[i] = g.ucol(i);
    }
    add_vec(w, prefix + "/ucol", ucol, "entry", "glu.ucol[0..xusub[n]]");
  }
  } // internals
  std::vector<double> b = b_in;
  std::vector<double> x(n);
  VectorProxy<Real> bm(b.data(), n);
  VectorProxy<Real> xm(x.data(), n);
  xm = lu.solve(bm);
  add_vec(w, prefix + "/x", x, "entry", "x = SparseLU::solve(b) into a Map destination");
  return x;
}

struct MeshSpec {
  std::string name;
  std::string where; // up / fz
  std::string rel;
  ElementType type;
};

std::string type_word(MatrixType t) { return t == _symmetric ? "sym" : "unsym"; }

/// Element matrices of a weighted graph Laplacian (stiffness-like, SPD on
/// the assembled system with Dirichlet rows) for `nb_dof` = 2, optionally
/// with an antisymmetric convection-like part; and a mass-like matrix.
void element_matrices(const Mesh & mesh, ElementType type, bool unsym, Array<Real> & K,
                      Array<Real> & M) {
  const auto & conn = mesh.getConnectivity(type);
  const auto & X = mesh.getNodes();
  const Int nnpe = conn.getNbComponent();
  const Int nd = nnpe * 2;
  const Int ne = conn.size();
  K.resize(ne);
  M.resize(ne);
  const double D[2][2] = {{2.0, 0.5}, {0.5, 1.0}};
  for (Int e = 0; e < ne; ++e) {
    for (Int a = 0; a < nnpe; ++a) {
      for (Int b = 0; b < nnpe; ++b) {
        const auto na = conn(e, a);
        const auto nb = conn(e, b);
        const double dx = X(nb, 0) - X(na, 0);
        const double dy = X(nb, 1) - X(na, 1);
        double lap = 0.;
        if (a != b) {
          lap = -1. / (1. + 40. * (dx * dx + dy * dy));
        } else {
          for (Int c = 0; c < nnpe; ++c) {
            if (c != a) {
              const auto nc = conn(e, c);
              const double ex = X(nc, 0) - X(na, 0);
              const double ey = X(nc, 1) - X(na, 1);
              lap += 1. / (1. + 40. * (ex * ex + ey * ey));
            }
          }
        }
        const double conv = unsym ? 0.1 * (dx + 0.3 * dy) + (a < b ? 1e-3 * (e % 5) : 0.) : 0.;
        const double mass = (a == b ? 2. : 1.) * 0.01 * double(e % 7 + 1);
        for (Int i = 0; i < 2; ++i) {
          for (Int j = 0; j < 2; ++j) {
            const Int r = a * 2 + i;
            const Int c = b * 2 + j;
            K(e, r + c * nd) = lap * D[i][j] + (i == j ? conv : 0.);
            M(e, r + c * nd) = i == j ? mass : 0.;
          }
        }
      }
    }
  }
}

void record_element_inputs(ap::FixtureWriter & w, const std::string & name, const Array<Real> & el,
                           Int nd) {
  const Int ne = el.size();
  std::vector<double> data(ne * nd * nd);
  for (Int e = 0; e < ne; ++e) {
    for (Int r = 0; r < nd; ++r) {
      for (Int c = 0; c < nd; ++c) {
        data[(e * nd + r) * nd + c] = el(e, r + c * nd);
      }
    }
  }
  w.add(name, {std::size_t(ne), std::size_t(nd), std::size_t(nd)}, {"element", "row", "col"},
        "matrix", data)
      .description("elemental matrices M(r, c), stored [element, row, col] row-major "
                   "(input fed to assembleElementalMatricesToMatrix)");
}

void aij_case(const fs::path & root, const fs::path & up, const fs::path & fz, const MeshSpec & ms,
              MatrixType ktype, MatrixType etype) {
  const auto cid = "solver/aij/" + ms.name + "_" + type_word(ktype) + "_" + type_word(etype);
  ap::FixtureWriter w(root, cid,
                      "DOFManagerDefault assembly of " + ms.name + " (" + type_word(ktype) +
                          " matrix, " + type_word(etype) +
                          " elemental matrices), terms, matVecMul, copyProfile/add, "
                          "applyBoundary, saveMatrix/saveProfile, SparseSolverEigen",
                      kProbe, kProbeVersion);
  w.add_tag("solver");
  w.add_tag("aij");
  // The large mesh stores no duplicated coordinate arrays, no factor
  // internals and only digests of the text outputs (size policy, 1 MiB).
  const bool big = ms.name == "triangle";
  const fs::path file = (ms.where == "up" ? up : fz) / ms.rel;
  w.add_input_file("mesh", file,
                   ms.where == "up" ? "upstream:" + ms.rel
                                    : "frozen:fixtures/meshes/gmsh-4.13.1/" + ms.rel);
  auto & p = w.parameters();
  p["mesh"] = ms.name;
  p["matrix_type"] = type_word(ktype);
  p["elemental_matrix_type"] = type_word(etype);
  p["nb_dof_per_node"] = 2LL;
  {
    std::stringstream ss;
    ss << ms.type;
    p["element_type"] = ss.str();
  }

  Mesh mesh(2);
  mesh.read(file.string());
  const Int nb_nodes = mesh.getNbNodes();
  DOFManagerDefault dm(mesh, "dof_manager");
  Array<Real> u(nb_nodes, 2, 0., "u");
  Array<bool> blocked(nb_nodes, 2, false, "blocked");
  dm.registerDOFs("u", u, _dst_nodal);
  dm.registerBlockedDOFs("u", blocked);
  const Int n = dm.getSystemSize();
  p["system_size"] = static_cast<long long>(n);

  // element equations (global) and element matrices
  const auto & conn = mesh.getConnectivity(ms.type);
  const Int nnpe = conn.getNbComponent();
  const Int nd = nnpe * 2;
  const auto & leq = dm.getLocalEquationsNumbers("u");
  {
    std::vector<I64> eqs;
    for (Int e = 0; e < conn.size(); ++e) {
      for (Int a = 0; a < nnpe; ++a) {
        for (Int d = 0; d < 2; ++d) {
          eqs.push_back(dm.localToGlobalEquationNumber(leq(conn(e, a) * 2 + d)));
        }
      }
    }
    w.add("element_equations", {std::size_t(conn.size()), std::size_t(nd)}, {"element", "entry"},
          "table", eqs)
        .description("global equation numbers of each element, node-major (n*nb_dof+d)");
  }
  Array<Real> Ke(0, nd * nd), Me(0, nd * nd);
  element_matrices(mesh, ms.type, etype == _unsymmetric, Ke, Me);
  record_element_inputs(w, "element_matrices", Ke, nd);
  if (!big) {
    record_element_inputs(w, "element_mass_matrices", Me, nd);
  }

  auto & K = aka::as_type<SparseMatrixAIJ>(dm.getNewMatrix("K", ktype));
  dm.assembleElementalMatricesToMatrix("K", "u", Ke, ms.type, _not_ghost, etype, empty_filter);
  record_aij(w, "K", K);
  {
    auto tmp = fs::temp_directory_path() / ("solver_probe_" + std::to_string(std::hash<std::string>{}(cid)));
    fs::remove_all(tmp);
    fs::create_directories(tmp);
    K.saveMatrix((tmp / "K").string()); // ".mtx" appended upstream
    K.saveProfile((tmp / "K_profile.mtx").string());
    if (big) {
      p["save_matrix_sha256"] = ap::Sha256::of(ap::read_file(tmp / "K.mtx"));
      p["save_profile_sha256"] = ap::Sha256::of(ap::read_file(tmp / "K_profile.mtx"));
    } else {
      w.add_input_file("save_matrix", tmp / "K.mtx", "SparseMatrixAIJ::saveMatrix(\"K\") output");
      w.add_input_file("save_profile", tmp / "K_profile.mtx", "SparseMatrixAIJ::saveProfile output");
    }
    fs::remove_all(tmp);
  }

  // matVecMul (Array version) twice: (alpha, beta) = (0.75, -1.25) on y0
  // with signed zeros, then (1, 0).
  {
    Array<Real> x(n, 1, 0., "x"), y(n, 1, 0., "y");
    std::vector<double> xv(n), y0(n), y1(n), y2(n);
    for (Int i = 0; i < n; ++i) {
      xv[i] = double((i * 7919) % 101 - 50) / 37.0;
      y0[i] = i % 11 == 0 ? -0.0 : double((i * 104729) % 13 - 6) / 3.0;
      x(i) = xv[i];
      y(i) = y0[i];
    }
    K.matVecMul(x, y, 0.75, -1.25);
    for (Int i = 0; i < n; ++i) {
      y1[i] = y(i);
    }
    K.matVecMul(x, y, 1., 0.);
    for (Int i = 0; i < n; ++i) {
      y2[i] = y(i);
    }
    add_vec(w, "matvec/x", xv, "dof", "input x");
    add_vec(w, "matvec/y0", y0, "dof", "input y");
    add_vec(w, "matvec/y_a0.75_b-1.25", y1, "dof", "K.matVecMul(x, y0, 0.75, -1.25)");
    add_vec(w, "matvec/y_a1_b0", y2, "dof", "then K.matVecMul(x, y, 1, 0)");
  }

  // Newmark-style J: J.copyProfile(M); J.add(K, e); J.add(M, c)
  if (!big) {
    auto & M = aka::as_type<SparseMatrixAIJ>(dm.getNewMatrix("M", _symmetric));
    dm.assembleElementalMatricesToMatrix("M", "u", Me, ms.type, _not_ghost, _symmetric,
                                         empty_filter);
    record_aij(w, "M", M);
    const auto jtype = std::min(ktype, MatrixType(_symmetric));
    auto & J = aka::as_type<SparseMatrixAIJ>(dm.getNewMatrix("J", jtype));
    J.copyProfile(M);
    record_aij(w, "J_copy_profile", J, Coo::Releases);
    // as in NewmarkBeta::assembleJacobian (J is a SparseMatrix& there)
    SparseMatrix & Jb = J;
    Jb.add(K, 0.3);
    Jb.add(M, 2.5);
    record_aij(w, "J", J);
    p["j_coefficients"] = ap::Json::array_of(std::vector<double>{0.3, 2.5});
  }

  // pre-assembled terms on K (local DOF indices node*2 + d)
  {
    TermsToAssemble terms("u", "u");
    std::vector<I64> ti, tj;
    std::vector<double> tv;
    for (Int k = 0; k < 12; ++k) {
      const Int i = (k * 37) % (nb_nodes * 2);
      const Int j = (k * 53 + 5) % (nb_nodes * 2);
      const double v = k % 4 == 3 ? -0.0 : 0.125 * double(k + 1) - 0.7;
      terms(i, j) += v;
      ti.push_back(i);
      tj.push_back(j);
      tv.push_back(v);
    }
    dm.assemblePreassembledMatrix("K", terms);
    add_ivec(w, "terms/i", ti, "entry", "TermsToAssemble rows (local DOF index node*2+d)");
    add_ivec(w, "terms/j", tj, "entry", "TermsToAssemble columns");
    add_vec(w, "terms/values", tv, "entry", "values added with terms(i, j) += v");
    record_aij(w, "K_terms", K, Coo::ValuesOnly);
  }

  // Dirichlet: x == 0 (both components), y == 0 (component 1)
  {
    const auto & X = mesh.getNodes();
    std::vector<I64> bl;
    for (Int i = 0; i < nb_nodes; ++i) {
      if (std::abs(X(i, 0)) < 1e-12) {
        blocked(i, 0) = true;
        blocked(i, 1) = true;
      }
      if (std::abs(X(i, 1)) < 1e-12) {
        blocked(i, 1) = true;
      }
      bl.push_back(blocked(i, 0));
      bl.push_back(blocked(i, 1));
    }
    add_ivec(w, "blocked", bl, "dof", "blocked flag per local DOF (node*2+d)");
    dm.updateGlobalBlockedDofs();
    dm.applyBoundary("K");
    record_aij(w, "K_bc", K, Coo::ValuesOnly);
  }

  // SparseSolverEigen on the Dirichlet-modified K, b = residual
  {
    Array<Real> b(nb_nodes, 2, 0., "b");
    std::vector<double> bv(n);
    for (Int i = 0; i < nb_nodes; ++i) {
      for (Int d = 0; d < 2; ++d) {
        b(i, d) = 1.0 + 0.25 * double((i * 3 + d * 7) % 9) - (i % 5 == 0 ? 2.0 : 0.0);
      }
    }
    dm.zeroResidual();
    dm.assembleToResidual("u", b);
    const auto & r = aka::as_type<SolverVectorDefault>(dm.getResidual()).getVector();
    for (Int i = 0; i < n; ++i) {
      bv[i] = r(i);
    }
    add_vec(w, "eigen/b", bv, "dof", "residual (global equation order) passed to SparseSolverEigen");
    {
      auto tmp = fs::temp_directory_path() / ("solver_probe_v_" + std::to_string(std::hash<std::string>{}(cid)));
      fs::remove_all(tmp);
      fs::create_directories(tmp);
      dm.getResidual().saveVector((tmp / "b").string());
      if (big) {
        p["save_vector_sha256"] = ap::Sha256::of(ap::read_file(tmp / "b.mtx"));
      } else {
        w.add_input_file("save_vector", tmp / "b.mtx", "SolverVectorDefault::saveVector(\"b\") output");
      }
      fs::remove_all(tmp);
    }
    SparseSolverEigen solver(dm, "K");
    solver.solve();
    const auto & sol = dm.getSolution("u");
    std::vector<double> xv(n);
    for (Int i = 0; i < nb_nodes; ++i) {
      for (Int d = 0; d < 2; ++d) {
        xv[dm.localToGlobalEquationNumber(leq(i * 2 + d))] = sol(i * 2 + d); // flat (size * nb_component, 1 component)
      }
    }
    add_vec(w, "eigen/x", xv, "dof", "SparseSolverEigen::solve() solution (global equation order)");

    // the same factorisation through the spy (internal arrays)
    std::vector<Eigen::Triplet<double>> triplets;
    for (auto && [i, j, a] : zip(K.getIRN(), K.getJCN(), K.getA())) {
      triplets.emplace_back(i - 1, j - 1, a);
      if (K.getMatrixType() == _symmetric and i != j) {
        triplets.emplace_back(j - 1, i - 1, a);
      }
    }
    auto xs = record_sparse_lu(w, "lu", n, triplets, bv, !big);
    bool same = xs.size() == xv.size();
    for (std::size_t i = 0; same and i < xs.size(); ++i) {
      same = std::memcmp(&xs[i], &xv[i], sizeof(double)) == 0;
    }
    p["spy_solution_equals_sparse_solver_eigen"] = same;
  }
  w.finish();
}

/// Triplet test matrices for the LU cases.
struct LuMatrix {
  std::string name;
  std::string description;
  Int n;
  std::vector<Eigen::Triplet<double>> t;
};

std::vector<LuMatrix> lu_matrices() {
  std::vector<LuMatrix> out;
  { // test_sparse_solver_eigen: diag(1/(1+i)), n = 11
    LuMatrix m{"diag_upstream_test", "test_sparse_solver_eigen matrix diag(1/(1+i)), n = 11", 11, {}};
    for (Int i = 0; i < 11; ++i) {
      m.t.emplace_back(i, i, 1. / (1. + i));
    }
    out.push_back(m);
  }
  { // unsymmetric random with weak diagonal: forces row pivoting
    LuMatrix m{"random_pivoting", "unsymmetric random sparse matrix, weak diagonal, n = 90", 90, {}};
    Lcg g(12345);
    std::vector<Int> perm(m.n);
    for (Int i = 0; i < m.n; ++i) {
      perm[i] = i;
    }
    for (Int i = m.n - 1; i > 0; --i) {
      Int j = Int(g.next() * double(i + 1));
      std::swap(perm[i], perm[j]);
    }
    for (Int i = 0; i < m.n; ++i) {
      m.t.emplace_back(i, perm[i], 4.0 + g.next());
      m.t.emplace_back(i, i, 1e-3 * (g.next() - 0.5));
      for (Int k = 0; k < 5; ++k) {
        Int j = Int(g.next() * double(m.n));
        m.t.emplace_back(i, j, 2.0 * g.next() - 1.0);
      }
    }
    out.push_back(m);
  }
  { // dense block of 150 columns (supernode split at maxsuper = 128) + tail
    LuMatrix m{"dense_block", "dense 150x150 block, diagonally dominant, plus a sparse tail, n = 170", 170, {}};
    Lcg g(777);
    for (Int j = 0; j < 150; ++j) {
      for (Int i = 0; i < 150; ++i) {
        m.t.emplace_back(i, j, (i == j ? 200.0 : 0.0) + g.next() - 0.5);
      }
    }
    for (Int i = 150; i < 170; ++i) {
      m.t.emplace_back(i, i, 3.0 + g.next());
      m.t.emplace_back(i, i - 150, 0.5 * g.next());
      m.t.emplace_back(i - 150, i, 0.5 * g.next());
      m.t.emplace_back(i, i - 1, -0.25);
    }
    out.push_back(m);
  }
  { // arrowhead (dense last row and column)
    LuMatrix m{"arrow", "arrowhead matrix, n = 61", 61, {}};
    Lcg g(99);
    for (Int i = 0; i < 60; ++i) {
      m.t.emplace_back(i, i, 2.0 + g.next());
      m.t.emplace_back(60, i, g.next() - 0.5);
      m.t.emplace_back(i, 60, g.next() - 0.5);
    }
    m.t.emplace_back(60, 60, 50.0);
    out.push_back(m);
  }
  { // convection-diffusion 5-point stencil on a 21x19 grid
    LuMatrix m{"convection_diffusion", "unsymmetric 5-point convection-diffusion, 21x19 grid", 21 * 19, {}};
    const Int nx = 21, ny = 19;
    for (Int j = 0; j < ny; ++j) {
      for (Int i = 0; i < nx; ++i) {
        const Int r = j * nx + i;
        m.t.emplace_back(r, r, 4.0);
        if (i > 0) m.t.emplace_back(r, r - 1, -1.0 - 0.3);
        if (i + 1 < nx) m.t.emplace_back(r, r + 1, -1.0 + 0.3);
        if (j > 0) m.t.emplace_back(r, r - nx, -1.0 - 0.1);
        if (j + 1 < ny) m.t.emplace_back(r, r + nx, -1.0 + 0.1);
      }
    }
    out.push_back(m);
  }
  { // explicit stored zeros incl. a zero diagonal and duplicate triplets
    LuMatrix m{"explicit_zeros_duplicates", "stored zeros, zero diagonal entries, duplicate triplets", 7, {}};
    for (Int i = 0; i < 7; ++i) {
      m.t.emplace_back(i, i, i % 3 == 1 ? 0.0 : 2.0 + i);
      m.t.emplace_back(i, (i + 1) % 7, 1.0);
      m.t.emplace_back((i + 3) % 7, i, 0.0);
    }
    m.t.emplace_back(1, 1, 1e-16);
    m.t.emplace_back(1, 1, 0.5);
    m.t.emplace_back(4, 4, -1.0);
    out.push_back(m);
  }
  { // structurally singular: empty column 2
    LuMatrix m{"structurally_singular", "column 2 has no entry", 5, {}};
    for (Int i = 0; i < 5; ++i) {
      if (i != 2) {
        m.t.emplace_back(i, i, 1.0 + i);
        m.t.emplace_back((i + 1) % 5, i, 0.5);
      }
    }
    out.push_back(m);
  }
  { // numerically singular: two identical rows of small integers
    LuMatrix m{"numerically_singular", "rows 0 and 3 identical (exact zero pivot)", 5, {}};
    const double r0[5] = {2, 1, 0, 1, 3};
    for (Int j = 0; j < 5; ++j) {
      if (r0[j] != 0) {
        m.t.emplace_back(0, j, r0[j]);
        m.t.emplace_back(3, j, r0[j]);
      }
    }
    m.t.emplace_back(1, 1, 4.0);
    m.t.emplace_back(1, 2, 1.0);
    m.t.emplace_back(2, 2, 5.0);
    m.t.emplace_back(2, 0, 1.0);
    m.t.emplace_back(4, 4, 6.0);
    m.t.emplace_back(4, 1, 1.0);
    out.push_back(m);
  }
  return out;
}

void lu_case(const fs::path & root, const LuMatrix & m) {
  const auto cid = "solver/lu/" + m.name;
  ap::FixtureWriter w(root, cid, "Eigen::SparseLU<SparseMatrix<double>> (COLAMD): " + m.description,
                      kProbe, kProbeVersion);
  w.add_tag("solver");
  w.add_tag("lu");
  auto & p = w.parameters();
  p["n"] = static_cast<long long>(m.n);
  std::vector<I64> rows, cols;
  std::vector<double> vals;
  for (auto & t : m.t) {
    rows.push_back(t.row());
    cols.push_back(t.col());
    vals.push_back(t.value());
  }
  add_ivec(w, "A/rows", rows, "nnz", "triplet rows (0-based, setFromTriplets order)");
  add_ivec(w, "A/cols", cols, "nnz", "triplet columns (0-based)");
  add_vec(w, "A/values", vals, "nnz", "triplet values (duplicates summed in order)");
  std::vector<double> b(m.n);
  for (Int i = 0; i < m.n; ++i) {
    b[i] = m.name == "diag_upstream_test" ? 1.0 : 1.0 + 0.5 * double((i * 13) % 7) - (i % 4 == 0 ? 1.5 : 0.0);
  }
  add_vec(w, "b", b, "entry", "right-hand side");
  record_sparse_lu(w, "lu", m.n, m.t, b);
  w.finish();
}

/// Matrix Market text, ostream formatting and release counters.
void text_case(const fs::path & root, const fs::path & up) {
  const auto cid = std::string("solver/text/matrix_market");
  ap::FixtureWriter w(root, cid,
                      "saveMatrix/saveProfile/saveVector text with special values, "
                      "std::ostream << double at precision 15, release counters",
                      kProbe, kProbeVersion);
  w.add_tag("solver");
  w.add_tag("text");
  const fs::path file = up / "test/test_solver/bar.msh";
  w.add_input_file("mesh", file, "upstream:test/test_solver/bar.msh");
  auto tmp = fs::temp_directory_path() / "solver_probe_text";
  fs::remove_all(tmp);
  fs::create_directories(tmp);

  const std::vector<double> special = {
      0.0, -0.0, 1.0, -2.0, 0.1, 1.0 / 3.0, 2.0 / 3.0, 1e-5, 1.25e-5, 1e-4, 9.99999999999999e-5,
      1e14, 1e15, 123456789012345.6, 1234567890123455.0, 1234567890123445.0, 0.5, 2.5,
      1e-300, 5e-324, std::numeric_limits<double>::max(), -2.5e100,
      std::numeric_limits<double>::infinity(), -std::numeric_limits<double>::infinity(),
      std::numeric_limits<double>::quiet_NaN(), -std::numeric_limits<double>::quiet_NaN(),
      0.999999999999999999, 99999999999999.95, 999999999999999.5, 0.30000000000000004};
  add_vec(w, "ostream/values", special, "entry", "values printed with precision(15)");
  {
    std::ofstream os(tmp / "ostream.txt");
    os.precision(std::numeric_limits<Real>::digits10);
    for (auto v : special) {
      os << v << "\n";
    }
  }
  w.add_input_file("ostream_text", tmp / "ostream.txt", "std::ostream << v, precision 15, one per line");

  Mesh mesh(2);
  mesh.read(file.string());
  DOFManagerDefault dm(mesh, "dof_manager");
  Array<Real> u(mesh.getNbNodes(), 1, 0., "u");
  dm.registerDOFs("u", u, _dst_nodal);
  const Int n = dm.getSystemSize();
  w.parameters()["system_size"] = static_cast<long long>(n);
  for (auto type : {_symmetric, _unsymmetric}) {
    const auto id = std::string(type == _symmetric ? "S" : "U");
    auto & A = aka::as_type<SparseMatrixAIJ>(dm.getNewMatrix(id, type));
    std::vector<I64> seq_p, seq_v;
    auto snap = [&]() {
      seq_p.push_back(A.getProfileRelease());
      seq_v.push_back(A.getValueRelease());
    };
    snap();
    for (std::size_t k = 0; k < special.size(); ++k) {
      const Int i = Int((k * 7) % 13), j = Int((k * 5 + 3) % 13);
      A.add(i, j);
      snap();
      A.add(i, j, special[k]);
      snap();
    }
    A.add(2, 9, 1.0);
    A.add(9, 2, 2.0);
    snap();
    A.saveMatrix((tmp / (id + ".mtx")).string());
    A.saveProfile((tmp / (id + "_profile.mtx")).string());
    w.add_input_file("save_matrix_" + id, tmp / (id + ".mtx"), "saveMatrix output");
    w.add_input_file("save_profile_" + id, tmp / (id + "_profile.mtx"), "saveProfile output");
    record_aij(w, id, A);
    A.mul(2.0);
    snap();
    A.set(0.5);
    snap();
    A.zero();
    snap();
    A.clearProfile();
    snap();
    add_ivec(w, id + "/release_sequence/profile", seq_p, "entry",
             "profile release after: construction, then add(i,j) and add(i,j,v) per special value, "
             "add(2,9)+add(9,2), mul, set, zero, clearProfile");
    add_ivec(w, id + "/release_sequence/value", seq_v, "entry", "value release at the same points");
  }
  // saveVector of the residual with special values
  {
    Array<Real> r(mesh.getNbNodes(), 1, 0., "r");
    std::vector<double> rv(n);
    for (Int i = 0; i < mesh.getNbNodes(); ++i) {
      r(i) = special[i % special.size()];
      rv[i] = r(i);
    }
    dm.assembleToResidual("u", r);
    dm.getResidual().saveVector((tmp / "r").string());
    w.add_input_file("save_vector", tmp / "r.mtx", "saveVector(\"r\") output");
    add_vec(w, "residual", rv, "dof", "residual values (0 + r: assembled into a zero residual)");
  }
  fs::remove_all(tmp);
  w.finish();
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: solver_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>\n";
    return 2;
  }
  const fs::path root = argv[1], up = argv[2], fz = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  try {
    const std::vector<MeshSpec> meshes = {
        {"triangle", "fz", "test/test_solver/triangle.msh", _triangle_3},
        {"bar", "up", "test/test_solver/bar.msh", _quadrangle_4}};
    for (const auto & ms : meshes) {
      for (auto kt : {_symmetric, _unsymmetric}) {
        for (auto et : {_symmetric, _unsymmetric}) {
          // the large mesh: only the two matching combinations
          if (ms.name == "triangle" and kt != et) {
            continue;
          }
          aij_case(root, up, fz, ms, kt, et);
        }
      }
    }
    for (const auto & m : lu_matrices()) {
      lu_case(root, m);
    }
    text_case(root, up);
  } catch (std::exception & e) {
    std::cerr << "solver_probe: " << e.what() << "\n";
    finalize();
    return 1;
  }
  finalize();
  return 0;
}
