#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): account for every test
# binary of a `cargo test --workspace --no-fail-fast` run and decide the
# verdict of a CI tier (ci/tiers/test.sh, gates.sh, release.sh run it on the
# host; python3 >= 3.11, standard library only).
#
#   analyze_tests.py --job-dir J --repo R --known-failures F [--gate]
#
# Inputs in the job directory:
#   summary.tsv          step<TAB>exit code<TAB>seconds (every step of the job)
#   checks.txt           "FAIL <reason>", "GATEFAIL <reason>" (fails a --gate
#                        run, warning otherwise) or "WARN <reason>" lines
#   metadata.json        `cargo metadata --no-deps --format-version 1` (when
#                        the test step ran)
#   test-artifacts.jsonl `cargo test --workspace --no-run --message-format=json`
#   test.log             the output of `cargo test --workspace --no-fail-fast`
# Outputs: test-counts.txt (counts, every test binary and its result, failed
# targets, failures), verdict.txt; the last line printed is the verdict:
#   GATE: PASS (N known failures) | GATE: IMPROVED (...) | GATE: FAIL (...)
# ("RESULT:" instead of "GATE:" without --gate). Exit code 0 for PASS and
# IMPROVED, 1 for FAIL.
#
# Accounting rules (nothing may disappear):
#   * the expected test binaries are the artifacts of the --no-run build with
#     profile.test = true; every one must have run ("Running ... (<path>)")
#     and printed exactly one libtest "test result:" line, except targets
#     declared `harness = false` (judged by their exit code alone);
#   * a binary killed by a signal, aborted, timed out or exiting early (even
#     with status 0, e.g. std::process::exit(0) in a test) has no result line
#     and counts as a failed target, named;
#   * every lib target with doctest = true must have a "Doc-tests" section
#     with at least one result line (edition-2024 merged doctests print one
#     for the merged group and one for the standalone tests: they are summed);
#   * the failed test names of a binary (its "failures:" list and its
#     "test ... FAILED" lines) must agree with its result line's count;
#   * cargo's own list of failed targets ("error: N targets failed:" or the
#     single "error: test failed, to rerun pass ...") must equal the set of
#     targets derived above, and cargo's exit code must be 0 (no failure) or
#     101 (failures, all accounted for); anything else fails the gate;
#   * the failures are compared with the expected-failure baseline
#     (known-failures.txt): any failure not in it, or any failed target
#     without a named failing test, fails; a baseline entry that now passes
#     (or is absent from a complete run) gives IMPROVED. No other waiver.
import argparse
import json
import os
import re
import sys
import tomllib

# The executable path is absolute when CARGO_TARGET_DIR is (the images,
# CI) and relative when the target directory is under the working directory
# (a host run with the default `target/`, e.g. macOS); both resolve through
# the basename map below.
RUNNING = re.compile(r"Running (?:unittests )?(\S+) \(([^)\s]+)\)")
DOCTESTS = re.compile(r"Doc-tests (\S+)\s*$")
RESULT = re.compile(
    r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; "
    r"(\d+) measured; (\d+) filtered out"
)
TEST_LINE = re.compile(r"^test (.+?) \.\.\. (ok|FAILED|ignored.*)$")
TEST_FAILED_ANYWHERE = re.compile(r"test (\S+) \.\.\. FAILED\s*$")
RERUN = re.compile(r"error: (?:test|doctest) failed, to rerun pass `([^`]+)`")
SUMMARY = re.compile(r"error: (\d+) targets? failed:")
SUMMARY_ITEM = re.compile(r"^\s+`([^`]+)`\s*$")
PROC_FAIL = re.compile(r"process didn't exit successfully: `([^`]*)` \((.*)\)")


def norm_target(args):
    """'-p akantu-parser --test oracle' -> 'akantu-parser | --test oracle'."""
    toks = args.split()
    pkg = ""
    rest = []
    i = 0
    while i < len(toks):
        if toks[i] == "-p" and i + 1 < len(toks):
            pkg = toks[i + 1]
            i += 2
            continue
        rest.append(toks[i])
        i += 1
    return f"{pkg} | {' '.join(rest)}"


# cargo test runs the doc-tests only without a target selection or with
# --doc; any of these flags (before `--`) selects targets and drops them
TARGET_FLAGS = ("--lib", "--bin", "--bins", "--example", "--examples", "--test", "--tests",
                "--bench", "--benches", "--all-targets")


def selects_targets(cmd):
    """True when the cargo command selects targets explicitly (no --doc), so
    that cargo runs no doc-tests (the `--test <name>` lines of feature-gates.txt)."""
    words = cmd.split()
    if "--" in words:
        words = words[:words.index("--")]
    flags = {w.split("=")[0] for w in words}
    return "--doc" not in flags and any(f in flags for f in TARGET_FLAGS)


def kind_arg(kind, name):
    k = kind[0] if kind else ""
    if k in ("lib", "rlib", "dylib", "cdylib", "staticlib", "proc-macro"):
        return "--lib"
    return f"--{k} {name}"


def load_harness_false(manifests):
    """{(package, '--test name')} for targets declared harness = false."""
    out = set()
    for pkg, path in manifests.items():
        try:
            with open(path, "rb") as f:
                doc = tomllib.load(f)
        except (OSError, tomllib.TOMLDecodeError):
            continue
        for sect, flag in (("test", "--test"), ("bin", "--bin"), ("bench", "--bench"), ("example", "--example")):
            for t in doc.get(sect, []) or []:
                if t.get("harness", True) is False and "name" in t:
                    out.add(f"{pkg} | {flag} {t['name']}")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--job-dir", required=True)
    ap.add_argument("--repo", required=True)
    ap.add_argument("--known-failures", required=True)
    ap.add_argument("--gate", action="store_true")
    a = ap.parse_args()
    J = a.job_dir
    label = "GATE" if a.gate else "RESULT"
    fails = []  # reasons for FAIL
    warns = []
    improved = []
    counts_out = []

    # ---- steps and pre-checks -------------------------------------------
    steps = []
    try:
        with open(os.path.join(J, "summary.tsv")) as f:
            for line in f:
                p = line.rstrip("\n").split("\t")
                if len(p) >= 2:
                    steps.append((p[0], p[1]))
    except OSError:
        fails.append("no summary.tsv: the job did not run its steps")
    try:
        with open(os.path.join(J, "checks.txt")) as f:
            for line in f:
                line = line.rstrip("\n")
                if line.startswith("FAIL "):
                    fails.append(line[5:])
                elif line.startswith("GATEFAIL "):
                    (fails if a.gate else warns).append(line[9:] + ("" if a.gate else " (fails a --gate run)"))
                elif line.startswith("WARN "):
                    warns.append(line[5:])
    except OSError:
        pass
    step_rc = dict(steps)
    # Feature-gated commands (ci/tools/feature-gates.txt, the `gates` tier):
    # features.tsv lists N<TAB>kind<TAB>command; a `test` command ran as step
    # feature-N (accounted for below), its --no-run build as feature-N-build.
    feature_runs = []
    try:
        with open(os.path.join(J, "features.tsv")) as f:
            for line in f:
                p = line.rstrip("\n").split("\t")
                if len(p) == 3:
                    feature_runs.append((f"feature-{p[0]}", p[1], p[2]))
    except OSError:
        pass
    test_like = {"test"} | {name for name, kind, _ in feature_runs if kind == "test"}
    command_of = {}
    for name, kind, cmd in feature_runs:
        command_of[name] = cmd
        command_of[f"{name}-build"] = f"{cmd} --no-run"
    for name, rc in steps:
        if name in test_like:
            continue
        if rc != "0":
            fails.append(f"step {name} exited {rc}" + (f" ({command_of[name]})" if name in command_of else ""))
    for name, _, cmd in feature_runs:
        if name not in step_rc:
            fails.append(f"step {name} never ran ({cmd})")

    # ---- baseline --------------------------------------------------------
    baseline = {}  # (target, test) -> blocker
    with open(a.known_failures) as f:
        for n, line in enumerate(f, 1):
            s = line.split("#", 1)[0].strip()
            if not s:
                continue
            p = [x.strip() for x in s.split("|")]
            if len(p) != 4 or not all(p):
                fails.append(f"known-failures.txt line {n} is malformed (want: package | target | test | blocker)")
                continue
            baseline[(f"{p[0]} | {p[1]}", p[2])] = p[3]

    def account(run, log_name, art_name, subset, cmd=""):
        """Account for one `cargo test` step (the workspace `test` step, or a
        feature-gated `cargo test` line `cmd` when subset is true: its
        doc-tests are expected for the selected packages only, and not at all
        when the line selects targets explicitly). Problems go to fails,
        prefixed with the step name for feature runs."""
        px = f"[{run}] " if subset else ""
        failures = {}  # (target, test) -> True
        passed_tests = set()  # (target, test)
        failed_targets = {}  # target -> [reasons]
        complete = True
        # ---- expected binaries and doc-test crates ----------------------
        manifests = {}
        doc_crates = {}  # crate name -> target id
        try:
            with open(os.path.join(J, "metadata.json")) as f:
                meta = json.load(f)
            for pkg in meta.get("packages", []):
                mp = pkg["manifest_path"]
                if mp.startswith("/src/"):
                    mp = os.path.join(a.repo, mp[len("/src/"):])
                manifests[pkg["name"]] = mp
                for t in pkg.get("targets", []):
                    kinds = t.get("kind", [])
                    if t.get("doctest") and any(k in ("lib", "rlib", "dylib", "proc-macro") for k in kinds):
                        doc_crates[t["name"].replace("-", "_")] = f"{pkg['name']} | --doc"
        except (OSError, ValueError, KeyError) as e:
            fails.append(px + f"cannot read metadata.json ({e}): test binaries cannot be accounted for")
            complete = False
        harness_false = load_harness_false(manifests)
        pkg_by_manifest = {os.path.realpath(v): k for k, v in manifests.items()}
        expected = {}  # executable path -> target id
        try:
            with open(os.path.join(J, art_name)) as f:
                for line in f:
                    try:
                        m = json.loads(line)
                    except ValueError:
                        continue
                    if not isinstance(m, dict) or m.get("reason") != "compiler-artifact":
                        continue
                    if not m.get("executable") or not (m.get("profile") or {}).get("test"):
                        continue
                    mp = m.get("manifest_path", "")
                    if mp.startswith("/src/"):
                        mp = os.path.join(a.repo, mp[len("/src/"):])
                    pkg = pkg_by_manifest.get(os.path.realpath(mp))
                    if pkg is None:
                        continue  # not a workspace member
                    t = m["target"]
                    expected[m["executable"]] = f"{pkg} | {kind_arg(t.get('kind', []), t['name'])}"
        except OSError as e:
            fails.append(px + f"cannot read {art_name} ({e})")
            complete = False
        if complete and not expected:
            fails.append(px + "the --no-run build listed no test binary")
            complete = False

        if subset:
            # `cargo test -p X …` runs the doc-tests of the selected packages only
            selected = {t.split(" | ")[0] for t in expected.values()}
            doc_crates = {c: t for c, t in doc_crates.items() if t.split(" | ")[0] in selected}
            if selects_targets(cmd):
                # `cargo test -p X --test T` runs no doc-tests
                doc_crates = {}
        by_base = {os.path.basename(k): k for k in expected}
        # ---- sections of test.log ---------------------------------------
        sections = []  # dicts: target, kind(run|doc), results, failed names, ok names, cargo errors
        cargo_summary = None
        cargo_rerun = []
        cur = None
        in_failures_list = False
        summary_items = None
        try:
            with open(os.path.join(J, log_name), errors="replace") as f:
                lines = f.read().split("\n")
        except OSError:
            lines = []
            fails.append(px + f"no {log_name}")
        for raw in lines:
            line = raw.rstrip("\r")
            if summary_items is not None:
                m = SUMMARY_ITEM.match(line)
                if m:
                    summary_items.append(norm_target(m.group(1)))
                    continue
                summary_items = None
            m = RUNNING.search(line)
            exe = None
            if m:
                exe = m.group(2) if m.group(2) in expected else by_base.get(os.path.basename(m.group(2)))
            if exe:
                cur = {"target": expected[exe], "exe": exe, "doc": False,
                       "results": [], "failed": set(), "ok": set(), "errors": []}
                sections.append(cur)
                in_failures_list = False
                continue
            m = DOCTESTS.search(line)
            if m and m.group(1) in doc_crates:
                cur = {"target": doc_crates[m.group(1)], "exe": None, "doc": True,
                       "results": [], "failed": set(), "ok": set(), "errors": []}
                sections.append(cur)
                in_failures_list = False
                continue
            m = SUMMARY.search(line)
            if m:
                cargo_summary = (int(m.group(1)), [])
                summary_items = cargo_summary[1]
                continue
            m = RERUN.search(line)
            if m:
                cargo_rerun.append(norm_target(m.group(1)))
                if cur is not None:
                    cur["errors"].append(f"cargo: test failed ({m.group(1)})")
                continue
            m = PROC_FAIL.search(line)
            if m and cur is not None:
                cur["errors"].append(f"process didn't exit successfully ({m.group(2)})")
                continue
            if cur is None:
                continue
            m = RESULT.search(line)
            if m:
                cur["results"].append(tuple(int(x) for x in m.groups()[1:]))
                in_failures_list = False
                continue
            if line == "failures:":
                in_failures_list = True
                continue
            if in_failures_list:
                if line.startswith("    ") and line.strip():
                    cur["failed"].add(line.strip())
                    continue
                if line.startswith("---- "):
                    in_failures_list = False
            m = TEST_LINE.match(line)
            if m:
                if m.group(2) == "ok":
                    cur["ok"].add(m.group(1))
                elif m.group(2) == "FAILED":
                    cur["failed"].add(m.group(1))
                continue
            m = TEST_FAILED_ANYWHERE.search(line)
            if m:
                cur["failed"].add(m.group(1))

        def tfail(target, reason):
            failed_targets.setdefault(target, []).append(reason)

        totals = [0, 0, 0, 0, 0]
        seen_exe = {}
        seen_doc = {}
        for s in sections:
            t = s["target"]
            if s["doc"]:
                seen_doc[t] = seen_doc.get(t, 0) + 1
            else:
                seen_exe[s["exe"]] = seen_exe.get(s["exe"], 0) + 1
            # Edition-2024 merged doctests print one result line for the merged
            # group and one for the standalone ones (compile_fail, ...): a
            # Doc-tests section may sum several; a test binary prints one.
            if len(s["results"]) > 1 and not s["doc"]:
                tfail(t, f"{len(s['results'])} 'test result' lines in one section")
            if not s["results"]:
                if t in harness_false and not s["errors"]:
                    continue  # harness = false, exit status 0 (no cargo error)
                why = "; ".join(s["errors"]) or "exited without error"
                tfail(t, f"no 'test result' line ({why}): killed, aborted, timed out or exited early")
                for name in s["failed"]:
                    failures[(t, name)] = True
                continue
            p, fl, ig, me, fo = (sum(r[i] for r in s["results"]) for i in range(5))
            for i, v in enumerate((p, fl, ig, me, fo)):
                totals[i] += v
            names = s["failed"]
            if fl != len(names):
                tfail(t, f"result line says {fl} failed but {len(names)} failing test names were found")
            for name in names:
                failures[(t, name)] = True
            for name in s["ok"]:
                passed_tests.add((t, name))
            if fl == 0 and s["errors"]:
                tfail(t, "; ".join(s["errors"]) + " although its result line has 0 failures")
        # (sorted by target; no keyword arguments named like credentials, so
        # that this file passes fetch.sh's credential filter unchanged)
        for t, exe in sorted((t, exe) for exe, t in expected.items()):
            n = seen_exe.get(exe, 0)
            if n == 0:
                tfail(t, "never ran (no 'Running' line): cargo stopped or was killed before it")
            elif n > 1:
                tfail(t, f"ran {n} times")
        for crate, t in sorted(doc_crates.items()):
            if seen_doc.get(t, 0) == 0:
                tfail(t, "doc-tests never ran")

        derived = set(failed_targets) | {t for (t, _) in failures}
        cargo_set = set(cargo_summary[1]) if cargo_summary else set(cargo_rerun)
        rc = step_rc.get(run)
        if rc == "0":
            if derived:
                fails.append(px + "cargo test exited 0 but failures were found in its output")
            if cargo_summary:
                fails.append(px + "cargo test exited 0 but printed a failed-target list")
        elif rc == "101":
            if not cargo_set:
                fails.append(px + "cargo test exited 101 without naming any failed target")
            if cargo_summary and cargo_summary[0] != len(cargo_summary[1]):
                fails.append(px + f"cargo said {cargo_summary[0]} targets failed but listed {len(cargo_summary[1])}")
            for t in sorted(cargo_set - derived):
                tfail(t, "listed as failed by cargo, but no failure was found in its output")
            for t in sorted(derived - cargo_set):
                fails.append(px + f"{t}: failures found in its output but cargo did not list it as failed")
        else:
            fails.append(px + f"cargo test exited {rc} (only 0 or 101 are understood: killed, timed out or crashed)")

        return {
            "failures": failures, "passed_tests": passed_tests, "failed_targets": failed_targets,
            "sections": sections, "derived": derived, "cargo_set": cargo_set,
            "cargo_summary": cargo_summary, "totals": totals, "expected": expected,
            "seen_exe": seen_exe, "doc_crates": doc_crates, "seen_doc": seen_doc,
            "complete": complete,
        }

    def counts_lines(r):
        return [
            "result_lines=%d passed=%d failed=%d ignored=%d measured=%d filtered_out=%d"
            % (sum(len(s["results"]) for s in r["sections"]), *r["totals"]),
            "binaries_expected=%d binaries_run=%d doctest_crates=%d doctest_sections=%d targets_failed=%d cargo_targets_failed=%s"
            % (len(r["expected"]), len(r["seen_exe"]), len(r["doc_crates"]), sum(r["seen_doc"].values()),
               len(r["derived"]), r["cargo_summary"][0] if r["cargo_summary"] else len(r["cargo_set"])),
        ]

    def detail_lines(r, indent):
        out = [f"{indent}cargo_failed_targets:"]
        out += [f"{indent}  {t}" for t in sorted(r["cargo_set"])]
        out.append(f"{indent}failed_tests:")
        for k in sorted(r["failures"]):
            tag = f"known {baseline[k]}" if k in baseline else "NEW"
            out.append(f"{indent}  {k[0]} | {k[1]}  [{tag}]")
        out.append(f"{indent}failed_targets_without_result:")
        for t in sorted(r["failed_targets"]):
            out.append(f"{indent}  {t}: {'; '.join(r['failed_targets'][t])}")
        out.append(f"{indent}binaries (target, passed, failed, ignored):")
        for s in r["sections"]:
            x = [sum(y[i] for y in s["results"]) for i in range(3)] if s["results"] else None
            out.append(f"{indent}  {s['target']}\t" + (f"{x[0]}\t{x[1]}\t{x[2]}" if x else "NO RESULT LINE"))
        return out

    complete = False
    known_keys = set()
    report_counts = []
    if "test" in step_rc:
        complete = True
        r = account("test", "test.log", "test-artifacts.jsonl", False)
        failures, passed_tests, failed_targets, sections = (
            r["failures"], r["passed_tests"], r["failed_targets"], r["sections"])
        # ---- compare with the baseline ----------------------------------
        extra = sorted(k for k in failures if k not in baseline)
        unnamed = sorted(t for t in failed_targets)
        for k in extra:
            fails.append(f"new failure: {k[0]} | {k[1]}")
        for t in unnamed:
            fails.append(f"failed target: {t}: {'; '.join(failed_targets[t])}")
        known_seen = sorted(k for k in failures if k in baseline)
        for k, blocker in sorted(baseline.items()):
            if k in failures:
                continue
            if k in passed_tests:
                improved.append(f"{k[0]} | {k[1]} ({blocker}) now passes")
            elif not any(s["target"] == k[0] for s in sections):
                fails.append(f"known failure {k[0]} | {k[1]} ({blocker}): its target did not run")
            else:
                improved.append(f"{k[0]} | {k[1]} ({blocker}) absent from the run (renamed or removed?)")

        counts_out += counts_lines(r)
        counts_out += detail_lines(r, "")
        report_counts += counts_out[:2]
        known_keys |= set(known_seen)
    for run, kind, cmd in feature_runs:
        if kind != "test" or run not in step_rc:
            continue
        complete = True
        r = account(run, f"{run}.log", f"{run}-artifacts.jsonl", True, cmd)
        for k in sorted(k for k in r["failures"] if k not in baseline):
            fails.append(f"new failure: [{run}] {k[0]} | {k[1]}")
        for t in sorted(r["failed_targets"]):
            fails.append(f"failed target: [{run}] {t}: {'; '.join(r['failed_targets'][t])}")
        known_keys |= {k for k in r["failures"] if k in baseline}
        c = counts_lines(r)
        counts_out.append(f"{run}: {cmd}")
        counts_out += [f"  {x}" for x in c]
        counts_out += detail_lines(r, "  ")
        report_counts.append(f"{run}: {c[0]} {c[1]}")
    n_known = len(known_keys)

    # ---- verdict ---------------------------------------------------------
    if fails:
        verdict = f"{label}: FAIL ({len(fails)} problem{'s' if len(fails) != 1 else ''})"
        rc = 1
    elif improved:
        verdict = f"{label}: IMPROVED ({len(improved)} known failure{'s' if len(improved) != 1 else ''} no longer failing, {n_known} still failing: update ci/tools/known-failures.txt)"
        rc = 0
    elif complete:
        verdict = f"{label}: PASS ({n_known} known failure{'s' if n_known != 1 else ''})"
        rc = 0
    else:
        verdict = f"{label}: PASS (no test step)"
        rc = 0
    with open(os.path.join(J, "test-counts.txt"), "w") as f:
        f.write("\n".join(counts_out) + ("\n" if counts_out else ""))
    report = []
    report += report_counts
    for w in warns:
        report.append(f"WARNING: {w}")
    for i in improved:
        report.append(f"IMPROVED: {i}")
    for x in fails:
        report.append(f"FAIL: {x}")
    report.append(verdict)
    with open(os.path.join(J, "verdict.txt"), "w") as f:
        f.write("\n".join(report) + "\n")
    print("\n".join(report))
    return rc


if __name__ == "__main__":
    sys.exit(main())
