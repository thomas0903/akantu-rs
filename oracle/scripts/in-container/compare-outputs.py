#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, the Rust port of Akantu (EPFL/LSMS). Oracle tooling, not
# derived from an Akantu source file.
"""Compare the printed numerical outputs of two oracle output trees.

Compared files: every test/example stdout capture (`*.lastout`, written by
upstream's akantu_test_driver.sh) and every generated `*.csv` present in either
tree, excluding CMake/CTest internals. Typical uses: `strict` vs `default`
build tree, or two output snapshots of the same configuration (run-to-run).

Design:

* No line is discarded by a broad pattern. Volatile *fields* (wall-clock
  timings, heap addresses, dates, absolute build-tree paths, progress-bar
  refreshes) are removed only by the explicit, anchored, file-scoped rules in
  RULES below; every masked field and dropped segment is counted per file and
  per rule in the report.
* Files that are not compared (gtest captures, files present in only one
  tree, files not produced by the run under comparison) are listed with the
  reason; nothing silently disappears.
* Tokens are compared as text first. A token pair that differs as text is
  classified exactly: same decimal value but different formatting
  (`format_only`), different decimal value that rounds to the same IEEE-754
  binary64 (`f64_equal`), different value (`value`), integer tokens compared as
  arbitrary-precision integers (`integer`, no float collapse), `+0` vs `-0`
  (`signed_zero`), and non-finite mismatches (`nonfinite`, e.g. nan vs 1,
  inf vs -inf, nan vs -nan) whose error metric is reported as "inf".
* Statuses: "identical_bytes" (raw files byte-identical);
  "identical_after_normalization" (every compared token text-identical once
  the counted volatile fields are masked); "value_equal_format_diff" (same
  decimal values, different formatting); "numeric_diff"; "reordered_lines"
  (same multiset of lines in another order, e.g. interleaved stdout of MPI
  ranks); "structure_diff" (text skeleton or token count differs); "stale"
  (not produced by the run under comparison). Files without numeric tokens
  and empty files are counted separately (`numeric_tokens`, `empty`) so that
  they are never mistaken for numerical agreement.
* Provenance: if a tree contains MANIFEST.json (written by ctest.sh output
  snapshots or isolated-runs.py), files whose manifest entry says
  `produced_in_run: false` are reported as `stale` (not counted as a
  comparison). `--window-a/--window-b START END` (epoch seconds) apply the same
  check from file mtimes for trees without a manifest.

Usage:
  compare-outputs.py [options] TREE_A TREE_B > report.json
  compare-outputs.py --self-test
Options:
  --window-a START END / --window-b START END   provenance window (mtime)
  --source-root DIR  exclude files byte-identical to DIR/<same path> (input copies)
  --suffixes LIST    comma-separated suffixes to compare (default .lastout,.csv), or all
  --exit-status    exit 1 unless every compared file is identical (bytes or
                   after normalization) and nothing is stale/only-in-one-tree
"""
import argparse
import decimal
import hashlib
import json
import math
import os
import re
import sys
import tempfile
from fnmatch import fnmatch

SUFFIXES = (".lastout", ".csv")
SKIP_DIRS = ("CMakeFiles", "Testing")
MANIFEST = "MANIFEST.json"

# Numeric token: decimal integer/fixed/scientific, or a non-finite word.
_NUM = r"[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?"
_NONFINITE = r"(?<![A-Za-z_])[-+]?(?:nan|inf(?:inity)?)(?![A-Za-z_0-9])"
TOKEN = re.compile(rf"{_NONFINITE}|{_NUM}", re.I)
# Field regex used inside rules (a printed number, incl. exponent).
N = r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?"

# --------------------------------------------------------------------------
# Normalization rules. Each rule is anchored to a whole line (or, for
# kind "drop-segment", a whole carriage-return segment of a line), scoped to a
# file glob, and either masks the named groups starting with "v" (volatile
# fields; the rest of the line is still compared) or drops the segment.
# Every rule carries the justification for why the field is not scientific
# output. Add rules only with such a justification (docs/ORACLE_BUILD.md).
# --------------------------------------------------------------------------
RULES = [
    {
        "id": "progress-bar",
        "kind": "drop-segment",
        "files": "examples/c++/phase_field_model/*",
        "regex": rf"\[[-\\|/]\]\s+\d+/\d+ \(\s*{N}ms/step - elapsed:\s*{N}s - ETA:\s*{N}s\)\x14*",
        "why": "wall-clock progress display (refreshed by time, written with \\r): the number "
               "of refreshes and their ms/step, elapsed and ETA values depend on timing",
    },
    {
        "id": "gmsh-wall-cpu",
        "kind": "mask",
        "files": "*",
        "regex": rf"Info\s+: .*\(Wall (?P<v1>{N})s, CPU (?P<v2>{N})s\)",
        "why": "Gmsh mesher timing report (wall/CPU seconds) printed while meshing",
    },
    {
        "id": "gmsh-date",
        "kind": "mask",
        "files": "*",
        "regex": r"Info\s+: (?:Started|Stopped) on (?P<v1>\w{3} \w{3} [ \d]\d \d\d:\d\d:\d\d \d{4})"
                 rf"(?: \(From start: Wall (?P<v2>{N})s, CPU (?P<v3>{N})s\))?",
        "why": "Gmsh start/stop wall-clock date and timing",
    },
    {
        "id": "heap-address",
        "kind": "mask",
        "files": "*",
        "regex": r"\s*\+ address\s+: (?P<v>0x[0-9a-f]+)",
        "why": "heap address of an Array printed by Array::printself",
    },
    {
        "id": "build-tree-path",
        "kind": "mask",
        "files": "*",
        "regex": r"rootdir: (?P<v>/build/[A-Za-z0-9_-]+)(?:/\S*)?",
        "why": "pytest header: absolute path of the build tree (/build/strict vs /build/default)",
    },
    {
        "id": "pytest-duration",
        "kind": "mask",
        "files": "*",
        "regex": r"(?P<v1>=+) (?:\d+ (?:passed|failed|skipped|errors?|warnings?|xfailed|xpassed)"
                 r"(?:, )?)+ in (?P<v2>\d+\.\d+)s(?: \(\d+:\d\d:\d\d\))? (?P<v3>=+)",
        "why": "pytest summary: wall-clock duration and the '=' padding whose width depends on it "
               "(the pass/fail counts are kept)",
    },
    {
        "id": "gtest-xml-time",
        "kind": "mask",
        "files": "*.xml",
        "regex": r"\s*<(?:testsuites|testsuite|testcase) .* time=\"(?P<v1>[0-9.]+)\" "
                 r"timestamp=\"(?P<v2>[0-9T:.+-]+)\".*",
        "why": "gtest XML report: wall-clock duration and start timestamp of a suite/case "
               "(names, counts, statuses and failure messages are kept)",
    },
    {
        "id": "gtest-console-ms",
        "kind": "mask",
        "files": "*",
        "regex": r"\[(?: {7}OK |  FAILED  |  SKIPPED )\] \S.* \((?P<v>\d+) ms\)"
                 r"|\[-{10}\] \d+ tests? from \S.* \((?P<v_>\d+) ms total\)"
                 r"|\[={10}\] \d+ tests? from \d+ test suites? ran\. \((?P<v__>\d+) ms total\)",
        "why": "gtest console: wall-clock duration of a case/suite/run (same fields as "
               "upstream's driver ignore pattern '([0-9]+ ms( total)?)')",
    },
    {
        "id": "akantu-debug-prefix",
        "kind": "mask",
        "files": "*",
        "regex": r"<(?P<v1>\d+)>\[R\d+\|S\d+\] \{(?P<v2>\d+)\} .*",
        "why": "Akantu debug/exception message prefix: process id and microsecond wall-clock "
               "timestamp (rank/size and the message are kept)",
    },
    {
        "id": "mpi-process-ids",
        "kind": "mask",
        "files": "*",
        "regex": r"\s*Process name: \[prterun-(?P<v1>[0-9a-f]+)-(?P<v2>\d+)@\d+,(?P<v8>\d+)\]"
                 r"|prterun noticed that process rank \d+ with PID (?P<v3>\d+) on node "
                 r"(?P<v4>[0-9a-f]+) exited on"
                 r"|\[(?P<v5>[0-9a-f]+):(?P<v6>\d+)\] (?:PMIX ERROR: .*|\*\*\* .*|Signal: .*"
                 r"|Signal code: .*|Failing at address: (?P<v7>0x[0-9a-f]+))",
        "why": "Open MPI/PMIx launcher and signal-handler messages: container host name, "
               "process ids, the faulting address (ASLR) and which rank happened to exit "
               "first (an exit-order race)",
    },
    {
        "id": "mpi-backtrace-address",
        "kind": "mask",
        "files": "*",
        "regex": r"\[(?P<v1>[0-9a-f]+):(?P<v2>\d+)\] \[ ?\d+\] \S.* \[(?P<v3>0x[0-9a-f]+)\]",
        "why": "Open MPI signal-handler backtrace frame: host name, pid and absolute "
               "return address (ASLR); the symbol+offset is kept",
    },
    {
        "id": "scalability-chrono",
        "kind": "mask",
        "files": "examples/c++/scalability_test/*",
        "regex": rf"Passing the first \w+ chrono! \[(?P<v>{N})\]",
        "why": "scalability example: wall-clock timer of each phase",
    },
    {
        "id": "scalability-time",
        "kind": "mask",
        "files": "examples/c++/scalability_test/*",
        "regex": rf"(?:Full|Init) time: (?P<v>{N})",
        "why": "scalability example: wall-clock time",
    },
    {
        "id": "scalability-step-time",
        "kind": "mask",
        "files": "examples/c++/scalability_test/*",
        "regex": rf"Step time: (?P<v1>{N}) - nb_steps: \d+ - time_per_step: (?P<v2>{N})",
        "why": "scalability example: wall-clock step timing (nb_steps is kept)",
    },
    {
        "id": "contact-search-time",
        "kind": "mask",
        "files": "examples/python/contact_mechanics_model/compression/*",
        "regex": rf"Search time = %s seconds (?P<v>{N})",
        "why": "python contact detection example: wall-clock duration of the search",
    },
]
for _r in RULES:
    _r["re"] = re.compile(_r["regex"] + r"\Z")
    assert not re.search(r"\d", _r["id"]), "rule ids must not contain digits"


def _rules_for(rel):
    return [r for r in RULES if fnmatch(rel, r["files"])]


def normalize(text, rules, counts):
    """Apply the rules to `text`; returns the list of normalized lines.

    `counts[rule_id]` is incremented for every masked field / dropped segment.
    """
    out = []
    for line in text.split("\n"):
        segs = line.split("\r")
        kept = []
        for seg in segs:
            drop = False
            for r in rules:
                if r["kind"] != "drop-segment":
                    continue
                if r["re"].match(seg):
                    counts[r["id"]] = counts.get(r["id"], 0) + 1
                    drop = True
                    break
            if drop:
                continue
            for r in rules:
                if r["kind"] != "mask":
                    continue
                m = r["re"].match(seg)
                if not m:
                    continue
                spans = sorted((m.span(g) for g, v in m.groupdict().items()
                                if g.startswith("v") and v is not None), reverse=True)
                for a, b in spans:
                    seg = seg[:a] + "<MASKED:" + r["id"] + ">" + seg[b:]
                    counts[r["id"]] = counts.get(r["id"], 0) + 1
                break  # at most one mask rule per segment
            kept.append(seg)
        out.append("\r".join(kept))
    return out


def tokenize(lines):
    """Skeleton (numbers replaced by '#') and the (line_no, token) list."""
    skel, toks = [], []
    for i, line in enumerate(lines):
        skel.append(TOKEN.sub("#", line))
        toks.extend((i + 1, t) for t in TOKEN.findall(line))
    return skel, toks


class Tok:
    """Exact interpretation of one printed numeric token."""

    __slots__ = ("text", "kind", "neg", "value")

    def __init__(self, text):
        self.text = text
        low = text.lower()
        self.neg = low.startswith("-")
        body = low.lstrip("+-")
        if body.startswith("nan"):
            self.kind, self.value = "nan", None
        elif body.startswith("inf"):
            self.kind, self.value = "inf", None
        elif re.fullmatch(r"\d+", body):
            self.kind, self.value = "int", decimal.Decimal(int(text))
        else:
            self.kind, self.value = "dec", decimal.Decimal(text)

    @property
    def finite(self):
        return self.kind in ("int", "dec")

    def f64(self):
        return float(self.value)


def classify(a, b):
    """Classify a pair of text-different tokens. Returns (category, abs, rel)

    abs/rel are Decimal (finite pairs) or the string "inf" (non-finite)."""
    if not (a.finite and b.finite):
        if a.kind == b.kind == "inf" and a.neg == b.neg:
            return "format_only", decimal.Decimal(0), decimal.Decimal(0)
        return "nonfinite", "inf", "inf"
    if a.value == b.value:
        if a.value.is_zero() and a.neg != b.neg:
            return "signed_zero", decimal.Decimal(0), decimal.Decimal(0)
        return "format_only", decimal.Decimal(0), decimal.Decimal(0)
    d = abs(a.value - b.value)
    scale = max(abs(a.value), abs(b.value))
    rel = d / scale if scale else decimal.Decimal(0)
    if a.kind == b.kind == "int":
        return "integer", d, rel
    if a.f64() == b.f64() and math.copysign(1, a.f64()) == math.copysign(1, b.f64()):
        return "f64_equal", d, rel
    return "value", d, rel


def _num(x):
    """JSON-safe number: Decimal -> float (non-finite as string)."""
    if isinstance(x, str):
        return x
    f = float(x)
    return f if math.isfinite(f) else str(f)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def compare_files(rel, pa, pb, rule_prefix=None):
    da, db = open(pa, "rb").read(), open(pb, "rb").read()
    rules = _rules_for(os.path.join(rule_prefix, rel) if rule_prefix else rel)
    ca, cb = {}, {}
    la = normalize(da.decode("utf-8", "surrogateescape"), rules, ca)
    lb = normalize(db.decode("utf-8", "surrogateescape"), rules, cb)
    sa, ta = tokenize(la)
    sb, tb = tokenize(lb)
    rec = {"file": rel, "bytes": [len(da), len(db)], "sha256": [sha256(da), sha256(db)],
           "lines": [len(la), len(lb)], "numeric_tokens": [len(ta), len(tb)],
           "empty": len(da) == 0 and len(db) == 0}
    if ca or cb:
        rec["normalized_fields"] = {"a": ca, "b": cb}
    if da == db:
        rec["status"] = "identical_bytes"
        return rec
    if la == lb:
        rec["status"] = "identical_after_normalization"
        return rec
    if sorted(la) == sorted(lb):
        # Same multiset of (normalized) lines in a different order: typical of
        # stdout interleaving of several MPI ranks. Not a numerical comparison.
        rec["status"] = "reordered_lines"
        rec["lines_out_of_place"] = sum(x != y for x, y in zip(la, lb))
        return rec
    if sa != sb or len(ta) != len(tb):
        rec["status"] = "structure_diff"
        first = next((i for i, (x, y) in enumerate(zip(sa, sb)) if x != y), min(len(sa), len(sb)))
        rec["first_structure_diff"] = {
            "line": first + 1,
            "a": la[first][:200] if first < len(la) else None,
            "b": lb[first][:200] if first < len(lb) else None}
        return rec
    cats = {k: 0 for k in ("format_only", "f64_equal", "value", "integer",
                           "signed_zero", "nonfinite")}
    max_abs = decimal.Decimal(0)
    max_rel = decimal.Decimal(0)
    nonfinite = False
    firsts = []
    for (ln, x), (_, y) in zip(ta, tb):
        if x == y:
            continue
        cat, d, r = classify(Tok(x), Tok(y))
        cats[cat] += 1
        if cat == "format_only":
            continue
        if len(firsts) < 5:
            firsts.append({"line": ln, "a": x, "b": y, "kind": cat})
        if d == "inf":
            nonfinite = True
        else:
            max_abs = max(max_abs, d)
            max_rel = max(max_rel, r)
    rec["token_diffs"] = {k: v for k, v in cats.items() if v}
    n_real = sum(v for k, v in cats.items() if k != "format_only")
    if n_real == 0:
        rec["status"] = "value_equal_format_diff"
        return rec
    rec["status"] = "numeric_diff"
    rec["n_diff"] = n_real
    rec["first_differences"] = firsts
    scale = max((abs(Tok(t).value) for _, t in ta if Tok(t).finite), default=decimal.Decimal(0))
    if nonfinite:
        rec.update(max_abs="inf", max_rel="inf", normwise_rel="inf")
        if max_abs:
            rec["max_abs_finite"] = _num(max_abs)
    else:
        rec["max_abs"] = _num(max_abs)
        rec["max_rel"] = _num(max_rel)
        rec["normwise_rel"] = _num(max_abs / scale if scale else max_abs)
    return rec


def collect(root, suffixes=SUFFIXES):
    """rel -> path of the compared files (suffixes=None: every file)."""
    out = {}
    for d, dirs, files in os.walk(root):
        dirs[:] = [x for x in dirs if x not in SKIP_DIRS]
        for fn in files:
            if fn == MANIFEST and d == root:
                continue
            if suffixes is None or fn.endswith(suffixes):
                p = os.path.join(d, fn)
                out[os.path.relpath(p, root)] = p
    return out


def provenance(root, window, suffixes=SUFFIXES):
    """(info, rel -> produced_in_run, set of rels recorded as input copies)
    from MANIFEST.json and/or mtimes."""
    man = os.path.join(root, MANIFEST)
    info = {"root": root}
    flags = {}
    inputs = set()
    if os.path.exists(man):
        m = json.load(open(man))
        info["manifest"] = {k: v for k, v in m.items() if k != "files"}
        for rel, f in m.get("files", {}).items():
            flags[rel] = bool(f.get("produced_in_run", True))
            if f.get("input_copy"):
                inputs.add(rel)
    if window:
        lo, hi = window
        info["window"] = [lo, hi]
        for rel, p in collect(root, suffixes).items():
            mt = os.stat(p).st_mtime
            flags[rel] = flags.get(rel, True) and lo <= mt <= hi
    return info, flags, inputs


def excluded_reason(rel, pa, source_root):
    fn = os.path.basename(rel)
    if fn.endswith(".lastout") and "gtest" in fn:
        return ("gtest-capture: overwritten by every gtest case ctest runs from the same "
                "binary; content depends on scheduling (compare isolated per-case runs instead)")
    if source_root:
        src = os.path.join(source_root, rel)
        if os.path.isfile(src) and open(src, "rb").read() == open(pa, "rb").read():
            return "input-copy: byte-identical to the same path in the reference source tree"
    return None


def compare_trees(a_root, b_root, win_a=None, win_b=None, suffixes=SUFFIXES,
                  source_root=None, rule_prefix=None):
    """Compare two output trees. rule_prefix: path (relative to the build tree)
    prepended to file names for rule scoping, e.g. the source work directory of
    an isolated run whose captures are named rank-<r>.stdout."""
    a, b = collect(a_root, suffixes), collect(b_root, suffixes)
    prov_a, fl_a, in_a = provenance(a_root, win_a, suffixes)
    prov_b, fl_b, in_b = provenance(b_root, win_b, suffixes)
    res = {"tool": "compare-outputs.py v2 (field-level normalization, review V10 OB-01)",
           "a": prov_a, "b": prov_b,
           "suffixes": list(suffixes) if suffixes else "all files",
           "source_root": source_root, "rule_prefix": rule_prefix,
           "rules": [{k: r[k] for k in ("id", "kind", "files", "regex", "why")} for r in RULES],
           "only_in_a": [], "only_in_b": [], "excluded": [], "files": []}
    # Files present in one tree only: input copies (by source tree or by the
    # other tree's manifest) are excluded, everything else is reported.
    for side, mine, other_inputs in (("a", a, in_b), ("b", b, in_a)):
        theirs = b if side == "a" else a
        for rel in sorted(set(mine) - set(theirs)):
            why = excluded_reason(rel, mine[rel], source_root)
            if why is None and rel in other_inputs:
                why = "input-copy: recorded as such in the other tree's MANIFEST.json"
            if why and why.startswith("input-copy"):
                res["excluded"].append({"file": rel, "reason": why, "present_in": side})
            else:
                res[f"only_in_{side}"].append(rel)
    for rel in sorted(set(a) & set(b)):
        why = excluded_reason(rel, a[rel], source_root)
        if why is None and (rel in in_a or rel in in_b):
            why = "input-copy: recorded as such in MANIFEST.json"
        if why:
            da, db = open(a[rel], "rb").read(), open(b[rel], "rb").read()
            res["excluded"].append({"file": rel, "reason": why, "bytes_identical": da == db})
            continue
        rec = compare_files(rel, a[rel], b[rel], rule_prefix)
        stale = [s for s, fl in (("a", fl_a), ("b", fl_b)) if not fl.get(rel, True)]
        if stale:
            rec["status_if_not_stale"] = rec["status"]
            rec["status"] = "stale"
            rec["stale_in"] = stale
        res["files"].append(rec)
    res["summary"] = summarize(res)
    return res


STATUSES = ("identical_bytes", "identical_after_normalization", "value_equal_format_diff",
            "numeric_diff", "reordered_lines", "structure_diff", "stale")


def summarize(res):
    files = res["files"]
    s = {"compared": len(files), "excluded": len(res["excluded"]),
         "only_in_a": len(res["only_in_a"]), "only_in_b": len(res["only_in_b"])}
    s["by_status"] = {k: sum(f["status"] == k for f in files) for k in STATUSES}
    with_data = [f for f in files if min(f["numeric_tokens"]) > 0]
    s["by_status_with_numeric_tokens"] = {k: sum(f["status"] == k for f in with_data)
                                          for k in STATUSES}
    s["files_without_numeric_tokens"] = len(files) - len(with_data)
    s["empty_files"] = sum(f["empty"] for f in files)
    totals = {}
    for f in files:
        for side in ("a", "b"):
            for k, v in f.get("normalized_fields", {}).get(side, {}).items():
                totals.setdefault(k, [0, 0])[side == "b"] += v
    s["normalized_fields_total"] = {k: {"a": v[0], "b": v[1]} for k, v in sorted(totals.items())}
    s["files_with_normalized_fields"] = sum("normalized_fields" in f for f in files)
    kinds = {}
    for f in files:
        for k, v in f.get("token_diffs", {}).items():
            kinds[k] = kinds.get(k, 0) + v
    s["token_diffs_total"] = kinds
    nd = [f for f in files if f["status"] == "numeric_diff"]
    s["numeric_diff_with_nonfinite_mismatch"] = sum(f["normwise_rel"] == "inf" for f in nd)
    rels = sorted(f["normwise_rel"] for f in nd if isinstance(f["normwise_rel"], float))
    if rels:
        s["max_finite_normwise_rel"] = rels[-1]
        s["median_finite_normwise_rel"] = rels[len(rels) // 2]
        s["normwise_rel_histogram"] = {
            k: sum(1 for r in rels if lo <= r < hi) for k, lo, hi in [
                ("<1e-15", 0, 1e-15), ("1e-15..1e-12", 1e-15, 1e-12),
                ("1e-12..1e-9", 1e-12, 1e-9), ("1e-9..1e-6", 1e-9, 1e-6),
                ("1e-6..1e-3", 1e-6, 1e-3), (">=1e-3", 1e-3, math.inf)]}
    return s


def all_identical(res):
    s = res["summary"]
    ok = s["by_status"]["identical_bytes"] + s["by_status"]["identical_after_normalization"]
    return ok == s["compared"] and not res["only_in_a"] and not res["only_in_b"]


# --------------------------------------------------------------------------
# Self-tests: adversarial cases and rule coverage.
# --------------------------------------------------------------------------
def self_test():
    cases = {
        # name: (content a, content b, expected status, extra checks)
        "energy.lastout": ("energy[0] = 1\n", "energy[0] = 999\n", "numeric_diff",
                           {"token_diffs": {"integer": 1}, "max_abs": 998.0}),
        "dt.lastout": ("time_step = 0.1\n", "time_step = 999\n", "numeric_diff",
                       {"token_diffs": {"value": 1}}),
        "nan.lastout": ("x = nan\n", "x = 1\n", "numeric_diff",
                        {"token_diffs": {"nonfinite": 1}, "max_abs": "inf",
                         "normwise_rel": "inf"}),
        "nansign.lastout": ("x = nan\n", "x = -nan\n", "numeric_diff",
                            {"token_diffs": {"nonfinite": 1}}),
        "inf.lastout": ("x = inf\n", "x = -inf\n", "numeric_diff",
                        {"token_diffs": {"nonfinite": 1}}),
        "nannan.lastout": ("x = nan 1\n", "x = nan 1\n", "identical_bytes", {}),
        "zero.lastout": ("v = +0\n", "v = -0\n", "numeric_diff",
                         {"token_diffs": {"signed_zero": 1}}),
        "zero2.csv": ("a,0.0\n", "a,-0.0\n", "numeric_diff",
                      {"token_diffs": {"signed_zero": 1}}),
        "big.lastout": ("id 9007199254740992\n", "id 9007199254740993\n", "numeric_diff",
                        {"token_diffs": {"integer": 1}, "max_abs": 1.0}),
        "f64eq.lastout": ("v 0.1\n", "v 0.10000000000000000001\n", "numeric_diff",
                          {"token_diffs": {"f64_equal": 1}}),
        "fmt.lastout": ("v 1.0 2e1\n", "v 1.00 20\n", "value_equal_format_diff",
                        {"token_diffs": {"format_only": 2}}),
        "timeword.lastout": ("Time = 5\nupdate 3\nelapsed 7 [4] <2>\n",
                             "Time = 6\nupdate 4\nelapsed 8 [5] <3>\n", "numeric_diff",
                             {"n_diff": 5}),
        "gmsh.lastout": ("Info    : Done meshing 1D (Wall 0.0002s, CPU 8.4e-05s)\n"
                         "Info    : 459 nodes 572 elements\n"
                         "Info    : Started on Mon Nov  9 11:45:42 2026\n",
                         "Info    : Done meshing 1D (Wall 0.0013s, CPU 2e-05s)\n"
                         "Info    : 460 nodes 572 elements\n"
                         "Info    : Started on Tue Nov 10 01:02:03 2026\n", "numeric_diff",
                         {"token_diffs": {"integer": 1},
                          "normalized_fields": {"a": {"gmsh-wall-cpu": 2, "gmsh-date": 1},
                                                "b": {"gmsh-wall-cpu": 2, "gmsh-date": 1}}}),
        "addr.lastout": ("   + address        : 0xaaaa1\n  + size : 3\n",
                         "   + address        : 0xbbbb2\n  + size : 3\n",
                         "identical_after_normalization",
                         {"normalized_fields": {"a": {"heap-address": 1},
                                                "b": {"heap-address": 1}}}),
        "addr2.lastout": ("+ address : 0x10 extra 5\n", "+ address : 0x10 extra 6\n",
                          "numeric_diff", {"token_diffs": {"integer": 1}}),
        "pytest.lastout": ("rootdir: /build/strict/test/x\n"
                           "====== 15 passed, 1 warning in 0.04s ======\n",
                           "rootdir: /build/default/test/x\n"
                           "======= 15 passed, 1 warning in 10.04s ======\n",
                           "identical_after_normalization", {}),
        "pytest2.lastout": ("== 15 passed in 0.04s ==\n", "== 14 passed in 0.04s ==\n",
                            "numeric_diff", {"token_diffs": {"integer": 1}}),
        "reorder.lastout": ("a 1\nb 2\n", "b 2\na 1\n", "reordered_lines",
                            {"lines_out_of_place": 2}),
        "reorder2.lastout": ("0 pos: 0.5\n1 pos: 0.25\n", "1 pos: 0.25\n0 pos: 0.5\n",
                             "reordered_lines", {}),
        "struct.lastout": ("a 1\n", "a 1 2\n", "structure_diff", {}),
        "gtest.stdout": ("[       OK ] F/0.D (10 ms)\n[----------] 1 test from F/0 (10 ms total)\n"
                         "[==========] 1 test from 1 test suite ran. (11 ms total)\n"
                         "[  FAILED  ] F/1.D (3 ms)\n",
                         "[       OK ] F/0.D (4 ms)\n[----------] 1 test from F/0 (4 ms total)\n"
                         "[==========] 1 test from 1 test suite ran. (4 ms total)\n"
                         "[  FAILED  ] F/1.D (12 ms)\n",
                         "identical_after_normalization",
                         {"normalized_fields": {"a": {"gtest-console-ms": 4},
                                                "b": {"gtest-console-ms": 4}}}),
        "gtest2.stdout": ("[       OK ] F/0.D (10 ms)\n[  PASSED  ] 1 test.\n",
                          "[       OK ] F/0.D (10 ms)\n[  PASSED  ] 2 test.\n",
                          "numeric_diff", {}),
        "mpi.stderr": ("<91>[R0|S2] {1794239697242047} ### Error in the solution: -1 != 7 [1.14286].\n"
                       "[563f97ddead9:00091] Signal: Aborted (6)\n"
                       "[563f97ddead9:00091] [ 7] /x/libakantu.so.5.0(_ZN+0x140) [0xffff959e4440]\n"
                       "   Process name: [prterun-563f97ddead9-68@1,1]\n",
                       "<101>[R0|S2] {1794239697580981} ### Error in the solution: -1 != 7 [1.14286].\n"
                       "[aaaabbbbcccc:00101] Signal: Aborted (6)\n"
                       "[aaaabbbbcccc:00101] [ 7] /x/libakantu.so.5.0(_ZN+0x140) [0xffff00000040]\n"
                       "   Process name: [prterun-aaaabbbbcccc-78@1,1]\n",
                       "identical_after_normalization", {}),
        "mpi2.stderr": ("<91>[R0|S2] {1} ### Error in the solution: -1 != 7 [1.14286].\n"
                        "[563f97ddead9:00091] [ 7] /x/libakantu.so.5.0(_ZN+0x140) [0xffff959e4440]\n",
                        "<91>[R0|S2] {1} ### Error in the solution: -1 != 8 [1.14286].\n"
                        "[563f97ddead9:00091] [ 7] /x/libakantu.so.5.0(_ZN+0x144) [0xffff959e4440]\n",
                        "numeric_diff", {"n_diff": 2}),
        "struct2.lastout": ("a 1\nb 2\n", "a 1\nc 2\n", "structure_diff", {}),
        "empty.lastout": ("", "", "identical_bytes", {"empty": True}),
        "nonum.lastout": ("passed\n", "passed\n", "identical_bytes",
                          {"numeric_tokens": [0, 0]}),
        "info.lastout": ("information 1\n", "information 2\n", "numeric_diff",
                         {"token_diffs": {"integer": 1}}),
        "test_gtest.lastout": ("[ OK ] A (3 ms)\n", "[ OK ] B (4 ms)\n", None, {}),
        "input.csv": ("1,2\n", "1,2\n", None, {}),
        "rank-0.xml": ('  <testcase name="D" status="run" result="completed" time="0.002" '
                       'timestamp="2026-11-10T01:02:03.456" classname="F/0" />\n'
                       '    <failure message="error 5.30801e-08 &gt; 1e-10" />\n',
                       '  <testcase name="D" status="run" result="completed" time="0.013" '
                       'timestamp="2026-11-10T01:05:00.001" classname="F/0" />\n'
                       '    <failure message="error 5.30801e-08 &gt; 1e-10" />\n',
                       "identical_after_normalization",
                       {"normalized_fields": {"a": {"gtest-xml-time": 2},
                                              "b": {"gtest-xml-time": 2}}}),
        "rank-1.xml": ('    <failure message="error 5.30801e-08 &gt; 1e-10" />\n',
                       '    <failure message="error 5.30802e-08 &gt; 1e-10" />\n',
                       "numeric_diff", {}),
        "only_a.csv": ("1\n", None, None, {}),
        "input_only_a.csv": ("7,8\n", None, None, {}),
    }
    pb_a = ("Number of nodes: 87\n\r[-]    10/1000 (    9.95ms/step - elapsed:     0.10s - "
            "ETA:     9.85s)\x14\x14\r[\\]    20/1000 (   10.51ms/step - elapsed:     0.21s - "
            "ETA:    10.30s)\x14\rdone 3\n")
    pb_b = ("Number of nodes: 87\n\r[-]    10/1000 (    8.95ms/step - elapsed:     0.09s - "
            "ETA:     8.85s)\x14\rdone 3\n")
    cases["examples/c++/phase_field_model/pb.lastout"] = (
        pb_a, pb_b, "identical_after_normalization",
        {"normalized_fields": {"a": {"progress-bar": 2}, "b": {"progress-bar": 1}}})
    cases["examples/c++/phase_field_model/pb2.lastout"] = (
        pb_a.replace("done 3", "done 4"), pb_b, "numeric_diff", {"token_diffs": {"integer": 1}})
    # The progress-bar rule is file-scoped: elsewhere such a line is compared.
    cases["other/pb.lastout"] = (pb_a, pb_b, "structure_diff", {})
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        ra, rb = os.path.join(tmp, "a"), os.path.join(tmp, "b")
        for rel, (ca, cb, _, _) in cases.items():
            for root, c in ((ra, ca), (rb, cb)):
                if c is None:
                    continue
                p = os.path.join(root, rel)
                os.makedirs(os.path.dirname(p), exist_ok=True)
                with open(p, "w", newline="") as f:
                    f.write(c)
        src = os.path.join(tmp, "src")
        os.makedirs(src)
        with open(os.path.join(src, "input.csv"), "w") as f:
            f.write("1,2\n")
        with open(os.path.join(src, "input_only_a.csv"), "w") as f:
            f.write("7,8\n")
        res = compare_trees(ra, rb, suffixes=SUFFIXES + (".xml", ".stdout", ".stderr"), source_root=src)
        json.dumps(res, allow_nan=False)  # report must be strict JSON
        recs = {f["file"]: f for f in res["files"]}
        for rel, (_, _, status, extra) in cases.items():
            if status is None:
                continue
            r = recs.get(rel)
            if r is None:
                fails.append(f"{rel}: not compared")
                continue
            if r["status"] != status:
                fails.append(f"{rel}: status {r['status']} != {status}")
            for k, v in extra.items():
                if r.get(k) != v:
                    fails.append(f"{rel}: {k} = {r.get(k)!r}, expected {v!r}")
        if res["only_in_a"] != ["only_a.csv"]:
            fails.append(f"only_in_a = {res['only_in_a']}")
        if sorted(e["file"] for e in res["excluded"]) != ["input.csv", "input_only_a.csv",
                                                          "test_gtest.lastout"]:
            fails.append(f"excluded = {res['excluded']}")
        if all_identical(res):
            fails.append("all_identical() true on a differing tree")
        # Provenance: a manifest entry marks a file as stale.
        with open(os.path.join(rb, MANIFEST), "w") as f:
            json.dump({"files": {"energy.lastout": {"produced_in_run": False}}}, f)
        r2 = {f["file"]: f for f in compare_trees(ra, rb, suffixes=None)["files"]}
        if "MANIFEST.json" in r2:
            fails.append("MANIFEST.json compared as an output")
        if r2["energy.lastout"]["status"] != "stale" or \
                r2["energy.lastout"]["status_if_not_stale"] != "numeric_diff":
            fails.append(f"manifest staleness: {r2['energy.lastout']}")
        # Rule scoping through rule_prefix (isolated-run captures).
        rp = os.path.join(tmp, "p")
        for side, c in (("a", pb_a), ("b", pb_b)):
            os.makedirs(os.path.join(rp, side))
            with open(os.path.join(rp, side, "rank-0.stdout"), "w", newline="") as f:
                f.write(c)
        r4 = compare_trees(os.path.join(rp, "a"), os.path.join(rp, "b"), suffixes=None,
                           rule_prefix="examples/c++/phase_field_model")
        r5 = compare_trees(os.path.join(rp, "a"), os.path.join(rp, "b"), suffixes=None)
        if not all_identical(r4) or all_identical(r5):
            fails.append("rule_prefix scoping")
        # Identical trees: all_identical() true.
        res3 = compare_trees(ra, ra)
        if not all_identical(res3):
            fails.append("all_identical() false on identical trees")
    # The rules never match an ordinary scientific line.
    for line in ("Time Step = 0.00104083s", "Time = 0", "time,ekin,epot",
                 "0 0 pos: 0.5 [5] 0x03", "Stable time increment    : 0.000103417 sec",
                 "  r-- previous_dt [Time step of previous solveStep]   : 0"):
        c = {}
        for r in RULES:
            if normalize(line, [r], c) != [line]:
                fails.append(f"rule {r['id']} modifies scientific line {line!r}")
    for f in fails:
        print("FAIL:", f)
    print(f"compare-outputs self-test: {len(cases)} file cases + tree/provenance/rule "
          f"checks, {len(fails)} failures")
    return 1 if fails else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("a", nargs="?")
    ap.add_argument("b", nargs="?")
    ap.add_argument("--window-a", nargs=2, type=float, metavar=("START", "END"))
    ap.add_argument("--window-b", nargs=2, type=float, metavar=("START", "END"))
    ap.add_argument("--suffixes", default=",".join(SUFFIXES),
                    help="comma-separated file suffixes to compare, or 'all'")
    ap.add_argument("--source-root", help="reference source tree: files byte-identical to "
                    "their source path are excluded as input copies")
    ap.add_argument("--exit-status", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        sys.exit(self_test())
    if not (args.a and args.b):
        ap.error("TREE_A and TREE_B are required")
    decimal.getcontext().prec = 120
    suffixes = None if args.suffixes == "all" else tuple(args.suffixes.split(","))
    res = compare_trees(args.a, args.b, args.window_a, args.window_b, suffixes,
                        args.source_root)
    json.dump(res, sys.stdout, indent=1, allow_nan=False)
    print()
    print(json.dumps(res["summary"]), file=sys.stderr)
    if args.exit_status and not all_identical(res):
        sys.exit(1)


if __name__ == "__main__":
    decimal.getcontext().prec = 120
    main()
