# Dense-algebra parity probe

`probe.cc` calls the **real upstream code** — Akantu's `aka_types.hh`
(`MatrixBase::eig`/`eigh` overloads, `determinant`, `inverse`, products,
`norm`, `trace`, `doubleDot`), `aka_voigthelper.hh` and Eigen 3.4.0 — from the
oracle install, and prints bit-exact results. `run.sh` builds it inside
`akantu-oracle:dev` (read-only mount of the `akantu-oracle-build` volume, no
Akantu rebuild) in three configurations and writes:

* `crates/akantu-linalg/tests/fixtures/dense/dense_strict.txt` — the parity
  fixture (strict oracle flags `-O2 -DNDEBUG -ffp-contract=off
  -DEIGEN_DONT_VECTORIZE`, the flags of the `strict` oracle library);
* `crates/akantu-linalg/tests/fixtures/dense/dense_config_comparison.txt` —
  how many records differ from strict when the same program is built with
  Eigen's NEON kernels (`-ffp-contract=off`) or with upstream's default
  flags (contraction + NEON FMA).

```sh
oracle/probes/dense/run.sh     # ~4 min, ~1 GB RAM, from the repository root
cargo test -p akantu-linalg --test dense_parity
```

## Record format

One record per line, fields separated by ` | `; doubles are the 16 hex
digits of their IEEE-754 bit pattern (column-major for matrices); `EXC`
marks an upstream exception.

| op | head | input | outputs |
|---|---|---|---|
| `eig` | `eig F\|D n` | `A` | `A.eig<complex>()` (re, im pairs) · `A.eig(v)` · `A.eig(v, V)` (sort=true; or `EXC`) · `A.eig(v, V, false)` (or `EXC`) · `EigenSolver(A, true)` values + complex vectors · `A.eigh(v)` · `A.eigh(v, V)` · `A.eigh(v, V, false)` |
| `detinv` | `detinv F\|D n` | `A` | `A.determinant()` · `A.inverse()` |
| `matmul` | `matmul F\|D R C K` | `a (R×C)`, `b (C×K)` | `a * b` |
| `trmatmul` | `trmatmul F C R K` | `at (C×R)`, `b (C×K)` | `at.transpose() * b` |
| `reduce` | `reduce F R C` | `a`, `b` | `a.norm()`, `a.squaredNorm()`, `a.sum()`, `a.cwiseProduct(b).sum()`, `a.trace()`, `a.doubleDot(b)` (last two 0 if not square) |
| `dot` | `dot F N` | `a`, `b` | `a.dot(b)`, `a.norm()` |
| `voigt` | `voigt F dim nb` | `m (dim×dim)`, `B (dim×nb)`, `G (dim×dim)` | `matrixToVoigt(m)`, `matrixToVoigtWithFactors(m)`, `voigtToMatrix(v)`, `transferBMatrixToSymVoigtBMatrix`, `transferBMatrixToBNL`, `transferBMatrixToBL2` |
| `cdiv` | `cdiv D 1` | `a b c d` | `std::complex(a,b) / std::complex(c,d)` |

`F` = compile-time size (`akantu::Matrix<Real, n, n>`), `D` = run-time size
(`akantu::Matrix<Real>`).

Upstream's complex-output `A.eig(values, vectors, false)` does not compile
(the sort comparator is instantiated for `std::complex`), so the complex
eigenvectors are taken from `Eigen::EigenSolver` directly, exactly as that
overload would.

## Matrix families (`family(g, n)`)

0 uniform `[-1, 1)` · 1 wide exponent range `10^[-8, 8]` · 2 symmetric ·
3 symmetric with repeated/clustered eigenvalues (`Q diag(λ) Qᵀ`) · 4 near
singular (rank `n−1` + `1e-14` noise) · 5 non-symmetric with complex
conjugate pairs · 6 defective (Jordan block) · 7 small integers · 8 diagonal
with ties and `-0.0` · 9 sparse with exact `±0.0` · 10 upper triangular ·
11 graded rows · 12 nearly symmetric (`1e-15` asymmetry) · 13 SPD
(stiffness-like). Plus: upstream `test_tensors.cc` `MatrixEigs` 4×4,
Mazars-style plane-strain 3×3 tensors, isotropic/orthotropic Voigt stiffness
(dynamic 3×3 / 6×6), exactly singular matrices, products of every Eigen
product kind (coefficient-based, GEMV, GEMM), complex divisions hitting every
scaling branch and the special-value recovery.

Deterministic: splitmix64 seeded with `0x5eed0001`.

## Product probe (`gemm_probe.cc`, decision D-036)

`gemm_probe.cc` evaluates the products upstream routes through Eigen's GEMM
machinery with the **expressions and operand types of the upstream
kernels** (dynamic `Map`s from `make_view`, `.noalias()` assignments), plus
plain products at the `gebp` boundaries. `run_gemm.sh` builds it against the
strict install only (`-ffp-contract=off -DEIGEN_DONT_VECTORIZE`, ~30 s) and
writes `crates/akantu-linalg/tests/fixtures/dense/dense_gemm_strict.txt`
(812 records, seed `0x5eed0002`; ~4 min since the fix added the
mixed-extent families), checked bit for bit by
`crates/akantu-linalg/tests/dense_gemm_parity.rs`. New families are
appended at the end of `main()` so that earlier records keep their random
inputs (the first 373 records are unchanged since the GEMM translation).

```sh
oracle/probes/dense/run_gemm.sh
cargo test -p akantu-linalg --test dense_gemm_parity
```

| op | head | input | output | upstream expression |
|---|---|---|---|---|
| `btdb4` | `btdb4 D dim nnodes elem` | `Bfull (dim×nnodes)`, `D (voigt×voigt)` | `Bt_D_B` | `shape_lagrange_inline_impl.hh:451`: `B` from `transferBMatrixToSymVoigtBMatrix`, `Bt_D_B.noalias() = B.transpose() * D * B` |
| `btdb2` | `btdb2 D dim nnodes elem` | `B (dim×nnodes)`, `D (dim×dim)` | `Bt_D_B` | `shape_lagrange_inline_impl.hh:460` |
| `grad` | `grad M dim nnodes dof elem` | `u (dof×nnodes)`, `B (dim×nnodes)` | `nabla_u (dof×dim)` | `shape_functions_inline_impl.hh:303`: `u` is `Map<Matrix<Dynamic, nnodes>>`, `B` `Map<const Matrix<dim, nnodes>>` (mixed compile-time/run-time sizes) |
| `ntbn` | `ntbn D S elem` | `N (1×S)`, `b (1×1)` | `Nt_b_N (S×S)` | `shape_lagrange_inline_impl.hh:497` (only `nb_dof = 1` is well-formed upstream) |
| `matmul` | `matmul F\|D R C K` | `a`, `b` | `a * b` | as `probe.cc`; also structural `N * T`, `B * R` sizes |
| `trmatmul` | `trmatmul F\|D C R K` | `at (C×R)`, `b` | `at.transpose() * b` | row-major left operand |
| `aniso` | `aniso M dim` | `C (v×v)`, `voigt_strain (v)` | `voigt_stress(I)`, `I < v` | `material_elastic_linear_anisotropic_inline_impl.hh:38-40`: `C` is `Matrix<Real>` (dynamic), the strain the `Vector<Real, v>` of `VoigtHelper<dim>::matrixToVoigtWithFactors`; `auto voigt_stress = C * voigt_strain` read coefficient-wise like `voigtToMatrix` |
| `anisorot` | `anisorot M dim` | `rotator`, `Cprime`, `revrotor` (each `D²×D²`) | `Cfull` | `material_elastic_linear_anisotropic.cc:147-149`: `Cfull = rotator * Cprime * revrotor` with `Matrix<Real, D², D²>`, `Matrix<Real>`, `Matrix<Real, D², D²>` |
| `mixed` | `mixed M <code> R K C` | `a (R×K)`, `b (K×C)` | `a * b` | `<code>`: four letters `F` (compile-time) / `D` (`Eigen::Dynamic`) for lhs rows, lhs cols, rhs rows, rhs cols of the `akantu::Matrix<Real, ·, ·>` operand types; result in the product's `PlainObject` |
| `trmixed` | `trmixed M <code> R K C` | `at (K×R)`, `b (K×C)` | `at.transpose() * b` | `<code>` describes the product operands (`at.transpose()`, `b`) |
| `mixedtr` | `mixedtr M <code> R K C` | `a (R×K)`, `bt (C×K)` | `a * bt.transpose()` | `<code>` describes the product operands (`a`, `bt.transpose()`) |

Coverage: the 12 Lagrange shapes (`_segment_2` … `_hexahedron_20`) for both
`computeBtDB` orders (one of three repetitions with exact `±0` entries and
negative `D`, so that coefficient-based results keep `-0` while `gebp`/GEMV
results start from `+0`), gradients for 2…20 nodes and a synthetic 27-node
shape with `dof ∈ {1, 2, 3}`, depth 8–16 with odd rows and 4…13 columns
(dynamic and fixed), depth blocking at 407/408/409/816/817/1000 (dynamic)
and 409/817 (fixed, never blocked), row blocking (`max ≥ 48`), column
blocking (`nc < cols`), GEMV column/row/inner fallbacks at depths
20/127/128/129/200/409. Fix (mixed compile-time/run-time
operands, `docs/NUMERICAL_VALIDATION.md` item 3): the anisotropic
material's `C * voigt_strain` (dim 1–3, one repetition with exact `±0`)
and rotation `rotator * Cprime * revrotor`; all 16 `F`/`D` combinations of
the four operand extents for 17 shapes across the inner / outer /
coefficient-based (depths 7/8, unrolling limit 27/28, `rows + cols +
depth < 20`) / GEMV / gebp boundaries, 4 of them repeated with exact `±0`,
3 + 2 shapes with a transposed lhs / rhs, and 5 deep records (fixed depth
409 with a run-time result extent: depth-blocked; fixed depth 129: column
GEMV, `GemmProduct` → GEMV, coefficient-based). Comment lines (`#`) label
the groups.

## Eigen-decompositions for `13 ≤ n ≤ 48` (`probe.cc --eig-large`)

The D-065 addendum: `run_eig_large.sh` builds the same
probe and runs it with `--eig-large` (Eigen cache sizes pinned, D-050; three
identical runs), writing
`crates/akantu-linalg/tests/fixtures/dense/dense_eig_large_strict.txt`:
dynamic `eig` records (the format above) of tie-free symmetric (n = 13, 17,
24, 48), SPD (33), general (17, 40) and complex-pair (20) matrices and of
tied spectra at `n ≤ 16` (repeated eigenvalues 13, 16; diagonal ties with
`-0.0` at 16). The committed file was generated on the **x86_64** oracle
(D-043): its complex eigenvectors follow the x86_64 runtime's complex
division, which `dense_parity.rs` selects with
`ComplexDivision::Unfused` (the clean-room experiments for it:
`complex_cleanroom/run_x86_64.sh`, `COMPLEX_CLEANROOM_LOG.md` §8).

```sh
# on an x86_64 host with the oracle image (AKANTU_SRC_HOST not needed)
oracle/probes/dense/run_eig_large.sh
```
