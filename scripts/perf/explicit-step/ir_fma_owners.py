#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): which functions of an LLVM IR file call
# llvm.fma / llvm.fmuladd or carry fast-math flags (demangled with c++filt
# when available), grouped by module path.
import re, subprocess, sys
from collections import Counter
fn = None; fma = Counter(); flags = Counter()
fm = re.compile(r"= (fadd|fsub|fmul|fdiv|frem|fneg|fcmp)( | [a-z ]*)(fast|reassoc|contract|nnan|ninf|nsz|arcp|afn)")
for line in open(sys.argv[1]):
    m = re.match(r"define [^@]*@(\"?[^\s(\"]+\"?)\(", line)
    if m:
        fn = m.group(1).strip('"'); continue
    if fn and re.search(r"call [^@]*@llvm\.(fma|fmuladd)\.", line): fma[fn] += 1
    if fn and fm.search(line): flags[fn] += 1
def dem(names):
    try:
        out = subprocess.run(["c++filt"], input="\n".join(names), capture_output=True, text=True).stdout.split("\n")
    except Exception:
        out = names
    return out
for title, c in (("llvm.fma/fmuladd calls", fma), ("fast-math flags", flags)):
    names = list(c)
    groups = Counter()
    for n, d in zip(names, dem(names)):
        path = re.sub(r"::h[0-9a-f]{16}$", "", d)
        mod = "::".join(path.split("::")[:3])
        groups[mod] += c[n]
    print(f"{title}: {sum(c.values())} in {len(c)} functions; by module:")
    for k, v in groups.most_common():
        print(f"  {v:4d}  {k}")
