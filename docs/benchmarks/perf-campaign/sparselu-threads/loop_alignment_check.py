# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): checks, in a built
# x86-64 binary, that the two loops placed by `simd::align_next_loop`
# start on a 64-byte boundary (README, "The gemm's hot loop on a 64-byte
# boundary"; `loop-alignment/`).
#
#   python3 -I loop_alignment_check.py BINARY [BINARY ...]
#
# - the k loop of the AVX-512 16 x 8 gemm tile (non-FMA): in
#   `simd::x86::gemm_avx512`, every innermost loop with 16 `vmulpd` on zmm
#   registers and no fused multiply-add;
# - the L-scatter loop of `PanelUpdate::run_serial`: the innermost loop
#   with a `subsd`.
#
# Prints each loop head and its offset mod 64; exits 1 when one is not 0
# or a loop is not found. Needs `objdump` (binutils). Run it after every
# toolchain bump: the NOP counts of the directive compensate the
# preheaders that this rustc emits.
import re
import subprocess
import sys

INSN = re.compile(r"^\s*([0-9a-f]+):\s+(\S+)\s*(.*)$")
JUMP = re.compile(r"^j[a-z]+$")


def function(binary, symbol):
    """The instructions (address, mnemonic, operands) of one function."""
    out = subprocess.run(
        ["objdump", "-d", "--no-show-raw-insn", "-C", binary],
        check=True, capture_output=True, text=True).stdout
    head = re.compile(r"^[0-9a-f]+ <" + re.escape(symbol) + r">:$")
    insns, inside = [], False
    for line in out.splitlines():
        if not inside:
            inside = bool(head.match(line))
            continue
        if not line.strip():
            break
        m = INSN.match(line)
        if m:
            insns.append((int(m.group(1), 16), m.group(2), m.group(3)))
    return insns


def innermost_loops(insns):
    """(head, end) of every backward jump with no other backward jump inside."""
    loops = []
    for addr, op, args in insns:
        if JUMP.match(op):
            t = args.split()[0] if args else ""
            if re.fullmatch(r"[0-9a-f]+", t) and int(t, 16) <= addr:
                loops.append((int(t, 16), addr))
    return [lp for lp in loops
            if not any(o != lp and lp[0] <= o[0] and o[1] <= lp[1] for o in loops)]


def body(insns, loop):
    return [(op, args) for a, op, args in insns if loop[0] <= a <= loop[1]]


def check(binary):
    ok = True
    checks = [
        ("gemm 16x8 k loop", "akantu_linalg::eigen_sparse_lu::simd::x86::gemm_avx512",
         lambda b: sum(op == "vmulpd" and "zmm" in a for op, a in b) == 16
         and not any(op.startswith("vfmadd") for op, _ in b)),
        ("L-scatter loop", "<akantu_linalg::eigen_sparse_lu::kernels::PanelUpdate>::run_serial",
         lambda b: any(op == "subsd" for op, _ in b)),
    ]
    for name, symbol, pred in checks:
        insns = function(binary, symbol)
        found = [lp for lp in innermost_loops(insns) if pred(body(insns, lp))]
        if not insns or not found:
            print(f"{binary}: {name}: not found in {symbol}")
            ok = False
            continue
        for head, _ in found:
            off = head % 64
            print(f"{binary}: {name}: function 0x{insns[0][0]:x} (mod 64 = {insns[0][0] % 64}), "
                  f"loop head 0x{head:x}, mod 64 = {off}{'' if off == 0 else '  FAIL'}")
            ok &= off == 0
    return ok


def main():
    if len(sys.argv) < 2:
        print("usage: loop_alignment_check.py BINARY [BINARY ...]", file=sys.stderr)
        return 2
    return 0 if all([check(b) for b in sys.argv[1:]]) else 1


if __name__ == "__main__":
    sys.exit(main())
