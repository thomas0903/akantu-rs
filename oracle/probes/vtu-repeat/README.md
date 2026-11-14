# Probe: full-precision repeatability of multi-rank dumps

The printed outputs compared in `docs/ORACLE_BUILD.md` are rounded.
This probe re-runs the two 2-rank examples (`phase_field_parallel-test`,
`parallel_2d-test`) three times each in isolated scratch work directories and
compares their ParaView VTU dumps **array by array at full binary64
precision**, once with `AKA_SEED=1` and once with `AKA_SEED` unset (Akantu
then seeds the Scotch partitioning from `time(nullptr)`). It also reports
whether a difference is only a permutation of mesh parts among ranks.

```sh
oracle/scripts/run-probe.sh vtu-repeat strict   # -> oracle/results/probe-vtu-repeat-strict.json
```
