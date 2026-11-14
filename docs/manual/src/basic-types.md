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
  first stored `a` with `|a − value| ≤ ε`, `ε = DBL_EPSILON`
  (`aka_array.cc:35-44`), and so does the port for `f64`: `find(&0.3)` on
  `[0.1 + 0.2]` finds the value. The bound is absolute: `find(&0.0)`
  matches `1e-300`, `find(&1.0)` matches `1 + ε` but not `1 + 2ε`, and a
  large value matches only itself; NaN and ±∞ are never found (`∞ − ∞` is
  NaN). Every other element type (integers, `bool`, `f32`, …) compares
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
  to avoid confusion;
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
