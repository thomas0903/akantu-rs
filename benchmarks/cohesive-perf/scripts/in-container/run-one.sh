#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs cohesive scalability harness: one run of the driver inside the
# oracle image (called by scripts/run-series.sh, gen-fixtures.sh and
# rerun-invalid.sh; one container per run so that the cgroup memory peak
# belongs to this run only).
#
# Usage: run-one.sh <run dir> <config> <ranks> <material> <mesh path> <steps> \
#                   [key=value ...]   (extra user-section parameters)
#
# <run dir> must be /perf/runs/<campaign>/.../rep<k> with plain components and
# no symlink; every argument is validated before the run
# directory is (re)created. Before the run, the build record of <config>
# (scripts/build.sh) is verified against the executable, the libraries it
# resolves and the image: a mismatch refuses the run (exit 3,
# refused.txt). Writes into <run dir>: input.dat, out.txt (stdout), err.txt
# (stderr, holds the rank-0 Chrono CSV), time.txt (GNU time of mpiexec),
# mem.trace, build-record.json (copy of the build's record), run.json (run
# provenance, incl. the run-time library check, host, environment and the
# single-node launch description), plus the driver's summary/insertion/energy
# files.
#
# Single node only: `mpiexec --oversubscribe -n N` in this container,
# no hostfile, no remote launch, no EFA; run.json records the container's CPU
# count and whether the run was oversubscribed.
set -euo pipefail
. /harness/scripts/in-container/validate.sh
[ $# -ge 6 ] || { echo "usage: run-one.sh <run dir> <config> <ranks> <material> <mesh> <steps> [k=v...]" >&2; exit 2; }
run_dir="$1"; config="$2"; ranks="$3"; material="$4"; mesh="$5"; steps="$6"
shift 6
perf_check_run_dir /perf/runs "${run_dir}" || exit 2
perf_confine_dir /perf/runs "${run_dir}" || exit 2
perf_check_config "${config}" || exit 2
perf_check_posint ranks "${ranks}" || exit 2
perf_check_material "${material}" || exit 2
perf_check_mesh_path "${mesh}" || exit 2
perf_check_posint steps "${steps}" || exit 2
for kv in "$@"; do perf_check_kv "${kv}" || exit 2; done

# Akantu seeds Scotch (and its RandomGenerator) from time(nullptr) unless
# AKA_SEED is set: a fixed seed makes multi-rank partitions reproducible.
export AKA_SEED="${AKA_SEED:-1}"
# The image's Scotch 7 partitions with several threads by default; its result
# then varies from run to run even with a fixed seed. One thread: reproducible.
export SCOTCH_PTHREAD_NUMBER="${SCOTCH_PTHREAD_NUMBER:-1}"
export AKANTU_RS_FILL_NODES_TO_ELEMENTS="${AKANTU_RS_FILL_NODES_TO_ELEMENTS:-upstream}"
perf_check_uint AKA_SEED "${AKA_SEED}" || exit 2
perf_check_posint SCOTCH_PTHREAD_NUMBER "${SCOTCH_PTHREAD_NUMBER}" || exit 2
perf_check_fnte "${AKANTU_RS_FILL_NODES_TO_ELEMENTS}" || exit 2
# Optional sparse solver of the static solve (akantu-rs,
# gen-fixtures-eigen.sh): empty = the upstream input (`_auto` = MUMPS).
[ -z "${PERF_STATIC_SPARSE_SOLVER:-}" ] \
  || perf_check_enum PERF_STATIC_SPARSE_SOLVER "${PERF_STATIC_SPARSE_SOLVER}" eigen mumps || exit 2

bin="/perf/builds/${config}/cohesive_extrinsic"
record="/perf/builds/${config}/build-record.json"
test -f "${mesh}" || { echo "ERROR: mesh ${mesh} missing" >&2; exit 1; }

# Validated and confined: replace the run directory (stale results of an
# earlier run must never survive a refused or failed run).
rm -rf "${run_dir}"
mkdir -p "${run_dir}"
cd "${run_dir}"

. "/build/install-${config}/share/akantu-oracle/env.sh"
export OMP_NUM_THREADS=1

if ! test -x "${bin}" || ! test -f "${record}"; then
  echo "ERROR: ${bin} or its build record missing: run scripts/build.sh ${config}" | tee refused.txt >&2
  exit 3
fi
cp "${record}" build-record.json
if ! python3 -I /harness/scripts/in-container/build_record.py verify build-record.json \
     > runtime-check.json 2> refused.txt; then
  cat refused.txt >&2
  exit 3
fi
rm -f refused.txt

# Derive the input from the upstream file: only mesh, max_steps and the
# optional parameters change (material, BCs and the increment schedule do not).
src="/harness/inputs/material-${material}.dat"
sedargs=(-e "s|^  max_steps = .*|  max_steps = ${steps}|" -e "s|^  mesh = .*|  mesh = ${mesh}|")
extra=""
for kv in "$@"; do
  k="${kv%%=*}"; v="${kv#*=}"
  if grep -qE "^  ${k} = " "${src}"; then
    sedargs+=(-e "s|^  ${k} = .*|  ${k} = ${v}|")
  else  # new user-section parameter: inserted after `user section [`
    extra="${extra}\\n  ${k} = ${v}"
  fi
done
[ -z "${extra}" ] || sedargs+=(-e "s|^user section \\[\$|&${extra}|")
# Upstream input syntax `sparse_solver_type` in the
# `time_step_solver static` section (e.g. `eigen`, whose result the Rust
# port reproduces bitwise).
if [ -n "${PERF_STATIC_SPARSE_SOLVER:-}" ]; then
  sedargs+=(-e "s|^    name = static\$|&\n    sparse_solver_type = ${PERF_STATIC_SPARSE_SOLVER}|")
fi
sed "${sedargs[@]}" "${src}" > input.dat

started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
load_start="$(cut -d' ' -f1-3 /proc/loadavg)"
status=0
# Memory trace of the container (cgroup v2): "<ms since start> <bytes>"
# every 0.5 s, to attribute the peak to a phase (mesh/model vs MUMPS).
t0="$(date +%s%N)"
( while :; do
    t="$(date +%s%N)"
    echo "$(( (t - t0) / 1000000 )) $(cat /sys/fs/cgroup/memory.current)"
    sleep 0.5
  done ) > mem.trace 2>/dev/null &
sampler=$!
launch=(mpiexec --oversubscribe -n "${ranks}" -x AKA_SEED -x SCOTCH_PTHREAD_NUMBER -x AKANTU_RS_FILL_NODES_TO_ELEMENTS)
/usr/bin/time -f '%e %M' -o time.txt \
  "${launch[@]}" "${bin}" --aka_input_file input.dat \
  > out.txt 2> err.txt &
run_pid=$!
# Early abort: once Akantu prints a MUMPS error/warning
# (UD-216 retry: the static solution is wrong) the run is invalid whatever
# follows, so stop it instead of paying for the explicit loop. The markers are
# those of scripts/perfcheck.py (MUMPS_MARKERS); collect.py and
# make_fixtures.py reject the run from its logs in any case.
( while kill -0 "${run_pid}" 2>/dev/null; do
    if grep -qF -e "MUMPS dynamic memory is insufficient" -e "The MUMPS workarray is too small" \
         -e "Error in mumps during solve process" -e "Warning in mumps during solve process" \
         out.txt err.txt 2>/dev/null; then
      echo "MUMPS error/warning (UD-216 class): run stopped early" > aborted.txt
      # SIGTERM to mpiexec (the child of GNU time), which forwards it to the
      # ranks; no procps needed
      kids="$(cat /proc/"${run_pid}"/task/*/children 2>/dev/null || true)"
      [ -n "${kids}" ] || for p in /proc/[0-9]*; do
        case "$(cat "${p}/comm" 2>/dev/null)" in mpiexec|prterun) kids="${kids} ${p#/proc/}" ;; esac
      done
      # shellcheck disable=SC2086
      [ -z "${kids}" ] || kill -TERM ${kids} 2>/dev/null || true
      break
    fi
    sleep 2
  done ) &
watcher=$!
wait "${run_pid}" || status=$?
kill "${sampler}" "${watcher}" 2>/dev/null || true
load_end="$(cut -d' ' -f1-3 /proc/loadavg)"
mem_peak="$(cat /sys/fs/cgroup/memory.peak 2>/dev/null || echo null)"
# OOM kills inside this container (cgroup v2 memory.events) and its limits:
# an out-of-memory kill of a rank is told apart from other failures
# (c1_session.sh's 64 -> 32 rank fall-back).
oom_kills=null
if [ -r /sys/fs/cgroup/memory.events ]; then
  while read -r k v; do if [ "${k}" = oom_kill ]; then oom_kills="${v}"; fi; done < /sys/fs/cgroup/memory.events
fi
mem_max="$(cat /sys/fs/cgroup/memory.max 2>/dev/null || echo null)"
wall=""; max_rss_kib=""
read -r wall max_rss_kib < <(tail -n 1 time.txt) || true

RUN_CONFIG="${config}" RUN_RANKS="${ranks}" RUN_MATERIAL="${material}" RUN_MESH="${mesh}" \
RUN_STEPS="${steps}" RUN_BIN="${bin}" RUN_STARTED="${started}" RUN_STATUS="${status}" \
RUN_WALL="${wall}" RUN_RSS="${max_rss_kib}" RUN_MEM_PEAK="${mem_peak}" \
RUN_LOAD_START="${load_start}" RUN_LOAD_END="${load_end}" RUN_LAUNCH="${launch[*]}" \
RUN_OOM_KILLS="${oom_kills}" RUN_MEM_MAX="${mem_max}" \
python3 -I - "$@" > run.json.tmp <<'PY'
import hashlib, json, os, platform, sys
E = os.environ
def sha(p):
    try:
        with open(p, "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()
    except OSError:
        return None
def num(s, conv):
    try:
        return conv(s)
    except (TypeError, ValueError):
        return None
def cpu_model():
    try:
        for line in open("/proc/cpuinfo"):
            if line.lower().startswith(("model name", "cpu model")):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or None
ranks = int(E["RUN_RANKS"])
ncpu = len(os.sched_getaffinity(0))
build = json.load(open("build-record.json"))
runtime = json.load(open("runtime-check.json"))
harness = {rel: sha(os.path.join("/harness", rel)) for rel in build["harness_sha256"]}
env_keys = ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS", "AKA_SEED",
            "SCOTCH_PTHREAD_NUMBER", "AKANTU_RS_FILL_NODES_TO_ELEMENTS", "LD_LIBRARY_PATH")
env = {k: E.get(k) for k in env_keys}
env.update({k: v for k, v in E.items() if k.startswith(("OMPI_MCA_", "PMIX_MCA_", "PRTE_MCA_"))})
print(json.dumps({
    "format": "cohesive-perf run v2",
    "config": E["RUN_CONFIG"], "ranks": ranks, "material": E["RUN_MATERIAL"],
    "mesh": E["RUN_MESH"], "mesh_sha256": sha(E["RUN_MESH"]), "max_steps": int(E["RUN_STEPS"]),
    "extra_parameters": sys.argv[1:], "input_sha256": sha("input.dat"),
    "binary_sha256": sha(E["RUN_BIN"]), "started_utc": E["RUN_STARTED"],
    "exit_status": int(E["RUN_STATUS"]), "wall_s": num(E["RUN_WALL"], float),
    "max_rss_per_process_kib": num(E["RUN_RSS"], int),
    "container_memory_peak_bytes": num(E["RUN_MEM_PEAK"], int),
    "container_oom_kills": num(E["RUN_OOM_KILLS"], int),
    "container_memory_max": E["RUN_MEM_MAX"],
    "loadavg_start": E["RUN_LOAD_START"], "loadavg_end": E["RUN_LOAD_END"],
    "omp_num_threads": E.get("OMP_NUM_THREADS"), "aka_seed": E.get("AKA_SEED"),
    "scotch_pthread_number": E.get("SCOTCH_PTHREAD_NUMBER"),
    "fill_nodes_to_elements": E.get("AKANTU_RS_FILL_NODES_TO_ELEMENTS"),
    "static_sparse_solver": E.get("PERF_STATIC_SPARSE_SOLVER") or None,
    "mpiexec": E["RUN_LAUNCH"],
    "launcher": "single node: mpiexec --oversubscribe in one container (no hostfile, "
                "no remote launch, no EFA; image OpenMPI transports)",
    "container_cpus": ncpu, "oversubscribed": ranks > ncpu,
    "host": {"machine": platform.machine(), "kernel": platform.release(),
             "cpu_model": cpu_model(), "logical_cpus": os.cpu_count()},
    "image": E.get("PERF_IMAGE"), "image_id": E.get("PERF_IMAGE_ID"),
    "build_record_sha256": runtime["build_record_sha256"],
    "runtime_check": runtime,
    "harness_sha256_at_run": harness,
    "harness_matches_build": harness == build["harness_sha256"],
    "aborted": open("aborted.txt").read().strip() if os.path.exists("aborted.txt") else None,
    "environment": env,
}, indent=1, sort_keys=True))
PY
mv run.json.tmp run.json
if [ "${status}" -ne 0 ]; then
  echo "ERROR: run failed (exit ${status}) in ${run_dir}" >&2
  tail -n 20 out.txt err.txt >&2
  exit "${status}"
fi
