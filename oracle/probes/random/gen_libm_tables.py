#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs. Generates crates/akantu-core/src/random/libm_tables.rs,
# the constant tables of the glibc routines ported in
# crates/akantu-core/src/random/libm.rs (log, exp, pow, tan).
#
# The upstream copyright notices of those files are extracted and reproduced in
# the generated header.
#
# Values are parsed from the glibc sources of the oracle's libc
# (Debian glibc 2.41-12+deb13u4 = upstream 2.41 + debian/patches/git-updates.diff;
# files sysdeps/ieee754/dbl-64/{e_exp_data.c,e_log_data.c,e_pow_log_data.c,
# utan.h,utan.tbl}) and every table is located and verified byte-for-byte in
# the oracle's /usr/lib/aarch64-linux-gnu/libm.so.6, so the Rust tables are
# exactly the data the oracle executes.
#
# Usage:
#   gen_libm_tables.py --glibc-src <glibc-2.41 tree> --libm <libm.so.6> > libm_tables.rs
import argparse
import hashlib
import re
import struct
import sys


def fromhex_expr(text, n=128):
    """Evaluate a C constant like `-0x1.0000000000006p-2 * -2` or `0x1.8p52 / N`."""
    text = text.strip().rstrip(",")
    parts = re.split(r"\s*([*/])\s*", text)
    val = float.fromhex(parts[0])
    for op, operand in zip(parts[1::2], parts[2::2]):
        operand = float(n) if operand == "N" else float(operand)
        val = val * operand if op == "*" else val / operand
    return val


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def upstream_notice(path):
    """The complete leading notice block of a glibc source file.

    Returns every line of the file's first comment, with only the comment
    delimiters, the leading ``*`` decoration and the indentation removed
    (wording and line breaks unchanged), after checking that the file is under
    LGPL-2.1-or-later and carries a copyright line.
    """
    text = open(path).read()
    start = text.index("/*")
    end = text.index("*/", start)
    out = []
    for line in text[start + 2:end].split("\n"):
        line = line.strip()
        if line == "*":
            line = ""
        elif line.startswith("* "):
            line = line[2:]
        out.append(line.rstrip())
    while out and not out[0]:
        out.pop(0)
    while out and not out[-1]:
        out.pop()
    joined = " ".join(out)
    if not re.search(r"version 2\.1 of the License, or\s+\(at your option\) any later version",
                     joined):
        raise SystemExit(f"{path}: not LGPL-2.1-or-later")
    if not any(line.startswith("Copyright (C)") for line in out):
        raise SystemExit(f"{path}: no copyright line")
    return out


def notice_lines(d, names):
    """Comment lines reproducing the complete notices of `names`, identical
    blocks grouped with an explicit file mapping."""
    groups = []
    for name in names:
        notice = upstream_notice(d + name)
        for g in groups:
            if g[1] == notice:
                g[0].append(name)
                break
        else:
            groups.append(([name], notice))
    lines = []
    for names_, notice in groups:
        lines.append("// --- upstream notice of " + ", ".join(names_) + " ---")
        for line in notice:
            lines.append(("//   " + line) if line else "//")
    return lines


def block(src, field):
    m = re.search(r"\." + field + r"\s*=\s*\{(.*?)\n\},", src, re.S)
    if not m:
        raise SystemExit(f"field {field} not found")
    body = re.sub(r"//[^\n]*", "", m.group(1))
    body = re.sub(r"#[^\n]*", "", body)
    return body


def scalar(src, field):
    m = re.search(r"\." + field + r"\s*=\s*([^,\n]+),", src)
    if not m:
        raise SystemExit(f"field {field} not found")
    return fromhex_expr(m.group(1))


def hexfloats(body):
    return [fromhex_expr(t) for t in re.findall(r"-?0x[0-9a-fA-F.]+p[-+]?\d+(?:\s*[*/]\s*-?\w+)?", body)]


def hexints(body):
    return [int(t, 16) for t in re.findall(r"\b0x[0-9a-fA-F]+\b", body)]


class Elf:
    def __init__(self, path):
        self.data = open(path, "rb").read()
        self.sha256 = hashlib.sha256(self.data).hexdigest()

    def find_unique(self, words, what):
        pat = b"".join(struct.pack("<Q", w) for w in words)
        first = self.data.find(pat)
        if first < 0:
            raise SystemExit(f"{what}: not found in libm")
        if self.data.find(pat, first + 1) >= 0:
            raise SystemExit(f"{what}: found more than once in libm")
        return first


# Upstream files translated by the hand-written crates/akantu-core/src/random/libm.rs.
LIBM_RS_FILES = ["e_log.c", "e_exp.c", "e_pow.c", "math_config.h", "math_err.c", "s_tan.c",
                 "dla.h", "utan.h"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--glibc-src", required=True)
    ap.add_argument("--libm")
    ap.add_argument("--libm-rs-header", action="store_true",
                    help="print the notice header of the hand-written libm.rs and exit")
    args = ap.parse_args()
    d = args.glibc_src + "/sysdeps/ieee754/dbl-64/"
    if args.libm_rs_header:
        print("\n".join(notice_lines(d, LIBM_RS_FILES)))
        return
    if not args.libm:
        raise SystemExit("--libm is required")
    elf = Elf(args.libm)

    # ---- exp -------------------------------------------------------------
    src = open(d + "e_exp_data.c").read()
    exp = {
        "invln2n": scalar(src, "invln2N"),
        "negln2hin": scalar(src, "negln2hiN"),
        "negln2lon": scalar(src, "negln2loN"),
    }
    exp_poly = hexfloats(block(src, "poly"))
    exp_tab = hexints(block(src, "tab"))
    assert len(exp_poly) == 4 and len(exp_tab) == 256
    # Debian layout: invln2N, negln2hiN, negln2loN, poly[4], shift, ...
    elf.find_unique([bits(exp["invln2n"]), bits(exp["negln2hin"]), bits(exp["negln2lon"])]
                    + [bits(v) for v in exp_poly], "exp scalars")
    elf.find_unique(exp_tab, "exp tab")

    # ---- log -------------------------------------------------------------
    src = open(d + "e_log_data.c").read()
    log_ln2hi, log_ln2lo = scalar(src, "ln2hi"), scalar(src, "ln2lo")
    log_poly1 = hexfloats(block(src, "poly1"))
    log_poly = hexfloats(block(src, "poly"))
    log_tab = hexfloats(block(src, "tab"))
    assert len(log_poly1) == 11 and len(log_poly) == 5 and len(log_tab) == 256
    elf.find_unique([bits(log_ln2hi), bits(log_ln2lo)] + [bits(v) for v in log_poly]
                    + [bits(v) for v in log_poly1] + [bits(v) for v in log_tab], "log data")

    # ---- pow -------------------------------------------------------------
    src = open(d + "e_pow_log_data.c").read()
    pow_ln2hi, pow_ln2lo = scalar(src, "ln2hi"), scalar(src, "ln2lo")
    pow_poly = hexfloats(block(src, "poly"))
    pow_tab_raw = block(src, "tab")
    # Rows are `A(invc, logc, logctail)` with `#define A(a,b,c) {a,0,b,c},`.
    rows = re.findall(r"A\(([^()]*)\)", pow_tab_raw)
    pow_tab = []
    for r in rows:
        vals = [fromhex_expr(t) for t in r.split(",") if t.strip()]
        assert len(vals) == 3, r
        pow_tab.append(vals)
    assert len(pow_poly) == 7 and len(pow_tab) == 128
    words = [bits(pow_ln2hi), bits(pow_ln2lo)] + [bits(v) for v in pow_poly]
    off = elf.find_unique(words, "pow scalars")
    tab_off = off + 8 * len(words)
    for i, (invc, logc, logctail) in enumerate(pow_tab):
        got = struct.unpack_from("<QQQQ", elf.data, tab_off + 32 * i)
        if (got[0], got[2], got[3]) != (bits(invc), bits(logc), bits(logctail)):
            raise SystemExit(f"pow tab[{i}] mismatch")

    # ---- tan -------------------------------------------------------------
    uh = open(d + "utan.h").read()
    big = uh[uh.index("#ifdef BIG_ENDI"):uh.index("#ifdef LITTLE_ENDI")]
    tan_consts = {}
    for name, hi, lo in re.findall(r"(\w+)\s*=\s*\{\{0x([0-9a-fA-F]+),\s*0x([0-9a-fA-F]+)\}\s*\}", big):
        tan_consts[name] = (int(hi, 16) << 32) | int(lo, 16)
    need = ["d3", "d5", "d7", "d9", "d11", "e0", "e1", "mfftnhf", "g1", "g2", "g3", "g4",
            "gy2", "mp1", "mp2", "mp3", "hpinv", "toint"]
    for n in need:
        assert n in tan_consts, n
    ut = open(d + "utan.tbl").read()
    big = ut[ut.index("#ifdef BIG_ENDI"):ut.index("#ifdef LITTLE_ENDI")]
    pairs = re.findall(r"\{0x([0-9a-fA-F]+),\s*0x([0-9a-fA-F]+)\}", big)
    xfg = [(int(h, 16) << 32) | int(l, 16) for h, l in pairs]
    assert len(xfg) == 186 * 4
    elf.find_unique(xfg, "tan xfg")
    for n in ["hpinv", "mp1", "mp2", "mp3", "d11", "d9", "d7", "d5", "e0", "e1"]:
        if elf.data.find(struct.pack("<Q", tan_consts[n])) < 0:
            raise SystemExit(f"tan constant {n} not found in libm")

    # ---- emit ------------------------------------------------------------
    out = []
    w = out.append
    w("// SPDX-License-Identifier: LGPL-3.0-or-later")
    w("//")
    for line in notice_lines(d, ["e_exp_data.c", "e_log_data.c", "e_pow_log_data.c",
                                 "utan.h", "utan.tbl"]):
        w(line)
    w("//")
    w("// Part of akantu-rs, a port of Akantu (EPFL/LSMS): constant tables of the libm")
    w("// routines behind Akantu's random distributions (src/common/aka_random_generator.hh")
    w("// via the C++ standard library), translated to Rust from the GNU C Library 2.41")
    w("// (Debian 2.41-12+deb13u4) files above, sysdeps/ieee754/dbl-64/. As permitted by")
    w("// their \"version 2.1 of the License, or (at your option) any later version\"")
    w("// grant, this translation is distributed under the GNU Lesser General Public")
    w("// License version 3 or later (see NOTICE.md and docs/DEPENDENCIES.md). The")
    w("// exp/log/pow data were contributed to glibc from the Arm Optimized Routines")
    w("// project.")
    w("//")
    w("// GENERATED by oracle/probes/random/gen_libm_tables.py -- do not edit.")
    w(f"// Every table was verified byte-for-byte in the oracle's libm.so.6")
    w(f"// (sha256 {elf.sha256}).")
    w("")
    w("//! Constant tables for [`super::libm`] (generated, see file header).")
    w("")
    w("#![allow(clippy::unreadable_literal)] // bit patterns copied verbatim from glibc")
    w("")

    def f(name, v, doc):
        w(f"/// {doc}")
        w(f"pub(super) const {name}: f64 = f64::from_bits(0x{bits(v):016x});")

    def fb(name, b, doc):
        w(f"/// {doc}")
        w(f"pub(super) const {name}: f64 = f64::from_bits(0x{b:016x});")

    w("// ---- exp (`__exp_data`) ----")
    f("EXP_INVLN2N", exp["invln2n"], "`__exp_data.invln2N` = N/ln2.")
    f("EXP_NEGLN2HIN", exp["negln2hin"], "`__exp_data.negln2hiN` (high part of -ln2/N).")
    f("EXP_NEGLN2LON", exp["negln2lon"], "`__exp_data.negln2loN` (low part of -ln2/N).")
    for i, v in enumerate(exp_poly):
        f(f"EXP_C{i + 2}", v, f"`__exp_data.poly[{i}]` (C{i + 2}).")
    w("/// `__exp_data.tab`: `tab[2k]` = bits of T\\[k\\], `tab[2k+1]` = bits of H\\[k\\] - (k << 45).")
    w("pub(super) static EXP_TAB: [u64; 256] = [")
    for i in range(0, 256, 2):
        w(f"    0x{exp_tab[i]:016x}, 0x{exp_tab[i + 1]:016x},")
    w("];")
    w("")
    w("// ---- log (`__log_data`) ----")
    f("LOG_LN2HI", log_ln2hi, "`__log_data.ln2hi`.")
    f("LOG_LN2LO", log_ln2lo, "`__log_data.ln2lo`.")
    w("/// `__log_data.poly` (A\\[0..5\\]).")
    w("pub(super) const LOG_A: [f64; 5] = [")
    for v in log_poly:
        w(f"    f64::from_bits(0x{bits(v):016x}),")
    w("];")
    w("/// `__log_data.poly1` (B\\[0..11\\], used close to 1).")
    w("pub(super) const LOG_B: [f64; 11] = [")
    for v in log_poly1:
        w(f"    f64::from_bits(0x{bits(v):016x}),")
    w("];")
    w("/// `__log_data.tab`: `[invc, logc]` per subinterval.")
    w("pub(super) static LOG_TAB: [[f64; 2]; 128] = [")
    for i in range(0, 256, 2):
        w(f"    [f64::from_bits(0x{bits(log_tab[i]):016x}), f64::from_bits(0x{bits(log_tab[i + 1]):016x})],")
    w("];")
    w("")
    w("// ---- pow (`__pow_log_data`) ----")
    f("POW_LN2HI", pow_ln2hi, "`__pow_log_data.ln2hi`.")
    f("POW_LN2LO", pow_ln2lo, "`__pow_log_data.ln2lo`.")
    w("/// `__pow_log_data.poly` (A\\[0..7\\], pre-scaled as in glibc).")
    w("pub(super) const POW_A: [f64; 7] = [")
    for v in pow_poly:
        w(f"    f64::from_bits(0x{bits(v):016x}),")
    w("];")
    w("/// `__pow_log_data.tab`: `[invc, logc, logctail]` (the unused `pad` field is dropped).")
    w("pub(super) static POW_TAB: [[f64; 3]; 128] = [")
    for invc, logc, logctail in pow_tab:
        w(f"    [f64::from_bits(0x{bits(invc):016x}), f64::from_bits(0x{bits(logc):016x}), f64::from_bits(0x{bits(logctail):016x})],")
    w("];")
    w("")
    w("// ---- tan (`utan.h`, `utan.tbl`) ----")
    docs = {
        "d3": "polynomial I", "d5": "polynomial I", "d7": "polynomial I", "d9": "polynomial I",
        "d11": "polynomial I", "e0": "polynomial III", "e1": "polynomial III",
        "mfftnhf": "-15.5", "g1": "1.259e-8", "g2": "0.0608", "g3": "0.787", "g4": "25.0",
        "gy2": "0.0608", "mp1": "pi/2 split (1)", "mp2": "pi/2 split (2)", "mp3": "pi/2 split (3)",
        "hpinv": "2/pi", "toint": "1.5 * 2^52",
    }
    for n in need:
        fb(f"TAN_{n.upper()}", tan_consts[n], f"`utan.h` `{n}` ({docs[n]}).")
    w("/// `utan.tbl` `xfg[i][0..3]` = `[x_i, F_i, G_i]` for i = 16..201 (`FF_i` is unused).")
    w("pub(super) static TAN_XFG: [[f64; 3]; 186] = [")
    for i in range(186):
        a, b, c = xfg[4 * i], xfg[4 * i + 1], xfg[4 * i + 2]
        w(f"    [f64::from_bits(0x{a:016x}), f64::from_bits(0x{b:016x}), f64::from_bits(0x{c:016x})],")
    w("];")
    sys.stdout.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
