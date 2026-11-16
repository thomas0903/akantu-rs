<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# State digests, before (`0769542f6`) and after (`74e97f334`)

- `serial-digests.txt` (`scripts/dig.sh`): one run of each binary per input;
  `digest` is the first 16 hex digits of the SHA-256 of `energies.csv`
  (every step, 17 digits), `insertions.csv` and the nodal state of the
  last step (`rust_diag_*` output, `diag/*.csv`), then the SHA-256 of
  `energies.csv` alone and the last line of `insertions.csv`. Inputs:
  `dig-5000` (h = 0.12, full schedule, `output_energy = true`,
  `rust_diag_node_steps = rust_diag_from = 5000`) and `dig06-200` (the
  same on `cube-h0.06.msh`, 200 steps, `rust_static_solve = false`).
- `mpi-eigen-digests.txt` (`scripts/parbit.sh` layout): the full schedule
  at 2, 4 and 8 ranks with `output_energy = true` and
  `rust_sparse_solver = eigen` (the MUMPS static solve does not repeat from
  run to run); SHA-256 prefix of `energies.csv` and `insertions.csv`, and
  the last line of `insertions.csv`.

The same serial digest (`0774bbce41511cc1` at h = 0.12) was obtained at
every intermediate commit of the branch that was run, and the same three
MPI digests at two intermediate commits (`3b911394`, `27da8795`); the
set-up and solid suite digests are identical before and after
(`../series/suite-*/`). The energy and insertion files themselves are not
kept (about 0.5 MB per run; identical between before and after).
