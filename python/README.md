# The `akantu` Python package of akantu-rs

This directory builds the Python interface of akantu-rs, the Rust port of
the [Akantu](https://gitlab.com/akantu/akantu) finite-element library. The
package keeps upstream's names: it is imported as `akantu`, its extension
module is `akantu.py11_akantu`, and `akantu/__init__.py` is upstream's file
with a few documented corrections. Scripts written for upstream's package
run with it; the differences are listed in the user manual's chapter
[Python interface](../docs/manual/src/models/python.md).

The package is not published on PyPI. The name `akantu` on PyPI belongs to
upstream's own package, which installs the C++ library's bindings; this one
is built from source with [maturin](https://www.maturin.rs/).

## Building

You need the Rust toolchain of the repository (`rust-toolchain.toml`),
Python 3.9 or later, NumPy and SciPy. From the root of the repository:

```sh
python3 -m venv .venv
.venv/bin/pip install maturin numpy scipy pytest greenlet matplotlib
cd python
../.venv/bin/maturin develop --release   # builds the extension and installs `akantu` into .venv
../.venv/bin/python -m pytest -q tests   # the package's tests
```

`greenlet` and `matplotlib` are needed by some tests only: without
greenlet its cases are skipped, and two example comparisons
(`structural_mechanics_dynamics*`) fail without matplotlib, because the
example defines the compared value `final.times` only when matplotlib
imports. `maturin build --release` writes a wheel instead (one `abi3` wheel serves
every CPython from 3.9 on). The default build has no MPI. The MPI build of
the module (Cargo feature `mpi` of `akantu-python`) needs an MPI
implementation and Scotch at build time and covers the mesh level only:
`Mesh.distribute()` and the local and global numbering on several ranks.
Models on a distributed mesh are run from Rust.

## Contents

| Path | What it is |
|---|---|
| `pyproject.toml` | the maturin build: the extension is the crate `crates/akantu-python` with its feature `python` |
| `akantu/` | upstream's pure-Python package; the built extension is placed next to it |
| `tests/` | the package's tests: upstream's Python tests and the port's tests of the bindings (NumPy views, callbacks, refused calls) |
| `examples/` | upstream's Python examples, unchanged, compared in `tests/examples` with runs of upstream's module |
| `oracle/` | scripts that run upstream's module (the oracle) on the same inputs for the comparisons of the tests |

## Licence

LGPL-3.0-or-later, like the rest of akantu-rs (see the repository's
[`README.md`](../README.md)).
