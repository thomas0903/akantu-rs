#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs test tooling (the enum check of python/tests/test_enums_upstream.py).
# Reads Akantu (EPFL/LSMS)
# sources: python/py_aka_common.cc, py_aka_error.cc, py_parser.cc and the C++
# enum definitions they bind.
"""Dump the enums bound by upstream's Python module from its *sources*.

This is NOT a run of upstream's module: it derives names and integer values
from the binding sources (`py::enum_<T>(...).value("name", c)` chains and the
`PY_AKANTU_REGISTER_*ENUM` macros of `python/py_aka_common.cc`) and from the
C++ enum definitions (`enum E { a, b = 3, ... }` and the Boost.PP sequence
macros `#define LIST (a)(b)...`), evaluating C++ implicit enumerator values.

Configuration assumed (upstream wheel): AKANTU_COHESIVE_ELEMENT and
AKANTU_STRUCTURAL_MECHANICS defined, AKANTU_IGFEM not defined.

Usage:  dump_upstream_enums.py [--reference DIR] [--output FILE]
DIR is the reference Akantu checkout (commit 35a851780d0b); without
--reference the script reads AKANTU_SRC_HOST and stops if neither is given.
FILE defaults to upstream_enums.json next to this script.  Pure standard
library; reading only.
"""

import argparse
import json
import os
import pathlib
import re
import sys

DEFINED = {"AKANTU_COHESIVE_ELEMENT", "AKANTU_STRUCTURAL_MECHANICS"}

HEADERS = [
    "src/common/aka_common.hh",
    "src/common/aka_element_classes_info.hh",
    "src/common/aka_error.hh",
    "src/common/aka_random_generator.hh",
    "src/io/parser/parser.hh",
    "src/io/parser/parameter_registry.hh",
]
BINDINGS = [
    "python/py_aka_common.cc",  # register_enums
    "python/py_aka_error.cc",  # register_error (DebugLevel)
    "python/py_parser.cc",  # register_parser (ParameterAccessType)
]


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def preprocess(text):
    """Join continuation lines, drop `#if defined(X)` branches not taken
    (only the `defined(...)` form, the only one used in the files read)."""
    text = text.replace("\\\n", " ")
    out, stack = [], []  # stack of (taking, parent_taking)
    taking = True
    for line in text.split("\n"):
        s = line.strip()
        m = re.match(r"#\s*if\s+defined\(\s*(\w+)\s*\)\s*$", s)
        if m:
            stack.append(taking)
            taking = taking and m.group(1) in DEFINED
            continue
        if re.match(r"#\s*if(?:n?def)?\b", s):  # any other #if: keep first branch
            stack.append(taking)
            continue
        if re.match(r"#\s*else\b", s):
            if stack:
                # flip only for the branches opened by `defined(...)`
                taking = stack[-1] and not taking
            continue
        if re.match(r"#\s*endif\b", s):
            taking = stack.pop() if stack else True
            continue
        if taking:
            out.append(line)
    return "\n".join(out)


def parse_seq_macros(text):
    """`#define NAME (a)(b)(c)` -> NAME: ['a','b','c'] (also NAME referring
    to other sequences, as AKANTU_ALL_ELEMENT_TYPE)."""
    raw = {}
    for m in re.finditer(r"^[ \t]*#\s*define\s+(\w+)(?!\()[ \t]*([^\n]*)$", text, re.M):
        raw[m.group(1)] = m.group(2).strip()

    def expand(body, depth=0):
        items = []
        pos = 0
        while pos < len(body):
            ws = re.match(r"\s+", body[pos:])
            if ws:
                pos += ws.end()
                continue
            par = re.match(r"\(\s*([^()]*?)\s*\)", body[pos:])
            if par:
                items.append(par.group(1))
                pos += par.end()
                continue
            ident = re.match(r"(\w+)", body[pos:])
            if ident and ident.group(1) in raw and depth < 8:
                items.extend(expand(raw[ident.group(1)], depth + 1))
                pos += ident.end()
                continue
            raise ValueError(f"cannot expand macro text: {body[pos:pos+40]!r}")
        return items

    seqs = {}
    for name, body in raw.items():
        if body.startswith("("):
            try:
                seqs[name] = expand(body)
            except ValueError:  # not a pure sequence macro
                pass
    # aliases such as AKANTU_ALL_ELEMENT_TYPE (a list of other macros)
    for name, body in raw.items():
        if name not in seqs and re.fullmatch(r"(\w+\s*)+", body or "x!"):
            try:
                seqs[name] = expand(body)
            except ValueError:
                pass
    return seqs


def eval_int(expr, known):
    expr = expr.strip()
    expr = re.sub(r"\b(\w+)\b", lambda m: str(known[m.group(1)]) if m.group(1) in known else m.group(1), expr)
    if not re.fullmatch(r"[0-9xXa-fA-F+\-|&<> ()]+", expr):
        raise ValueError(f"cannot evaluate {expr!r}")
    return int(eval(expr, {"__builtins__": {}}, {}))  # noqa: S307 (constants only)


def parse_enums(text, seqs):
    """name -> (is_class, {enumerator: value}) for `enum [class] N { ... }`
    and `AKANTU_CLASS_ENUM_DECLARE(N, LIST)` (class enum, `_` prefix)."""
    enums = {}
    for m in re.finditer(r"\benum\s+(class\s+)?(\w+)\s*\{(.*?)\}", text, re.S):
        is_class, name, body = bool(m.group(1)), m.group(2), m.group(3)
        items = []
        for part in re.split(r",(?![^()]*\))", body):
            part = part.strip()
            if not part:
                continue
            seq = re.fullmatch(r"BOOST_PP_SEQ_ENUM\(\s*(\w+)\s*\)", part)
            if seq:
                items.extend((n, None) for n in seqs[seq.group(1)])
                continue
            if "=" in part:
                n, e = part.split("=", 1)
                items.append((n.strip(), e.strip()))
            else:
                items.append((part, None))
        values, nxt = {}, 0
        for n, e in items:
            nxt = eval_int(e, values) if e is not None else nxt
            values[n] = nxt
            nxt += 1
        enums[name] = (is_class, values)
    for m in re.finditer(r"AKANTU_CLASS_ENUM_DECLARE\(\s*(\w+)\s*,\s*(\w+)\s*\)", text):
        name, lst = m.groups()
        enums[name] = (True, {"_" + n: i for i, n in enumerate(seqs[lst])})
    return enums


def parse_bindings(text, seqs):
    """Ordered list of {cls, scope, arithmetic, members: [(py_name, cpp)]}."""
    entries = []
    # explicit py::enum_<T>(scope, "Name"[, py::arithmetic()]) .value(...) ... ;
    for m in re.finditer(r"py::enum_<(\w+)>\(\s*(\w+)\s*,\s*\"(\w+)\"\s*(,\s*py::arithmetic\(\))?\s*\)(.*?);", text, re.S):
        cpp, scope, pyname, arith, chain = m.groups()
        members = [(a, b.strip()) for a, b in re.findall(r"\.value\(\s*\"(\w+)\"\s*,\s*([\w:]+)\s*\)", chain)]
        entries.append(
            dict(cpp=cpp, name=pyname, scope=scope, arithmetic=bool(arith), members=members, pos=m.start())
        )
    # PY_AKANTU_REGISTER_(CLASS_)ENUM(Type, LIST[(args)], mod)
    for m in re.finditer(r"PY_AKANTU_REGISTER_(CLASS_)?ENUM\(\s*(\w+)\s*,\s*(\w+)\s*(?:\(\s*(\w+)\s*\))?\s*,\s*(\w+)\s*\)\s*;", text):
        is_class, cpp, lst, extra, scope = m.groups()
        names = list(seqs[lst]) + ([extra] if extra else [])
        prefix = f"{cpp}::_" if is_class else ""
        members = [(n, prefix + n) for n in names]
        entries.append(dict(cpp=cpp, name=cpp, scope=scope, arithmetic=False, members=members, pos=m.start()))
    entries.sort(key=lambda e: e["pos"])
    return entries


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--reference", default=os.environ.get("AKANTU_SRC_HOST"),
                    help="reference Akantu checkout (default: $AKANTU_SRC_HOST)")
    ap.add_argument("--output", default=str(pathlib.Path(__file__).with_name("upstream_enums.json")))
    args = ap.parse_args()
    if not args.reference:
        ap.error("set AKANTU_SRC_HOST or pass --reference to the upstream Akantu checkout (commit 35a851780d0b)")
    ref = pathlib.Path(args.reference)

    seqs, enums = {}, {}
    for h in HEADERS:
        text = preprocess(strip_comments((ref / h).read_text()))
        seqs.update(parse_seq_macros(text))
    for h in HEADERS:
        text = preprocess(strip_comments((ref / h).read_text()))
        enums.update(parse_enums(text, seqs))

    entries = []
    for b in BINDINGS:
        text = preprocess(strip_comments((ref / b).read_text()))
        # macros are #defined in py_aka_common.cc itself: ignore them here
        text = re.sub(r"^[ \t]*#\s*define.*$", "", text, flags=re.M)
        entries.extend(parse_bindings(text, seqs))

    out_enums, exports = [], {}
    for e in entries:
        is_class, values = enums[e["cpp"]]
        members = []
        for py_name, cpp in e["members"]:
            key = cpp.split("::")[-1] if "::" in cpp else cpp
            members.append({"name": py_name, "value": values[key]})
            exports[py_name] = {"enum": e["name"], "scope": e["scope"], "value": values[key]}
        first = {}
        for mem in members:
            first.setdefault(mem["value"], mem["name"])
        out_enums.append(
            {
                "name": e["name"],
                "scope": "RandomDistributionType" if e["scope"] == "mod_rdt" else "module",
                "class_enum": is_class,
                "arithmetic": e["arithmetic"],
                "members": members,
                "str_name_of_value": {str(v): n for v, n in first.items()},
            }
        )
    # `export_values()` puts every member in the module namespace of
    # `scope`; later registrations win on collisions (module scope only).
    module_exports = {}
    for e in out_enums:
        if e["scope"] != "module":
            continue
        for mem in e["members"]:
            module_exports[mem["name"]] = {"enum": e["name"], "value": mem["value"]}

    doc = {
        "provenance": (
            "Derived from the SOURCES of Akantu @ 35a851780d0b9471d258ed53a003ae98aaa95ea0 "
            "(python/py_aka_common.cc, py_aka_error.cc, py_parser.cc and the C++ enum "
            "definitions in src/common/aka_common.hh, aka_element_classes_info.hh, aka_error.hh, "
            "aka_random_generator.hh, src/io/parser/parser.hh, parameter_registry.hh) by "
            "dump_upstream_enums.py in this directory; NOT from a run of the upstream "
            "Python module. Configuration: AKANTU_COHESIVE_ELEMENT and AKANTU_STRUCTURAL_MECHANICS "
            "defined, AKANTU_IGFEM not (upstream wheel). Member order is the registration order."
        ),
        "enums": out_enums,
        "module_exports": dict(sorted(module_exports.items())),
    }
    pathlib.Path(args.output).write_text(json.dumps(doc, indent=1, sort_keys=False) + "\n")
    n = sum(len(e["members"]) for e in out_enums)
    print(f"{len(out_enums)} enums, {n} members, {len(module_exports)} module-level names -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
