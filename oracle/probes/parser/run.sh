#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Build the parser probe against the oracle install and dump every input-file
# fixture of crates/akantu-parser/tests/fixtures to
# crates/akantu-parser/tests/fixtures/parser/<relative path>.json.
#
# Run on the host from the repository root (needs Docker, the
# akantu-oracle:dev image and the akantu-oracle-build volume with
# /build/install-strict/READY):
#   oracle/probes/parser/run.sh
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)
repo="$(cd "$(dirname "$0")/../../.." && pwd)"
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} --memory 4g \
  -v akantu-oracle-build:/build:ro \
  -v "$repo":/work \
  -w /work \
  akantu-oracle:dev bash -c '
set -euo pipefail
test -f /build/install-strict/READY
cmake -S oracle/probes/parser -B /tmp/probe -G Ninja \
      -DAKANTU_ORACLE_PREFIX=/build/install-strict >/dev/null
cmake --build /tmp/probe -j1 >/dev/null
fixtures=crates/akantu-parser/tests/fixtures
out=$fixtures/parser
/tmp/probe/stream_blackbox > $out/stream_blackbox.jsonl
python3 oracle/probes/parser/cli_cases.py /tmp/probe/cli_probe > $out/cli_blackbox.jsonl
(cd $fixtures/registry && /tmp/probe/registry_probe registry_bytes.dat) > $out/registry_bytes.json
cat /build/install-strict/READY > /tmp/oracle-sha
cd $fixtures
find test examples extra_packages extra -type f \( -name "*.dat" -o -name "*.in" \) | sort | while read -r f; do
  mkdir -p "parser/$(dirname "$f")"
  if ! (cd "$(dirname "$f")" && /tmp/probe/dump_parser "$(basename "$f")") > "parser/$f.json" 2>/dev/null; then
    echo "{\"file\": \"$(basename "$f")\", \"crash\": true}" > "parser/$f.json"
  fi
done
# Compact the extra fixtures whose indented dumps would be large (deep trees).
for j in parser/extra/depth_*.json parser/extra/bytes.dat.json parser/extra/cycles.dat.json; do
  python3 -c "import json,sys; d=json.load(open(sys.argv[1])); open(sys.argv[1],\"w\").write(json.dumps(d,separators=(\",\",\":\"))+\"\\n\")" "$j"
done
{
  echo "oracle: /build/install-strict (strict), scripts $(cat /tmp/oracle-sha)"
  echo "reference: 35a851780d0b9471d258ed53a003ae98aaa95ea0"
  echo "boost: $(grep "define BOOST_LIB_VERSION" /usr/include/boost/version.hpp | cut -d\" -f2)"
  echo "compiler: $(g++ --version | head -1)"
  echo "glibc: $(ldd --version | head -1)"
} > parser/PROVENANCE.txt
'
