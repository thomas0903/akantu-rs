# The JOSS paper's claims

Upstream presents Akantu in a paper for the *Journal of Open Source
Software*: N. Richart, G. Anciaux, E. Gallyamov et al., *Akantu: an HPC
finite-element library for contact and dynamic fracture simulations*
(`joss/paper.md` of the reference tree, dated February 2023). This
appendix takes the paper's claims one by one and names the parity rows of
[`docs/FEATURE_PARITY.md`](../../../FEATURE_PARITY.md) and the tests that
carry each claim in the port (parity row DOC-019).

The status of every row this page cites is in the table
[Status of the cited rows](#status-of-the-cited-rows) at the end, generated
from the parity matrix: `manual_generated_sections_are_current` (run by
`cargo test -p akantu --test manual`) fails when a row's status changes
and the table is not regenerated, and
`joss_claims_page_cites_existing_rows_and_tests` fails when the page cites
a row the matrix does not have or a test that does not exist. The
statuses are the matrix's own: *Verified* means that an independent review
accepted the row on reproducible evidence; the matrix's header explains
the others.

## How to cite

The paper was published in 2024; it is the reference upstream asks its
users to cite (`doc/dev-doc/manual/cite-akantu.rst`). The port asks the
same, and to cite the port's software record as well when a text refers
to the Rust port itself (its `README.md` and the machine-readable
`CITATION.cff` at the root of the repository):

Richart, N., Anciaux, G., Gallyamov, E., Frérot, L., Kammer, D.,
Pundir, M., Vocialta, M., Ramos, A. C., Corrado, M., Müller, P.,
Barras, F., Zhang, S., Ferry, R., Durussel, S. and Molinari, J.-F.
(2024). Akantu: an HPC finite-element library for contact and dynamic
fracture simulations. *Journal of Open Source Software*, 9(94), 5253.
[doi:10.21105/joss.05253](https://doi.org/10.21105/joss.05253)

In BibTeX (upstream's `cite-akantu.bib`):

```text
@article{richart_akantu_2024,
  title = {Akantu: an {HPC} finite-element library for contact and
           dynamic fracture simulations},
  author = {Richart, Nicolas and Anciaux, Guillaume and Gallyamov, Emil
            and Frérot, Lucas and Kammer, David and Pundir, Mohit and
            Vocialta, Marco and Ramos, Aurelia Cuba and Corrado, Mauro
            and Müller, Philip and Barras, Fabian and Zhang, Shenghan and
            Ferry, Roxane and Durussel, Shad and Molinari, Jean-François},
  journal = {Journal of Open Source Software},
  volume = {9},
  number = {94},
  pages = {5253},
  year = {2024},
  doi = {10.21105/joss.05253},
}
```

## Features

The paper's summary lists six capabilities:

| Claim of the paper | Parity rows | Tests in the port | Notes |
|---|---|---|---|
| Crack propagation with cohesive elements | COH-003, COH-004, COH-005, CMAT-001, CMAT-002, CMAT-003, CMAT-004, CMAT-005, CMAT-006 | `crates/akantu-cohesive/tests/independent_checks.rs::linear_extrinsic_envelope_and_unloading`, `crates/akantu-cohesive/tests/independent_checks.rs::mesh_invariants_under_full_insertion` | intrinsic and extrinsic (dynamic) insertion, six cohesive laws: [Cohesive elements](../models/cohesive.md). What the named tests cover: the first inserts and opens one interface facet by the extrinsic stress check and compares the traction law with its closed forms, with unloading; the second inserts every facet (intrinsic insertion) and checks the topology and the fragments. Neither follows a crack advancing over several facets; the evidence for that is in the cited rows |
| Non-local damage models | NL-001, NL-003, MAT-012, MAT-014, MAT-016, MAT-027 | `crates/akantu-solid/tests/upstream_material_non_local.rs::test_material_damage_non_local`, `crates/akantu-solid/tests/non_local_materials.rs::non_local_laws_follow_their_local_law_under_uniform_strain` | [Non-local laws](../laws/non-local.md). What the named tests cover: `test_material_damage_non_local` runs with singleton neighbourhoods (its radius is below the element size, as the test says) and the second test averages a uniform field; neither averages over a real neighbourhood, which the cited rows' evidence does |
| Plastic and visco-plastic constitutive laws | MAT-019, MAT-020, MAT-022, XMAT-012 | `crates/akantu-solid/tests/inelastic_properties.rs::j2_return_mapping_unloading_and_plastic_work`, `crates/akantu-extra-materials/tests/point_laws.rs::viscoplastic_explicit_update_from_committed_state` | [Plasticity](../laws/plasticity.md); the law `visco_plastic` is one of the extra materials (feature `extra-materials`, [Extra materials](../models/extra-materials.md)) |
| Large deformations | SMM-023, MAT-005, MAT-021, MAT-025, MAT-026 | `crates/akantu-solid/tests/model_finite_deformation.rs::fd_tangent_is_the_derivative_of_the_internal_force`, `crates/akantu-solid/tests/model_finite_deformation.rs::rigid_rotation_is_stress_free`, `crates/akantu-solid/tests/model_neohookean.rs::assembled_tangent_is_the_derivative_of_the_internal_force`, `crates/akantu-solid/tests/inelastic_properties.rs::ud030_finite_strain_plasticity_is_unsupported_by_default` | the total-Lagrangian formulation and the Neo-Hookean law ([Neo-Hookean hyperelasticity](../laws/neohookean.md)); plasticity with `finite_deformation = true` is refused by default and reproduces upstream behind a switch (UD-030, [Plasticity](../laws/plasticity.md)) |
| Contact constraints, including rate-and-state friction | CONT-001, CONT-004, CONT-015, CONT-016, CPL-001, TASN-002, TASN-003, TASN-016, TASN-025, TASN-034 | `crates/akantu-contact/tests/contact_explicit.rs::test_explicit_dynamic`, `crates/akantu-ntn-contact/tests/independent_checks.rs::rate_and_state_steady_state_closed_forms` | the contact mechanics model with its penalty resolutions and Coulomb friction ([Contact mechanics](../models/contact.md)); the rate-and-state friction laws belong to split-node contact (feature `ntn-contact`, [Split-node contact](../models/ntn-contact.md)), as in upstream's package `traction_at_split_node_contact`. What the named tests cover: the first is upstream's explicit contact test ($\sigma_{yy} \approx 7 \times 10^5$ within 1 %); the second checks the steady state of the rate-and-state laws against closed forms and skips the flash-heating law, for which TASN-034 is cited: its evidence is in the row, not in this test |
| Interaction between contact and cohesive elements (residual crack shear strength) | CPL-003, CPL-004, CMAT-005, EX-003 | `crates/akantu-couplers/tests/cohesive_contact.rs::cohesive_contact_penalty_limits_crack_face_penetration`, `crates/akantu-couplers/tests/coupler_fixtures.rs::coupler_solid_cohesive_contact_fixtures`, `crates/akantu-cohesive/tests/cohesive_fixtures.rs::va3_oracle_cases` | the coupler `CouplerSolidCohesiveContact` ([Model couplers](../models/couplers.md)) and the law [`cohesive_linear_friction`](../models/cohesive.md#cohesive_linear_friction). Covered: the friction of the cohesive law (stick, slip and reverse sliding), by the friction case of `va3_oracle_cases` (`va3_friction_prescribed_quadrangle_4`, bitwise against the C++ reference). The two coupler tests check the normal contact only: the penalty limits the penetration of the crack faces, and two oracle cases whose inputs have a friction coefficient `mu = 0` replay. Not covered: the coupler with Coulomb friction on the faces of a new crack (`mu > 0`); the coupler's runs, upstream's example included, use `mu = 0`, so no test runs the frictional contact of crack faces through the coupler |

## High-performance design

The paper's statement of need adds claims about the library's design:

| Claim of the paper | Parity rows | Tests in the port | Notes |
|---|---|---|---|
| Distributed-memory parallelisation with domain decomposition | PART-001, PART-003, PART-006, MPI-002 | `crates/akantu-mesh/tests/parallel_oracle.rs::scotch_partition_matches_the_oracle`, `crates/akantu-solid/tests/smm_parallel.rs::test_dof_mesh_distribute` | Scotch partitioning, the distribution of the root's mesh and the MPI communicator (features `scotch` and `mpi`): [Parallel computations](../parallel.md) |
| Communication through a layer of ghost elements rather than ghost nodes | PART-002, PART-007, MPI-016, MPI-020 | `crates/akantu-mesh/tests/upstream_synchronizers.rs::test_synchronizer_communication_element` | after the distribution every rank holds its elements and one layer of ghost elements ([Distributing a mesh](../parallel.md#distributing-a-mesh)); the element and facet synchronizers exchange their data |
| Dynamic insertion of cohesive elements in parallel | PART-011, PART-017, COH-008, EX-018 | `crates/akantu-cohesive/tests/parallel_cohesive.rs::invariants_hold_for_every_scenario`, `crates/akantu-cohesive/tests/parallel_cohesive.rs::one_rank_equals_the_serial_run`, `crates/akantu/tests/manual_claims_2.rs::doc023_cohesive_model_runs_on_a_distributed_mesh`, `crates/akantu-bench/tests/ghost_boundary_insertion.rs::ghost_boundary_facets_keep_their_nodes_and_the_load_is_serial` | [Distributed meshes](../models/cohesive.md#distributed-meshes) in the cohesive chapter; the last test is the port's side of UD-890 ([below](#upstreams-multi-rank-runs-of-this-benchmark-ud-890)) |
| A structure-of-arrays layout | — | — | a statement about upstream's code; the port keeps nodal and quadrature-point data in flat arrays with a fixed number of components per entry (`docs/ARCHITECTURE.md`) |

Fracture combined with contact, which the paper names as Akantu's
distinguishing feature, runs on one process in the port as upstream: the
contact model refuses a distributed mesh (approved exception CONT-019,
test `crates/akantu/tests/manual_claims_2.rs::doc012_contact_model_refuses_a_distributed_mesh`),
and upstream's contact model has no distributed support either.

The paper is tagged `python`: the port's Python package and the status of
its rows are described in [Python interface](../models/python.md).

The paper's first figure, the dynamic fragmentation of a tempered glass
pane, is taken from an earlier study (Vocialta et al., 2018); the
reference tree ships the picture only (`joss/results/cohesive_insertion.png`),
so there is nothing to run again. The port's tools for such studies are
the fragment manager (COH-012) and upstream's example `fragmentation.py`
(EX-039).

## Scaling analysis

The paper's study compresses and shears a cube of 734,594 nodes and
4,392,180 tetrahedra (the published mesh file has 4,392,183:
`docs/BENCHMARKS.md`, "The cohesive scalability benchmark"), in which about 460,000 cohesive elements
are inserted, on 1 to 720 cores of EPFL's *jed* cluster (Intel Xeon nodes
with two 36-core sockets), and plots the time to solution with and
without cohesive insertion. The paper concludes that without insertion the
time to solution scales well up to about 576 cores, and that with
insertion the scalability decreases and plateaus at about 288 cores
because changing the mesh's topology needs extra communication. The paper
adds two caveats to the first conclusion: the machine was newly installed,
the effect of the network on the loss of scalability is "not fully
understood", and the minimum and maximum measures "vary a lot after 288
cores (four nodes)". The paper says that the code, material and mesh files
of the study are on Zenodo; the port uses upstream's in-tree
`scalability_test` inputs and the Git LFS `cube.msh` (EX-018,
`docs/BENCHMARKS.md`, "The cohesive scalability benchmark"), and whether these equal the Zenodo archive
was not checked (no network). In the port, the benchmark's driver is the example `scalability_test` (EX-018),
its scaling tooling is EX-049, its distributed static solve with MUMPS is
EX-051, and the comparison of the port's scaling with the paper's data is
GOLD-013.

### The figure's data

Upstream's `joss/results/` holds the timing files of the figure, drawn by
its `plot.py`; they are committed verbatim under
`benchmarks/cohesive-perf/results/published-jed/`
(`benchmarks/cohesive-perf/README.md`). The figure draws two series of the
v5.0.4 files: *insertion* (`timmings_cohesive_gcc_jed_v5.0.4.csv`) and
*no insertion* (`timmings_elastic_gcc_jed_v5.0.4.csv`), each as the median
over the runs of a core count with the band between the minimum and the
maximum. The table below is computed from these two files by the manual's
tests. TTS is the sum of the totals of the chronometers `solve_step` and,
for the insertion series, `check_cohesive_stress` (seconds; every run made
5,000 steps); the speed-up is the median at 1 core divided by the median.
Upstream's `plot.py` multiplies the totals by the number of repetitions
once more (`docs/BENCHMARKS.md`, "Set-up", *Quantities*), so the figure's
axis differs from these seconds by a constant factor and has the same
shape. The paper says that the time to solution is averaged over six
runs; the *runs* columns give the number of runs per core count in the
v5.0.4 files (five, and seven for the insertion series from 288 cores),
and `plot.py` draws medians. The v4.0.1 files, which `plot.py` reads but
does not draw, hold exactly six runs per core count in both series, the
likely source of the paper's six.

<!-- BEGIN GENERATED: joss-figure-data -->
| Cores | No insertion: runs | No insertion: TTS s, median [min, max] | No insertion: speed-up | Insertion: runs | Insertion: TTS s, median [min, max] | Insertion: speed-up |
|---:|---:|---|---:|---:|---|---:|
| 1 | 5 | 10667.5 [10574.9, 10934.8] | 1.0 | 5 | 57117.5 [56575.2, 60653.3] | 1.0 |
| 2 | 5 | 5461.2 [5457.0, 5468.6] | 2.0 | 5 | 40050.8 [39763.6, 40855.2] | 1.4 |
| 4 | 5 | 2828.6 [2825.1, 2848.4] | 3.8 | 5 | 20849.7 [20474.0, 21165.5] | 2.7 |
| 9 | 5 | 1476.8 [1430.1, 1489.9] | 7.2 | 5 | 9445.2 [9353.2, 9839.5] | 6.0 |
| 18 | 5 | 814.9 [794.1, 835.7] | 13.1 | 5 | 4871.9 [4808.0, 5009.9] | 11.7 |
| 36 | 5 | 505.6 [476.1, 582.5] | 21.1 | 5 | 2686.8 [2632.6, 2739.0] | 21.3 |
| 72 | 5 | 416.7 [396.1, 437.4] | 25.6 | 5 | 1829.2 [1748.2, 1868.0] | 31.2 |
| 144 | 5 | 218.1 [216.9, 230.2] | 48.9 | 5 | 888.1 [880.9, 1008.3] | 64.3 |
| 288 | 5 | 114.3 [109.5, 116.1] | 93.3 | 7 | 953.2 [515.8, 1213.4] | 59.9 |
| 432 | 5 | 84.4 [80.7, 86.4] | 126.4 | 7 | 1095.4 [946.5, 1162.7] | 52.1 |
| 576 | 5 | 63.6 [62.9, 65.3] | 167.8 | 7 | 908.4 [509.8, 1054.2] | 62.9 |
| 648 | 5 | 78.9 [59.7, 91.7] | 135.3 | 7 | 940.0 [873.6, 1086.8] | 60.8 |
| 720 | 5 | 52.3 [51.4, 55.3] | 203.9 | 7 | 988.7 [887.4, 1096.7] | 57.8 |
<!-- END GENERATED: joss-figure-data -->

The v4.0.1 files in the same directory are read by `plot.py` but not
drawn.

### Upstream's multi-rank runs of this benchmark (UD-890)

At the reference version, upstream's distributed run of this benchmark
stops solving the problem of the 1-core run once an insertion touches
ghost boundary elements of the loaded groups: the insertion overwrites
ghost boundary triangles of these groups with the connectivity of an
interior facet, and the next shear increment, integrated over them, loads an
interior node (on the frozen 0.12 mesh at 2 ranks, 3,607.05 N in z on one
interior node; registered as UD-890, category B, corrected without a switch). From then on the
energies and insertion counts of a multi-rank run are those of another
load case, while its per-phase timings remain comparable work.

Upstream's distributed insertion has a second defect that acts in these
runs, the placement of the accumulated load of doubled nodes (UD-1280,
category B, D-105 as amended by D-108). When an
insertion doubles a node of a loaded face, upstream gives the new copy no
external force and leaves the whole Neumann load applied so far on the
original copy; which copy is the original follows from the orientation of
the doubled facet, and on a distributed mesh a facet on the partition
interface can be oriented the other way than in the serial mesh. The
port reproduces this bit for bit: on the h = 0.06 mesh at 8 ranks the C++
reference and the port swap the same copies at step 1,467 with
bit-identical forces, and in a forced insertion at 8 ranks, where UD-890
cannot act, both codes swap the same 106 copies against their serial runs
(the doubled-node experiment, described in `docs/BENCHMARKS.md`). D-105 decides that the port corrects the placement by
default, with a compatibility switch, `doubled_node_keeps_external_force`,
that reproduces upstream's. The multi-rank runs of the port on this page
made before 13 November predate the correction and place the accumulated
load as upstream's does; the full-size rerun of 13 November with the
correction has rows of its own (below).

The paper's *insertion* curve compares runs on different core counts;
whether the published v4.0.1 and v5.0.4 runs were affected by either
mechanism cannot be decided from the files, which hold timings only. The
port's regression test
`crates/akantu-bench/tests/ghost_boundary_insertion.rs::ghost_boundary_facets_keep_their_nodes_and_the_load_is_serial`
asserts what the UD-890 part needs: the distributed insertion leaves the
ghost boundary elements on their own faces, so that the Neumann load is
integrated as in the serial run. It does not assert that the accumulated
load of doubled nodes sits on the same copy as in the serial run.

### What the port has measured

`docs/BENCHMARKS.md` records the port's driver run next to the C++
reference on the same inputs and the same partitions, one machine at a
time (the sections on the distributed static solve with MUMPS, the
mid-size runs and the weak and strong scaling):

* the frozen h = 0.12 mesh (23,156 tetrahedra), 1,500 steps, at 1, 2 and
  4 ranks;
* h = 0.06 (177,328 tetrahedra), the full 5,000-step schedule with about
  51,500 cohesive elements inserted, at 1, 2, 4 and 8 ranks;
* h = 0.04 (558,875 tetrahedra), the static solve and 300 steps before
  any insertion, at 1, 2, 4 and 8 ranks (C++ at 8 ranks: killed by the
  container's memory limit);
* the weak- and strong-scaling series of 13 November on one 8-vCPU
  x86_64 cloud machine: 500 steps before any insertion on five meshes from h = 0.12 to
  h = 0.0476 (23,156 to 352,996 tetrahedra) at 1, 2, 4 and 8 ranks, and the
  insertion regime on the frozen h = 0.12 mesh (1,500 steps) at 1, 2, 4
  and 8 ranks, five repetitions of every point.

On 1 rank the port's run equals the C++ reference bit for bit over the
whole fragmenting h = 0.06 run. On 2, 4 and 8 ranks the port stays on the
serial trajectory until steps 2,498, 2,152 and 1,467. The 8-rank departure at
step 1,467 is the partition-dependent placement of the accumulated load of
doubled nodes (UD-1280, D-105; the doubled-node experiment). With the serial static
solution imposed, every insertion decision and every node state agrees with the
serial run to rounding (at most 3.5e-12 in the facet stresses and 1e-15 in the
external force) up to step 1,466. At step 1,467 the same four facets are
inserted, one corner facet on the partition interface is oriented the other way,
and three nodal loads of 1.4e5 to 2.2e5 N stay on the other lip of the new
crack. The static solve is not the trigger, and the departure is not a matter of
the order of floating-point summation. With the external force rebuilt on the
current topology after each doubling, the serial and the 8-rank runs insert the
same 8,699 elements at the same 1,486 steps over 2,500 steps, with E_p, E_k and
E_t within 1e-10 of their own values. The 2- and 4-rank departures (steps 2,498 and 2,152) are not
explained by that experiment; its reproducer (`doubled_node_load_side`) shows
that the same mechanism exists at 2 and 4 ranks. Every C++ multi-rank run leaves
the serial trajectory at steps 901 to 907 (UD-890), and upstream's distributed
insertion places the accumulated load as the port's does, so the final counts
of runs at more than one rank, of either code, are not a correctness
criterion. The tables of times and memory are in those sections; this page
quotes none of them.

### The full-size run

The paper's mesh was run once on one 64-core x86_64 node (one AWS
`r7a.16xlarge`: 64 cores of an AMD EPYC 9R14, one NUMA node, one thread per
core), the insertion case with 5,000 steps, with the port at 64, 32 and 8
ranks and with the C++ reference at 64 and 32 ranks. The C++ run at 8 ranks
was not made, for lack of time in the session; the table marks it as not
run. The table's numbers come from the committed summaries of the run and
from nothing else; they, the mesh hash, the images and the binaries are
recorded in `docs/BENCHMARKS.md` (full-size run).

What the table is and is not:

* Configuration. Upstream's laws in both codes. The port runs with its
  UD-890 correction (no corrupted ghost boundary elements) and with
  upstream's placement of the accumulated load of doubled nodes (UD-1280,
  D-105): the runs of 12 November predate the correction of D-105. The
  rerun of the port with the corrected placement (13 November, the port
  only, at 64 and 32 processes) has separate rows labelled with that
  setting.
* Columns. Wall time, `init_full`, the static solve, the `solve_step`
  and `check_cohesive_stress` times per step, the cost per element and step
  (the sum of the two per-step times times the number of ranks, divided by
  the number of tetrahedra, in core-µs), the cohesive elements inserted and
  the steps of the first insertions, the peak memory and the energies at the
  end. The peak memory is the container's cgroup peak for both codes (the
  table's note); the committed summaries hold every energy of the five
  runs. "Time to solution" in the paper's sense is not a
  column: the paper's figure plots five times the sum of the `solve_step`
  and check totals.
* No correctness comparison. The final counts and energies are not
  compared between rank counts, nor between the two codes, as a correctness
  result. Upstream's runs at more than one rank solve another load case from
  their first insertion that touches a ghost boundary element (UD-890), and
  both codes place the accumulated load of doubled nodes depending on the
  partition (UD-1280, D-105) until the correction. The differences between
  the port's runs of 12 November at 64 and 32 ranks include this load
  placement; they are not a statement about rounding. In the rerun with the
  correction, the 64- and 32-process runs insert the same counts for 3,176
  steps and end 4 elements apart, and two 64-process runs of the same binary
  on one node and on eight hosts, between which no placement can differ,
  part as early: the remaining differences behave as rounding amplified by
  the fracture process, and no partition-dependent placement shows within
  the window in which one would (about 2,700 steps); in two further runs of
  the same library code, which insert the same counts through step 3,052, a
  check of every copy's load passed at every step to 3,200
  (`docs/BENCHMARKS.md`, the rerun after UD-1280; the acceptance statement
  that D-108 wrote before the rerun is met in its parts 1 (a) and 1 (c),
  not in 1 (b) as written).
* No performance claim beyond the measured ratios on this one node
  (D-096): the runs across eight hosts of 13 November are reported, as
  measured, in `docs/BENCHMARKS.md` ("Multi-node runs"); nothing is
  extrapolated to the paper's 144 to 720 cores, and no absolute times are
  compared with the files of *jed* (another machine, version, compiler and
  year). The comparison of the shapes of the curves is parity row GOLD-013
  (below).

| Processes | Code | Wall s | `init_full` s | Static solve s | `solve_step` ms/step | Check ms/step | core-µs/el/step | Cohesive elements inserted | Insertion steps (first) | Peak memory GiB | Energies at the end: dissipated / potential / kinetic J |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---|---:|---|
| 64 | port | 2,387 | 1.91 | 305 | 108 | 255 | 5.29 | 476,343 | 4,114 (683) | 206.2 | 476.09 / 896,157.6 / 1,842,332.3 |
| 64 | C++ | 2,729 | 7.42 | 228 | 132 | 318 | 6.56 | 485,584 | 4,114 (683) | 191.7 | 488.94 / 905,967.6 / 1,862,622.1 |
| 32 | port | 3,221 | 3.12 | 356 | 112 | 418 | 3.86 | 476,020 | 4,114 (683) | 185.5 | 475.16 / 896,154.4 / 1,816,693.3 |
| 32 | C++ | 3,787 | 17.3 | 353 | 179 | 459 | 4.65 | 482,747 | 4,114 (683) | 194.2 | 484.09 / 904,985.2 / 1,836,170.2 |
| 8 | port | 10,967 | 12.8 | 831 | 249 | 1,656 | 3.47 | 475,636 | 4,114 (683) | 165.9 | 474.36 / 896,256.3 / 1,817,697.4 |
| 8 | C++ | not run (no time left in the session) | — | — | — | — | — | — | — | — | — |
| 64 | port, corrected placement (13 Nov) | 2,415 | 1.86 | 324 | 111 | 254 | 5.32 | 471,932 | 4,114 (683) | 198.2 | 480.73 / 913,421.1 / 567,193.4 |
| 32 | port, corrected placement (13 Nov) | 3,107 | 3.12 | 338 | 110 | 402 | 3.73 | 471,936 | 4,114 (683) | 190.7 | 480.75 / 913,418.3 / 567,182.7 |

Note: the numbers are those of the committed summaries of the two runs
(the per-run rows, as printed by `crates/akantu-bench/scripts/c1_report.py
tables` and `crates/akantu-bench/scripts/results_report.py joss`; their
provenance is in `docs/BENCHMARKS.md`). The peak memory is the container's
cgroup peak (`memory.peak`), the same kind for both codes. One run per
point. The two rows of the corrected placement are the rerun of 13 November:
the port only, with
upstream's laws and `rust_doubled_node_load = follows_elements`, on the
partitions of 12 November. Their counts and energies are not expected to
match the rows above after the first doubled loaded node (D-108): the load
placement changes the problem from there on, and other changes between the
two binaries may contribute.
The change is large: at step 5,000 the kinetic energy of the corrected
runs is 3.2 times smaller than that of the port's runs of 12 November
(567,193 J against 1,842,332 J at 64 processes), the potential energy is
1.9 % and the dissipated energy 1.0 % larger, and 0.9 % fewer cohesive
elements are inserted. The first doubled loaded node is at step 829 (the
per-copy runs of 13 November, the same library code), before the first
recorded departure of the rerun from the runs of 12 November (the work of
a step at step 890, an insertion step at step 945).

### Comparison with the paper's files (GOLD-013)

The comparison of these runs with the paper's files is in
`docs/BENCHMARKS.md`, "Comparison with upstream's published data
(GOLD-013)". Over the same factors of processes the measured efficiencies of
the explicit loop are 90 % and 73 % for the port from 8 to 32 to 64
processes (one run per point; their spread is not measured) against 88 %
and 73 % for the paper's runs from 9 to 36 to 72 cores (medians of five), and the time per element and step on this mesh is of the same
order (3.9 / 4.4 / 6.0 core-µs on *jed* at 9 / 36 / 72 cores, 3.5 / 3.9 /
5.3 for the port at 8 / 32 / 64 processes). Absolute times of two machines
and years are not compared, and the port has one run per point against
five in the paper's files.

### What the port has not measured

* More than 128 processes: the paper's points from 144 cores (two *jed*
  nodes) upwards. The final measurement campaign ran both codes at 128
  processes on one 128-core machine and on four 32-core hosts
  (`docs/BENCHMARKS.md`, "The full-size run" and "Multi-node runs"); the
  port ran across eight hosts at up to 64 processes on 13 November; its
  multi-node acceptance is parity row INT-009.
* The no-insertion series (elastic material) at full size.

## Status of the cited rows

<!-- BEGIN GENERATED: parity-cited appendix/joss.md -->
| Row | Feature | Status |
|---|---|---|
| `MPI-002` | MPI backend | Verified |
| `MPI-016` | `ElementSynchronizer` | Verified |
| `MPI-020` | `FacetSynchronizer` | Verified |
| `PART-001` | `Mesh::distribute` | Verified |
| `PART-002` | `MeshPartition` base (dual graph, ghost partitions) | Verified |
| `PART-003` | Scotch partitioning | Verified |
| `PART-006` | Centralized mesh distribution driver | Verified |
| `PART-007` | Element distribution (`ElementInfoPerProc`) | Verified |
| `PART-011` | Parallel cohesive insertion hooks | Verified |
| `PART-017` | Global-data update hook after topology changes | Verified |
| `SMM-023` | Finite-deformation total-Lagrangian assembly (Bathe 1975) | Verified |
| `MAT-005` | `elastic` + `finite_deformation = true` (St-Venant-Kirchhoff) | Verified |
| `MAT-012` | `MaterialMarigoNonLocal` (`marigo_non_local`) | Verified |
| `MAT-014` | `mazars_non_local` | Verified |
| `MAT-016` | `plastic_mazars_non_local` | Verified |
| `MAT-019` | `MaterialPlastic` base | Verified |
| `MAT-020` | `plastic_linear_isotropic_hardening` (small strain J2) | Verified |
| `MAT-021` | `plastic_linear_isotropic_hardening` with `finite_deformation` | Verified |
| `MAT-022` | `plastic_drucker_prager` | Verified |
| `MAT-025` | `neohookean` (compressible Neo-Hookean, 1D/3D and 2D plane strain) | Verified |
| `MAT-026` | `neohookean` 2D plane stress (C33 solve) | Verified |
| `MAT-027` | Non-local damage material templates | Verified |
| `NL-001` | `NonLocalManager` | Verified |
| `NL-003` | Non-local weighting & averaging | Verified |
| `COH-003` | Intrinsic insertion | Verified |
| `COH-004` | Extrinsic (dynamic) insertion & stress check | Verified |
| `COH-005` | Cohesive element insertion (topology) | Verified |
| `COH-008` | Cohesive model parallel data | Verified |
| `COH-012` | `FragmentManager` | Verified |
| `CMAT-001` | `cohesive_linear` (extrinsic & intrinsic linear softening, Camacho-Or… | Verified |
| `CMAT-002` | `cohesive_bilinear` (intrinsic bilinear) | Verified |
| `CMAT-003` | `cohesive_exponential` (Xu-Needleman-type exponential) | Verified |
| `CMAT-004` | `cohesive_linear_fatigue` (Nguyen-Repetto-Ortiz-Radovitzky 2001) | Verified |
| `CMAT-005` | `cohesive_linear_friction` | Verified |
| `CMAT-006` | `cohesive_linear_uncoupled` | Verified |
| `CONT-001` | `ContactMechanicsModel` | Verified |
| `CONT-004` | Contact force assembly | Verified |
| `CONT-015` | Penalty (linear) resolution `penalty_linear` | Verified |
| `CONT-016` | Penalty quadratic resolution `penalty_quadratic` | Verified |
| `CONT-019` | Distributed contact search and resolution: remote surface and candidate search, force and stiffness, history ownership (an extension asked by the mission; upstream's generic contact has no distributed protocol) | Exception (approved) |
| `CPL-001` | `CouplerSolidContactTemplate<SMM>` (monolithic solid + contact) | Verified |
| `CPL-003` | `CouplerSolidCohesiveContact` | Verified |
| `CPL-004` | `CohesiveContactSolverCallback` | Verified |
| `XMAT-012` | `visco_plastic` (Perzyna-type viscoplasticity) | Verified |
| `TASN-002` | `NTNContact` (node-to-node pairs) | Verified |
| `TASN-003` | `NTRFContact` (node-to-rigid-flat) and `MIIASYMContact` (mode-II anti… | Verified |
| `TASN-016` | rate_and_state:dieterich:ageing: μ = μ0 + a ln(V/V*) + b ln(θ/θ*); Δθ = Δt (1 − θV/d_c), applied before μ and only when not sticking | Verified |
| `TASN-025` | rate_and_state:n_shaped:ageing: μ = (1 + b ln(1 + θ/θ*)) [μ0/√(1 + (V/V*)²) + a ln(V/V*)]; Δθ = Δt (1 − θV/d_c), applied before μ and only when not sticking | Verified |
| `TASN-034` | rate_and_state:flashheating: its own velocity branches, μ_w / v_w arrays and restart | Verified |
| `EX-003` | extrinsic cohesive insertion + contact on new crack faces,… | Verified |
| `EX-018` | `mesh.distribute()`, extrinsic cohesive in parallel, `FromHigherDim`,… | Verified |
| `EX-039` | `fragmentation.py` (`fragmentation-test`) | Verified |
| `EX-049` | Scalability tooling | Verified |
| `EX-051` | Distributed MUMPS static solve of the cohesive scalability driver: the driver's `_auto` static solver resolves to MUMPS on the MPI world at every rank count; every rank passes its rank-partial AIJ entries (`ICNTL(18) = 3`), the right-hand side is centralised on rank 0, the matrix is not gathered | Verified |
| `GOLD-013` | Benchmark timing data of the JOSS paper | Verified |
| `DOC-019` | JOSS paper claims & benchmark data | Verified |
| `INT-009` | MPI lifecycle, error propagation, rank-count and multi-node acceptance: externally initialised MPI lifecycle, rank failures and their propagation and cleanup, physics equivalence at 1 / 2 / 4 / 8 ranks, partition independence | Verified |
<!-- END GENERATED: parity-cited appendix/joss.md -->
