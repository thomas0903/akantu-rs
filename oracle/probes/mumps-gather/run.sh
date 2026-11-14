#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the oracle image (host wrapper: oracle/scripts/run-probe.sh).
# Builds the probe in /tmp against /build/install-<config> (volume may be
# read-only) and runs both variants on 1/2/4/8 ranks; JSON report on stdout.
set -uo pipefail
config="${1:-strict}"
src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bld="/tmp/mumps-gather-${config}"
. "/build/install-${config}/share/akantu-oracle/env.sh"
cmake -S "${src}" -B "${bld}" -G Ninja -DORACLE_CONFIG="${config}" >&2 \
  && cmake --build "${bld}" -j 2 >&2 || { echo '{"error": "build failed"}'; exit 2; }
cd "${bld}"
mkdir -p logs
: > runs.tsv
for variant in original corrected; do
  for n in 1 2 4 8; do
    mpiexec --oversubscribe --bind-to none -n "${n}" "./probe_${variant}" \
      > "logs/${variant}-${n}.log" 2>&1
    printf '%s\t%s\t%s\n' "${variant}" "${n}" "$?" >> runs.tsv
  done
done
python3 -I - "${config}" "${src}/probe.cc" "/build/install-${config}" <<'PY'
import hashlib, json, os, re, sys
cfg, src, inst = sys.argv[1:4]
res = []
for line in open("runs.tsv"):
    v, n, rc = line.split()
    log = open(f"logs/{v}-{n}.log", errors="replace").read()
    err = re.search(r"Error in the solution: [^(\n]*", log)
    vals = re.findall(r"\+ values\s*: (.*)", log)
    res.append({"variant": v, "ranks": int(n), "returncode": int(rc),
                "error": err.group(0).strip() if err else None,
                "rank0_checked_values": vals[-1] if vals else None,
                "log_sha256": hashlib.sha256(log.encode()).hexdigest()})
lib = os.path.realpath(os.path.join(inst, "lib", "libakantu.so"))
print(json.dumps({
    "probe": "mumps-gather", "config": cfg,
    "source_sha256": hashlib.sha256(open(src, "rb").read()).hexdigest(),
    "library": lib, "library_sha256": hashlib.sha256(open(lib, "rb").read()).hexdigest(),
    "image_id": os.environ.get("ORACLE_IMAGE_ID"),
    "results": res}, indent=1))
PY
