# Model suite: rank scaling of the distributed cases

- Machine: an AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, 32 GiB).
- Binary: `suite_models` with the `mpi` feature, built in the project's MPI
  image (`akantu-rs-mpi:dev`, Open MPI 5.0.7) from commit `e3743be5` (the
  heat and phase-field cases are the same code as in the baseline commit
  `ee4140ce`); SHA-256 in `meta.json`.
- Command: `docker run --cpuset-cpus C akantu-rs-mpi:dev mpirun -n P
  --bind-to core --map-by core suite_models --case CASE --size m --reps 1`,
  one `mpirun` per run. P = 1, 2, 4 on cores 0–3 inside the campaign's
  timing wrapper; P = 8 on the whole machine with nothing else running.
- Mesh read on rank 0 and distributed by recursive coordinate bisection
  (`distribute` phase: 0.38–0.65 s for the tet4 cube at 2–8 ranks, 0.05–0.08 s
  for the tri3 square). Digests: gathered by global node id; at one rank
  they equal the serial binary's digests (`b4798ad9264a3e5e` heat,
  `a77809465b3e7c18` phase field); at P > 1 they differ from the serial
  ones (summation order at interface nodes, as the parallel tests document)
  and repeat from run to run.
- Repetitions: three at 1, 2 and 4 ranks, two at 8 ranks (the third 8-rank
  run was still queued for an empty machine when this was written;
  stopped to spare the shared machine). Raw data: `CASE.pP.rR.jsonl`.
- Date (UTC): 2026-11-16, 07:50–10:40.

| case | phase | ranks | reps | seconds, median [min, max] | speed-up vs 1 rank (medians) | digest repeats |
|---|---|---|---|---|---|---|
| heat_explicit_tet4 | solve_step | 1 | 3 | 3.993 [3.992, 4.011] | 1.00 | yes |
| heat_explicit_tet4 | solve_step | 2 | 3 | 2.351 [2.322, 2.375] | 1.70 | yes |
| heat_explicit_tet4 | solve_step | 4 | 3 | 1.445 [1.431, 1.452] | 2.76 | yes |
| heat_explicit_tet4 | solve_step | 8 | 2 | 1.003 [1.001, 1.004] | 3.98 | yes |
| phasefield_staggered_tri3 | solid_solve | 1 | 3 | 11.422 [11.416, 11.715] | 1.00 | yes |
| phasefield_staggered_tri3 | solid_solve | 2 | 3 | 11.391 [11.347, 11.834] | 1.00 | yes |
| phasefield_staggered_tri3 | solid_solve | 4 | 3 | 11.206 [11.191, 11.442] | 1.02 | yes |
| phasefield_staggered_tri3 | solid_solve | 8 | 2 | 10.898 [10.861, 10.935] | 1.05 | yes |
| phasefield_staggered_tri3 | phase_solve | 1 | 3 | 2.143 [2.139, 2.179] | 1.00 | yes |
| phasefield_staggered_tri3 | phase_solve | 2 | 3 | 1.822 [1.769, 1.967] | 1.18 | yes |
| phasefield_staggered_tri3 | phase_solve | 4 | 3 | 1.789 [1.769, 1.826] | 1.20 | yes |
| phasefield_staggered_tri3 | phase_solve | 8 | 2 | 1.640 [1.589, 1.690] | 1.31 | yes |
| phasefield_staggered_tri3 | solid_assemble_stiffness | 1 | 3 | 0.322 [0.320, 0.374] | 1.00 | yes |
| phasefield_staggered_tri3 | solid_assemble_stiffness | 2 | 3 | 0.185 [0.185, 0.189] | 1.74 | yes |
| phasefield_staggered_tri3 | solid_assemble_stiffness | 4 | 3 | 0.093 [0.093, 0.098] | 3.46 | yes |
| phasefield_staggered_tri3 | solid_assemble_stiffness | 8 | 2 | 0.041 [0.041, 0.041] | 7.84 | yes |
| phasefield_staggered_tri3 | phase_assemble_stiffness | 1 | 3 | 0.167 [0.165, 0.197] | 1.00 | yes |
| phasefield_staggered_tri3 | phase_assemble_stiffness | 2 | 3 | 0.080 [0.080, 0.080] | 2.08 | yes |
| phasefield_staggered_tri3 | phase_assemble_stiffness | 4 | 3 | 0.039 [0.039, 0.040] | 4.22 | yes |
| phasefield_staggered_tri3 | phase_assemble_stiffness | 8 | 2 | 0.019 [0.019, 0.019] | 8.61 | yes |

Reading:

- The explicit heat step scales to 2.8× at 4 ranks and 4.0× at 8 ranks
  (the step is memory-bound and every rank exchanges ghost temperatures
  each step).
- The phase-field solid solve does not scale at all: without the
  `mumps` feature the default `_auto` sparse solver is Eigen centralised
  on rank 0 (crates/akantu-bench/Cargo.toml, feature `mumps`), so the
  factorisation stays serial while the assemblies scale (7.8–8.6× at 8
  ranks). At 8 ranks the staggered step is 92 % solid solve.
