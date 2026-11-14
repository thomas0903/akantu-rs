#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the oracle image (host wrapper: oracle/scripts/run-probe.sh).
# Builds the UD-131 probe in /tmp against /build/install-<config> (volume
# read-only), runs it twice (determinism) and prints its JSON report.
set -uo pipefail
config="${1:-strict}"
src="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
bld="/tmp/ud131-${config}"
. "/build/install-${config}/share/akantu-oracle/env.sh"
cmake -S "${src}" -B "${bld}" -G Ninja -DORACLE_CONFIG="${config}" >&2 \
  && cmake --build "${bld}" -j 2 >&2 || { echo '{"error": "build failed"}'; exit 2; }
cd "${bld}"
./ud131_probe > run1.json 2> run1.err
rc1=$?
./ud131_probe > run2.json 2> run2.err
rc2=$?
python3 -I - "${config}" "${src}/probe.cc" "/build/install-${config}" "${rc1}" "${rc2}" <<'PY'
import hashlib, json, os, sys
cfg, src, inst, rc1, rc2 = sys.argv[1:6]
r1 = open("run1.json").read()
r2 = open("run2.json").read()
lib = os.path.realpath(os.path.join(inst, "lib", "libakantu.so"))
out = {
    "probe": "ud131", "config": cfg,
    "source_sha256": hashlib.sha256(open(src, "rb").read()).hexdigest(),
    "library": lib, "library_sha256": hashlib.sha256(open(lib, "rb").read()).hexdigest(),
    "image_id": os.environ.get("ORACLE_IMAGE_ID"),
    "returncodes": [int(rc1), int(rc2)],
    "deterministic": r1 == r2,
}
try:
    out["report"] = json.loads(r1)
except ValueError as e:
    out["report_error"] = str(e)
    out["report_text"] = r1[-4000:]
    out["stderr_tail"] = open("run1.err", errors="replace").read()[-4000:]
print(json.dumps(out, indent=1))
PY
