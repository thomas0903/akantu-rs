# The per-copy load check at mid sizes (2026-11-13)

A check mode of the port's cohesive driver (`crates/akantu-bench/src/load_check.rs`)
that compares, at chosen steps, every node's external force with a fresh
re-application of the loads on the current mesh. It tests where the
accumulated load of doubled nodes is placed after insertions (UD-1280,
decisions D-105 and D-108). This directory holds its evidence at the sizes
an 8-vCPU virtual machine allows; the full-size runs are in
`docs/benchmarks/mn-2026-11-13/percopy/`.

## The mode

Two keys of the input's user section, both off by default (without them
the driver writes exactly what it wrote before):

- `rust_load_check_every = K`: a check at every step that is a multiple of
  K, step 0 (after the static solve, before the loop) included;
- `rust_load_check_steps = LIST`: further steps (`683, 700:800:10`).

A check at step s runs after the step's insertion and before its solve. On
every rank the run's loads (compression on "top" and "bottom", the initial
shear and the shear increments so far) are applied again into a scratch
array with the model's own boundary engine, and the external force is
compared with it at every node the rank holds (local, master, slave and
pure ghost copies). The bound is 1e-12 of the largest nodal load (D-105);
the smallest physical misplacement, one element's share of one load, is at
least 3e-5 of it. Output: `load_check.csv` (rank 0, one row per check, with
the verdict), `load_check.r<rank>.csv`, a `rust_load_check` block in
`summary.json` and a `load_check` Chrono phase. A failed check never stops
the run.

## The runs

x86_64 Linux virtual machine (boat.dev `large`), image `akantu-rs-mumps:dev`, release
driver; `crates/akantu-bench/scripts/percopy_campaign.sh` with
`material-cohesive.dat`, `blocked_node = global`, `rust_cohesive_compat =
upstream`, either placement (`rust_doubled_node_load = follows_elements` or
`upstream`), the driver's own Scotch partition (`AKA_SEED = 1`), 2,500
steps, a check at every step (2,501 checks per run). h = 0.12: the frozen
mesh, Eigen static solve (deterministic); h = 0.06: the rehearsal mesh, the
serial run's MUMPS static solution imposed in every run, so every rank count
starts from the same state. Each corrected run has a twin without the keys
(`-plain`).

`analysis.txt` is the output of `crates/akantu-bench/scripts/percopy_analyze.py`
on the 21 runs (tested by `tests/test_percopy_analyze.py`).
`runs/files.sha256` lists the SHA-256 of every run file; the run directories
themselves are not kept.

## Results

- The corrected placement passes every check (13 runs, 32,513 checks): the
  largest difference is 1.836e-15 (h = 0.12) and 1.976e-15 (h = 0.06) of the
  largest nodal load at 1, 2, 4 and 8 ranks; no move from a written node and
  no foreign write. The insertion histories are identical at every rank
  count of a mesh.
- Upstream's placement fails at the first doubled loaded node (7 runs): at
  step 838 (h = 0.12) and 892 (h = 0.06) the original copy keeps about twice
  its re-applied load (0.343 and 0.388 of the largest nodal load), and 1,663
  and 1,609 of 2,501 checks fail. With it the insertion histories depend on
  the rank count.
- The check changes nothing of the run: with and without the keys
  `energies.csv` and `insertions.csv` are byte-identical at 2, 4 and 8 ranks
  on both meshes. A check costs 9–14 shear increments, 5–20 % of a step.
- Before the first insertion that doubles a node, the force and the
  re-application agree bit for bit. After it about 700 (h = 0.12) and 2,700
  (h = 0.06) nodes differ by rounding: that insertion rewrites boundary
  triangles as cyclic rotations of the same nodes, and with one quadrature
  point the centroid's shape values differ by one ulp, so a re-application
  gives the rounding to another node.

Not covered: the per-copy force against the serial run (the re-application
does not depend on the partition), boundary elements whose nodal shares
differ within an element (these runs use `_triangle_3` with one point; such
meshes are tested by the driver's own tests), and the steps after the last
check.
