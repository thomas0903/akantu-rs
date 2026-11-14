#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate golden fixtures from the C++ Akantu oracle.
# Documentation: docs/NUMERICAL_VALIDATION.md ("Generating fixtures").
#
# Usage: oracle/scripts/generate-fixtures.sh [options] <target>...
#   targets : selftest element fe_engine fe_extra mesh material model
#             cl_random core_init parsable all
#             (fe_extra, cl_random, core_init, parsable: probes,
#             fixtures/fe/engine_extra, fixtures/cl, fixtures/core,
#             fixtures/parser/subregistry)
#             mesh:<case_id>         one mesh_io case (e.g. mesh:mesh_io/test_io/x)
#             material:<case_id>     one material case (material:material/elastic/2d_shear)
#             model:<case_id>        one model case (model:model/static/patch_triangle_3)
#             regression:<case_id>   replay a pending_oracle case
#   --config NAME   oracle configuration (install prefix /build/install-NAME,
#                   default: strict)
#   --standalone    build header-only probes (element, selftest) against the
#                   configured source tree, without the installed library
#                   (usable before /build/install-NAME/READY exists)
#   --out DIR       fixtures root (default: <repo>/fixtures)
#   --memory SIZE   container memory cap (default 4g; protects concurrent
#                   oracle builds sharing the Docker VM)
#   --dry-run       validate the command line and print the container
#                   command without running it (no Docker, no checkout check)
# Environment: ORACLE_IMAGE (akantu-oracle:dev), ORACLE_VOLUME
#   (akantu-oracle-build), AKANTU_SRC_HOST (reference checkout),
#   AKANTU_ALLOW_IMAGE_MISMATCH=1 (record instead of refuse an install built
#   in another image).
#   AKANTU_PROBE_ALLOW_ON_VARIANT=1 (material cases: stage regular cases on an
#   oracle variant build, for the inertness check of docs/ORACLE_BUILD.md
#   "Oracle variants"; never for goldens).
#
# Runs entirely inside the oracle image; never modifies the reference
# checkout or the oracle build volume (both mounted read-only). Refuses a
# reference checkout that is not exactly the pinned commit with a clean
# working tree (no modified or untracked files).
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/." && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:-}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"

config=strict
mode=installed
out="${repo}/fixtures"
memory=4g
dry_run=0
targets=()
cmdline="oracle/scripts/generate-fixtures.sh $*"
usage() { echo "usage: $0 [--config NAME] [--standalone] [--out DIR] [--memory SIZE] [--dry-run] <target>..." >&2; exit 2; }
case_id_ok() { [[ "$1" =~ ^[A-Za-z0-9_.+-]+(/[A-Za-z0-9_.+-]+)+$ ]] && [[ ! "$1" =~ (^|/)\.\.?(/|$) ]]; }
while [ $# -gt 0 ]; do
  case "$1" in
    --config) [ $# -ge 2 ] || usage; config="$2"; shift 2 ;;
    --standalone) mode=standalone; shift ;;
    --out) [ $# -ge 2 ] || usage; out="$2"; shift 2 ;;
    --memory) [ $# -ge 2 ] || usage; memory="$2"; shift 2 ;;
    --dry-run) dry_run=1; shift ;;
    -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
    all) targets+=(selftest element fe_engine fe_extra mesh material model cl_random core_init parsable); shift ;;
    selftest|element|fe_engine|fe_extra|mesh|material|model|cl_random|core_init|parsable) targets+=("$1"); shift ;;
    mesh:mesh_io/*|material:material/*|model:model/*|regression:regression/*)
      case_id_ok "${1#*:}" || { echo "invalid case id in target $1" >&2; exit 2; }
      targets+=("$1"); shift ;;
    -*) echo "unknown option $1" >&2; usage ;;
    *) echo "unknown target $1 (see --help)" >&2; exit 2 ;;
  esac
done
[ ${#targets[@]} -gt 0 ] || { echo "no target given (see --help)" >&2; exit 2; }
[[ "${config}" =~ ^[A-Za-z0-9_-]+$ ]] || { echo "invalid config name ${config}" >&2; exit 2; }
if [ "${mode}" = standalone ]; then
  for t in "${targets[@]}"; do
    case "${t}" in selftest|element) ;; *) echo "target ${t} needs the installed library" >&2; exit 2 ;; esac
  done
fi

if [ "${dry_run}" = 1 ]; then
  echo "dry run: config=${config} mode=${mode} out=${out} targets=${targets[*]}"
  echo "would run: docker run ... ${ORACLE_IMAGE} bash /probes/run-in-container.sh ${config} ${mode} ${targets[*]}"
  exit 0
fi

[ -n "${AKANTU_SRC_HOST}" ] || { echo "set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)" >&2; exit 2; }
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
if [ "${sha}" != "${REF_SHA}" ]; then
  echo "reference checkout is at ${sha}, expected ${REF_SHA}" >&2
  exit 1
fi
dirty="$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)"
if [ -n "${dirty}" ]; then
  echo "reference checkout ${AKANTU_SRC_HOST} is not clean (golden data must come from the" >&2
  echo "exact pinned source); modified/untracked files:" >&2
  echo "${dirty}" | head -20 >&2
  exit 1
fi
describe="$(git -C "${AKANTU_SRC_HOST}" describe --tags --always 2>/dev/null || echo unknown)"
probes_commit="$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)"
if [ -n "$(git -C "${repo}" status --porcelain -- oracle/probes 2>/dev/null)" ]; then
  probes_commit="${probes_commit}+uncommitted-probe-changes"
fi
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
created="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"

echo "generate-fixtures: config=${config} mode=${mode} targets=${targets[*]} out=${out}"
frozen_mount=()
if [ -d "${repo}/fixtures/meshes" ]; then
  # Gmsh-generated meshes frozen per D-009 (oracle/probes/mesh_io/).
  frozen_mount=(-v "${repo}/fixtures/meshes:/frozen-meshes:ro")
fi
exec docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --init \
  --memory="${memory}" --memory-swap="${memory}" --shm-size=1g \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  ${frozen_mount[@]+"${frozen_mount[@]}"} \
  -v "${out}:/fixtures" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  -e AKANTU_PROBE_REFERENCE_DESCRIBE="${describe}" \
  -e AKANTU_PROBE_CREATED="${created}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e AKANTU_ALLOW_IMAGE_MISMATCH="${AKANTU_ALLOW_IMAGE_MISMATCH:-0}" \
  -e AKANTU_PROBE_ALLOW_ON_VARIANT="${AKANTU_PROBE_ALLOW_ON_VARIANT:-0}" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e OMPI_MCA_rmaps_base_oversubscribe=1 \
  "${ORACLE_IMAGE}" bash /probes/run-in-container.sh "${config}" "${mode}" "${targets[@]}"
