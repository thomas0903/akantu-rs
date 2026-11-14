# Inventory R1 — CORE area (common, fe_engine, geometry, mesh, mesh_utils, io, third-party)

_Taken 2026-11-09. Reference: upstream Akantu @ `35a851780d0b9471d258ed53a003ae98aaa95ea0`._
_All `path:line` references are relative to the reference checkout root unless stated._

This document is the source-level feature inventory for the CORE area. It is the
input for the authoritative parity matrix (`docs/FEATURE_PARITY.md`). Every row
is meant to be implementable and verifiable independently.

How to read it:

* **Section 1** is the main table. For readability it is split into
  sub-tables per ID prefix; all sub-tables share exactly the same columns and
  together form the single main table.
* **Section 2** lists porting-critical conventions with exact values and
  `file:line` references (node numbering, facets, quadrature, Voigt, storage
  order, Gmsh codes, ...). Implementers must reproduce these bit-for-bit.
* **Section 3** is the public API surface used by user code / Python.
* **Section 4** (appendix) maps every `.hh`/`.cc` file in scope to feature IDs.
* **Section 5** lists open questions.

Build options referred to below (CMake `AKANTU_*` options, see
`packages/*.cmake`): `core` (always on), `cohesive_element` (default ON,
depends on solid_mechanics), `structural_mechanics` (default OFF),
`dumpers` (default ON, needs `IOHelper`), `CGAL` (external, OFF by default),
`Scotch` (external), `MPI`/`parallel`, `debug_tools` (Boost.Graph),
`petsc`, `python_interface`, `IGFEM` (extra package `extra_packages/igfem`).
Default scalar types: `Real = double`, `Int = Idx = int` (32 bit),
`UInt = unsigned int` (`cmake/AkantuMacros.cmake:85-166`,
`src/common/aka_config.hh.in`).


### Summary counts

| Prefix | Rows | Scope |
|---|---|---|
| CORE | 43 | `src/common` (types, Array, tensors, Voigt, math, containers, RNG, errors, init) |
| FE | 39 | `src/fe_engine` engine, integrators, shape functions, assembly |
| ELEM | 27 | 13 regular + 7 cohesive + 3 structural + 3 IGFEM element types + Hermite helper |
| QUAD | 19 | quadrature rules and order mapping |
| MESH | 35 | `src/mesh` |
| MUTIL | 18 | `src/mesh_utils` |
| IO | 12 | mesh readers/writers |
| DUMP | 16 | dumpers + IOHelper |
| PARSE | 12 | input-file parser, parameters, CLI |
| GEOM | 9 | CGAL geometry |
| ITER | 7 | `akantu_iterators` |
| **Total** | **237** | 322 source/test files mapped in §4 |

Highest-risk items for bit-for-bit parity (details in rows): facet construction
order (MUTIL-004), element removal renumbering (MESH-013), cohesive insertion
order (MUTIL-016), tet4 natural coordinates (ELEM-008), quadrature ordering
(QUAD-007/017), `Math::reduce` (CORE-026), RNG streams (CORE-031/032), Gmsh
node reordering and physical-name handling (IO-004/005/007), column-major
storage conventions (§2.1).

---

## 1. Main feature table

Columns: `| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |`

### 1.1 CORE — `src/common/`

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| CORE-001 | Scalar & index types | `src/common/aka_config.hh.in` (`Real`, `Int`, `Idx`, `UInt`, `AKANTU_INTEGER_SIZE`, `AKANTU_FLOAT_SIZE`); `cmake/AkantuMacros.cmake:85-190` | `Real` = double (float / long double selectable), `Int`=`Idx`= signed 32-bit (64-bit selectable), `UInt` unsigned. `Idx` is the Eigen dense index type (`EIGEN_DEFAULT_DENSE_INDEX_TYPE`, `aka_types.hh:34`). | core | — | indirectly all | Scotch requires `SCOTCH_Num` size == `AKANTU_INTEGER_SIZE` (`packages/scotch.cmake`) | Port uses `f64` and the index types fixed in `docs/ARCHITECTURE.md`; signed indices with `-1` sentinels are used throughout (e.g. `Array::find` returns -1, `ElementNull.element = -1`). |
| CORE-002 | Version query | `aka_config.hh.in` (`AKANTU_VERSION_*`), `aka_config.cc.in` `getVersion()` | Returns `"MAJOR.MINOR.PATCH<prerelease><local>"`; `AKANTU_VERSION = MAJOR*10000+MINOR*100+PATCH`. | core | — | — | — | Expose equivalent `version()`; Python binding exposes it too (R4). |
| CORE-003 | Global constants & dimension tags | `aka_common.hh:55-75` (`_all_dimensions`, `REAL_INIT_VALUE`, `dim_1_t..dim_3_t`, `AllSpatialDimensions`); `aka_extern.cc` (`_all_dimensions = -1` second definition) | `_all_dimensions` = `std::numeric_limits<Int>::max()` (header, anonymous namespace) — used as "any dimension" filter. `REAL_INIT_VALUE` = quiet NaN in debug builds, `0.` when `AKANTU_NDEBUG` (release). | core | — | — | — | Two conflicting definitions of `_all_dimensions` (header: INT_MAX; `aka_extern.cc:74`: -1). Only the header one is visible to users. Port must pick INT_MAX semantics. NaN-vs-0 init differs by build type → affects uninitialised-array behaviour; port should choose deterministic init and document. |
| CORE-004 | `ElementType` enum and string I/O | `aka_element_classes_info.hh:36-95`; `aka_element_classes_info_inline_impl.hh:150-160` (`AKANTU_ENUM_OUTPUT_STREAM/INPUT_STREAM`) | Enum order: `_not_defined`, 13 regular types, 7 cohesive (if `AKANTU_COHESIVE_ELEMENT`), 3 structural (if `AKANTU_STRUCTURAL_MECHANICS`), 3 IGFEM (if `AKANTU_IGFEM`), `_max_element_type`. String form keeps the leading underscore, e.g. `"_triangle_3"` (`std::to_string`, `operator<<`, `operator>>`). | core (+cohesive_element / structural_mechanics / IGFEM) | Boost.Preprocessor | all mesh/FE tests (meshes named `_triangle_3.msh` etc.) | — | Numeric values shift with enabled packages → never persist the numeric value; port should use a fixed full enum (all kinds always present) and map names. Element-type strings appear in file names, dumper output, parser input and Python. |
| CORE-005 | Element kind / geometry / interpolation enums | `aka_element_classes_info.hh:97-238` (`ElementKind`, `element_kind_t`, `GeometricalType`, `InterpolationType`, `GeometricalShapeType`, `GaussIntegrationType`, `InterpolationKind`) | `ElementKind`: `_ek_regular, _ek_cohesive, _ek_structural, _ek_not_defined` (no igfem kind!). `GeometricalType` lists `_gt_*` (pentahedra after hexahedra). `InterpolationType`: 13 regular `_itp_*` + `_itp_hermite_2, _itp_bernoulli_beam_2/3, _itp_discrete_kirchhoff_triangle_18` + 3 igfem. `GaussIntegrationType`: point, segment, triangle, tetrahedron, pentahedron. | core | Boost.PP | — | — | `ElementKind` string I/O exists (`_ek_regular` etc.). The IGFEM extra package still uses `_ek_igfem` and a 4-argument `Element` constructor (`extra_packages/igfem/src/element_class_igfem.hh:149,289`, `mesh_igfem_spherical_growing_gel_tmpl.hh:219`) which **do not exist** in core → IGFEM is bit-rotted (see ELEM-024..026, Q3). |
| CORE-006 | Compile-time element-type dispatch | `aka_element_classes_info_inline_impl.hh:37-334` (`AKANTU_BOOST_*_SWITCH`, `ElementTypes<kind>`, `AllElementTypes`, `tuple_dispatch`, `tuple_dispatch_with_default`); `aka_constexpr_map.hh` (`ConstexprMap`, `static_switch_dispatch`) | Maps a runtime `ElementType`/dimension to a template instantiation; unknown type → `std::range_error("Unknown ... type in dispatch")` or default functor. | core | Boost.PP | everything | — | Rust: `match` over enum or trait objects; keep the "unknown type" error path as `Result`. |
| CORE-007 | Misc model/solver enums with string I/O | `aka_common.hh:88-437` (`SpatialDirection`, `MeshIOType`, `EventHandlerPriority`, `ModelType`, `AnalysisMethod`, `DOFSupportType`, `NonLinearSolverType`, `SparseSolverType`, `TimeStepSolverType`, `IntegrationSchemeType`, `SolveConvergenceCriteria`, `CohesiveMethod`, `MatrixType`, `DetectionType`, `ContactState`, `ReduceOperation`, `CommunicatorType`, `SynchronizationTag`) | Class enums declared with `AKANTU_CLASS_ENUM_*`: value `_name`, string `"name"` (no underscore). `AnalysisMethod`: `_static=0,_implicit_dynamic=1,_explicit_lumped_mass=2,_explicit_lumped_capacity=2,_explicit_consistent_mass=3,_explicit_contact=4,_implicit_contact=5`. `EventHandlerPriority` values 0,5,9,10,20,94,100,100. `MeshIOType`: `_miot_auto,_miot_gmsh,_miot_gmsh_struct,_miot_diana,_miot_abaqus`. `SynchronizationTag` has no input stream. | core | Boost.PP | parser tests (enum parsing) | SynchronizationTag used by synchronizers (R3) | String spellings are user-facing (input files, Python). Note `_miot_abaqus` exists but no Abaqus reader is compiled (see IO-012). |
| CORE-008 | `GhostType` | `aka_common.hh:481-555` (`GhostType`, `ghost_type_t`, `ghost_types`), `aka_common_inline_impl.hh:34-49` | `_not_ghost=0, _ghost=1, _casper`; iterating `ghost_types` yields `_not_ghost`, `_ghost`. Stream: `"not_ghost"`, `"ghost"`, `"Casper the friendly ghost"`. | core | — | all | ghost elements = elements owned by other ranks | Iteration order (not_ghost then ghost) matters for output order everywhere. |
| CORE-009 | `NodeFlag` bit flags | `aka_common.hh:489-540`; `aka_common.cc:157-167` (`operator<<`) | `uint8` flags: `_normal 0x00, _distributed 0x01, _master 0x03, _slave 0x05, _pure_ghost 0x09, _shared_mask 0x0F, _periodic 0x10, _periodic_master 0x30, _periodic_slave 0x50, _periodic_mask 0xF0, _local_master_mask 0xCC` (+ debug combos 0x33..0x59); bitwise `&`, `\|`, `~`, `&=`, `\|=`; printed as `0x` + 2 hex digits. | core | — | `test_mesh_periodic` | central to parallel node ownership (R3) | Reproduce exact bit values (stored in mesh `nodes_flags`, dumped by `Mesh` dumpers, exchanged via MPI). |
| CORE-010 | Enum ↔ string macro machinery | `aka_enum_macros.hh:30-140` | Plain enums: name string = identifier incl. leading `_`; class enums: list entries without `_` give enum value `_x` and string `"x"`. Input parsing error lists all valid values (`AKANTU_EXCEPTION`). | core | Boost.PP | parser tests | — | Error message text not part of contract; keep value spelling exact. |
| CORE-011 | `safe_enum` iterable enums | `aka_safe_enum.hh` (`safe_enum<def>`) | Wraps enum with `_begin_`/`_end_` for range-for iteration (`ghost_types`, `spatial_directions`, `element_kind_t`). | core | — | — | — | Rust iterator over fixed slices. |
| CORE-012 | `Array<T>` storage & allocation | `aka_array.hh:37-260`, `aka_array_tmpl.hh:13-340` (`ArrayBase`, `ArrayDataLayer<T,_default/_pod>`, `AKANTU_MIN_ALLOCATION`=2000 `aka_common.hh:561`) | Contiguous tuple storage: `size` tuples × `nb_component` values, **row-major per tuple** (`values[i*nb_component+j]`, `aka_array_tmpl.hh:380`). POD types: malloc/realloc with growth `diff>2000 ? size : (diff>0 ? alloc+2000 : alloc)`; resize to 0 frees. Non-POD: `std::vector`. Has string `id`. `resize(n, val)` fills only new tuples. | core | — | `test/test_common/test_array.cc` (ArrayConstructor.*, ArrayFixture.Resize/Reserve/PushBack...) | Arrays are what is communicated by synchronizers | Non-POD `push_back(value)` appends **one scalar** (not `nb_component`), POD `push_back(value)` appends a whole tuple filled with `value` (`aka_array_tmpl.hh:144-148` vs `:269-271`) — inconsistency; port should define tuple semantics. Allocation growth policy is not observable except `getAllocatedSize()`/`getMemorySize()` printing. |
| CORE-013 | `Array<T>` operations | `aka_array_tmpl.hh:340-700`, `aka_array.cc:34-75` | `operator()(i,j)`, `operator[](flat)`, `push_back(tuple/Eigen/iterator)`, `erase(i)` = **swap-remove** (last tuple moved into hole), `find(elem)` (exact, -1 if absent), `find(tensor)`, `set(val)`, `set(tensor)`, `zero()`, `clear()`, `append`, `copy(other, no_sanity_check)` (resizes to `other.size*other.nb_comp/nb_comp`), `+=,-=,*=`, `==` (**also compares `id`**), `isFinite()`, `printself` (values printed only at `dblDump`/`dblTest`). `Array<Real>::find` uses `abs(a-elem) <= eps` and returns `end()-it` (**bug: distance from end, not index**, `aka_array.cc:36-45`). `Array<ElementType/char>` arithmetic throws `NotImplemented`. | core | — | `test_array.cc` (Copy, Set, PushBack, IteratorFilter, ...) | — | Reproduce swap-remove order (affects element renumbering in `removeElements`-like code). Decide whether to replicate the `Array<Real>::find` bug (probably unused; grep shows no core user) — record in DECISIONS. `operator=` prints a debug warning. |
| CORE-014 | `ArrayProxy<T>` | `aka_array.hh:407-455` | Non-owning view over external memory or another Array; resize/reserve to a different size throws "cannot resize a temporary array". | core | — | used by `test_fe_engine_precomputation.cc` | — | Rust: slice views / `ArrayView`. Used heavily by Python bindings (numpy wrapping). |
| CORE-015 | Array views & typed iterators | `aka_array_view.hh` (`ArrayView`, `make_view`, `make_const_view`), `aka_view_iterators.hh` (`view_iterator`, `const_view_iterator`, `internal_view_iterator`), `aka_types.hh:246-345` (`EigenView`, `make_view<R>`/`make_view<R,C>`), `aka_array_tmpl.hh:760-880` (`begin(ns...)`, `get_iterator`) | Iterate an `Array` as scalars, `Vector`(n), `Matrix`(m,n) (Eigen `Map`, **column-major**), or `Tensor3`; size mismatch (`nb_component*size != product(ns)`) throws `debug::ArrayException`. Static-size variants via template args. | core | Eigen3 | `test_array.cc` (ViewVectorDynamic/Static, ViewMatrix*, ViewVectorWrong, IteratorThrow, ...), `test_tensors.cc` | — | **Matrix views reinterpret a tuple column-major**: tuple of a `m×n` matrix stores `M(i,j)` at offset `i + j*m`. Every elemental/quad-point matrix field (gradients, stresses, B, connectivity-of-facets...) depends on this. |
| CORE-016 | `ArrayFilter` | `aka_array_filter.hh` | Read-only iteration over a subset (index list) of an Array as vectors. | core | — | — | — | Simple adapter. |
| CORE-017 | `ArrayPrinter` / `make_printer` | `aka_array_printer.hh` | Prints `{a, b, ...}` or `{{..},{..}}` for Arrays. | core | — | — | — | Debug helper; low priority. |
| CORE-018 | Dense `Vector`/`Matrix` types (Eigen + plugins) | `aka_types.hh:33-120` (`Vector<T,n>`, `Matrix<T,m,n>`, `VectorProxy`, `MatrixProxy`, `VectorXr`, `MatrixXr`, IO format); `aka_types_eigen_matrix_base_plugin.hh` (`operator()(c)`=column, `begin/end` column iteration, `size(i)`, `set`, `eye`, `clear`, `zero`, `distance`, `doubleDot`, `<=,>=,<,>` via `isMuchSmallerThan`); `aka_types_eigen_matrix_plugin.hh`, `aka_types_eigen_plain_object_base_plugin.hh` (initializer lists) | Eigen with `ColMajor` default, `Idx` index type. `Matrix{{a,b},{c,d}}` initializer is **row-wise**. `M(c)` on a matrix returns **column c**; range-for over a matrix iterates **columns**. Default stream format `[a, b, c]` / `[[..], [..]]`. | core | Eigen3 ≥3.3.7 (bundled 5.0.1 script) | `test/test_common/test_tensors.cc` (VectorConstruct*, MatrixConstruct*, MatrixIterator, ...) | — | Rust choice (nalgebra / faer / hand-rolled) must keep column-major semantics and the "column" meaning of `M(c)`; many algorithms (e.g. `Math::barycenter`, facet extraction) iterate matrix columns. `operator<=` on matrices is Eigen fuzzy compare, not element-wise. |
| CORE-019 | Eigen decomposition helpers | `aka_types.hh:440-575` (`eig()`, `eig(values)`, `eig(values,vectors,sort)`, `eigh(values)`, `eigh(values,vectors,sort)`) | `eig` uses `EigenSolver` (general); throws if eigenvectors have imag part > `1e-15*norm`; `eigh` uses `SelfAdjointEigenSolver`. `sort=true` → **descending** order of eigenvalues, eigenvectors permuted accordingly. | core | Eigen3 | `test_tensors.cc` MatrixEigs | — | Eigenvalue algorithms differ between libraries → results equal only to rounding; differential tests must use tolerances; sign of eigenvectors is arbitrary. Used by materials (principal stresses). |
| CORE-020 | `Tensor3` / `TensorBase` / `TensorProxy` | `aka_tensor.hh:40-463` | N-d (≥3) tensor, **first index fastest** (`compute_index`, `aka_tensor.hh:86-99`); `T(s)` on 3-tensor returns `Map<Matrix>` slice along last index (contiguous `n0*n1` block); `transform`, `accumulate`, norms (L1/L2/L∞/Lp). | core | Eigen3 | `test_tensors.cc` | — | Used for shape derivatives per quad point (`dim × nnodes × nquad`), Jacobians, structural shapes. |
| CORE-021 | Voigt notation helper | `aka_voigthelper.hh:31-118` (`VoigtHelperData<1/2/3>`, `VoigtHelper::matrixToVoigt`, `matrixToVoigtWithFactors`, `voigtToMatrix`), `aka_voigthelper_tmpl.hh:30-97` | Sizes 1/3/6. Index tables (`vec`): 2D `(0,0),(1,1),(0,1)[,(1,0)]`; 3D `(0,0),(1,1),(2,2),(1,2),(0,2),(0,1)[,(2,1),(2,0),(1,0)]`. Factors `1,1,2` / `1,1,1,2,2,2` (engineering shear for strain). `voigtToMatrix` symmetrises. See §2.6. | core | — | `test/test_common/test_voigt_helper.cc` (Size, Indicies, Factors, MatrixToVoight, MatrixToVoightFactors, VoightToMatrix) | — | Order `xx,yy,zz,yz,xz,xy` must match exactly (material tangents, dumped Voigt fields). |
| CORE-022 | Voigt B-matrix transfers | `aka_voigthelper_tmpl.hh:97-239` (`transferBMatrixToSymVoigtBMatrix`, `transferBMatrixToBNL`, `transferBMatrixToBL2`) | Builds symmetric-gradient Voigt B (`size × dim*nnodes`, **node-major dofs** `n*dim+i`) from `dNdx (dim×nnodes)`; BNL (Bathe 1975) and BL2 (finite strain, 1D/2D/3D specialisations). | core | — | via solid mechanics stiffness tests (R2) | — | Exact row filling order of shear rows (2D row2 = `[dN/dy, dN/dx]`; 3D rows 3,4,5 = yz, xz, xy) must be reproduced. BNL comment "TODO verify 2D/1D". |
| CORE-023 | Geometric math helpers | `aka_math.hh:35-75`, `aka_math_tmpl.hh:31-136` (`Math::normal(v)`, `Math::normal(v1,v2)`, `triangle_inradius`, `tetrahedron_volume`, `tetrahedron_inradius`, `barycenter`) | 2D normal: `(v1, -v0)` normalised; 3D: `(v1×v2)` normalised; triangle inradius `sqrt((s-a)(s-b)(s-c)/s)`; tet volume via 4 determinants (signed) /6; tet inradius `3V/ΣA_i`; barycenter = mean of matrix columns. | core | — | `test_fe_engine_inradius.cc` | — | Reproduce exact operation order for bitwise equality (e.g. tet volume formula `det(x2,x3,x4) - det(x1,x3,x4) + det(x1,x2,x4) - det(x1,x2,x3)`, `aka_math_tmpl.hh:71-90`). |
| CORE-024 | Tolerant comparisons & scalar helpers | `aka_math.hh:77-130`, `aka_math_tmpl.hh:138-200`, `aka_extern.cc:71` | `Math::tolerance = 1e2*DBL_EPSILON` (global, settable via `setTolerance`); `are_float_equal(x,y)`: `abs(x-y) <= tol*max(1, abs(x), abs(y))`; `are_vector_equal`; `isnan` (`x!=x`); `intersects(xmin,xmax,ymin,ymax)`; `is_in_range` (closed); `pow<p>`, `pow(x,p)`; `kronecker`. | core | — | `test_fe_engine_precomputation_structural.cc` sets tolerance 1e-15 | — | Global mutable tolerance is process-wide state; port as configurable value (tests change it). |
| CORE-025 | Tangents from normals | `aka_math.cc:29-72` (`Math::compute_tangents`) | For each normal: 2D tangent = `normal(n)` (= `(n1,-n0)` normalised; if `norm(n)≈0` tangent=(1,0)); 3D second tangent = `normalize(n × t0)`. Output `dim × (dim-1)` col-major per normal. | core | — | contact (R3) | — | Degenerate normal handling in 3D uses only first tangent rule. |
| CORE-026 | Deterministic pairwise reduction | `aka_math.cc:74-100` (`Math::reduce`) | **Sorts the array ascending in place**, then pairwise sums halves repeatedly (odd remainder folded into `a[n-2]`). Used by `IntegratorGauss::integrate` (scalar result). | core | — | `test_integrate.cc`, `test_fe_engine_gauss_integration.cc` | Local only (no MPI reduction here) | Must be replicated exactly for bitwise-equal scalar integrals; it **mutates** its input. |
| CORE-027 | Scalar Newton–Raphson | `aka_math.hh:142-162`, `aka_math_tmpl.hh:189-210` (`Math::NewtonRaphson<T>`, `NewtonRaphsonFunctor`) | `x -= f/f'` until `abs(f) <= tol` or max iterations; assert on non-convergence (debug only). | core | — | — (used by materials) | — | Non-convergence silently returns in release builds. |
| CORE-028 | Axis-aligned bounding box `BBox` | `aka_bbox.hh:36-250` | Empty box `lower=+max, upper=-max`; `+= point`, `intersects` (per direction / all), `intersection` (clamped), `contains` (closed), `reset`, `getCenter`, `size`, `allSum` (MPI min-reduce of `[lower,-upper]`), `allGather`, `intersection(other, comm)` returns `map<rank,BBox>`. | core | — | (used by `test_mesh_periodic`, grid synchroniser tests R3) | allSum/allGather over Communicator | Ghost/partition bounding boxes used for element synchronisation (R3). |
| CORE-029 | Compressed sparse row container `CSR<T>`, `DataCSR<T>` | `aka_csr.hh:36-270` | Two-pass build protocol: `rowOffset(i)++` counts → `countToCSR()` → `resizeCols()` → `beginInsertions()` → `insertInRow(row,val)` → `endInsertions()` (shifts offsets back). Row iteration, reverse iteration, `getNbCols(row)`, `copy(offsets, values)`. | core | — | `test/test_common/test_csr.cc` (CheckInsertion, Iteration, ReverseIteration) | — | Used for node→element maps (`MeshUtils::buildNode2Elements`). Insertion order within a row = call order → defines output ordering. |
| CORE-030 | Dynamic spatial hash grid `SpatialGrid<T>` | `aka_grid_dynamic.hh:36-519` (`CellID`, `neighbor_cells_iterator`, `Cell`, `insert`, `getCellID`, `saveAsMesh`) | Cells indexed by `floor((x - center)/spacing)` per direction; stored in `std::map<CellID,Cell>` (lexicographic order); neighbour iteration over 3^d cells encoded base-3 with offsets −1..1 (first coordinate fastest); `saveAsMesh` writes one segment/quad/hex per non-empty cell (2^d new nodes each, no node sharing) with `Int` mesh data `"tag_1"` = running cell id (`aka_grid_dynamic.hh:433-510`). | core | — | `test/test_common/test_grid.cc` + `circle.geo` (`test_grid_mesh`) | `GridSynchronizer` friend (R3) | Used by non-local neighbourhoods & grid synchroniser. Map ordering defines iteration order. |
| CORE-031 | Global random generator | `aka_random_generator.hh:49-82` (`RandomGenerator<T>`), `aka_extern.cc:80-85` (default seed 5489), `aka_common.cc:104-115` (seed policy) | Single static `std::default_random_engine` per T (`Idx`, `UInt`); seed priority: env `AKA_SEED` > `--aka_seed` > input-file `seed` parameter > `time(nullptr)`; final seed multiplied by `(rank+1)`. | core | C++ `<random>` | (material random-field tests, R2) | per-rank seeds `seed*(prank+1)` | **`std::default_random_engine` is implementation-defined** (libstdc++: `minstd_rand0`; libc++: `minstd_rand`), and std distributions are implementation-specific → bit-for-bit random fields require emulating the oracle's libstdc++ engine + distribution algorithms. Major reproducibility risk. |
| CORE-032 | Random distributions & random parameters | `aka_random_generator.hh:35-44` (`RandomDistributionType`: uniform, exponential, gamma, weibull, extreme_value, normal, lognormal, chi_squared, cauchy, fisher_f, student_t, not_defined), `RandomDistribution`, `RandomDistributionProxy`, `RandomParameter<T>`, `make_random_parameter` | `RandomParameter = base_value + distribution(gen)`; default distribution `uniform(0,0)`. `make_random_parameter(base, type, a, b)`; **weibull built as `std::weibull_distribution(b, a)`** (i.e. `a`=scale λ, `b`=shape k in the user's order). Printing: `"<base> + <type> [ <params> ]"`. | core | C++ `<random>` | `test_parser` (random parameter parsing) | — | Parameter order per distribution must match parser grammar (PARSE-0xx). Same RNG reproducibility risk as CORE-031. |
| CORE-033 | Error handling & debug output | `aka_error.hh:31-429` (`DebugLevel`, `debug::Exception`, `CriticalError`, `AssertException`, `NotImplementedException`, `Debugger`, `DebugLevelContext`, `AKANTU_EXCEPTION`, `AKANTU_ERROR`, `AKANTU_DEBUG_ASSERT`, `AKANTU_DEBUG_*`, `AKANTU_TO_IMPLEMENT`), `aka_error.cc` (demangle, backtrace via addr2line, terminate handler, `printMessage`, `setLogFile`, `setParallelContext` prefix `<pid>[R<rank>\|S<size>] `) | Debug levels: error 0, exception 1, warning 4, info 5, in/out 6, trace 8, dump 100, test 1337. Default level `dblWarning`, overwritten to `dblError` in `initialize` then to `--aka_debug_level` (default `dblWarning`) / env `AKA_DEBUG_LEVEL`. `AKANTU_DEBUG_ASSERT` compiled out with `AKANTU_NDEBUG` (Release). Messages: `<ctx>{timestamp} <prefix> <info> (func(): file:line)`. Terminate handler prints the random seed. | core | (addr2line/readlink optional) | all tests (test level `dblTest` disables addresses in `printself`) | parallel context prefix | Map to `akantu_core::Error` variants; many asserts are debug-only in the oracle — the port must decide which become hard errors (input validation must be `Result`). |
| CORE-034 | Named arguments | `aka_named_argument.hh` (`param_t`, `param_proxy`, `DECLARE_NAMED_ARGUMENT`, `REQUIRED_NAMED_ARG`, `OPTIONAL_NAMED_ARG`, `use_named_args`) | `_name = value` keyword-style args. Declared names in scope: `_all_ghost_types, _default_value, _element_kind, _ghost_type, _nb_component, _nb_component_functor, _with_nb_element, _with_nb_nodes_per_element, _spatial_dimension, _do_not_default, _element_filter` (`element_type_map.hh:40-50`), `_communicator, _edge_weight_function, _vertex_weight_function` (`mesh.hh:55-57`). | core | — | — | — | Rust: builder structs / option structs. Python keeps kwargs. |
| CORE-035 | Generic factory singleton | `aka_factory.hh` (`Factory<Base,T,Args...>`) | Map id→allocator; duplicate registration or unknown id throws; `knows`, `getPossibleAllocators`. | core | — | (materials/solvers factories, R2/R3) | — | Rust: registry with explicit registration (no static-init order tricks). |
| CORE-036 | Event handler manager | `aka_event_handler_manager.hh` (`EventHandlerManager<H>`) | Handlers kept sorted by `EventHandlerPriority` ascending (lower value first); registration uses `lower_bound` → **a new handler is placed before existing ones of equal priority**; duplicate registration throws; `sendEvent` iterates in order. | core | — | via mesh event tests | — | Order of callbacks (mesh → fe_engine → synchroniser → dof_manager → model → non-local) must be preserved. |
| CORE-037 | `CircularArray<T>` | `aka_circular_array.hh`, `aka_circular_array_inline_impl.hh` | Ring buffer over Array with `makeStep()`. | core | — | none | — | **Unused** anywhere in `src/`/`test/` (grep). Port optional; keep in matrix as "dead code — not ported" decision candidate. |
| CORE-038 | Library initialisation & CLI options | `aka_common.hh:619-627`, `aka_common.cc:47-155` (`initialize(argc,argv)`, `initialize(input_file[,argc,argv])`, `finalize`, `readInputFile`, `getStaticArgumentParser`, `getStaticParser`, `getUserParser`) | Creates static Communicator, sets `Tag::max_tag`, registers CLI args `--aka_input_file`, `--aka_debug_level`, `--aka_print_backtrace`, `--aka_seed` (parsed with `_remove_parsed`), reads input file (CLI arg overrides function arg), seeds RNG (CORE-031), sets debug level, `atexit(finalize)`. `getUserParser()` returns first `user` section of the static parser. | core | (MPI if enabled) | all executables | communicator init, `seed*(rank+1)` | `getUserParser` dereferences without checking existence (crash if no `user` section) → port should return `Result`. |
| CORE-039 | String helpers | `aka_common_inline_impl.hh:52-120` (`to_lower`, `trim`, `trim(str,c)`, `printMemorySize<T>`) | trim whitespace / char; memory size printed `x.xx{,Ki,Mi,...}Byte` (2 decimals). | core | — | — | — | Used by parser (lower-casing keys?) — verify in PARSE rows. |
| CORE-040 | Type traits, casts, hashing | `aka_common.hh:640-725` (`aka::is_of_type`, `aka::as_type`, `aka::is_scalar`, `decay_v`), `aka_common.hh:740-768` (`std::hash<pair>` with magic `0x9e3779b9`/64-bit), `AKANTU_GET_MACRO*`/`AKANTU_SET_MACRO` (`aka_common.hh:566-612`) | Dynamic-cast helpers; pair hash `h(b) + magic + (seed<<6) + (seed>>2)` with `seed=h(a)`. Getter/setter macro naming convention `getX()/setX()`. | core | — | — | — | Hash only affects unordered containers' iteration order — any output that iterates an `unordered_map` keyed by pairs is order-dependent (check users when porting). |
| CORE-041 | Process-global objects | `aka_extern.cc:43-90` (`debug::debugger`, `_akantu_debug_cout`, `static_argparser`, `static_parser`, `Parser::permissive_parser=false`, `Math::tolerance`, `empty_filter` (Array<Int>(1,1)), `RandomGenerator` statics, `Tag::max_tag`) | Singletons used across the library; `empty_filter` is a size-1 sentinel compared **by address** to mean "no filter". | core | — | — | — | Rust: avoid global state; `Option<&[Idx]>` instead of sentinel filter. |
| CORE-042 | Compiler-warning pragmas | `aka_warning.hh`, `aka_warning_restore.hh` | Push/pop diagnostic pragmas. | core | — | — | — | Build glue, no behavior. |
| CORE-043 | Forward declarations & `is_array` trait | `aka_fwd.hh` | Forward decls (`Array`, `ElementTypeMapArray`, `SpatialGrid`, `Parser`, `Mesh`, ...), `aka::is_array<T>`. | core | — | — | — | Build glue. |

### 1.2 FE — `src/fe_engine/` (engine, shapes, integrators)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| FE-001 | `Element` handle | `src/fe_engine/element.hh` (`Element{type, element, ghost_type}`, `ElementNull{_not_defined,-1,_casper}`, `operator<`, `to_string`) | Ordering: anything `< ElementNull`; otherwise lexicographic on `(ghost_type, type, element)`. String `"Element [_triangle_3, 4, not_ghost]"` / `"ElementNull"`. | core | — | many | elements are exchanged between ranks | Ordering is used in `std::set/map<Element>` (e.g. `MeshUtils`, groups) → affects iteration order of outputs. |
| FE-002 | `IntegrationPoint` | `src/fe_engine/integration_point.hh` | `Element` + `num_point` + `global_num = element*nb_quad + num_point`; ordering by element then point. | core | — | — | non-local (R3) | Note `operator!=` also compares `global_num` but `==` does not. |
| FE-003 | Per-type compile-time element tables | `element_class.hh:31-60` (`ElementClassProperty`, `AKANTU_DEFINE_ELEMENT_CLASS_PROPERTY`), `interpolation_element_tmpl.hh` (`InterpolationProperty<_itp_*>`), `geometrical_element_property.hh:40-465` (`GeometricalElementProperty<_gt_*>`, `ElementClassExtraGeometryProperties<type>`), `geometrical_element_property.cc` | For each type: geometrical type, interpolation type, kind, spatial dim, Gauss family, polynomial degree (= default integration order); nb nodes; natural dim; facet types/counts/local connectivity; p1 (linear) type; facet element types. Mesh queries (`Mesh::getNbNodesPerElement`, `getFacetType`, `getFacetLocalConnectivity`, `getP1ElementType`, `getKind`, `getSpatialDimension(type)`, `getNaturalSpaceDimension`) read these. | core (+cohesive) | — | `test_buildfacets` (facet tables), `test_facet_element_mapping` | — | Facet local connectivity is stored **column-major** (`Eigen::Map<Matrix<Idx,nb_facets,nb_nodes_per_facet>>`, `element_class_tmpl.hh:47-75`): see §2.2 for decoded facet lists. Hex8 and Hex20 facet *orderings differ*. |
| FE-004 | Natural-coordinate containment test | `element_class_tmpl.hh:116-168` (`GeometricalShapeContains<_gst_point/_gst_square/_gst_triangle/_gst_prism>`) | point: `x0 < eps`; square: each `abs(ξ_i) <= 1+eps` (`eps`=DBL_EPSILON); triangle/tet: `ξ_i ∈ [-tol, 1+tol]` and `Σξ ≤ 1+tol` (`tol`=`Math::tolerance`); prism: `ξ0∈[-1,1]`, `ξ1,ξ2 ≥0`, `ξ1+ξ2 ≤ 1` (no tolerance). | core | — | `test_inverse_map.cc` (contains==true) | used by node/element search | Different tolerances per shape must be kept exactly. |
| FE-005 | Interpolation primitives | `element_class.hh:160-270`, `element_class_tmpl.hh:170-270` (`computeShapes`, `computeDNDS`, `computeD2NDS2`, `interpolate`, `interpolateOnNaturalCoordinates`, `gradientOnNaturalCoordinates`) | `N` vector per point; `dnds` = `natural_dim × nnodes` (`dnds(i,j)=∂N_j/∂s_i`); `interpolate(nodal(dof×nnodes), N) = nodal*N`; gradient wrt natural coords `f * dnds^T`. `computeD2NDS2` only for seg2 (zero), tri3 (zero), quad4 (non-zero rows 1,2 = ±1/4); others not implemented. | core | — | `test_fe_engine_precomputation.cc` (vs `py_engine.py`), `test_gradient.cc`, `test_interpolate.cc` (not registered) | — | Formulas per type in ELEM rows / §2.3. |
| FE-006 | Jacobian matrix & determinant | `element_class_tmpl.hh:295-370` (`computeJMat`, `computeJacobian`) | `J = dnds · Xᵀ` (`natural_dim × spatial_dim`, `J(i,j)=∂x_j/∂s_i`). `computeJacobian`: square → `det(J)` (signed); non-square: natural dim 1 → `‖J‖`; natural dim 2 in 3D → `‖J.row0 × J.row1‖`; otherwise 0. | core | — | `test_fe_engine_precomputation.cc` ("j"), `test_integrate.cc` | — | Signed det for square case; negative values trigger an exception in `IntegratorGauss::checkJacobians` (FE-013). |
| FE-007 | Physical shape derivatives | `element_class_tmpl.hh:376-396` (`computeShapeDerivatives`), `shape_lagrange_inline_impl.hh:62-84` (`computeShapeDerivativesOnCPointsByElement`) | `dNdx = J⁻¹ · dnds` (`spatial_dim × nnodes`); only computed when natural dim == mesh spatial dim (`shape_lagrange_inline_impl.hh:52-56`). | core | — | `test_fe_engine_precomputation.cc` ("B"), `test_gradient.cc` | — | Uses Eigen `inverse()` (closed form for ≤4×4). Bitwise equality requires same inversion formula (cofactor/determinant). |
| FE-008 | Normals from natural coords | `element_class_tmpl.hh:398-420` (`computeNormalsOnNaturalCoordinates`) | Requires `spatial_dim-1 == natural_dim`; `J = ∂x/∂s` via gradient; 2D: `Math::normal(J)` = `(J1,-J0)/‖·‖`; 3D: `normalize(J.col0 × J.col1)`. `_point_1` specialisation does nothing. | core | — | `test_mesh_boundary`? (no), used by contact/BC tests (R3) | — | Orientation follows node order of facet (outward if facet listed as in §2.2). |
| FE-009 | Inverse mapping (real → natural) | `element_class_tmpl.hh:425-575` (`inverseMap` vector & matrix overloads; defaults `max_iterations=100`, `tolerance=1e-10`) | Gauss–Newton: initial guess `ξ=0`; residual `f = x - Σ N_i(ξ) x_i`; `Jt = ∂x/∂ξ (sp×nat)`, `G = JtᵀJt`, `dξ = (Jt G⁻¹)ᵀ f`; stop when `‖f‖ ≤ tol` (absolute) or throws `"The solver in inverse map did not converge"` after 100 iterations. | core | — | `test/test_fe_engine/test_inverse_map.cc` (tolerance 3.5e-11 on normalised distance) | used for node-to-element search in contact / embedded | Absolute tolerance 1e-10 (not scaled with element size). For tet4 the natural coordinates follow the non-standard node mapping (§2.3, ELEM-008). |
| FE-010 | Element characteristic length ("inradius") | `GeometricalElement<_gt_*>::getInradius` in each `element_classes/*_inline_impl.hh`; `fe_engine_inline_impl.hh:34-52` (`FEEngine::getElementInradius(coord,type)`, `getElementInradius(Element)`) | Per type (see ELEM rows): seg2 = length; seg3 = min half-length; tri3 = 2·inradius; tri6 = 2·min inradius of 4 sub-triangles; quad4/quad8 = min edge (sub-edge); tet4 = 2·inradius; tet10 = 2·min inradius of 4 corner sub-tets; hex8 = min of 12 edges; hex20 = ½·hex8 value (corner nodes); penta6 = 2·min(inradius of 2 triangles, ½ of 3 vertical edges); penta15 = ½·penta6; point/other = 0. | core | — | `test/test_fe_engine/test_fe_engine_inradius.cc` with `_<type>.csv` reference per element (tol 1e-6) | — | Used for stable time step (`getStableTimeStep`, R2). Values in csv are 6-digit; port must reproduce formulas exactly. |
| FE-011 | Runtime element-class helper | `element_class_helper.hh` (`ElementClassHelper<_ek_regular>::getN`, `getDNDS`, `getD2NDS2`, `getJMat`) | Runtime-type wrappers returning dynamic `Vector/Matrix`. | core | — | — | — | Used by Python bindings (`ElementClassHelper` exposed?) and contact. |
| FE-012 | Element→interpolation type conversion | `element_type_conversion.hh` (`convertType<ElementType,InterpolationType>`), `FEEngine::getInterpolationType` | Lookup of `ElementClassProperty<type>::interpolation_type`, `_itp_not_defined` otherwise. | core | — | — | — | Shape storage is keyed by interpolation type (FE-017). |
| FE-013 | Gauss integrator (precomputed) | `integrator.hh`, `integrator_gauss.hh`, `integrator_gauss_inline_impl.hh` (`IntegratorGauss<kind,IOF>::initIntegrator`, `computeQuadraturePoints`, `precomputeJacobiansOnQuadraturePoints`, `checkJacobians`, `multiplyJacobiansByWeights`, `integrate(...)`, `integrateOnElement`, `getIntegrationPoints`, `getNbIntegrationPoints`, `onElementsAdded`) | Stores per type/ghost `jacobians` = `det(J)·w` per quad point (Array size `nb_elem*nq`, 1 comp). Init order: quad points (default order = polynomial degree) → raw dets → **throw `IntegratorGaussException` if any det < 0** ("Negative jacobian computed, possible problem in the element node ordering") → multiply by weights. `integrate(f, intf, nb_dof)`: per element `intf = F(nb_dof×nq) · Jw`; scalar `integrate(f)` = `Math::reduce` of per-element integrals; `integrate(Vector f, index)` for one element. Filters supported. | core | — | `test_integrate.cc` (IntegrateConstant, tol 1e-14), `test_fe_engine_precomputation.cc`, `test_fe_engine_gauss_integration.cc` | ghost elements integrated separately (per ghost type) | Quad-point field layout: per element, quad points contiguous, each with `nb_dof` comps; per element view `(nb_dof × nq)` col-major. |
| FE-014 | Arbitrary-order integration (non-precomputed) | `integrator_gauss_inline_impl.hh:125-150, 440-492` (`getIntegrationPoints<type,n>()`, `getIntegrationWeights<type,n>()`, `integrate<type,n>(f, intf, nb_dof, ghost)`, `integrate<type,n>(f, ghost)`) | Computes jacobians on the fly with the order-`n` rule, multiplies by order-`n` weights, integrates; scalar version uses `Math::reduce`. | core | — | `test/test_fe_engine/test_fe_engine_gauss_integration.cc` (ArbitraryOrder: degrees 0..5 polynomials integrated exactly, tol 5e-14) | — | Order→rule mapping in QUAD rows; used by consistent mass (FE-031). |
| FE-015 | Cohesive-element Jacobians | `integrator_gauss_inline_impl.hh:318-360` (`IntegratorGauss<_ek_cohesive>::computeJacobiansOnIntegrationPoints`) | Jacobian of the **mid-surface**: node `n` coordinate = `(X_n + X_{n+half})/2`; `_cohesive_1d_2` → `J=1`. | cohesive_element | — | cohesive tests (R2) | — | — |
| FE-016 | Structural-element Jacobians | `integrator_gauss_inline_impl.hh:240-316` (`IntegratorGauss<_ek_structural>::computeJacobiansOnIntegrationPoints`) | Rotates nodal coordinates with element rotation matrix `R` (from `computeRotationMatrix`, using mesh data `"extra_normal"` if present, else zero vector) and uses first `natural_dim` rows. | structural_mechanics | — | `test_fe_engine_precomputation_structural.cc` | — | Requires per-element `extra_normal` mesh data for `_bernoulli_beam_3`. |
| FE-017 | Shape-function storage base | `shape_functions.hh`, `shape_functions.cc`, `shape_functions_inline_impl.hh` (`ShapeFunctions::shapes/shapes_derivatives` = `ElementTypeMapArray<Real, InterpolationType>`, `integration_points`, `setIntegrationPointsByType`, `getShapes`, `getShapesDerivatives`, `getShapeSize`, `getShapeDerivativesSize`) | Shapes stored per **interpolation type** and ghost type, **per element per quad point** (Array size `nb_elem*nq`, comps = nnodes); derivatives comps = `nnodes*spatial_dim` laid out as `(dim × nnodes)` col-major (`∂N0/∂x, ∂N0/∂y, ∂N1/∂x, ...`). | core | — | `test_fe_engine_precomputation.cc` | — | Redundant per-element storage of N is part of the observable API (`getShapes`). |
| FE-018 | Lagrange shapes precomputation | `shape_lagrange.hh`, `shape_lagrange_inline_impl.hh:30-58, 150-220`, `shape_lagrange_base.{hh,cc}`, `shape_lagrange_base_inline_impl.hh` (`initShapeFunctions`, `precomputeShapesOnIntegrationPoints`, `precomputeShapeDerivativesOnIntegrationPoints`, `computeShapesOnIntegrationPoints`, `computeShapeDerivativesOnIntegrationPoints`, `onElementsAdded/Removed`) | Computes N and dN/dx at the integrator's quad points for every element; derivatives skipped for elements with natural dim ≠ mesh dim (facets). Element addition computes only new elements; removal renumbers via `onElementsRemoved(new_numbering)`. | core | — | `test_fe_engine_precomputation.cc` (tol 1e-10 vs python) | — | — |
| FE-019 | Interpolation on integration points | `shape_lagrange_inline_impl.hh:300-342`, `shape_functions_inline_impl.hh:215-258` (`interpolateOnIntegrationPoints`, `interpolateElementalFieldOnIntegrationPoints`), `fe_engine_template_tmpl.hh:190-260` | `u_q = U_el(nb_dof×nnodes) · N(nnodes×nq)` per element; ElementTypeMapArray overload iterates ghost types × types present in output map. | core | — | `test_interpolate.cc` (**not registered in CMake**), `test_fe_engine_gauss_integration.cc` (indirect), `test_variational_operators.cc` | — | — |
| FE-020 | Gradient on integration points | `shape_lagrange_inline_impl.hh:342-370`, `shape_functions_inline_impl.hh:258-310` (`gradientOnIntegrationPoints`, `gradientElementalFieldOnIntegrationPoints`), `fe_engine_template_tmpl.hh:44-80` | `∇u_q = U_el · Bᵀ` → `(nb_dof × dim)` col-major per quad point (component `i + j*nb_dof` = `∂u_i/∂x_j`); point elements (natural dim 0) return zeros. Dispatch guard `if (element_dimension == ElementClass<type>::getSpatialDimension())` (`fe_engine_template_tmpl.hh:87`): in debug builds `element_dimension` is a local = `mesh.getSpatialDimension(type)` (always true), in release (`AKANTU_NDEBUG`) the local is compiled out and the **FEEngine member** `element_dimension` is used → gradient silently skipped (output resized, values left unset) for types whose dimension differs from the engine's (`:66-82`). | core | — | `test/test_fe_engine/test_gradient.cc` (GradientPoly tol 5e-13, GradientPositions tol 2e-14) | — | Layout of displacement gradient is consumed by all materials. |
| FE-021 | `BᵀD` operator | `shape_lagrange_inline_impl.hh:371-410` (`computeBtD`) | Per quad point `BtD = D(ncomp/dim × dim) · B(dim × nnodes)` (stress·∇N), output `(ncomp/dim × nnodes)` col-major = node-major dof vector. | core | — | `test_variational_operators.cc` (consistent_force) | — | Used for internal force assembly. |
| FE-022 | `BᵀDB` operator | `shape_lagrange_inline_impl.hh:411-465` (`computeBtDB`, `order_d` 2 or 4) | `order_d==4`: Voigt B (CORE-022) then `BᵀDB` (`dim*nnodes` square, node-major); `order_d==2`: `BᵀDB` with `D` `dim×dim` (`nnodes` square). Other orders: silently nothing. | core | — | `test_variational_operators.cc` (stiffness_matrix) | — | Uses `ElementClass<type>::getSpatialDimension()` (element dim) — only valid when element dim == mesh dim. |
| FE-023 | `Nᵀb` and `NᵀbN` operators | `shape_lagrange_inline_impl.hh:466-530` (`computeNtb`, `computeNtbN`) | `Ntb = b(nb_dof×1)·N(1×nnodes)` → node-major (`b0N0,b1N0,..`); `NtbN = Nᵀ b N` (only meaningful for scalar `b`). | core | — | `test_variational_operators.cc` | — | — |
| FE-024 | Evaluation at arbitrary real coordinates | `shape_lagrange_inline_impl.hh:86-170` (`inverseMap`, `contains`, `interpolate`, `computeShapes(real)`, `computeShapeDerivatives(real)`), `fe_engine_template_tmpl.hh:645-735` | Uses inverseMap (FE-009) per element then natural-coordinate evaluation; derivatives via J⁻¹·dnds at those points. Cohesive kind: `AKANTU_TO_IMPLEMENT`; structural: interpolate/computeShapes `AKANTU_TO_IMPLEMENT`. | core | — | `test_inverse_map.cc` | — | — |
| FE-025 | Elemental-field interpolation from integration points | `shape_functions.cc:37-215`, `shape_functions_inline_impl.hh:60-205` (`initElementalFieldInterpolationFromIntegrationPoints`, `interpolateElementalFieldFromIntegrationPoints`, `buildInterpolationMatrix`, `BuildElementalFieldInterpolationMatrix<_quadrangle_4/_quadrangle_8>`), `fe_engine_template_tmpl.hh:395-470` | Fits a polynomial through quad-point values (order 1: constant; order 2: `[1, x, y, (z)]`; quad4/quad8: `[1,x,y,xy]`) using **physical** quad-point coordinates; matrix `nq×nq` inverted per element; evaluates at given interpolation points. Requires element filter. Only regular types; orders >2 not implemented. | core | — | non-local / material interpolation tests (R2) | — | Used by `MaterialNonLocal`/`interpolateStress` (R2). Inversion of physical-coordinate Vandermonde-like matrix can be ill-conditioned. |
| FE-026 | Nodal→elemental extraction & elemental filtering | `fe_engine_inline_impl.hh:94-170` (`FEEngine::extractNodalToElementField`, `filterElementalData`) | Gathers nodal values per element (`nnodes × nb_comp` per element tuple, node-major); filters elemental data by index list. | core | — | indirectly | — | — |
| FE-027 | FEEngine interface and template wiring | `fe_engine.hh`, `fe_engine.cc`, `fe_engine_template.hh` (`FEEngineTemplate<I,S,kind,IOF>`, `DefaultIntegrationOrderFunctor`), `fe_engine_template_tmpl.hh:28-120` | Abstract API (integrate, interpolate, gradient, BtD, BtDB, Ntb, NtbN, normals, assembly, inverse map, contains, inradius...). Template binds an Integrator and ShapeFunctions per kind; ctor registers as mesh event handler (`_ehp_fe_engine`) and precomputes (not_ghost then ghost) unless `do_not_precompute`. `initShapeFunctions` loops `mesh.elementTypes(element_dimension, ghost, kind)`. `elementTypes()` iterates types with jacobians. The `element_dimension` ctor argument (default = mesh dimension) is how models build FE engines **on facets** (e.g. `dim−1` engines on `mesh_facets` for cohesive/contact/BC integration): N and jacobians are computed, physical shape derivatives are not (FE-018). | core | — | all FE tests (`TestFEMFixture`) | init for ghost elements too | Rust: trait + generic struct; `IntegrationOrderFunctor` customization (cohesive model uses its own, R2). |
| FE-028 | Integration-point coordinates | `fe_engine_template_tmpl.hh:355-395` (`computeIntegrationPointsCoordinates` ×2) | Interpolates nodal coordinates to quad points. | core | — | `test_fe_engine_gauss_integration.cc` | — | — |
| FE-029 | Normals on integration points | `fe_engine_template_tmpl.hh:498-640` (`computeNormalsOnIntegrationPoints`, `_point_1` specialisation) | For facet-dim elements (`element_dimension = mesh dim -1` typically): normals per quad point (FE-008). `_point_1` in 1D: ±1 from `coords(elem) - coords(other segment node)` using `getElementToSubelement` (parent mesh if mesh of facets); value 1 if attached to ≠1 segment. | core | — | (BC/contact tests R3) | — | **Bug**: point normal uses `coords(elem)` (element index as node index) instead of `coords(point)` (`fe_engine_template_tmpl.hh:614`). Results correct only if point element i ≡ node i. |
| FE-030 | Lumped field assembly (lumped mass/capacity) | `fe_engine_template_tmpl_field.hh:33-232` (`assembleFieldLumped`, `assembleLumpedRowSum`, `assembleLumpedDiagonalScaling`, `fillField`) | Linear types: row-sum `M̃_i = ∫ρN_i` (default quadrature). Quadratic types (`_triangle_6, _quadrangle_8, _tetrahedron_10, _hexahedron_20, _pentahedron_15`): diagonal scaling `M̃_i = c_i ∫ρ` with factors tri6 corner 1/12 mid 1/4; tet10 1/32, 7/48; quad8 3/76, 16/76; hex20 7/248, 16/248; penta15 corners 51/2358, tri-mid nodes (6,7,8,12,13,14) 192/2358, quad-mid (9,10,11) 300/2358. Assembled via `DOFManager::assembleElementalArrayLocalArray`. | core | — | `test_solid_mechanics_model_*` mass tests (R2) | assembled per ghost type; DOF manager handles ghosts (R3) | Coefficients exact rationals; the field functor is called once per element with `(nb_dof × nq)` matrix. |
| FE-031 | Consistent field matrix assembly (mass) | `fe_engine_template_tmpl_field.hh:265-400` (`assembleFieldMatrix`, `ShapesForMassHelper`) | Uses order `2*polynomial_degree` quadrature (non-precomputed, FE-014), `N` extended to `(nb_dof × nb_dof*nnodes)` block form (node-major), local `Nᵀ diag(ρ) N` per quad point, integrated and assembled with `assembleElementalMatricesToMatrix`. Cohesive kind: not implemented. Structural: uses `computeShapesMassOnIntegrationPoints`. | core | — | `test_solid_mechanics_model_*` implicit dynamic (R2) | — | Rule selection per type: see §2.4 "mass order" table. |
| FE-032 | Mesh-event handling in FEEngine | `fe_engine_template_tmpl.hh:780-815` (`onElementsAdded`, `onElementsRemoved`, `onElementsChanged` (no-op)) | On added elements: integrator recomputes quad points + jacobians for new elements, shape functions get integration points then compute shapes for new elements. Removal renumbers jacobians/shapes. | core | — | cohesive insertion tests (R2) | — | — |
| FE-033 | Cohesive shape functions | `shape_cohesive.hh`, `shape_cohesive_inline_impl.hh` (`ShapeLagrange<_ek_cohesive>`, `CohesiveReduceFunctionMean`, `CohesiveReduceFunctionOpening`, `extractNodalToElementField<type,Reduce>`, `interpolateOnIntegrationPoints<type,Reduce>`, `variationOnIntegrationPoints`, `computeNormalsOnIntegrationPoints<type,Reduce>`) | Cohesive connectivity viewed as `(nnodes/2 × 2)`: column 0 = side "plus" nodes, column 1 = "minus". Reduce: mean `½(u⁺+u⁻)` or opening `u⁺−u⁻`. **Shape derivatives stored are natural `dN/ds`** (no Jacobian). Normals: 2D `n=(-t1,t0)/‖t‖` where `t=∂x̄/∂s`; 3D `n = normalize(t0×t1)`; 1D sign from barycentres of adjacent segments via mesh facets. BtD/BtDB/Ntb/NtbN not implemented. | cohesive_element | — | cohesive element tests (R2) | — | Plus/minus side convention must match cohesive inserter (MUTIL rows). |
| FE-034 | FEEngine cohesive specialisations | `fe_engine_template_cohesive.cc` | `integrate` (scalar & field) specialised for cohesive kind; `gradientOnIntegrationPoints` throws not-implemented. | cohesive_element | — | — | — | — |
| FE-035 | Structural shape functions | `shape_structural.{hh,cc}`, `shape_structural_inline_impl.hh` (`ShapeStructural<_ek_structural>`: `precomputeRotationMatrices`, `computeShapesOnIntegrationPointsInternal`, `computeShapesMassOnIntegrationPoints`, `precomputeShapesOnIntegrationPoints`, `precomputeShapeDerivativesOnIntegrationPoints`, `interpolateOnIntegrationPoints`, `gradientOnIntegrationPoints`, `computeBtD`, `computeNtb`, `rotation_matrices`) | Per element rotation `R (ndof×ndof)`; `T` = block-diag(R) over nodes; shapes stored as `N·T` (`ndof × ndof*nnodes` per quad); derivatives `B·T` in Voigt layout (`nb_stress × ndof*nnodes`); gradient = strains `B u`; `BtD = Bᵀ D`; `Ntb = Nᵀ b`. | structural_mechanics | — | `test_fe_engine_precomputation_structural.cc` (PrecomputeRotations for B2/B3; DKT18 test DISABLED), `test_fe_engine_precomputation_bernoulli_2/3.cc` (**not registered**) | — | Shape sizes: `getShapeSize = nnodes*ndof*ndof`, derivative size `nnodes*ndof*nb_stress`. |
| FE-036 | FEEngine structural specialisations | `fe_engine_template_tmpl_struct.hh` | `AssembleFieldMatrixStructHelper` dispatch; the old-style `assembleFieldMatrix(field, ndof, M, n, rotation, ghost)` is `AKANTU_TO_IMPLEMENT`. | structural_mechanics | — | — | — | Effectively dead path; mass for structural goes through FE-031 with `ShapesForMassHelper<_ek_structural>`. |
| FE-037 | Facet type → cohesive type | `fe_engine_inline_impl.hh:68-80` (`getCohesiveElementType`), `cohesive_element.hh:56-70` (`CohesiveFacetProperty`) | `_point_1→_cohesive_1d_2`, `_segment_2→_cohesive_2d_4`, `_segment_3→_cohesive_2d_6`, `_triangle_3→_cohesive_3d_6`, `_triangle_6→_cohesive_3d_12`, `_quadrangle_4→_cohesive_3d_8`, `_quadrangle_8→_cohesive_3d_16`; else `_not_defined`. | cohesive_element | — | cohesive tests | — | — |
| FE-038 | IGFEM element-type hook | `fe_engine_inline_impl.hh:82-92` (`getIGFEMElementTypes`) | Dispatches to `IGFEMHelper::getIGFEMElementTypes<type>()`. | IGFEM (extra package) | — | `extra_packages/igfem/test/test_fe_engine/*` | — | **Bug**: missing `return` in the function (UB if compiled with IGFEM). |
| FE-039 | Integration-order functor | `fe_engine_template.hh:45-50` (`DefaultIntegrationOrderFunctor::getOrder<type>()` = `polynomial_degree`) | Pluggable default quadrature order per type (cohesive model provides another functor, R2). | core | — | — | — | — |

### 1.3 ELEM — element types (one row per element type)

Natural coordinates, node numbering, facets, quadrature and formulas are given
precisely in §2.1–§2.4; rows below summarise and point to the source.
"Default quad" = rule used by the precomputed integrator (order = polynomial
degree); "mass quad" = rule used by `assembleFieldMatrix` (order = 2×degree).

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| ELEM-001 | `_point_1` | `element_classes/element_class_point_1_inline_impl.hh:41-80`; `geometrical_element_property.hh:49-56,362-365` | 1 node, natural dim 0, spatial dim 0, `N=1`, no dnds, `_git_point` order 1 (1 point at 0, weight 1), facet = itself, inradius 0, normals: no-op in ElementClass, special FE-029 path. | core | — | (no `_point_1.msh`; excluded from typed FE tests `test_gtest_utils.hh:180`) | facets of 1D meshes | Gmsh type 15. Used as facet of segments and as cohesive 1D facet. |
| ELEM-002 | `_segment_2` | `element_class_segment_2_inline_impl.hh:44-95`; `geometrical_element_property.hh:58-65,367-370` | Nodes ξ=−1, +1. `N=(½(1−ξ), ½(1+ξ))`, `dN/dξ = (−½, ½)`, `d2N=0`. Default quad: order 1 → 1 point ξ=0, w=2. Mass: order 2 → 2 points. Facets: 2 × `_point_1` `(0),(1)`. Inradius = length. `getNormal` (2D) via `Math::normal(c0−c1)`. | core | — | `_segment_2.msh`, `_segment_2.csv` in `test/test_fe_engine` (all typed FE tests); `test_buildfacets` | — | `GeometricalElement<_gt_segment_2>::getNormal` calls the 2-vector `Math::normal` overload with an output arg — broken, but `GeometricalElement::getNormal` is **never called** anywhere in `src/`/`python/` (dead code; same for seg3). Gmsh type 1. |
| ELEM-003 | `_segment_3` | `element_class_segment_3_inline_impl.hh:56-105`; `geometrical_element_property.hh:67-76,372-375` | Nodes ξ=−1, +1, 0 (**mid node last**). `N=(ξ(ξ−1)/2, ξ(ξ+1)/2, 1−ξ²)`, `dN=(ξ−½, ξ+½, −2ξ)`. Default quad: order 2 → 2 Gauss points ±1/√3, w=1. Mass: order 4 → 3 points. p1 type `_segment_2`. Facets 2×`_point_1` `(0),(1)`. Inradius = min(dist(x1,x0), dist(x2,x1)) (note: uses node1−node0 = full length and node2−node1 = half). | core | — | `_segment_3.msh/.csv` | — | Inradius formula mixes full and half length (as coded). `getNormal` evaluates at ξ=½. Gmsh type 8. |
| ELEM-004 | `_triangle_3` | `element_class_triangle_3_inline_impl.hh:52-120` | Nodes (0,0),(1,0),(0,1). `N=(1−ξ−η, ξ, η)`; dnds rows `[−1,1,0]`, `[−1,0,1]`; `d2N=0`. Default quad order 1: (1/3,1/3), w=½. Mass: order 2 → 3 points. Facets 3×`_segment_2`: (0,1),(1,2),(2,0). Inradius = 2·inradius. | core | — | `_triangle_3.msh/.csv`; `test_buildfacets/_triangle_3.msh`, `_quadrangle_4_triangle_3.msh` | — | Gmsh type 2. |
| ELEM-005 | `_triangle_6` | `element_class_triangle_6_inline_impl.hh:88-170` | Nodes 0..2 corners, 3=(½,0), 4=(½,½), 5=(0,½). With `c0=1−ξ−η`: `N_i = c_i(2c_i−1)` (i=0..2), `N3=4c0ξ`, `N4=4ξη`, `N5=4ηc0`. Default quad order 2 → 3 points (1/6,1/6),(2/3,1/6),(1/6,2/3), w=1/6. Mass: order 4 → 6-point rule. Lumped: diagonal scaling 1/12, 1/4. Facets 3×`_segment_3`: (0,1,3),(1,2,4),(2,0,5). Inradius 2·min over sub-triangles {0,3,5},{3,1,4},{3,4,5},{5,4,2}. | core | — | `_triangle_6.msh/.csv`; `test_buildfacets/_triangle_6.msh`, `_quadrangle_8_triangle_6.msh` | — | Gmsh type 9. |
| ELEM-006 | `_quadrangle_4` | `element_class_quadrangle_4_inline_impl.hh:64-150` | Nodes (−1,−1),(1,−1),(1,1),(−1,1). Bilinear `N_i=¼(1±ξ)(1±η)`. `d2N` rows 1 and 2 = `[¼,−¼,¼,−¼]` (cross derivative stored twice), row 0/3 zero. Default quad order 2 → 2×2 Gauss (x fastest). Mass: order 4 → 3×3. Facets 4×`_segment_2`: (0,1),(1,2),(2,3),(3,0). Inradius = min edge length. Elemental-field interpolation basis `[1,x,y,xy]`. | core | — | `_quadrangle_4.msh/.csv`; `test_buildfacets/_quadrangle_4.msh` | — | Gmsh type 3. |
| ELEM-007 | `_quadrangle_8` | `element_class_quadrangle_8_inline_impl.hh:82-170` | Serendipity: corners as quad4, mid 4=(0,−1),5=(1,0),6=(0,1),7=(−1,0). `N0=¼(1−ξ)(1−η)(−1−ξ−η)` etc., mid `½(1−ξ²)(1∓η)`, `½(1±ξ)(1−η²)`. Default quad order 3 → **2×2 Gauss**. Mass: order 6 → 4×4. Lumped: 3/76, 16/76. Facets 4×`_segment_3`: (0,1,4),(1,2,5),(2,3,6),(3,0,7). Inradius = min of 8 half-edges. Interpolation basis `[1,x,y,xy]` (4 quad points). | core | — | `_quadrangle_8.msh/.csv`; `test_buildfacets/_quadrangle_8.msh` | — | Gmsh type 16. No `computeD2NDS2`. |
| ELEM-008 | `_tetrahedron_4` | `element_class_tetrahedron_4_inline_impl.hh:57-135` | **Non-standard natural mapping in code**: `N = (1−ξ−η−ζ, η, ζ, ξ)` (`:65-78`) and dnds columns consistent with it (`:82-120`) — i.e. node 1 ↔ η-axis, node 2 ↔ ζ-axis, node 3 ↔ ξ-axis; the doc comment (`:39-44`) claims `N2=ξ, N3=η, N4=ζ`. Default quad order 1: (¼,¼,¼), w=1/6. Mass: order 2 → 4 points. Facets 4×`_triangle_3`: (0,2,1),(1,2,3),(2,0,3),(0,1,3). Inradius 2·inradius (CORE-023). | core | — | `_tetrahedron_4.msh/.csv`; `test_buildfacets` (via tet10 mesh); `test_geometry_tetrahedron.msh` | — | Physical shape derivatives, jacobians and integrals are invariant under this permutation (Lagrange basis), so `B`, `j`, `N@quad` match the Python reference; **but natural coordinates (inverseMap output, `computeShapes(natural)` callers, contact projections) differ from the standard/tet10 convention** — must be replicated bit-for-bit. Gmsh type 4. |
| ELEM-009 | `_tetrahedron_10` | `element_class_tetrahedron_10_inline_impl.hh:132-273` | Standard: corners (0,0,0),(1,0,0),(0,1,0),(0,0,1); 4=(½,0,0), 5=(½,½,0), 6=(0,½,0), 7=(0,0,½), 8=(½,0,½), 9=(0,½,½). `N0=c0(1−2Σ)`, `N1=ξ(2ξ−1)`... `N9=4ηζ`. Default quad order 2 → 4 points (a=(5−√5)/20, b=(5+3√5)/20), w=1/24. Mass: order 4 → 15 points. Lumped 1/32, 7/48. Facets 4×`_triangle_6`: (0,2,1,6,5,4),(1,2,3,5,9,8),(2,0,3,6,7,9),(0,1,3,4,8,7). Inradius 2·min over corner sub-tets {0,4,6,7},{4,1,5,8},{6,5,2,9},{7,8,9,3}. | core | — | `_tetrahedron_10.msh/.csv`; `test_buildfacets/_tetrahedron_10.msh` | — | Gmsh type 11 with node reordering (IO-00x). |
| ELEM-010 | `_pentahedron_6` | `element_class_pentahedron_6_inline_impl.hh:59-140` | Natural coords (x along prism axis ∈[−1,1]; (y,z) triangle): N0=(−1,1,0), N1=(−1,0,1), N2=(−1,0,0), N3=(1,1,0), N4=(1,0,1), N5=(1,0,0). `N0=½y(1−x)`, `N1=½z(1−x)`, `N2=½(1−y−z)(1−x)`, `N3..5` with `(1+x)`. Default quad order 1 → (x_order 3 → 2 pts) × (yz_order 2 → 3 pts) = **6 points**, w = 1·(1/6). Mass: order 2 → same 6 points. Facets: 2×`_triangle_3` (0,2,1),(3,4,5); 3×`_quadrangle_4` (0,1,4,3),(0,3,5,2),(1,2,5,4). Inradius 2·min(tri inradii of (0,1,2),(3,4,5), ½·dist(x3,x0), ½·dist(x5,x2), ½·dist(x4,x1)). | core | — | `_pentahedron_6.msh/.csv`; `test_buildfacets/_pentahedron_6.msh`, `_hexahedron_8_pentahedron_6.msh`; `pentahedron.geo/.msh` | — | Two facet types → `getNbFacetTypes()==2`. Gmsh type 6. |
| ELEM-011 | `_pentahedron_15` | `element_class_pentahedron_15_inline_impl.hh:70-185` | Nodes 0..5 as penta6; 6=(−1,½,½), 7=(−1,0,½), 8=(−1,½,0), 9=(0,1,0), 10=(0,0,1), 11=(0,0,0), 12=(1,½,½), 13=(1,0,½), 14=(1,½,0). Shape functions `:78-96`. Default quad order 2 → (3,2) → **6 points** (same as penta6). Mass: order 4 → (5,5) → 3×7 = 21 points. Lumped: corners 51/2358, tri-mid 192/2358, quad-mid 300/2358. Facets: 2×`_triangle_6` (0,2,1,8,7,6),(3,4,5,12,13,14); 3×`_quadrangle_8` (0,1,4,3,6,10,12,9),(0,3,5,2,9,14,11,8),(1,2,5,4,7,11,13,10). Inradius ½·penta6 formula. | core | — | `_pentahedron_15.msh/.csv`; `test_buildfacets/_pentahedron_15.msh`, `_hexahedron_20_pentahedron_15.msh` | — | Gmsh type 18 with reordering. Only 6 default quadrature points for a quadratic prism (under-integration) — keep. |
| ELEM-012 | `_hexahedron_8` | `element_class_hexahedron_8_inline_impl.hh:106-213` | Nodes (−1,−1,−1),(1,−1,−1),(1,1,−1),(−1,1,−1),(−1,−1,1),(1,−1,1),(1,1,1),(−1,1,1). Trilinear `⅛(1±ξ)(1±η)(1±ζ)`. Default quad order 2 → 2×2×2 Gauss (x fastest, then y, then z — **differs from the doc comment order** `:60-80`). Mass: order 4 → 27 points. Facets 6×`_quadrangle_4`: (0,3,2,1),(0,1,5,4),(1,2,6,5),(2,3,7,6),(3,0,4,7),(4,5,6,7). Inradius = min of 12 edges. | core | — | `_hexahedron_8.msh/.csv`; `test_buildfacets/_hexahedron_8.msh` | — | Gmsh type 5. |
| ELEM-013 | `_hexahedron_20` | `element_class_hexahedron_20_inline_impl.hh:71-217` | Serendipity: corners as hex8; mid-edge 8=(0,−1,−1), 9=(1,0,−1), 10=(0,1,−1), 11=(−1,0,−1), 12=(−1,−1,0), 13=(1,−1,0), 14=(1,1,0), 15=(−1,1,0), 16=(0,−1,1), 17=(1,0,1), 18=(0,1,1), 19=(−1,0,1). Default quad order 3 → 2×2×2. Mass: order 6 → 4×4×4 = 64. Lumped 7/248, 16/248. Facets 6×`_quadrangle_8`: (0,1,5,4,8,13,16,12),(1,2,6,5,9,14,17,13),(2,3,7,6,10,15,18,14),(3,0,4,7,11,12,19,15),(0,3,2,1,11,10,9,8),(4,5,6,7,16,17,18,19). Inradius ½·hex8 formula on corners. | core | — | `_hexahedron_20.msh/.csv`; `test_buildfacets/_hexahedron_20.msh` | — | **Facet ordering differs from hex8** (hex20 facet 0 is hex8 facet 1, ..., hex20 facet 4 is hex8 facet 0). Gmsh type 17 with reordering. |
| ELEM-014 | `_cohesive_1d_2` | `cohesive_element.hh:31-33`; `geometrical_element_property.hh:275-285,440-443` | 2 nodes (one per side), interpolation `_itp_lagrange_point_1`, spatial dim 1, `_git_point` order 1 (1 point). Facets 2×`_point_1` (0),(1). Jacobian forced to 1. | cohesive_element | — | cohesive 1D tests (R2) | — | Normal sign via facet barycentres (FE-033). |
| ELEM-015 | `_cohesive_2d_4` | `cohesive_element.hh:34-36`; `geometrical_element_property.hh:246-258,430-433` | 4 nodes: side⁺ (0,1), side⁻ (2,3); interpolation `_itp_lagrange_segment_2`; `_git_segment` order 2 → 2 Gauss points. Facets 2×`_segment_2`: (0,1),(2,3). | cohesive_element | — | R2 cohesive tests | — | Gmsh has no cohesive type; written as? (see DUMP/IO rows: dumper maps cohesive types to VTK). |
| ELEM-016 | `_cohesive_2d_6` | `cohesive_element.hh:37-39`; `geometrical_element_property.hh:260-273` | 6 nodes: (0,1,2)⁺, (3,4,5)⁻; interpolation `_itp_lagrange_segment_3`; order 3 → 2 Gauss points. p1 `_cohesive_2d_4`. | cohesive_element | — | R2 | — | — |
| ELEM-017 | `_cohesive_3d_6` | `cohesive_element.hh:40-42`; `geometrical_element_property.hh:287-300` | 6 nodes (0,1,2)⁺,(3,4,5)⁻; `_itp_lagrange_triangle_3`; triangle order 2 → 3 points. | cohesive_element | — | R2 | — | — |
| ELEM-018 | `_cohesive_3d_12` | `cohesive_element.hh:43-45`; `geometrical_element_property.hh:302-318` | 12 nodes (0..5)⁺,(6..11)⁻; `_itp_lagrange_triangle_6`; triangle order 3 → **4-point rule with negative weight** (−27/96). p1 `_cohesive_3d_6`. | cohesive_element | — | R2 | — | Negative quadrature weight → jacobian·w negative at one point (not checked by `checkJacobians`, which runs before weighting). |
| ELEM-019 | `_cohesive_3d_8` | `cohesive_element.hh:46-48`; `geometrical_element_property.hh:320-334` | 8 nodes (0..3)⁺,(4..7)⁻; `_itp_lagrange_quadrangle_4`; segment order 2 → 2×2. | cohesive_element | — | R2 | — | — |
| ELEM-020 | `_cohesive_3d_16` | `cohesive_element.hh:49-51`; `geometrical_element_property.hh:336-352` | 16 nodes (0..7)⁺,(8..15)⁻; `_itp_serendip_quadrangle_8`; segment order 3 → 2×2. p1 `_cohesive_3d_8`. | cohesive_element | — | R2 | — | — |
| ELEM-021 | `_bernoulli_beam_2` | `element_class_structural.hh:58-75`; `element_classes/element_class_bernoulli_beam_inline_impl.hh:30-55,95-120,155-170`; `element_class_hermite_inline_impl.hh` | 2D Euler–Bernoulli beam: geometry `_gt_segment_2`, 3 dofs/node (u, v, θ), 2 stress comps (ε, χ), dnds cols 6; `_git_segment` order 3 → 2 Gauss points. `N` (3×6): Lagrange for u, Hermite (M1,L1,M2,L2 with a=L/2) for v and their x-derivatives for θ. Voigt `B` (2×6): `[L'0,0,0,L'1,0,0; 0,−H'0,−H'1,0,−H'2,−H'3]`. Rotation `R=[[c,s,0],[−s,c,0],[0,0,1]]` from node 0→1 direction. | structural_mechanics | — | `test_fe_engine_precomputation_structural.cc` (TestBernoulliB2.PrecomputeRotations), `_bernoulli_beam_2.msh`; `test_interpolate_bernoulli_beam_2.cc` + `.verified` (not registered) | — | Gmsh: read through `MeshIOMSHStruct` (segments reinterpreted as beams, IO-008). |
| ELEM-022 | `_bernoulli_beam_3` | same files (`computeShapes` 6×12, `arrangeInVoigt` 4×12, `computeRotationMatrix` with `extra_normal`) | 3D beam: 6 dofs/node (u,v,w,θx,θy,θz), 4 stress comps (ε, χ strong, χ weak, torsion); order 3 → 2 points. Rotation: `Pe=diag(1,−1,1)·[x; x×n; n]⁻¹` (rows x, x×n, n), block-diag(Pe,Pe). Needs per-element mesh data `"extra_normal"`. | structural_mechanics | — | TestBernoulliB3.PrecomputeRotations, `_bernoulli_beam_3.msh` | — | `Pg(0)=x` assigns **columns** (plugin `operator()(c)`) so `Pg` has columns x, x×n, n (not rows) — reproduce exactly. |
| ELEM-023 | `_discrete_kirchhoff_triangle_18` | `element_class_structural.hh:76-80`; `element_classes/element_class_kirchhoff_shell_inline_impl.hh` | DKT shell: geometry `_gt_triangle_3`, 6 dofs/node, 6 stress comps (3 membrane + 3 bending), dnds cols 21, triangle order 2 → 3 points. Local basis `P` (a1=normalised x2−x1, e3=normalised a1×(x3−x1), e2=e3×a1); `computeDNDS` builds Batoz DKT derivative blocks (dNm, dNx1..3, dNy1..3). **`computeShapes` is empty** (N not filled) → mass/interpolation for DKT unusable. | structural_mechanics | — | `_discrete_kirchhoff_triangle_18.msh`; `TestDKT18.DISABLED_PrecomputeRotations` | — | Experimental; port derivative formulas verbatim; flag missing shape functions in BLOCKERS/DECISIONS. |
| ELEM-024 | `_igfem_segment_3` | `extra_packages/igfem/src/element_classes_igfem/element_class_igfem_segment_3_inline_impl.hh:70` (`AKANTU_DEFINE_IGFEM_ELEMENT_CLASS_PROPERTY(..., _segment_2 parent/sub-elements, _ek_igfem, 1, 1)`) | Enriched segment (3 nodes) made of 2 `_segment_2` sub-elements; orientation logic from inside/outside flags. | IGFEM (extra_packages, `DEPENDS CGAL`) | CGAL | `extra_packages/igfem/test/test_fe_engine/*` | — | Owned by R2 (extra packages) for physics; listed here because the enum entries live in core. Package is **bit-rotted** (uses removed `_ek_igfem`). |
| ELEM-025 | `_igfem_triangle_4` | `element_class_igfem_triangle_4_inline_impl.hh:71` (sub-elements `_triangle_3`,`_triangle_3`) | Enriched triangle (4 nodes) split into two triangles. | IGFEM | CGAL | `mesh_info_igfem_triangle_4.txt`, `test_integrate.verified` etc. | — | Same as ELEM-024. |
| ELEM-026 | `_igfem_triangle_5` | `element_class_igfem_triangle_5_inline_impl.hh:71` (sub-elements `_triangle_3`,`_quadrangle_4`) | Enriched triangle (5 nodes) split into a triangle and a quadrangle. | IGFEM | CGAL | `mesh_info_igfem_triangle_5.txt` | — | Same as ELEM-024. |
| ELEM-027 | Hermite interpolation `_itp_hermite_2` (helper, not an element type) | `element_class_hermite_inline_impl.hh:60-140` | Cubic Hermite on segment with half-length `a=L/2`: `M1=¼(2−3ξ+ξ³)`, `M2=¼(2+3ξ−ξ³)`, `L1=a/4(1−ξ−ξ²+ξ³)`, `L2=a/4(−1−ξ+ξ²+ξ³)`; rotation rows = x-derivatives `M1'=3/(4a)(ξ²−1)` etc.; curvature `B = [3ξ/2, a/2(3ξ−1), −3ξ/2, a/2(3ξ+1)]/a`. | structural_mechanics | — | via beam tests | — | Used by ELEM-021/022. |

### 1.4 QUAD — quadrature rules

Raw data: `src/fe_engine/gauss_integration.cc`; assembly into rules:
`src/fe_engine/gauss_integration_tmpl.hh`. Coordinates tables are read as
`Eigen::Map<Matrix<Real, dim, nb_points>>` (column per point, i.e. data listed
point by point). Exact values reproduced in §2.4.

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| QUAD-001 | Order → number-of-points map | `gauss_integration_tmpl.hh:31-80` (`GaussIntegrationNbPoints`) | point: 1. segment: `n_pts = (n+1)/2 + ((n+1)%2 ? 1 : 0)` (integer division) → orders 1:1, 2:2, 3:2, 4:3, 5:3, 6:4, 7:4. triangle: 1→1, 2→3, 3→4, 4→6, 5→7. tetrahedron: 1→1, 2→4, 3→5, 4→15, 5→15. pentahedron: (x_order, yz_order) = 1:(3,2), 2:(3,2), 3:(3,3), 4:(5,5), 5:(5,5). | core | — | `test_fe_engine_gauss_integration.cc` (orders 1..5) | — | Segment formula gives 2 points for order 2 (over-integrates) — keep exactly. Orders >5 for simplices/prism and segment >7 do not compile. |
| QUAD-002 | Point rule | `gauss_integration.cc:33-34` | position 0, weight 1. | core | — | — | — | — |
| QUAD-003 | Segment 1-point | `gauss_integration.cc:39-40` | ξ=0, w=2. | core | — | ArbitraryOrder | — | — |
| QUAD-004 | Segment 2-point | `gauss_integration.cc:42-43` | ξ=∓1/√3 (`-1./sqrt(3.)`, `1./sqrt(3.)`), w=1,1. | core | — | ArbitraryOrder | — | Literal expression `1./sqrt(3.)` (not `sqrt(3.)/3.`) — bitwise relevant. |
| QUAD-005 | Segment 3-point | `gauss_integration.cc:45-46` | ξ = −√(3/5), 0, √(3/5); w = 5/9, 8/9, 5/9. | core | — | ArbitraryOrder | — | — |
| QUAD-006 | Segment 4-point | `gauss_integration.cc:48-55` | ξ = ∓√((3+2√(6/5))/7), ∓√((3−2√(6/5))/7) ordered −outer, −inner, +inner, +outer; w = (18−√30)/36 (outer), (18+√30)/36 (inner). | core | — | ArbitraryOrder (mass of quad8/hex20) | — | — |
| QUAD-007 | Tensor-product rules for quadrangles/hexahedra (and cohesive quads) | `gauss_integration_tmpl.hh:110-170` (`GaussIntegrationTypeDataHelper<_git_segment, dim, n>`) | `n_seg^dim` points; coordinates/weights built so that **the first coordinate varies fastest**, then second, then third; weight = product of 1D weights. | core | — | `test_integrate.cc`, ArbitraryOrder, precomputation | — | Index formula `rq = q % tot + q / tot` reproduces lexicographic order (verified by hand for n=2,3). Doc comment in hex8 shows a different order — code governs. |
| QUAD-008 | Triangle 1-point | `gauss_integration.cc:59-60` | (1/3,1/3), w=½. | core | — | — | — | Weights sum to 1/2 (reference triangle area). |
| QUAD-009 | Triangle 3-point | `gauss_integration.cc:62-65` | (1/6,1/6), (2/3,1/6), (1/6,2/3); w=1/6 each. | core | — | — | — | Interior points (not mid-edge). |
| QUAD-010 | Triangle 4-point | `gauss_integration.cc:67-71` | (1/5,1/5), (3/5,1/5), (1/5,3/5), (1/3,1/3); w = 25/96 ×3, −27/96. | core | — | — | — | Negative weight. Written as `25./(24.*4.)`, `-27/(24.*4.)`. |
| QUAD-011 | Triangle 6-point (order 4) | `gauss_integration.cc:73-86` (`tri_6_a`, `tri_6_b`, `tri_6_w1`, `tri_6_w2`) | a=(8−√10+√(38−44√(2/5)))/18, b=(8−√10−√(38−44√(2/5)))/18; points (b,b),(1−2b,b),(b,1−2b),(a,1−2a),(a,a),(1−2a,a); w1=(620−√(213125−53320√10))/7440 (first 3), w2=(620+√(...))/7440 (last 3). | core | — | ArbitraryOrder; tri6 mass | — | Static initialisation of `tri_6_*` from `std::sqrt` expressions. |
| QUAD-012 | Triangle 7-point (order 5) | `gauss_integration.cc:88-101` | (1/3,1/3) w=9/80; a=(6+√15)/21: (a,a),(1−2a,a),(a,1−2a) w1=(155+√15)/2400; b=(6−√15)/21: (b,b),(1−2b,b),(b,1−2b) w2=(155−√15)/2400. | core | — | ArbitraryOrder | — | — |
| QUAD-013 | Tetrahedron 1-point | `gauss_integration.cc:109-110` | (¼,¼,¼), w=1/6. | core | — | — | — | Weights sum to 1/6. |
| QUAD-014 | Tetrahedron 4-point | `gauss_integration.cc:112-118` | a=(5−√5)/20, b=(5+3√5)/20; (a,a,a),(b,a,a),(a,b,a),(a,a,b); w=1/24. | core | — | — | — | — |
| QUAD-015 | Tetrahedron 5-point (order 3) | `gauss_integration.cc:120-127` | (¼,¼,¼) w=−2/15; (1/6,1/6,1/6),(1/6,1/6,½),(1/6,½,1/6),(½,1/6,1/6) w=3/40. | core | — | ArbitraryOrder | — | Negative weight. |
| QUAD-016 | Tetrahedron 15-point (orders 4,5) | `gauss_integration.cc:129-156` | Centroid w=8/405; a=(7+√15)/34,b=(13−3√15)/34 4 pts w1=(2665−14√15)/226800; c=(7−√15)/34,d=(13+3√15)/34 4 pts w2=(2665+14√15)/226800; e=(5−√15)/20,f=(5+√15)/20 6 pts w3=5/567 (exact point order in §2.4). | core | — | ArbitraryOrder; tet10 mass | — | — |
| QUAD-017 | Pentahedron rules (segment × triangle product) | `gauss_integration_tmpl.hh:172-230` (`GaussIntegrationTypeDataHelper<_git_pentahedron,...>`) | Points = for each segment point (outer loop, along x) × each triangle point (inner loop): `(ξ_seg, tri_x, tri_y)`, weight product. Orders 1–2: 2×3=6 points; 3: 2×4=8; 4–5: 3×7=21. | core | — | ArbitraryOrder; penta6/penta15 FE tests | — | — |
| QUAD-018 | Unused explicit pentahedron tables | `gauss_integration.cc:158-215` (`GaussIntegrationTypeData<_git_pentahedron, 6/8/21>`) | Explicit 6/8/21-point prism tables are **never referenced** (pentahedron helper uses segment×triangle). The 6-point table uses triangle points (½,½),(0,½),(½,0) and the 21-point weight table contains a copy-paste error (`w1_2*w1_2` in the middle block). | core | — | — | — | Do **not** port as active rules; mention in DECISIONS (dead data). |
| QUAD-019 | Default & mass orders per element type | `ElementClassProperty<type>::polynomial_degree` (ELEM rows); `fe_engine_template_tmpl_field.hh:349` (mass uses `2*polynomial_degree`) | See table §2.4.3 (points per type for default and mass rules). | core | — | precomputation & mass tests | — | — |

### 1.5 MESH — `src/mesh/`

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| MESH-001 | `Mesh` container (nodes + connectivities) | `src/mesh/mesh.hh:70-560`, `mesh.cc:48-90` (ctors), `mesh_inline_impl.hh` | Holds `nodes` (`Array<Real>`, `nb_component = spatial_dimension`, id `<id>:coordinates`), `connectivities` (`ElementTypeMapArray<Idx>`, one Array per (type, ghost) with `nb_nodes_per_element` comps), `nodes_flags` (`Array<NodeFlag>`, default `_normal`), optional `nodes_global_ids`, `nodes_prank` map, `ghosts_counters`. Ctors: `(dim, id="mesh")`, `(dim, communicator, id)`, `(dim, shared_ptr<nodes>, id)` (shares node array, e.g. mesh_facets). Non-copyable. Inherits `EventHandlerManager<MeshEventHandler>`, `GroupManager`, `MeshData`, `Dumpable`. | core | — | nearly all tests | ghost connectivities stored under `_ghost` | Rust: `Mesh` struct with typed per-(type,ghost) storage; shared node array between mesh and mesh_facets requires `Rc<RefCell<>>`/`Arc` or index-based design (ARCHITECTURE). |
| MESH-002 | Element/node counting & type iteration | `mesh.hh:520-560` (`getNbElement(type,ghost)`, `getNbElement(dim,ghost,kind)`), `mesh_inline_impl.hh:65-70` (`elementTypes(...)`) | `getNbElement(type)` returns 0 if type absent (exception swallowed). `elementTypes(dim, ghost, kind)` iterates **types in enum order** (std::map key order) filtered by `Mesh::getSpatialDimension(type)==dim` (unless `_all_dimensions`) and kind (unless `_ek_not_defined`). Named-arg form `elementTypes(_spatial_dimension=, _ghost_type=, _element_kind=)`. | core | — | all | per ghost type | Iteration order = enum order is observable (output ordering, dumps, assembly order). Default kind filter for `mesh.elementTypes(dim)` is `_ek_not_defined` = all kinds; for `ElementTypeMapArray::elementTypes(dim, ghost, kind)` same. |
| MESH-003 | Static element-type queries on `Mesh` | `mesh.hh:450-480`, `mesh_inline_impl.hh:28-40, 255-330` (`getNbNodesPerElement`, `getP1ElementType`, `getKind`, `getSpatialDimension(type)`, `getNaturalSpaceDimension`, `getNbFacetsPerElement(type[,t])`, `getFacetLocalConnectivity`, `getFacetConnectivity(element,t)`, `getNbFacetTypes`, `getFacetType`, `getAllFacetTypes`, `getNbNodesPerElementList`) | Thin wrappers over FE-003 tables; `getFacetConnectivity(element, t)` returns global node ids of all facets of type `t` (`nb_facets × nb_nodes_per_facet`). Unknown type → 0 / `_not_defined`. | core | — | `test_buildfacets`, `test_facet_element_mapping` | — | — |
| MESH-004 | Mesh read/write entry points | `mesh.cc:358-400` (`Mesh::read`, `Mesh::write`, `makeReady`) | `read(filename, MeshIOType=_miot_auto)` → `MeshIO::read` then warns if no element of mesh dimension was read, then `makeReady()` (nb_global_nodes = nb nodes, bbox, flags resized to `_normal`, empty node→element sets). Reading an already distributed mesh asserts. | core | — | `test_mesh_io_msh*`, all mesh-based tests | — | — |
| MESH-005 | Bounding boxes | `mesh.cc:415-430` (`computeBoundingBox`), `mesh.hh:280-300` (`getLowerBounds`, `getUpperBounds`, `getBBox`, `getLocalLowerBounds`, `getLocalUpperBounds`, `getLocalBBox`) | Local bbox over **all** nodes (incl. ghosts); global bbox = MPI `allSum` (min/max) when distributed. Recomputed on node add/remove, element removal, distribution. | core | — | `test_grid.cc`, FE fixtures (`lower`/`upper`) | global bbox via allreduce | — |
| MESH-006 | Barycenters | `mesh_inline_impl.hh:345-365` (`getBarycenter(element, out)`, `getBarycenter(element)`), `mesh.cc:154-162` (`getBarycenters(array, type, ghost)`) | Arithmetic mean of **all** element nodes (incl. mid-side nodes for quadratic elements) via `Math::barycenter`. | core | — | `test_grid`, `test_mesh_iterators`? | — | Not a centroid for curved/quadratic elements — reproduce mean of nodes. |
| MESH-007 | Node flags & ownership predicates | `mesh_inline_impl.hh:497-545` (`isPureGhostNode`, `isLocalOrMasterNode`, `isLocalNode`, `isMasterNode`, `isSlaveNode`, `isPeriodicSlave`, `isPeriodicMaster`, `getNodeFlag`, `getNodePrank` (-1 if unknown)) | Bit tests against `NodeFlag` masks (CORE-009): e.g. `isLocalOrMasterNode = (flag & 0xCC) == 0`. | core | — | `test_mesh_periodic` | central for DOF ownership (R3) | — |
| MESH-008 | Global node ids | `mesh_inline_impl.hh:147-157, 545-560` (`getNodesGlobalIdsPointer` creates identity map lazily, `getNodeGlobalId`, `getNodeLocalId` (linear `find`, -1 if absent), `getNbGlobalNodes`), `mesh.cc:437-455` (`getGlobalConnectivity`) | Without global ids array: identity. `getNodeLocalId` is O(n) search. | core | — | parallel tests (R3) | essential in parallel | O(n) lookup is a performance trap; port can use a hash map but must keep semantics. |
| MESH-009 | Connectivity access per element | `mesh_inline_impl.hh:55-58, 690-705` (`getConnectivity(element)`, `getConnectivityWithPeriodicity(element)` — slaves replaced by masters), `extractNodalValuesFromElement` (`mesh_inline_impl.hh:445-495`) | `extractNodalValuesFromElement(array, element)` returns `(nb_comp × nnodes)` matrix (column per node). | core | — | many | — | — |
| MESH-010 | Mesh-facets sub-mesh | `mesh.cc:233-355` (`initMeshFacets(id="mesh_facets")`, `defineMeshParent`), `mesh_inline_impl.hh:578-608` (`getMeshFacets`, `hasMeshFacets`, `getMeshParent`, `isMeshFacets`) | Creates (once) a `Mesh` sharing the node array, flags and global ids, id `<mesh_id>:mesh_facets`, and calls `MeshUtils::buildAllFacets(mesh, mesh_facets, 0)` (all facet dimensions down to points). If distributed: builds `FacetSynchronizer`, exchanges global facet connectivities (`_smmc_facets_conn`) and `flipFacets` for ghost facets. If parent has `"physical_names"`: for each (dim−1) element of the parent, finds the facet with the same barycenter (tolerance `Math::tolerance·max(1, norm(bary))`, linear search) → copies the physical name to `mesh_facets` data `"physical_names"`, records parent data `"mesh_to_mesh_facet"` (Element), then `createGroupsFromMeshData<std::string>("physical_names")` on mesh_facets. | core | — | `test_facet_element_mapping.cc`, `test_buildfacets`, cohesive tests | facet synchroniser (R3) | O(n²) barycenter matching; tie-breaking = first facet within tolerance (lowest index). Port should keep result identical; an acceleration structure is fine if first-match semantics preserved. |
| MESH-011 | Element↔sub-element adjacency data | `mesh_inline_impl.hh:255-345` (`getElementToSubelement(...)` = mesh data `"element_to_subelement"` of `std::vector<Element>`, `getSubelementToElement(...)` = mesh data `"subelement_to_element"` of `Element` with `nb_facets` comps, default `ElementNull`) | For a facet: list of adjacent higher-dim elements (2 for interior, `[el, ElementNull]` for boundary when built by `buildFacets*`); for an element: its facets in **creation order** (first free slot, see MUTIL-004). | core | — | `test_buildfacets` (+ `.verified`), `test_facet_element_mapping` | ghost facets | Stored as generic mesh data → appear in data listings. |
| MESH-012 | Node→element association | `mesh.cc:590-645` (`fillNodesToElements(dim)`, `getAssociatedElements(node_list / node)`), `mesh_inline_impl.hh:705-710` | `nodes_to_elements[n]` = `std::set<Element>` (sorted by Element `<`) over all ghost types/types of given dim (default all). Refilled on `NewElementsEvent` and `RemovedElementsEvent`. | core | — | contact tests (R3) | — | — |
| MESH-013 | Element removal & renumbering | `mesh.cc:661-730` (`eraseElements`), `mesh_inline_impl.hh:81-90` (`sendEvent<RemovedElementsEvent>`), `element_type_map_tmpl.hh` `onElementsRemoved` | Ghost elements: decrement `ghosts_counters`, removed only when counter hits 0. New numbering: init identity, removed = −1, then **holes filled by moving the last surviving elements** (last-not-deleted → first-deleted, iteratively). All `ElementTypeMapArray`s apply `onElementsRemoved(new_numbering)` (tuple moved to `new_i`, array truncated). Event sent to handlers after connectivity update, node→element refill and bbox recompute. | core | — | cohesive/contact tests (R2/R3) | ghost counters | The hole-filling permutation must be identical (it changes element ids seen by users and dumps). |
| MESH-014 | Node removal | `mesh_inline_impl.hh:91-145` (`sendEvent<RemovedNodesEvent>`, `removeNodesFromArray`) | `RemovedNodesEvent` carries `new_numbering` (size nb_nodes, −1 = removed); node arrays (coords, global ids & flags unless mesh_facets) compacted by `new_i`; node→element sets moved; bbox recomputed. | core | — | `test_purify_mesh.cc` | — | — |
| MESH-015 | Mesh events & handlers | `mesh_events.hh` (`MeshEvent<Entity>`, `NewNodesEvent`, `RemovedNodesEvent`, `NewElementsEvent`, `MeshIsDistributedEvent`, `RemovedElementsEvent`, `ChangedElementsEvent`, `MeshEventHandler::on*`), `mesh.cc:108-148` (`sendEvent<NewNodesEvent>`: bbox, flags resize, cohesive connectivity refresh from `mesh_to_mesh_facet`, `GroupManager::onNodesAdded`) | Each event has a `list`, an `origin` string; handlers notified in priority order (CORE-036). `NewElementsEvent` → `fillNodesToElements()` first. | core | — | indirectly | `MeshIsDistributedEvent` | Rust: observer list or explicit callbacks; keep ordering. |
| MESH-016 | Mesh accessor (privileged mutation) | `mesh_accessor.hh` (`MeshAccessor`: `getNodes`, `getConnectivity`, `resizeConnectivity`, `resizeNodes`, `getNodesGlobalIds`, `getNodesFlags`, `setNodePrank`, `setNbGlobalNodes`, `setDistributed`, `getGhostsCounters`, `getElementToSubelement`, `getSubelementToElement`, `getData`, `updateGlobalData`, `registerGlobalDataUpdater`, `makeReady`, `addPeriodicSlave`, `markMeshPeriodic`, `wipePeriodicInfo`, synchronizer getters) | Friend wrapper used by readers, MeshUtils, inserters and Python to mutate mesh internals. | core | — | used by readers & `test_mesh_iterators` | — | Rust: `pub(crate)` builder API. |
| MESH-017 | `ElementTypeMap<Stored, SupportType>` | `element_type_map.hh:60-215`, `element_type_map_tmpl.hh:28-110, 290-370` | Two `std::map<SupportType, Stored>` (not_ghost, ghost); `exists`, `operator()(type, ghost)` (throws if absent for const), insert-new `operator()(value, type, ghost)` (throws if exists), `elementTypes` filtered iteration (type_iterator skips by dim/kind), `printType` → `"(ghost_type:type)"`. | core | — | indirectly all | — | Key ordering = enum order (InterpolationType order for shape storage). |
| MESH-018 | `ElementTypeMapArray<T>` | `element_type_map.hh:217-420`, `element_type_map_tmpl.hh:110-290` (`alloc` (resizes with warning if exists), `alloc` both ghosts, `operator()(type,ghost)`, `operator()(Element, comp)`, `operator()(IntegrationPoint, comp)` (row `IntegrationPoint::global_num` = `element*nq + num_point`, `element_type_map_tmpl.hh:766-815`), `get(element)` (tuple view), `get(element, ns...)` (matrix view), `setArray`, `free`, `clear`, `empty`, `zero`, `set`, `onElementsRemoved`, `copy`, `getNbComponents`, `size(...)`, `isNodal`) | Array ids: `<parent_id>:<id>:<type>[:ghost]`. Per-element data accessed by element index; per-quad-point data by `IntegrationPoint::global_num`. | core | — | indirectly all | ghost arrays separate | `IntegrationPoint` access uses `global_num` row (must be consistent with FE quad layout). |
| MESH-019 | `ElementTypeMapArray::initialize` (mesh / FE-engine based allocation) | `element_type_map_tmpl.hh:430-745` (`MeshElementTypeMapArrayInitializer`, `FEEngineElementTypeMapArrayInitializer`, named args `_nb_component`, `_spatial_dimension`, `_ghost_type`, `_all_ghost_types`, `_element_kind`, `_with_nb_element`, `_with_nb_nodes_per_element`, `_default_value`, `_do_not_default`, `_element_filter`), `element_type_map.cc` | Mesh initializer: types of `mesh.elementTypes(dim, ghost, kind)` (or filter's types); **default `_spatial_dimension` = mesh spatial dimension** (not all dims); size = nb elements if `_with_nb_element` else 0; comps × nnodes if `_with_nb_nodes_per_element` (marks array nodal). FE initializer: element dimension of the FE engine, size = nb elements × nb integration points. Default: both ghost types. Existing arrays are resized (default value fills new entries). | core | — | indirectly | allocates ghost arrays too | Subtle defaults (dimension, ghost types) must be replicated or callers will see different array sets. |
| MESH-020 | `ElementTypeMapArrayFilter` | `element_type_map_filter.hh` | Read-only view of an `ElementTypeMapArray` restricted to element lists of a group (`ElementTypeMapArray<Idx>` filter), with `nb_data_per_elem`; used for group dumps. | core | — | `test_dumper` (group dumps) | — | — |
| MESH-021 | Mesh data (tagged per-element / per-node data) | `mesh_data.hh` (`MeshData`, `MeshDataTypeCode` {`_int, _uint, _real, _bool, _element, _std_string, _std_vector_element, _unknown`}, `MeshDataType` {`_nodal`, `_elemental`}), `mesh_data_tmpl.hh`, `mesh_data.cc`; `mesh_inline_impl.hh:127-180` (`getData`, `getDataPointer(name, type, ghost, nb_comp, size_to_nb_element, resize_with_parent[, default])`, `hasData`) | Named datasets: elemental (`ElementTypeMapArray<T>`) or nodal (`Array<T>`); type registered on first allocation and checked on access (type mismatch → exception). `getTagNames(type, ghost)`, `getNbComponent`. Standard names used by core: `"tag_0".."tag_N"` (Gmsh tags), `"physical_names"` (std::string), `"element_to_subelement"`, `"subelement_to_element"`, `"mesh_to_mesh_facet"`, `"extra_normal"` (structural), Diana `"material"`? (IO rows). | core | — | `test/test_fe_engine/test_mesh_data.cc` (typed: string "5"/"10", UInt 5/10) | data distributed with mesh (R3: `SlaveNodeInfoPerProc` friend) | Type codes are exchanged in MPI distribution → keep enum order. |
| MESH-022 | Element groups | `element_group.hh`, `element_group.cc`, `element_group_inline_impl.hh` (`ElementGroup`: `add(element, add_nodes, check_for_duplicate)`, `addNode`, `removeNode`, `append`, `optimize`, `fillFromNodeGroup`, `clear`, `empty`, `getElements(type,ghost)`, `getElementsIterable`, `getNodeGroup`, `getDimension`, `addDimension`, `onNodesAdded`, `elementTypes`) | Per (type, ghost) `Array<Idx>` of element ids + associated `NodeGroup`. `dimension` = max spatial dim of added types. `optimize()` sorts+uniques element lists and node group. Each group registers a ParaView dumper `"paraview_<name>"` dumping the filtered mesh. `fillFromNodeGroup` adds elements whose nodes are **all** in the node group (dimension-filtered). On `CohesiveNewNodesEvent` the node group is rebuilt from connectivities. | core | — | `test_mesh_io_msh_physical_names`, `test_mesh_boundary`, `test_facet_element_mapping` | group names synchronised (MESH-024) | Automatic dumper registration per group is part of the API (Python `getElementGroup(..).dump`?). |
| MESH-023 | Node groups | `node_group.hh`, `node_group.cc`, `node_group_inline_impl.hh` (`NodeGroup`: `add(node, check_for_duplicate)`, `remove` (Array swap-remove), `optimize` (sort+unique), `append` (+optimize), `applyNodeFilter`, `find`, `size`, `clear`, `getNodes`, `getName`) | `Array<Idx>` of node ids; registers ParaView dumper `"paraview_<name>"` with field `"positions"`. | core | — | same as above; `test_mesh_periodic` | — | `remove`/`applyNodeFilter` use swap-remove → order changes (unless optimized afterwards). |
| MESH-024 | Group manager | `group_manager.hh`, `group_manager.cc`, `group_manager_inline_impl.hh` (`createNodeGroup(name, replace)`, `createElementGroup(name, dim, replace)` (also creates node group `<name>_nodes`), `createElementGroup(name, dim, node_group)`, `createFilteredNodeGroup`, `createFilteredElementGroup`, `destroyNodeGroup`, `destroyElementGroup(name, destroy_node_group)`, `renameElementGroup/NodeGroup`, `copyElementGroup/NodeGroup`, `getElementGroup/getNodeGroup` (throw if missing), `elementGroupExists`, `nodeGroupExists`, `iterateElementGroups/NodeGroups`, `getNbElementGroups(dim)`, `createElementGroupFromNodeGroup`, `synchronizeGroupNames`, `onNodesAdded`) | Groups kept in `std::map<std::string, unique_ptr<...>>` → **name-sorted** iteration. Duplicate creation throws unless `replace_group`. Group ids `<manager_id>:<name>_element_group` / `_node_group`. In parallel, `synchronizeGroupNames` gathers names on rank 0, broadcasts union so all ranks have all groups. | core | — | `test_mesh_io_msh_physical_names`, `test_mesh_boundary` | name synchronisation (rank 0 gather + broadcast) | — |
| MESH-025 | Groups from mesh data | `group_manager.cc:597-690` (`createGroupsFromMeshData<std::string>` / `<UInt>`) | One element group per distinct value of the dataset (value converted to string with `operator<<`), dimension = max spatial dim of tagged types; elements added in (ghost, type, element) order with their nodes; then `optimize()`. Used for `"physical_names"` and Gmsh tags. | core | — | `test_mesh_io_msh_physical_names` (+`.verified`), `test_facet_element_mapping` | synchronises names | `int`-tag groups would be named `"1"`, `"2"`... |
| MESH-026 | Clustering / boundary groups from geometry | `group_manager.cc:221-597` (`createBoundaryGroupFromGeometry` → `createClusters(dim-1, "boundary")`, `createClusters(element_dimension, prefix, filter[, mesh_facets])`, `ClusteringFilter`, `ClusterSynchronizer`) | BFS over facet adjacency (temporary mesh_facets built with `buildAllFacets(mesh, mf, dim, dim-1)`) creating groups `<prefix>_<i>` in discovery order (ghost type, type, element order); point elements are clusters by themselves. Parallel: temporary `tmp_<prefix>_<i>` groups merged into globally numbered clusters via pair exchange (`_gm_clusters`), exclusive scan, allGatherV. Returns number of clusters. "doesn't work in 1D" (comment). | core | — | `test/test_fe_engine/test_mesh_boundary.cc` (`cube.geo` → `test_boundary_msh`, `cube_physical_names.geo`) | cluster renumbering across ranks | Discovery order defines group names `boundary_0..`; must be reproduced. |
| MESH-027 | Group dumping field factories | `group_manager.hh:200-290`, `group_manager_inline_impl.hh` (`createElementalField`, `createElementalFilteredField`, `createNodalField`, `createStridedNodalField`), `mesh.cc:470-527` (`createFieldFromAttachedData<Real/Int>`, `getNbDataPerElem`, `getGroupDumper`) | Builds dumper fields for whole mesh (`"all"`) or a group (filtered by element/node lists); group dimension must match requested dimension (else `throw;` — **rethrow with no active exception → std::terminate**). | core + dumpers | IOHelper | `test_dumper.cc` | — | Replace bare `throw;` by proper errors. |
| MESH-028 | Periodic mesh (node pairing) | `mesh_periodic.cc:43-420` (`makePeriodic(direction)`, `makePeriodic(direction, group1, group2)`, internal `makePeriodic(direction, list_left, list_right)`), `mesh_inline_impl.hh:611-700` (`addPeriodicSlave`, `getPeriodicMaster`, `PeriodicSlaves`, `getPeriodicSlaves`) | Auto mode: nodes with `abs(x_d − lower_d)/L < 1e-10` (left) and `abs(x_d − upper_d)/L < 1e-10` (right). Positions projected (direction coord set to 0); in parallel, nodes in bbox intersections exchanged and added as pure-ghost periodic nodes (`NewNodesEvent`). Lists sorted (with Eigen fuzzy `<`), then **O(N²) matching**: for each left node first right node with `dist/L < 1e-10`. Master/slave resolution merges chains (`updating_master`); flags set `_periodic_master/_slave`; maps `periodic_slave_master` (unordered_map) / `periodic_master_slave` (unordered_multimap). Then `updatePeriodicSynchronizer()` and `is_periodic=true`. | core | — | `test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic.cc` (+ `test_periodic_mass.cc`, `test_periodic_static.cc`, fixtures, `cube.geo`, `square.geo`, `line.geo`, `material.dat`) | periodic node synchroniser, ghost periodic nodes (R3) | Sort comparator is not a strict weak ordering (fuzzy) — UB in std::sort; port should use a well-defined order and verify identical pairing. Multiple calls (x then y then z) chain masters (corner nodes). |
| MESH-029 | Periodic data consistency | `mesh_periodic.cc:420-502` (`makeConsistentForPeriodicity<Real/bool>(array, ReduceOperation)`, `wipePeriodicInfo`, `updatePeriodicSynchronizer`) | `_id`: copy master → slaves; `_sum`/`_or`: zero periodic slave & periodic pure-ghost values, then reduce slaves into masters and broadcast back. Node synchroniser scheme filtered to exclude periodic slaves. | core | — | `test_periodic_mass`, `test_periodic_static` | PBC synchroniser (R3) | — |
| MESH-030 | Mesh distribution entry points (list only) | `mesh.cc:528-590` (`distribute(_communicator=, _edge_weight_function=, _vertex_weight_function=)` → `distributeImpl` (Scotch on rank 0), `distribute(partition, communicator)`), `mesh.hh:110-130` | Creates Element/Node synchronisers; rank 0 partitions (Scotch) and runs `MeshUtilsDistribution::distributeMeshCentralized`; sets `is_distributed`, recomputes global bbox, sends `MeshIsDistributedEvent`. Without Scotch and >1 rank: error "Cannot distribute a mesh without a partitioning tool". | core (+Scotch, MPI) | Scotch | parallel tests (R3) | **owned by R3** for semantics | — |
| MESH-031 | Mesh iteration helpers | `mesh_iterators.hh` (`MeshElementsByTypes` (groups an `Array<Element>` into contiguous (type,ghost) ranges), `make_mesh_adaptor`-like transforms, `for_each_element(nb, filter, f)`, `for_each_element(mesh, f, _spatial_dimension=, _ghost_type=, _element_kind=, _element_filter=)`) | `for_each_element(mesh, ...)`: default dimension = mesh dimension, default both ghost types (not_ghost first), kind default `_ek_not_defined`. | core | — | `test/test_mesh_utils/test_mesh_iterators.cc` (`iterators_mesh.geo`) | — | Defaults matter (which elements get visited). |
| MESH-032 | Mesh filter functors | `mesh_filter.hh` (`FilterFunctor`, `NodeFilterFunctor`, `ElementFilterFunctor`) | Base functor tags for `createFiltered*Group`; base `operator()` throws not-implemented. | core | — | — | — | — |
| MESH-033 | Global data updater hook | `mesh_global_data_updater.hh` (`MeshGlobalDataUpdater::updateData(nodes_event, elements_event)`), `mesh.cc:645-660` (`updateGlobalData`, `registerGlobalDataUpdater`) | Strategy object (registered by models, e.g. cohesive) that renumbers global ids and sends events; default returns `(nb new nodes, nb new elements)` **without sending events**. | core | — | cohesive tests | global ids (R3) | Cohesive insertion relies on the model's updater to dispatch events (R2). |
| MESH-034 | Mesh printing | `mesh.cc:395-414` (`printself`) | Prints id, dimension, nodes array, connectivities, groups. | core | — | — | — | Debug only. |
| MESH-035 | Spatial grid ↔ mesh | `aka_grid_dynamic.hh:430-519` (`SpatialGrid::saveAsMesh`) | see CORE-030. | core | — | `test_grid` | — | Listed for traceability. |

### 1.6 MUTIL — `src/mesh_utils/`

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| MUTIL-001 | Node→element CSR | `mesh_utils.cc:37-75` (`MeshUtils::buildNode2Elements(mesh, CSR<Element>, dim)`) | Two passes over `for_each_element(mesh, _spatial_dimension=dim (default mesh dim), _element_kind=_ek_not_defined)`; row entries in visiting order (not_ghost then ghost; types in enum order; elements ascending). | core | — | indirectly (`test_buildfacets`) | includes ghost elements | Row order feeds facet construction ordering. |
| MUTIL-002 | Node→element CSR for one type | `mesh_utils.cc:76-110` (`buildNode2ElementsElementTypeMap`) | Same for a single (type, ghost), storing element indices. | core | — | — | — | — |
| MUTIL-003 | Boundary facet generation in-place | `mesh_utils.cc:112-125` (`buildFacets(mesh)`) | Clears existing (dim−1) connectivities (no event!) and builds only **boundary** facets (`boundary_only=true`) into the mesh itself. | core | — | `test_buildfacets`? (uses buildAllFacets), `test_mesh_boundary` | — | — |
| MUTIL-004 | Facet construction (all dimensions) | `mesh_utils.cc:126-420` (`buildAllFacets(mesh, mesh_facets, from_dim, to_dim)`, `buildAllFacets(mesh, mesh_facets, to_dim=0)`, `buildFacetsDimension(mesh, mesh_facets, boundary_only, dimension)`) | For every element (ghost order, type order, index order) and every facet type `ft` and local facet (FE-003 ordering): finds elements sharing all facet nodes (via CSR row of first facet node); **the facet is created only by the minimum connected element** (Element `<`), with that element's local node order. Skips facets whose nodes are all pure-ghost. `element_to_subelement` of the facet = connected elements in CSR row order, plus `ElementNull` if boundary. Interface facets between a local and a ghost element: facet is `_not_ghost` iff `prank(ghost elem) > prank(local elem)`, else `_ghost` (`mesh_utils.cc:340-365`; note the source comment states the opposite — code governs). Each connected element's `subelement_to_element` gets the facet in its **first free slot** (so slot order = facet creation order, not local facet index). Sends one `NewElementsEvent` on mesh_facets. `buildAllFacets` repeats for dims `from−1 … to+1` on mesh_facets itself. | core | — | `test/test_mesh_utils/test_buildfacets/test_buildfacets.cc` + `test_buildfacets_gtest.verified` + meshes `_triangle_3, _triangle_6, _quadrangle_4, _quadrangle_8, _quadrangle_4_triangle_3, _quadrangle_8_triangle_6, _tetrahedron_10, _hexahedron_8, _hexahedron_20, _pentahedron_6, _pentahedron_15, _hexahedron_8_pentahedron_6, _hexahedron_20_pentahedron_15 .msh` | ghost/interface facet ownership via ranks | Highest-risk ordering algorithm in the core: facet ids, facet node order (normals orientation) and adjacency slot order must match the oracle exactly; `.verified` file gives a golden output. |
| MUTIL-005 | Facet flipping for consistency across ranks | `mesh_utils.cc:537-652` (`flipFacets(mesh_facets, remote_global_connectivities, gt_facet)`) | For ghost facets whose local global connectivity differs from the remote one: reorder local connectivity to match remote order; in 3D skip if only a rotation of the P1 nodes; otherwise swap `element_to_subelement[0/1]` (if second not null) and swap first two `subelement_to_element` entries. | core | — | parallel cohesive tests (R3/R2) | MPI | — |
| MUTIL-006 | Element→sub-element data from existing facets | `mesh_utils.cc:654-787` (`fillElementToSubElementsData(mesh)`) | Given a mesh that already contains elements of dimension d−1 (read from file), computes `element_to_subelement` / `subelement_to_element` for all dimensions using node-sharing counts on P1 nodes; connected elements ordered by `std::map<Element,Int>` order; **no `ElementNull` appended for boundary facets** (differs from MUTIL-004). Warns and returns if no (d−1) elements. | core | — | `test_facet_element_mapping.cc` | — | Difference with buildFacets semantics must be preserved. |
| MUTIL-007 | Node renumbering of local connectivities | `mesh_utils.cc:423-496` (`renumberMeshNodes`, `renumberNodesInConnectivity`) | Renumbers nodes in order of first appearance (map old→new), splits local/ghost connectivities; used by distribution. | core | — | parallel (R3) | used in mesh distribution | — |
| MUTIL-008 | Purify mesh (remove unused nodes) | `mesh_utils.cc:497-536` (`purifyMesh`) | New node numbering = order of first appearance over all connectivities (ghost types, all dims, all kinds); unused nodes removed via `RemovedNodesEvent`. | core | — | `test/test_mesh_utils/test_purify_mesh.cc` + `purify_mesh.msh` | — | Node order changes even if no node is unused (renumbered by first appearance)! |
| MUTIL-009 | Legacy PBC map computation | `mesh_utils_pbc.cc` (`computePBCMap(mesh, dir, map)`, `computePBCMap(mesh, (surf1, surf2), map)`, `matchPBCPairs`, `CoordinatesComparison`) | Left/right nodes by `are_float_equal` with bbox; sort by transverse coords (comparator `<=`, tolerance 1e-7 normalised), merge-walk matching with `dx²+dy²` / L < 1e-7. Surface-pair variant detects the flat direction. | core | — | none (no caller in src/test/python/examples) | — | **Dead code** (superseded by MESH-028). Candidate "not ported" decision. |
| MUTIL-010 | Centralised mesh distribution (list only) | `mesh_utils_distribution.{hh,cc}` (`MeshUtilsDistribution::distributeMeshCentralized(mesh, root, partition)`, `distributeMeshCentralized(mesh, root)`) | Rank 0 sends element/node info per partition (via `ElementInfoPerProc`/`NodeInfoPerProc`, R3). | core (+MPI) | MPI | parallel tests | **owned by R3** | — |
| MUTIL-011 | Global ids updater | `global_ids_updater.{hh,cc}`, `global_ids_updater_inline_impl.hh` (`GlobalIdsUpdater::updateGlobalIDs`, `updateGlobalIDsLocally`, `synchronizeGlobalIDs`, pack/unpack `_giu_global_conn`) | New local/master nodes get consecutive global ids after an exclusive scan; slaves receive ids from masters (slave reduction then synchronisation). Serial: returns count unchanged. | core | MPI | cohesive parallel tests | **owned by R3** for semantics | — |
| MUTIL-012 | Mesh partition base & dual graph | `mesh_partition.{hh,cc}` (`MeshPartition::partitionate`, `reorder`, `buildDualGraph` (port of METIS `MeshToDual`: neighbours sharing ≥ min facet node count; cohesive–cohesive adjacency excluded), `fillPartitionInformation` (partitions + ghost partition CSR), `tweakConnectivity`/`restoreConnectivity` (periodic: slaves→masters), `linearized`/`unlinearized`, `getPartitions`, `getGhostPartitionCSR`) | Element linearisation by type offsets (mesh dim, not_ghost). Ghost partitions: elements touching nodes of other partitions. | core | — | `test/test_mesh_utils/test_mesh_partitionate/test_mesh_partitionate_mesh_data.cc` (`quad.geo`, `triangle.geo`) | partitions consumed by distribution (R3) | Elements with no neighbour are absent from the dual graph map (size = map size) — edge case. |
| MUTIL-013 | Partition from mesh data | `mesh_partition/mesh_partition_mesh_data.{hh,cc}` (`MeshPartitionMeshData(mesh, mapping, dim)`, `partitionate`, `setPartitionMapping`; `setPartitionMappingFromMeshData` & `reorder` not implemented) | Partition id per element read from an `ElementTypeMapArray<Idx>`; asserts the number of distinct ids == nb_part (debug). | core | — | `test_mesh_partitionate_mesh_data.cc` | — | — |
| MUTIL-014 | Scotch graph partitioning & node reordering | `mesh_partition/mesh_partition_scotch.{hh,cc}` (`MeshPartitionScotch::partitionate` (`SCOTCH_graphPart` default strategy, seeded with `RandomGenerator` seed), `reorder` (`SCOTCH_meshGraph` + graph ordering → node permutation), `createMesh`) | Debug dumps of Scotch graphs/meshes at `dblDump`. | Scotch (header in core) | Scotch (≥5, PT-Scotch optional, `SCOTCH_Num` size == `Int`) | `test/test_mesh_utils/test_mesh_partitionate/test_mesh_partitionate_scotch.cc` | used for parallel distribution | Partition result depends on Scotch version/strategy → bit-for-bit parallel reproducibility vs oracle requires same Scotch build or exporting oracle partitions (MUTIL-013 path). |
| MUTIL-015 | Cohesive element inserter (serial semantics) | `cohesive_element_inserter.{hh,cc}`, `cohesive_element_inserter_inline_impl.hh` (`CohesiveElementInserter(mesh, id)`, params `cohesive_surfaces`, `cohesive_zones`, `bounding_box` (parser section `cohesive_inserter`), `setLimit(axis, a, b)`, `insertIntrinsicElements`, `insertElements(only_double_facets)`, `limitCheckFacets`, `addPhysicalSurface`, `addPhysicalVolume`, `updateInsertionFacets`, `getInsertionFacets`, `getCheckFacets`, `CohesiveNewNodesEvent` (new + old node lists)) | Builds `mesh_facets` (MESH-010). Check facets: true except boundary facets (`elem[1]==ElementNull`), facets between two ghosts, facets touching cohesive elements, facets whose barycenter is outside `insertion_limits ± Math::tolerance` (open interval), facets not between two elements of `cohesive_zones` (if set), facets whose `physical_names` not in `cohesive_surfaces` (if set). Intrinsic: insertion_facets = check facets then insert. Extrinsic: after insertion, inserted facets removed from check list; arrays resized for new facets. Events dispatched through the global data updater (MESH-033). | cohesive_element | — | cohesive tests in `test/test_model/test_solid_mechanics_model/test_solid_mechanics_model_cohesive/*` (R2) | `_ce_groups` sync of insertion flags | Parsing of `bounding_box` as a matrix (`dim × 2`). |
| MUTIL-016 | Cohesive insertion algorithm (facet/node doubling) | `cohesive_element_inserter_helper.{hh,cc}` (`CohesiveElementInserterHelper`: ctor (facets to double, global insertion order in parallel), `insertCohesiveElement`, `insertFacetsOnly`, `doubleFacets<dim>`, `findSubfacetToDouble<dim>`, `doubleSubfacet<dim>`, `doublePointFacet`, `doubleNodes`, `findElementsAroundSubfacet`, `updateElementalConnectivity`, `updateElementToSubelement`, `updateSubelementToElement`, `updateQuadraticSegments`, `updateCohesiveData`) | Facets to double collected in (ghost, type, index) order (parallel: sorted by a global ordering); boundary/cohesive-adjacent facets skipped with warning. New facet appended (id = current count), element on side 1 re-pointed to the new facet; both facets become boundary facets. 2D: double facets → find sub-facets (nodes) to double → double nodes; 3D: facets, edges, nodes. Doubled nodes appended at end of node array (copy of coordinates), recorded as `(old,new)` pairs. Cohesive connectivity = `[facet_a nodes, facet_b nodes]` with `(a,b) = (old,new)` in 2D and **(new,old) in 3D**; `element_to_subelement(facet)[1] = cohesive element`; mesh_facets `subelement_to_element(cohesive_type)` = the two facets. | cohesive_element | — | cohesive tests (R2); `test_segment_nodetype`? | parallel ordering via `_ce_insertion_order`; global ids via MUTIL-011 | Exact ordering of new nodes/facets/elements is observable (ids in dumps, DOF numbering). |
| MUTIL-017 | Boost graph converter (debug) | `boost_graph_converter.{hh,cc}` (`BoostGraphConverter(mesh)`, `toGraphviz(filename)`) | Builds a Boost adjacency graph of elements, facets, sub-facets and cohesive elements (edges up/down) and writes Graphviz `.dot`. | debug_tools (`BOOST_COMPONENTS graph`) | Boost.Graph | none | — | Debug-only; optional to port (could emit DOT directly). |
| MUTIL-018 | Segment node-type test support | — (uses `Mesh` node flags after distribution) | Checks node types on a distributed segment mesh. | core + MPI | — | `test/test_mesh_utils/test_segment_nodetype/test_segment_nodetype.cc` + `mesh.geo/.msh`, `test_segment_nodetype.sh` | parallel (R3) | Listed for test mapping only. |

### 1.7 IO — mesh input/output (`src/io/mesh_io*`)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| IO-001 | Mesh reader/writer dispatch | `src/io/mesh_io.{hh,cc}` (`MeshIO::read/write(filename, mesh, MeshIOType)`, `getMeshIO`) | `_miot_auto`: extension `msh` → Gmsh, `diana` → Diana, else exception "Cannot guess the type of file". `_miot_gmsh_struct` only if structural_mechanics; `_miot_abaqus` → `nullptr` → **null dereference** (no Abaqus reader exists). | core | — | `test_mesh_io_msh*` | read before distribution | Return an error for unsupported types. |
| IO-002 | Physical-name construction (Gmsh v2 path) | `mesh_io.cc:89-112` (`constructPhysicalNames(tag_name, mesh)`) | For every not-ghost type (all dims): `physical_names[e]` = name from `$PhysicalNames` keyed by `(spatial dim of type, tag)`; if missing → decimal string of the tag. Only when the file had physical names. | core | — | `test_mesh_io_msh_physical_names` (+ `.verified`) | — | — |
| IO-003 | Gmsh `.msh` element-type mapping | `mesh_io_msh.hh:45-75` (`MSHElementType`), `mesh_io_msh.cc:145-215` (`_msh_nodes_per_elem`, `_msh_to_akantu_element_types`, `_akantu_to_msh_element_types`) | Read: 1→`_segment_2`, 2→`_triangle_3`, 3→`_quadrangle_4`, 4→`_tetrahedron_4`, 5→`_hexahedron_8`, 6→`_pentahedron_6`, 8→`_segment_3`, 9→`_triangle_6`, 11→`_tetrahedron_10`, 15→`_point_1`, 16→`_quadrangle_8`, 17→`_hexahedron_20`, 18→`_pentahedron_15`; 7 (pyramid), 10 (quad9), 12 (hex27), 13 (prism18), 14 (pyramid14) and others → skipped with warning. Write: inverse map, plus structural `_bernoulli_beam_2/3`→1, `_discrete_kirchhoff_triangle_18`→2. | core | — | all `.msh` fixtures | — | `_msh_pyramid_1` nodes-per-element table says 1 (wrong, unused). |
| IO-004 | Gmsh node-ordering conversion | `mesh_io_msh.cc:216-264` (`_read_order`) | `local_connect(read_order[j]) = node_j`, i.e. Gmsh node `j` goes to Akantu slot `read_order[j]`. Identity except: tet10 `8↔9`; penta6 `[2,0,1,5,3,4]`; penta15 `[2,0,1,5,3,4,8,7,11,6,9,10,14,13,12]`; hex20 `9→11, 10→12, 11→9, 12→13, 13→10, 17→19, 18→17, 19→18` (others identity). Same table used for writing? **No** — writer outputs Akantu order directly (no inverse reorder). | core | — | FE typed tests (`_tetrahedron_10.msh`, `_pentahedron_*.msh`, `_hexahedron_20.msh`), `test_buildfacets` | — | Write path does not apply the inverse permutation → round-trip of tet10/penta/hex20 through `Mesh::write` changes element node order w.r.t. Gmsh convention (bug to document; reproduce or fix with DECISION). |
| IO-005 | Gmsh MSH 2.x ASCII reader | `mesh_io_msh.cc:266-473` (`File`, `populateReaders2`: `$Nodes`, `$Elements`, `$Periodic`; common: `$MeshFormat`, `$PhysicalNames`, `$NodeData`, `$ElementData`, `$ElementNodeData`, unknown blocks skipped) | Version from `$MeshFormat`: `major*1000 + round(10*(v-major))`; binary (`file-type != 0`) → error. Nodes: tag→index map (`node_tags`), coordinates read as 3 values, first `dim` kept; `nb_global_nodes` set. Elements: per element `n_tags` tags stored as `Int` data `"tag_0".."tag_{n-1}"`; connectivity via node tags + read order; `element_tags` map (file id → Element). `$Periodic` parsed but **ignored** (code `continue`s before using it). After reading: `constructPhysicalNames("tag_0")`, if `$PhysicalNames` present `createGroupsFromMeshData<std::string>("physical_names")`, then `MeshUtils::fillElementToSubElementsData(mesh)`. | core | — | all 35 committed `.msh` fixtures (all v2.2: `test/test_fe_engine/_*.msh`, `test_buildfacets/*.msh`, `purify_mesh.msh`, `test_dumper.msh`, `test_geometry_*.msh`, `segment.msh`, `pentahedron.msh`, `test_segment_nodetype/mesh.msh`) | serial read only | Meshes generated at build time with **gmsh from `.geo`** (`add_mesh`) → fixture content depends on gmsh version: freeze generated meshes from the oracle build. Element node arrays from `fillElementToSubElementsData` are side effects of reading. |
| IO-006 | Gmsh MSH 1.0 (`$NOD`/`$ELM`) | `mesh_io_msh.cc:326-430` (`readers["$NOD"]`, `readers["$ELM"]`, version<2000 branch reads `reg-phys reg-elem nb-nodes` into `tag_0`, `tag_1`) | Code exists but readers are registered only inside the `$MeshFormat` handler; v1 files have no `$MeshFormat` → `$NOD` treated as unsupported block, skipping until `$EndNOD` (v1 uses `$ENDNOD`) → EOF exception. **Effectively unsupported.** | core | — | none | — | Port: either implement v1 properly (documented deviation) or mark unsupported; never silently loop. |
| IO-007 | Gmsh MSH 4.0 / 4.1 ASCII reader | `mesh_io_msh.cc:475-690` (`populateReaders4`: `$Entities`, `$Nodes`, `$Elements`) | `$Entities`: for each entity physical tags → creates element group named from `$PhysicalNames` (key `(entity_dim, phys_tag)`) or `"msh_block_<tag>"`, with dimension = entity dim (`addDimension` if exists); records entity→physical tags. **`$PhysicalNames` must precede `$Entities`** (true in Gmsh output). 4.1 header has min/max node tags; 4.0 point entities have bbox, 4.1 only point coords. `$Nodes`: block-wise; parametric nodes → error. `$Elements`: per block, `"tag_0"` (`Idx`) = first physical tag of the entity (0 if none), `"physical_names"` (string) = name of first physical tag if named (else ""), element added (without nodes duplicates check) to each *named* physical group; at end node groups optimised. v4 path does **not** call `constructPhysicalNames`/`createGroupsFromMeshData`. | core | — | All 35 committed `.msh` fixtures in scope are v2.2, but meshes generated at build time by `add_mesh` (`cmake/Modules/FindGMSH.cmake:96-97`: `gmsh -<dim> -order <o> -o out.msh in.geo`, **no `-format` flag**) are MSH 4.1 with gmsh ≥ 4 → `test_mesh_io_msh*`, `test_mesh_boundary`, `test_facet_element_mapping`, `test_grid`, `test_mesh_iterators`, `test_mesh_partitionate*`, `test_mesh_periodic` exercise this reader | — | Groups `"msh_block_<tag>"` for unnamed physical tags are created but **never filled** (elements only added for named physicals) — bug. Elements of entities with several physical tags only get the first name in `physical_names` but join all named groups. |
| IO-008 | Gmsh structural reader (`_miot_gmsh_struct`) | `mesh_io_msh_struct.{hh,cc}` (`MeshIOMSHStruct`) | Reuses MSH reader with remapped types: segment(1) → `_bernoulli_beam_2` (2D mesh) or `_bernoulli_beam_3` (3D mesh, warning: provide `"extra_normal"` element data), triangle(2) → `_discrete_kirchhoff_triangle_18`; identity read order. | structural_mechanics | — | `test_fe_engine_structural_fixture.hh` (`_bernoulli_beam_2.msh`, `_bernoulli_beam_3.msh`, `_discrete_kirchhoff_triangle_18.msh`), structural model tests (R3) | — | `extra_normal` must be provided by user code (e.g. via `$ElementData`?). |
| IO-009 | Gmsh `$NodeData` / `$ElementData` / `$ElementNodeData` reading | `mesh_io_msh.cc:726-880` (`read_data` helper) | Reads string/real/int tag headers; name = first string tag (quotes trimmed), `nb_component` = int tag[1], count = int tag[2]. NodeData → nodal mesh data `Array<double>` (resized to count — indexing by node index from tag map). ElementData → elemental `double` data allocated per type on first element (size = nb elements of type). ElementNodeData → elemental data marked nodal, comps = nb_comp × nb_nodes_per_element but **only `nb_component` values read** per element (bug). | core | — | none in core tests | — | Partial/buggy features; decide port semantics explicitly. |
| IO-010 | Gmsh MSH 2.2 writer | `mesh_io_msh.cc:923-1110` (`MeshIOMSH::write`) | Writes `$MeshFormat 2.2 0 8`, nodes `i+1 x y z` with `setprecision(digits10=15)` and `std::uppercase` (exponents `E`), missing coords padded `0`; elements (not_ghost, all dims/kinds in enum order) `id type 2 tag0 tag1 conn+1...` (`tag_0`/`tag_1` from Int data if present else 0); `$NodeData` blocks for each **nodal** Real dataset. Elemental-data block (only if the mesh has any elemental data, e.g. Gmsh `tag_*`) iterates `getTagNames()` — which returns the **nodal** dataset names — and calls the const `getElementalData<double>(name)`, which throws `"No dataset named ... found"` unless an elemental dataset of the same name exists → elemental data is never written, and a mesh having both nodal data and elemental tags cannot be written (`mesh_io_msh.cc:1053-1105`, `mesh_data_tmpl.hh:184-191, 388-394`). | core | — | `test_mesh_io_msh.cc` (read + dump), Python `mesh.write` | — | 15 significant digits is lossy for doubles (round-trip not exact). Bug to be confirmed on the oracle (Q-list). |
| IO-011 | Diana mesh reader | `mesh_io_diana.{hh,cc}` (`MeshIODiana::read`, `readCoordinates`, `readElements`, `readConnectivity`, `readMaterialElement`, `readMaterial`, `readGroups`, `readInterval`) | Sections `'COORDINATES'`, `'ELEMENTS'` (`CONNECTIVITY` → types `T9TM→_triangle_3, CT6CM→_triangle_6, Q12TM→_quadrangle_4, CQ8CM→_quadrangle_8, TP18L→_pentahedron_6, CTP45→_pentahedron_15, TE12L→_tetrahedron_4, HX24L→_hexahedron_8, CHX60→_hexahedron_20`, elements may span 2 lines; `MATERIALS` → Int data `"material"` per element from `/a-b/ mat` ranges), `'MATERIALS'` (writes `material_<filename>` input file with `material elastic [ name = materialN; E/rho/nu ]` from `YOUNG/DENSIT/POISON`), `'GROUPS'` (`NODES`/`ELEMEN` groups with intervals `a-b`). Node ids shifted by the first node number (assumes contiguous numbering). Ends with `fillElementToSubElementsData`. Node-order tables for penta15 and hex20 (`mesh_io_diana.cc:54-100`). | core | — | **none** (no Diana test in tree) | — | Reads coordinates as 3-vectors and `push_back`s into an Array with `dim` comps → **fails for 2D meshes** (view size check). Writer `AKANTU_TO_IMPLEMENT`. Writes a side file next to input. Low priority; port with tests created from scratch. |
| IO-012 | Abaqus reader (absent) | `packages/core.cmake` `AKANTU_SPIRIT_SOURCES` lists `io/mesh_io/mesh_io_abaqus.cc` (file **does not exist**); `MeshIOType::_miot_abaqus` | Enum value only; no implementation. Also stale: `parser_random.cc`, `parser_types.cc` listed but absent. | — | — | — | — | Mark "not present in reference" in parity matrix (do not invent). |

### 1.8 DUMP — output (`src/io/dumper/`, `third-party/iohelper`)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| DUMP-001 | `Dumpable` mixin (dumper registry) | `dumpable.hh`, `dumpable_iohelper.hh`, `dumpable_inline_impl.hh`, `dumpable.cc` (`registerDumper<T>(name, file_name, is_default)`, `registerExternalDumper`, `addDumpMesh[ToDumper]`, `addDumpFilteredMesh[ToDumper]`, `addDumpField[ToDumper]` (base: not implemented; models override), `addDumpFieldExternal[ToDumper]` (Array → `NodalField`, ElementTypeMapArray → `ElementalField`, or a `dumpers::Field`), `removeDumpField[FromDumper]`, `addDumpFieldVector/Tensor[ToDumper]`, `setDirectory[ToDumper]`, `setBaseName[ToDumper]`, `setTimeStepToDumper`, `setTextModeToDumper`, `dump()`, `dump(step)`, `dump(time, step)`, `dump(name[, step / time, step])`, `getDumper([name])`, `getDefaultDumperName`) | Named dumpers in `std::map`; default dumper name; registering an existing name replaces it (info message). Unknown dumper → exception. Mesh, groups, models are Dumpable. | core (interface) + dumpers | IOHelper | `test/test_io/test_dumper/test_dumper.cc` (+ `input_file.dat`, `test_dumper.msh`) | — | `dumpable_dummy.hh` (no-op variant) exists but is **never included** (dumpers always compiled in). |
| DUMP-002 | `DumperIOHelper` (field registry & dump driver) | `dumper_iohelper.{hh,cc}` (`registerMesh` (fields `"connectivities"`=`ElementalField<Idx>`, `"positions"`=`NodalField<Real>`), `registerFilteredMesh` (`FilteredConnectivityField`, filtered positions), `registerField` (duplicate id → warning, ignored), `unRegisterField`, `registerVariable`, `unRegisterVariable`, `dump()` (count++), `dump(step)` (count=step), `dump(time, step)`, `setParallelContext`, `setDirectory`, `setBaseName`, `setTimeStep` (first call activates time description files)) | IOHelper exceptions converted to `AKANTU_ERROR`. | dumpers | IOHelper | `test_dumper` | parallel context = rank/size (per-rank VTU pieces) | — |
| DUMP-003 | Element type → IOHelper type | `dumper_iohelper.cc:200-290` (`getIOHelperType<type>()`, `getIOHelperType(type)`) | point→POINT_SET, seg2→LINE1, seg3→LINE2, tri3→TRIANGLE1, tri6→TRIANGLE2, quad4→QUAD1, quad8→QUAD2, tet4→TETRA1, tet10→TETRA2, hex8→HEX1, hex20→HEX2, penta6→PRISM1, penta15→PRISM2, cohesive 1d2/2d4/2d6/3d6/3d12/3d8 → COH*, beams → BEAM2/BEAM3. **`_cohesive_3d_16` and `_discrete_kirchhoff_triangle_18` → `AKANTU_TO_IMPLEMENT` (throws)**. | dumpers | IOHelper | `test_dumper` | — | Dumping meshes with cohesive_3d_16 or DKT18 fails in the reference. |
| DUMP-004 | ParaView (VTU/PVTU/PVD) dumper | `dumper_iohelper_paraview.{hh,cc}` (`DumperParaview(filename, directory="./paraview", parallel=true)`, `setBaseName` → VTU sub-dir `<name>-VTU`); `third-party/iohelper/src/dumper_paraview.{hh,cc}`, `paraview_helper.{hh,cc,tcc}`, `base64.hh`, `dumper.{hh,cc}`, `file_manager.hh` | Files: `<dir>/<name>_<step:04d>.pvtu` (rank 0) referencing `<dir>/<name>-VTU/<name>_<step:04d>[.proc<rank:width ceil(log10(nproc))>].vtu`; with time step activated, `<dir>/<name>.pvd` collection updated each dump (time += Δt). XML `UnstructuredGrid` v0.1 LittleEndian; default **BASE64 binary** inline data (TEXT and COMPRESSED modes exist); positions always 3 comps (`Float64`); connectivity `Int32`, offsets `Int32`, types `UInt32`; VTK cell types: tri3 5, tri6 22, tet4 10, tet10 24, point set 2, line 3, quad. edge 21, quad 9, quad8 23, hex 12, hex20 25, beams 3, wedge 13, quad. wedge 26, COH1D2 3, COH2D4/2D6 polygon 7, COH3D6 wedge 13, COH3D12 `VTK_QUADRATIC_LINEAR_WEDGE` 31, COH3D8 hex 12. Node reorder on write: COH3D12 `[0,1,2,6,7,8,3,4,5,9,10,11]`, COH2D6 `[0,2,1,4,5,3]`, COH2D4 `[0,1,3,2]`, HEX2 `12..15↔16..19`, PRISM2 `[0..8,12,13,14,9,10,11]`. Point data / cell data arrays from registered fields (non-homogeneous fields → exception). | dumpers | IOHelper (bundled, LGPL) | `test_dumper.cc`; `third-party/iohelper/test/test_paraview_dumper.cc` (not built by Akantu) | per-rank pieces + PVTU by rank 0 | Output must be readable by ParaView; bitwise identity of base64 payload requires identical value formatting (binary doubles → identical if same values). |
| DUMP-005 | Text dumper | `dumper_text.{hh,cc}` (`DumperText(basename="dumper_text", TextDumpMode=_tdm_space, parallel=true)`, `registerMesh` (field `"position"` + `"nodes_type"` in parallel), `registerFilteredMesh`, `setBaseName` (data sub-dir `<name>-DataFiles`), `setPrecision`); `third-party/iohelper/src/dumper_text.{hh,cc}` | Writes `<name>.info` description file and per-field data files (`.csv` in csv mode, space separated otherwise), optional time file, precision default 6. Only nodal fields (positions, displacements...). | dumpers | IOHelper | `test_dumper.cc` (`_tdm_csv`) | `nodes_type` in parallel | `registerMesh` wraps NodeFlag field with `ComputeIntFromEnum<ContactState>` (type mismatch vs. filtered version using `NodeFlag`) — latent bug. |
| DUMP-006 | Field base class & homogeneity | `dumper_field.hh` (`dumpers::Field`: `registerToDumper`, `setPadding`, `setNbData`, `setNbDataPerElem`, `getNbComponents`, `connect(FieldComputeProxy / HomogenizerProxy)`, `checkHomogeneity`, `isHomogeneous`) | Fields iterate tuples; IOHelper requires homogeneous component counts across types (else homogenisation/padding). | dumpers | — | `test_dumper` | — | — |
| DUMP-007 | Nodal field | `dumper_nodal_field.hh` (`NodalField<T, filtered, Container, Filter>`: `n`, `stride`, filter, padding) | Dumps an `Array<T>` per node (optionally a strided subset of components `[stride, stride+n)` and/or a node filter list); `getDim()` returns padding if set. | dumpers | — | `test_dumper` (group fields) | — | Used by models for displacement, velocity, etc. (padding 3 in 2D for ParaView vectors). |
| DUMP-008 | Elemental field (+ generic machinery) | `dumper_elemental_field.hh` (`ElementalField<T, ret, filtered>`), `dumper_generic_elemental_field.{hh}`, `dumper_generic_elemental_field_tmpl.hh` (`GenericElementalField::checkHomogeneity`, `getNbDataPerElem`, `begin/end`), `dumper_element_iterator.hh` (`element_iterator`, `elemental_field_iterator`), `dumper_type_traits.hh` (`TypeTraits`, `SingleType`, `DualType`) | Iterates element types in (dim, ghost, kind) filter order, one tuple per element (`nb_data_per_elem` comps; default = array comps). Non-homogeneous across types → `homogeneous=false`. | dumpers | — | `test_dumper` | — | — |
| DUMP-009 | Quadrature-point / internal material fields | `dumper_quadrature_point_iterator.hh`, `dumper_internal_material_field.hh` (`InternalMaterialField<T, filtered>`), `mesh.cc:470-527` (`createFieldFromAttachedData`) | Per-element field whose tuple concatenates all quadrature-point values (`nb_data_per_elem = nb_comp × nq`); used for material internals and mesh data dumps. | dumpers | — | solid mechanics dumps (R2) | — | Usually combined with homogenizer (DUMP-012). |
| DUMP-010 | Filtered connectivity field | `dumper_filtered_connectivity.hh` (`FilteredConnectivityField`, `filtered_connectivity_field_iterator`) | Renumbers group connectivities into the group's compact node numbering (index of node in the node filter) for group dumps. | dumpers | — | `test_dumper` (group dumpers), group ParaView dumpers | — | Node lookup via `Array::find` (linear). |
| DUMP-011 | Element partition field | `dumper_element_partition.hh` (`ElementPartitionField<filtered>`) | Per-element int = MPI rank (`prank`) of the owner; used for `"partitions"` dump field. | core (header) | — | parallel dumps (R3) | rank value | — |
| DUMP-012 | Compute fields & homogenisation | `dumper_compute.hh` (`ComputeFunctorInterface`, `ComputeFunctorOutput`, `ComputeFunctor<In,Out>`, `ComputeIntFromEnum<Enum>`, `FieldCompute`, `FieldComputeProxy::createFieldCompute`), `dumper_homogenizing_field.hh` (`AvgHomogenizingFunctor`, `HomogenizerProxy`, `homogeneize(field)`) | Applies per-tuple functors to a field (e.g. enum → int); homogenizer averages the `nb_data` blocks of each element tuple (e.g. quad-point average → one value per element). | dumpers | — | model dumps (R2) | — | Averaging order (sequential sum then divide) must match for bitwise equality. |
| DUMP-013 | Padding helpers | `dumper_padding_helper.hh` (`PadderInterface`, `PadderGeneric<Vector<T>,Matrix<T>>`) | Pads matrix-valued data to `padding_m × padding_n` per data block (e.g. 2×2 → 3×3 with zeros) for ParaView tensors. | dumpers | — | model dumps | — | — |
| DUMP-014 | Material-aware padders & derived fields (solid mechanics coupling) | `dumper_material_padders.hh` (`MaterialFunctor`, `MaterialPadder`, `StressPadder<dim>` (3×3; plane strain in 2D: `σzz = ν(σxx+σyy)`, uses material params `Plane_Stress`, `nu`), `StrainPadder<dim>` (plane stress: `εzz = ν/(ν−1)(εxx+εyy)`), `ComputeStrain<green>`, `ComputePrincipalStrain<green>` (unsorted `eig`), `ComputeVonMisesStress`) | Model-specific functors living in core io; depend on `SolidMechanicsModel`/`Material`. | core header (used with solid_mechanics) | — | solid mechanics dump tests (R2) | — | Owned jointly with R2 (materials). `ComputePrincipalStrain` uses general (non-symmetric) eigen solver without sorting → order not guaranteed. |
| DUMP-015 | Dumper variables | `dumper_variable.hh` (`VariableBase`, `Variable<T>`, `Variable<Vector<T>>`) | Scalar/vector variables written in dump metadata (IOHelper variables). | dumpers | IOHelper | — | — | Low use. |
| DUMP-016 | IOHelper library pieces not used by Akantu | `third-party/iohelper/src/dumper_lammps.*`, `dumper_restart.*`, `reader*.{cc,hh}`, `dumper_C_wrapper.*`, `reader_C_wrapper.*`, `base64_reader.hh`, `example/`, `doc/`, `test/` | LAMMPS dumper, restart dumper/reader, C wrappers — compiled (lammps) or not compiled, never referenced from Akantu `src/`. | IOHelper | — | iohelper own tests (not part of Akantu suite) | — | Out of scope for parity (record as "unused third-party functionality"). |

### 1.9 PARSE — input files & command line (`src/io/parser/`)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| PARSE-001 | Input-file grammar (`.dat`) | `input_file_parser.hh:112-245` (`InputFileGrammar`: `start`, `mini_section`, `entry`, `section`, `section_name`, `section_option`, `key`, `value`, `mono_line_value`, `skipper`), `parser_input_files.cc` (`Parser::parse(filename)`) | Whole file = section content. Entry: `key = value` with key `[a-zA-Z_][a-zA-Z_0-9]*`; value = rest of line up to `#`, `;`, `=`, `\` or EOL (lexeme, inner spaces kept), `\` continues on next line (pieces concatenated). Section: `type [name [option]] [ ... ]` — type case-insensitive among `ParserType` names (PARSE-002), name/option `[a-zA-Z_][a-zA-Z0-9_:]*`, nested sections allowed. Comments `#` to EOL; whitespace skipped. Unnamed sections get names `anonymous_<n>` from a **process-global counter starting at 12**. Duplicate parameter in a section → exception. Parse errors report file/line/column (debug builds attach `on_error`). After parsing: `permissive_parser` global flag read from top-level parameter. | core | Boost.Spirit Qi, Phoenix | `test/test_io/test_parser/test_parser.cc` (+ `input_file.dat`, `test_parser.verified`), `test_parser.py` (Python) | input read on every rank independently | Rust: hand-written recursive-descent or `pest`/`nom` grammar reproducing exact token rules; keep debug positions. Values are stored as raw strings (trailing spaces included?) — verify `trim` behaviour against `.verified` output. |
| PARSE-002 | Section types | `parser.hh:31-70` (`AKANTU_SECTION_TYPES`: `cohesive_inserter, contact, embedded_interface, friction, global, heat, integration_scheme, material, phasefield, constitutive_law, mesh, model, model_solver, neighborhood, neighborhoods, non_linear_solver, non_local, rules, solver, time_step_solver, user, weight_function, contact_detector, contact_resolution, not_defined`) | `ParserType` class enum with string I/O; matching uses Spirit `symbols` (longest match). | core | — | `test_parser` | — | Consumers of each type live in other areas (R2/R3). |
| PARSE-003 | Parser data model | `parser.hh:80-470`, `parser.cc` (`ParserParameter` (name, raw value, parent, debug file/line/col), `ParserSection` (name, type, option, `std::map` parameters, `std::multimap<ParserType, ParserSection>` sub-sections), `getSubSections(type)`, `getNbSubSections`, `getParameters`, `getParameter(name, ctx)`, `getParameter(name, default, ctx)`, `hasParameter`, `getParameterValue<T>`, `getName`, `getType`, `getOption(default)`, `printself`; `Parser` (root `"global"`), `getLastParsedFile`, `isPermissive`/`setPermissive`, `ParserParameterSearchCxt` {`_ppsc_current_scope`=1, `_ppsc_parent_scope`=2, `_ppsc_current_and_parent_scope`=3}) | Parameters sorted by name; sub-sections ordered by type then insertion. Lookup can climb to parent sections. | core | — | `test_parser` (+ `.verified` printout) | — | `getParameter` with parent-only context uses an uninitialised iterator (UB) — avoid. |
| PARSE-004 | Typed parameter conversion | `parser_tmpl.hh` (`operator T()` via `stringstream`, `operator Real()` → `parseReal`, `operator bool()` (boolalpha then numeric), `operator std::vector<std::string>()` / `std::set<std::string>()` (strip spaces & brackets, split on commas), `operator Vector<T>` → `parseVector`, `operator Matrix<T>` → `parseMatrix`, `operator RandomParameter<Real>` → `parseRandomParameter`) | Generic types use `>>` (stops at whitespace; failbit ignored, only badbit checked). | core | — | `test_parser`, `test_parser_gtest` | — | `int` params parsed with `>>` (e.g. `"3.5"` → 3 silently). |
| PARSE-005 | Scalar algebraic expressions (X3) | `algebraic_parser_x3.{hh,cc}` (`parser::algebraic::parse_real`) used by `Parser::parseReal` (`parser_real.cc:38-41`) | Grammar: `expr = term (('+' \| '-') term)*`, `term = factor (('*' \| '/') factor)*`, `factor = number ('**' number)*` (**left-assoc power, unary minus binds tighter**: `-2**2 = 4`), `number = double \| '-' number \| '+' number \| function \| constant \| variable \| '(' expr ')'`. Functions (case-sensitive): unary `abs acos asin atan ceil cos cosh exp floor log10 log sin sinh sqrt tan tanh acosh asinh atanh exp2 expm1 log1p log2 erf erfc lgamma tgamma trunc round`; binary `pow min max atan2 fmod hypot`. Constants `pi`, `e`. Variables = other parameters looked up in current then parent scopes (recursively parsed). Whole string must be consumed. | core | Boost.Spirit X3 | `test/test_io/test_parser/test_parser_gtest.cc` (AlgebraicParser.Number/Expr/Term/Factor/Func...) | — | Symbol tables match prefixes: a variable named e.g. `pi2` or `eta` is read as constant `pi`/`e` followed by garbage → parse error (quirk to reproduce or document). `double_` accepts `inf`, `nan`. |
| PARSE-006 | Vector / matrix expressions (Qi) | `algebraic_parser.hh` (`AlgebraicGrammar` (Qi variant), `VectorGrammar` `'[' expr (',' expr)* ']'`, `MatrixGrammar` `'[' row (',' row)* ']'` with row = vector literal or name of a vector parameter, `parsable_vector`, `parsable_matrix` (ragged rows zero-padded)), `parser_real.cc` (`parseVector`, `parseMatrix`), `parser_grammar_tmpl.hh` (`Parser::parseType`) | Element expressions use the Qi grammar whose `number` alternatives try **constant before function** and functions with `no_case` → inside vectors, `exp(...)`, `erf(...)` etc. are mis-parsed as constant `e` (error); X3 scalar grammar tries functions first. Matrix rows are vectors. | core | Boost.Spirit Qi | `test_parser` (`vect`, `mat` in `input_file.dat`) | — | Two slightly different expression grammars exist — port must reproduce both behaviours (or unify and document deviation). |
| PARSE-007 | Random-parameter expressions | `algebraic_parser.hh:420-505` (`RandomGeneratorGrammar`, `ParsableRandomGenerator`), `parser_real.cc:43-62` (`parseRandomParameter`) | `base [dist '[' p1 (',' p2)? ']']`; distributions `uniform exponential gamma weibull extreme_value normal lognormal chi_squared cauchy fisher_f student_t`; maps to `make_random_parameter(base, type, a, b)` (CORE-032, weibull param swap). A plain number gives type `_not_defined` → `uniform(0,0)` (deterministic). | core | Boost.Spirit Qi | `test_parser` (`rand1 = 10 uniform [0.2, 0.5 ]`, `rand2 = 10 weibull [...]`, `rand3 = 10`) | random fields seeded per rank | Printing format `"<base> + <dist> [ params ]"` appears in `.verified`. |
| PARSE-008 | Parameter registry | `parameter_registry.{hh,cc}`, `parameter_registry_tmpl.hh` (`ParameterAccessType` {`_pat_internal`=0x0001, `_pat_writable`=0x0010, `_pat_readable`=0x0100, `_pat_modifiable`=0x0110, `_pat_parsable`=0x1000, `_pat_parsmod`=0x1110}, `Parameter`, `ParameterTyped<T>` (`set(std::any)`, `setAuto(ParserParameter)`, `to_string`, `operator Real`), specialisations for `std::string`, `Vector`, `Matrix`, `RandomParameter`, `std::vector/set<std::string>`, `ParameterRegistry::registerParam(name, var, [default,] type, description)`, `registerSubRegistry`, `set(name, any)`, `get(name)`, `listParameters`, `hasParameter`, `listSubRegisteries`, `setParameterAccessType`, `printself`; exceptions `ParameterException`, `ParameterAccessRightException`, `ParameterWrongTypeException`, `ParameterUnexistingException`) | Binds names to member variables with access rights; parsing a non-parsable param throws; get/set check readable/writable; sub-registries searched recursively. Print format `" rwp name [description] : value"`. | core | — | material/model tests (R2), `test_parser` | — | Backbone of material/solver configuration and Python `getParam/setParam`. |
| PARSE-009 | `Parsable` objects | `parsable.{hh,cc}` (`Parsable(section_type, id)`, `registerSubSection(type, name, parsable)`, `parseParam`, `parseSection`, `parseSubSection`) | `parseSection` checks the section type, applies each parameter (unknown → exception unless permissive, then warning), dispatches sub-sections by `(type, name)` key, then `updateInternalParameters()`. | core | — | via models (R2/R3) | — | — |
| PARSE-010 | PETSc options bridge | `parsable_petsc.hh` (`ParsablePETSc<ParsableParent, PETScType>`) | Parameters of a section are pushed as PETSc options (`PetscOptionsPush`) then `set_object_from_options`, then command-line/env options applied. | petsc | PETSc | PETSc solver tests (R3) | — | Owned by R3 (solvers); listed for file coverage. |
| PARSE-011 | Command-line argument parser (`cppargparse`) | `cppargparse/cppargparse.{hh,cc}`, `cppargparse_tmpl.hh` (`ArgumentParser::addArgument(name_or_flags, help, nargs, type[, default[, const]])`, `parse(argc, argv, flags=_stop_on_not_parsed, parse_help)`, `operator[]`, `has`, `print_help`, `print_usage`, `setExternalExitFunction`, `setParallelContext`, `ArgumentType` {`_string,_integer,_float,_boolean`}, `ArgumentNargs` {`_one_if_possible=-1,_at_least_one=-2,_any=-3`}, `ParseFlags` {`_no_flags`, `_stop_on_not_parsed`, `_remove_parsed`}) | Python-argparse-like: `-h/--help` built in; positional & optional args; `_remove_parsed` strips consumed args from argv (except help); help printed only on rank 0 in parallel. | core (bundled, GPLv3/LGPLv3 COPYING files) | — | used by all executables via `initialize` | prints only on rank 0 | Bundled third-party with its own COPYING (GPL text + LGPL) — record in DEPENDENCIES. Rust: `clap`-like or custom; must accept unknown args (examples pass their own). |
| PARSE-012 | User parameters access | `aka_common.cc:115-125` (`getStaticParser`, `getUserParser`), `aka_common.hh:686-690` | Global parser instance filled by `initialize`/`readInputFile`; `getUserParser()` = first `user` section. | core | — | examples (R4), Python | — | See CORE-038. |

### 1.10 GEOM — computational geometry (`src/geometry/`, CGAL)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| GEOM-001 | CGAL kernels | `mesh_geom_common.hh` (`cgal::Cartesian = Simple_cartesian<Real>`, `cgal::Spherical = Spherical_kernel_3<..., Algebraic_kernel_for_spheres_2_3<Real>>`) | Floating-point (inexact) kernels. | CGAL (OFF by default) | CGAL ≥4.5 (+ GMP/MPFR via CGAL) | all geometry tests | — | Port needs robust predicates (e.g. `robust` crate / custom) or exact arithmetic; results equal only to tolerance. |
| GEOM-002 | AABB primitives | `aabb_primitives/triangle.hh` (`Triangle<K>` with mesh id), `tetrahedron.hh` (`Tetrahedron<K>`), `line_arc.hh` (`Line_arc<K>` with mesh & segment ids), `aabb_primitive.{hh,cc}` (`AKANTU_AABB_CLASS` primitive adaptors, `Triangle_primitive`, `Line_arc_primitive`, `Tetrahedron_primitive` datum/reference point) | CGAL geometric objects tagged with element ids for AABB trees. | CGAL | CGAL | `test_geometry_intersection.cc` | — | — |
| GEOM-003 | AABB tree helpers | `tree_type_helper.hh` (`TreeTypeHelper<Primitive,Kernel>`, `VoidTree`, `IntersectionTypeHelper`) | Type plumbing for CGAL `AABB_tree` per primitive/kernel; invalid combos map to `VoidTree`. | CGAL | CGAL | — | — | — |
| GEOM-004 | Mesh geometry factory | `mesh_geom_abstract.hh` (`MeshGeomAbstract::constructData`), `mesh_geom_factory.{hh}`, `mesh_geom_factory_tmpl.hh` (`MeshGeomFactory<dim, type, Primitive, Kernel>::constructData`, `addPrimitive` specialisations: tri3 (2D, as 3D triangles z=0), tet4 (as 4 triangle facets or tetrahedra), segment → line arcs for spheres) | Builds a primitive list and AABB tree from mesh elements of one type. | CGAL | CGAL | `test_segment_intersection_*` | local elements only (ghost_type arg) | — |
| GEOM-005 | Abstract intersector | `mesh_abstract_intersector.{hh}`, `mesh_abstract_intersector_tmpl.hh` (`MeshAbstractIntersector<Query>`: `computeIntersectionQuery`, `computeIntersectionQueryList`, `computeMeshQueryIntersectionPoint`, `computeMeshQueryListIntersectionPoint`, `buildResultFromQueryList`, `intersection_points`) | Generic driver over query lists. | CGAL | CGAL | — | — | — |
| GEOM-006 | Mesh–geometry intersector base | `mesh_geom_intersector.{hh}`, `mesh_geom_intersector_tmpl.hh` (`MeshGeomIntersector<dim,type,Primitive,Query,Kernel>`) | Owns a `MeshGeomFactory`. | CGAL | CGAL | — | — | — |
| GEOM-007 | Segment–mesh intersection | `mesh_segment_intersector.{hh}`, `mesh_segment_intersector_tmpl.hh` (`MeshSegmentIntersector<dim,type>(mesh, result_mesh)`, `computeIntersectionQuery(Segment_3)`, `computeSegments`, `buildResultFromQueryList`, `setPhysicalName`) | Intersects segments with tri3 (2D) / tet4 (3D) meshes; produces `_segment_2` elements in `result_mesh` (two new nodes per segment, no sharing) with mesh data `"associated_element"` (Element) and `"physical_names"`. Handles segments fully inside one element (ray tests) and point intersections in tet4 (local facet tree). Degenerate segments skipped; set ordered by `segmentPairsLess`. | CGAL | CGAL | `test/test_geometry/test_segment_intersection_triangle_3.cc` (`test_geometry_triangle.msh`), `test_segment_intersection_tetrahedron_4.cc` (`test_geometry_tetrahedron.msh`) | — | Used by embedded reinforcement model (`EmbeddedInterfaceIntersector`, R2/R3). |
| GEOM-008 | Sphere–mesh intersection | `mesh_sphere_intersector.{hh}`, `mesh_sphere_intersector_tmpl.hh` (`MeshSphereIntersector<dim,type>`, `constructData`, `computeMeshQueryIntersectionPoint`, `setToleranceIntersectionOnNode` (default 1e-10), `new_node_per_elem`) | Computes intersection points of spheres with element edges (line arcs) for IGFEM enrichment; temporarily changes global `Math::tolerance`. | CGAL (used by IGFEM) | CGAL spherical kernel | `test_segment_intersection_triangle_3.cc` (sphere part) | — | Mutates global tolerance during computation. |
| GEOM-009 | Geometry predicates helpers | `geom_helper_functions.hh` (`comparePoints` (Cartesian & circular-arc points), `compareSegments` (same or reversed endpoints), `segmentPairsLess`, `IsSameSegment`) | Exact equality comparisons on CGAL points/segments. | CGAL | CGAL | `test/test_geometry/test_geometry_predicates.cc`, `test_geometry_intersection.cc` | — | — |

### 1.11 ITER — `third-party/akantu_iterators` (used functionality)

| ID | Feature | Original files / key symbols | Behavior summary | Package / build option | External deps | Upstream tests & fixtures | Parallel aspects | Porting notes / risks |
|---|---|---|---|---|---|---|---|---|
| ITER-001 | `zip`, `zip_cat`, `zip_append`, named zip | `include/iterators/aka_zip_iterator.hh` (`ZipIterator`, `NamedZipIterator`, `zip(...)`, `zip_iterator`, `zip_cat`, `zip_append`) | Lock-step iteration over several containers (tuple of references); named variant with `"name"_n` tags. | akantu_iterators (bundled, default ON) | — | `third-party/akantu_iterators/test/test_iterators.cc` (own gtest, not in Akantu suite) | — | Rust: `itertools::izip!`/`zip`. Size mismatch is not checked (stops at first end? equality on all) — check semantics when porting loops. |
| ITER-002 | `enumerate` | `iterators/aka_enumerate_iterator.hh` | (index, value) pairs; index type from container `size_type`. | akantu_iterators | — | `test_iterators.cc` | — | `.enumerate()`. |
| ITER-003 | `arange`, `range` | `iterators/aka_arange_iterator.hh` (`arange(stop)`, `arange(start, stop[, step])`, `range(it1, it2)`) | Integer ranges; `size = (stop-start)/step`. | akantu_iterators | — | `test_iterators.cc` | — | Rust ranges / `step_by`. |
| ITER-004 | `filter`, `filter_if` | `iterators/aka_filter_iterator.hh` (`FilterIterator`, `filter(index_list, container)`, `filter_if(container, pred)`) | Iterate container elements at given indices / matching predicate. | akantu_iterators | — | `test_iterators.cc`; `test_array.cc` IteratorFilter | — | — |
| ITER-005 | Transform adaptors | `iterators/aka_transform_iterator.hh` (`make_transform_adaptor`, `make_keys_adaptor`, `make_values_adaptor`, `make_dereference_adaptor`) | Lazy map over containers / map keys / values. | akantu_iterators | — | `test_iterators.cc` | — | `.map()`, `.keys()`, `.values()`. |
| ITER-006 | `concat`, `repeat_n` | `iterators/aka_concatenate_iterator.hh`, `iterators/aka_repeat_iterator.hh` | Concatenate heterogeneous ranges; repeat each element n times. | akantu_iterators | — | `test_iterators.cc` | — | `.chain()`, `flat_map(repeat)`. |
| ITER-007 | Tuple tools & named tuples | `aka_tuple_tools.hh` (`tuple::cat_t`, `filter_t`, `transform`, `foreach`, `flatten`, `replace`, `make_tuple_no_decay`), `aka_named_tuple.hh` (`named_tuple`, `named_tag`, `get<"x"_h>`, `operator""_n`), `aka_str_hash.hh` (compile-time CRC32/FNV-1a string hashes), `aka_static_if.hh`, `aka_compatibilty_with_cpp_standard.hh` (`aka::conjunction`, `bool_constant`, `make_from_tuple`, ...) | Metaprogramming support used for element-type tuples and named zips. | akantu_iterators | — | `test_tuples.cc`, `test_str.cc` | — | Not ported as such (Rust generics/macros); no behaviour to verify beyond callers. |

---

## 2. Porting-critical conventions

Everything in this section must be reproduced bit-for-bit (or deviations
recorded in `docs/DECISIONS.md`). Paths are relative to the reference root.

### 2.1 Storage order and data layout

| Item | Convention | Reference |
|---|---|---|
| `Array<T>` | `size` tuples × `nb_component` values, contiguous, **tuple-major**: value `(i, j)` at flat index `i*nb_component + j`. | `src/common/aka_array_tmpl.hh:380,394` |
| Dense matrices | Eigen **column-major** (`EIGEN_DEFAULT_MATRIX_STORAGE_ORDER_OPTION Eigen::ColMajor`), index type `Idx`. A matrix view of an Array tuple with `m×n` comps stores `M(i,j)` at tuple offset `i + j*m`. | `src/common/aka_types.hh:34-37`; views `aka_types.hh:240-330`, `aka_array_tmpl.hh:760-880` |
| `M(c)` / range-for on a matrix | Returns / iterates **columns** (Akantu Eigen plugin). | `src/common/aka_types_eigen_matrix_base_plugin.hh:24-36`; `aka_types.hh:355-440` |
| Matrix initializer list | `Matrix{{a,b},{c,d}}` is **row-wise** (`{a,b}` = first row). | `aka_types_eigen_plain_object_base_plugin.hh` |
| `Tensor3` (`TensorBase<T,3>`) | First index fastest: `T(i,j,k)` at `i + n0*(j + n1*k)`; `T(k)` = `n0×n1` column-major matrix slice (contiguous block k). | `src/common/aka_tensor.hh:86-99,136-150` |
| Nodal field | `Array` of size `nb_nodes`, `nb_component` = dofs per node. | everywhere |
| Elemental nodal extraction | per element, `nb_nodes_per_element` blocks of `nb_component` values (node-major); viewed as `(nb_comp × nnodes)` col-major (column = node). | `src/fe_engine/fe_engine_inline_impl.hh:94-130` |
| Quadrature-point field | `Array` of size `nb_element*nq` (per type & ghost); element `e`, point `q` at tuple `e*nq + q` (`IntegrationPoint::global_num`). Per element view `(nb_comp × nq)` col-major. | `fe_engine/integration_point.hh`; `integrator_gauss_inline_impl.hh:402-420` |
| Shapes `N` | Per element per quad point: tuple of `nnodes` values `N_0..N_{n-1}` (stored per element even though identical). | `shape_lagrange_base_inline_impl.hh:30-65` |
| Shape derivatives `B` | Per element per quad point: tuple `dim × nnodes` col-major: `[∂N0/∂x, ∂N0/∂y, (∂N0/∂z), ∂N1/∂x, ...]`. | `shape_lagrange_inline_impl.hh:191-240`; `shape_functions_inline_impl.hh:258-310` |
| Gradient of a field | Per quad point `(nb_dof × dim)` col-major: component `i + j*nb_dof` = `∂u_i/∂x_j` (i.e. `∇u = U_el·Bᵀ`). | `shape_functions_inline_impl.hh:300-304` |
| `BtD` result | Per quad point `(ncomp/dim × nnodes)` col-major = node-major dof vector `[f_0x, f_0y, f_1x, ...]` (`= D·B`). | `shape_lagrange_inline_impl.hh:371-410` |
| `BtDB` result | `(dim·nnodes)²` (order 4, Voigt B, node-major dofs `n*dim + i`) or `nnodes²` (order 2). | `shape_lagrange_inline_impl.hh:411-465`, `aka_voigthelper_tmpl.hh:97-140` |
| `Ntb` / mass blocks | `b·N` → `(nb_dof × nnodes)` col-major (node-major); mass shape block `N` is `(nb_dof × nb_dof·nnodes)` with `N(d, s*nb_dof + d) = N_s`. | `shape_lagrange_inline_impl.hh:500-530`; `fe_engine_template_tmpl_field.hh:270-305` |
| Integrator jacobians | Per element per quad point scalar `det(J)·w_q` (weights included). | `integrator_gauss_inline_impl.hh:378-398,545-560` |
| Cohesive connectivity | Viewed as `(nnodes/2 × 2)` col-major: first half = side ⁺ (facet 0), second half = side ⁻ (facet 1). | `shape_cohesive_inline_impl.hh:140-180`; inserter `cohesive_element_inserter_helper.cc:736-800` |
| `NodeFlag` | `uint8` bit field (CORE-009). | `src/common/aka_common.hh:489-510` |

### 2.2 Element natural coordinates and node numbering

Natural coordinate domains: segment/quad/hex `[-1,1]^d`; triangle/tet unit
simplex (`ξ_i ≥ 0`, `Σξ ≤ 1`); prism `x ∈ [-1,1]` (axis) × `(y,z)` unit triangle.

| Type | Nodes (natural coordinates, in Akantu local order) | Reference |
|---|---|---|
| `_point_1` | 0: (—) | `element_class_point_1_inline_impl.hh` |
| `_segment_2` | 0: −1, 1: +1 | `element_class_segment_2_inline_impl.hh:21-30` |
| `_segment_3` | 0: −1, 1: +1, 2: 0 | `element_class_segment_3_inline_impl.hh:21-40` |
| `_triangle_3` | 0: (0,0), 1: (1,0), 2: (0,1) | `element_class_triangle_3_inline_impl.hh:21-45` |
| `_triangle_6` | 0: (0,0), 1: (1,0), 2: (0,1), 3: (½,0), 4: (½,½), 5: (0,½) | `element_class_triangle_6_inline_impl.hh:21-50` |
| `_quadrangle_4` | 0: (−1,−1), 1: (1,−1), 2: (1,1), 3: (−1,1) | `element_class_quadrangle_4_inline_impl.hh:21-60` |
| `_quadrangle_8` | 0..3 as quad4; 4: (0,−1), 5: (1,0), 6: (0,1), 7: (−1,0) | `element_class_quadrangle_8_inline_impl.hh:21-80` |
| `_tetrahedron_4` | **As implemented** (`N = (1−ξ−η−ζ, η, ζ, ξ)`): 0: (0,0,0), 1: (0,1,0), 2: (0,0,1), 3: (1,0,0). (Doc comment claims the standard order 1: (1,0,0), 2: (0,1,0), 3: (0,0,1).) | code `element_class_tetrahedron_4_inline_impl.hh:65-120`; comment `:21-45` |
| `_tetrahedron_10` | 0: (0,0,0), 1: (1,0,0), 2: (0,1,0), 3: (0,0,1), 4: (½,0,0), 5: (½,½,0), 6: (0,½,0), 7: (0,0,½), 8: (½,0,½), 9: (0,½,½) | `element_class_tetrahedron_10_inline_impl.hh:45-60,145-170` |
| `_pentahedron_6` | 0: (−1,1,0), 1: (−1,0,1), 2: (−1,0,0), 3: (1,1,0), 4: (1,0,1), 5: (1,0,0) | `element_class_pentahedron_6_inline_impl.hh:21-55` |
| `_pentahedron_15` | 0..5 as penta6; 6: (−1,½,½), 7: (−1,0,½), 8: (−1,½,0), 9: (0,1,0), 10: (0,0,1), 11: (0,0,0), 12: (1,½,½), 13: (1,0,½), 14: (1,½,0) | `element_class_pentahedron_15_inline_impl.hh:21-68` |
| `_hexahedron_8` | 0: (−1,−1,−1), 1: (1,−1,−1), 2: (1,1,−1), 3: (−1,1,−1), 4: (−1,−1,1), 5: (1,−1,1), 6: (1,1,1), 7: (−1,1,1) | `element_class_hexahedron_8_inline_impl.hh:21-45` |
| `_hexahedron_20` | 0..7 as hex8; 8: (0,−1,−1), 9: (1,0,−1), 10: (0,1,−1), 11: (−1,0,−1), 12: (−1,−1,0), 13: (1,−1,0), 14: (1,1,0), 15: (−1,1,0), 16: (0,−1,1), 17: (1,0,1), 18: (0,1,1), 19: (−1,0,1) | `element_class_hexahedron_20_inline_impl.hh:21-65` |
| Cohesive types | Two copies of the facet's nodes: `[facet⁺ nodes..., facet⁻ nodes...]`, natural coords = facet's. | `geometrical_element_property.hh:246-352`, `cohesive_element.hh` |
| Structural | Geometry of `_segment_2` (beams) / `_triangle_3` (DKT). Dofs per node: beam2 (u, v, θ), beam3 (u, v, w, θx, θy, θz), DKT (6). | `element_class_structural.hh:58-80`, `element_class_bernoulli_beam_inline_impl.hh:34-75` |

### 2.3 Facet definitions (decoded local connectivity)

The tables `facet_connectivity_vect` in `src/fe_engine/geometrical_element_property.hh`
are read as **column-major** `nb_facets × nb_nodes_per_facet` matrices
(`element_class_tmpl.hh:47-75`): each visual source row lists the k-th node of
every facet. Decoded per facet (`facet f: local nodes`):

| Type (line) | Facet type(s) | Facets |
|---|---|---|
| `_point_1` (`:49`) | `_point_1` | 0:(0) |
| `_segment_2` (`:58`), `_segment_3` (`:67`) | `_point_1` | 0:(0) 1:(1) |
| `_triangle_3` (`:78`) | `_segment_2` | 0:(0,1) 1:(1,2) 2:(2,0) |
| `_triangle_6` (`:91`) | `_segment_3` | 0:(0,1,3) 1:(1,2,4) 2:(2,0,5) |
| `_quadrangle_4` (`:136`) | `_segment_2` | 0:(0,1) 1:(1,2) 2:(2,3) 3:(3,0) |
| `_quadrangle_8` (`:149`) | `_segment_3` | 0:(0,1,4) 1:(1,2,5) 2:(2,3,6) 3:(3,0,7) |
| `_tetrahedron_4` (`:105`) | `_triangle_3` | 0:(0,2,1) 1:(1,2,3) 2:(2,0,3) 3:(0,1,3) |
| `_tetrahedron_10` (`:119`) | `_triangle_6` | 0:(0,2,1,6,5,4) 1:(1,2,3,5,9,8) 2:(2,0,3,6,7,9) 3:(0,1,3,4,8,7) |
| `_hexahedron_8` (`:163`) | `_quadrangle_4` | 0:(0,3,2,1) 1:(0,1,5,4) 2:(1,2,6,5) 3:(2,3,7,6) 4:(3,0,4,7) 5:(4,5,6,7) |
| `_hexahedron_20` (`:178`) | `_quadrangle_8` | 0:(0,1,5,4,8,13,16,12) 1:(1,2,6,5,9,14,17,13) 2:(2,3,7,6,10,15,18,14) 3:(3,0,4,7,11,12,19,15) 4:(0,3,2,1,11,10,9,8) 5:(4,5,6,7,16,17,18,19) — **order differs from hex8** |
| `_pentahedron_6` (`:197`) | t0 `_triangle_3`, t1 `_quadrangle_4` | t0: 0:(0,2,1) 1:(3,4,5); t1: 0:(0,1,4,3) 1:(0,3,5,2) 2:(1,2,5,4) |
| `_pentahedron_15` (`:217`) | t0 `_triangle_6`, t1 `_quadrangle_8` | t0: 0:(0,2,1,8,7,6) 1:(3,4,5,12,13,14); t1: 0:(0,1,4,3,6,10,12,9) 1:(0,3,5,2,9,14,11,8) 2:(1,2,5,4,7,11,13,10) |
| `_cohesive_1d_2` (`:275`) | `_point_1` | 0:(0) 1:(1) |
| `_cohesive_2d_4` (`:246`) | `_segment_2` | 0:(0,1) 1:(2,3) |
| `_cohesive_2d_6` (`:260`) | `_segment_3` | 0:(0,1,2) 1:(3,4,5) |
| `_cohesive_3d_6` (`:287`) | `_triangle_3` | 0:(0,1,2) 1:(3,4,5) |
| `_cohesive_3d_12` (`:302`) | `_triangle_6` | 0:(0..5) 1:(6..11) |
| `_cohesive_3d_8` (`:320`) | `_quadrangle_4` | 0:(0,1,2,3) 1:(4,5,6,7) |
| `_cohesive_3d_16` (`:336`) | `_quadrangle_8` | 0:(0..7) 1:(8..15) |

Facets of volume elements are oriented with outward normals for positive-volume
elements (checked: tet4 facet 0 `(0,2,1)` → normal −z; hex8 facet 0 `(0,3,2,1)` → −z).
p1 types and facet types: `geometrical_element_property.hh:357-465` (e.g. tri6→p1 tri3, facet seg3).
Facet *element numbering* in a mesh_facets is defined by the construction
algorithm (MUTIL-004), not by these tables alone.

### 2.4 Shape functions, derivatives, quadrature

#### 2.4.1 Shape function formulas (as coded; `dnds(i,j) = ∂N_j/∂s_i`)

| Type | `N` | `dnds` | Reference |
|---|---|---|---|
| seg2 | `½(1−ξ), ½(1+ξ)` | `[−½, ½]` | `element_class_segment_2_inline_impl.hh:58-72` |
| seg3 | `(ξ−1)ξ/2, (ξ+1)ξ/2, 1−ξ²` | `[ξ−½, ξ+½, −2ξ]` | `element_class_segment_3_inline_impl.hh:67-82` |
| tri3 | `1−ξ−η, ξ, η` | `[[−1,1,0],[−1,0,1]]` | `element_class_triangle_3_inline_impl.hh:62-90` |
| tri6 | `c_i(2c_i−1)` (c0=1−ξ−η,c1=ξ,c2=η), `4c0c1, 4c1c2, 4c2c0` | row ξ: `1−4c0, 4c1−1, 0, 4(c0−c1), 4c2, −4c2`; row η: `1−4c0, 0, 4c2−1, −4c1, 4c1, 4(c0−c2)` | `element_class_triangle_6_inline_impl.hh:105-160` |
| quad4 | `¼(1∓ξ)(1∓η)` (node signs per 2.2) | see file | `element_class_quadrangle_4_inline_impl.hh:74-110` (+ `computeD2NDS2` `:113-126`) |
| quad8 | corners `¼(1±ξ)(1±η)(−1±ξ±η)`; mids `½(1−ξ²)(1∓η)`, `½(1±ξ)(1−η²)` | see file (`:117-134`) | `element_class_quadrangle_8_inline_impl.hh:97-134` |
| tet4 | `1−ξ−η−ζ, η, ζ, ξ` (**non-standard**) | col0 `(−1,−1,−1)`, col1 `(0,1,0)`, col2 `(0,0,1)`, col3 `(1,0,0)` | `element_class_tetrahedron_4_inline_impl.hh:65-120` |
| tet10 | `c0(1−2Σ), ξ(2ξ−1), η(2η−1), ζ(2ζ−1), 4ξc0, 4ξη, 4ηc0, 4ζc0, 4ξζ, 4ηζ` | see file (`:217-249`) | `element_class_tetrahedron_10_inline_impl.hh:145-249` |
| penta6 | `½y(1−x), ½z(1−x), ½(1−y−z)(1−x), ½y(1+x), ½z(1+x), ½(1−y−z)(1+x)` | see file | `element_class_pentahedron_6_inline_impl.hh:70-100` |
| penta15 | see file | see file | `element_class_pentahedron_15_inline_impl.hh:85-164` |
| hex8 | `⅛(1±ξ)(1±η)(1±ζ)` | see file | `element_class_hexahedron_8_inline_impl.hh:117-190` |
| hex20 | corners `⅛(1±ξ)(1±η)(1±ζ)(−2±ξ±η±ζ)`, mids `¼(1−ξ²)(1±η)(1±ζ)` etc. | see file (factored forms) | `element_class_hexahedron_20_inline_impl.hh:83-200` |
| Hermite (beams) | see ELEM-027 | see ELEM-027 | `element_class_hermite_inline_impl.hh:60-110` |

The exact floating-point expression forms (e.g. `.125 * (1 - c(0)) * ...`,
`1. / 4. * ...`, factored hex20 derivatives) must be copied verbatim for
bitwise reproducibility.

#### 2.4.2 Quadrature data and ordering

* Raw tables (point-by-point, `dim` coordinates per point): `src/fe_engine/gauss_integration.cc:33-215` (see QUAD-002..018 for the values).
* Order → number of points: `gauss_integration_tmpl.hh:31-80` (segment `(n+1)/2 + ((n+1)%2?1:0)` at `:47`).
* Quad/hex: tensor product, **first coordinate fastest**, weights `((1·w_x)·w_y)·w_z` (`gauss_integration_tmpl.hh:140-170`, index formula `rq = q % tot + q / tot` at `:151,173`). E.g. hex8 order: (−,−,−), (+,−,−), (−,+,−), (+,+,−), (−,−,+), (+,−,+), (−,+,+), (+,+,+) with ±=±1/√3.
* Prism: outer loop segment points, inner loop triangle points (`gauss_integration_tmpl.hh:172-230`).
* Pentahedron explicit tables (`_git_pentahedron` 6/8/21) are **unused**.

#### 2.4.3 Default and mass quadrature per element type

| Type | polynomial degree (default order) | default rule (points) | mass order (2×deg) → points |
|---|---|---|---|
| `_point_1` | 1 | 1 | 2 → 1 (point rule ignores order) |
| `_segment_2` | 1 | 1 (ξ=0, w=2) | 2 → 2 |
| `_segment_3` | 2 | 2 | 4 → 3 |
| `_triangle_3` | 1 | 1 | 2 → 3 |
| `_triangle_6` | 2 | 3 | 4 → 6 |
| `_quadrangle_4` | 2 | 2×2 = 4 | 4 → 3×3 = 9 |
| `_quadrangle_8` | 3 | 2×2 = 4 | 6 → 4×4 = 16 |
| `_tetrahedron_4` | 1 | 1 | 2 → 4 |
| `_tetrahedron_10` | 2 | 4 | 4 → 15 |
| `_pentahedron_6` | 1 | 2×3 = 6 | 2 → 6 |
| `_pentahedron_15` | 2 | 2×3 = 6 | 4 → 3×7 = 21 |
| `_hexahedron_8` | 2 | 2×2×2 = 8 | 4 → 27 |
| `_hexahedron_20` | 3 | 2×2×2 = 8 | 6 → 64 |
| `_cohesive_1d_2` | 1 | 1 | — (mass not implemented for cohesive) |
| `_cohesive_2d_4` | 2 | 2 | — |
| `_cohesive_2d_6` | 3 | 2 | — |
| `_cohesive_3d_6` | 2 | 3 | — |
| `_cohesive_3d_12` | 3 | 4 (negative weight) | — |
| `_cohesive_3d_8` | 2 | 4 | — |
| `_cohesive_3d_16` | 3 | 4 | — |
| `_bernoulli_beam_2/3` | 3 | 2 | 6 → 4 |
| `_discrete_kirchhoff_triangle_18` | 2 | 3 | 4 → 6 |

Sources: `ElementClassProperty` macros (`element_classes/*_inline_impl.hh`, `cohesive_element.hh:31-51`, `element_class_structural.hh:66-80`), `fe_engine_template_tmpl_field.hh:349-352`.

### 2.5 Jacobian and derivative conventions

* `J = dnds · Xᵀ` with `X` = `(spatial_dim × nnodes)` nodal coordinates (column per node) → `J(i,j) = ∂x_j/∂s_i` (`element_class_tmpl.hh:297-302`).
* Square `J`: jacobian = **signed** `det(J)` (`:354`); natural dim 1 embedded: `‖J‖` (`:358`); natural dim 2 in 3D: `‖J_row0 × J_row1‖` (`:363`); else 0.
* `dN/dx = J⁻¹ · dnds` (`:379`, `:384-396`), computed only if natural dim == mesh spatial dim.
* Integrator stores `det(J)·w`; any **raw** `det(J) < 0` → `IntegratorGaussException` (`integrator_gauss_inline_impl.hh:178-195`).
* Cohesive: Jacobian of the mid-surface `(X_n + X_{n+half})/2`; `_cohesive_1d_2` → 1 (`integrator_gauss_inline_impl.hh:318-360`); cohesive shape derivatives are natural (`dN/ds`).
* Inverse map: Gauss–Newton from `ξ=0`, absolute tolerance `1e-10` on `‖x − x(ξ)‖`, max 100 iterations, exception otherwise (`element_class_tmpl.hh:469-552`).
* Normals: 2D `(t1, −t0)/‖t‖` from `t = ∂x/∂s` (`Math::normal`), 3D `normalize(t0 × t1)` (`element_class_tmpl.hh:398-420`); cohesive 2D uses `(−t1, t0)` (opposite sign convention!) (`shape_cohesive_inline_impl.hh:276-292`).

### 2.6 Voigt notation

| dim | size | vector index → (i,j) | `mat[i][j]` (incl. non-symmetric extension) | strain factors |
|---|---|---|---|---|
| 1 | 1 | 0:(0,0) | `[[0]]` | `[1]` |
| 2 | 3 | 0:(0,0) 1:(1,1) 2:(0,1) [3:(1,0)] | `[[0,2],[3,1]]` | `[1,1,2]` |
| 3 | 6 | 0:(0,0) 1:(1,1) 2:(2,2) 3:(1,2) 4:(0,2) 5:(0,1) [6:(2,1) 7:(2,0) 8:(1,0)] | `[[0,5,4],[8,1,3],[7,6,2]]` | `[1,1,1,2,2,2]` |

Reference `src/common/aka_voigthelper.hh:42-71`. `matrixToVoigt` copies
`M(i,j)`; `matrixToVoigtWithFactors` multiplies by factors (engineering shear
strains); `voigtToMatrix` fills both `(i,j)` and `(j,i)`. Symmetric Voigt B
(`aka_voigthelper_tmpl.hh:82-120`): rows `xx, yy, (zz)`, 2D row 2 = `[∂N/∂y, ∂N/∂x]`
(cols `2n`, `2n+1`), 3D row 3 (yz) = `[0, ∂N/∂z, ∂N/∂y]`, row 4 (xz) =
`[∂N/∂z, 0, ∂N/∂x]`, row 5 (xy) = `[∂N/∂y, ∂N/∂x, 0]`.

### 2.7 Gmsh `.msh` conventions

* Supported: MSH 2.x ASCII and 4.0/4.1 ASCII (`mesh_io_msh.cc:692-725`); v1 effectively unsupported (IO-006); binary rejected.
* Element type codes: IO-003 (`mesh_io_msh.cc:145-215`).
* Node reordering Gmsh→Akantu (`local[read_order[j]] = gmsh_node_j`): tet10 swap 8↔9; penta6 `[2,0,1,5,3,4]`; penta15 `[2,0,1,5,3,4,8,7,11,6,9,10,14,13,12]`; hex20 `[0..8, 11, 12, 9, 13, 10, 14, 15, 16, 19, 17, 18]` (`mesh_io_msh.cc:216-262`).
* Node ids: file tags mapped to consecutive indices in file order (`node_tags`); element ids to `Element{type, index}` (`element_tags`).
* v2 tags: `n` integer tags per element → `Int` mesh data `"tag_0".."tag_{n-1}"` (Gmsh: tag_0 = physical, tag_1 = elementary) (`mesh_io_msh.cc:390-412`).
* Physical names (v2): `physical_names[(dim, tag)]` from `$PhysicalNames`; per element `"physical_names"` = name of `(spatial dim of element type, tag_0)` or the decimal tag; element groups are created from `"physical_names"` **only if `$PhysicalNames` existed** (`mesh_io.cc:89-112`, `mesh_io_msh.cc:913-917`). Group dimension = max dim of members; node group `<name>_nodes`.
* Physical names (v4): groups created in `$Entities` (named or `msh_block_<tag>`), elements added to named groups only; `"tag_0"` = first physical tag; `"physical_names"` = first named physical (`mesh_io_msh.cc:483-540, 600-680`).
* After any Gmsh read: `MeshUtils::fillElementToSubElementsData` (element↔facet adjacency for elements of consecutive dimensions present in the file) (`mesh_io_msh.cc:919`).
* Writer: v2.2, `digits10` (15) precision, uppercase exponent, 1-based ids, only not_ghost elements, tags `tag_0 tag_1` or 0 (`mesh_io_msh.cc:923-1050`).

### 2.8 Mesh data / naming conventions

| Name | Type | Producer | Meaning |
|---|---|---|---|
| `tag_0`, `tag_1`, … | `Int` (elemental) | Gmsh reader | Gmsh element tags (physical, elementary, partitions…) |
| `physical_names` | `std::string` (elemental) | Gmsh reader, `initMeshFacets` | physical group name per element |
| `element_to_subelement` | `std::vector<Element>` (elemental, 1 comp) | `buildFacets*`, `fillElementToSubElementsData`, inserter | adjacent higher-dim elements of a facet |
| `subelement_to_element` | `Element` (elemental, `nb_facets` comps) | idem | facets of an element (creation order) |
| `mesh_to_mesh_facet` | `Element` | `initMeshFacets` | parent (d−1)-element → facet in mesh_facets |
| `extra_normal` | `Real` (elemental, dim comps) | user / file | beam-3D orientation normal |
| `material` | `Int` | Diana reader | material id |
| `associated_element`, `physical_names` (on segments) | `Element`, string | `MeshSegmentIntersector` | embedded segments provenance |
| `tag_1` | `Int` | `SpatialGrid::saveAsMesh` | cell id |
| Arrays ids | — | `ElementTypeMapArray` | `<parent_id>:<id>:<type>[:ghost]`; mesh coordinates `<mesh_id>:coordinates`, flags `<mesh_id>:nodes_flags` |

### 2.9 Iteration-order rules (observable in outputs)

1. Ghost types: `_not_ghost` then `_ghost` (`aka_common.hh:545-555`).
2. Element types: `std::map` key order = **enum declaration order** (regular, cohesive, structural, igfem) (`element_type_map.hh:150`, `aka_element_classes_info.hh:36-95`).
3. Groups, mesh data, dumpers, parameters, parser parameters: `std::map` by name (lexicographic).
4. Parser sub-sections: by `ParserType` enum order, then insertion order (multimap).
5. Event handlers: ascending `EventHandlerPriority`; equal priority → later registration first (`aka_event_handler_manager.hh:62-76`).
6. `Element` ordering: `(ghost_type, type, element)`; `ElementNull` is the maximum (`element.hh:66-71`).
7. Facet creation: MUTIL-004; element removal renumbering: MESH-013; node purification: MUTIL-008; cohesive insertion: MUTIL-016.
8. `std::unordered_map` users (periodic maps, `nodes_prank`, MSH tag maps) — iteration order is implementation-defined; any output depending on it must be identified (none found in core outputs).

### 2.10 Defaults and magic constants

| Item | Value | Reference |
|---|---|---|
| `Math::tolerance` | `1e2 * DBL_EPSILON` (≈2.22e-14), global, settable | `aka_extern.cc:71`, `aka_math.hh:35,104-106` |
| Float equality | `abs(x−y) ≤ tol·max(1, abs(x), abs(y))` | `aka_math_tmpl.hh:140-145` |
| Inverse map | max 100 iterations, tol 1e-10 (absolute) | `element_class.hh:405-425` |
| Containment eps | `DBL_EPSILON` (square, point), `Math::tolerance` (simplex), 0 (prism) | `element_class_tmpl.hh:116-168` |
| Periodic matching tolerance | 1e-10 relative to box length | `mesh_periodic.cc:46,125` |
| Legacy PBC tolerance | 1e-7 | `mesh_utils_pbc.cc:210` |
| `initMeshFacets` barycenter match | `Math::tolerance · max(1, ‖bary‖)` | `mesh.cc:295-302` |
| Cohesive insertion bbox | `insertion_limits` default `(−DBL_MAX, DBL_MAX)`, open interval ± `Math::tolerance` | `cohesive_element_inserter.cc:52-58,145-160` |
| Sphere intersector node tolerance | 1e-10 | `mesh_sphere_intersector_tmpl.hh:35` |
| RNG default seed | 5489; runtime seed `AKA_SEED` env > `--aka_seed` > input `seed` > `time(nullptr)`, × (rank+1) | `aka_extern.cc:80-85`, `aka_common.cc:104-115` |
| Debug level | `dblWarning` (4) default via CLI; env `AKA_DEBUG_LEVEL` overrides | `aka_common.cc:76-83,119-125` |
| `AKANTU_MIN_ALLOCATION` | 2000 tuples | `aka_common.hh:561` |
| `REAL_INIT_VALUE` | NaN (debug) / 0 (release) | `aka_common.hh:58-63` |
| ParaView dumper | directory `./paraview`, VTU subdir `<name>-VTU`, 4-digit step, BASE64 | `dumper_iohelper_paraview.hh:36-38`, iohelper `dumper.cc:48,215-235` |
| Text dumper | basename `dumper_text`, space mode, precision 6, data subdir `<name>-DataFiles` | `dumper_text.hh:37-39`, iohelper `dumper_text.cc:49` |
| Anonymous parser sections | `anonymous_<n>`, n from 12 (global counter) | `input_file_parser.hh:70-80` |
| Lumped-mass diagonal factors | FE-030 | `fe_engine_template_tmpl_field.hh:160-205` |

### 2.11 Numerical routines to replicate exactly

* `Math::reduce` (sorted pairwise sum, mutating) — scalar integrals (`aka_math.cc:74-100`).
* Tensor-product weight products, triangle/tet rule constants computed with `std::sqrt` at static-init (`gauss_integration.cc:75-150`).
* `Math::tetrahedron_volume` 4-determinant formula, inradius formulas (`aka_math_tmpl.hh:50-120`).
* `J.inverse()` / `J.determinant()` as computed by Eigen closed forms for 1–3×3 (port must use the same cofactor formulas: Eigen `determinant()` for 3×3 = `m(0,0)*(m(1,1)m(2,2)−m(1,2)m(2,1)) − ...` via `bruteforce_det3_helper`; inverse via cofactors / determinant).
* Lumped mass: row-sum via `computeNtb` + integration; diagonal scaling via scalar integral × factor.
* Eigen-decomposition (`eig`/`eigh`) — not bit-reproducible across libraries; use tolerances.
* RNG streams (CORE-031/032) — libstdc++-specific.

---

## 3. Public API surface (user code & Python bindings)

Classes/functions in this scope that user code, examples and the Python
bindings (`python/py_*.cc`, see R4) use directly. Names are C++ names; Python
exposes most of them with the same spelling.

**Library lifecycle & globals** — `akantu::initialize(argc, argv)`, `initialize(input_file[, argc, argv])`, `finalize()`, `readInputFile(file)`, `getStaticParser()`, `getUserParser()`, `getStaticArgumentParser()`, `getVersion()`; `debug::setDebugLevel`, `debug::getDebugLevel`, `debug::Exception` (Python: `akantu.Exception`), `Math::setTolerance/getTolerance`, `RandomGenerator<Idx>::seed()`; Python `__initialize`, `getElementTypes`, `getVersion`.

**Enums** — `ElementType`, `ElementKind`, `GhostType` (+`ghost_types`), `SpatialDirection` (`_x,_y,_z`), `MeshIOType`, `NodeFlag`, `AnalysisMethod`, `ModelType`, `NonLinearSolverType`, `SparseSolverType`, `TimeStepSolverType`, `IntegrationSchemeType`, `SolveConvergenceCriteria`, `CohesiveMethod`, `MatrixType`, `SynchronizationTag`, `ParserType`, `RandomDistributionType`, `MeshDataTypeCode`, `ParameterAccessType`, `EventHandlerPriority`, `InterpolationType`, `DebugLevel`.

**Containers & small types** — `Array<T>` (ctor `(size, nb_component, [value], id)`, `size`, `getNbComponent`, `operator()`, `operator[]`, `push_back`, `resize`, `reserve`, `set`, `zero`, `clear`, `copy`, `append`, `erase`, `find`, `begin/end(n...)`, `data`, `getID`), `ArrayProxy<T>` (numpy views in Python), `make_view`, `make_const_view`, `Vector<T,n>`, `Matrix<T,m,n>`, `VectorProxy`, `MatrixProxy`, `Tensor3`, `VoigtHelper<dim>`, `BBox`, `CSR<T>`, `SpatialGrid<T>`, `Element` / `ElementNull`, `IntegrationPoint`, `ElementTypeMap<T>`, `ElementTypeMapArray<T>` (`alloc`, `operator()(type, ghost)`, `operator()(Element, comp)`, `initialize(mesh|fe_engine, _nb_component=…, …)`, `elementTypes(...)`, `exists`, `zero`, `set`), `RandomParameter<Real>`, `make_random_parameter`, iterator helpers `zip`, `enumerate`, `arange`, `filter`, `make_transform_adaptor`, `make_keys_adaptor`, `make_values_adaptor`, `for_each_element`.

**Mesh** — `Mesh(dim[, communicator], id="mesh")`, `read(file, MeshIOType=_miot_auto)`, `write(file, type)`, `getID`, `getSpatialDimension`, `getNodes`, `getNbNodes`, `getConnectivity(type, ghost)`, `getConnectivities`, `getConnectivity(element)`, `getNbElement(type|dim, ghost, kind)`, `elementTypes(...)`, `getLowerBounds/getUpperBounds/getBBox` (+ local), `getBarycenter(element[, out])`, `getBarycenters`, `getNodeFlag`, `isPureGhostNode`, `isLocalOrMasterNode`, `isLocalNode`, `isMasterNode`, `isSlaveNode`, `getNodeGlobalId/getNodeLocalId/getNbGlobalNodes`, `getGlobalNodesIds`, `getNodesFlags`, `getData<T>(name[, type, ghost])`, `getDataPointer<T>`, `hasData`, `getTagNames`, `getElementalData<T>`, `getNodalData<T>`, `initMeshFacets`, `getMeshFacets`, `hasMeshFacets`, `getMeshParent`, `isMeshFacets`, `getElementToSubelement(...)`, `getSubelementToElement(...)`, `getFacetConnectivity(element, t)`, static `getNbNodesPerElement`, `getP1ElementType`, `getKind`, `getSpatialDimension(type)`, `getNaturalSpaceDimension`, `getNbFacetsPerElement`, `getFacetType`, `getAllFacetTypes`, `getFacetLocalConnectivity`, `distribute(_communicator=…)`, `distribute(partition)`, `isDistributed`, `makePeriodic(direction[, group1, group2])`, `isPeriodic`, `getPeriodicMaster`, `getPeriodicSlaves`, `isPeriodicSlave/Master`, `makeConsistentForPeriodicity`, `eraseElements`, `fillNodesToElements`, `getAssociatedElements`, `extractNodalValuesFromElement`, `registerEventHandler/unregisterEventHandler`, `getElementSynchronizer/getNodeSynchronizer/getCommunicator`, `makeReady` (Python), `getGroupDumper`, `createFieldFromAttachedData`. `MeshAccessor` (Python exposes it), `MeshEventHandler` callbacks, `NewNodesEvent`/`NewElementsEvent`/`RemovedNodesEvent`/`RemovedElementsEvent`.

**Groups** — `GroupManager::createNodeGroup`, `createElementGroup`, `getElementGroup`, `getNodeGroup`, `elementGroupExists`, `nodeGroupExists`, `iterateElementGroups`, `iterateNodeGroups`, `createGroupsFromMeshData<std::string|UInt>`, `createElementGroupFromNodeGroup`, `createBoundaryGroupFromGeometry`, `createClusters`, `ClusteringFilter`, `renameElementGroup`, `copyElementGroup`, `destroyElementGroup`; `ElementGroup::add`, `addNode`, `getElements`, `getNodeGroup`, `getNodes`(via node group), `getDimension`, `append`, `optimize`, `fillFromNodeGroup`, `clear`, `empty`, `getName`, dumper methods; `NodeGroup::add`, `remove`, `getNodes`, `size`, `optimize`, `append`, `getName`, `clear`, `empty`.

**Mesh utilities** — `MeshUtils::buildNode2Elements`, `buildFacets`, `buildAllFacets`, `purifyMesh`, `fillElementToSubElementsData`, `flipFacets`, `computePBCMap` (legacy); `MeshPartitionScotch`, `MeshPartitionMeshData`, `MeshPartition::partitionate/getPartitions`; `CohesiveElementInserter` (`setLimit`, `insertIntrinsicElements`, `insertElements`, `addPhysicalSurface`, `addPhysicalVolume`, `getCheckFacets`, `getInsertionFacets`, `getMeshFacets`, `limitCheckFacets`); `BoostGraphConverter::toGraphviz` (debug).

**FE engine** — `FEEngine` (`integrate(...)`, `interpolateOnIntegrationPoints`, `gradientOnIntegrationPoints`, `computeBtD`, `computeBtDB`, `computeNtb`, `computeNtbN`, `computeIntegrationPointsCoordinates`, `getNbIntegrationPoints`, `getShapes`, `getShapesDerivatives`, `getIntegrationPoints`, `computeNormalsOnIntegrationPoints`, `getNormalsOnIntegrationPoints`, `getElementInradius`, `interpolate(real_coords, nodal_values, out, element)`, `computeShapes(real_coords, elem, type, out)`, `computeShapeDerivatives`, `assembleFieldLumped`, `assembleFieldMatrix`, `initShapeFunctions`, `elementTypes`, `getMesh`, `getElementDimension`, static `extractNodalToElementField`, `filterElementalData`, `getCohesiveElementType`, `getInterpolationType`); `FEEngineTemplate<IntegratorGauss, ShapeLagrange[, kind]>` (+ `inverseMap`, `contains`, `getIntegrator`, `getShapeFunctions`); `IntegratorGauss` (`getIntegrationPoints<type,n>`, `getIntegrationWeights<type,n>`, `integrate<type,n>`, `getJacobians`); `ShapeLagrange` (`computeShapesOnIntegrationPoints`, `interpolateOnIntegrationPoints(…, shapes)`); `ElementClass<type>` static API (`computeShapes`, `computeDNDS`, `computeJMat`, `computeJacobian`, `computeShapeDerivatives`, `inverseMap`, `contains`, `getNbNodesPerElement`, `getShapeSize`, `getSpatialDimension`, `getNaturalSpaceDimension`, `getFacetType(s)`, `getP1ElementType`, `getInradius`); `GaussIntegrationElement<type,n>` (`getQuadraturePoints`, `getWeights`, `getNbQuadraturePoints`); `ElementClassHelper<_ek_regular>` (`getN`, `getDNDS`, `getD2NDS2`, `getJMat`).

**Dumpers** — `Dumpable` (`registerDumper<DumperParaview|DumperText>`, `addDumpMesh`, `addDumpFilteredMesh`, `addDumpField[ToDumper]`, `addDumpFieldExternal[ToDumper]`, `addDumpFieldVector/Tensor`, `removeDumpField`, `setDirectory`, `setBaseName`, `setTimeStepToDumper`, `setTextModeToDumper`, `dump(...)`, `getDumper`), `DumperParaview(name, dir="./paraview", parallel)`, `DumperText(name, mode, parallel)` + `setPrecision`, `DumperIOHelper` (`registerField`, `registerMesh`, `registerFilteredMesh`, `dump`, `setTimeStep`), `dumpers::NodalField`, `ElementalField`, `InternalMaterialField`, `FieldComputeProxy`, `ComputeFunctor`, `HomogenizerProxy`; Python: `registerDumperParaview`, `addDumpMesh[ToDumper]`, `addDumpField[ToDumper]`, `dump`.

**Parser / parameters** — `Parser::parse`, `ParserSection` (`getSubSections(type)`, `getParameter(name[, default, ctx])`, `hasParameter`, `getParameterValue<T>`, `getName`, `getType`, `getOption`, `getParameters`), `ParserParameter` (`getName`, `getValue`, conversions), `Parser::parseReal/parseVector/parseMatrix/parseRandomParameter`, `Parser::setPermissive`, `ParameterRegistry` (`registerParam`, `get`, `set`, `listParameters`, `hasParameter`, `printself`), `Parsable` (`parseSection`, `registerSubSection`), `cppargparse::ArgumentParser` (`addArgument`, `parse`, `operator[]`, `has`, `print_help`); Python `getReal/getBool/getString/getInt`, `setReal/...`, `registerParamReal`.

**Geometry (CGAL builds only)** — `MeshSegmentIntersector<dim,type>`, `MeshSphereIntersector<dim,type>`, `MeshGeomFactory`, `cgal::Cartesian`, `cgal::Spherical`, `compareSegments`, `comparePoints` (used by embedded model / IGFEM).

---

## 4. Appendix: file coverage

Every `.hh`/`.cc` (plus `.h`, `.c`, `.cpp`, `.tcc`, `.in`) file under
`src/common`, `src/fe_engine`, `src/geometry`, `src/mesh`, `src/mesh_utils`,
`src/io`, `third-party/akantu_iterators`, `third-party/iohelper`,
`test/test_common`, `test/test_fe_engine`, `test/test_geometry`,
`test/test_io`, `test/test_mesh_utils` (320 files; generated by a script that
aborts on any unmapped file), plus the two shared test helpers in `test/`.

| File | Kind | Feature IDs (or reason) |
|---|---|---|
| `test/test_gtest_main.cc` | test | test glue (gtest main shared by all gtest suites) |
| `test/test_gtest_utils.hh` | test | test glue (typed-test type lists `TestElementTypes` (regular types minus `_point_1`), cross products, `Polynomial<n>` used by FE-014 test) |
| `src/common/aka_array.cc` | source | CORE-013 |
| `src/common/aka_array.hh` | source | CORE-012, CORE-013, CORE-014 |
| `src/common/aka_array_filter.hh` | source | CORE-016 |
| `src/common/aka_array_printer.hh` | source | CORE-017 |
| `src/common/aka_array_tmpl.hh` | source | CORE-012, CORE-013, CORE-015 |
| `src/common/aka_array_view.hh` | source | CORE-015 |
| `src/common/aka_bbox.hh` | source | CORE-028 |
| `src/common/aka_circular_array.hh` | source | CORE-037 (unused) |
| `src/common/aka_circular_array_inline_impl.hh` | source | CORE-037 (unused) |
| `src/common/aka_common.cc` | source | CORE-038, CORE-009, CORE-012 (PARSE-012) |
| `src/common/aka_common.hh` | source | CORE-003, CORE-007, CORE-008, CORE-009, CORE-038, CORE-039, CORE-040 |
| `src/common/aka_common_inline_impl.hh` | source | CORE-008, CORE-039 |
| `src/common/aka_config.cc.in` | config template | CORE-002 |
| `src/common/aka_config.hh.in` | config template | CORE-001, CORE-002 (build glue: feature macros) |
| `src/common/aka_constexpr_map.hh` | source | CORE-006 |
| `src/common/aka_csr.hh` | source | CORE-029 |
| `src/common/aka_element_classes_info.hh` | source | CORE-004, CORE-005 |
| `src/common/aka_element_classes_info_inline_impl.hh` | source | CORE-004, CORE-006 |
| `src/common/aka_enum_macros.hh` | source | CORE-010 |
| `src/common/aka_error.cc` | source | CORE-033 |
| `src/common/aka_error.hh` | source | CORE-033 |
| `src/common/aka_event_handler_manager.hh` | source | CORE-036 |
| `src/common/aka_extern.cc` | source | CORE-041, CORE-024, CORE-031 |
| `src/common/aka_factory.hh` | source | CORE-035 |
| `src/common/aka_fwd.hh` | source | CORE-043 (build glue: forward declarations) |
| `src/common/aka_grid_dynamic.hh` | source | CORE-030, MESH-035 |
| `src/common/aka_math.cc` | source | CORE-025, CORE-026 |
| `src/common/aka_math.hh` | source | CORE-023, CORE-024, CORE-025, CORE-026, CORE-027 |
| `src/common/aka_math_tmpl.hh` | source | CORE-023, CORE-024, CORE-027 |
| `src/common/aka_named_argument.hh` | source | CORE-034 |
| `src/common/aka_random_generator.hh` | source | CORE-031, CORE-032 |
| `src/common/aka_safe_enum.hh` | source | CORE-011 |
| `src/common/aka_tensor.hh` | source | CORE-020 |
| `src/common/aka_types.hh` | source | CORE-018, CORE-019, CORE-015 |
| `src/common/aka_types_eigen_matrix_base_plugin.hh` | source | CORE-018 |
| `src/common/aka_types_eigen_matrix_plugin.hh` | source | CORE-018 |
| `src/common/aka_types_eigen_plain_object_base_plugin.hh` | source | CORE-018 |
| `src/common/aka_view_iterators.hh` | source | CORE-015 |
| `src/common/aka_voigthelper.hh` | source | CORE-021, CORE-022 |
| `src/common/aka_voigthelper_tmpl.hh` | source | CORE-021, CORE-022 |
| `src/common/aka_warning.hh` | source | CORE-042 (build glue: diagnostic pragmas) |
| `src/common/aka_warning_restore.hh` | source | CORE-042 (build glue: diagnostic pragmas) |
| `src/fe_engine/cohesive_element.hh` | source | ELEM-014..ELEM-020, FE-037 |
| `src/fe_engine/element.hh` | source | FE-001 |
| `src/fe_engine/element_class.hh` | source | FE-003, FE-004..FE-009, QUAD-019 |
| `src/fe_engine/element_class_helper.hh` | source | FE-011 |
| `src/fe_engine/element_class_structural.hh` | source | ELEM-021, ELEM-022, ELEM-023, FE-016 |
| `src/fe_engine/element_class_tmpl.hh` | source | FE-003, FE-004, FE-005, FE-006, FE-007, FE-008, FE-009 |
| `src/fe_engine/element_classes/element_class_bernoulli_beam_inline_impl.hh` | source | ELEM-021, ELEM-022 |
| `src/fe_engine/element_classes/element_class_hermite_inline_impl.hh` | source | ELEM-027 |
| `src/fe_engine/element_classes/element_class_hexahedron_20_inline_impl.hh` | source | ELEM-013 |
| `src/fe_engine/element_classes/element_class_hexahedron_8_inline_impl.hh` | source | ELEM-012 |
| `src/fe_engine/element_classes/element_class_kirchhoff_shell_inline_impl.hh` | source | ELEM-023 |
| `src/fe_engine/element_classes/element_class_pentahedron_15_inline_impl.hh` | source | ELEM-011 |
| `src/fe_engine/element_classes/element_class_pentahedron_6_inline_impl.hh` | source | ELEM-010 |
| `src/fe_engine/element_classes/element_class_point_1_inline_impl.hh` | source | ELEM-001 |
| `src/fe_engine/element_classes/element_class_quadrangle_4_inline_impl.hh` | source | ELEM-006 |
| `src/fe_engine/element_classes/element_class_quadrangle_8_inline_impl.hh` | source | ELEM-007 |
| `src/fe_engine/element_classes/element_class_segment_2_inline_impl.hh` | source | ELEM-002 |
| `src/fe_engine/element_classes/element_class_segment_3_inline_impl.hh` | source | ELEM-003 |
| `src/fe_engine/element_classes/element_class_tetrahedron_10_inline_impl.hh` | source | ELEM-009 |
| `src/fe_engine/element_classes/element_class_tetrahedron_4_inline_impl.hh` | source | ELEM-008 |
| `src/fe_engine/element_classes/element_class_triangle_3_inline_impl.hh` | source | ELEM-004 |
| `src/fe_engine/element_classes/element_class_triangle_6_inline_impl.hh` | source | ELEM-005 |
| `src/fe_engine/element_type_conversion.hh` | source | FE-012 |
| `src/fe_engine/fe_engine.cc` | source | FE-027 |
| `src/fe_engine/fe_engine.hh` | source | FE-027 |
| `src/fe_engine/fe_engine_inline_impl.hh` | source | FE-010, FE-012, FE-026, FE-037, FE-038 |
| `src/fe_engine/fe_engine_template.hh` | source | FE-027, FE-039 |
| `src/fe_engine/fe_engine_template_cohesive.cc` | source | FE-034 |
| `src/fe_engine/fe_engine_template_tmpl.hh` | source | FE-019, FE-020, FE-024, FE-025, FE-027, FE-028, FE-029, FE-032 |
| `src/fe_engine/fe_engine_template_tmpl_field.hh` | source | FE-030, FE-031 |
| `src/fe_engine/fe_engine_template_tmpl_struct.hh` | source | FE-036 |
| `src/fe_engine/gauss_integration.cc` | source | QUAD-002..QUAD-018 |
| `src/fe_engine/gauss_integration_tmpl.hh` | source | QUAD-001, QUAD-007, QUAD-017, QUAD-019 |
| `src/fe_engine/geometrical_element_property.cc` | source | FE-003 (facets of all ELEM rows) |
| `src/fe_engine/geometrical_element_property.hh` | source | FE-003 (facets of all ELEM rows) |
| `src/fe_engine/integration_point.hh` | source | FE-002 |
| `src/fe_engine/integrator.hh` | source | FE-013 |
| `src/fe_engine/integrator_gauss.hh` | source | FE-013, FE-014, FE-015, FE-016 |
| `src/fe_engine/integrator_gauss_inline_impl.hh` | source | FE-013, FE-014, FE-015, FE-016 |
| `src/fe_engine/interpolation_element_tmpl.hh` | source | FE-003, FE-005 |
| `src/fe_engine/shape_cohesive.hh` | source | FE-033 |
| `src/fe_engine/shape_cohesive_inline_impl.hh` | source | FE-033 |
| `src/fe_engine/shape_functions.cc` | source | FE-017, FE-019, FE-020, FE-025 |
| `src/fe_engine/shape_functions.hh` | source | FE-017, FE-019, FE-020, FE-025 |
| `src/fe_engine/shape_functions_inline_impl.hh` | source | FE-017, FE-019, FE-020, FE-025 |
| `src/fe_engine/shape_lagrange.hh` | source | FE-018..FE-024 |
| `src/fe_engine/shape_lagrange_base.cc` | source | FE-018 |
| `src/fe_engine/shape_lagrange_base.hh` | source | FE-018 |
| `src/fe_engine/shape_lagrange_base_inline_impl.hh` | source | FE-018 |
| `src/fe_engine/shape_lagrange_inline_impl.hh` | source | FE-018, FE-019, FE-020, FE-021, FE-022, FE-023, FE-024 |
| `src/fe_engine/shape_structural.cc` | source | FE-035 |
| `src/fe_engine/shape_structural.hh` | source | FE-035 |
| `src/fe_engine/shape_structural_inline_impl.hh` | source | FE-035 |
| `src/geometry/aabb_primitives/aabb_primitive.cc` | source | GEOM-002 |
| `src/geometry/aabb_primitives/aabb_primitive.hh` | source | GEOM-002 |
| `src/geometry/aabb_primitives/line_arc.hh` | source | GEOM-002 |
| `src/geometry/aabb_primitives/tetrahedron.hh` | source | GEOM-002 |
| `src/geometry/aabb_primitives/triangle.hh` | source | GEOM-002 |
| `src/geometry/geom_helper_functions.hh` | source | GEOM-009 |
| `src/geometry/mesh_abstract_intersector.hh` | source | GEOM-005 |
| `src/geometry/mesh_abstract_intersector_tmpl.hh` | source | GEOM-005 |
| `src/geometry/mesh_geom_abstract.hh` | source | GEOM-004 |
| `src/geometry/mesh_geom_common.hh` | source | GEOM-001 |
| `src/geometry/mesh_geom_factory.hh` | source | GEOM-004 |
| `src/geometry/mesh_geom_factory_tmpl.hh` | source | GEOM-004 |
| `src/geometry/mesh_geom_intersector.hh` | source | GEOM-006 |
| `src/geometry/mesh_geom_intersector_tmpl.hh` | source | GEOM-006 |
| `src/geometry/mesh_segment_intersector.hh` | source | GEOM-007 |
| `src/geometry/mesh_segment_intersector_tmpl.hh` | source | GEOM-007 |
| `src/geometry/mesh_sphere_intersector.hh` | source | GEOM-008 |
| `src/geometry/mesh_sphere_intersector_tmpl.hh` | source | GEOM-008 |
| `src/geometry/tree_type_helper.hh` | source | GEOM-003 |
| `src/io/dumper/dumpable.cc` | source | DUMP-001 |
| `src/io/dumper/dumpable.hh` | source | DUMP-001 |
| `src/io/dumper/dumpable_dummy.hh` | source | DUMP-001 (no-op variant, never included) |
| `src/io/dumper/dumpable_inline_impl.hh` | source | DUMP-001 |
| `src/io/dumper/dumpable_iohelper.hh` | source | DUMP-001 |
| `src/io/dumper/dumper_compute.hh` | source | DUMP-012 |
| `src/io/dumper/dumper_element_iterator.hh` | source | DUMP-008 |
| `src/io/dumper/dumper_element_partition.hh` | source | DUMP-011 |
| `src/io/dumper/dumper_elemental_field.hh` | source | DUMP-008 |
| `src/io/dumper/dumper_field.hh` | source | DUMP-006 |
| `src/io/dumper/dumper_filtered_connectivity.hh` | source | DUMP-010 |
| `src/io/dumper/dumper_generic_elemental_field.hh` | source | DUMP-008 |
| `src/io/dumper/dumper_generic_elemental_field_tmpl.hh` | source | DUMP-008 |
| `src/io/dumper/dumper_homogenizing_field.hh` | source | DUMP-012 |
| `src/io/dumper/dumper_internal_material_field.hh` | source | DUMP-009 |
| `src/io/dumper/dumper_iohelper.cc` | source | DUMP-002, DUMP-003 |
| `src/io/dumper/dumper_iohelper.hh` | source | DUMP-002, DUMP-003 |
| `src/io/dumper/dumper_iohelper_paraview.cc` | source | DUMP-004 |
| `src/io/dumper/dumper_iohelper_paraview.hh` | source | DUMP-004 |
| `src/io/dumper/dumper_material_padders.hh` | source | DUMP-014 |
| `src/io/dumper/dumper_nodal_field.hh` | source | DUMP-007 |
| `src/io/dumper/dumper_padding_helper.hh` | source | DUMP-013 |
| `src/io/dumper/dumper_quadrature_point_iterator.hh` | source | DUMP-009 |
| `src/io/dumper/dumper_text.cc` | source | DUMP-005 |
| `src/io/dumper/dumper_text.hh` | source | DUMP-005 |
| `src/io/dumper/dumper_type_traits.hh` | source | DUMP-008 |
| `src/io/dumper/dumper_variable.hh` | source | DUMP-015 |
| `src/io/mesh_io.cc` | source | IO-001, IO-002 |
| `src/io/mesh_io.hh` | source | IO-001, IO-002 |
| `src/io/mesh_io/mesh_io_diana.cc` | source | IO-011 |
| `src/io/mesh_io/mesh_io_diana.hh` | source | IO-011 |
| `src/io/mesh_io/mesh_io_msh.cc` | source | IO-003..IO-007, IO-009, IO-010 |
| `src/io/mesh_io/mesh_io_msh.hh` | source | IO-003..IO-007, IO-009, IO-010 |
| `src/io/mesh_io/mesh_io_msh_struct.cc` | source | IO-008 |
| `src/io/mesh_io/mesh_io_msh_struct.hh` | source | IO-008 |
| `src/io/parser/algebraic_parser.hh` | source | PARSE-006, PARSE-007 |
| `src/io/parser/algebraic_parser_x3.cc` | source | PARSE-005 |
| `src/io/parser/algebraic_parser_x3.hh` | source | PARSE-005 |
| `src/io/parser/cppargparse/cppargparse.cc` | source | PARSE-011 |
| `src/io/parser/cppargparse/cppargparse.hh` | source | PARSE-011 |
| `src/io/parser/cppargparse/cppargparse_tmpl.hh` | source | PARSE-011 |
| `src/io/parser/input_file_parser.hh` | source | PARSE-001 |
| `src/io/parser/parameter_registry.cc` | source | PARSE-008 |
| `src/io/parser/parameter_registry.hh` | source | PARSE-008 |
| `src/io/parser/parameter_registry_tmpl.hh` | source | PARSE-008 |
| `src/io/parser/parsable.cc` | source | PARSE-009 |
| `src/io/parser/parsable.hh` | source | PARSE-009 |
| `src/io/parser/parsable_petsc.hh` | source | PARSE-010 |
| `src/io/parser/parser.cc` | source | PARSE-002, PARSE-003 |
| `src/io/parser/parser.hh` | source | PARSE-002, PARSE-003 |
| `src/io/parser/parser_grammar_tmpl.hh` | source | PARSE-001, PARSE-006 |
| `src/io/parser/parser_input_files.cc` | source | PARSE-001 |
| `src/io/parser/parser_real.cc` | source | PARSE-005, PARSE-006, PARSE-007 |
| `src/io/parser/parser_tmpl.hh` | source | PARSE-004 |
| `src/mesh/element_group.cc` | source | MESH-022 |
| `src/mesh/element_group.hh` | source | MESH-022 |
| `src/mesh/element_group_inline_impl.hh` | source | MESH-022 |
| `src/mesh/element_type_map.cc` | source | MESH-019 |
| `src/mesh/element_type_map.hh` | source | MESH-017, MESH-018, MESH-019, CORE-034 |
| `src/mesh/element_type_map_filter.hh` | source | MESH-020 |
| `src/mesh/element_type_map_tmpl.hh` | source | MESH-017, MESH-018, MESH-019 |
| `src/mesh/group_manager.cc` | source | MESH-024, MESH-025, MESH-026, MESH-027 |
| `src/mesh/group_manager.hh` | source | MESH-024, MESH-025, MESH-026, MESH-027 |
| `src/mesh/group_manager_inline_impl.hh` | source | MESH-024, MESH-025, MESH-026, MESH-027 |
| `src/mesh/mesh.cc` | source | MESH-001, MESH-004, MESH-005, MESH-006, MESH-008, MESH-010, MESH-012, MESH-013, MESH-015, MESH-027, MESH-030, MESH-033, MESH-034 |
| `src/mesh/mesh.hh` | source | MESH-001..MESH-016, MESH-028, MESH-030, CORE-034 |
| `src/mesh/mesh_accessor.hh` | source | MESH-016 |
| `src/mesh/mesh_data.cc` | source | MESH-021 |
| `src/mesh/mesh_data.hh` | source | MESH-021 |
| `src/mesh/mesh_data_tmpl.hh` | source | MESH-021 |
| `src/mesh/mesh_events.hh` | source | MESH-015 |
| `src/mesh/mesh_filter.hh` | source | MESH-032 |
| `src/mesh/mesh_global_data_updater.hh` | source | MESH-033 |
| `src/mesh/mesh_inline_impl.hh` | source | MESH-003, MESH-006..MESH-009, MESH-011, MESH-013, MESH-014, MESH-028 |
| `src/mesh/mesh_iterators.hh` | source | MESH-031 |
| `src/mesh/mesh_periodic.cc` | source | MESH-028, MESH-029 |
| `src/mesh/node_group.cc` | source | MESH-023 |
| `src/mesh/node_group.hh` | source | MESH-023 |
| `src/mesh/node_group_inline_impl.hh` | source | MESH-023 |
| `src/mesh_utils/boost_graph_converter.cc` | source | MUTIL-017 |
| `src/mesh_utils/boost_graph_converter.hh` | source | MUTIL-017 |
| `src/mesh_utils/cohesive_element_inserter.cc` | source | MUTIL-015 |
| `src/mesh_utils/cohesive_element_inserter.hh` | source | MUTIL-015 |
| `src/mesh_utils/cohesive_element_inserter_helper.cc` | source | MUTIL-016 |
| `src/mesh_utils/cohesive_element_inserter_helper.hh` | source | MUTIL-016 |
| `src/mesh_utils/cohesive_element_inserter_inline_impl.hh` | source | MUTIL-015 |
| `src/mesh_utils/cohesive_element_inserter_parallel.cc` | source | build glue: empty translation unit (only includes) — parallel insertion lives in MUTIL-016/R3 |
| `src/mesh_utils/global_ids_updater.cc` | source | MUTIL-011 |
| `src/mesh_utils/global_ids_updater.hh` | source | MUTIL-011 |
| `src/mesh_utils/global_ids_updater_inline_impl.hh` | source | MUTIL-011 |
| `src/mesh_utils/mesh_partition.cc` | source | MUTIL-012 |
| `src/mesh_utils/mesh_partition.hh` | source | MUTIL-012 |
| `src/mesh_utils/mesh_partition/mesh_partition_mesh_data.cc` | source | MUTIL-013 |
| `src/mesh_utils/mesh_partition/mesh_partition_mesh_data.hh` | source | MUTIL-013 |
| `src/mesh_utils/mesh_partition/mesh_partition_scotch.cc` | source | MUTIL-014 |
| `src/mesh_utils/mesh_partition/mesh_partition_scotch.hh` | source | MUTIL-014 |
| `src/mesh_utils/mesh_utils.cc` | source | MUTIL-001..MUTIL-008 |
| `src/mesh_utils/mesh_utils.hh` | source | MUTIL-001..MUTIL-009 |
| `src/mesh_utils/mesh_utils_distribution.cc` | source | MUTIL-010 |
| `src/mesh_utils/mesh_utils_distribution.hh` | source | MUTIL-010 |
| `src/mesh_utils/mesh_utils_inline_impl.hh` | source | build glue: empty namespace (no behavior) |
| `src/mesh_utils/mesh_utils_pbc.cc` | source | MUTIL-009 (dead code) |
| `test/test_common/test_array.cc` | test | CORE-012..CORE-016, ITER-004 |
| `test/test_common/test_csr.cc` | test | CORE-029 |
| `test/test_common/test_grid.cc` | test | CORE-030 |
| `test/test_common/test_tensors.cc` | test | CORE-018, CORE-019, CORE-020 |
| `test/test_common/test_voigt_helper.cc` | test | CORE-021 |
| `test/test_fe_engine/test_facet_element_mapping.cc` | test | MESH-010, MUTIL-006, FE-003 |
| `test/test_fe_engine/test_fe_engine_fixture.hh` | test | test glue (FE fixture) for FE-* / ELEM-* tests |
| `test/test_fe_engine/test_fe_engine_gauss_integration.cc` | test | FE-014, QUAD-001..QUAD-017 |
| `test/test_fe_engine/test_fe_engine_inradius.cc` | test | FE-010 |
| `test/test_fe_engine/test_fe_engine_precomputation.cc` | test | FE-017, FE-018, FE-006, FE-007 (vs py_engine.py) |
| `test/test_fe_engine/test_fe_engine_precomputation_bernoulli_2.cc` | test | FE-035 (NOT registered in CMake) |
| `test/test_fe_engine/test_fe_engine_precomputation_bernoulli_3.cc` | test | FE-035 (NOT registered in CMake) |
| `test/test_fe_engine/test_fe_engine_precomputation_structural.cc` | test | FE-035, ELEM-021, ELEM-022, ELEM-023 |
| `test/test_fe_engine/test_fe_engine_structural_fixture.hh` | test | test glue for FE-035, ELEM-021..023, IO-008 |
| `test/test_fe_engine/test_gradient.cc` | test | FE-020 |
| `test/test_fe_engine/test_integrate.cc` | test | FE-013, CORE-026 |
| `test/test_fe_engine/test_interpolate.cc` | test | FE-019 (NOT registered in CMake) |
| `test/test_fe_engine/test_interpolate_bernoulli_beam_2.cc` | test | FE-035, ELEM-021 (NOT registered in CMake) |
| `test/test_fe_engine/test_inverse_map.cc` | test | FE-009, FE-004, FE-024 |
| `test/test_fe_engine/test_mesh_boundary.cc` | test | MESH-026 |
| `test/test_fe_engine/test_mesh_data.cc` | test | MESH-021 |
| `test/test_fe_engine/test_variational_operators.cc` | test | FE-019, FE-021, FE-022, FE-023 |
| `test/test_geometry/test_geometry_intersection.cc` | test | GEOM-002, GEOM-003, GEOM-009 |
| `test/test_geometry/test_geometry_predicates.cc` | test | GEOM-009 |
| `test/test_geometry/test_segment_intersection_tetrahedron_4.cc` | test | GEOM-004, GEOM-007, GEOM-008 |
| `test/test_geometry/test_segment_intersection_triangle_3.cc` | test | GEOM-004, GEOM-007, GEOM-008 |
| `test/test_io/test_dumper/test_dumper.cc` | test | DUMP-001, DUMP-002, DUMP-004, DUMP-005, DUMP-007, MESH-027 |
| `test/test_io/test_parser/test_parser.cc` | test | PARSE-001..PARSE-004, PARSE-006, PARSE-007 |
| `test/test_io/test_parser/test_parser_gtest.cc` | test | PARSE-005 |
| `test/test_mesh_utils/test_buildfacets/test_buildfacets.cc` | test | MUTIL-004, FE-003 |
| `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh.cc` | test | IO-005, DUMP-004 |
| `test/test_mesh_utils/test_mesh_io/test_mesh_io_msh_physical_names.cc` | test | IO-002, IO-005, MESH-025 |
| `test/test_mesh_utils/test_mesh_iterators.cc` | test | MESH-031 |
| `test/test_mesh_utils/test_mesh_partitionate/test_mesh_partitionate_mesh_data.cc` | test | MUTIL-012, MUTIL-013 |
| `test/test_mesh_utils/test_mesh_partitionate/test_mesh_partitionate_scotch.cc` | test | MUTIL-012, MUTIL-014 |
| `test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic.cc` | test | MESH-028 |
| `test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic_fixture.hh` | test | MESH-028 |
| `test/test_mesh_utils/test_mesh_periodic/test_periodic_mass.cc` | test | MESH-028, MESH-029 (uses solid mechanics, R2) |
| `test/test_mesh_utils/test_mesh_periodic/test_periodic_static.cc` | test | MESH-028, MESH-029 (uses solid mechanics, R2) |
| `test/test_mesh_utils/test_mesh_periodic/test_periodic_static_fixture.hh` | test | MESH-028, MESH-029 (uses solid mechanics, R2) |
| `test/test_mesh_utils/test_purify_mesh.cc` | test | MUTIL-008, MESH-014 |
| `test/test_mesh_utils/test_segment_nodetype/test_segment_nodetype.cc` | test | MUTIL-018 |
| `third-party/akantu_iterators/include/aka_compatibilty_with_cpp_standard.hh` | third-party | ITER-007 |
| `third-party/akantu_iterators/include/aka_iterators.hh` | third-party | build glue: umbrella include of ITER-001..ITER-006 |
| `third-party/akantu_iterators/include/aka_named_tuple.hh` | third-party | ITER-007 |
| `third-party/akantu_iterators/include/aka_static_if.hh` | third-party | ITER-007 |
| `third-party/akantu_iterators/include/aka_str_hash.hh` | third-party | ITER-007 |
| `third-party/akantu_iterators/include/aka_tuple_tools.hh` | third-party | ITER-007 |
| `third-party/akantu_iterators/include/iterators/aka_arange_iterator.hh` | third-party | ITER-003 |
| `third-party/akantu_iterators/include/iterators/aka_concatenate_iterator.hh` | third-party | ITER-006 |
| `third-party/akantu_iterators/include/iterators/aka_enumerate_iterator.hh` | third-party | ITER-002 |
| `third-party/akantu_iterators/include/iterators/aka_filter_iterator.hh` | third-party | ITER-004 |
| `third-party/akantu_iterators/include/iterators/aka_iterator_tools.hh` | third-party | ITER-001..ITER-006 (shared helpers) |
| `third-party/akantu_iterators/include/iterators/aka_repeat_iterator.hh` | third-party | ITER-006 |
| `third-party/akantu_iterators/include/iterators/aka_transform_iterator.hh` | third-party | ITER-005 |
| `third-party/akantu_iterators/include/iterators/aka_zip_iterator.hh` | third-party | ITER-001 |
| `third-party/akantu_iterators/test/test_gtest_main.cc` | test | test glue (gtest main) for ITER tests |
| `third-party/akantu_iterators/test/test_iterators.cc` | test | ITER-001..ITER-007 (library's own tests, not in Akantu suite) |
| `third-party/akantu_iterators/test/test_str.cc` | test | ITER-001..ITER-007 (library's own tests, not in Akantu suite) |
| `third-party/akantu_iterators/test/test_tuples.cc` | test | ITER-001..ITER-007 (library's own tests, not in Akantu suite) |
| `third-party/iohelper/example/mesher.c` | third-party | DUMP-016 (examples, not built) |
| `third-party/iohelper/example/testc.c` | test | DUMP-016 (examples, not built) |
| `third-party/iohelper/example/testcpp.cpp` | test | DUMP-016 (examples, not built) |
| `third-party/iohelper/src/base64.hh` | third-party | DUMP-004 (BASE64 encoding) |
| `third-party/iohelper/src/base64_reader.hh` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/container_array.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/dumper.cc` | third-party | DUMP-002, DUMP-004, DUMP-005 (file naming, steps, time description) |
| `third-party/iohelper/src/dumper.hh` | third-party | DUMP-002, DUMP-004, DUMP-005 (file naming, steps, time description) |
| `third-party/iohelper/src/dumper_C_wrapper.cc` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/dumper_C_wrapper.h` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/dumper_lammps.cc` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/dumper_lammps.hh` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/dumper_paraview.cc` | third-party | DUMP-004 |
| `third-party/iohelper/src/dumper_paraview.hh` | third-party | DUMP-004 |
| `third-party/iohelper/src/dumper_restart.cc` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/dumper_restart.hh` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/dumper_text.cc` | third-party | DUMP-005 |
| `third-party/iohelper/src/dumper_text.hh` | third-party | DUMP-005 |
| `third-party/iohelper/src/field.cc` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/field.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/field_accept.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/field_inline_impl.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/field_interface.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/file_manager.hh` | third-party | DUMP-004, DUMP-005 (file/zlib handling) |
| `third-party/iohelper/src/io_helper.hh` | third-party | DUMP-003, DUMP-004 (element types, VTK codes, data types) |
| `third-party/iohelper/src/iohelper_common.hh` | third-party | DUMP-003, DUMP-004 (element types, VTK codes, data types) |
| `third-party/iohelper/src/paraview_helper.cc` | third-party | DUMP-004 |
| `third-party/iohelper/src/paraview_helper.hh` | third-party | DUMP-004 |
| `third-party/iohelper/src/paraview_helper.tcc` | third-party | DUMP-004 |
| `third-party/iohelper/src/reader.cc` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/reader.hh` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/reader_C_wrapper.cc` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/reader_C_wrapper.h` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/reader_restart.cc` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/reader_restart.hh` | third-party | DUMP-016 (unused by Akantu) |
| `third-party/iohelper/src/variable.cc` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/variable.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/variable_inline_impl.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/variable_interface.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/src/visitor.hh` | third-party | DUMP-006..DUMP-009, DUMP-015 (IOHelper field/variable plumbing) |
| `third-party/iohelper/test/mesh_io_msh.cc` | test | DUMP-004/DUMP-005 (library's own tests, not in Akantu suite) |
| `third-party/iohelper/test/mesh_io_msh.hh` | test | DUMP-004/DUMP-005 (library's own tests, not in Akantu suite) |
| `third-party/iohelper/test/test_paraview_dumper.cc` | test | DUMP-004/DUMP-005 (library's own tests, not in Akantu suite) |
| `third-party/iohelper/test/test_text_dumper.cc` | test | DUMP-004/DUMP-005 (library's own tests, not in Akantu suite) |

Non-code fixtures in scope (mapped through the main table "Upstream tests &
fixtures" column): `test/test_common/circle.geo`; `test/test_fe_engine/_<type>.msh`
(13 regular + 3 structural, MSH 2.2) and `_<type>.csv` (inradius references),
`*.geo` (`circle, cube, cube_physical_names, hexa_structured, line, pentahedron,
segment, square, square_structured`), `pentahedron.msh`, `segment.msh`,
`generate_test` (script), `py_engine/py_engine.py` (independent Python
implementation of shapes/derivatives/jacobians used by FE-018 test),
`test_interpolate_bernoulli_beam_2.verified`; `test/test_geometry/test_geometry_{triangle,tetrahedron}.msh`;
`test/test_io/test_dumper/{input_file.dat,test_dumper.msh}`;
`test/test_io/test_parser/{input_file.dat,test_parser.py,test_parser.verified}`;
`test/test_mesh_utils/{iterators_mesh.geo,purify_mesh.msh}`,
`test_buildfacets/*.msh` (13 meshes) + `test_buildfacets_gtest.verified`,
`test_mesh_io/{cube,cube_physical_names}.geo` + `test_mesh_io_msh_physical_names.verified`,
`test_mesh_partitionate/{quad,triangle}.geo`,
`test_mesh_periodic/{cube,line,square}.geo` + `material.dat`,
`test_segment_nodetype/{mesh.geo,mesh.msh,test_segment_nodetype.sh}`;
`src/io/parser/cppargparse/COPYING{,.lesser}` (license texts, no behaviour).
`test/test_mesh_utils/CMakeLists.txt` references a non-existent directory
`test_pbc_tweak` (stale).

---

## 5. Open questions

1. **tet4 natural-coordinate permutation** (ELEM-008): reproduce the code
   (`N = (1−ξ−η−ζ, η, ζ, ξ)`) or the documented/standard mapping? Physical
   results (B, jacobians, integrals) are identical; natural coordinates
   (inverse map, contact projections, `computeShapes` at given natural points)
   differ. Recommendation: reproduce the code bit-for-bit, document in DECISIONS.
2. **Known reference bugs** — reproduce or fix? `Array<Real>::find` returns
   distance-from-end (CORE-013); `_point_1` normal uses element index as node
   index (FE-029); MSH writer never writes elemental data / throws with nodal
   data (IO-010); MSH v4 `msh_block_*` groups stay empty (IO-007); MSH v1 not
   actually supported (IO-006); `$ElementNodeData` reads only `nb_component`
   values (IO-009); Diana reader fails in 2D (IO-011); `getIGFEMElementTypes`
   lacks a return (FE-038); `_miot_abaqus` null dereference (IO-001); bare
   `throw;` in group field factories (MESH-027); `DumperText::registerMesh`
   enum mismatch (DUMP-005); DKT shape functions empty (ELEM-023). Each needs a
   DECISIONS entry (proposal: fix crashes/UB, replicate numerically visible
   behaviour unless clearly a bug, and flag in parity matrix).
3. **IGFEM** (ELEM-024..026): the extra package references `_ek_igfem` and a
   4-argument `Element` constructor that no longer exist → it cannot compile
   against this core. Is IGFEM in scope for parity (R2 decision) and if so
   against which core API?
4. **Random numbers** (CORE-031/032): bit-for-bit random fields require
   emulating libstdc++'s `minstd_rand0` and its distribution algorithms
   (normal, weibull, ...). Is RNG stream equality a parity requirement, or are
   statistical tests acceptable? Which oracle platform (glibc/libstdc++ in
   Docker) is the reference?
5. **Scotch partitions** (MUTIL-014): parallel differential tests need either
   the same Scotch version/strategy or importing the oracle's partition
   (`MeshPartitionMeshData`). Proposal: export partitions from the oracle as
   golden data.
6. **Gmsh-generated fixtures**: build-time meshes depend on the gmsh version
   (MSH 4.1 by default) — freeze the oracle-generated `.msh` files as fixtures
   instead of regenerating.
7. **Pentahedron quadrature**: the active rules are segment×triangle products
   (6 points for penta6/penta15 by default); the explicit 6/8/21-point tables in
   `gauss_integration.cc` are dead and partly wrong. Confirm we port only the
   active rules.
8. **Two expression grammars** (PARSE-005 vs PARSE-006): scalar values use X3
   (functions before constants, case-sensitive), vector/matrix elements use Qi
   (constants before functions, case-insensitive functions) → `exp(...)` fails
   inside vectors. Replicate or unify?
9. **`ElementType` numbering**: numeric values depend on enabled packages
   (cohesive/structural/IGFEM). Does any persisted format (restart, Python
   pickles, MPI buffers between differently-configured builds) depend on it?
   Proposal: Rust uses one fixed full enum and string names for I/O.
10. **Eigen closed-form inverses/determinants**: bitwise equality of `dN/dx`
    and jacobians requires replicating Eigen's 2×2/3×3 `inverse()` and
    `determinant()` formulas exactly (and the 4×4 path if used). To be
    confirmed by the oracle validation which tolerance class applies.
11. **`REAL_INIT_VALUE`** is NaN in debug and 0 in release; which build type is
    the oracle (`docs/ORACLE_BUILD.md`)? Release (`-DAKANTU_NDEBUG`) also
    disables `AKANTU_DEBUG_ASSERT` checks, changing error behaviour.
12. **Unused/dead code** (CircularArray, legacy PBC map, `dumpable_dummy.hh`,
    unregistered tests `test_interpolate*.cc`, `test_fe_engine_precomputation_bernoulli_*.cc`,
    IOHelper LAMMPS/restart): confirm they are recorded in the parity matrix as
    "not ported (dead in reference)" rather than silently dropped.

## Errata (independent completeness review, 2026-11-09)

Factual corrections from an independent completeness review of the inventory. Where they conflict with the text above, **the errata win**.

| Finding | Existing row / location | Evidence and correction |
|---|---|---|
| E1 | DUMP-011, core inventory | Rank of the owner is false for ghost dumps. `src/io/dumper/dumper_element_partition.hh:39-52` initializes prank from the **static communicator current rank**, then fills every output with that value. Correct to writing rank, including ghosts; distinguish ElementSynchronizer::getRank. A ghost owned by rank 0 but dumped on rank 1 yields 1. |
