#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu): offline self-test of ci/ —
# light enough for a developer machine (no cargo build, no Docker).
#
#   ci/selftest.sh
#
# Checks: bash -n of every ci/ script with the running bash and, when
# present, /bin/bash (macOS bash 3.2); shellcheck -x when installed; the
# Python unit tests of ci/tools (synthetic cargo workspaces, nothing
# compiled); the step/tier status semantics of ci/lib.sh (pass, fail with
# its exit code, skip -> incomplete, na ignored, --ok-exit); run-all.sh
# usage errors, --list and a --dry-run (verdict INCOMPLETE, exit 3); the
# workflow files parse as YAML when PyYAML is available, never reference
# secrets and live outside .github/; the fault-injection cases of
# ci/fault_injection.sh (F-cases: a step that did not run or
# tested nothing is never a pass) and the licence-audit injection unit
# tests (R7, R8 …). Exit codes: 0 everything ran and passed; 1 a check
# failed; 3 nothing failed but a check could not run (shellcheck, PyYAML,
# actionlint or cargo missing, a skipped unit test): a skipped check is never
# a pass. CI_SELFTEST_ALLOW_SKIP=1 turns the 3 into 0 for a
# developer machine that lacks the optional tools (printed as such).
# The helper functions below are invoked indirectly (as words of `check` and
# `exit_code`), which shellcheck >= 0.9 reports as unreachable code.
# shellcheck disable=SC2317,SC2329
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"
fails=0
skips=0
ok() { echo "ok   $*"; }
skip() {
  echo "skip $*"
  skips=$((skips + 1))
}
bad() {
  echo "FAIL $*"
  fails=$((fails + 1))
}
# check <description> <command...>: ok if the command succeeds.
check() {
  local what="$1"
  shift
  if "$@"; then ok "$what"; else bad "$what"; fi
}
tmp="$(mktemp -d "${TMPDIR:-/tmp}/ci-selftest.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

scripts=""
for f in "$repo"/ci/*.sh "$repo"/ci/tiers/*.sh "$repo"/ci/github/*.sh; do
  scripts="$scripts ${f#"$repo"/}"
done
syntax() { # syntax <bash>
  local f r=0
  for f in $scripts; do "$1" -n "$repo/$f" || r=1; done
  return "$r"
}
for sh in bash /bin/bash; do
  command -v "$sh" > /dev/null 2>&1 || continue
  check "$sh -n ($("$sh" -c "echo \$BASH_VERSION"))" syntax "$sh"
done
if command -v shellcheck > /dev/null 2>&1; then
  # $scripts is a list of repository-relative paths without spaces; the
  # single-quoted script gets its arguments positionally.
  # shellcheck disable=SC2086,SC2016
  check "shellcheck -x ci/" sh -c 'cd "$1" && shift && shellcheck -x "$@"' sh "$repo" $scripts
else
  skip "shellcheck (not installed)"
fi

if PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s "$here/tools" -p 'test_*.py' > "$tmp/unittest.log" 2>&1; then
  ok "python unit tests ($(grep -o 'Ran [0-9]* tests' "$tmp/unittest.log"))"
  if grep -q 'skipped=' "$tmp/unittest.log"; then
    skip "python unit tests: $(grep -o 'skipped=[0-9]*' "$tmp/unittest.log") (cargo missing?)"
  fi
else
  bad "python unit tests"
  tail -n 30 "$tmp/unittest.log"
fi

# ci/lib.sh semantics: a scripted tier in a separate bash with its own CI_OUT.
cat > "$tmp/demo.sh" << 'EOF'
. "$1/lib.sh"
ci_tier_begin demo
ci_step good -- true
ci_step judged --ok-exit "0 3" -- sh -c 'exit 3'
ci_step broken -- sh -c 'echo out; exit 7'
ci_skip absent "no tool"
ci_na other "not here"
ci_tier_end
echo "rc=$?" > "$CI_OUT/rc"
EOF
cat > "$tmp/demo2.sh" << 'EOF'
. "$1/lib.sh"
ci_tier_begin demo
ci_step good -- true
ci_skip absent "no tool"
ci_tier_end
echo "rc=$?" > "$CI_OUT/rc"
EOF
env CI_OUT="$tmp/lib" CI_BACKEND=host bash "$tmp/demo.sh" "$here" 2> /dev/null
env CI_OUT="$tmp/lib2" CI_BACKEND=host bash "$tmp/demo2.sh" "$here" 2> /dev/null
got="$(cut -f1-3 "$tmp/lib/demo/steps.tsv" | tr '\t\n' ':;')"
want="good:pass:0;judged:pass:3;broken:fail:7;absent:skip:-;other:na:-;"
if [ "$got" = "$want" ]; then ok "lib.sh step records"; else bad "lib.sh step records: $got (want $want)"; fi
check "lib.sh fail status" test "$(cat "$tmp/lib/demo/status") $(cat "$tmp/lib/rc")" = "fail rc=1"
check "lib.sh step log" grep -q '^out$' "$tmp/lib/demo/broken.log"
check "lib.sh incomplete status" test "$(cat "$tmp/lib2/demo/status") $(cat "$tmp/lib2/rc")" = "incomplete rc=3"

# run-all.sh interface.
exit_code() { # exit_code <want> <command...>
  local want="$1"
  shift
  "$@" > /dev/null 2>&1
  [ "$?" = "$want" ]
}
check "run-all: unknown tier is a usage error" exit_code 2 bash "$here/run-all.sh" --tiers nosuch
check "run-all: --preset with --tiers refused" exit_code 2 bash "$here/run-all.sh" --preset fast --tiers lint
ntiers=0
for f in "$here"/tiers/*.sh; do ntiers=$((ntiers + 1)); done
n="$(bash "$here/run-all.sh" --list | grep -vc '^preset')"
check "run-all --list names every tier ($n of $ntiers)" test "$n" = "$ntiers"
bash "$here/run-all.sh" --preset fast --dry-run --out "$tmp/dry" > "$tmp/dry.log" 2>&1
rc=$?
if [ "$rc" = 3 ] && tail -n 1 "$tmp/dry.log" | grep -q '^CI: INCOMPLETE' && [ -s "$tmp/dry/summary.json" ]; then
  ok "run-all --dry-run: INCOMPLETE, exit 3, summary.json"
else
  bad "run-all --dry-run (exit $rc): $(tail -n 1 "$tmp/dry.log")"
fi

# Proposed workflows: no secrets, valid YAML (when PyYAML exists). Before
# activation no .github/workflows exists; activation (D-080, D-114) COPIES the
# chosen ci/github/*.yml there, and from then on every active workflow must be
# byte-identical to its ci/github/ copy, which the checks below read
# (without this the activation commit makes `selftest` red on its first
# GitHub run).
if [ -d "$repo/.github/workflows" ]; then
  active_match_proposals() {
    local f r=0 n=0
    for f in "$repo"/.github/workflows/*; do
      [ -e "$f" ] || continue
      n=$((n + 1))
      cmp -s "$f" "$here/github/$(basename "$f")" || {
        echo "not an unchanged copy of ci/github/$(basename "$f"): .github/workflows/$(basename "$f")"
        r=1
      }
    done
    [ "$n" -ge 1 ] || { echo ".github/workflows is empty"; r=1; }
    return "$r"
  }
  check "every .github/workflows file is an unchanged copy of its ci/github/ proposal (activated)" active_match_proposals
else
  ok "no .github/workflows (proposals stay in ci/github/)"
fi
if grep -n 'secrets\.' "$here"/github/*.yml > "$tmp/secrets.txt" 2> /dev/null; then
  bad "workflow files reference secrets: $(head -n 3 "$tmp/secrets.txt")"
else
  ok "workflow files reference no secrets"
fi
if python3 -c 'import yaml' > /dev/null 2>&1; then
  cat > "$tmp/yamlcheck.py" << 'EOF'
import sys
import yaml
for p in sys.argv[1:]:
    doc = yaml.safe_load(open(p))
    on = doc.get(True, doc.get("on"))  # YAML 1.1 reads `on:` as True
    assert isinstance(doc.get("jobs"), dict) and on is not None, p
    assert "pull_request_target" not in (on if isinstance(on, dict) else {}), p
    perms = doc.get("permissions")
    assert perms == {"contents": "read"}, f"{p}: permissions must be contents: read, got {perms}"
EOF
  check "workflow YAML parses (permissions contents: read, no pull_request_target)" python3 "$tmp/yamlcheck.py" "$here"/github/*.yml
  # D-114: the Pages deployment is gated and least-privileged.
  cat > "$tmp/pagescheck.py" << 'PYEOF'
import sys
import yaml
doc = yaml.safe_load(open(sys.argv[1]))
jobs = doc["jobs"]
assert {"rustdoc", "site", "deploy"} <= set(jobs), sorted(jobs)
for j in ("site", "deploy"):
    cond = jobs[j].get("if", "")
    assert "github.event_name == 'push'" in cond and "refs/heads/main" in cond, (j, cond)
assert jobs["deploy"]["permissions"] == {"contents": "read", "pages": "write", "id-token": "write"}, jobs["deploy"].get("permissions")
assert jobs["deploy"]["environment"]["name"] == "github-pages"
assert jobs["deploy"]["needs"] == "site" and jobs["site"]["needs"] == "rustdoc"
for name, job in jobs.items():
    if name != "deploy":
        assert "permissions" not in job, f"{name} must use the workflow's read-only permissions"
        assert not any("deploy-pages" in str(st.get("uses", "")) for st in job["steps"]), name
PYEOF
  check "docs.yml: the Pages deploy job is main-push only, alone has pages: write, uses the github-pages environment" python3 "$tmp/pagescheck.py" "$here/github/docs.yml"
else
  skip "workflow YAML parse (PyYAML not installed)"
fi

# GitHub semantics that PyYAML cannot see (`hashFiles` in a
# job-level `if`, unpinned actions, curl | sh): actionlint 1.7.12 when
# available — a local `actionlint` binary, else the Docker Hub image pinned
# by digest (never downloaded from GitHub; the image is pulled only when
# CI_SELFTEST_PULL_ACTIONLINT=1, so an offline run skips instead of
# fetching) — and always the text checks below.
ACTIONLINT_IMAGE="rhysd/actionlint@sha256:b1934ee5f1c509618f2508e6eb47ee0d3520686341fec936f3b79331f9315667"
run_actionlint() {
  if command -v actionlint > /dev/null 2>&1; then
    actionlint "$here"/github/*.yml
  else
    local f names=()
    for f in "$here"/github/*.yml; do names+=("./$(basename "$f")"); done
    docker run --rm -v "$here/github:/work:ro" -w /work "$ACTIONLINT_IMAGE" -no-color "${names[@]}"
  fi
}
if command -v actionlint > /dev/null 2>&1 || {
  command -v docker > /dev/null 2>&1 && {
    docker image inspect "$ACTIONLINT_IMAGE" > /dev/null 2>&1 \
      || { [ "${CI_SELFTEST_PULL_ACTIONLINT:-0}" = 1 ] && docker pull -q "$ACTIONLINT_IMAGE" > /dev/null 2>&1; }
  }
}; then
  check "actionlint ci/github/*.yml (GitHub expression contexts, schema)" run_actionlint
else
  skip "actionlint (no local binary; image $ACTIONLINT_IMAGE not present, CI_SELFTEST_PULL_ACTIONLINT=1 pulls it)"
fi
workflow_text_checks() {
  local r=0
  # hashFiles is allowed in step-level expressions only: an `if:` that
  # starts a job-level key (4 spaces of indentation) must not use it.
  if grep -nE '^    if:.*hashFiles' "$here"/github/*.yml; then r=1; fi
  # every `uses:` of an action is pinned to a full 40-hex commit SHA.
  if grep -nE '^\s*(-\s+)?uses:\s' "$here"/github/*.yml | grep -vE 'uses:\s+[A-Za-z0-9._/-]+@[0-9a-f]{40}(\s|$)'; then r=1; fi
  # no installer piped into a shell, no floating container tags.
  if grep -nE 'curl[^|]*\|\s*(sh|bash)' "$here"/github/*.yml; then r=1; fi
  if grep -nE '^\s*container:\s+[^$]\S+' "$here"/github/*.yml | grep -vE '@sha256:[0-9a-f]{64}'; then r=1; fi
  return "$r"
}
check "workflow files: no job-level hashFiles, actions pinned to SHAs, no curl|sh, containers by digest or variable" workflow_text_checks

# ci/github/replay.sh replays one job of a workflow on a Linux host with
# Docker word for word; a command of a workflow file that is not in it would
# go unreplayed.
replay_covers_workflows() {
  local r=0 line cmd
  while IFS= read -r line; do
    cmd="${line#*run: }"
    cmd="$(printf '%s' "$cmd" | sed -e 's/\${{ matrix\.[a-z]* }}/$LEG/g' -e 's/\${{ github\.sha }}/$GITHUB_SHA/g')"
    if ! grep -qF -- "$cmd" "$here/github/replay.sh"; then
      echo "not replayed by ci/github/replay.sh: $cmd"
      r=1
    fi
  done < <(grep -hE '^\s+(- )?run: (bash )?(ci/run-all\.sh|ci/selftest\.sh|ci/github/distro-run\.sh|ci/github/pages-site\.sh)' "$here"/github/*.yml)
  return "$r"
}
check "every ci/run-all.sh command of a workflow is replayed by ci/github/replay.sh" replay_covers_workflows
# The mdBook lines of docs.yml's `manual` and `site` jobs are multi-line `run:`
# blocks that the check above does not see; the replay must carry them word for
# word.
replay_covers_mdbook() {
  local r=0 line n=0
  # shellcheck disable=SC2016 # the grep pattern is a regex: the $ is literal
  while IFS= read -r line; do
    n=$((n + 1))
    line="${line#"${line%%[![:space:]]*}"}"
    grep -qF -- "$line" "$here/github/replay.sh" || {
      echo "not replayed by ci/github/replay.sh: $line"
      r=1
    }
  done < <(grep -hE '^\s+("\$RUNNER_TEMP/mdbook/bin/mdbook"|cargo install mdbook)' "$here"/github/*.yml)
  [ "$n" -ge 2 ] || r=1
  return "$r"
}
check "the mdBook lines of docs.yml are replayed by ci/github/replay.sh" replay_covers_mdbook
# The assembling step of the Pages site, on synthetic parts
# (nothing is built): all three parts and their entry pages present -> OK with
# resolving links; a missing part, an empty part and a missing entry page fail.
pages_site_case() { # pages_site_case <want-rc> <fault: none|manual|rust|python|empty-python|entry-rust>
  local d="$tmp/pages-$2" rc f
  rm -rf "$d"
  mkdir -p "$d/manual" "$d/rust/akantu" "$d/python"
  echo '<html></html>' | tee "$d/manual/index.html" "$d/rust/akantu/index.html" "$d/python/akantu.html" > /dev/null
  case "$2" in
    manual | rust | python) rm -rf "${d:?}/$2" ;;
    empty-python) rm -rf "$d/python" && mkdir "$d/python" ;;
    entry-rust) rm -f "$d/rust/akantu/index.html" && echo x > "$d/rust/akantu/other.html" ;;
  esac
  bash "$here/github/pages-site.sh" "$d/site" --manual "$d/manual" --rustdoc "$d/rust" --python "$d/python" > "$d/log" 2>&1
  rc=$?
  [ "$rc" = "$1" ] || {
    cat "$d/log"
    return 1
  }
  if [ "$1" = 0 ]; then
    for f in index.html manual/index.html rust/akantu/index.html python/akantu.html; do [ -s "$d/site/$f" ] || return 1; done
  fi
}
check "pages-site.sh: the three parts are assembled and every index link resolves" pages_site_case 0 none
for fault in manual rust python empty-python entry-rust; do
  check "pages-site.sh: a faulty part ($fault) fails" pages_site_case 1 "$fault"
done
check "the second distribution runs in a digest-pinned image (ci/github/distro-run.sh)" grep -qE 'ubuntu:22\.04@sha256:[0-9a-f]{64}' "$here/github/distro-run.sh"

# Fault injection: every injected fault must be detected. Runs tiny
# synthetic cargo workspaces (a few seconds each, offline, nothing outside
# the scratch directory).
if command -v cargo > /dev/null 2>&1; then
  bash "$here/fault_injection.sh" "$tmp/fault" > "$tmp/fault.log" 2>&1
  frc=$?
  grep -E '^(F[0-9a-z]+ |fault_injection:)' "$tmp/fault.log"
  if [ "$frc" = 0 ]; then
    ok "fault injection: every injected fault detected ($(grep -c ' OK$' "$tmp/fault.log") cases)"
  else
    bad "fault injection (exit $frc)"
  fi
else
  skip "fault injection (cargo not installed)"
fi
# The gate map of ci/github/README.md is complete and consistent with the tree
# (every line of ci/tools/feature-gates.txt has a workflow job; every named job
# exists and runs the named tier; every tier of ci/tiers/ is run by some
# workflow job).
gate_map_ok() { python3 -I "$here/tools/gate_map.py" --repo "$repo" > "$tmp/gate-map.txt" 2>&1 || { cat "$tmp/gate-map.txt"; return 1; }; }
check "gate map complete: every feature-gates line has a job (ci/tools/gate_map.py)" gate_map_ok
gate_lines_recorded() {
  local want got
  want="$(grep -vc '^[[:space:]]*\(#\|$\)' "$repo/ci/tools/feature-gates.txt")"
  CI_GATES_FILE="$repo/ci/tools/feature-gates.txt" bash "$here/run-all.sh" --backend host --tiers gates --dry-run --out "$tmp/gates" > /dev/null 2>&1
  got="$(awk -F '\t' '$1 ~ /^feature-[0-9]+$/' "$tmp/gates/gates/steps.tsv" | wc -l | tr -d ' ')"
  [ "$want" = "$got" ] && [ "$want" -gt 0 ]
}
if command -v cargo > /dev/null 2>&1; then
  check "gates tier accounts for every line of feature-gates.txt (dry run)" gate_lines_recorded
else
  skip "gates tier dry run (cargo not installed)"
fi
for t in gates examples petsc version coverage pydocs memcheck exportall; do
  check "tier $t: dry run records its commands and ends INCOMPLETE or PASS, never a failure" sh -c "bash '$here/run-all.sh' --tiers $t --dry-run --out '$tmp/dry-$t' > /dev/null 2>&1; rc=\$?; [ \$rc = 0 ] || [ \$rc = 3 ]"
done
# A docker-backend step runs in a container whose PATH is
# not the host's. A double-quoted `env "PATH=$tools/bin:$PATH"` hands the HOST's
# PATH to the container (exit 127 on a runner whose cargo lives in ~/.cargo/bin,
# the coverage job). No tier script may build a PATH on the host side of a step.
no_host_path_in_steps() {
  ! grep -nE '^\s*ci_step\b.*\benv\s+"?PATH=' "$here"/tiers/*.sh
}
check "tier scripts hand no host PATH to a step (env PATH=...)" no_host_path_in_steps

# The judging rules of the memcheck tier (ci/memcheck/lib.sh)
# on planted logs, without valgrind or Docker: an error, a missing or short
# ERROR SUMMARY, a non-zero exit code and a missing result line each fail; an
# empty artifact list and a failed build fail.
mc_ok_log="$tmp/mc-ok.log"
printf '==11== ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 4 from 2)\n==12== ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)\n==11== ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 4 from 2)\nSUMMARY failed=0\n' > "$mc_ok_log"
printf 'ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 4 from 2)\nSUMMARY failed=0\n' > "$tmp/mc-short.log"
printf 'ERROR SUMMARY: 3 errors from 1 contexts (suppressed: 0 from 0)\nERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)\nSUMMARY failed=0\n' > "$tmp/mc-errors.log"
printf 'SUMMARY failed=0\n' > "$tmp/mc-nosummary.log"
printf -- '--1-- used_suppression:      3 pmix-a /x/openmpi.supp:19\n--2-- used_suppression:      1 pmix-a /x/openmpi.supp:19\n--2-- used_suppression:      2 ompi-b /x/openmpi.supp:30\n' > "$tmp/mc-used.log"
mc_judged() { # mc_judged <want> <ranks> <rc> <log> <regex>
  local got
  got="$(bash -c '. "$1/memcheck/lib.sh"; shift; mc_judge "$@"' sh "$here" "$2" "$3" "$4" "$5")"
  [ "${got%% *}" = "$1" ]
}
check "memcheck judge: a clean 2-rank run passes" mc_judged PASS 2 0 "$mc_ok_log" 'failed=0'
check "memcheck judge: one ERROR SUMMARY for two ranks fails" mc_judged FAIL 2 0 "$tmp/mc-short.log" 'failed=0'
check "memcheck judge: a memcheck error fails" mc_judged FAIL 2 0 "$tmp/mc-errors.log" 'failed=0'
check "memcheck judge: no ERROR SUMMARY fails" mc_judged FAIL 1 0 "$tmp/mc-nosummary.log" 'failed=0'
check "memcheck judge: valgrind's exit code 99 fails" mc_judged FAIL 2 99 "$mc_ok_log" 'failed=0'
check "memcheck judge: a missing result line fails" mc_judged FAIL 2 0 "$mc_ok_log" 'NEVER PRINTED'
check "memcheck: used suppressions are summed per name" test "$(bash -c '. "$1/memcheck/lib.sh"; mc_suppressions "$2"' sh "$here" "$tmp/mc-used.log" | tr ' ' '\n' | sort | tr '\n' ' ')" = "ompi-b=2 pmix-a=4 "
if command -v jq > /dev/null 2>&1; then
  mc_build() { # mc_build <cargo stub body> -> exit code of mc_build_artifacts
    mkdir -p "$tmp/mclog"
    bash -c 'logdir="$1"; . "$2/memcheck/lib.sh"; cargo() { '"$1"'; }; mc_build_artifacts t -p demo > /dev/null' sh "$tmp/mclog" "$here"
  }
  art='{"reason":"compiler-artifact","profile":{"test":true},"executable":"/t/demo","target":{"name":"demo"}}'
  check "memcheck build: a listed test executable passes" mc_build "echo '$art'"
  check "memcheck build: a build that exits non-zero fails" exit_code 1 mc_build "echo '$art'; return 101"
  check "memcheck build: an empty executable list fails (a renamed target must not pass)" exit_code 1 mc_build "echo '{\"reason\":\"build-finished\"}'"
else
  skip "memcheck build checks (jq not installed)"
fi
printf '{"packages":[{"name":"a","version":"1.2.3"},{"name":"b","version":"1.2.3"}]}' > "$tmp/meta.json"
printf '{"packages":[{"name":"a","version":"1.2.3"},{"name":"b","version":"1.2.4"}]}' > "$tmp/meta-mixed.json"
version_check_ok() { python3 -I "$here/tools/version_check.py" "$@" > /dev/null; }
check "version_check.py: tag v1.2.3 names the version" version_check_ok "$tmp/meta.json" --tag v1.2.3
check "version_check.py: a tag that does not name the version fails" exit_code 1 python3 -I "$here/tools/version_check.py" "$tmp/meta.json" --tag v1.2.4
check "version_check.py: members on different versions fail" exit_code 1 python3 -I "$here/tools/version_check.py" "$tmp/meta-mixed.json"
check "miri tier applies the test-count floors" grep -q 'tools/count_tests.py' "$here/tiers/miri.sh"
check "test tier applies the analyser" grep -q analyze_tests.py "$here/tiers/test.sh"
check "test tier applies the floors" grep -q tools/count_tests.py "$here/tiers/test.sh"
check "release tier applies the analyser and the floors" grep -q tools/count_tests.py "$here/tiers/release.sh"

echo "selftest: $fails failure(s), $skips skipped check(s)"
if [ "$fails" != 0 ]; then exit 1; fi
if [ "$skips" != 0 ]; then
  if [ "${CI_SELFTEST_ALLOW_SKIP:-0}" = 1 ]; then
    echo "selftest: $skips check(s) skipped (CI_SELFTEST_ALLOW_SKIP=1: not a full pass)"
    exit 0
  fi
  echo "selftest: INCOMPLETE: a skipped check is never a pass (install the missing tool, or set CI_SELFTEST_ALLOW_SKIP=1 on a developer machine)"
  exit 3
fi
exit 0
