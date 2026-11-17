#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: interleaved, rotated timing of the C++ harness
# (benchmarks/cohesive-perf, oracle image, one or more builds) and the Rust
# driver (crates/akantu-bench, host binaries) on one machine. Not a port of
# Akantu code.
#
# Usage (on a Linux host with Docker, e.g. an x86_64 Linux host; never on the
# shared Mac):
#   timing_ab.sh --out DIR --meshes DIR [--h "0.12"] [--steps 500] [--reps 5]
#                [--materials "cohesive elastic"] [--memory 13g] [--rep-start K]
#                VARIANT...
#   VARIANT = <label>=cpp:<config>            C++ harness /perf/builds/<config>/
#                                              (benchmarks/cohesive-perf/scripts/
#                                              build.sh <config>), install-<config>
#           | <label>=rust:<binary>[,k=v...]   Rust driver; k=v are extra user-
#                                              section parameters (e.g.
#                                              rust_static_solve=false)
#
# Method (ADR proposal "interleaved A/B medians"): one run at
# a time; every repetition runs every (h, material) point with every variant,
# the variants in a rotated order (repetition k starts with variant
# (k - 1) mod n), so a drift of the machine speed favours no variant.
# --rep-start K (default 1) numbers the repetitions K..K+reps-1 (rotation and
# rep<k> directories included): a long series split into several calls with
# the same --out continues where the previous call stopped. Both
# sides read the same mesh file (<meshes>/cube-h<h>.msh, copied into the
# harness volume) and an input derived from benchmarks/cohesive-perf/inputs/
# material-<material>.dat exactly as the harness does (`mesh`, `max_steps`
# and the extra parameters change). Single-threaded: OMP_NUM_THREADS=1,
# OPENBLAS_NUM_THREADS=1 on both sides. Per run: <out>/<label>/<material>/
# h<h>/rep<k>/ with input.dat, chrono.csv, summary*.json, insertions.csv,
# out.txt, err.txt, time.txt (GNU time -v), exit and run.json (variant,
# binary digest, host CPU, load at start, start time); <out>/provenance.txt
# and <out>/schedule.log. Summarise with compare_cpp.py.
# PIN_CPU (optional, e.g. 2): every run on that core only (taskset for the
# Rust driver, docker --cpuset-cpus for the C++ harness); recorded in
# provenance.txt.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
out=""; meshes=""; hs="0.12"; steps=500; reps=5; materials="cohesive elastic"; memory=13g; rep_start=1
variants=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="$2"; shift 2 ;;
    --meshes) meshes="$2"; shift 2 ;;
    --h) hs="$2"; shift 2 ;;
    --steps) steps="$2"; shift 2 ;;
    --reps) reps="$2"; shift 2 ;;
    --materials) materials="$2"; shift 2 ;;
    --memory) memory="$2"; shift 2 ;;
    --rep-start) rep_start="$2"; shift 2 ;;
    *=cpp:*|*=rust:*) variants+=("$1"); shift ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
[ -n "${out}" ] || { echo "--out is required" >&2; exit 2; }
[ -d "${meshes}" ] || { echo "--meshes: ${meshes} missing" >&2; exit 2; }
[ ${#variants[@]} -gt 0 ] || { echo "no variant" >&2; exit 2; }
[[ "${rep_start}" =~ ^[1-9][0-9]*$ ]] || { echo "--rep-start: positive integer" >&2; exit 2; }
case "${out}" in /|"${HOME}"|"${repo}") echo "refusing --out ${out}" >&2; exit 2 ;; esac
img="${PERF_IMAGE:-akantu-oracle:dev}"
vol="${PERF_VOLUME:-akantu-perf-cohesive}"
oracle_vol="${ORACLE_VOLUME:-akantu-oracle-build}"
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"
meshes="$(cd "${meshes}" && pwd)"
tag="tab-$(date -u +%Y%m%dT%H%M%S)"
cpu="$(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //')"
pin_rust=(); pin_cpp=()
if [ -n "${PIN_CPU:-}" ]; then pin_rust=(taskset -c "${PIN_CPU}"); pin_cpp=(--cpuset-cpus "${PIN_CPU}"); fi

have_cpp=0
for v in "${variants[@]}"; do case "${v#*=}" in cpp:*) have_cpp=1 ;; esac; done
for h in ${hs}; do
  [ -f "${meshes}/cube-h${h}.msh" ] || { echo "mesh ${meshes}/cube-h${h}.msh missing" >&2; exit 1; }
done
if [ "${have_cpp}" = 1 ]; then
  # harness volume owned by the image user (as benchmarks/cohesive-perf/
  # scripts/common.sh prepares it), then the meshes (same bytes on both sides)
  docker volume inspect "${vol}" > /dev/null 2>&1 || docker volume create "${vol}" > /dev/null
  if ! docker run --rm -v "${vol}:/perf" "${img}" test -w /perf/runs; then
    docker run --rm -u 0 -v "${vol}:/perf" "${img}" \
      sh -c 'mkdir -p /perf/meshes /perf/builds /perf/runs && chown -R 1000:1000 /perf'
  fi
  for h in ${hs}; do
    docker run --rm -i -v "${vol}:/perf" "${img}" sh -c "cat > /perf/meshes/${tag}-cube-h${h}.msh" \
      < "${meshes}/cube-h${h}.msh"
  done
fi

{
  # no host name: a cloud machine's name carries its project and zone, and
  # these records are committed (the public-tree guard refuses them)
  echo "host=(redacted) cpu=${cpu} nproc=$(nproc) kernel=$(uname -r) mem_total_kib=$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
  echo "steps=${steps} reps=${reps} h=${hs} materials=${materials} memory_cap_cpp=${memory} pin_cpu=${PIN_CPU:-none} started_utc=$(date -u +%FT%TZ)"
  [ "${have_cpp}" = 0 ] || echo "image=$(docker image inspect --format '{{.Id}}' "${img}")"
  for h in ${hs}; do echo "mesh h${h} sha256=$(sha256sum "${meshes}/cube-h${h}.msh" | cut -d' ' -f1)"; done
  for v in "${variants[@]}"; do
    spec="${v#*=}"
    case "${spec}" in
      cpp:*) config="${spec#cpp:}"
        echo "variant ${v%%=*} cpp config=${config}"
        docker run --rm -v "${vol}:/perf:ro" "${img}" sh -c "sha256sum /perf/builds/${config}/cohesive_extrinsic; sed 's/^/  /' /perf/builds/${config}/build-info.txt" ;;
      rust:*) bin="${spec#rust:}"; bin="${bin%%,*}"
        echo "variant ${v%%=*} rust bin=${bin} sha256=$(sha256sum "${bin}" | cut -d' ' -f1) extra=${spec#rust:${bin}}" ;;
    esac
  done
} >> "${out}/provenance.txt"

input_for() { # material mesh extra(comma list) -> stdout
  local material=$1 mesh=$2 extra=$3 src="${repo}/benchmarks/cohesive-perf/inputs/material-$1.dat"
  local sedargs=(-e "s|^  max_steps = .*|  max_steps = ${steps}|" -e "s|^  mesh = .*|  mesh = ${mesh}|")
  local add="" kv k v
  IFS=',' read -r -a kvs <<< "${extra}"
  for kv in ${kvs[@]+"${kvs[@]}"}; do
    [ -n "${kv}" ] || continue
    k="${kv%%=*}"; v="${kv#*=}"
    if grep -qE "^  ${k} = " "${src}"; then sedargs+=(-e "s|^  ${k} = .*|  ${k} = ${v}|")
    else add="${add}\\n  ${k} = ${v}"; fi
  done
  [ -z "${add}" ] || sedargs+=(-e "s|^user section \\[\$|&${add}|")
  sed "${sedargs[@]}" "${src}"
}

run_json() { # dir label kind ref material h rep extra start load status
  python3 -I - "$@" <<'EOF'
import hashlib, json, sys
d, label, kind, ref, material, h, rep, extra, start, load, status, cpu = sys.argv[1:13]
json.dump({"label": label, "kind": kind, "ref": ref, "material": material, "h": h,
           "rep": int(rep), "extra": extra, "started_utc": start, "load_at_start": load,
           "exit_status": int(status), "cpu": cpu,
           "input_sha256": hashlib.sha256(open(d + "/input.dat", "rb").read()).hexdigest()},
          open(d + "/run.json", "w"), indent=1)
EOF
}

run_cpp() { # label config material h rep
  local label=$1 config=$2 material=$3 h=$4 rep=$5
  local d="/perf/runs/${tag}/${label}/${material}/h${h}/rep${rep}" host="${out}/${label}/${material}/h${h}/rep${rep}"
  input_for "${material}" "/perf/meshes/${tag}-cube-h${h}.msh" "" \
    | docker run --rm -i -v "${vol}:/perf" "${img}" sh -c "rm -rf ${d} && mkdir -p ${d} && cat > ${d}/input.dat"
  local start load
  start="$(date -u +%FT%TZ)"; load="$(cut -d' ' -f1 /proc/loadavg)"
  echo "${start} ${label} ${material} h${h} rep${rep} load=${load}" >> "${out}/schedule.log"
  docker run --rm ${pin_cpp[@]+"${pin_cpp[@]}"} --shm-size=4g --init --memory="${memory}" --memory-swap="${memory}" \
    -v "${oracle_vol}:/build:ro" -v "${vol}:/perf" -w "${d}" \
    -e AKA_SEED=1 -e SCOTCH_PTHREAD_NUMBER=1 -e AKANTU_RS_FILL_NODES_TO_ELEMENTS=upstream \
    -e OMP_NUM_THREADS=1 -e OPENBLAS_NUM_THREADS=1 "${img}" \
    bash -c ". /build/install-${config}/share/akantu-oracle/env.sh; /usr/bin/time -v -o time.txt /perf/builds/${config}/cohesive_extrinsic --aka_input_file input.dat > out.txt 2> err.txt; echo \$? > exit" || true
  rm -rf "${host}"; mkdir -p "${host}"
  docker run --rm -v "${vol}:/perf" "${img}" tar -C "${d}" -cf - . | tar -C "${host}" -xf -
  run_json "${host}" "${label}" cpp "${config}" "${material}" "${h}" "${rep}" "" "${start}" "${load}" "$(cat "${host}/exit" 2>/dev/null || echo 125)" "${cpu}"
}

run_rust() { # label spec material h rep
  local label=$1 spec=$2 material=$3 h=$4 rep=$5
  local bin="${spec%%,*}" extra="" d="${out}/${label}/${material}/h${h}/rep${rep}"
  [ "${bin}" = "${spec}" ] || extra="${spec#*,}"
  rm -rf "${d}"; mkdir -p "${d}"
  input_for "${material}" "${meshes}/cube-h${h}.msh" "${extra}" > "${d}/input.dat"
  local start load status=0
  start="$(date -u +%FT%TZ)"; load="$(cut -d' ' -f1 /proc/loadavg)"
  echo "${start} ${label} ${material} h${h} rep${rep} load=${load}" >> "${out}/schedule.log"
  (cd "${d}" && OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 ${pin_rust[@]+"${pin_rust[@]}"} /usr/bin/time -v -o time.txt "${bin}" --aka_input_file input.dat > out.txt 2> err.txt) || status=$?
  echo "${status}" > "${d}/exit"
  run_json "${d}" "${label}" rust "$(sha256sum "${bin}" | cut -d' ' -f1)" "${material}" "${h}" "${rep}" "${extra}" "${start}" "${load}" "${status}" "${cpu}"
}

n=${#variants[@]}
for rep in $(seq "${rep_start}" $((rep_start + reps - 1))); do
  for h in ${hs}; do
    for material in ${materials}; do
      for k in $(seq 0 $((n - 1))); do
        v="${variants[$(((k + rep - 1) % n))]}"
        label="${v%%=*}"; spec="${v#*=}"
        case "${spec}" in
          cpp:*) run_cpp "${label}" "${spec#cpp:}" "${material}" "${h}" "${rep}" ;;
          rust:*) run_rust "${label}" "${spec#rust:}" "${material}" "${h}" "${rep}" ;;
        esac
      done
    done
  done
done
echo "$(date -u +%FT%TZ) done" >> "${out}/schedule.log"
