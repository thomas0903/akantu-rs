#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
# counterpart: performance sanity of the cohesive scalability
# driver on a distributed mesh. Runs INSIDE akantu-rs-mpi:dev
# (`scripts/mpi/in-docker.sh bash crates/akantu-bench/scripts/par2b_sanity.sh`):
# release build with the `mpi` feature, then the input of the 1-rank Eigen
# oracle fixtures/perf_cohesive/coh_np1_eigen (h = 0.12 cube, cohesive
# insertion, 1500 steps, Eigen static solve) with `blocked_node = global`
# (the 1-rank problem at every rank count), RCB partitions, under
# `mpirun -np 1, 2, 4`;
# and the 2-rank run on the C++ harness's frozen partition of
# fixtures/perf_cohesive/coh_np2_global (its insertion steps and counts are
# compared by the caller). Outputs in /target/par2b-sanity/<stamp>/.
set -uo pipefail
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="${CARGO_TARGET_DIR:-/target}/par2b-sanity/${stamp}"
mkdir -p "${out}"
cargo build --release -p akantu-bench --features mpi --locked --bin cohesive_extrinsic \
  > "${out}/build.log" 2>&1 || { tail -30 "${out}/build.log"; exit 1; }
bin="${CARGO_TARGET_DIR:-/target}/release/cohesive_extrinsic"
mesh=/src/fixtures/perf_cohesive/meshes/cube-h0.12.msh
src=/src/fixtures/perf_cohesive/coh_np1_eigen/input.dat # the 1-rank Eigen oracle's input
export OMPI_MCA_rmaps_base_oversubscribe=1 OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1
add='\n  blocked_node = global'
run() { # dir np partitioner
  local d="${out}/$1" np=$2 part=$3
  mkdir -p "${d}"
  sed -e "s|^  mesh = .*|  mesh = ${mesh}|" \
      -e "s|^user section \\[\$|&${add}\\n  rust_partitioner = ${part}|" "${src}" > "${d}/input.dat"
  local t0 t1
  t0=$(date +%s.%N)
  (cd "${d}" && timeout -k 10 3000 mpirun -np "${np}" --bind-to none -x OMP_NUM_THREADS \
    -x OPENBLAS_NUM_THREADS "${bin}" --aka_input_file input.dat > out.txt 2> err.txt; echo $? > exit)
  t1=$(date +%s.%N)
  echo "$1 np=${np} exit=$(cat "${d}/exit") wall=$(python3 -c "print(f'{${t1}-${t0}:.1f}')") s"
}
run global-rcb-np1 1 rcb
run global-rcb-np2 2 rcb
run global-rcb-np4 4 rcb
run cpp-partition-np2 2 file:/src/fixtures/perf_cohesive/coh_np2_global/partition.csv
for d in "${out}"/*/; do
  echo "== ${d}"
  cat "${d}/chrono.csv" 2>/dev/null
  python3 -c "import json,sys; s=json.load(open(sys.argv[1])); print({k: s[k] for k in ('psize','nb_cohesive_elements','nb_insertion_steps','first_insertion_step','nb_global_nodes_end','energy_dissipated','energy_potential','energy_kinetic','displacement_l2_norm','time_step') if k in s})" "${d}/summary.json" 2>/dev/null
  tail -2 "${d}/insertions.csv" 2>/dev/null
  tail -3 "${d}/err.txt"
done
echo "outputs: ${out}"
