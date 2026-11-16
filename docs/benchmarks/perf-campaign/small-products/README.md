<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Small dense products and reductions in Eigen's order (W2-GEBP)

Branch `perf/small-products` (from `perf/wave2-base` 59e05aa3f). Every
change keeps every bit of the default path (D-122): the kernels perform the
additions of the general evaluation on the same operands in the same
order; the suite digests of all 71 runnable cases are unchanged
(`digests-s.txt`), and so are those of every timed case at size `m`
(tables below, column "digest").

## What changed

| Change | Where | Order argument |
|---|---|---|
| Halving-tree reduction without recursion (six levels of always-inlined functions up to 64 terms, recursion above) | `akantu-core/src/eigen_order.rs` `redux_tree` | same expression tree `op(left half, right half)`; the recursive definition stays as `redux_tree_recursive` |
| Route chosen once per product, at compile time for fixed-size tensors (`route` is a `const fn`; `eval_routed`) | `eigen_gebp.rs`, `tensor.rs` | the route function is unchanged; only where it is evaluated |
| `SliceProduct`: route, strides and kernel decided once; kernels per route: halving tree (one kernel per depth ≤ 27), sequential (column kernel from 4 rows, unrolled ≤ 8, loop), chain from `+0` for `Gebp` with depth < 8 and unblocked `Gemv` (column kernel, unrolled ≤ 7, loop); other routes: the general evaluation | `akantu-core/src/eigen_kernels.rs` | per coefficient the general evaluation's additions; loops over independent coefficients reordered |
| FE per-element loops prepare their products once (`akantu_fe::eigen::prepare`) | `akantu-fe` engine `ops.rs`, `mod.rs` (Jacobians, shape derivatives), `kernels.rs`, `structural/engine.rs` | same products |
| Consistent mass: each element's point matrices integrated as they are computed (memory `nq·(nb_dof·nn)²` instead of `ne·nq·(nb_dof·nn)²`); a point matrix is reused while the density bits repeat | `akantu-fe/src/engine/assembly.rs` | same products and per-element integration; a point matrix depends only on the shapes at the point and on `ρ` |
| Batch integration of scalar fields (the `1 × 1` inner-product fallback) | `akantu-fe/src/engine/fast.rs` `integrate_scalar` | sequential order, then `0 + r` |
| Natural derivatives of the Jacobian loop computed once per rule point | `akantu-fe/src/engine/mod.rs` `compute_jacobians` | same values |

Tests: `eigen_order` levelled tree against the recursive definition (all
lengths 1–80, `+` and `·`); `eigen_gebp` inlined routes and compile-time
tensor routes against a reference evaluation with the recursive trees
(every shape class to 9 × 9 × 9, all compile-time/run-time flags, both
storage orders); `eigen_kernels` every kernel and route against
`product_into` (classes to 12 × 12 × 12, the FE classes up to 60 rows,
GEMV depths around the 128 block limit, 2 000 draws per hot class);
`fast::integrate_scalar` against `product_into`; the mass reuse with
alternating densities, `±0` and a payload NaN. Inputs: signed zeros,
infinities, default NaNs of both signs, subnormals, extremes, exact
cancellation partners, arbitrary bit patterns, and a payload mode (one NaN
with a payload among finite values, compared bit for bit). Comparison rule
D-046/D-051 (identical bits, or two default NaNs that differ only in the
sign: when two NaNs meet in one addition IEEE 754 leaves the result's
payload unspecified and the compiler may commute the operands).

## Size classes

`size-classes.tsv`: the calls of the generic entry points (products of
`eigen_gebp::eval_product`, `eigen_order::{sum, prod, product_coeff}`)
counted per (size, route, compile-time extents, storage orders, call site)
by an instrumented build (not committed) over 38 cases of the three suites
at size `m`, one repetition; `size-classes-per-case.tsv` per case. The top
of the list (calls in millions):

| calls | class | call site | kernel now |
|---|---|---|---|
| 668 | `sum` of 2–3 terms, fixed (trace) | `akantu-solid` `elastic.rs:323` | inlined tree (core change) |
| 246 | `product_coeff`, depth 2–3, fixed lhs | `akantu-diffusion` `heat_diffusion.rs:386` (`D∇u`) | inlined tree (core change); call site not moved (law) |
| 191 | `product_coeff`, depth 6, fixed lhs | `akantu-solid` `maxwell.rs:193`, `:202` | inlined tree; call site not moved (law) |
| 236 | `sum` of 9, fixed (double dot) | `elastic.rs:212`, `damage.rs:464`, `plastic.rs:353`, `marigo.rs:153`, ... | inlined tree |
| 222 | `squared_norm` of 2–3 | `akantu-fe` `eigen.rs:261` | inlined |
| 54 | `1 × 1 × 1` `Dot(Sequential)` | `akantu-fe` `ops.rs` integration of scalar fields | `fast::integrate_scalar` |
| 43 | `3 × 3 × 2` `Coeff(Tree)` | `ops.rs` generic gradient | `SliceProduct` tree, prepared |
| 30 | `6 × 6 × 1` `Coeff(Sequential)` | `akantu-solid` `anisotropic.rs:151` | general (law, not moved) |
| 29–20 | `1 × 4 × 1`, `1 × 4 × 4` `Coeff(Sequential)` | `ops.rs` interpolation | `SliceProduct` unrolled, prepared |
| 18 | `3 × 3 × 3` `Coeff(Tree)` fixed | `akantu-solid` `material_tensor.rs:127` | compile-time route |
| 13–4 | `3 × nn × 3` tree / sequential, `3 × 3 × nn` | `kernels.rs` Jacobians and shape derivatives | `SliceProduct`, prepared |
| 4–2 | `12 × 3 × 12`, `24 × 3 × 24`, `30 × 3 × 30` `Gebp{kc: 3}` | consistent mass | chain column kernel, reuse |
| 2–1 | `n × 6 × 6`, `n × 6 × n` `Gebp{kc: 6}` | stiffness `BᵀDB` | chain kernels, prepared |

## Measurements

RESULTS_PLACEHOLDER
