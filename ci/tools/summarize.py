#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; replaces upstream's
# test/ci/ctest2junit.xsl report conversion, parity row CI-020): build the
# machine-readable summary of a ci/run-all.sh output directory.
#
#   summarize.py <out-dir>
#
# Reads <out-dir>/provenance.txt, summary.tsv and every <tier>/steps.tsv,
# commands.tsv, status; writes <out-dir>/summary.json and prints the verdict
# as the last line: "CI: PASS", "CI: INCOMPLETE (...)" or "CI: FAIL (...)".
# Exit code 0 / 3 / 1 accordingly (standard library only, Python >= 3.9).
#
# "Nothing ran" is never a pass: PASS needs at least
# one tier with status `pass` and every tier `pass` or `na`; a status other
# than pass / fail / incomplete / na is a failure; a tier recorded `pass`
# whose steps contradict it (a failed or skipped step, or no passing step)
# gets the worse verdict derived from its steps; no tier at all is a failure.
"""Summarise a ci/run-all.sh run into summary.json."""

from __future__ import annotations

import json
import sys
from pathlib import Path

SCHEMA = "akantu-rs-ci-summary/1"
TIER_STATUSES = ("pass", "fail", "incomplete", "na")
STEP_STATUSES = ("pass", "fail", "skip", "na")


def derived_status(steps: list[dict]) -> str:
    """The status ci/lib.sh's ci_tier_end would record for these steps."""
    sts = [x["status"] for x in steps]
    if any(x not in STEP_STATUSES or x == "fail" for x in sts):
        return "fail"
    if "skip" in sts:
        return "incomplete"
    if "pass" in sts:
        return "pass"
    return "na"


def read_kv(path: Path) -> dict:
    out: dict = {}
    if not path.exists():
        return out
    for line in path.read_text(errors="replace").splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            out[key] = value
    return out


def read_tsv(path: Path, ncols: int) -> list[list[str]]:
    rows = []
    if not path.exists():
        return rows
    for line in path.read_text(errors="replace").splitlines():
        if not line:
            continue
        cols = line.split("\t")
        cols += [""] * (ncols - len(cols))
        rows.append(cols[:ncols])
    return rows


def tier_record(out: Path, name: str, seconds: str, reason: str) -> dict:
    tdir = out / name
    status_file = tdir / "status"
    status = status_file.read_text().strip() if status_file.exists() else "fail"
    problems: list[str] = []
    if not status_file.exists():
        problems.append("no status file")
    elif status not in TIER_STATUSES:
        problems.append(f"unknown tier status {status!r}")
        status = "fail"
    commands = {r[0]: {"backend": r[1], "command": r[2]} for r in read_tsv(tdir / "commands.tsv", 3)}
    steps = []
    for step, st, rc, secs, why in read_tsv(tdir / "steps.tsv", 5):
        rec = {
            "name": step,
            "status": st,
            "exit_code": None if rc in ("", "-") else int(rc),
            "seconds": int(secs) if secs.isdigit() else None,
            "reason": why or None,
        }
        rec.update(commands.get(step, {}))
        log = tdir / ("".join(c if c.isalnum() or c in "._-" else "_" for c in step) + ".log")
        rec["log"] = str(log.relative_to(out)) if log.exists() else None
        steps.append(rec)
    extra = {}
    report = tdir / "report.json"
    if report.exists():
        try:
            extra["report"] = str(report.relative_to(out))
        except ValueError:
            pass
    counts = {s: sum(1 for x in steps if x["status"] == s) for s in STEP_STATUSES}
    unknown = sorted({x["status"] for x in steps if x["status"] not in STEP_STATUSES})
    if unknown:
        problems.append(f"unknown step status {', '.join(map(repr, unknown))}")
    derived = derived_status(steps)
    rank = {"pass": 0, "na": 0, "incomplete": 1, "fail": 2}
    if status in ("pass", "na") and rank[derived] > rank[status]:
        problems.append(f"recorded {status} but its steps give {derived}")
        status = derived
    elif status == "pass" and counts["pass"] == 0:
        problems.append("recorded pass but no step passed (nothing ran)")
        status = "incomplete"
    if problems and not reason:
        reason = "; ".join(problems)
    return {
        "name": name,
        "status": status,
        "seconds": int(seconds) if seconds.isdigit() else None,
        "reason": reason or None,
        "step_counts": counts,
        "steps": steps,
        **extra,
    }


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    out = Path(argv[1]).resolve()
    prov = read_kv(out / "provenance.txt")
    tiers = [tier_record(out, t, s, r) for t, _st, s, r in read_tsv(out / "summary.tsv", 4)]
    failed = [t["name"] for t in tiers if t["status"] == "fail"]
    incomplete = [t["name"] for t in tiers if t["status"] == "incomplete"]
    if not tiers:
        failed = ["(no tier)"]  # an empty tier list ran nothing
    elif not failed and not incomplete and not any(t["status"] == "pass" for t in tiers):
        incomplete = ["(every selected tier is not applicable: nothing ran)"]
    if failed:
        verdict, code = "FAIL", 1
    elif incomplete:
        verdict, code = "INCOMPLETE", 3
    else:
        verdict, code = "PASS", 0
    summary = {
        "schema": SCHEMA,
        "verdict": verdict,
        "failed_tiers": failed,
        "incomplete_tiers": incomplete,
        "provenance": prov,
        "tiers": tiers,
    }
    (out / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=False) + "\n")
    for t in tiers:
        c = t["step_counts"]
        print(
            f"{t['name']:<10} {t['status']:<10} {t['seconds'] or 0:>6} s  "
            f"pass={c['pass']} fail={c['fail']} skip={c['skip']} na={c['na']}"
        )
        if t["reason"] and t["status"] != "pass":
            print(f"    note: {t['reason']}")
        for s in t["steps"]:
            if s["status"] in ("fail", "skip"):
                print(f"    {s['status']}: {s['name']}: {s['reason'] or ''}")
    detail = ""
    if failed:
        detail = f" ({', '.join(failed)}"
        detail += f"; incomplete: {', '.join(incomplete)})" if incomplete else ")"
    elif incomplete:
        detail = f" ({', '.join(incomplete)})"
    print(f"summary: {out / 'summary.json'}")
    print(f"CI: {verdict}{detail}")
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv))
