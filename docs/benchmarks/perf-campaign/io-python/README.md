<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Input parser, mesh I/O, dumpers and the Python boundary

Measurements of the changes to the `.dat` parser (`akantu-parser`), the
Gmsh reader and writer and the dumpers (`akantu-io`), and the Python
bindings (`akantu-python`). Every change keeps the values, the written bytes
and the user-visible behaviour: the digests of every case (state arrays,
written files) are the same before and after.

Machine: an AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, 32 GiB), Ubuntu
24.04, release profile, one process per run pinned to core 2
(`taskset`), interleaved A/B with the order rotated per repetition, 5
repetitions, 2026-11-16 (UTC). "Before" is commit `f373ab7ce` (the base
plus the `parser_chain` case), "after" commit `bc71120b5`. A build and the
test tier of other work ran on the other four cores during the series.
Ratios: median [min, max] of the per-repetition ratios after / before.

## Set-up suite (`suite_setup`, size `m`)

Raw data `ab3/` (summary `ab3.md`); the intermediate series `ab1/`,
`ab2/` measured the changes one at a time.

| case | phase | before | after | ratio |
|---|---|---|---|---|
| `parser_chain` (21 lines) | evaluate the last | 400 ms [394, 409] | 7.1 µs [7.0, 10.0] | 1.8e-5 |
| `parser_dat` (2,000 sections) | evaluate all 16,003 parameters | 15.6 ms [15.4, 15.8] | 11.9 ms [11.8, 12.2] | 0.769 [0.755, 0.777] |
| `parser_dat` | parse | 8.72 ms [8.71, 9.00] | 8.77 ms [8.30, 8.97] | no measurable change |
| `msh22_read_tet4` (162,000 Tet4) | read | 153 ms [151, 155] | 84.8 ms [84.6, 85.9] | 0.560 [0.546, 0.566] |
| `msh41_read_tet4` | read | 129 ms [124, 131] | 73.9 ms [73.3, 74.4] | 0.573 [0.563, 0.590] |
| `msh22_read_hex8` (125,000 Hex8) | read | 197 ms [193, 201] | 101 ms [100, 103] | 0.514 [0.502, 0.524] |
| `msh_write_tet4` | write | 53.9 ms [53.0, 54.6] | 25.8 ms [25.5, 26.4] | 0.481 [0.466, 0.489] |
| `dump_paraview_tet4` (base64) | dump | 208 ms [199, 210] | 99.2 ms [95.5, 101] | 0.480 [0.462, 0.487] |
| `dump_paraview_ascii_tet4` | dump | 913 ms [904, 938] | 296 ms [285, 311] | 0.327 [0.313, 0.332] |
| `dump_text_tet4` | dump | 73.8 ms [73.4, 74.0] | 17.2 ms [16.9, 17.3] | 0.233 [0.230, 0.235] |

Command (`tools/` holds the scripts as run):

```text
scripts/perf/suite/run_ab.sh --out ~/w/io/ab3 --size m --reps 5 --core 2 \
  --cases parser_chain,parser_dat,msh22_read_tet4,msh41_read_tet4,msh22_read_hex8,msh_write_tet4,dump_paraview_tet4,dump_paraview_ascii_tet4,dump_text_tet4 \
  before=~/w/io/bin/before after=~/w/io/bin/after5
```

`parser_chain` evaluated the last of 21 parameters that each refer twice
to the previous one: before, 2^21 expressions were parsed; the time
doubled with every further line (24 lines: 1.8 s).

## Python suite (`python/benchmarks/suite_python.py`, size `m`)

Two builds of the extension (`maturin develop --release`), before
(`f373ab7ce`) and after (`eb93a14eb`, the last change of the bindings),
`tools/pyab.sh`, one BLAS thread. Raw data `pyab/`.

| case | phase | before | after | ratio |
|---|---|---|---|---|
| `dirichlet_tri3` (40,401 nodes) | Python `__call__` per node, one `applyBC` | 31.5 ms [28.3, 32.0] | 23.4 ms [23.3, 27.1] | 0.746 [0.728, 0.939] |
| `dirichlet_group_tri3` | `callNodes`, one `applyBC` | – | 1.00 ms [0.99, 1.11] | 0.032 against `__call__` before |
| `dirichlet_tri3` | built-in `FixedValue` | 0.292 ms | 0.282 ms | no measurable change |
| `explicit_tri3` | mesh (written in Python, read) | 54.1 ms [53.8, 63.2] | 31.1 ms [31.1, 35.8] | 0.575 [0.567, 0.577] |
| `explicit_tri3` | `solveStep` / tiny mesh | 4.95 ms / 9.63 µs | 4.85 ms / 9.49 µs | no measurable change |
| `neumann_tri3` | Python `__call__` per point | 1.25 ms | 1.27 ms | no measurable change |
| `material_tri3` | Python law, per step | 10.6 ms | 10.7 ms | no measurable change |

The digests of every Python case are the same with both builds, and
`dirichlet_group_tri3` gives the digest of `dirichlet_tri3`.

## What changed

- Parser: each variable is evaluated once per evaluation and its value
  reused (only successful values are stored, so errors and the cycle check
  are unchanged; the tests compare the messages with and without the
  cache); function names are borrowed and error messages built only on
  failure; one pointer-hashed map serves the cycle check and the cache.
- Number output (`printf` `%e`/`%g` emulation): numbers are appended to
  the output buffer instead of one `String` each; the digits of up to 17
  significant digits come from a 128-bit table of powers of ten with an
  error bound, and `core::fmt` decides the cases within that bound of a
  rounding tie (tested against `core::fmt` on random bit patterns, values
  near powers of ten and short decimals, every precision; a run of about
  3 × 10^8 combinations matched).
- VTU (ParaView) pieces: validated before any file is touched (the same
  checks and errors, in the same order), then written straight to the
  file through a 1 MiB buffer; base64 arrays are encoded in one pass with
  the byte count computed in advance (the encoded header and payload are
  the original encoder's bytes; tested on every length up to 300 bytes and
  around the block size). The whole file is no longer held in memory.
- Dumped fields: value vectors are allocated at their final size; derived
  quantities (strain, principal strain, von Mises) are computed over a
  whole block in one call.
- Gmsh reader: node and element tags in dense tables (a hash map for tags
  beyond twice the announced count), no allocation per element, integers
  parsed in one pass, group node lists sorted through a bitmap when they
  are dense.
- Python: a Python Dirichlet functor's group is handled with one export of
  the arrays and one entry into Python (`__call__` is still called once
  per node with views of the live rows); an optional group method
  `callNodes(nodes, flags, primal, coords)` (copies of the rows, written
  back after the call).

## Not changed, with the reason

- Python `refresh` after every call: it already re-leases only the exports
  whose storage changed; what remains is a map lookup per export (about a
  microsecond, visible on toy meshes only).
- NumPy inputs (`Array::from_vec` in `py/physics.rs`): the native section
  releases the GIL, so the input cannot be borrowed across it; one copy of
  the input array per call.
- `import akantu`: about half of it is `import scipy.sparse`, needed at
  import for `AkantuSparseMatrix` (a subclass of `scipy.sparse.coo_matrix`
  that a star import exports); a lazy import would change those semantics.
- Python material laws and Neumann functors: the time is in the NumPy code
  of the law and in one call per integration point; the binding adds no
  measurable cost.
