// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs oracle tooling (not ported from Akantu): FP contraction probe, see README.md.
#include <Eigen/Dense>
using M3 = Eigen::Matrix<double, 3, 3>;
using MX = Eigen::Matrix<double, Eigen::Dynamic, Eigen::Dynamic>;
void prod3(const M3 & a, const M3 & b, M3 & c) { c.noalias() = a * b; }
void prodx(const MX & a, const MX & b, MX & c) { c.noalias() = a * b; }
double dot(const Eigen::VectorXd & a, const Eigen::VectorXd & b) { return a.dot(b); }
void gemv(const MX & a, const Eigen::VectorXd & x, Eigen::VectorXd & y) { y.noalias() += a * x; }
