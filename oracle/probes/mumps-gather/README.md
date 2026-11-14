# Probe: `test_sparse_solver_mumps` multi-rank gather (UD-075)

Upstream `test/test_solver/test_sparse_solver_mumps.cc` initialises the input
array `x` to −1, solves with MUMPS, then rank 0 calls
`sync.gather(y, x_gathered)` on the **solution** `y` while the other ranks call
`sync.gather(x)` on the **unsolved input** — so rank 0 checks −1 for every
entry owned by another rank (`Error in the solution: -1 != 7` on 2 ranks).

`probe.cc` is that test with one change selectable at compile time:
`probe_original` (`-DORACLE_PROBE_ORIGINAL`) is the upstream test,
`probe_corrected` gathers `y` on non-root ranks. Both link the installed oracle
library (no rebuild of Akantu). Run from the repository root:

```sh
oracle/scripts/run-probe.sh mumps-gather strict   # -> oracle/results/probe-mumps-gather-strict.json
```

Result (`oracle/results/probe-mumps-gather-strict.json`): see
`docs/ORACLE_BUILD.md`. This establishes a test defect for this small
distributed solve/gather; it does not verify all MUMPS functionality.
