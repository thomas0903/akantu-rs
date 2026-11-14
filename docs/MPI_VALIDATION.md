# MPI validation report

| | |
|---|---|
| Purpose | How the port's distributed execution is validated: every distributed model, its rank counts, partitioners, comparison class and worst deviation, where upstream itself depends on the rank count, what is refused on several ranks, the full-size and multi-node runs |
| Platform | x86_64 Linux (D-043), inside the project images (`akantu-rs-mpi:dev`, `akantu-rs-scotch:dev`, `akantu-rs-mumps:dev`; Open MPI 5.0.7, the oracle's version) and the C++ oracle image (strict install, D-050 pins) |
| Evidence | every number below is printed by a test named next to it in the distributed-model tier, or recorded in a committed fixture; other sources are named |
| Not here | the times of the multi-node runs and the strong / weak scaling figures (`docs/BENCHMARKS.md`) |

## 1. How to read this report

**Comparison classes** (strongest first):

| Class | Meaning | Rules |
|---|---|---|
| **B**: bitwise against upstream | the port, given the same partition, reproduces the C++ oracle's per-rank record bit for bit (distribution, schemes, fields, internals) | D-035 (≥ 3 identical oracle runs, pinned image), D-046 (NaN canonicalisation) |
| **B/D-039**: bitwise, global sums by their reduction order | as B, except values that upstream reduces over ranks with `MPI_Allreduce` or in message-arrival order: under `ReductionPolicy::NativeMpi` (real MPI) they are compared bitwise; under the default rank-ordered policy the port's value must equal the rank-order fold of the per-rank contributions exactly (D-039 amendment): the recorded contributions where the probe records them, else the port's own (the explicit energies: `local_energy`), and then the recorded reduced value must equal, bit for bit, the fold of the same contributions in the association observed for the oracle's Open MPI at that rank count (no tolerance in either case) | D-039 and its amendment |
| **E**: D-082 envelope | upstream's own distributed result varies run to run through a solver the port does not run (MUMPS); the port is compared with **every** distinct upstream result of a variation study of at least 12 runs | D-082, bound 1e-14 relative to the maximum |
| **S**: distributed vs serial (port) | the port at P ranks against the port's own serial run, under a stated bound (regression guard or error bound) | per test, `docs/NUMERICAL_VALIDATION.md` |
| **I**: invariants only | consistency checks without a numerical reference (ownership, schemes, global ids, agreed errors) | per test |

**Partitioners.** *Scotch*: upstream's seeded Scotch partition, recorded by
the oracle probe (`partition/linearized`) and handed to the port
(`PrescribedPartition`); the port's own Scotch binding gives the same 2-,
4- and 8-part partitions of `cube.msh` (`scotch_partition_matches_oracle`:
739 / 739 elements in the oracle's part at each count; also
`pbd_strip_scotch_partition_matches_the_oracle` at 2 and 4 parts; feature
`scotch`). *Rule*: a barycentre rule written in the probe
(`MeshPartitionMeshData`) and in the port's test, recorded and compared.
*RCB*: the port's recursive coordinate bisection (no upstream counterpart),
used by the distributed-vs-serial and invariant checks at any rank count.

**Rank counts.** "1 / 2 / 4 / 8" means the comparison ran at each of those
counts in the distributed-model tier (`scripts/mpi/run-par-tier.sh --ranks
"1 2 4 8" --repeat 3`, every binary three times per count; §8) and, for the
oracle classes, against a fixture recorded at that count. A scenario that
does not run at a rank count (no fixture recorded there, a rule defined for
other counts) prints `NOT RUN <name>: <why>` and is not counted as passed
(`SUMMARY … scenarios=S failed=F not_run=K`, S the scenarios that ran), and
every binary fails at start-up when a replayed fixture has no recorded
count.

**The summation order of `MPI_Allreduce`.** Measured for the oracle's Open
MPI 5.0.7 on one node with the default MCA selection: the association
depends on the rank count, rank order at 2 and 3 ranks, `(0+1)+(2+3)` at 4,
mixed trees at 5 to 7, fully pairwise `((0+1)+(2+3))+((4+5)+(6+7))` at 8, for
1 to 100,000 doubles, in the oracle's and the project's image. It is a fact
about the recorded fixtures, not a property of MPI: the port relies only on
the recorded per-rank values and on its own stated order (rank order,
D-039), never on the library's order; the tests use the observed
association only to check data recorded with it, and refuse a rank count
without a measured entry.

## 2. Summary per distributed model

Measured values are the worst over the rank counts listed, from the
distributed-model tier (three identical runs per count; §8), unless another
test is named. "rel." is relative to the field's maximum.

| Model / feature | Ranks | Partitioners | Class | Measured worst deviation | Tests (binary / local world) and fixtures | Upstream rank / run dependence |
|---|---|---|---|---|---|---|
| Mesh distribution | 1 / 2 / 4 / 8 (`distribution_cube_msh`); 1 / 2 / 4 (adv2d, sparse mixed, square data, periodic strips) | Scotch, rule, RCB | B | 0 (bitwise; bounding boxes bitwise) | `mpi_distribution` (`distribution_matches_oracle`, `distributed_bounding_boxes_match_the_oracle`, `distribution_adv2d_matches_oracle_in_compat`, `pbd_strip_*`, `distribution_sparse_mixed_matches_oracle`); `parallel_oracle.rs`; `fixtures/parallel/distribution_*`, `pbd_strip_*` | UD-231 (orphan point, compat), UD-655 / UD-1060 (periodic before distribution) |
| Facet synchronizer | 1 / 2 / 4 / 8 (`facets_cube_n4`); 1 / 2 / 4 (others) | Scotch, rule | B | 0 | `mpi_synchronizers` (`facets_*_matches_oracle`, `v101_facets_*`); `parallel_oracle_synchronizers.rs`; `fixtures/parallel/facets_*`, `v101_facets_*` | UD-251 (mixed facet types: upstream does not complete) |
| Grid synchronizer | 1 / 2 / 4 / 8 (`grid_cube_n4`); 1 / 2 / 4 (others) | Scotch | B | 0 | `grid_*_matches_oracle`, `v101_grid_*`; `fixtures/parallel/grid_*`, `v101_grid_*` | none |
| Periodic meshes and synchronizer | 1 / 2 / 4 / 8 (`periodic_3d_xyz`); 1 / 2 / 4 (five other cases and the adversarial `v101_*` cases) | Scotch, parity rule | B/D-039 (the non-dyadic `_sum`: exact mask of the arrival-order entries, port = rank-order fold) | 0 at every compared entry | `periodic_matches_oracle`, `v101_periodic_matches_oracle`; `fixtures/parallel/periodic_*`, `v101_periodic_*` | D-039 amendment (upstream's periodic `_sum` folds in `waitAny` order), UD-654 (slave order), UD-655 / UD-1060 / UD-1061 |
| Solid mechanics, explicit | 1 / 2 / 4 / 8 (two laws: oracle 1 / 2 / 4) | Scotch (oracle), RCB | B/D-039 (oracle); S (serial) | oracle: nodal fields, time step, law filters and (two laws) internals 0; energies exact at every step (the record = the fold of the port's per-rank contributions in the oracle's observed `MPI_Allreduce` association, the port's value = their rank-order fold; the two associations differ by ≤ 1.861e-16 relative (kinetic) and 1.530e-16 (potential), at 14 / 20 of 40 steps at 8 ranks), NativeMpi 0. Serial (RCB, bound 1e-12): one law \|du\| ≤ 1.179e-18 of 6.835e-4, \|dv\| ≤ 2.392e-18 of 1.312e-3; two laws (finite deformation) \|dv\| ≤ 3.469e-18 of 1.744e-3 | `mpi_smm` (`smm_explicit_matches_oracle{,_native_mpi}`, `smm_explicit_two_laws_matches_oracle{,_native_mpi}`, `explicit_distributed_vs_serial`, `explicit_two_laws_finite_deformation_distributed_vs_serial`); `parallel_oracle.rs`; `fixtures/parallel/smm_explicit_cube_n4` (with an 8-rank record), `smm_explicit_two_laws_cube_n4` | UD-232 (ghost filter order in `waitAny` order: pinned by the probe), D-039 (energies) |
| Solid mechanics, static | oracle 1 / 2; serial 1 / 2 / 4 / 8 | Scotch (oracle), RCB | oracle: displacement within 1e-12 (MUMPS vs faer); S | oracle 1.920e-16; serial (bound 1e-10) \|du\| ≤ 2.776e-17 of 1.452e-1 | `smm_static_matches_oracle`, `static_distributed_vs_serial`; `fixtures/parallel/smm_static_cube_n3` | UD-348: upstream's distributed MUMPS solve is not repeatable at 4 ranks (not recorded there) |
| Solid mechanics, implicit dynamic | 1 / 2 / 4 / 8 | RCB | S | cube (bound 1e-10): \|du\| ≤ 2.439e-19 of 3.289e-4, \|dv\| ≤ 1.518e-18 of 1.243e-3; TEST-109 bars vs serial (bound 1e-12, upstream's criterion 2e-3 on the error): \|dv\| ≤ 1.235e-15 of 6.103e-2 | `implicit_dynamic_distributed_vs_serial`, `smm_bar_dynamics_implicit_vs_serial` (TEST-109) | `_segment_2` at 4 and 8 ranks skipped, as upstream's registration (TEST-109) |
| Cohesive, intrinsic / extrinsic | 1 / 2 / 4 / 8 (`intrinsic_tri`, `extrinsic_tri`); 1 / 2 / 4 (`*_tet`, `extrinsic_igtg_tri`, `facet_stress_tet10`) | rule (probe = port test), RCB | B/D-039 (energies: rank-order fold of the recorded contributions; NativeMpi bitwise) | 0 at every compared quantity | `mpi_cohesive` (`*_matches_oracle{,_native_mpi}`, `invariants_rcb_*`, `invariants_rule_*`); `parallel_cohesive.rs`; `fixtures/parallel_cohesive/*` (8-rank records for the two 2-D scenarios) | UD-890 (upstream's insertion corrupts ghost boundary elements); UD-1280: upstream leaves the accumulated Neumann load of a doubled node with the copy that keeps the original identity, which on a distributed mesh depends on the partition (§7). **With upstream's placement (in the port: the switch `doubled_node_keeps_external_force`) partition independence of distributed cohesive runs with loaded faces is not given, in either code**; the port's default corrects it (D-105 as amended by D-108): shown at h = 0.06 for 2 / 4 / 8 ranks; at full size (64 and 32 ranks, §7) no partition-dependent placement within the energies' detection window (≈ 2,700 steps; D-108 1 (b) not met as written, its magnitude bound) and every copy's load equal to a re-application at every step to 3,200 (0 of 3,201 checks failed), for the benchmark's equal nodal shares; unequal shares followed by node identity (1 / 2 / 4 ranks; the full-size runs predate that correction and describe it to rounding); UD-349 |
| Cohesive, facets-only, clusters, fragments | 1 / 2 / 4 | rule | B/D-039 (fragment integrals as energies) | 0 | `facets_only_*_matches_oracle`, `clusters_*`, `fragments_tri_*` (both policies); `fixtures/parallel_cohesive/facets_only_*`, `clusters_*`, `fragments_tri` | UD-1043 (doubled shared node: upstream asserts) |
| Cohesive, independent checks | 1 / 2 / 3 / 4 (local world), 1 / 2 / 4 / 8 (par tier) | RCB, rule (2 / 4; 8 in 2-D) | I + S (dissipated energy vs serial within 1e-12) | asserted bound held (the test prints no value) | `invariants_hold_for_every_scenario`, `invariants_rcb_*`, `invariants_rule_*` | none |
| Diffusion | gradient: 1 / 2 / 4 / 8; serial: 1 / 2 / 4 / 8 | Scotch (oracle), RCB | B (gradient); S | gradient 0; serial: explicit (1e-13) \|dTdot\| ≤ 1.634e-13 of 9.151e1, implicit (1e-12) ≤ 3.264e-14 of 1.345, static (1e-14) \|dT\| ≤ 7.105e-15 of 1.0e2 | `diffusion_gradient_matches_oracle`, `diffusion_{explicit,static,implicit}_distributed_vs_serial`; `fixtures/parallel/diffusion_gradient_cube_n3` (with an 8-rank record) | UD-275: upstream's distributed diffusion model throws at its first solve (compat reproduces it) |
| Phase field and coupler | oracle 1 (B), 2 / 4 (E); serial 1 / 2 / 4 / 8 | Scotch, RCB | B (1 rank), E (2 / 4), S | E: worst over every recorded upstream result 3.55e-15 (`law phi`, eigen side; bound 1e-14); serial (1e-14): \|dd\| ≤ 2.220e-16 of 5.224e-1, \|du\| ≤ 8.674e-19 of 8.0e-3 | `mpi_phase_field`, `phase_field_parallel.rs`; `fixtures/parallel/phase_field_square_n8` (np2 / np4: r1, r2) | D-082: upstream's distributed result takes two distinct values at 2 and at 4 ranks (MUMPS) |
| Marigo, lumped mass with `degrade_mass` | 1 / 2 / 4 / 8 | RCB | S (1e-14) | Marigo \|du\| ≤ 8.674e-19 of 6.306e-4; mass \|dm\| ≤ 6.939e-18 of 4.553e-2 | `marigo_distributed_vs_serial`, `phasefield_degrade_mass_lumped_vs_serial` | none |
| CL-Marigo (`clh_marigo`) | 1 / 2 / 4 | Scotch | B | 0 | `clh_marigo_matches_oracle`; `fixtures/parallel/clh_marigo_cube_n3` | one random generator per process (recorded per rank) |
| Non-local laws | oracle 1 / 2 / 4 / 8 (`nl_region`, `nl_damage_wf`); 1 / 2 / 4 (`nl_mazars`, `nl_mixed`; bitwise at 8 against a staged oracle run, §3); 1 / 2 (`nl_pair`); 1 (`nl_mazars_two`); 3 (`nl_mazars_two_strips`) | Scotch, strips | B under the compat switches (UD-1101 / UD-1102 / UD-1103); S with the corrected defaults | oracle 0; corrected defaults vs serial: one neighbourhood (bound 1e-14) 1.382e-15, mixed laws' owned internal force (1e-14) 1.098e-15, two neighbourhoods damage (1e-14) 2.381e-15 and internal force (1e-12) 1.722e-14, quadrangles 6.409e-16, extra laws (1e-14) ≤ 4.340e-16; UD-1103's upstream exchange 4.272e-2 from serial (the reason for the corrected default) | `mpi_smm` (`nl_*_matches_oracle`, `nl_region_default_vs_serial`, `nl_*_vs_serial`, `nl_key_order_refused_on_a_distributed_mesh`), `mpi_xmat`; `non_local_parallel.rs`; `fixtures/parallel/nl_*` | UD-1100 (upstream aborts with two neighbourhoods on several ranks: reproduced at 8 ranks, §3 F2), UD-1101…1104 |
| Split-node contact | 1 / 2 / 4 / 8 | strips (probe = port) | B/D-039 (the external work: per-rank works bitwise, the port's value the rank-order fold of the recorded works exactly, the recorded value their fold in the observed association exactly; NativeMpi bitwise) | 0; external work at 8 ranks: rank-ordered 1 ulp from upstream's `MPI_Allreduce` (both folds exact), NativeMpi 0 | `mpi_ntn` (`ntn_compression_matches_oracle{,_native_mpi}`, `ntrf_coulomb_matches_oracle`, `ntn_update_normals_matches_oracle`); `ntn_parallel.rs`; `fixtures/parallel/ntn_*`, `ntrf_coulomb` | UD-1105 (per-rank pairing; partitions that separate the interface are refused), D-039 (`getExternalWork`'s `allReduce`) |
| PETSc back-end | 1 / 2 / 4 (PETSc tier; not re-run with the MPI campaign) | partitioned bar, distributed models | S (vs serial Eigen; `pc_type none` models vs one rank) | not re-measured here; asserted bounds: PETSC-DIRECT 1e-12, `VS_ONE_RANK` 1e-14 (`petsc_models.rs:380`) | `petsc_mpi.rs`, `petsc_models.rs` (PETSc tier, `docs/TESTING.md`) | UD-006 / UD-429: upstream's parallel PETSc path is unfinished, so there is no multi-rank oracle |
| MUMPS back-end | 1 / 2 / 4 (MUMPS tier: pass) | partitioned bar (contiguous parts) | S (vs serial Eigen, identical Newton iterations, 1e-12) | static linear 9.592e-15, static Newton 0, implicit Newmark 2.864e-16; oracle's Eigen run of `test_model_solver_mumps` 1.110e-17 absolute | `mumps_mpi` (three thread levels), `mumps_mpi_solver`, `perf_cohesive_mumps`, the MUMPS regression tests (MUMPS tier) | UD-348 (multi-rank MUMPS not repeatable run to run upstream) |
| Python module, mesh level | 1 / 2 / 4 (not run at 8) | each code's own seeded Scotch on rank 0 (`Mesh.distribute()`) | B (tolerance 0 against the oracle's MPI-enabled module) | 0: 7 / 7 rank outputs equal (two meshes, 2-D plate with a hole and 3-D cube: nodes bitwise, connectivities, groups, bounds, global ids; three identical oracle runs); 25 / 25 lifecycle runs pass | `python/oracle/mpi/run.sh` (`mesh_distribute.py`, `lifecycle.py`), `cargo test -p akantu-python --features mpi` (par tier); `docs/TESTING.md`; not re-run with the 8-rank campaign | upstream never runs its Python module on several ranks (D-101); models on several ranks from Python are not covered (D-101: model level not planned) |

## 3. The 8-rank oracle cases

The oracle comparisons of the distributed binaries (`mpi_distribution`,
`mpi_synchronizers`, `mpi_smm`, `mpi_diffusion`, `mpi_cohesive`, `mpi_ntn`)
follow the rank counts the probe recorded
(`fixtures/<area>/<scenario>/np<P>`); a scenario whose rule exists only for
some counts (the adv2d owner counts, the periodic strips of UD-1061, the
cohesive rules in 3-D) keeps its explicit set. The local-world tests run the
same scenarios at every recorded count and name the expected counts per
scenario.

The 8-rank cases were recorded with the existing probes on the pinned
x86_64 oracle (`OPENBLAS_CORETYPE=Haswell`, Scotch seeded with one thread),
three identical runs each: 104 rank records, 12,066,434 bytes in total, each
under the 1 MiB case limit. Two probe versions were needed:
`parallel_cohesive` version 4 (the 2-D partition rule at 8 ranks; the
port's test computes the same rule itself, `par_coh::part_2d_8`, and
compares it with the recorded partition) and `par4` version 2 for
`ntn_compression` (records each rank's `external_work`, see F1).

| Scenario | Probe | Partition at 8 | Size | Class at 8 | Result at 8 (real MPI, 3 / 3 runs; local world) |
|---|---|---|---|---|---|
| `distribution_cube_msh` | parallel | upstream Scotch | 957,217 B | B | bitwise, bounding boxes bitwise |
| `facets_cube_n4` | parallel | upstream Scotch | 1,402,358 B | B | bitwise (both flip modes) |
| `grid_cube_n4` | parallel | upstream Scotch | 571,817 B | B | bitwise |
| `periodic_3d_xyz` | parallel | upstream Scotch | 533,583 B | B/D-039 | bitwise; non-dyadic `_sum` by the exact mask |
| `smm_explicit_cube_n4` | parallel | upstream Scotch | 401,846 B | B/D-039 | fields bitwise; energies exact at every step from the per-rank contributions (the port's rank-order bits differ from the record at 14 (kinetic) / 20 (potential) of 40 steps, ≤ 1.861e-16 / 1.530e-16 relative), NativeMpi bitwise |
| `diffusion_gradient_cube_n3` | parallel | upstream Scotch | 283,201 B | B | bitwise |
| `intrinsic_tri` | parallel_cohesive v4 | 2-D rule at 8 | 1,617,676 B | B/D-039 | bitwise, both reduction policies |
| `extrinsic_tri` | parallel_cohesive v4 | 2-D rule at 8 | 1,873,663 B | B/D-039 | bitwise, both reduction policies |
| `nl_region` | par4 | upstream Scotch | 1,535,069 B | B (compat) | bitwise (local world and `mpi_smm`) |
| `nl_damage_wf` | par4 | upstream Scotch | 1,455,598 B | B (compat, `WeightExchange::Upstream`) | bitwise (local world and `mpi_smm`) |
| `ntn_compression` | par4 v2 | strips | 570,111 B | B/D-039 | bitwise; external work: finding below |
| `ntrf_coulomb` | par4 | strips | 514,492 B | B | bitwise |
| `ntn_update_normals` | par4 | strips | 349,803 B | B | bitwise |

Not recorded at 8 (size, or a rule defined for fewer ranks): the other
distribution / synchronizer cases, the two-laws explicit case, the 3-D and
grain-selector cohesive cases (no 3-D rule at 8), facets-only / clusters /
fragments, `clh_marigo`, `nl_mazars` and `nl_mixed` (replayed bitwise at 8
against a staged oracle run, not committed), the phase field (class E).

**Finding F1 (D-039, not a port defect): `ntn_compression`'s external work
at 8 ranks.** Every recorded quantity replays bitwise except
`final/external_work`, the work of the contact and friction forces summed
over the ranks by upstream's `getExternalWork()` (`allReduce(_sum)` =
`MPI_Allreduce`): the port's rank-ordered sum is
`[-1.1486667077725262e-8, 4.037665515531106e-12]`, upstream's
`[-1.1486667077725264e-8, 4.0376655155311055e-12]` (one ulp in each
component). Upstream's eight per-rank works (recorded by probe version 2)
equal the port's bit for bit; upstream's recorded value equals bit for bit
their fold in the association observed at 8 ranks,
`((w0 + w1) + (w2 + w3)) + ((w4 + w5) + (w6 + w7))`, and their rank-order
sum is the port's value. At 2 ranks the two associations are the same; at
4 ranks the observed one, `(w0 + w1) + (w2 + w3)`, is not the rank order,
and for this case it happens to give the same bits. Handled in the form of
the D-039 amendment, exactly, with no tolerance: the port's per-rank work
is compared bitwise with the recorded one, the recorded global value must
equal the fold of the recorded per-rank works in the observed association
(also in the fast tier), and where the port's global value differs it must
equal the rank-order fold of the recorded per-rank works
(`external_work_matches`, `par_ntn/mod.rs`);
`ntn_compression_matches_oracle_native_mpi` (real MPI,
`ReductionPolicy::NativeMpi`) reproduces upstream's value bitwise at 1 / 2
/ 4 / 8 ranks.

**Finding F2 (upstream, UD-1100 at 8 ranks).** The two cases UD-1100
records as aborting at 2 / 4 ranks abort at 8 as well: `nl_pair` exits 1
with an assertion on every rank (ranks 0 to 3 "A local quadrature_point … as
been removed instead of just being renumbered", ranks 4 to 7 out-of-range
ghost jacobians), the two assertion families UD-1100 records at 4 ranks;
`nl_mazars_two` asserts on ranks 0 / 1 / 4 / 5 while the other ranks wait in
a collective, and is killed by the 300 s timeout. No fixture; the port's
compat outcome (`nl_ud1100_compat_outcome_is_agreed`, agreed error on every
rank) and its corrected default are unchanged.

## 4. Where upstream itself depends on the rank count or the run

| Item | What depends on what | How the port is compared |
|---|---|---|
| D-039 (collective sums) | the association of `MPI_Allreduce` observed for the oracle's Open MPI 5.0.7 (one node, default MCA) depends on the rank count: rank order only at 2 and 3 ranks, `(0+1)+(2+3)` at 4, mixed at 5 to 7, fully pairwise at 8 (§1); the contact work happens to give the same bits at 2 and 4 ranks, the explicit energies differ at 4 and 8 | NativeMpi bitwise; rank-ordered exactly: the port's value against the rank-order fold of the per-rank contributions, the recorded value against their fold in the observed association (no tolerance) |
| D-039 amendment (synchronizer sums) | the periodic `_sum` and other slave → owner sums fold in message-arrival (`waitAny`) order | exact mask of the order-dependent entries, port = rank-order fold of the recorded contributions |
| UD-232 | ghost elements join a law's filter in `waitAny` order, so the assembly order of ghost contributions varies run to run | the probes rewrite the ghost filters into rank order after checking the move carries no state |
| UD-348, D-082 | multi-rank MUMPS solves are not repeatable run to run (static SMM at 4 ranks, phase field at 2 / 4) | static: recorded at 1 / 2 only; phase field: envelope over every distinct upstream result |
| UD-655 / UD-1060 / UD-1061 | a mesh made periodic before the distribution crashes upstream when a relocated rank holds a periodic slave (which ranks are relocated depends on the rank count); nodal data of missing partners are read past the buffer | port corrects (A); strip cases at 1 / 2 / 4 avoid the crash |
| UD-890 | upstream's distributed extrinsic insertion overwrites ghost boundary elements, so its later Neumann loads differ from the serial problem at P > 1 | port keeps the serial problem; comparisons with upstream at P > 1 stop before the first ghost-touching insertion |
| Load placement on doubled nodes (D-105 / D-108, UD-1280) | upstream: which copy of a doubled node keeps the accumulated Neumann load follows the facet orientation, which on a distributed mesh depends on the partition (§7). The port corrects this by default (the load follows the boundary elements; switch `doubled_node_keeps_external_force` for upstream's placement) | corrected default: the per-copy force at 2 / 4 ranks equals serial within 2.7e-16 of the largest nodal load, and the h = 0.06 benchmark keeps the serial insertion list at 2 / 4 / 8 ranks for 2,500 steps (`docs/NUMERICAL_VALIDATION.md`, "Cohesive model"); with the switch, as in upstream: not a correctness criterion at P > 1 |
| UD-349 | upstream's benchmark blocks the **local** node 3 of every rank: every rank count solves another problem | harness option `blocked_node = global` |
| UD-1100 | two non-local neighbourhoods abort upstream on several ranks (2, 4 and 8 ranks, F2) | compat returns the same agreed error; corrected default compared with serial |
| UD-1105 | split-node pairs form on each rank only, so the contact depends on the partition | supported partitions keep both sides on the same ranks; others refused |
| UD-275 | upstream's distributed diffusion model throws at its first solve | corrected; compat reproduces the failure |

## 5. Lifecycle and error propagation

`akantu-comm`'s real-MPI binaries (MPI tier, `scripts/mpi/run-tests.sh`):
`mpi_acceptance` (rank-specific failures at every phase boundary, serial ≡
local world ≡ MPI-N, rendezvous messages, park / resume), `mpi_abort_paths`
(transport failures turned into an `MPI_Abort` with per-rank evidence,
thread-level-legal aborts), `mpi_lifecycle` (externally initialised MPI,
host-first imports, finalisation choreography), `mpi_progress`,
`mpi_self_messages`, and the exit hook of D-067 (repeated-run campaign
`scripts/mpi/run-exit-hook-campaign.sh`). In the distributed-model tier the
`funneled_self_wait_on_worker_{early,late}` lifecycle scenarios run at 1 and
2 ranks. Every distributed model's binary also carries agreed-failure
scenarios (a failure on one rank returns the same recoverable error on
every rank, the communicator usable afterwards): e.g.
`distribution_failure_agreed`, `distributed_facets_failure_agreed`,
`grid_synchronizer_failure_agreed`, `periodic_failure_agreed`,
`pa01_*` / `pa2_*` (`mpi_smm`), `diffusion_rank_specific_*`,
`contact_sums_failure_is_agreed`, `rank_specific_contact_step_failure_is_agreed`,
`nl_*_failure_is_agreed`. Some binaries still contain per-rank verdicts
before a collective call, which would hang a run instead of failing it if a
rank disagreed. Results: §8.

## 6. What is refused on several ranks, and why

| Feature | Behaviour on a distributed mesh / several ranks | Why | Evidence |
|---|---|---|---|
| Contact mechanics model (generic contact search / resolution) | recoverable refusal | CONT-019 is an approved exception (D-089): upstream's contact model and couplers are serial only, no reference | `doc012_contact_model_refuses_a_distributed_mesh` (`crates/akantu/tests/manual_claims_2.rs`) |
| Structural mechanics model | refused at construction (`Error::Unsupported`) | UD-902 (A): upstream registers no synchronizer and skips the ghost elements' forces, so its residual is wrong at every interface node | `st05_serial_only_refusals` (`review_v76_regressions.rs`) |
| Parallel IGFEM | not provided; serial IGFEM complete | D-085 (approved exception): upstream's parallel IGFEM uses removed APIs and does not compile at the reference (UD-609) | parity rows IGFEM-004 / 005, TEST-192 / 203 |
| Distributed 1-D facet mesh (1-D cohesive model) | refused on every rank, mesh and communicator usable afterwards | UD-1141: upstream throws in `flipFacets` (no `subelement_to_element` below `_point_1` facets) | `distributed_1d_facet_mesh_refused` (`mpi_cohesive`), `two_ranks_1d_facet_mesh_refused` (`upstream_cohesive.rs`) |
| Non-local summation route `KeyOrder` | refused at `init_full` on a distributed mesh: on every rank `Error::Unsupported` naming `KeyOrder`, the same message, before any change of the model or the mesh; **recoverable** (agreed in the selection phase of `init_materials`, before the selection is committed): set another route and call `init_materials` on the same model, which then runs as a model on a freshly distributed mesh | D-097 (1): its key uses `QuadPoint::global_element`, which a distributed mesh does not fill with partition-independent ids | `key_order_is_refused_on_a_distributed_mesh` (`non_local_parallel.rs`, local world 1 / 2 / 4 / 8: the refusal, then `init_materials` of the same model with the default route and its first assembly bitwise against a fresh model; at one rank on a mesh that is not distributed `KeyOrder` is accepted), `nl_key_order_refused_on_a_distributed_mesh` (`mpi_smm`, real MPI) |
| Split-node contact on partitions that separate an interface pair, or where a rank sees one slave node | refused at `init_parallel`, agreed, recoverable | D-097 (2), UD-1105 (A for these cases): upstream silently drops pairs or pairs with an arbitrary master | `separated_partition_is_refused` (`mpi_ntn`, `ntn_parallel.rs`), `parallel::tests` |
| Split-node contact without `init_parallel` | refused | UD-904 | `distributed_contact_without_init_parallel_is_refused` |
| Phase-field element events on a distributed mesh | refused | events that would need a distributed renumbering | `pf_distributed_element_events_refused` (`phase_field_parallel.rs`) |
| Python module on several ranks, model level | not planned | D-101: no bitwise reference (upstream's Eigen solver asserts one rank, MUMPS varies) and upstream never runs it in parallel; the mesh level is ported (§2, last row) | D-101 |

## 7. The full-size run (C1)

The full-size cohesive benchmark (JOSS mesh, 5,000 steps) ran on one AWS
node, five of six points (the port at 64 / 32 / 8 ranks, upstream at 64 /
32; tables in `docs/BENCHMARKS.md`). Its results depend on the rank count in
both codes. Where the runs first differ: the static solve differs between
rank counts but is not the trigger, the dumped quantities are not computed
differently, and the explicit phase's summation order depends on the
partition by construction. An experiment on the h = 0.06 rehearsal mesh
(one 8-rank partition, 2,500 steps) found the deterministic cause: **the
accumulated Neumann load of a doubled node stays with the copy that keeps
the original identity**, and which copy that is follows from the
orientation of the doubled facet, which on a distributed mesh depends on the
partition. With the serial static solution imposed, the 8-rank run agrees
with the serial run to rounding until the first insertion that orients an
interface facet the other way; rebuilding the external force on the
current topology removes the departure over the 2,500 steps. Upstream does
the same, bit for bit. That experiment covered one mesh, one partition and
2,500 steps; the original JOSS runs at 64 and 32 ranks show the signature
of this mechanism (first departure in the work of the loads, step 1,122),
which was not proven on them. The port corrects the placement by default
with a compat switch (UD-1280, D-105 as amended by D-108).

Consequence: **partition independence of distributed cohesive runs with
loaded faces is not given in upstream, nor in the port with the switch for
upstream's placement**. With the port's corrected default it is shown at
h = 0.06 for 2 / 4 / 8 ranks (the serial insertion list for 2,500 steps;
serial against 8 ranks, each energy within 4.5e-12 of the largest energy of
the step, D-108's bound being 1e-10). No final count or energy is compared
between rank counts or between the two codes as a correctness criterion;
the cohesive oracle scenarios of §2 / §3 (bitwise at a fixed partition) and
the port's invariants drive the bodies by prescribed velocities and
displacements, without Neumann loads (`par_scenarios::Loading`), so they do
not exercise this placement.

**The full-size rerun with the corrected placement (2026-11-13):** the port
only, 64 and 32 ranks on one node. The counts are identical for 3,176 steps
(152,520 elements) and end 4 apart (471,932 against 471,936: 8.5e-6). Every
energy stays within 1e-12 of the step's largest energy until step 2,422,
then grows by amplification without a one-step event in the external work,
and the energies end at most 3.5e-5 apart. D-108's parts 1 (a) and 1 (c)
are met. **1 (b) is not met as written**: the external work's difference
first exceeds 1e-10 of the step's largest energy at step 2,716, before the
counts part (its magnitude bound), while the jump criterion holds (× 63 at
most, one step after a smooth zero crossing). The comparison shows no
partition-dependent placement up to about step 2,700 (its detection
window: an injected event of the original size stands out at any onset up
to step 2,689); it cannot show a placement error after that, one too small
to affect the work, or one common to both partitions. **Control:** the same
binary on the same 64-rank partition, one node against eight hosts (no
placement can differ), leaves 1e-10 at step 2,656 against the rank-count
pair's 2,653, and its counts part at step 3,053; the 32-rank pair at step
2,648 and 3,177. The divergence of the two rank counts is amplified
rounding. **The per-copy check at full size:** at every step from 0 to
3,200 and on every rank, every copy's external force equals a re-application
of the loads on the current mesh within 2.57e-15 of the largest nodal load,
at 64 and at 32 ranks: 3,201 checks each, 0 failed (bound 1e-12), with
12,174 and 12,172 doubled loaded nodes at step 3,200. It proves the
placement mechanism through step 3,200 (not the boundary connectivity or the
traction integration, which the check shares with the run), and only for
boundary elements whose nodal shares are equal within an element, as the
benchmark's are; with unequal shares the placement was corrected afterwards
(records kept by node identity, verified at 1 / 2 / 4 ranks), so these runs
describe the corrected code to rounding. Figures: `docs/BENCHMARKS.md`.

## 8. Runs

The values of §2 and §3 come from the distributed-model tier at 1 / 2 / 4 /
8 ranks × 3 on x86_64, inside the project images (containers capped at 4
CPUs / 6 GiB); the measured values printed by the tests (`INFO` lines) are
identical in the three runs of every binary and rank count, only the
interleaving of the ranks' output differs. Final result: clippy
(`mpi,scotch`) clean, the port's Scotch against the oracle 8 / 8,
`akantu-python --features mpi` clippy and tests, 100 / 100 runs pass: every
one of the eight binaries 3 / 3 at 1, 2, 4 and 8 ranks, and the 4 lifecycle
runs. The SUMMARY lines (S scenarios that ran / K `NOT RUN`):

| Binary | 1 rank | 2 ranks | 4 ranks | 8 ranks |
|---|---|---|---|---|
| `mpi_distribution` | 22 / 1 | 21 / 2 | 21 / 2 | 15 / 8 |
| `mpi_synchronizers` | 23 / 2 | 25 / 0 | 25 / 0 | 17 / 8 |
| `mpi_smm` | 46 / 10 | 55 / 1 | 53 / 3 | 48 / 8 |
| `mpi_diffusion` | 7 / 2 | 9 / 0 | 9 / 0 | 9 / 0 |
| `mpi_cohesive` | 30 / 5 | 35 / 0 | 35 / 0 | 13 / 22 |
| `mpi_phase_field` | 6 / 4 | 9 / 1 | 9 / 1 | 8 / 2 |
| `mpi_ntn` | 7 / 2 | 9 / 0 | 9 / 0 | 9 / 0 |
| `mpi_xmat` | 3 | 3 | 3 | 3 |

At 8 ranks `mpi_smm` runs `smm_explicit_matches_oracle` (kinetic 14,
potential 20 of 40 steps differing in bits from the record, max rel
1.861e-16 / 1.530e-16, every step exact by the folds),
`smm_explicit_matches_oracle_native_mpi`, `nl_region_matches_oracle`,
`nl_damage_wf_matches_oracle` and `nl_key_order_refused_on_a_distributed_mesh`;
its eight `NOT RUN` lines are the oracle replays without an 8-rank fixture
(`nl_pair`, `nl_mazars`, `nl_mixed`, `nl_mazars_two_strips`, the two-laws
case under both policies, the static case, `clh_marigo`); the two
phase-field oracle scenarios are `NOT RUN` at 8 ranks with their reason. At
4 ranks the energies give 10 / 9 differing steps (one law) and 7 / 10 (two
laws), max rel 1.861e-16 / 1.530e-16 and 1.239e-16 / 9.934e-17, every step
exact by the folds. The MPI tier (`akantu-comm`'s real-MPI scenarios at 1 /
2 / 4 ranks) and the MUMPS tier pass on the same code.

## 9. Multi-node execution and scaling

One MPI job across hosts was run once, on 2026-11-13: 8 AWS `r7a.2xlarge`
hosts in one cluster placement group, ranks bound to cores, the rerun's
binary and settings, one run per point (launcher and comparator:
`crates/akantu-bench/scripts/mn_cluster.py`, `mn_session.py`,
`mn_compare.py`). The smoke session passed: with the Eigen static solve, 8
ranks on 8 hosts give `energies.csv` and `insertions.csv` byte-identical to
8 ranks on one host (1,500 steps, 416 cohesive elements); with MUMPS the two
insert the same elements and their energies differ by at most 1.0e-14 of the
step's largest energy; the first 64-rank job across the eight hosts ended
`ok`. The main session ran 13 of 13 runs `ok`: the full JOSS problem at 64
and 32 ranks across the eight hosts on C1's partitions, with the
distribution of the single-node run of the same binary (64 ranks:
placement, partition, per-rank counts, the static matrix and MUMPS's
settings identical; 32 ranks: the partition, the static matrix and MUMPS's
settings identical) and the energies and counts of a run that differs from
it by MUMPS's rounding only (the control of §7); h = 0.04 at 8 ranks on 1,
2, 4 and 8 hosts; h = 0.04 at 16, 32 and 64 ranks on 2, 4 and 8 hosts; and a
weak-scaling series at about 70,000 tetrahedra per rank up to the JOSS mesh
at 64 ranks. The per-copy check at full size (§7) ran on the same hosts.
Times, scaling and the comparison with one node: `docs/BENCHMARKS.md`; no
performance claim is made here (D-096). Not covered: more than 8 hosts or 64
ranks, other interconnects, and NativeMpi bitwise expectations across hosts
(the observed `MPI_Allreduce` association is a one-node fact; no oracle was
recorded across hosts). Every other run of this report is on one node.

## 10. Limits

- Still at 1 / 2 / 4 ranks only (fixture size, or rules defined for fewer
  ranks): the oracle scenarios listed under §3 "Not recorded at 8"; the 3-D
  cohesive scenarios have no 8-rank rule.
- The PETSc tier was not re-run with this campaign; its multi-rank values
  are not reproduced here, only its asserted bounds.
- The Python module's mesh-level MPI harness needs the oracle's MPI-enabled
  module and was not re-run with this campaign.
- The full-size rerun's D-108 statement is not met in its part 1 (b) as
  written (§7).
