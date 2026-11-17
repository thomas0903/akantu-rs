<!-- availability: available akantu::core -->
# Basic types and iteration

This chapter corresponds to upstream's *Basic types* chapter
(`basic_types.rst`). Upstream v5 builds its containers on Eigen and on the
`akantu_iterators` library (`make_view`, `zip`, `enumerate`); the port uses
plain Rust slices, arrays and iterators with the same memory layout, so
that data can be compared with upstream value by value.

## Scalars and indices

| Upstream | Rust (`akantu::core`) | Type |
|---|---|---|
| `Real` | `Real` | `f64` |
| `Int`, `Idx` | `Int`, `Idx` | `i32` (local node, element and DOF numbers) |
| `UInt` | `UInt` | `u32` |
| global node numbers | `GlobalIdx` | `i64` |
| MPI rank | `Rank` | `i32` |

Sizes and counts are `usize`; conversions between `usize` and `Idx` are
checked (`akantu::core::try_to_usize`, `try_idx`), never silently
truncated.

## `Array<T>`

`Array<T>` is a contiguous array of *tuples* of `nb_component` values (the
upstream `Array<T>`): component `j` of tuple `i` is stored at
`i * nb_component + j`, exactly as upstream and as a C-ordered NumPy array
of shape `(len, nb_component)`.

```rust
use akantu::core::Array;

// upstream: Array<Real> position(10, 3);
let mut position = Array::<f64>::zeros(10, 3);
assert_eq!((position.len(), position.nb_component()), (10, 3));

// upstream: position(7, 0) = 1.5;
position[(7, 0)] = 1.5;
assert_eq!(position.tuple(7), &[1.5, 0.0, 0.0]);

// upstream: push_back(tuple)
position.push_tuple(&[1.0, 2.0, 3.0]);
assert_eq!(position.len(), 11);

// upstream: set(t) and zero()
let mut flags = Array::filled(4, 1, false);
flags.fill(true);
assert!(flags.as_slice().iter().all(|&b| b));

// upstream: find(value): the index of the first match, `None` instead of -1
let ids = Array::from_vec(vec![4, 8, 15, 16], 1)?;
assert_eq!(ids.find(&15)?, Some(2));
assert_eq!(ids.find(&42)?, None);
// f64: upstream's comparison |a − value| ≤ DBL_EPSILON (absolute)
let x = Array::from_vec(vec![0.1 + 0.2], 1)?;
assert_eq!(x.find(&0.3)?, Some(0));
// several components: an error, as upstream throws (use `find_tuple`)
assert!(position.find(&1.5).is_err());
# Ok::<(), akantu::core::Error>(())
```

| Upstream | Rust | Note |
|---|---|---|
| `Array<T> a(n, nbc)` | `Array::zeros(n, nbc)`, `Array::filled(n, nbc, v)`, `Array::new(nbc)` (empty) | |
| `a(i, j)` | `a[(i, j)]` | bounds-checked |
| `a.size()`, `a.getNbComponent()` | `a.len()`, `a.nb_component()` | |
| `resize(n)` | `resize(n, fill)` / `try_resize(n, fill)` | new tuples get `fill` |
| `clear()` | `clear()` | length 0, as upstream (≥ v4) |
| `set(t)`, `zero()` | `fill(t)`, `zero()` | |
| `copy(other)` | `copy_from(&other)` | |
| `push_back(tuple)` | `push_tuple(&tuple)` | |
| `erase(i)` | `swap_remove_tuple(i)` | moves the last tuple into `i`, as upstream |
| `find(value)` | `find(&value) -> Result<Option<usize>>`, `find_tuple(&tuple)`; `find_upstream(&value)` (upstream's raw value) | the index of the first match, upstream's comparison; see below |
| `storage()` | `as_slice()`, `as_mut_slice()` | |

`find` compares as upstream and differs from it only in the returned
position (decision D-061):

* Comparison (as upstream). Upstream's `Array<Real>::find` matches the
  first stored $a$ with $\lvert a - v \rvert \le \varepsilon$, where $v$
  is the searched `value` and $\varepsilon$ = `DBL_EPSILON`
  (`aka_array.cc:35-44`), and so does the port for `f64`: `find(&0.3)` on
  `[0.1 + 0.2]` finds the value. The bound is absolute: `find(&0.0)`
  matches `1e-300`, `find(&1.0)` matches $1 + \varepsilon$ but not
  $1 + 2\varepsilon$, and a large value matches only itself; NaN and
  $\pm\infty$ are never found ($\infty - \infty$ is NaN). Every other element type (integers, `bool`, `f32`, …) compares
  with `==`, as upstream's generic `Array<T>::find`
  (`aka_array_tmpl.hh:639-645`).
* Return value (UD-042, corrected). Upstream's generic `find` returns
  the index of the first match or −1, as the upstream manual says; its
  `Array<Real>` specialisation returns `end() − it`, the distance from the
  end (`size − index`), when the value is found. The port returns
  `Ok(Some(index))` or `Ok(None)` for every element type;
  `find_upstream(&value)` returns upstream's raw value (`size − index` for
  `f64`, the index otherwise, −1 when absent) for differential comparisons,
  and `find_real(value)` is a shorthand for `f64` returning
  `Option<usize>`.
* Several components. Upstream's scalar `find` iterates over tuples
  of one value and throws (`ArrayException`, `aka_array_tmpl.hh:832-840`)
  on a non-empty array with several components; the port returns
  `Err(Error::DimensionMismatch)` there (an empty array gives `Ok(None)`:
  upstream does not throw either). Use `find_tuple`, upstream's
  `find(Vector)`, for tuples.

These semantics are checked by `doc017_array_find_semantics`
(`crates/akantu/tests/manual_claims.rs`).

Sizes given by users or files go through the `try_*` constructors
(`try_zeros`, `try_resize`, …), which return `Error::CapacityExceeded`
instead of aborting.

## Iterating over arrays

Upstream iterates with `make_view(array, n)` (vectors) or
`make_view(array, rows, cols)` (matrices). In Rust:

| Upstream | Rust |
|---|---|
| `make_view(a)` (scalars) | `a.as_slice().iter()` |
| `make_view(a, n)` (dynamic size) | `a.tuples()`, `a.tuples_mut()` (slices of `nb_component` values) |
| `make_view<n>(a)` (static size) | `a.as_fixed::<N>()?` (`&[[T; N]]`) |
| `make_view(a, r, c)` | `a.tuple_matrix::<R, C>(i)?` (column-major `Matrix<R, C>`) |
| `enumerate(…)`, `zip(…)`, `arange(n)` | `Iterator::enumerate`, `Iterator::zip`, `0..n` |

```rust
use akantu::core::Array;
use akantu::core::tensor::MatrixExt;

let nodes = Array::from_vec(vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0], 2)?;

// upstream: for (auto && [i, x] : enumerate(make_view(nodes, 2)))
for (i, x) in nodes.tuples().enumerate() {
    assert_eq!(x.len(), 2);
    if i == 2 {
        assert_eq!(x, &[1.0, 1.0]);
    }
}

// static views: each tuple is a [f64; 2]
let fixed: &[[f64; 2]] = nodes.as_fixed::<2>()?;
let centroid = fixed
    .iter()
    .fold([0.0; 2], |c, x| [c[0] + x[0] / 3.0, c[1] + x[1] / 3.0]);
assert!((centroid[0] - 2.0 / 3.0).abs() < 1e-15);

// a 2×2 tensor per tuple, stored column-major as upstream
let stresses = Array::from_vec(vec![1.0, 2.0, 3.0, 4.0], 4)?;
let s = stresses.tuple_matrix::<2, 2>(0)?;
assert_eq!(s.at(1, 0), 2.0); // row 1, column 0
assert_eq!(s.at(0, 1), 3.0);
# Ok::<(), akantu::core::Error>(())
```

## Vectors and matrices

Small tensors are fixed-size Rust arrays (`akantu::core::tensor`):

* `Vector<N>` = `[f64; N]`;
* `Matrix<R, C>` = `[[f64; R]; C]`, column-major: `m[j][i]` is row
  `i`, column `j`, Eigen's storage order. Use `at(i, j)` and `at_mut(i, j)`
  to avoid confusion. In the flat storage (`as_flat()`), the entry
  $A_{ij}$ of a matrix with $M$ rows is at position $k = i + jM$, as in
  upstream's `A[k]`;
* `MatrixX` is the dynamically sized column-major matrix.

The operations are provided by the traits `VectorExt`, `MatrixExt` and
`SquareMatrixExt` and reproduce the evaluation order of the corresponding
Eigen 3.4 expressions, so that they give the same bits as upstream's
strict build (no contraction, no vectorised kernels; see [Bitwise
reproducibility](differences.md#bitwise-reproducibility-and-platforms)).

```rust
use akantu::core::tensor::{Matrix, MatrixExt, SquareMatrixExt, VectorExt};

let a: Matrix<2, 2> = Matrix::from_rows([[2.0, 1.0], [1.0, 3.0]]);
assert_eq!(a.at(0, 1), 1.0);
assert_eq!(a.det(), 5.0);
assert_eq!(a.trace(), 5.0);

let inv = a.inverse()?; // error for an exactly singular matrix
let id = a.matmul(&inv);
assert!((id.at(0, 0) - 1.0).abs() < 1e-15 && id.at(1, 0).abs() < 1e-15);

let v = [3.0, 4.0];
assert_eq!(v.norm(), 5.0);
assert_eq!(a.mul_vec(&v), [10.0, 15.0]);
assert_eq!(a.double_dot(&Matrix::<2, 2>::identity()), 5.0);
# Ok::<(), akantu::core::Error>(())
```

| Upstream (`Vector`/`Matrix`) | Rust |
|---|---|
| `v.norm()`, `v.dot(x)` | `v.norm()`, `v.dot(&x)` |
| `A(i, j)`, `A.rows()`, `A.cols()` | `a.at(i, j)`; sizes are type parameters (`MatrixX::rows()/cols()`) |
| `A.trace()`, `A.det()`, `A.doubleDot(B)` | `a.trace()`, `a.det()`, `a.double_dot(&b)` |
| `A.inverse()`, `A.transpose()` | `a.inverse()?` (`inverse_unchecked()` reproduces upstream on singular input), `a.transpose()` |
| `A * B`, `A.transpose() * B` | `a.matmul(&b)`, `a.tr_matmul(&b)` |
| `Matrix<Real>::Identity(n, n)` | `Matrix::<N, N>::identity()`, `MatrixX::identity(n)` |
| `A.eig(…)`, `A.eigh(…)` | `akantu::linalg::dense::{eig, eig_vectors, eigh, eigh_vectors}` |

The norms and products are the usual ones: `norm()` is the Euclidean
norm $\lVert \boldsymbol{v} \rVert = \left(\sum_i v_i^2\right)^{1/2}$ of
a vector and the Frobenius norm
$\lVert \boldsymbol{A} \rVert = \left(\sum_{i,j} A_{ij}^2\right)^{1/2}$
of a matrix (upstream's default `norm()`); `double_dot` is
$\boldsymbol{A} : \boldsymbol{B} = \sum_{i,j} A_{ij} B_{ij}$;
`matmul` and `tr_matmul` give $\boldsymbol{A}\boldsymbol{B}$ and
$\boldsymbol{A}^T\boldsymbol{B}$; `scale` and `div` multiply or divide
every component by a scalar, `add` and `sub` act component by component.
Each sum is taken in the order of the Eigen expression it replaces
(a halving tree for `dot`, `squared_norm` and `sum`), which is why these
are methods rather than generic iterator folds.

Upstream's `Vector` and `Matrix` also offer the $L_N$ norms
$\left(\sum_i \lvert v_i \rvert^N\right)^{1/N}$ (`norm<N>()`),
`distance`, `crossProduct`, `outerProduct` ($\boldsymbol{v}_1
\boldsymbol{v}_2^T$), `solve` and the generic `mul<t_A, t_B>` with
transposition flags. The port's `akantu::core::tensor` traits have no
counterpart of these: the code that needs one computes it in place, in
upstream's operation order.

## Element types and elements

`ElementType` lists the upstream element types in upstream order
(`_point_1` … `_hexahedron_20`, the cohesive, structural and IGFEM types;
see the [element appendix](appendix/elements.md)). Their static
properties come from tables shared by every crate. An `Element` is the
triple *(type, index, ghost type)*, as upstream.

```rust
use akantu::core::{Element, ElementType, GhostType};

let t: ElementType = "_triangle_6".parse()?; // also accepts "triangle_6"
assert_eq!(t, ElementType::Triangle6);
assert_eq!(t.name(), "_triangle_6");
assert_eq!(t.nb_nodes_per_element(), 6);
assert_eq!(t.spatial_dimension(), 2);
assert_eq!(t.p1_type(), Some(ElementType::Triangle3));
assert_eq!(t.facet_type(0), Some(ElementType::Segment3));

let e = Element::new(ElementType::Quadrangle4, 3, GhostType::NotGhost);
assert_eq!((e.ty(), e.index(), e.ghost_type()), (ElementType::Quadrangle4, 3, GhostType::NotGhost));
assert!(Element::NULL.is_null()); // upstream ElementNull
# Ok::<(), akantu::core::Error>(())
```

## Per-type data: `ElementTypeMapArray<T>`

Data that depends on the element type (connectivities, quadrature-point
fields, element groups) is stored in an `ElementTypeMapArray<T>`: one
`Array<T>` per *(element type, ghost type)*, iterated in upstream enum
order. `TypeFilter` selects the types to visit (upstream's
`elementTypes(dim, ghost_type, kind)` filters).

```rust
use akantu::core::enums::MeshIoType;
use akantu::core::{ElementType, GhostType, TypeFilter};
use akantu::mesh::Mesh;
# let mesh_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
#     .join("../../docs/manual/src/meshes/square.msh");

let mut mesh = Mesh::new(2, "mesh")?;
akantu::io::read_mesh(&mut mesh, &mesh_file, MeshIoType::Auto)?;

// upstream: mesh.getConnectivity(_quadrangle_4)
let quads = mesh.connectivity(ElementType::Quadrangle4, GhostType::NotGhost)?;
assert_eq!((quads.len(), quads.nb_component()), (4, 4));
assert_eq!(quads.tuple(0), &[0, 1, 4, 3]);

// upstream: for (auto && type : mesh.elementTypes(2)) …
let types: Vec<ElementType> = mesh.element_types(TypeFilter::new().dim(2)).collect();
assert_eq!(types, [ElementType::Quadrangle4]);

// every element of dimension 1 (the boundary segments), in upstream order
let segments = mesh.elements(TypeFilter::new().dim(1)).count();
assert_eq!(segments, 8);
# Ok::<(), akantu::core::Error>(())
```

## The finite-element engine

The finite-element engine, `akantu::fe::FeEngine` (upstream `FEEngine`
with the `IntegratorGauss` and `ShapeLagrange` policies), holds the
quadrature rules of the element types of a mesh and the values of the
shape functions, of their derivatives and of the Jacobians at the
quadrature points. A model creates the engines it needs and constitutive
laws reach them by name (`ModelServices::fe_engine`), so a simulation
seldom builds one itself; a standalone engine is built with
`FeEngine::with_precomputation(mesh, None, ElementKind::Regular,
ShapeFamily::Lagrange, id)` (`None`: the elements of the mesh's
dimension). Structural elements have their own engine,
`StructuralFeEngine` ([Structural mechanics model](models/structural.md)).

Every element is the image of a reference element (the [element
appendix](appendix/elements.md) lists them, with their shape functions
$N_i$ in the natural coordinates $\boldsymbol{\xi}$ and their quadrature
points $\boldsymbol{\xi}_q$ and weights $w_q$). The geometry is
interpolated with the same shape functions as the fields
(iso-parametric elements):

$$
\boldsymbol{x}(\boldsymbol{\xi}) = \sum_i N_i(\boldsymbol{\xi})\, \boldsymbol{x}_i ,
\qquad
\boldsymbol{J}(\boldsymbol{\xi}) = \sum_i \frac{\partial N_i}{\partial \boldsymbol{\xi}} \otimes \boldsymbol{x}_i ,
\quad J_{ab} = \frac{\partial x_b}{\partial \xi_a} ,
$$

where $\boldsymbol{x}_i$ are the coordinates of the element's nodes
($\boldsymbol{J}$ has one row per natural coordinate, as upstream's
`computeJacobian`). The
three basic operations of the engine are then, for a nodal field with
values $\boldsymbol{u}_i$ (`nb_dof` components per node):

| Operation | Formula at the quadrature point $\boldsymbol{\xi}_q$ of an element | Output |
|---|---|---|
| `interpolate_on_integration_points` | $\boldsymbol{u}(\boldsymbol{\xi}_q) = \sum_i N_i(\boldsymbol{\xi}_q)\, \boldsymbol{u}_i$ | one tuple of `nb_dof` per quadrature point |
| `gradient_on_integration_points` | $\dfrac{\partial u_k}{\partial x_j}(\boldsymbol{\xi}_q) = \sum_i \dfrac{\partial N_i}{\partial x_j}(\boldsymbol{\xi}_q)\, u_{ik}$ | one tuple of `nb_dof` $\times$ dimension per quadrature point, column-major ($\partial u_k / \partial x_j$ at $k + j \cdot$ `nb_dof`) |
| `integrate` | $\displaystyle\int_{\Omega_e} \boldsymbol{f} \,\mathrm{d}\Omega \approx \sum_q \boldsymbol{f}(\boldsymbol{\xi}_q)\, w_q \det \boldsymbol{J}(\boldsymbol{\xi}_q)$ | one tuple of `nb_dof` per element |

The derivatives with respect to $\boldsymbol{x}$ are
$\partial N_i / \partial \boldsymbol{x} = \boldsymbol{J}^{-1}\,
\partial N_i / \partial \boldsymbol{\xi}$. For an element of lower
dimension than the space (a segment in 2-D or 3-D, a surface element in
3-D), $\det \boldsymbol{J}$ in the integral stands for
$\lVert \boldsymbol{J} \rVert$ (segment) or
$\lVert \boldsymbol{J}_0 \times \boldsymbol{J}_1 \rVert$ (surface,
$\boldsymbol{J}_0$ and $\boldsymbol{J}_1$ the two rows of
$\boldsymbol{J}$), the length or area ratio of the map. The engine stores them (`shapes_derivatives`) together with the shape
values (`shapes`) and the products $w_q \det \boldsymbol{J}$
(`jacobians`), so that each operation is a loop over stored values.
`integrate_scalar` adds the integrals of a scalar field over all the
elements of a type with upstream's sorted pairwise sum (`Math::reduce`).
All these functions take the element type, the ghost type and an
optional filter (a list of element indices, upstream's `filter_elements`),
as upstream; the cohesive engine interpolates on the mid-surface of the
two faces ([element appendix](appendix/elements.md#cohesive-elements)).

Upstream's second example integrates the strain and stress of the
elements of one material: in the port, a constitutive law's internal
fields hold the values per quadrature point of its own elements, and
`integrate` with the law's element filter gives the integral of each.
