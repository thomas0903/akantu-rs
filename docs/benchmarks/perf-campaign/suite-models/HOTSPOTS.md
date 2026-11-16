# Model suite: where the time goes

Ranked hot spots of the model suite (`suite_models`, crates/akantu-bench,
README section "Model suite"): heat transfer, phase field, contact,
structural mechanics, embedded reinforcement and node-to-node contact.
No optimisation was made; this list is the input of the next tasks.

- Baseline (seconds per phase, size `m`, 5 repetitions): `baseline/`,
  table in `baseline/README.md`. Machine: an AWS `m7a.2xlarge` (AMD EPYC
  9R14, 8 cores, 32 GiB), one process per run pinned to one core.
- Profiles: `profiles/<case>.txt`, `perf record --call-graph dwarf` at size
  `m` with `--loop-seconds 6` (every loop phase extended to at least 6 s,
  so the shares of a profile weigh the phases about equally, not as in
  the baseline). They were recorded in a 4-vCPU development VM (AMD Ryzen
  7 9700X): shares only, never times. Shares below are of the whole
  profile unless a phase is named.
- `libc.so.6` frames are unresolved (no glibc symbols in the VM); their
  callers (`ArrayStorage::try_resize`, `Vec` collection, copies) say they
  are `memset` / `memmove` of temporary arrays.
- Symbol names of generic kernels can be folded by the linker:
  `fast::gradient::<Cohesive3d8>` in 2-D quadrangle cases is the
  `Quadrangle4` instance (identical code), `<Cohesive3d6>` the
  `Triangle3` one.

## Baseline at a glance

Per call, median of five runs (`baseline/README.md`):

| Case | Dominant phase | Per call | Unit cost |
|---|---|---|---|
| `heat_explicit_tet4` | explicit step | 40.2 ms | 144 ns per element |
| `heat_explicit_quad4` | explicit step | 39.8 ms | 249 ns per element |
| `heat_implicit_hex8_kvar` | backward-Euler step (5 Newton iterations) | 272 ms | 124 µs per DOF |
| `heat_static_tri6` | static solve / assembly | 4.52 s / 45 ms | 93 µs per DOF / 1.9 µs per element |
| `phasefield_staggered_tri3` | solid solve / phase solve | 2.85 s / 0.53 s | 37 / 6.8 µs per DOF |
| `phasefield_staggered_tet4` | solid solve | 0.56 s | 105 µs per DOF |
| `contact_search_quad4` / `_tet4` | detection | 90 ms / 170 ms | 643 / 355 ns per element |
| `contact_explicit_quad4` | coupled explicit step | 24.4 ms | 194 ns per element |
| `contact_implicit_static_quad4` | static Newton step | 433 ms | 29 µs per DOF |
| `contact_cohesive_quad4` | explicit step with insertion check | 29.8 ms | 465 ns per element |
| `structural_beam3_frame_static` | static solve / assembly | 5.87 s / 38 ms | 445 µs per DOF / 6.3 µs per element |
| `structural_beam3_frame_dynamic` | Newmark step | 87 ms | 6.6 µs per DOF |
| `structural_dkt18_plate_static` | static solve / assembly | 2.65 s / 155 ms | 119 µs per DOF / 21.5 µs per element |
| `embedded_rebar_tri3` / `_tet4` | intersection set-up | 1.56 s / 1.40 s | 62 / 65 µs per element |
| `ntn_friction_quad4` | explicit step | 38.0 ms | 190 ns per element |

## Ranked list

Rank = share × expected gain × how common the path is. "Bitwise-safe" and
"fast mode" follow `docs/DECISIONS.md` D-122.

| # | Hot spot | Where (cases, share) | Reading | Fix and expected gain | Kind |
|---|---|---|---|---|---|
| 1 | Sparse assembly through a SipHash `HashMap` per entry: `SparseMatrixAij::resolve_block` (crates/akantu-linalg/src/sparse.rs:597, map at :239) with `DefaultHasher::write`, `hash_one<(i64,i64)>`, `add_values` (:527), `DofManager::element_equations` (crates/akantu-solver/src/dof_manager.rs:1739) | every matrix assembly: `heat_static_tri6` assemble 50 % (resolve 35 + hash 7.6 + add 4.7 + equations 2.7), `structural_beam3_frame_static` 44 %, `structural_dkt18_plate_static` 39 %, `phasefield_*` 30–40 %, `embedded_*` 15–16 %, `contact_implicit_static_quad4` 21 % (with `add_to_profile`) | repeated set-up work per entry: the (row, column) → position map is constant while the profile release does not change, yet every elemental entry pays a hash of two `i64` and a probe | per element block, cache the positions (one `Vec<usize>` per type and matrix profile release) and accumulate `a[k] += v` in the same order; or an `FxHash`-type hasher (new dependency) | bitwise-safe; assembly 2–3× (it is 40–50 % of an assembly), all implicit models |
| 2 | Supernodal sparse LU (`akantu_linalg::eigen_sparse_lu`): `dense::sparselu_gemm` (dense.rs:539) 11–32 %, `trsm_unit_lower`, `kernels::panel_bmod`, `dfs_kernel`, `copy_to_ucol` | every solve with the default `_auto` solver (Eigen SparseLU port): `heat_implicit_hex8_kvar` 65 %, `phasefield_staggered_tri3` solid solve (dominant phase), `phasefield_staggered_tet4`, `heat_static_tri6`, `structural_beam3_frame_static`, `contact_implicit_static_quad4` 38 % | kernel efficiency: `sparselu_gemm` is a scalar triple loop with bounds checks (`RK = 2` unrolled `k`); baseline: the 3-D frame (13.2 k DOFs) 5.87 s for one static solve, the 2-D tri6 heat problem (48.8 k DOFs) 4.52 s, the 2-D phase-field solid solve (51.8 k DOFs) 2.85 s, the 3-D tet4 one (4 k DOFs) 0.56 s; on the development VM a 3-D tet4 phase field with 28 k DOFs did not finish a staggered step in 2 minutes | (a) bitwise: SIMD across independent rows `i` of the update (each lane keeps the `k` order), unchecked indexing, register blocking over `j`: 3–6× on `sparselu_gemm` / `trsm`; (b) fast mode: another ordering (AMD / nested dissection) or a multifrontal / supernodal solver with BLAS-3 and FMA: 5–20× on the factorisation | (a) bitwise-safe, (b) fast mode |
| 3 | Small fixed-size products through the generic GEBP emulation: `eigen_gebp::route` (crates/akantu-core/src/eigen_gebp.rs:436), `product_into` (:523), `eigen_order::redux_tree`, `fe::eigen::product_slices` (crates/akantu-fe/src/eigen.rs:215) | everywhere: `contact_cohesive_quad4` 37 % (stress interpolation for the insertion check), `structural_dkt18_plate_static` 28 % (B^T D B), `contact_search_*` 18–32 %, `heat_explicit_*` 15–18 % (with the interpolation of item 5), `structural_*` 10–15 %, `embedded_*` 8–9 % | generic kernels where a fixed-size one would do: a shape decision (`route`, 2–8 % alone) and a dynamic-size product per element / quadrature point / facet | fixed-size kernels per element type with the same reduction order (as the solid explicit `fast::` kernels already do for gradient / BtD / integrate) | bitwise-safe; 2–4× on those shares; also helps the solid suite |
| 4 | Heat law per step: `HeatDiffusion::compute_diffusivity_grad_u_on_quad_points` (crates/akantu-diffusion/src/heat_diffusion.rs:358): `D∇u` with `eigen_order::product_coeff` and a closure per coefficient (:386), copies of `grad_u` and `diffusivity` (`to_vec`, :366–367), a new output array per call | `heat_explicit_tet4` 20 % (redux 16.4 + function 3.5), `heat_explicit_quad4` 20 % | generic kernel + allocation and copies per step | fixed `d × d` product written out in the reduction order of `product_coeff`, in-place output | bitwise-safe; ~20 % of the explicit heat step |
| 5 | Heat law per step: `compute_diffusivity_on_quad_points` (heat_diffusion.rs:297) recomputes the conductivity at every quadrature point, with an interpolation of `T` (`interpolate_on_integration_points`), whenever the temperature release changes, i.e. every step, also with `conductivity_variation = 0` | `heat_explicit_tet4` 8.5 % (3.3 function + 3.2 interpolate GEBP + 1.9 interpolate_impl), `heat_explicit_quad4` 8.6 % | repeated set-up work per step (upstream behaviour, UD-284 notes the release logic) | skip when `conductivity_variation == 0` (the value `k + 0·(T − T_ref)` equals `k` except for non-finite `T`: keep the recomputation when `T` has a NaN or infinity, or document the difference) | bitwise-safe for finite `T`; 8 % of the explicit heat step |
| 6 | Embedded intersection set-up: `LinearScan::all_intersections` (crates/akantu-geometry/src/index.rs:68) tests every bar segment against every background element; used by `EmbeddedInterfaceIntersector` (crates/akantu-embedded/src/intersector.rs:270) | `embedded_rebar_tri3` 49 %, `embedded_rebar_tet4` 40 % (+ `ray_triangle_do_intersect` 4.6 %); baseline: `init` 1.45–1.52 s and `intersect` 1.40–1.56 s per call at size `m`; 113 s at 9× the elements and segments (calibration on the development VM) | algorithmic: O(segments × elements); the upstream AABB tree is GPL (CGAL), the LGPL port scans | a uniform grid or bounding-volume tree over the triangles (LGPL), candidates sorted by primitive id so the reporting order stays the scan order | bitwise-safe (same intersections, same order); set-up 50–100× at size `m`, more on larger meshes |
| 7 | Allocation and zeroing of temporaries per step: `ArrayStorage::try_resize` 3–5 %, libc `memset`/`memmove` 8–15 %, `Vec<usize>` collection of element filters (`filter.to_vec()`, crates/akantu-diffusion/src/model/assembly.rs:319) | `heat_explicit_*` (try_resize 3–5 % + libc 10 %), `contact_explicit_quad4` (libc 16 %), `ntn_friction_quad4`, `contact_cohesive_quad4` | allocation and copies (`bt_d_gu` / `int_bt_d_gu` arrays per law and step in `assemble_internal_flow_of`, assembly.rs:309–350) | workspace arrays kept in the law / model | bitwise-safe; 10–15 % of explicit steps |
| 8 | Contact detection: `SpatialGrid` cells in a `BTreeMap` (crates/akantu-core/src/spatial_grid.rs:281; `NeighborhoodItems::next` 11 %, `insert` 4.6 %), grid rebuilt per search; `compute_maximal_detection_distance` (crates/akantu-contact/src/detector.rs:337) recomputed per search (2.8 %); outward orientation by the adjacent element's barycenter per projection (`geometry::barycenter_on`, geometry.rs:304, 7 %) | `contact_search_quad4`, `contact_search_tet4` (about 25 % together) | data layout (ordered map for a dense grid), repeated set-up work per search | dense cell array over the box (cells in the same order), cache the element sizes and the facet → element barycenters per mesh revision | bitwise-safe; search 1.5–2× |
| 9 | `SparseMatrixAij::mat_vec_mul` (sparse.rs:866–890) rebuilds a `Vec` of (local row, local column) pairs with two `EquationLayout::local` SipHash lookups (crates/akantu-linalg/src/layout.rs:58, :131) per non-zero on every call | `structural_beam3_frame_dynamic` 15 % (mat_vec 9.8 + hash 5.8) | repeated set-up work per call | cache the local pairs per profile release; a `Vec` index map for the serial layout | bitwise-safe; ~15 % of an implicit dynamic step |
| 10 | Stress interpolation of the extrinsic insertion check: `SolidMechanicsModelCohesive::interpolate_stress` (crates/akantu-cohesive/src/model.rs:1486) with `copy_points` 10 % and generic products 27 % | `contact_cohesive_quad4`: the check is 48 % inclusive | generic kernels + copies (the cohesive suite owns this path) | fixed-size interpolation kernel, no copy | bitwise-safe; 2× of the check |

Not hot anywhere in this suite: the phase-field transfers (`strain_transfer`,
`damage_transfer`: < 0.2 % of the staggered step), the internal-force
assembly of the phase-field coupler, the node-to-node contact and friction
update itself (`NtnContactSolverCallback`: ~4 % of its step; the rest is the
solid explicit step), mesh generation and reading.

## Per case

The phase times are in `baseline/README.md`. What dominates each case:

- `heat_explicit_tet4`, `heat_explicit_quad4`: residual assembly of the
  lumped explicit step (time per element and step in the baseline table).
  Items 4, 5, 7, then the FE kernels (`fast::integrate` 8.7 %,
  `fast::gradient` 7.4 %, `fast::btd` 6.5 %) and the scatter
  (`assemble_elemental_array_local_view` 5.3 %).
- `heat_implicit_hex8_kvar`: 5 Newton iterations per backward-Euler step;
  per iteration a conductivity update, an assembly and a factorisation.
  Item 2 (65 %), item 1 (9 %).
- `heat_static_tri6`: assembly item 1; solve item 2.
- `phasefield_staggered_tri3`, `_tet4`: the solid solve dominates the
  staggered step (it refactorises every staggered iteration because the
  degraded stiffness changes), then the phase-field solve; transfers are
  negligible. Items 2, 1.
- `contact_search_quad4`, `_tet4`: item 8 and item 3 (projections:
  `orthogonal_projection`, `natural_projection` through generic products).
- `contact_explicit_quad4`: the solid explicit step (`fast::btd` 13.9 %,
  `gradient` 11 %, `integrate` 9.8 %, elastic stress 8.6 %) plus allocation
  (item 7); the detection per step is a smaller share (not among the top
  40 inclusive symbols, so below 22 %).
- `contact_implicit_static_quad4`: items 2 and 1; `add_to_profile` and
  `HashMap::insert` (5.5 %) because the profile changes with the contact
  set every iteration.
- `contact_cohesive_quad4`: item 10 (the insertion check, 48 % inclusive)
  dominates; then the solid explicit step (23 %).
- `structural_beam3_frame_static`: assembly item 1 (44 %) and the
  elemental products (item 3); the solve item 2.
- `structural_beam3_frame_dynamic`: the factorisations happen in the
  warm-up steps (1.6 s per call on average, 38 % of the profile); the
  timed steps reuse the factors: triangular solves (`SparseLu::solve` 23 %
  inclusive) and item 9 (15.5 %).
- `structural_dkt18_plate_static`: assembly items 1 and 3 (21.5 µs per
  element and assembly in the baseline); the solve item 2.
- `embedded_rebar_tri3`, `_tet4`: item 6 in `init` and `intersect`;
  assembly item 1; the reinforcement's internal forces are cheap.
- `ntn_friction_quad4`: the solid explicit step (as `contact_explicit`);
  the contact and friction update is a few percent.

## Defects found (not fixed here)

1. `LinearScan` (crates/akantu-geometry/src/index.rs:57–88) makes the
   embedded intersection set-up O(segments × elements): 1.56 s at 20 k
   triangles and 5.2 k bar segments (baseline), 113 s at 180 k triangles
   and 48 k segments (development VM).
2. `compute_diffusivity_on_quad_points` (crates/akantu-diffusion/src/
   heat_diffusion.rs:297–352) interpolates `T` and rewrites the
   conductivity at every quadrature point every step even when the
   conductivity does not depend on `T` (`conductivity_variation = 0`).
3. `SparseMatrixAij::mat_vec_mul` (crates/akantu-linalg/src/sparse.rs:
   866–890) rebuilds the local index pairs of every non-zero with two hash
   lookups on every product.
4. The contact detection grid spacing is `√2 ·` the largest inradius of the
   facets around any master node, side facets included
   (crates/akantu-contact/src/detector.rs:337–372, :390–408): with
   elongated boundary facets next to the contact surface each grid cell
   holds thousands of master nodes and the local search degenerates
   towards O(slaves × masters). Measured: 1.2 s per search with 2000:1
   side cells against 56 ms with square cells, same surfaces (40 k / 30 k
   segments). This is upstream's rule; the cases now use square cells.
5. `structural_dkt18_plate_static` at 80 k triangles (241 k DOFs) ran out
   of memory on an 8 GB VM in the solve (the case now uses 7 k triangles);
   the sparse LU's fill on this plate is worth a look with item 2.

## Fast-mode candidates (D-122; not to be written before the switch exists)

| Candidate | Cases | Expected gain |
|---|---|---|
| A fill-reducing ordering / multifrontal or supernodal LU with BLAS-3 and FMA instead of the bitwise Eigen SparseLU port | all implicit cases | 5–20× on factorisations (60 % of `heat_implicit_hex8_kvar`, most of the phase-field step) |
| Threaded assembly with per-thread partial matrices summed in a fixed order independent of the thread count is bitwise-safe; with per-thread order it is fast mode | all assemblies | up to the core count on item 1 after it is fixed |
| FMA in the per-quadrature-point products (`D∇u`, `B^T D B`) | heat, structural, phase field | 10–20 % of those kernels |
| Reassociated (vectorised) reductions in `sparselu_gemm` / `trsm` | all implicit cases | 1.5–2× beyond the bitwise SIMD of item 2 |

## C++ twins

Upstream examples with the same problem, for a later C++ timing task:

- `heat_explicit_tet4`: examples/c++/diffusion_model/heat_diffusion_dynamics_3d.cc
- `heat_explicit_quad4`: examples/c++/diffusion_model/heat_diffusion_dynamics_2d.cc (triangles upstream)
- `heat_static_tri6`: examples/c++/diffusion_model/heat_diffusion_static_2d.cc
- `phasefield_staggered_tri3`: examples/c++/phase_field_model/phase_field_notch.cc (and phase_field_parallel.cc)
- `contact_explicit_quad4`: examples/c++/contact_mechanics_model/contact_explicit_dynamic.cc (Hertz)
- `contact_implicit_static_quad4`: examples/c++/contact_mechanics_model/contact_explicit_static.cc
- `contact_cohesive_quad4`: examples/c++/contact_mechanics_model/ (cohesive-contact set-up, test_cohesive_contact)
- `structural_beam3_frame_*`, `structural_dkt18_plate_static`: test/test_model/test_structural_mechanics_model/ (Bernoulli 3, DKT18)
- `embedded_rebar_tri3`: examples/c++/embedded_model/embedded.cc
- `ntn_friction_quad4`: extra_packages/traction-at-split-node-contact/test/test_ntn_model/ (friction fixture)
