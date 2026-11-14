// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe (decision D-036): bit-exact
// fixtures for the products upstream evaluates through Eigen's GEMM route,
// computed by the REAL upstream code -- Akantu's aka_types.hh (Matrix /
// Map aliases), aka_voigthelper.hh and Eigen 3.4.0 -- with the expressions
// and operand types of the upstream kernels:
//   * ShapeLagrange::computeBtDB (src/fe_engine/shape_lagrange_inline_impl.hh
//     l.451 order 4, l.460 order 2): Bt_D_B.noalias() = B.transpose() * D * B
//   * ShapeFunctions::gradientElementalFieldOnIntegrationPoints
//     (src/fe_engine/shape_functions_inline_impl.hh l.303):
//     nabla_u.noalias() = u * B.transpose()
//   * ShapeLagrange::computeNtbN (shape_lagrange_inline_impl.hh l.497):
//     Nt_b_N.noalias() = N.transpose() * b * N
//   * structural N * T, B * R (src/fe_engine/element_class_structural.hh
//     l.117/132/147, dynamic Matrix<Real>)
//   * MaterialElasticLinearAnisotropic<dim> (src/model/solid_mechanics/
//     materials/material_elastic_linear_anisotropic_inline_impl.hh l.38-40):
//     voigt_stress = C * voigt_strain (C: Matrix<Real>, dynamic; strain:
//     Vector<Real, voigt>, fixed), and material_elastic_linear_anisotropic.cc
//     l.147-149: Cfull = rotator * Cprime * revrotor (fixed, dynamic, fixed)
//   * mixed compile-time / run-time operands: every combination
//     of fixed (F) / dynamic (D) lhs rows, lhs cols, rhs rows, rhs cols at
//     shapes across the inner / outer / coefficient-based / GEMV / gebp
//     route boundaries, with plain, transposed-lhs and transposed-rhs
//     operands (a CwiseBinaryOp takes its compile-time size from its LEFT
//     operand, so lazy reductions are unrolled only for a fixed lhs column
//     count even when the rhs fixes the depth)
// plus plain dynamic / fixed products at the gebp boundaries (depth 8-16,
// odd rows, column-panel remainders, depth blocking 408/409/816/817, row
// and column blocking, GEMV column blocking from depth 128).
//
// Output format: as probe.cc (README.md): one record per line,
//   <op> <storage> <dims...> | <inputs> | <outputs>
// doubles as 16 hex digits of their IEEE-754 bit pattern, column-major.

#include <aka_types.hh>
#include <aka_voigthelper.hh>

#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <utility>
#include <vector>

using akantu::Real;
using Eigen::Dynamic;

namespace {

std::string hex(double x) {
  std::uint64_t b;
  std::memcpy(&b, &x, sizeof b);
  char buf[17];
  std::snprintf(buf, sizeof buf, "%016llx", static_cast<unsigned long long>(b));
  return buf;
}

struct Rec {
  std::string line;
  explicit Rec(const std::string & head) : line(head) {}
  Rec & sep() { line += " |"; return *this; }
  Rec & d(double x) { line += ' '; line += hex(x); return *this; }
  template <class M> Rec & mat(const M & m) {  // column-major
    for (Eigen::Index j = 0; j < m.cols(); ++j)
      for (Eigen::Index i = 0; i < m.rows(); ++i) d(m(i, j));
    return *this;
  }
  void emit() { std::puts(line.c_str()); }
};

void comment(const std::string & c) { std::printf("# %s\n", c.c_str()); }

struct Rng {
  std::uint64_t s;
  std::uint64_t next() {  // splitmix64
    std::uint64_t z = (s += 0x9e3779b97f4a7c15ULL);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    return z ^ (z >> 31);
  }
  double uni() { return double(next() >> 11) * 0x1.0p-53 * 2.0 - 1.0; }  // [-1, 1)
  int irange(int lo, int hi) { return lo + int(next() % std::uint64_t(hi - lo + 1)); }
  double wide() { return uni() * std::pow(10.0, irange(-3, 3)); }
  /// sparse entry: exact +0 / -0 / value
  double sparse() {
    int k = irange(0, 3);
    return k == 0 ? uni() : (k == 1 ? -0.0 : 0.0);
  }
};

const char * hd(const char * fmt, int a, int b, int c) {
  static char buf[128];
  std::snprintf(buf, sizeof buf, fmt, a, b, c);
  return buf;
}

/* ------------------------------------------------- plain dynamic products */
void matmul_dynamic(Rng & r, int R, int Cc, int K, bool sparse = false) {
  akantu::Matrix<Real> a(R, Cc), b(Cc, K);
  for (int j = 0; j < Cc; ++j) for (int i = 0; i < R; ++i) a(i, j) = sparse ? r.sparse() : r.wide();
  for (int j = 0; j < K; ++j) for (int i = 0; i < Cc; ++i) b(i, j) = sparse ? -std::fabs(r.uni()) : r.uni();
  akantu::Matrix<Real> c = a * b;
  Rec rec(hd("matmul D %d %d %d", R, Cc, K));
  rec.sep().mat(a).mat(b).sep().mat(c).emit();
}

/// at.transpose() * b with dynamic at (C x R): a row-major lhs for Eigen.
void trmatmul_dynamic(Rng & r, int Cc, int R, int K) {
  akantu::Matrix<Real> at(Cc, R), b(Cc, K);
  for (int j = 0; j < R; ++j) for (int i = 0; i < Cc; ++i) at(i, j) = r.wide();
  for (int j = 0; j < K; ++j) for (int i = 0; i < Cc; ++i) b(i, j) = r.uni();
  akantu::Matrix<Real> c = at.transpose() * b;
  Rec rec(hd("trmatmul D %d %d %d", Cc, R, K));
  rec.sep().mat(at).mat(b).sep().mat(c).emit();
}

/* --------------------------------------------------- plain fixed products */
template <int R, int Cc, int K> void matmul_fixed(Rng & r) {
  akantu::Matrix<Real, R, Cc> a;
  akantu::Matrix<Real, Cc, K> b;
  for (int j = 0; j < Cc; ++j) for (int i = 0; i < R; ++i) a(i, j) = r.wide();
  for (int j = 0; j < K; ++j) for (int i = 0; i < Cc; ++i) b(i, j) = r.uni();
  akantu::Matrix<Real, R, K> c = a * b;
  Rec rec(hd("matmul F %d %d %d", R, Cc, K));
  rec.sep().mat(a).mat(b).sep().mat(c).emit();
  akantu::Matrix<Real, Cc, R> at = a.transpose();
  akantu::Matrix<Real, R, K> c2 = at.transpose() * b;
  Rec rec2(hd("trmatmul F %d %d %d", Cc, R, K));
  rec2.sep().mat(at).mat(b).sep().mat(c2).emit();
}

/* ------------------------------------------------ ShapeLagrange::computeBtDB */
/// stiffness-like Voigt matrix (symmetric, positive definite, sometimes with
/// negative off-diagonal coupling)
akantu::Matrix<Real> stiffness(Rng & r, int n) {
  akantu::Matrix<Real> c(n, n);
  for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) c(i, j) = r.uni();
  akantu::Matrix<Real> d = c * c.transpose();
  for (int i = 0; i < n; ++i) d(i, i) += double(n);
  return d * (1e9 * (1.0 + std::fabs(r.uni())));
}

template <int dim>
void btdb4(Rng & r, const char * elem, int nb_nodes, bool sparse) {
  constexpr int tangent_size = akantu::VoigtHelper<dim>::size;
  std::vector<Real> bfull_data(dim * nb_nodes), d_data(tangent_size * tangent_size),
      out(dim * nb_nodes * dim * nb_nodes);
  for (auto & v : bfull_data) v = sparse ? r.sparse() : r.wide();
  {
    akantu::Matrix<Real> d = stiffness(r, tangent_size);
    if (sparse) d = -d;  // negative terms: zero products are -0
    std::memcpy(d_data.data(), d.data(), sizeof(Real) * d_data.size());
  }
  // upstream types: make_const_view(shapes_derivatives, dim, nb_nodes) and
  // make_view(Ds, tangent_size, tangent_size) / make_view(BtDBs, n, n) are
  // dynamic Eigen maps; B is a dynamic akantu::Matrix<Real>.
  Eigen::Map<const akantu::Matrix<Real>> Bfull(bfull_data.data(), dim, nb_nodes);
  Eigen::Map<const akantu::Matrix<Real>> D(d_data.data(), tangent_size, tangent_size);
  Eigen::Map<akantu::Matrix<Real>> Bt_D_B(out.data(), dim * nb_nodes, dim * nb_nodes);
  akantu::Matrix<Real> B(tangent_size, dim * nb_nodes);
  akantu::VoigtHelper<dim>::transferBMatrixToSymVoigtBMatrix(Bfull, B, nb_nodes);
  Bt_D_B.noalias() = B.transpose() * D * B;
  char head[128];
  std::snprintf(head, sizeof head, "btdb4 D %d %d %s", dim, nb_nodes, elem);
  Rec rec(head);
  rec.sep().mat(Bfull).mat(D).sep().mat(Bt_D_B).emit();
}

template <int dim>
void btdb2(Rng & r, const char * elem, int nb_nodes, bool sparse) {
  std::vector<Real> b_data(dim * nb_nodes), d_data(dim * dim), out(nb_nodes * nb_nodes);
  for (auto & v : b_data) v = sparse ? r.sparse() : r.wide();
  {
    akantu::Matrix<Real> d = stiffness(r, dim) * 1e-9;
    if (sparse) d = -d;
    std::memcpy(d_data.data(), d.data(), sizeof(Real) * d_data.size());
  }
  Eigen::Map<const akantu::Matrix<Real>> B(b_data.data(), dim, nb_nodes);
  Eigen::Map<const akantu::Matrix<Real>> D(d_data.data(), dim, dim);
  Eigen::Map<akantu::Matrix<Real>> Bt_D_B(out.data(), nb_nodes, nb_nodes);
  Bt_D_B.noalias() = B.transpose() * D * B;
  char head[128];
  std::snprintf(head, sizeof head, "btdb2 D %d %d %s", dim, nb_nodes, elem);
  Rec rec(head);
  rec.sep().mat(B).mat(D).sep().mat(Bt_D_B).emit();
}

/* ------------------------- gradientElementalFieldOnIntegrationPoints */
template <int element_dimension, int nb_nodes_per_element>
void gradient(Rng & r, const char * elem, int nb_dof, bool sparse) {
  std::vector<Real> u_data(nb_dof * nb_nodes_per_element),
      b_data(element_dimension * nb_nodes_per_element), out(nb_dof * element_dimension);
  for (auto & v : u_data) v = sparse ? r.sparse() : r.wide();
  for (auto & v : b_data) v = sparse ? -std::fabs(r.uni()) : r.uni();
  // make_view<Dynamic, nnodes>(u_el, dof, nnodes),
  // make_const_view<element_dimension, nnodes>(shapes_derivatives),
  // make_view<Dynamic, element_dimension>(out_nablauq, dof, element_dimension)
  Eigen::Map<akantu::Matrix<Real, Dynamic, nb_nodes_per_element>> u(u_data.data(), nb_dof,
                                                                      nb_nodes_per_element);
  Eigen::Map<const akantu::Matrix<Real, element_dimension, nb_nodes_per_element>> B(b_data.data());
  Eigen::Map<akantu::Matrix<Real, Dynamic, element_dimension>> nabla_u(out.data(), nb_dof,
                                                                        element_dimension);
  nabla_u.noalias() = u * B.transpose();
  char head[128];
  std::snprintf(head, sizeof head, "grad M %d %d %d %s", element_dimension, nb_nodes_per_element,
                nb_dof, elem);
  Rec rec(head);
  rec.sep().mat(u).mat(B).sep().mat(nabla_u).emit();
}

/* --------------------------------------------------- ShapeLagrange::computeNtbN */
void ntbn(Rng & r, const char * elem, int size_of_shapes, bool sparse) {
  // only nb_degree_of_freedom == 1 is well-formed upstream (N.transpose()
  // is size_of_shapes x 1, b is nb_dof x 1)
  std::vector<Real> n_data(size_of_shapes), b_data(1), out(size_of_shapes * size_of_shapes);
  for (auto & v : n_data) v = sparse ? r.sparse() : r.uni();
  b_data[0] = sparse ? -std::fabs(r.uni()) : r.wide();
  Eigen::Map<const akantu::Matrix<Real>> N(n_data.data(), 1, size_of_shapes);
  Eigen::Map<akantu::Matrix<Real>> b(b_data.data(), 1, 1);
  Eigen::Map<akantu::Matrix<Real>> Nt_b_N(out.data(), size_of_shapes, size_of_shapes);
  Nt_b_N.noalias() = N.transpose() * b * N;
  char head[128];
  std::snprintf(head, sizeof head, "ntbn D %d %s", size_of_shapes, elem);
  Rec rec(head);
  rec.sep().mat(N).mat(b).sep().mat(Nt_b_N).emit();
}

/* -------------------------------------- mixed compile-time / run-time sizes */
/// Extent of one dimension: compile-time `n` or Eigen::Dynamic.
constexpr int ext(bool fixed, int n) { return fixed ? n : int(Dynamic); }

/// A matrix of the given (possibly fixed) type with run-time size r x c
/// (resize is a no-op for fixed sizes; avoids the coefficient-initialising
/// two-argument constructor of fixed 2-vectors).
template <class M> M sized(Eigen::Index r, Eigen::Index c) {
  M m;
  m.resize(r, c);
  return m;
}

/// code bits 3..0: lhs rows, lhs cols, rhs rows, rhs cols fixed (F) or not (D)
std::string code_str(int code) {
  std::string s;
  for (int b = 3; b >= 0; --b) s += (code >> b) & 1 ? 'F' : 'D';
  return s;
}

/// One product `lhs (R x K) * rhs (K x C)` whose operand types have the
/// compile-time extents of `code`, assigned to the product's plain type.
///   variant 0 `mixed`:   a * b              (inputs a (R x K), b (K x C))
///   variant 1 `trmixed`: at.transpose() * b (inputs at (K x R), b)
///   variant 2 `mixedtr`: a * bt.transpose() (inputs a, bt (C x K))
template <int R, int K, int C, int code, int variant>
void mixed_one(Rng & r, bool sparse) {
  constexpr int LR = ext(code & 8, R), LC = ext(code & 4, K);
  constexpr int RR = ext(code & 2, K), RC = ext(code & 1, C);
  auto lval = [&] { return sparse ? r.sparse() : r.wide(); };
  auto rval = [&] { return sparse ? -std::fabs(r.uni()) : r.uni(); };
  static const char * names[] = {"mixed", "trmixed", "mixedtr"};
  char head[128];
  std::snprintf(head, sizeof head, "%s M %s %d %d %d", names[variant], code_str(code).c_str(), R,
                K, C);
  Rec rec(head);
  rec.sep();
  if constexpr (variant == 0) {
    auto a = sized<akantu::Matrix<Real, LR, LC>>(R, K);
    auto b = sized<akantu::Matrix<Real, RR, RC>>(K, C);
    for (int j = 0; j < K; ++j) for (int i = 0; i < R; ++i) a(i, j) = lval();
    for (int j = 0; j < C; ++j) for (int i = 0; i < K; ++i) b(i, j) = rval();
    typename decltype(a * b)::PlainObject c = a * b;
    rec.mat(a).mat(b).sep().mat(c);
  } else if constexpr (variant == 1) {
    auto at = sized<akantu::Matrix<Real, LC, LR>>(K, R);
    auto b = sized<akantu::Matrix<Real, RR, RC>>(K, C);
    for (int j = 0; j < R; ++j) for (int i = 0; i < K; ++i) at(i, j) = lval();
    for (int j = 0; j < C; ++j) for (int i = 0; i < K; ++i) b(i, j) = rval();
    typename decltype(at.transpose() * b)::PlainObject c = at.transpose() * b;
    rec.mat(at).mat(b).sep().mat(c);
  } else {
    auto a = sized<akantu::Matrix<Real, LR, LC>>(R, K);
    auto bt = sized<akantu::Matrix<Real, RC, RR>>(C, K);
    for (int j = 0; j < K; ++j) for (int i = 0; i < R; ++i) a(i, j) = lval();
    for (int j = 0; j < K; ++j) for (int i = 0; i < C; ++i) bt(i, j) = rval();
    typename decltype(a * bt.transpose())::PlainObject c = a * bt.transpose();
    rec.mat(a).mat(bt).sep().mat(c);
  }
  rec.emit();
}

template <int R, int K, int C, int variant, int... codes>
void mixed_codes(Rng & r, bool sparse, std::integer_sequence<int, codes...>) {
  (mixed_one<R, K, C, codes, variant>(r, sparse), ...);
}

/// all 16 F/D combinations of the four operand extents
template <int R, int K, int C, int variant = 0> void mixed_all(Rng & r, bool sparse = false) {
  mixed_codes<R, K, C, variant>(r, sparse, std::make_integer_sequence<int, 16>{});
}

/* ------------------------------------ MaterialElasticLinearAnisotropic<dim> */
/// computeStressOnQuad: `auto voigt_stress = this->C * voigt_strain;
/// voigtToStress<dim>(voigt_stress, sigma)` -- C is the member
/// `Matrix<Real> C` (dynamic), the strain the `Vector<Real, voigt>` returned
/// by VoigtHelper<dim>::matrixToVoigtWithFactors; voigtToMatrix reads
/// `voigt_stress(I)`, which is printed here for every I.
template <int dim> void aniso_stress(Rng & r, bool sparse) {
  constexpr int v = akantu::VoigtHelper<dim>::size;
  akantu::Matrix<Real> C = stiffness(r, v);
  if (sparse)
    for (int j = 0; j < v; ++j) for (int i = 0; i < v; ++i) if (r.irange(0, 1)) C(i, j) = r.sparse();
  akantu::Matrix<Real, dim, dim> eps;
  for (int j = 0; j < dim; ++j)
    for (int i = 0; i <= j; ++i)
      eps(i, j) = eps(j, i) = sparse ? -std::fabs(r.sparse()) : r.uni() * 1e-3;
  auto voigt_strain = akantu::VoigtHelper<dim>::matrixToVoigtWithFactors(eps);
  auto voigt_stress = C * voigt_strain;
  char head[64];
  std::snprintf(head, sizeof head, "aniso M %d", dim);
  Rec rec(head);
  rec.sep().mat(C).mat(voigt_strain).sep();
  for (int I = 0; I < v; ++I) rec.d(voigt_stress(I));
  rec.emit();
}

/// updateInternalParameters: rotator / revrotor (Matrix<Real, D2, D2>, built
/// from rot_mat as upstream), Cprime (Matrix<Real>, dynamic D2 x D2),
/// `Cfull = rotator * Cprime * revrotor` into a Matrix<Real, D2, D2>.
template <int dim> void aniso_rot(Rng & r) {
  constexpr int D2 = dim * dim;
  akantu::Matrix<Real, dim, dim> rot;
  for (int j = 0; j < dim; ++j) for (int i = 0; i < dim; ++i) rot(i, j) = r.uni();
  akantu::Matrix<Real, D2, D2> rotator, revrotor;
  for (int i = 0; i < dim; ++i)
    for (int j = 0; j < dim; ++j)
      for (int k = 0; k < dim; ++k)
        for (int l = 0; l < dim; ++l) {
          auto I = akantu::VoigtHelper<dim>::mat[i][j];
          auto J = akantu::VoigtHelper<dim>::mat[k][l];
          rotator(I, J) = rot(k, i) * rot(l, j);
          revrotor(I, J) = rot(i, k) * rot(j, l);
        }
  akantu::Matrix<Real> Cprime = stiffness(r, D2);
  akantu::Matrix<Real, D2, D2> Cfull;
  Cfull = rotator * Cprime * revrotor;
  char head[64];
  std::snprintf(head, sizeof head, "anisorot M %d", dim);
  Rec rec(head);
  rec.sep().mat(rotator).mat(Cprime).mat(revrotor).sep().mat(Cfull).emit();
}

struct Lagrange {
  const char * name;
  int dim, nnodes;
};
const Lagrange lagrange[] = {
    {"_segment_2", 1, 2},       {"_segment_3", 1, 3},        {"_triangle_3", 2, 3},
    {"_triangle_6", 2, 6},      {"_quadrangle_4", 2, 4},     {"_quadrangle_8", 2, 8},
    {"_tetrahedron_4", 3, 4},   {"_tetrahedron_10", 3, 10},  {"_pentahedron_6", 3, 6},
    {"_pentahedron_15", 3, 15}, {"_hexahedron_8", 3, 8},     {"_hexahedron_20", 3, 20},
};

template <int dim> void btdb_both(Rng & r, const Lagrange & e, bool sparse) {
  btdb4<dim>(r, e.name, e.nnodes, sparse);
  btdb2<dim>(r, e.name, e.nnodes, sparse);
}

template <int dim, int nn> void gradient_dofs(Rng & r, const char * name, bool sparse) {
  gradient<dim, nn>(r, name, 1, sparse);
  gradient<dim, nn>(r, name, 2, sparse);
  gradient<dim, nn>(r, name, 3, sparse);
}

void gradients(Rng & r, bool sparse) {
  gradient_dofs<1, 2>(r, "_segment_2", sparse);
  gradient_dofs<1, 3>(r, "_segment_3", sparse);
  gradient_dofs<2, 3>(r, "_triangle_3", sparse);
  gradient_dofs<2, 6>(r, "_triangle_6", sparse);
  gradient_dofs<2, 4>(r, "_quadrangle_4", sparse);
  gradient_dofs<2, 8>(r, "_quadrangle_8", sparse);
  gradient_dofs<3, 4>(r, "_tetrahedron_4", sparse);
  gradient_dofs<3, 10>(r, "_tetrahedron_10", sparse);
  gradient_dofs<3, 6>(r, "_pentahedron_6", sparse);
  gradient_dofs<3, 15>(r, "_pentahedron_15", sparse);
  gradient_dofs<3, 8>(r, "_hexahedron_8", sparse);
  gradient_dofs<3, 20>(r, "_hexahedron_20", sparse);
  // 27 nodes: no upstream element type, requested shape (gebp, depth 27)
  gradient_dofs<3, 27>(r, "_synthetic_27", sparse);
}

}  // namespace

int main() {
  Rng r{0x5eed0002ULL};

  comment("ShapeLagrange::computeBtDB, order_d 4 and 2, every Lagrange element shape size");
  for (int rep = 0; rep < 3; ++rep) {
    bool sparse = rep == 2;
    for (auto & e : lagrange) {
      if (e.dim == 1) btdb_both<1>(r, e, sparse);
      if (e.dim == 2) btdb_both<2>(r, e, sparse);
      if (e.dim == 3) btdb_both<3>(r, e, sparse);
    }
  }

  comment("gradientElementalFieldOnIntegrationPoints: u * B.transpose(), dof 1..3");
  for (int rep = 0; rep < 2; ++rep) gradients(r, rep == 1);

  comment("ShapeLagrange::computeNtbN: N.transpose() * b * N (nb_dof = 1)");
  for (int rep = 0; rep < 3; ++rep)
    for (auto & e : lagrange) ntbn(r, e.name, e.nnodes, rep == 2);
  for (int s : {9, 10, 17, 18, 27}) ntbn(r, "_synthetic", s, false);

  comment("structural N * T and B * R (dynamic): bernoulli_beam_2 (3x6, 2x6; T 6x6),");
  comment("bernoulli_beam_3 (6x12, 4x12; T 12x12), discrete_kirchhoff_triangle_18 (6x18; T 18x18)");
  for (int rep = 0; rep < 2; ++rep) {
    matmul_dynamic(r, 3, 6, 6);
    matmul_dynamic(r, 2, 6, 6);
    matmul_dynamic(r, 6, 12, 12);
    matmul_dynamic(r, 4, 12, 12);
    matmul_dynamic(r, 6, 18, 18);
    matmul_dynamic(r, 6, 18, 18, true);
  }

  comment("depth 8..16, odd rows, >= 4 columns (column-panel remainders), dynamic and fixed");
  for (int c = 8; c <= 16; ++c) {
    matmul_dynamic(r, 5, c, 7);
    matmul_dynamic(r, 7, c, 4);
    matmul_dynamic(r, 3, c, 9);
    matmul_dynamic(r, 9, c, 13);
    matmul_dynamic(r, 5, c, 7, true);
    trmatmul_dynamic(r, c, 5, 7);
  }
#define FIXED_DEPTH(c) matmul_fixed<5, c, 7>(r); matmul_fixed<9, c, 5>(r); matmul_fixed<3, c, 13>(r);
  FIXED_DEPTH(8) FIXED_DEPTH(9) FIXED_DEPTH(10) FIXED_DEPTH(11) FIXED_DEPTH(12)
  FIXED_DEPTH(13) FIXED_DEPTH(14) FIXED_DEPTH(15) FIXED_DEPTH(16)
#undef FIXED_DEPTH
  matmul_fixed<6, 12, 12>(r); matmul_fixed<4, 12, 12>(r); matmul_fixed<6, 18, 18>(r);
  matmul_fixed<12, 3, 12>(r); matmul_fixed<24, 6, 24>(r); matmul_fixed<9, 9, 9>(r);

  comment("depth blocking (dynamic: kc 408 / 208+201 / 408+408 / 280+280+257; fixed: none)");
  for (int depth : {407, 408, 409, 816, 817}) {
    matmul_dynamic(r, 3, depth, 5);
    matmul_dynamic(r, 2, depth, 4);
  }
  matmul_dynamic(r, 3, 1000, 6);
  matmul_fixed<3, 409, 5>(r);
  matmul_fixed<3, 817, 5>(r);

  comment("row blocking (max >= 48, depth <= 408) and column blocking (nc < cols)");
  matmul_dynamic(r, 50, 20, 50);
  matmul_dynamic(r, 49, 9, 6);
  matmul_dynamic(r, 3, 12, 301);
  matmul_dynamic(r, 2, 10, 300);
  matmul_dynamic(r, 60, 6, 60);

  comment("GEMV fallbacks: column (col-major kernel, blocked from depth 128), row, 1x1");
  for (int depth : {20, 127, 128, 129, 200, 409}) {
    matmul_dynamic(r, 7, depth, 1);
    matmul_dynamic(r, 1, depth, 6);
    matmul_dynamic(r, 1, depth, 1);
    trmatmul_dynamic(r, depth, 7, 1);
    trmatmul_dynamic(r, depth, 1, 6);
  }
  comment("MaterialElasticLinearAnisotropic<dim>: C * voigt_strain (C dynamic, strain");
  comment("Vector<Real, voigt>), Cfull = rotator * Cprime * revrotor (fixed, dynamic, fixed)");
  for (int rep = 0; rep < 4; ++rep) {
    aniso_stress<1>(r, rep == 3);
    aniso_stress<2>(r, rep == 3);
    aniso_stress<3>(r, rep == 3);
  }
  for (int rep = 0; rep < 2; ++rep) {
    aniso_rot<1>(r);
    aniso_rot<2>(r);
    aniso_rot<3>(r);
  }

  comment("mixed fixed (F) / dynamic (D) extents (lhs rows, lhs cols, rhs rows, rhs cols):");
  comment("all 16 combinations, inner / outer / coefficient-based / GEMV / gebp boundaries");
  mixed_all<3, 3, 1>(r);    // 2-D anisotropic stress
  mixed_all<6, 6, 1>(r);    // 3-D anisotropic stress
  mixed_all<1, 3, 1>(r);    // inner product / 1x1 fallbacks
  mixed_all<1, 6, 4>(r);    // row vector x matrix
  mixed_all<3, 1, 3>(r);    // depth 1, small
  mixed_all<8, 1, 8>(r);    // depth 1, large: outer product
  mixed_all<4, 4, 4>(r);    // 2-D rotation (D2 = 4)
  mixed_all<9, 9, 9>(r);    // 3-D rotation (D2 = 9)
  mixed_all<3, 7, 5>(r);    // largest small depth
  mixed_all<3, 8, 3>(r);    // smallest large depth, GemmProduct below 20
  mixed_all<5, 9, 5>(r);    // 19 < 20: coefficient-based
  mixed_all<5, 10, 5>(r);   // 20: gebp / GEMV
  mixed_all<6, 13, 7>(r);   // gebp panels + remainder, even/odd chains
  mixed_all<3, 27, 1>(r);   // unrolling limit: 27*3 + 26 = 107 <= 110
  mixed_all<3, 28, 1>(r);   // beyond the unrolling limit
  mixed_all<7, 20, 1>(r);   // column GEMV vs coefficient-based
  mixed_all<1, 20, 1>(r);   // inner product vs run-time 1x1 dot fallback
  comment("mixed extents with exact +-0 terms (signed zeros of each route)");
  mixed_all<6, 6, 1>(r, true);
  mixed_all<3, 8, 3>(r, true);
  mixed_all<1, 20, 1>(r, true);
  mixed_all<8, 1, 8>(r, true);
  comment("mixed extents, transposed lhs (trmixed: at.transpose() * b) and rhs (mixedtr)");
  mixed_all<3, 4, 3, 1>(r);
  mixed_all<3, 8, 3, 1>(r);
  mixed_all<7, 12, 1, 1>(r);
  mixed_all<3, 4, 3, 2>(r);
  mixed_all<1, 14, 6, 2>(r);
  comment("mixed extents, deep: depth blocking with a compile-time depth and a run-time");
  comment("result dimension (not finite: 208 + 201), GEMV column blocking (depth 129)");
  mixed_one<2, 409, 4, 0b0011, 0>(r, false);  // DD * FF
  mixed_one<2, 409, 4, 0b1100, 0>(r, false);  // FF * DD
  mixed_one<7, 129, 1, 0b0011, 0>(r, false);  // DD * FF: column GEMV
  mixed_one<7, 129, 1, 0b0100, 0>(r, false);  // DF * DD: GemmProduct -> GEMV
  mixed_one<7, 129, 1, 0b1011, 0>(r, false);  // FD * FF: coefficient-based
  return 0;
}
