#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; D-080: the delivered CI is the
# GitHub workflows, so every gate needs a workflow job or a stated reason why
# not).
"""Check the gate map of ci/github/README.md against the tree.

    gate_map.py [--repo DIR]

The README holds a table between ``<!-- GATE-MAP-BEGIN -->`` and
``<!-- GATE-MAP-END -->``: one row per gate, five cells

    | gate id | command (in backticks) | workflow job | tier(s) | when |

where the workflow job is ``file.yml:job`` (a job of ci/github/file.yml; a
comma separated list of such jobs for a gate that spans several) or ``none: <reason>`` (a stated reason why the gate has
no job), the tiers are the ``ci/run-all.sh`` tiers the job(s) run (comma
separated) or ``-``, and the last cell says WHEN the job runs: a comma
separated subset of ``pr`` (a ``pull_request`` trigger), ``merge`` (a push to
``main``) and ``schedule`` (a ``schedule`` trigger), or ``-`` for a row with no
job (a gate map that does not say when a job runs presents a nightly job as a
merge gate).

Required rows: ``feature-gates.txt line N`` for every command line N of
ci/tools/feature-gates.txt, the command text equal to the line. Other rows (for
example ``tier T`` rows for the merge-gate tiers) are optional and checked
like the required ones.
Every ``file.yml:job`` must exist, and every named tier must be run by one of
the row's jobs (its ``--tiers``, its ``--preset`` or the matrix of
``${{ matrix.tier }}``) and must be a tier of ci/tiers/. A ``none:`` reason
needs at least 20 characters. The ``when`` cell is checked against the
workflow's ``on:`` block: every job of the row must have every claimed
trigger (``workflow_dispatch`` is always there and not listed); the rows of the
PRE-MERGE gates (a ``tier T`` row for T among ``fmt``,
``clippy``, ``test``, ``fixtures`` and ``fast``: the checks of every pull
request) must claim ``pr``; every ``feature-gates.txt`` line must claim ``pr``
or ``schedule`` (the feature gates run weekly and on manual dispatch, never
only by hand); a step that runs a tier must carry no ``if:`` and a job no
``if: false`` (a gated-off leg is not a gate).

Exit 0 when the map is complete and consistent, 1 otherwise (one line per
failure), 2 on a usage error.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

BEGIN = "<!-- GATE-MAP-BEGIN -->"
END = "<!-- GATE-MAP-END -->"
# The tiers that every pull request runs: their rows must say that the job
# runs on pull requests. The feature-gates lines run weekly (or on pull
# requests): their rows must claim one of the two.
PRE_MERGE_TIERS = ("fmt", "clippy", "test", "fixtures", "fast")
WHEN_VALUES = ("pr", "merge", "schedule")


def parse_rows(readme: str) -> tuple[list[tuple[str, str, str, str, str]], list[str]]:
    """Rows of the gate-map table and parse problems."""
    problems: list[str] = []
    if BEGIN not in readme or END not in readme:
        return [], [f"README has no {BEGIN} ... {END} block"]
    block = readme.split(BEGIN, 1)[1].split(END, 1)[0]
    rows = []
    for line in block.splitlines():
        line = line.strip()
        if not line.startswith("|") or re.match(r"^\|[\s:|-]+\|$", line):
            continue
        cells = [c.strip() for c in line.strip("|").split("|")]
        if cells[0].lower() in ("gate", "gate id"):
            continue
        if len(cells) != 5:
            problems.append(f"gate-map row with {len(cells)} cells instead of 5: {line[:80]}")
            continue
        rows.append((cells[0], cells[1], cells[2], cells[3], cells[4]))
    return rows, problems


def gate_lines(path: Path) -> list[str]:
    out = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        s = raw.strip()
        if s and not s.startswith("#"):
            out.append(s)
    return out


def presets(run_all: Path) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    for m in re.finditer(r'^\s+([a-z]+)\) echo "([^"]*)" ;;', run_all.read_text(encoding="utf-8"), re.M):
        out[m.group(1)] = m.group(2).split()
    return out


def job_blocks(workflow: str) -> dict[str, str]:
    """Job id -> text of its block (2-space indented keys under `jobs:`)."""
    lines = workflow.splitlines()
    try:
        start = next(i for i, ln in enumerate(lines) if ln.rstrip() == "jobs:")
    except StopIteration:
        return {}
    blocks: dict[str, list[str]] = {}
    cur = None
    for ln in lines[start + 1 :]:
        m = re.match(r"^  ([A-Za-z0-9_-]+):\s*$", ln)
        if m:
            cur = m.group(1)
            blocks[cur] = []
        elif cur is not None:
            blocks[cur].append(ln)
    return {k: "\n".join(v) for k, v in blocks.items()}


def workflow_triggers(workflow: str) -> set[str]:
    """Triggers of a workflow: pr, merge (push to main), schedule.

    Read from the top-level ``on:`` block by text (the CI tools use the
    standard library only): ``pull_request`` -> pr; ``push`` with a branches
    list naming main -> merge; ``schedule`` -> schedule. A push of tags only
    is not a merge trigger."""
    lines = workflow.splitlines()
    try:
        start = next(i for i, ln in enumerate(lines) if re.match(r"^(on|\"on\"|'on'):\s*$", ln.rstrip()))
    except StopIteration:
        return set()
    body: list[str] = []
    for ln in lines[start + 1 :]:
        if ln and not ln.startswith((" ", "#")):
            break
        body.append(ln)
    keys: dict[str, list[str]] = {}
    key = None
    for ln in body:
        m = re.match(r"^  ([A-Za-z_]+):", ln)
        if m:
            key = m.group(1)
            keys[key] = [ln]
        elif key is not None:
            keys[key].append(ln)
    triggers: set[str] = set()
    if "pull_request" in keys:
        triggers.add("pr")
    if "schedule" in keys:
        triggers.add("schedule")
    if any(re.search(r"branches:.*\bmain\b", ln) for ln in keys.get("push", [])):
        triggers.add("merge")
    return triggers


def step_blocks(block: str) -> list[str]:
    """Steps of a job block: text from one ``      - `` item to the next."""
    steps: list[list[str]] = []
    for ln in block.splitlines():
        if re.match(r"^      - ", ln):
            steps.append([ln])
        elif steps:
            steps[-1].append(ln)
    return ["\n".join(x) for x in steps]


def gated_off(block: str) -> list[str]:
    """Reasons why a job's tier run could be silently skipped: an ``if:`` on a
    step that runs ``ci/run-all.sh`` with tiers, or a job-level ``if: false``."""
    out: list[str] = []
    for ln in block.splitlines():
        if re.match(r"^    if:\s*(false|0|'false'|\"false\")\s*$", ln):
            out.append("job-level 'if: false'")
    for st in step_blocks(block):
        if "run-all.sh" in st and re.search(r"--(tiers|preset)\b", st) and re.search(r"^\s*(- )?if:", st, re.M):
            name = next((x.strip() for x in st.splitlines() if "name:" in x), st.splitlines()[0].strip())
            out.append(f"the tier step has an 'if:' ({name[:60]})")
    return out


def job_tiers(block: str, preset_map: dict[str, list[str]]) -> set[str]:
    tiers: set[str] = set()
    matrix: list[str] = []
    for m in re.finditer(r"^\s+tier:\s*\[([^\]]*)\]", block, re.M):
        matrix += [t.strip() for t in m.group(1).split(",") if t.strip()]
    for m in re.finditer(r"--tiers\s+(\$\{\{\s*matrix\.tier\s*\}\}|\S+)", block):
        for t in m.group(1).split(","):
            if "matrix.tier" in t:
                tiers.update(matrix)
            else:
                tiers.add(t)
    for m in re.finditer(r"--preset\s+([a-z]+)", block):
        tiers.update(preset_map.get(m.group(1), []))
    return tiers


def check(repo: Path) -> list[str]:
    fails: list[str] = []
    gh = repo / "ci" / "github"
    readme_path = gh / "README.md"
    if not readme_path.is_file():
        return [f"{readme_path} missing"]
    rows, problems = parse_rows(readme_path.read_text(encoding="utf-8"))
    fails += problems
    by_id = {r[0].strip("` "): r for r in rows}

    required: list[tuple[str, str | None]] = []
    fg = repo / "ci" / "tools" / "feature-gates.txt"
    if fg.is_file():
        for i, line in enumerate(gate_lines(fg), start=1):
            required.append((f"feature-gates.txt line {i}", line))
    else:
        fails.append("ci/tools/feature-gates.txt missing")

    tier_files = {p.stem for p in (repo / "ci" / "tiers").glob("*.sh")}
    preset_map = presets(repo / "ci" / "run-all.sh")
    wf_cache: dict[str, dict[str, str]] = {}
    for gid, cmd in required:
        row = by_id.get(gid)
        if row is None:
            fails.append(f"gate-map: no row for '{gid}'")
            continue
        if cmd is not None and f"`{cmd}`" not in row[1]:
            fails.append(f"gate-map: row '{gid}' does not quote the command `{cmd}`")
    triggers_cache: dict[str, set[str]] = {}
    for gid, cmd, job, tiers, when in rows:
        job = job.strip("` ")
        claimed = [w.strip("` ") for w in when.split(",") if w.strip("` ") not in ("", "-")]
        for w in claimed:
            if w not in WHEN_VALUES:
                fails.append(f"gate-map: row '{gid}': when '{w}' is none of {', '.join(WHEN_VALUES)}")
        tier_gate = re.fullmatch(r"tier ([a-z0-9-]+)", gid)
        premerge = bool(tier_gate and tier_gate.group(1) in PRE_MERGE_TIERS)
        feature_gate = gid.startswith("feature-gates.txt line ")
        if job.startswith("none:"):
            if len(job[5:].strip()) < 20:
                fails.append(f"gate-map: row '{gid}': 'none:' needs a stated reason (>= 20 characters)")
            if claimed:
                fails.append(f"gate-map: row '{gid}': a row without a job cannot say when it runs (when '{when}')")
            continue
        if premerge and "pr" not in claimed:
            fails.append(f"gate-map: row '{gid}' is a pre-merge gate: its 'when' must include pr (it says '{when}')")
        if feature_gate and not {"pr", "schedule"} & set(claimed):
            fails.append(f"gate-map: row '{gid}' is a feature gate: its 'when' must include pr or schedule (it says '{when}')")
        want = [t.strip("` ") for t in tiers.split(",") if t.strip("` ") not in ("", "-")]
        have: set[str] = set()
        ok_jobs = True
        for one in [j.strip("` ") for j in job.split(",")]:
            m = re.fullmatch(r"([A-Za-z0-9_.-]+\.yml):([A-Za-z0-9_-]+)", one)
            if not m:
                fails.append(f"gate-map: row '{gid}': job '{one}' is neither file.yml:job nor none: <reason>")
                ok_jobs = False
                continue
            wf, jid = m.groups()
            if wf not in wf_cache:
                p = gh / wf
                text = p.read_text(encoding="utf-8") if p.is_file() else ""
                wf_cache[wf] = job_blocks(text)
                triggers_cache[wf] = workflow_triggers(text)
            blocks = wf_cache[wf]
            if jid not in blocks:
                fails.append(f"gate-map: row '{gid}': {wf} has no job '{jid}'")
                ok_jobs = False
                continue
            have |= job_tiers(blocks[jid], preset_map)
            for w in claimed:
                if w in WHEN_VALUES and w not in triggers_cache[wf]:
                    fails.append(
                        f"gate-map: row '{gid}' says it runs on {w}, but {wf} has no such trigger "
                        f"(it runs on: {', '.join(sorted(triggers_cache[wf])) or 'manual dispatch only'})"
                    )
        if not ok_jobs:
            continue
        for t in want:
            if t not in tier_files:
                fails.append(f"gate-map: row '{gid}': '{t}' is not a tier of ci/tiers/")
            elif t not in have:
                fails.append(f"gate-map: row '{gid}': job(s) {job} do not run tier '{t}' (they run: {', '.join(sorted(have)) or 'no tier'})")
    # Every tier of ci/tiers/ is run by some workflow job, and a workflow never
    # names a tier that does not exist (a dead tier or a typo would go unseen).
    used: set[str] = set()
    for wf in sorted(gh.glob("*.yml")):
        for jid, block in job_blocks(wf.read_text(encoding="utf-8")).items():
            t = job_tiers(block, preset_map)
            for name in t:
                if name not in tier_files:
                    fails.append(f"gate-map: {wf.name}:{jid} runs '{name}', which is not a tier of ci/tiers/")
            used |= t
            for why in gated_off(block):
                fails.append(f"gate-map: {wf.name}:{jid}: {why}: a gated-off tier is not a gate")
    for t in sorted(tier_files - used):
        fails.append(f"gate-map: tier '{t}' is run by no workflow job")
    return fails


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default=str(Path(__file__).resolve().parents[2]))
    a = ap.parse_args(argv)
    repo = Path(a.repo)
    if not (repo / "ci").is_dir():
        print(f"gate_map.py: {repo} has no ci/ directory", file=sys.stderr)
        return 2
    fails = check(repo)
    for f in fails:
        print("FAIL", f)
    if not fails:
        print("ok   gate map complete and consistent")
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
