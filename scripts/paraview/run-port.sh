#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): runs the Rust examples
# of the port (crates/akantu/examples, release build, run as their tests do)
# once each and leaves every file they write in <out>/<example>/
# (scripts/paraview/README.md).
#   scripts/paraview/run-port.sh <out-dir> <oracle-out-dir> [example ...]
# Host: an x86_64 Linux host; the examples are built beforehand in the project image:
#   scripts/mpi/in-docker.sh cargo build --release -p akantu --examples --features cgal --locked
# (<oracle-out-dir>: the output of run-oracle.sh; parallel_2d replays the
# partition of the upstream run found there.)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
out="${1:?usage: run-port.sh <out-dir> <oracle-out-dir> [example ...]}"
oracle="${2:?oracle out dir}"
shift 2
mkdir -p "${out}"; out="$(cd "${out}" && pwd)"; oracle="$(cd "${oracle}" && pwd)"
image="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
want=("$@")
while IFS=$'\t' read -r name binary ocase args _desc; do
  case "${name}" in ''|'#'*) continue ;; esac
  if [ "${#want[@]}" != 0 ] && [[ " ${want[*]} " != *" ${name} "* ]]; then continue; fi
  case "${binary}" in python:*) continue ;; esac  # run-ex012.sh (feature python)
  [ "${args}" != "-" ] || args=""
  args="${args//\{oracle\}//oracle/${ocase}}"
  args="${args//\{pview\}//src/scripts/paraview}"
  d="${out}/${name}"
  rm -rf "${d}"; mkdir -p "${d}"
  t0=$(date +%s)
  set +e
  # shellcheck disable=SC2086 # args is a word list
  docker run --rm --init --memory 6g --memory-swap 6g --cpus 4 --shm-size 512m \
    --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "${repo}:/src:ro" -v akantu-rs-mpi-target:/target:ro -v "${oracle}:/oracle:ro" -v "${d}:/out" \
    -w /out "${image}" bash -c "timeout -k 10 3600 /target/release/examples/${binary} --output-dir /out ${args} > /out/stdout.txt 2> /out/stderr.txt"
  rc=$?
  set -e
  echo "${rc}" > "${d}/exit_code"
  echo "/target/release/examples/${binary} --output-dir <dir> ${args}" > "${d}/command.txt"
  echo "${name}: exit ${rc} in $(( $(date +%s) - t0 )) s, $(find "${d}" -type f | wc -l) files, $(du -sh "${d}" | cut -f1)"
done < "${here}/cases.tsv"
