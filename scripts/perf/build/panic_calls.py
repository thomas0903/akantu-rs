#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): per function of an
# x86_64 release binary, the number of instructions, of scalar/packed
# double add/sub/mul/div, of call sites of the bounds-check panics
# (core::panicking::panic_bounds_check, core::slice::index::*_fail) and of
# other panic paths. Calls go through the GOT in a PIE binary, so the GOT
# slots are resolved through the R_X86_64_RELATIVE relocations.
# Inputs in the current directory: got.txt (`objdump --dynamic-reloc BIN`,
# "slot target" per line), syms.txt (`objdump -t BIN`, "address name" of the
# panic functions); argument 1: `objdump -d --no-show-raw-insn BIN`
# (mangled names); further arguments: substrings of function names.
#   panic_calls.py BIN.asm PATTERN...
import re, sys, subprocess
got = {}
for l in open("got.txt"):
    a, t = l.split(); got[int(a, 16)] = int(t, 16)
syms = {}
for l in open("syms.txt"):
    a, n = l.split(maxsplit=1); syms[int(a, 16)] = n.strip()
def cls(n):
    if "panic_bounds_check" in n or "slice_index_fail" in n or "slice_end_index" in n or "slice_start_index" in n or "index_order_fail" in n: return "bounds"
    if "panic" in n or "unwrap" in n or "expect" in n or "_fail" in n: return "panic"
    return None
blocks = open(sys.argv[1]).read().split("\n\n")
for p in sys.argv[2:]:
    for b in blocks:
        m = re.search(r"^[0-9a-f]+ <([^>]*)>:", b, re.M)
        if not m or p not in m.group(1): continue
        ins = re.findall(r"^\s*[0-9a-f]+:\s+(\S+)\s*(.*)$", b, re.M)
        n = {"bounds": 0, "panic": 0}
        for i, a in ins:
            if not i.startswith("call"): continue
            t = None
            mm = re.search(r"# 0x([0-9a-f]+)", a)
            if mm and int(mm.group(1), 16) in got: t = got[int(mm.group(1), 16)]
            mm2 = re.match(r"0x([0-9a-f]+)", a)
            if mm2: t = int(mm2.group(1), 16)
            c = cls(syms.get(t, "")) if t is not None else None
            if c: n[c] += 1
        fl = sum(1 for i, a in ins if re.fullmatch(r"v?(add|sub|mul|div)[sp]d", i))
        cmpj = sum(1 for i, a in ins if i in ("cmpq", "cmpl") )
        print(f"{len(ins):6} instr {fl:5} fp {n['bounds']:3} bounds-panic calls {n['panic']:3} other panic calls {cmpj:4} cmp  {m.group(1)[:140]}")
