// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle probe: bit-exact fixtures
// for small dense operations and eigen-decompositions computed by the REAL
// upstream code -- Akantu's aka_types.hh (MatrixBase::eig / eigh overloads,
// determinant, inverse, products, doubleDot), aka_voigthelper.hh and Eigen
// 3.4.0 -- from the oracle install (/build/install-<config>).
//
// Output: one record per line, fields separated by " | ":
//   <op> <storage F|D> <dims...> | <input doubles> | <output fields...>
// Doubles are 16 hex digits of their IEEE-754 bit pattern. An upstream
// exception is written as the output token "EXC". See README.md.

#include <aka_types.hh>
#include <aka_voigthelper.hh>

#include <complex>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

using akantu::Real;
using C = std::complex<double>;

namespace {

/* ---------------------------------------------------------------- output */
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
  Rec & tok(const char * t) { line += ' '; line += t; return *this; }
  template <class M> Rec & mat(const M & m) {  // column-major
    for (Eigen::Index j = 0; j < m.cols(); ++j)
      for (Eigen::Index i = 0; i < m.rows(); ++i) d(m(i, j));
    return *this;
  }
  template <class M> Rec & cmat(const M & m) {
    for (Eigen::Index j = 0; j < m.cols(); ++j)
      for (Eigen::Index i = 0; i < m.rows(); ++i) { d(m(i, j).real()); d(m(i, j).imag()); }
    return *this;
  }
  void emit() { std::puts(line.c_str()); }
};

/* ---------------------------------------------------------------- random */
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
};

using MX = Eigen::MatrixXd;

MX orthogonal(Rng & r, int n) {  // product of two Householder reflections
  MX q = MX::Identity(n, n);
  for (int t = 0; t < 2; ++t) {
    Eigen::VectorXd v(n);
    for (int i = 0; i < n; ++i) v(i) = r.uni();
    double nv = v.norm();
    if (nv == 0) continue;
    v /= nv;
    q = (MX::Identity(n, n) - 2.0 * v * v.transpose()) * q;
  }
  return q;
}

/// Matrix family `g`, size n (families documented in README.md).
MX family(Rng & r, int g, int n) {
  MX a = MX::Zero(n, n);
  switch (g) {
  case 0:  // uniform
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) = r.uni();
    break;
  case 1:  // wide exponent range
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) = r.uni() * std::pow(10.0, r.irange(-8, 8));
    break;
  case 2: {  // symmetric
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) = r.uni();
    MX t = a.transpose();
    a = (a + t) * 0.5;
    break;
  }
  case 3: {  // symmetric, repeated / clustered eigenvalues
    MX q = orthogonal(r, n);
    Eigen::VectorXd lam(n);
    for (int i = 0; i < n; ++i) lam(i) = double(r.irange(-2, 2));
    if (n > 1) lam(1) = lam(0);
    if (n > 3 && (r.next() & 1)) lam(3) = lam(0) * (1.0 + 1e-13);
    a = q * lam.asDiagonal() * q.transpose();
    break;
  }
  case 4: {  // near singular: rank n-1 + tiny perturbation
    for (int k = 0; k + 1 < n; ++k) {
      Eigen::VectorXd u(n), v(n);
      for (int i = 0; i < n; ++i) { u(i) = r.uni(); v(i) = r.uni(); }
      a += u * v.transpose();
    }
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) += 1e-14 * r.uni();
    break;
  }
  case 5: {  // non-symmetric with complex conjugate pairs
    for (int i = 0; i + 1 < n; i += 2) {
      double re = r.uni(), im = 0.1 + std::fabs(r.uni());
      a(i, i) = re; a(i + 1, i + 1) = re; a(i, i + 1) = -im; a(i + 1, i) = im;
    }
    if (n % 2) a(n - 1, n - 1) = r.uni();
    MX q = orthogonal(r, n);
    a = q * a * q.transpose();
    break;
  }
  case 6: {  // defective (Jordan block) conjugated by a well-conditioned matrix
    double lam = r.uni();
    for (int i = 0; i < n; ++i) { a(i, i) = lam; if (i + 1 < n) a(i, i + 1) = 1.0; }
    MX s = MX::Identity(n, n);
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) s(i, j) += 0.2 * r.uni();
    MX si = s.inverse();
    a = s * a * si;
    break;
  }
  case 7:  // small integers
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) = double(r.irange(-3, 3));
    break;
  case 8: {  // diagonal with ties and signed zeros
    for (int i = 0; i < n; ++i) a(i, i) = double(r.irange(-1, 2));
    if (n > 2) a(2, 2) = -0.0;
    break;
  }
  case 9:  // sparse with exact zeros and -0.0
    for (int j = 0; j < n; ++j)
      for (int i = 0; i < n; ++i) {
        int k = r.irange(0, 3);
        a(i, j) = k == 0 ? r.uni() : (k == 1 ? -0.0 : 0.0);
      }
    break;
  case 10:  // upper triangular (already in Schur form)
    for (int j = 0; j < n; ++j) for (int i = 0; i <= j; ++i) a(i, j) = r.uni();
    break;
  case 11: {  // graded
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) = r.uni() * std::pow(10.0, 2 * i - n);
    break;
  }
  case 12: {  // nearly symmetric (rounding-level asymmetry)
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) = r.uni();
    MX t = a.transpose();
    a = (a + t) * 0.5;
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) a(i, j) += 1e-15 * r.uni();
    break;
  }
  default: {  // 13: upstream-like symmetric positive definite (stiffness/strain-like)
    MX b(n, n);
    for (int j = 0; j < n; ++j) for (int i = 0; i < n; ++i) b(i, j) = r.uni();
    a = b * b.transpose() + double(n) * MX::Identity(n, n);
    break;
  }
  }
  return a;
}
constexpr int nb_families = 14;

/* ------------------------------------------------------- eig / eigh ops */
template <class MatT, class VecT, class CMatT, class CVecT>
void eig_record(const char * st, int n, const MatT & A) {
  char head[64];
  std::snprintf(head, sizeof head, "eig %s %d", st, n);
  Rec rec(head);
  rec.sep().mat(A).sep();
  // 1. A.eig<complex>() : complex eigenvalues, EigenSolver order
  {
    CVecT cv = A.template eig<C>();
    for (Eigen::Index i = 0; i < cv.size(); ++i) { rec.d(cv(i).real()); rec.d(cv(i).imag()); }
  }
  rec.sep();
  // 2. A.eig(values) real
  { VecT v(n); A.eig(v); rec.mat(v); }
  rec.sep();
  // 3. A.eig(values, vectors) (sort = true default, real)
  try { VecT v(n); MatT V(n, n); A.eig(v, V); rec.mat(v).mat(V); } catch (...) { rec.tok("EXC"); }
  rec.sep();
  // 4. A.eig(values, vectors, false) (real)
  try { VecT v(n); MatT V(n, n); A.eig(v, V, false); rec.mat(v).mat(V); } catch (...) { rec.tok("EXC"); }
  rec.sep();
  // 5. complex eigenvectors. Upstream's A.eig(cvalues, cvectors, false) is
  //    ill-formed (the sort comparator is instantiated for std::complex and
  //    does not compile), so the same EigenSolver call is made directly.
  {
    Eigen::EigenSolver<MatT> solver(A, true);
    CVecT v = solver.eigenvalues();
    CMatT V = solver.eigenvectors();
    rec.cmat(v).cmat(V);
  }
  rec.sep();
  // 6. A.eigh(values)
  { VecT v(n); A.eigh(v); rec.mat(v); }
  rec.sep();
  // 7. A.eigh(values, vectors) (sort = true default)
  { VecT v(n); MatT V(n, n); A.eigh(v, V); rec.mat(v).mat(V); }
  rec.sep();
  // 8. A.eigh(values, vectors, false)
  { VecT v(n); MatT V(n, n); A.eigh(v, V, false); rec.mat(v).mat(V); }
  rec.emit();
}

template <int N> void eig_fixed(const MX & a) {
  using M = akantu::Matrix<Real, N, N>;
  M A = a;
  eig_record<M, akantu::Vector<Real, N>, Eigen::Matrix<C, N, N>, Eigen::Matrix<C, N, 1>>("F", N, A);
}

void eig_dynamic(const MX & a) {
  akantu::Matrix<Real> A = a;
  eig_record<akantu::Matrix<Real>, akantu::Vector<Real>, Eigen::MatrixXcd, Eigen::VectorXcd>("D", int(a.rows()), A);
}

/* ---------------------------------------------------- det / inverse ops */
template <int N> void detinv_fixed(const MX & a) {
  akantu::Matrix<Real, N, N> A = a;
  char head[64];
  std::snprintf(head, sizeof head, "detinv F %d", N);
  Rec rec(head);
  rec.sep().mat(A).sep().d(A.determinant()).sep();
  akantu::Matrix<Real, N, N> I = A.inverse();
  rec.mat(I).emit();
}

void detinv_dynamic(const MX & a) {
  akantu::Matrix<Real> A = a;
  char head[64];
  std::snprintf(head, sizeof head, "detinv D %d", int(a.rows()));
  Rec rec(head);
  rec.sep().mat(A).sep().d(A.determinant()).sep();
  akantu::Matrix<Real> I = A.inverse();
  rec.mat(I).emit();
}

void eig_any(const MX & a, bool fixed) {
  int n = int(a.rows());
  if (!fixed) { eig_dynamic(a); return; }
  switch (n) {
  case 1: eig_fixed<1>(a); break;
  case 2: eig_fixed<2>(a); break;
  case 3: eig_fixed<3>(a); break;
  case 4: eig_fixed<4>(a); break;
  case 5: eig_fixed<5>(a); break;
  case 6: eig_fixed<6>(a); break;
  case 8: eig_fixed<8>(a); break;
  case 9: eig_fixed<9>(a); break;
  default: break;
  }
}

void detinv_any(const MX & a, bool fixed) {
  int n = int(a.rows());
  if (!fixed) { detinv_dynamic(a); return; }
  switch (n) {
  case 1: detinv_fixed<1>(a); break;
  case 2: detinv_fixed<2>(a); break;
  case 3: detinv_fixed<3>(a); break;
  case 4: detinv_fixed<4>(a); break;
  case 5: detinv_fixed<5>(a); break;
  case 6: detinv_fixed<6>(a); break;
  case 8: detinv_fixed<8>(a); break;
  default: break;
  }
}

/* ------------------------------------------------------ products, norms */
template <int R, int Cc, int K> void matmul_fixed(Rng & r) {
  akantu::Matrix<Real, R, Cc> a;
  akantu::Matrix<Real, Cc, K> b;
  akantu::Matrix<Real, R, K> bt_lhs;  // for a^T b: (Cc x R)^T ... use at = R x Cc as lhs^T
  for (int j = 0; j < Cc; ++j) for (int i = 0; i < R; ++i) a(i, j) = r.uni() * std::pow(10.0, r.irange(-3, 3));
  for (int j = 0; j < K; ++j) for (int i = 0; i < Cc; ++i) b(i, j) = r.uni();
  (void)bt_lhs;
  char head[64];
  std::snprintf(head, sizeof head, "matmul F %d %d %d", R, Cc, K);
  Rec rec(head);
  akantu::Matrix<Real, R, K> c = a * b;
  rec.sep().mat(a).mat(b).sep().mat(c).emit();
  // transposed product: at^T * b' with at: Cc x R stored, (at^T: R x Cc)
  akantu::Matrix<Real, Cc, R> at = a.transpose();
  akantu::Matrix<Real, R, K> c2 = at.transpose() * b;
  std::snprintf(head, sizeof head, "trmatmul F %d %d %d", Cc, R, K);
  Rec rec2(head);
  rec2.sep().mat(at).mat(b).sep().mat(c2).emit();
}

void matmul_dynamic(Rng & r, int R, int Cc, int K) {
  akantu::Matrix<Real> a(R, Cc), b(Cc, K);
  for (int j = 0; j < Cc; ++j) for (int i = 0; i < R; ++i) a(i, j) = r.uni() * std::pow(10.0, r.irange(-3, 3));
  for (int j = 0; j < K; ++j) for (int i = 0; i < Cc; ++i) b(i, j) = r.uni();
  akantu::Matrix<Real> c = a * b;
  char head[64];
  std::snprintf(head, sizeof head, "matmul D %d %d %d", R, Cc, K);
  Rec rec(head);
  rec.sep().mat(a).mat(b).sep().mat(c).emit();
}

template <int R, int Cc> void reductions_fixed(Rng & r) {
  akantu::Matrix<Real, R, Cc> a, b;
  for (int j = 0; j < Cc; ++j)
    for (int i = 0; i < R; ++i) { a(i, j) = r.uni() * std::pow(10.0, r.irange(-4, 4)); b(i, j) = r.uni(); }
  char head[64];
  std::snprintf(head, sizeof head, "reduce F %d %d", R, Cc);
  Rec rec(head);
  rec.sep().mat(a).mat(b).sep().d(a.norm()).d(a.squaredNorm()).d(a.sum()).d(a.cwiseProduct(b).sum());
  if constexpr (R == Cc) rec.d(a.trace()).d(a.doubleDot(b)); else rec.d(0.0).d(0.0);
  rec.emit();
}

template <int N> void dot_fixed(Rng & r) {
  akantu::Vector<Real, N> a, b;
  for (int i = 0; i < N; ++i) { a(i) = r.uni() * std::pow(10.0, r.irange(-4, 4)); b(i) = r.uni(); }
  char head[64];
  std::snprintf(head, sizeof head, "dot F %d", N);
  Rec rec(head);
  rec.sep().mat(a).mat(b).sep().d(a.dot(b)).d(a.norm()).emit();
}

/* ------------------------------------------------------------------ Voigt */
template <int dim> void voigt(Rng & r, int nb_nodes) {
  using VH = akantu::VoigtHelper<dim>;
  constexpr int size = VH::size;
  akantu::Matrix<Real, dim, dim> m;
  for (int j = 0; j < dim; ++j) for (int i = 0; i < dim; ++i) m(i, j) = r.uni();
  akantu::Vector<Real, size> v1, v2;
  VH::matrixToVoigt(m, v1);
  VH::matrixToVoigtWithFactors(m, v2);
  akantu::Matrix<Real, dim, dim> back;
  VH::voigtToMatrix(v1, back);
  akantu::Matrix<Real> B(dim, nb_nodes), G(dim, dim);
  for (int j = 0; j < nb_nodes; ++j) for (int i = 0; i < dim; ++i) B(i, j) = r.uni();
  for (int j = 0; j < dim; ++j) for (int i = 0; i < dim; ++i) G(i, j) = r.uni();
  akantu::Matrix<Real> Bv(size, dim * nb_nodes), Bnl(dim * dim, dim * nb_nodes), Bl2(size, dim * nb_nodes);
  VH::transferBMatrixToSymVoigtBMatrix(B, Bv, nb_nodes);
  VH::transferBMatrixToBNL(B, Bnl, nb_nodes);
  VH::transferBMatrixToBL2(B, G, Bl2, nb_nodes);
  char head[64];
  std::snprintf(head, sizeof head, "voigt F %d %d", dim, nb_nodes);
  Rec rec(head);
  rec.sep().mat(m).mat(B).mat(G).sep().mat(v1).mat(v2).mat(back).mat(Bv).mat(Bnl).mat(Bl2).emit();
}

/* -------------------------------------------------------- complex division */
void cdiv(double a, double b, double c, double d) {
  C z = C(a, b) / C(c, d);
  Rec rec("cdiv D 1");
  rec.sep().d(a).d(b).d(c).d(d).sep().d(z.real()).d(z.imag()).emit();
}

}  // namespace

// --eig-large (review addendum): eig / eigh
// records of run-time-size matrices with 13 <= n <= 48, tie-free spectra at
// every size and tied spectra at n <= 16 only (the bitwise scope), with the
// D-050 cache-size pin. A separate RNG stream: the default records (the
// committed dense_strict.txt) are unchanged.
int eig_large() {
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  std::fprintf(stderr, "[eig-large] eigen cache sizes l1=%td l2=%td l3=%td\n",
               Eigen::l1CacheSize(), Eigen::l2CacheSize(), Eigen::l3CacheSize());
  Rng r{0x5eed0013ULL};
  struct Case { int family; int n; };
  // 2 symmetric, 13 SPD, 0 general (complex pairs likely), 5 complex
  // pairs: tie-free; 3 repeated eigenvalues, 8 diagonal ties with -0.0:
  // tied, n <= 16 only
  const Case cases[] = {{2, 13}, {2, 17}, {2, 24}, {2, 48}, {13, 33}, {0, 17}, {0, 40},
                        {5, 20}, {3, 13}, {3, 16}, {8, 16}};
  for (const auto & c : cases) eig_any(family(r, c.family, c.n), false);
  return 0;
}

int main(int argc, char ** argv) {
  if (argc > 1 && std::strcmp(argv[1], "--eig-large") == 0) return eig_large();
  Rng r{0x5eed0001ULL};

  // eigen-decompositions: fixed sizes (compile-time) and dynamic sizes
  const int fixed_sizes[] = {1, 2, 3, 4, 5, 6, 8, 9};
  const int dyn_sizes[] = {1, 2, 3, 4, 5, 6, 7, 8, 9, 12};
  for (int g = 0; g < nb_families; ++g) {
    int reps = (g == 3 || g == 5 || g == 12) ? 4 : 2;
    for (int rep = 0; rep < reps; ++rep) {
      // sizes >= 8 once per family (n = 12 only for a few families) to keep
      // the fixture small; every code path is still exercised.
      for (int n : fixed_sizes)
        if (n < 8 || rep == 0) eig_any(family(r, g, n), true);
      for (int n : dyn_sizes)
        if ((n < 8 || rep == 0) && (n != 12 || g == 0 || g == 2 || g == 3 || g == 5 || g == 13))
          eig_any(family(r, g, n), false);
    }
  }
  // upstream test_tensors MatrixEigs matrix
  {
    MX a(4, 4);
    a << 0, 1., 0, 0, 1., 0, 0, 0, 0, 1., 0, 1., 0, 0, 4., 0;
    eig_any(a, true);
    eig_any(a, false);
  }
  // Mazars-style plane strain: 3x3 with zero third row/column
  for (int rep = 0; rep < 8; ++rep) {
    MX e = MX::Zero(3, 3);
    double x = r.uni() * 1e-3, y = r.uni() * 1e-3, z = r.uni() * 1e-3;
    e(0, 0) = x; e(1, 1) = y; e(0, 1) = e(1, 0) = z;
    eig_any(e, true);
  }
  // isotropic / orthotropic Voigt stiffness (dynamic, as MaterialElasticLinearAnisotropic::C)
  for (int dim = 2; dim <= 3; ++dim) {
    int s = dim == 2 ? 3 : 6;
    for (int rep = 0; rep < 4; ++rep) {
      double E = 1e9 * (1.0 + std::fabs(r.uni())), nu = 0.3 * std::fabs(r.uni());
      double lambda = E * nu / ((1 + nu) * (1 - 2 * nu)), mu = E / (2 * (1 + nu));
      MX c = MX::Zero(s, s);
      for (int i = 0; i < dim; ++i) for (int j = 0; j < dim; ++j) c(i, j) = lambda + (i == j ? 2 * mu : 0.0);
      for (int i = dim; i < s; ++i) c(i, i) = mu * (rep % 2 ? 1.0 + 0.1 * r.uni() : 1.0);
      eig_any(c, false);
    }
  }

  // det / inverse
  const int fixed_inv[] = {1, 2, 3, 4, 5, 6, 8};
  for (int g = 0; g < nb_families; ++g)
    for (int rep = 0; rep < 3; ++rep) {
      for (int n : fixed_inv) detinv_any(family(r, g, n), true);
      for (int n = 1; n <= 9; ++n) detinv_any(family(r, g, n), false);
    }
  // exactly singular
  for (int n = 1; n <= 4; ++n) { detinv_any(MX::Zero(n, n), true); detinv_any(MX::Zero(n, n), false); }

  // products
  for (int rep = 0; rep < 6; ++rep) {
    matmul_fixed<1, 1, 1>(r); matmul_fixed<1, 3, 1>(r); matmul_fixed<3, 1, 3>(r);
    matmul_fixed<2, 2, 2>(r); matmul_fixed<3, 3, 3>(r); matmul_fixed<2, 3, 4>(r);
    matmul_fixed<3, 2, 4>(r); matmul_fixed<6, 6, 6>(r); matmul_fixed<6, 3, 6>(r);
    matmul_fixed<3, 6, 3>(r); matmul_fixed<1, 6, 6>(r); matmul_fixed<6, 6, 1>(r);
    matmul_fixed<8, 8, 1>(r); matmul_fixed<1, 8, 8>(r); matmul_fixed<9, 9, 1>(r);
    matmul_fixed<3, 9, 3>(r); matmul_fixed<4, 27, 1>(r); matmul_fixed<2, 30, 2>(r);
    matmul_fixed<8, 8, 8>(r); matmul_fixed<24, 6, 6>(r);
    const int dshapes[][3] = {{1, 1, 1}, {2, 2, 2}, {3, 3, 3}, {6, 6, 6}, {6, 7, 6}, {3, 2, 4},
                              {1, 9, 1}, {8, 8, 1}, {1, 8, 8}, {20, 20, 1}, {1, 30, 1},
                              {7, 7, 7}, {24, 6, 6}, {6, 24, 24}, {10, 10, 10}};
    for (auto & s : dshapes) matmul_dynamic(r, s[0], s[1], s[2]);
  }
  for (int rep = 0; rep < 6; ++rep) {
    reductions_fixed<1, 1>(r); reductions_fixed<2, 2>(r); reductions_fixed<3, 3>(r);
    reductions_fixed<2, 3>(r); reductions_fixed<6, 6>(r); reductions_fixed<4, 9>(r);
    dot_fixed<1>(r); dot_fixed<2>(r); dot_fixed<3>(r); dot_fixed<4>(r); dot_fixed<6>(r);
    dot_fixed<9>(r); dot_fixed<27>(r); dot_fixed<28>(r); dot_fixed<37>(r); dot_fixed<38>(r);
  }

  // Voigt helper
  for (int rep = 0; rep < 4; ++rep) {
    voigt<1>(r, 2); voigt<2>(r, 3); voigt<2>(r, 4); voigt<3>(r, 4); voigt<3>(r, 8);
  }

  // complex division (incl. scaling branches and special values)
  const double sp[] = {0.0, -0.0, 1.0, -1.0, 1e-300, 1e300, 1e-310, 1.7e308, 0.5, 3.0};
  for (double a : sp) for (double c : sp) cdiv(a, 1.0, c, 2.0), cdiv(1.0, a, 2.0, c);
  for (int k = 0; k < 400; ++k) {
    double a = r.uni() * std::pow(10.0, r.irange(-12, 12)), b = r.uni() * std::pow(10.0, r.irange(-12, 12));
    double c = r.uni() * std::pow(10.0, r.irange(-12, 12)), d = r.uni() * std::pow(10.0, r.irange(-12, 12));
    cdiv(a, b, c, d);
  }
  cdiv(1.0, 1.0, 0.0, 0.0);
  cdiv(INFINITY, 1.0, 1.0, 1.0);
  cdiv(1.0, 1.0, INFINITY, 1.0);
  return 0;
}
