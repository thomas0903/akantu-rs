#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: generate fixtures/solver/petsc/oracle/serial.<arch>[-<core>].json
# with the probe (oracle/probes/petsc/petsc_probe.cc) in
# akantu-oracle:dev, reference checkout and build volume read-only. Refuses
# an unpinned or dirty reference checkout and an existing output file (golden
# files are never replaced); fails (and writes nothing) unless the three runs
# are identical. The D-050 BLAS pin (scripts/mumps/blas-pin.sh: Haswell, one
# thread on x86_64) is passed into the oracle container and names the file
# (serial.x86_64-haswell.json); the probe pins Eigen's cache sizes. Run on
# the x86_64 oracle host:
#   AKANTU_SRC_HOST=/path/to/akantu oracle/probes/petsc/generate.sh [--config strict]
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "${here}/../../.." && pwd)"
ORACLE_IMAGE="${ORACLE_IMAGE:-akantu-oracle:dev}"
ORACLE_VOLUME="${ORACLE_VOLUME:-akantu-oracle-build}"
AKANTU_SRC_HOST="${AKANTU_SRC_HOST:?set AKANTU_SRC_HOST to the upstream Akantu checkout (commit 35a851780d0b)}"
REF_SHA="35a851780d0b9471d258ed53a003ae98aaa95ea0"
config=strict
cmdline="oracle/probes/petsc/generate.sh $*"
while [ $# -gt 0 ]; do
  case "$1" in
    --config) config="$2"; shift 2 ;;
    *) echo "usage: $0 [--config NAME]" >&2; exit 2 ;;
  esac
done
sha="$(git -C "${AKANTU_SRC_HOST}" rev-parse HEAD)"
[ "${sha}" = "${REF_SHA}" ] || { echo "reference checkout at ${sha}, expected ${REF_SHA}" >&2; exit 1; }
[ -z "$(git -C "${AKANTU_SRC_HOST}" status --porcelain --untracked-files=all)" ] ||
  { echo "reference checkout is not clean" >&2; exit 1; }
probes_commit="$(git -C "${repo}" rev-parse HEAD 2>/dev/null || echo unknown)"
if [ -n "$(git -C "${repo}" status --porcelain -- oracle/probes 2>/dev/null)" ]; then
  probes_commit="${probes_commit}+uncommitted-probe-changes"
fi
image_id="$(docker image inspect --format '{{.Id}}' "${ORACLE_IMAGE}")"
arch="$(uname -m)"
. "${repo}/scripts/mumps/blas-pin.sh"
pin_env=()
while IFS= read -r a; do pin_env+=("$a"); done < <(akantu_blas_docker_env)
suffix=""
if [ -n "${AKANTU_BLAS_CORETYPE}" ]; then
  suffix="-$(printf '%s' "${AKANTU_BLAS_CORETYPE}" | tr '[:upper:]' '[:lower:]')"
fi
outdir="${repo}/fixtures/solver/petsc/oracle"
out="${outdir}/serial.${arch}${suffix}.json"
[ ! -e "${out}" ] || { echo "${out} exists: golden files are never replaced" >&2; exit 1; }
mkdir -p "${outdir}"
tmp="$(mktemp)"
docker run --rm --init --memory=4g --memory-swap=4g --shm-size=1g \
  "${pin_env[@]}" \
  --user "$(id -u):$(id -g)" -e HOME=/tmp \
  -v "${AKANTU_SRC_HOST}:/akantu-src:ro" \
  -v "${ORACLE_VOLUME}:/build:ro" \
  -v "${repo}/oracle/probes:/probes:ro" \
  -e ORACLE_IMAGE_ID="${image_id}" \
  -e AKANTU_PROBE_COMMAND="${cmdline}" \
  -e AKANTU_PROBE_SOURCES_COMMIT="${probes_commit}" \
  -e AKANTU_PROBE_REFERENCE_SHA="${REF_SHA}" \
  "${ORACLE_IMAGE}" bash /probes/petsc/run.sh "${config}" > "${tmp}"
python3 -I - "${tmp}" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
p = d.get("provenance", {})
if "error" in d or p.get("returncodes") != [0, 0, 0] or not p.get("identical_runs"):
    sys.exit(f"petsc probe failed or not deterministic: {d.get('error', p)}")
PY
mv "${tmp}" "${out}"
echo "wrote ${out} ($(wc -c < "${out}") bytes)"
