<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Small dense products and reductions in Eigen's order

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

Machine: a Google Cloud VM, AMD EPYC 9B45 (Zen 5, 8 cores, one thread per
core), Ubuntu 24.04, the agent's own machine (campaign rules 3d). Release
profile, one process per run, `scripts/perf/suite/run_ab.sh` (variants
interleaved, order rotated per repetition), 5 repetitions, size `m`;
median and the paired ratio per repetition, median [min, max]. "Before" =
the suite binaries of `perf/wave2-base` 59e05aa3f built on the machine;
"after" = 184dbb6e5 (the later commits change documents only). The solid
series ran pinned to core 2 while the model series ran pinned to core 6
(two series at the same time; the three 3-repetition screens in
`screens/`, one series at a time, give the same ratios within their
ranges). Date 2026-11-16, 11:02–11:17 UTC. Raw JSON lines and run metadata:
`final/` (tables `final/solid.md`, `final/models.md`), screens of the
intermediate commits `screens/ab1` (585cdaa67, 66a3ee3fd), `ab2`
(967d847e9), `ab3` (083a3ac6a, d7b9583ac). A range that contains 1 is no
measurable change.

| case | phase | before | after | after / before | digest |
|---|---|---|---|---|---|
| setup_tet4 | mass_consistent | 3.791 s | 2.047 s | 0.571 [0.525, 0.590] | same |
| setup_tet4 | stiffness_assembly | 2.400 s | 2.239 s | 0.898 [0.828, 1.019] | same |
| setup_tet4 | shape_functions | 180.66 ms | 89.20 ms | 0.473 [0.467, 0.517] | same |
| setup_tet10 | mass_consistent | 3.337 s | 1.217 s | 0.348 [0.322, 0.486] | same |
| setup_tet10 | stiffness_assembly | 2.006 s | 1.482 s | 0.816 [0.628, 1.010] | same |
| setup_tet10 | shape_functions | 83.58 ms | 30.57 ms | 0.378 [0.348, 0.385] | same |
| setup_hex8 | mass_consistent | 4.616 s | 1.034 s | 0.224 [0.209, 0.295] | same |
| setup_hex8 | stiffness_assembly | 2.433 s | 1.713 s | 0.691 [0.612, 0.791] | same |
| setup_hex8 | shape_functions | 169.64 ms | 65.44 ms | 0.392 [0.341, 0.408] | same |
| implicit_dynamic | mass_consistent | 162.34 ms | 91.58 ms | 0.585 [0.468, 0.671] | same |
| implicit_dynamic | solve_step | 2.236 s | 2.236 s | 1.000 [0.961, 1.036] | same |
| explicit_tet4_ss | solve_step | 2.172 s | 2.139 s | 0.958 [0.914, 1.018] | same |
| explicit_tet4_ss | energies | 505.17 ms | 227.98 ms | 0.442 [0.408, 0.502] | same |
| explicit_hex8_ss | solve_step | 2.544 s | 2.574 s | 1.014 [0.957, 1.107] | same |
| explicit_hex8_ss | energies | 618.26 ms | 413.51 ms | 0.659 [0.644, 0.692] | same |
| explicit_hex8_ss | init | 619.98 ms | 311.15 ms | 0.492 [0.461, 0.580] | same |
| explicit_marigo | solve_step | 3.457 s | 2.417 s | 0.690 [0.666, 0.735] | same |
| explicit_marigo | energies | 227.65 ms | 69.01 ms | 0.306 [0.279, 0.350] | same |
| explicit_maxwell | solve_step | 4.382 s | 3.600 s | 0.814 [0.796, 0.839] | same |
| explicit_plastic | solve_step | 3.853 s | 3.006 s | 0.786 [0.755, 0.806] | same |
| explicit_neohookean | solve_step | 2.407 s | 1.743 s | 0.710 [0.691, 0.750] | same |
| explicit_orthotropic | solve_step | 2.090 s | 1.985 s | 0.950 [0.898, 1.014] | same |
| explicit_sls | solve_step | 3.134 s | 2.837 s | 0.925 [0.898, 0.964] | same |
| explicit_mazars | solve_step | 5.949 s | 5.556 s | 0.914 [0.899, 0.969] | same |
| cohesive_extrinsic_check | check_insertion | 3.969 s | 3.108 s | 0.783 [0.717, 0.841] | same |
| cohesive_extrinsic_check | solve_step | 940.18 ms | 983.30 ms | 1.041 [0.993, 1.119] | same |
| cohesive_intrinsic_bilinear | solve_step | 5.197 s | 3.473 s | 0.662 [0.635, 0.683] | same |
| structural_dkt18_plate_static | assemble_stiffness | 732.82 ms | 478.98 ms | 0.691 [0.614, 0.898] | same |
| structural_dkt18_plate_static | solve_step | 1.982 s | 1.937 s | 0.996 [0.937, 1.056] | same |
| structural_beam3_frame_static | assemble_stiffness | 208.73 ms | 152.52 ms | 0.721 [0.662, 0.809] | same |
| contact_search_quad4 | search | 4.478 s | 4.151 s | 0.916 [0.898, 0.959] | same |
| contact_search_tet4 | search | 4.217 s | 3.343 s | 0.801 [0.776, 0.813] | same |
| contact_cohesive_quad4 | solve_step | 7.352 s | 5.957 s | 0.790 [0.787, 0.841] | same |
| heat_explicit_tet4 | solve_step | 3.077 s | 2.279 s | 0.741 [0.720, 0.778] | same |
| heat_explicit_quad4 | solve_step | 3.645 s | 2.713 s | 0.756 [0.715, 0.777] | same |
| embedded_rebar_tri3 | assemble_stiffness | 204.22 ms | 198.45 ms | 1.011 [0.780, 1.199] | same |
| embedded_rebar_tri3 | solve_step | 404.59 ms | 433.62 ms | 1.108 [0.791, 1.221] | same |

Not affected (ranges contain 1): the explicit elastic steps (they already
used the `fast::` kernels), the sparse solves, the embedded case.

### Share of the profile in the product route

`perf record --call-graph dwarf` of the same binaries
(`profiles/before/`, `profiles/after/`, `profiles/generic-shares.txt`):
self time of the generic route (`eigen_gebp`, `eigen_order`,
`akantu_fe::eigen::product*`) and of the new fixed-order kernels
(`eigen_kernels`, `fast::integrate_scalar`). The reductions inlined into
the laws (`double_dot`, `product_coeff`, `trace`) no longer appear as
symbols of their own; their cost is now part of the calling law
(`maxwell::scaled_matvec` 40 % of `explicit_maxwell`, `double_dot` 12 % of
`explicit_marigo`).

| case | generic route before | generic route after | fixed kernels after |
|---|---|---|---|
| setup_hex8 | 44.9 % | 0.0 % | 14.5 % |
| setup_tet10 | 33.1 % | 0.0 % | 7.8 % |
| explicit_marigo | 34.3 % | 0.0 % (inlined) | 0.3 % |
| explicit_maxwell | 49.0 % | 0.0 % (inlined) | 0.1 % |
| explicit_hex8_ss | 11.2 % | 0.0 % | 1.3 % |
| cohesive_extrinsic_check | 27.6 % | 2.5 % | 13.4 % |
| structural_dkt18_plate_static | 28.4 % | 0.5 % | 14.2 % |
| structural_beam3_frame_static | 14.1 % | 0.3 % | 7.9 % |
| contact_search_quad4 | 22.1 % | 7.7 % | 5.8 % |
| contact_search_tet4 | 31.1 % | 9.8 % | 6.8 % |
| heat_explicit_tet4 | 38.1 % | 0.0 % | 11.8 % |
| contact_cohesive_quad4 | 44.4 % | 8.1 % | 22.7 % |

What is left in the generic route: the one-shot products of
`akantu-contact` (`eigen_ops::matmul`, `eigen_ops::gemv` through
`product_into`, the FE kernels called per projection) and the GEMV of the
cohesive insertion check. What is left in the fixed kernels: the products
themselves (memory-bound column loops for the mass and `BᵀDB`, the
per-element interpolations of heat and cohesive).

### Fast mode (numbers first)

`examples/small_products` (akantu-core), pinned to one core, median of 9
runs, ns per product; `micro-small-products.md`. Micro-benchmarks only
size the fast-mode question; the suite cases above are the evidence for the
default path.

| class | route | general (ns) | slice kernel (ns) | fast mode (ns) | fast / slice |
|---|---|---|---|---|---|
| BtD 30x6x6 (tet10 stiffness) | Gebp { kc: 6 } | 1384.8 | 301.0 | 386.1 | 1.28 |
| BtDB 30x6x30 | Gebp { kc: 6 } | 7178.1 | 1330.4 | 2085.9 | 1.57 |
| NtN 30x3x30 (tet10 mass) | Gebp { kc: 3 } | 3761.7 | 748.3 | 1334.1 | 1.78 |
| BtDB 18x3x18 (DKT18) | Gebp { kc: 3 } | 1312.2 | 339.8 | 510.8 | 1.50 |
| J 3x10x3 (tet10 Jacobian, tree) | Coeff(Tree) | 220.9 | 40.4 | 54.6 | 1.35 |
| C eps 6x6x1 (anisotropic) | Coeff(Sequential) | 46.3 | 18.7 | 68.2 | 3.64 |
| D grad 3x3x1 (heat) | Coeff(Sequential) | 15.9 | 11.5 | 12.1 | 1.06 |

The exact-order slice kernels are 3.8–5.4× faster than the general closure
route on the FE classes. The order-free variants (`fast_mode`: dot products
with four partial sums) are slower than them: the exact kernels already
run independent coefficients of a result column together (vectorised
across rows), which is what reassociation would buy, and the depth of these
products (3–10) is too short for partial sums of one dot product to pay.
A fast mode would gain only with FMA, which needs the `fma` target feature
(run-time dispatch with `#[target_feature]` and `unsafe` calls, not in
these `forbid(unsafe_code)` crates); expected at most about 1.5× on the
multiply-add count of the column kernels.

## Candidates left

Bitwise-safe, by share:

1. 3×3 eigenvalues for Mazars (`akantu-linalg` `real_schur`,
   `apply_householder_*`, the 168-byte `Buf` moves): about 57 % of
   `explicit_mazars` (real_schur 25 %, memmove 13 %, Householder 13 %,
   `Buf::push` 4 %, `make_householder` 3 %). A fixed 3×3 path that keeps
   the Hessenberg reduction's and the Francis iteration's operations
   (including the Eigen-order products of the Householder applications);
   `akantu-linalg` is outside this task's files.
2. Call sites in the laws that still pass a run-time length to the tree
   (`maxwell.rs:193`, `:202` `scaled_matvec` / `matvec`, 40 % of
   `explicit_maxwell`; `heat_diffusion.rs:386` `D∇u`, 9 % of
   `heat_explicit_tet4`; `anisotropic.rs:151` `C·ε`): match the Voigt size
   once and call a const-size kernel (`eigen_order::redux_tree` with a
   literal length folds to straight-line code), or
   `eigen_kernels::SliceProduct` prepared once per law.
3. `akantu-contact` one-shot products (`eigen_ops::matmul` 1×2×2 … 2×3×3,
   `eigen_ops::gemv`, the projections' `kernels::jmat` /
   `gradient_natural`): 8–10 % generic + 6–7 % kernels of
   `contact_search_*`: prepare per facet type, or fixed 2-D/3-D kernels.
4. Hand-written AVX2 / AVX-512 column kernels (`ChainColumns`,
   `SeqColumns`): lanes are independent coefficients with separate
   multiply and add, so bitwise; the x86-64 baseline build uses 2-lane SSE2.
   7–15 % of the set-up, structural and heat cases.
5. Heat explicit: the per-element interpolation (`coeff_seq_unrolled`,
   7 % of `heat_explicit_tet4`) as a batch kernel next to `fast::btd`.

Fast mode: FMA in the column kernels (≤ 1.5× on their share), see above;
closed-form 3×3 eigenvalues for Mazars (not bitwise).

