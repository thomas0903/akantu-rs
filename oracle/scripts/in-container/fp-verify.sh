#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Runs INSIDE the akantu-oracle:dev container: counts fused multiply-add
# instructions (aarch64 fmadd/fmsub/fnmadd/fnmsub scalar, fmla/fmls vector;
# x86_64 FMA3 vfmadd*/vfmsub*/vfnmadd*/vfnmsub*, incl. vfmaddsub*/vfmsubadd*)
# in the shared objects of an oracle install tree, overall and per function
# (top 15). Evidence for D-031: `strict` should contain (almost) none.
# Usage: fp-verify.sh <config> [more configs...]
set -euo pipefail
re='[[:space:]](fmadd|fmsub|fnmadd|fnmsub|fmla|fmls|vfn?m(add|sub)[0-9a-z]*)[[:space:]]'
for config in "$@"; do
  inst="/build/install-${config}"
  echo "=== ${config} (${inst})"
  for so in "${inst}"/lib/libakantu.so "${inst}"/lib/libiohelper.so \
            $(find "${inst}" -type f -name 'py11_akantu*.so'); do
    [ -e "${so}" ] || continue
    so="$(readlink -f "${so}")"
    total=$(objdump -d --no-show-raw-insn "${so}" | grep -cE "${re}" || true)
    insns=$(objdump -d --no-show-raw-insn "${so}" | grep -cE '^\s+[0-9a-f]+:' || true)
    echo "$(basename "${so}"): fused=${total} of ${insns} instructions"
    if [ "${total}" -gt 0 ]; then
      objdump -d --no-show-raw-insn -C "${so}" | awk -v re="${re}" '
        /^[0-9a-f]+ <.*>:$/ { fn = $0; sub(/^[0-9a-f]+ </, "", fn); sub(/>:$/, "", fn) }
        $0 ~ re { n[fn]++ }
        END { for (f in n) printf "%7d  %s\n", n[f], substr(f, 1, 160) }' \
        | sort -rn | head -15 || true   # head closes the pipe early (pipefail)
    fi
  done
done
