#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: writes the extra parser fixtures for deep
# nesting, cyclic references and non-UTF-8 bytes into
# crates/akantu-parser/tests/fixtures/extra/ (dumped by run.sh like every
# other fixture; compared by tests/oracle.rs). Deterministic.
#   python3 oracle/probes/parser/gen_extra.py crates/akantu-parser/tests/fixtures/extra
import pathlib
import sys

out = pathlib.Path(sys.argv[1])
files = {
    # Review reproduction: 130 nested parentheses (scalar and vector).
    "depth_paren_130.dat":
        "deep = " + "(" * 130 + "1" + ")" * 130 + "\n"
        "v = [" + "(" * 130 + "2" + ")" * 130 + "]\n",
    # Deeper than any former port limit but accepted by upstream (8 MiB stack).
    "depth_paren_5000.dat": "deep = " + "(" * 5000 + "1" + ")" * 5000 + "\n",
    "depth_signs_20000.dat": "deep = " + "-" * 20000 + "1\n",
    # Review reproduction: 257 nested sections, each with a variable
    # of the root scope.
    "depth_sections_257.dat":
        "a = 1\n" + "user [\n x = a + 1\n" * 257 + "]\n" * 257,
    # A chain of 300 variables and a matrix of two of them.
    "depth_chain_300.dat":
        "a0 = 1\n" + "".join(f"a{i} = a{i - 1} + 1\n" for i in range(1, 301))
        + "m = [[a300, a299]]\n",
    # Cyclic references (upstream: stack overflow) next to acyclic ones.
    "cycles.dat":
        "a = b\nb = a + 1\nc = c\nd = 2 * e\ne = [d]\nf = 1\ng = f + a\n",
}
for name, text in files.items():
    (out / name).write_text(text)
# Bytes >= 0x80 in values, comments, names (via comments).
(out / "bytes.dat").write_bytes(
    b"q = abc\xff\nE # caf\xe9\n = 2\nw = \xe9t\xe9 x\nl = [a\xff, b\xa0c]\n"
    b"r = 1 # \xff comment\nt = 1\xff\ns = \xa0 5\nv =\xa0[1, 2]\n"
    b"# \xff\xfe comment\nx = 1\nuser u [\n y = 2\xe9\n]\n")
# 0xa0 between a section type and '[' is not white space: parse error.
(out / "bytes_header.dat").write_bytes(b"user\xa0[ ]\n")
