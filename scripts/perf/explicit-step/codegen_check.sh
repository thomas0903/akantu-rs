#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): generated code of the
# explicit-step hot kernels. (1) LLVM IR of the four crates touched
# (release profile of the workspace): fast-math flags on floating-point
# instructions (fast, reassoc, contract, nnan, ninf, nsz, arcp, afn) and the
# fused intrinsics llvm.fma / llvm.fmuladd, counted per crate and inside the
# functions of the fast paths; (2) machine code of the release binary BIN:
# FMA instructions (x86: vfmadd/vfmsub/vfnmadd/vfnmsub; aarch64: fmadd, fmsub,
# fnmadd, fnmsub, fmla, fmls), horizontal adds (haddpd, faddp) and packed
# double arithmetic inside the hot functions.
#   codegen_check.sh REPO TARGET_DIR OUT_DIR BIN...
set -u
repo=$1 tdir=$2 out=$3; shift 3; mkdir -p "$out"
cd "$repo" || exit 1
for c in akantu-core akantu-fe akantu-model akantu-solid akantu-solver akantu-cohesive; do
  CARGO_TARGET_DIR="$tdir" cargo rustc --release --locked -q -p $c --lib -- --emit=llvm-ir -C codegen-units=1 > /dev/null 2>&1
  # shellcheck disable=SC2012 # the newest .ll file of the crate; names have no spaces
  ll=$(ls -t "$tdir"/release/deps/"$(echo "$c" | tr - _)"-*.ll | head -1)
  flags=$(grep -cE '= (fadd|fsub|fmul|fdiv|frem|fneg|fcmp)( | [a-z ]*)(fast|reassoc|contract|nnan|ninf|nsz|arcp|afn)' "$ll")
  fma=$(grep -cE 'call [^@]*@llvm\.(fma|fmuladd)\.' "$ll")
  fops=$(grep -cE '= (fadd|fsub|fmul|fdiv) ' "$ll")
  echo "IR $c: $fops fadd/fsub/fmul/fdiv instructions, $flags with fast-math flags, $fma llvm.fma/fmuladd calls ($(basename "$ll"))"
  cp "$ll" "$out/$c.ll"
done
# functions of the fast paths in the IR (mangled names contain these paths)
for pat in 'fast8gradient' 'fast3btd' 'fast9integrate' 'fast9residuals' '11scatter_add' '15tr_matmul_small' '11matmul_small' '16grad_u_in_place' '14eigen_unrolled' '15gradient_kernel' '14variation_into' '35gradient_on_integration_points_view' '31interpolate_elemental_field_with'; do
  n=$(grep -hE "^define .*$pat" "$out"/*.ll | grep -c .)
  echo "IR functions matching $pat: $n"
done
for bin in "$@"; do
# machine code
objdump -d --no-show-raw-insn -C "$bin" > "$out/bin.asm"
echo "binary $(sha256sum "$bin" | cut -c1-16): total instructions $(grep -cE '^\s*[0-9a-f]+:' "$out/bin.asm")"
echo "  FMA instructions (x86 vfmadd/vfmsub/vfnmadd/vfnmsub, aarch64 fmadd/fmsub/fnmadd/fnmsub/fmla/fmls): $(grep -cE '\s(vfn?m(add|sub)[0-9a-z]*|fn?m(add|sub)|fml[as])\s' "$out/bin.asm")"
echo "  horizontal adds (haddpd, faddp): $(grep -cE '\s(v?haddp[sd]|faddp)\s' "$out/bin.asm")"
echo "  AVX (vex-encoded) instructions: $(grep -cE '\sv[a-z]+(pd|sd|ps|ss)\s' "$out/bin.asm")"
python3 - "$out/bin.asm" <<'PY'
import re, sys
asm = open(sys.argv[1]).read().split("\n\n")
pats = ["engine::fast::gradient", "engine::fast::btd", "engine::fast::integrate",
        "finite_deformation::fast::residuals", "finite_deformation::fast::run", "scatter_add",
        "material_tensor::tr_matmul_small", "material_tensor::matmul_small", "material_tensor::s_to_cauchy",
        "material_tensor::grad_u_to_green", "grad_u_in_place", "assemble_elemental_array_local_array",
        "integrate_with_jacobians", "compute_btd", "gradient_kernel", "internal_forces",
        "variation_into", "gradient_on_integration_points_view", "interpolate_elemental_field_with"]
for p in pats:
    blocks = [b for b in asm if re.search(r"^[0-9a-f]+ <[^>]*" + re.escape(p), b, re.M)]
    if not blocks:
        print(f"  {p}: not a separate symbol (inlined)")
        continue
    txt = "\n".join(blocks)
    ins = re.findall(r"^\s*[0-9a-f]+:\s+(\S+)", txt, re.M)
    c = lambda rx: sum(1 for i in ins if re.fullmatch(rx, i))
    print(f"  {p}: {len(blocks)} symbol(s), {len(ins)} instr; scalar add/sub/mul sd {c(r'v?(add|sub|mul)sd')}, "
          f"packed add/sub/mul pd {c(r'v?(add|sub|mul)pd')}, haddpd {c(r'v?haddpd')}, FMA {c(r'v?fn?m(add|sub)\w*|fn?m(add|sub)|fml[as]')}, "
          f"aarch64 fadd/fmul {c(r'fadd|fmul|fsub')}")
fma_rx = re.compile(r"^\s*[0-9a-f]+:\s+(v?fn?m(add|sub)\w*|fn?m(add|sub)|fml[as])\s", re.M)
holders = {}
for b in asm:
    m = re.search(r"^[0-9a-f]+ <([^>]*)>:", b, re.M)
    if m and fma_rx.search(b):
        holders[m.group(1)[:160]] = len(fma_rx.findall(b))
print(f"  functions holding FMA instructions: {len(holders)}")
for k, v in sorted(holders.items()):
    print(f"    {v} in {k}")
PY
done
