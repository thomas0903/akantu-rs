#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): the tolerance
# ledger. No C++ counterpart.
"""Write the ledger of every tolerance rule committed under `fixtures/`.

Reads every `fixtures/**/tolerances.json` (the files the validation harness
resolves rules from: `crates/akantu-validation/src/tolerance.rs`) and writes
`docs/TOLERANCE_LEDGER.md`: one row per rule with its file, its index in the
file (first match wins, so the order matters), its case and quantity
patterns, the criterion (exact, or absolute / relative with the scale of the
relative part, or ULPs), its status and its written justification. A rule
without `reviewed_by` is a proposal: its status is PROPOSED, exactly as
`ToleranceSpec::is_reviewed` decides in the harness, and its `evidence` is
listed in a section of its own.

The script edits nothing but the ledger: rule texts and values are read as
written (numbers keep their literal spelling from the JSON file).

Usage:
  scripts/gold/tolerance_ledger.py            # (re)write the ledger
  scripts/gold/tolerance_ledger.py --check    # exit 1 if the ledger is stale

`--check` writes nothing (the gate's source mount is read-only); it is run
by `crates/akantu-validation/tests/tolerance_ledger.rs`, which also checks
the counts of the ledger's marker line against the Rust reader.
"""

from __future__ import annotations

import argparse
import difflib
import json
import string
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "fixtures"
LEDGER = ROOT / "docs" / "TOLERANCE_LEDGER.md"
TOLERANCE_FILE = "tolerances.json"

# Fields of a rule (`ToleranceRule` + flattened `ToleranceSpec`, both
# `deny_unknown_fields` in the harness).
RULE_FIELDS = {
    "quantity",
    "case",
    "abs",
    "rel",
    "ulps",
    "scale",
    "nan",
    "justification",
    "evidence",
    "reviewed_by",
}
# `is_placeholder_review` of the harness.
PLACEHOLDERS = {"", "pending", "todo", "tbd", "proposed"}
ASCII_LOWER = str.maketrans(string.ascii_uppercase, string.ascii_lowercase)


class LedgerError(Exception):
    """A tolerance file the harness would refuse, or an unexpected shape."""


def _no_duplicates(pairs):
    seen = {}
    for k, v in pairs:
        if k in seen:
            raise LedgerError(f"duplicate key `{k}`")
        seen[k] = v
    return seen


def load(path: Path) -> dict:
    """Parse a tolerance file, keeping number literals as written."""
    text = path.read_text(encoding="utf-8")
    try:
        data = json.loads(
            text,
            object_pairs_hook=_no_duplicates,
            parse_float=str,
            parse_int=str,
        )
    except (json.JSONDecodeError, LedgerError) as e:
        raise LedgerError(f"{path}: {e}") from e
    if not isinstance(data, dict) or set(data) != {"format_version", "rules"}:
        raise LedgerError(f"{path}: expected exactly `format_version` and `rules`")
    if data["format_version"] != "1":
        raise LedgerError(f"{path}: unsupported format_version {data['format_version']}")
    for i, rule in enumerate(data["rules"]):
        unknown = set(rule) - RULE_FIELDS
        if unknown:
            raise LedgerError(f"{path} rule [{i}]: unknown field(s) {sorted(unknown)}")
        for key in ("quantity", "justification"):
            if not isinstance(rule.get(key), str):
                raise LedgerError(f"{path} rule [{i}]: `{key}` missing")
    return data


def tolerance_files() -> list[Path]:
    return sorted(FIXTURES.rglob(TOLERANCE_FILE), key=lambda p: p.relative_to(ROOT).as_posix())


def is_zero(literal) -> bool:
    return literal is None or float(literal) == 0.0


def is_reviewed(rule: dict) -> bool:
    rb = rule.get("reviewed_by")
    # ASCII-only lower-casing, as the harness's `to_ascii_lowercase`.
    return isinstance(rb, str) and rb.strip().translate(ASCII_LOWER) not in PLACEHOLDERS


def scale_text(scale) -> str:
    if scale is None or scale == "reference":
        return "|e_i|"
    if scale == "max_abs":
        return "max|e|"
    if scale == "max_abs_per_component":
        return "max|e| per component"
    if scale == "max_abs_per_step":
        return "max|e| per step"
    if isinstance(scale, dict) and set(scale) == {"max_abs_per_group"}:
        return "max|e| per (" + ", ".join(scale["max_abs_per_group"]) + ")"
    if isinstance(scale, dict) and set(scale) == {"value"}:
        return f"fixed {scale['value']}"
    raise LedgerError(f"unknown scale {scale!r}")


def criterion(rule: dict) -> tuple[str, str]:
    """(class, text): class is `exact`, `abs/rel` or `ulps`."""
    a, r, u = rule.get("abs"), rule.get("rel"), rule.get("ulps")
    parts = []
    if not is_zero(a):
        parts.append(f"abs {a}")
    if not is_zero(r):
        parts.append(f"rel {r} × {scale_text(rule.get('scale'))}")
    tol = " + ".join(parts)
    if u is not None and int(u) != 0:
        tol = f"{tol} or ≤ {u} ulp" if tol else f"≤ {u} ulp"
        cls = "ulps" if not parts else "abs/rel"
    elif parts:
        cls = "abs/rel"
    else:
        cls, tol = "exact", "exact (bit identity)"
    if rule.get("nan") == "forbid":
        tol += "; NaN forbidden"
    return cls, tol


def cell(text: str) -> str:
    """Make a string safe inside one Markdown table cell."""
    return " ".join(text.split()).replace("|", "\\|")


def code(text: str) -> str:
    text = cell(text)
    return f"`{text}`" if "`" not in text else f"``{text}``"


def collect() -> list[dict]:
    rows = []
    for path in tolerance_files():
        data = load(path)
        rel = path.relative_to(FIXTURES).as_posix()
        for i, rule in enumerate(data["rules"]):
            cls, crit = criterion(rule)
            rows.append(
                {
                    "file": rel,
                    "index": i,
                    "case": rule.get("case"),
                    "quantity": rule["quantity"],
                    "class": cls,
                    "criterion": crit,
                    "reviewed": is_reviewed(rule),
                    "justification": rule["justification"],
                    "evidence": rule.get("evidence") or "",
                }
            )
    return rows


def render(rows: list[dict]) -> str:
    files = []
    for r in rows:
        if r["file"] not in files:
            files.append(r["file"])
    n_acc = sum(r["reviewed"] for r in rows)
    n_prop = len(rows) - n_acc
    by_class = {c: sum(r["class"] == c for r in rows) for c in ("exact", "abs/rel", "ulps")}
    out = []
    w = out.append
    w("# Tolerance ledger")
    w("")
    w(
        "<!-- GENERATED by scripts/gold/tolerance_ledger.py from fixtures/**/tolerances.json; "
        "do not edit by hand: change the tolerance file (with its review), then re-run the "
        "script. The guard crates/akantu-validation/tests/tolerance_ledger.rs checks it. -->"
    )
    w(
        f"<!-- ledger: files={len(files)} rules={len(rows)} accepted={n_acc} "
        f"proposed={n_prop} -->"
    )
    w("")
    w(
        "Every tolerance rule the validation harness can resolve, one row per rule, with its "
        "written justification. The rules live in `tolerances.json` files next to the "
        "fixtures; the measured evidence is in those files (`evidence`) and in "
        "`docs/NUMERICAL_VALIDATION.md`, which explains the tolerance method. How a rule is "
        "applied: `crates/akantu-validation/src/tolerance.rs` (the most specific file first, "
        "then its ancestor directories; inside a file the **first** matching rule wins)."
    )
    w("")
    w(
        f"**{len(rows)} rules in {len(files)} files: {n_acc} accepted by a review, "
        f"{n_prop} PROPOSED.** By criterion: {by_class['exact']} exact (bit identity), "
        f"{by_class['abs/rel']} absolute / relative, {by_class['ulps']} ULP-only."
    )
    w("")
    w("Columns:")
    w("")
    w("- **[i]**: the rule's index in its file (0-based; the order in which rules are tried).")
    w(
        "- **Case**: the case-id glob the rule is restricted to (`—`: every case that resolves "
        "to the file). **Quantity**: the quantity glob (`*` matches any sequence, `/` "
        "included)."
    )
    w(
        "- **Criterion**: *exact* is bit identity (`+0` ≠ `−0`, D-051; NaNs per D-046). "
        "Otherwise an entry with reference `e` and result `a` passes when `|a − e| ≤ abs + "
        "rel × scale`, where the scale is `|e_i|` (entry-wise), `max|e|` over the whole "
        "quantity, `max|e|` per component / per step / per group of axes, or a fixed value. "
        "Numbers are spelled as in the file."
    )
    w(
        "- **Status**: *accepted* when the rule's `reviewed_by` names a review (the harness's "
        "`ToleranceSpec::is_reviewed`); *PROPOSED* otherwise: such a rule must not be the "
        "evidence of a *Verified* parity claim."
    )
    w(
        "- **Justification**: the rule's `justification`, verbatim (white space collapsed, "
        "`|` escaped)."
    )
    w("")
    w("## Per file")
    w("")
    w("| File | Rules | Accepted | PROPOSED | Exact | Abs / rel | ULP |")
    w("|---|---:|---:|---:|---:|---:|---:|")
    for f in files:
        fr = [r for r in rows if r["file"] == f]
        w(
            f"| `fixtures/{f}` | {len(fr)} | {sum(r['reviewed'] for r in fr)} | "
            f"{sum(not r['reviewed'] for r in fr)} | {sum(r['class'] == 'exact' for r in fr)} | "
            f"{sum(r['class'] == 'abs/rel' for r in fr)} | "
            f"{sum(r['class'] == 'ulps' for r in fr)} |"
        )
    w(
        f"| **Total** | **{len(rows)}** | **{n_acc}** | **{n_prop}** | "
        f"**{by_class['exact']}** | **{by_class['abs/rel']}** | **{by_class['ulps']}** |"
    )
    w("")
    w("## PROPOSED rules")
    w("")
    if n_prop == 0:
        w("None: every rule names the review that accepted it.")
    else:
        w(
            "Rules without `reviewed_by`, with their `evidence` (white space collapsed). "
            "Deciding them is a review's job; the ledger only lists them."
        )
        w("")
        w("| # | File | [i] | Case | Quantity | Criterion | Evidence |")
        w("|---:|---|---:|---|---|---|---|")
        for n, r in enumerate(rows, 1):
            if r["reviewed"]:
                continue
            w(
                f"| {n} | `fixtures/{r['file']}` | {r['index']} | "
                f"{code(r['case']) if r['case'] else '—'} | {code(r['quantity'])} | "
                f"{cell(r['criterion'])} | {cell(r['evidence']) or '— (no evidence field)'} |"
            )
    w("")
    w("## All rules")
    w("")
    w("| # | File | [i] | Case | Quantity | Criterion | Status | Justification |")
    w("|---:|---|---:|---|---|---|---|---|")
    for n, r in enumerate(rows, 1):
        status = "accepted" if r["reviewed"] else "**PROPOSED**"
        w(
            f"| {n} | `fixtures/{r['file']}` | {r['index']} | "
            f"{code(r['case']) if r['case'] else '—'} | {code(r['quantity'])} | "
            f"{cell(r['criterion'])} | {status} | {cell(r['justification'])} |"
        )
    w("")
    return "\n".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    ap.add_argument(
        "--check",
        action="store_true",
        help="compare with the committed ledger, write nothing, exit 1 on a difference",
    )
    args = ap.parse_args()
    try:
        rows = collect()
    except LedgerError as e:
        print(f"tolerance_ledger: {e}", file=sys.stderr)
        return 2
    text = render(rows)
    if args.check:
        current = LEDGER.read_text(encoding="utf-8") if LEDGER.is_file() else ""
        if current == text:
            print(f"tolerance ledger: current ({len(rows)} rules)")
            return 0
        diff = difflib.unified_diff(
            current.splitlines(), text.splitlines(), "committed", "regenerated", lineterm="", n=0
        )
        print("tolerance ledger: STALE; re-run `python3 scripts/gold/tolerance_ledger.py`")
        for i, line in enumerate(diff):
            if i >= 40:
                print("…")
                break
            print(line[:300])
        return 1
    LEDGER.write_text(text, encoding="utf-8")
    print(f"wrote {LEDGER.relative_to(ROOT)} ({len(rows)} rules)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
