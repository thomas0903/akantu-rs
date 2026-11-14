#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: (D-043) corpus regeneration driver.
#
# Regenerates the whole oracle fixture corpus with every generator listed in
# docs/TESTING.md, IN PLACE inside a disposable staging clone of akantu-rs
# (never in the working copy whose fixtures are golden), against the oracle
# of the machine it runs on (an x86_64 oracle host).
# Afterwards the clone's fixtures/ and crates/*/tests/fixtures/ are the
# staged corpus; compare them with the committed corpus using
# oracle/x86/compare-corpus.sh.
#
# Usage: oracle/x86/regenerate-corpus.sh <staging-clone> <log-dir> [lane...]
#   lanes: core (generate-fixtures selftest/element/fe_engine/mesh/material,
#          the generic model cases one by one, the regression replay),
#          model (smm, smm_fd, solver_model, solver), mesh (mesh_io probe, mesh_utils, parallel,
#          frozen-mesh re-generation into <log-dir>/meshes), misc (dump,
#          random, parser, dense, gemm, complex division). Default: all four,
#          sequentially. Run lanes in separate processes to overlap them:
#          finalize_fixtures.py serialises their finalisation (flock on
#          the fixtures root).
# Each step logs to <log-dir>/<step>.log and records its exit code in
# <log-dir>/<step>.rc; a failing step does not stop the lane.
# Environment: AKANTU_SRC_HOST (reference checkout, required).
set -uo pipefail
clone="$(cd "${1:?usage: regenerate-corpus.sh <staging-clone> <log-dir> [lane...]}" && pwd)"
logs="${2:?log dir}"
shift 2
mkdir -p "${logs}"
logs="$(cd "${logs}" && pwd)"
[ -n "${AKANTU_SRC_HOST:-}" ] || { echo "AKANTU_SRC_HOST is not set" >&2; exit 2; }
lanes=("$@")
[ ${#lanes[@]} -gt 0 ] || lanes=(core model mesh misc)

step() { # step <name> <command...>
  local name="$1"; shift
  echo "[$(date -u +%H:%M:%S)] start ${name}: $*" | tee -a "${logs}/steps.log"
  ( cd "${clone}" && "$@" ) > "${logs}/${name}.log" 2>&1
  local rc=$?
  echo "${rc}" > "${logs}/${name}.rc"
  echo "[$(date -u +%H:%M:%S)] done  ${name}: rc=${rc}" | tee -a "${logs}/steps.log"
}

# Case ids of fixtures/model/* written by generate-fixtures.sh itself (not by
# the smm/smm_fd probes): `generate-fixtures.sh all` (or `model`) treats the
# whole `model` area as its own and refuses to finalise because the smm cases
# look stale ("finalisation failed (nothing was modified)"), so the generic
# model cases are regenerated one by one as model:<case_id> targets.
generic_model_targets() {
  python3 -I - "${clone}/fixtures" <<'PY'
import glob, json, os, sys
root = sys.argv[1]
for f in sorted(glob.glob(os.path.join(root, "model", "**", "manifest.json"), recursive=True)):
    m = json.load(open(f))
    if "generate-fixtures.sh" in m.get("generator", {}).get("command", ""):
        print("model:" + m["case_id"])
PY
}

lane_core() {
  step gen-core oracle/scripts/generate-fixtures.sh --config strict \
    selftest element fe_engine mesh material
  # shellcheck disable=SC2046
  step gen-model oracle/scripts/generate-fixtures.sh --config strict $(generic_model_targets)
  step gen-regression oracle/scripts/generate-fixtures.sh --config strict \
    regression:regression/material/replay_elastic_2d
}

lane_model() {
  step smm oracle/probes/smm/generate.sh
  step smm_fd env SMM_PROBE_BIN=smm_fd_probe oracle/probes/smm/generate.sh
  step solver_model oracle/probes/solver_model/generate.sh
  step solver oracle/probes/solver/generate.sh
}

lane_mesh() {
  step mesh_io_probe oracle/probes/mesh_io/generate.sh
  step mesh_utils oracle/probes/mesh_utils/generate.sh
  step parallel oracle/probes/parallel/generate.sh
  mkdir -p "${logs}/meshes"
  step freeze_meshes oracle/probes/mesh_io/freeze-meshes.sh --out "${logs}/meshes"
}

lane_misc() {
  step dump oracle/probes/dump/run.sh
  step random oracle/probes/random/run.sh
  step parser oracle/probes/parser/run.sh
  step dense oracle/probes/dense/run.sh
  step gemm oracle/probes/dense/run_gemm.sh
  step complex_div oracle/probes/dense/complex_cleanroom/run.sh
}

for l in "${lanes[@]}"; do
  case "${l}" in
    core|model|mesh|misc) "lane_${l}" ;;
    dump|random) step "${l}" "oracle/probes/${l}/run.sh" ;;  # single steps (re-runs)
    *) echo "unknown lane ${l}" >&2; exit 2 ;;
  esac
done
echo "[$(date -u +%H:%M:%S)] lanes ${lanes[*]} finished" | tee -a "${logs}/steps.log"
