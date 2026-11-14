#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; reproducers
# F4s, F6s/F6a/F6f, F7g0, F8, F9, F10 turned into self-test cases):
# "a step that did not run, or did not test anything, never counts as a
# pass". Fault injection into the ci/ tooling of THIS tree on SYNTHETIC
# trees (tiny path-only cargo workspaces, a few seconds of compilation, no
# Docker, no network, nothing outside the scratch directory).
#
#   ci/fault_injection.sh [<scratch dir>]
#
# Prints one line per case:
#   <id> want=<verdict> got=<last line of run-all.sh> exit=<code> <OK|MISSED|UNEXPECTED>
# MISSED = the injected fault still produced `CI: PASS`. Exit 0 when every
# injection was detected as expected, 1 otherwise, 3 when cargo is missing
# (the cases cannot run). Cases:
#   F0   control: a healthy synthetic tree passes the test tier
#   F3a  a test binary that aborts (analyser present)
#   F4s  a test binary that calls exit(0) early, tree WITHOUT the analyser
#   F4a  the same with the analyser
#   F6s  no test at all, tree without the analyser
#   F6a  no test at all, analyser present (the floor step catches it)
#   F6f  `cargo test <filter matching nothing>` exits 0: count_tests.py fails it
#   F7g0 a corrupted golden file in a tree without .git
#   F7g1 the same inside a git checkout (control)
#   F8   an empty tier list
#   F9   a reused --out directory and a tier script that dies before it
#        writes a status
#   F10  an unknown tier status
#   F11  a tier directory with every step n/a (nothing ran)
#   F12  the `gates` tier (feature-gates.txt lines): control, a test binary
#        that exits 0 early inside a `cargo test` line, a line
#        whose selection matches nothing (the floor), a line that fails, a
#        `--test NAME` line (no doc tests expected), a line that breaks the
#        format of ci/tools/feature-gates.txt (no --locked)
set -u
here="$(cd "$(dirname "$0")" && pwd)"
command -v cargo > /dev/null 2>&1 || {
  echo "fault_injection: cargo is not installed: the cases cannot run" >&2
  exit 3
}
W="${1:-$(mktemp -d "${TMPDIR:-/tmp}/ci-fault.XXXXXX")}"
mkdir -p "$W"
W="$(cd "$W" && pwd)"
export CARGO_TARGET_DIR="$W/target" CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
unset CI_OUT CI_REPO CI_BACKEND CI_IMAGE CI_STEP_TIMEOUT CI_DRY_RUN AKANTU_CI_ANALYSER_DIR CI_FLOORS
bad=0

# new_repo <name> <git:0|1> <analyser:0|1>: synthetic repository with this ci/
new_repo() {
  local r="$W/$1"
  rm -rf "$r" && mkdir -p "$r/crates/demo/src" "$r/crates/demo/tests" "$r/docs"
  cp -R "$here" "$r/ci"
  rm -rf "$r/ci/__pycache__" "$r/ci/tools/__pycache__"
  : > "$r/ci/floors.txt" # built-in floors only (at least one test)
  cat > "$r/Cargo.toml" << 'TOML'
[workspace]
resolver = "3"
members = ["crates/*"]
TOML
  cat > "$r/crates/demo/Cargo.toml" << 'TOML'
[package]
name = "demo"
version = "0.1.0"
edition = "2024"
license = "LGPL-3.0-or-later"
publish = false
TOML
  printf '// SPDX-License-Identifier: LGPL-3.0-or-later\n//! demo\n/// add\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n' > "$r/crates/demo/src/lib.rs"
  printf '// SPDX-License-Identifier: LGPL-3.0-or-later\n#[test]\nfn adds() {\n    assert_eq!(demo::add(1, 2), 3);\n}\n' > "$r/crates/demo/tests/basic.rs"
  (cd "$r" && cargo generate-lockfile --offline -q) || echo "fault_injection: cargo generate-lockfile failed in $r" >&2
  if [ "$3" = 1 ]; then
    printf '# empty baseline (ci/fault_injection.sh)\n' > "$r/ci/tools/known-failures.txt"
  else
    rm -f "$r/ci/tools/analyze_tests.py" "$r/ci/tools/known-failures.txt"
  fi
  if [ "$2" = 1 ]; then
    (cd "$r" && git init -q && git add -A && git -c user.name=ci -c user.email=ci@invalid commit -qm synthetic)
  fi
  echo "$r"
}
commit_all() { (cd "$1" && git add -A && git -c user.name=ci -c user.email=ci@invalid commit -qm change); }

# report <id> <want> <got line> <exit>: want is PASS, FAIL, INCOMPLETE or USAGE
report() {
  local id="$1" want="$2" line="$3" rc="$4" verdict mark=OK
  verdict="$(printf '%s' "$line" | sed -n 's/^CI: \([A-Z]*\).*/\1/p')"
  [ "$rc" = 2 ] && verdict=USAGE
  if [ "$verdict" = PASS ] && [ "$want" != PASS ]; then
    mark=MISSED
  elif [ "$verdict" != "$want" ]; then
    mark="UNEXPECTED"
  fi
  [ "$mark" = OK ] || bad=$((bad + 1))
  printf '%-5s want=%-10s got=%-52s exit=%s %s\n' "$id" "$want" "\"$line\"" "$rc" "$mark"
}

# run <id> <want> <repo> <run-all args...>
run() {
  local id="$1" want="$2" r="$3" out rc
  shift 3
  out="$W/out-$id"
  "$r/ci/run-all.sh" --out "$out" "$@" > "$W/$id.log" 2>&1
  rc=$?
  report "$id" "$want" "$(tail -n 1 "$W/$id.log")" "$rc"
}

# F0 control
r=$(new_repo f0 1 1)
run F0 PASS "$r" --tiers test

# F3a a test binary that aborts (analyser present)
r=$(new_repo f3a 1 1)
printf '// SPDX-License-Identifier: LGPL-3.0-or-later\n#[test]\nfn crashes() {\n    std::process::abort();\n}\n' > "$r/crates/demo/tests/crash.rs"
commit_all "$r"
run F3a FAIL "$r" --tiers test

# F4 a test binary that exits 0 early (the UD-018 shape): the second test
# never runs and the binary prints no libtest result line.
early='// SPDX-License-Identifier: LGPL-3.0-or-later\n#[test]\nfn a_exits_early() {\n    std::process::exit(0);\n}\n#[test]\nfn b_would_fail() {\n    std::thread::sleep(std::time::Duration::from_millis(500));\n    panic!("never reached");\n}\n'
r=$(new_repo f4s 1 0)
# shellcheck disable=SC2059
printf "$early" > "$r/crates/demo/tests/early.rs"
commit_all "$r"
run F4s FAIL "$r" --tiers test
r=$(new_repo f4a 1 1)
# shellcheck disable=SC2059
printf "$early" > "$r/crates/demo/tests/early.rs"
commit_all "$r"
run F4a FAIL "$r" --tiers test

# F6 no test at all; and a filter that matches nothing
r=$(new_repo f6s 1 0)
rm -f "$r/crates/demo/tests/basic.rs"
commit_all "$r"
run F6s FAIL "$r" --tiers test
r=$(new_repo f6a 1 1)
rm -f "$r/crates/demo/tests/basic.rs"
commit_all "$r"
run F6a FAIL "$r" --tiers test
(cd "$r" && cargo test -q --lib no_such_test_filter > "$W/F6f.log" 2>&1)
rc=$?
python3 -I "$here/tools/count_tests.py" "$W/F6f.log" --key F6f > "$W/F6f.count" 2>&1
crc=$?
if [ "$rc" = 0 ] && [ "$crc" = 1 ]; then
  printf '%-5s want=%-10s got=%-52s exit=%s %s\n' F6f FAIL "\"cargo exit 0, count_tests exit 1: $(head -c 60 "$W/F6f.count")...\"" "$crc" OK
else
  bad=$((bad + 1))
  printf '%-5s want=%-10s got=%-52s exit=%s %s\n' F6f FAIL "\"cargo exit $rc, count_tests exit $crc\"" "$crc" "MISSED/UNEXPECTED"
fi

# F7 fixtures: a corrupted golden file in an exported tree (no .git) and,
# as the control, in a git checkout
for g in 0 1; do
  r=$(new_repo "f7g$g" 0 0)
  mkdir -p "$r/fixtures/case"
  printf 'golden\n' > "$r/fixtures/case/value.txt"
  (cd "$r/fixtures" && { sha256sum case/value.txt 2> /dev/null || shasum -a 256 case/value.txt; } > SHA256SUMS)
  printf 'CORRUPTED\n' > "$r/fixtures/case/value.txt"
  if [ "$g" = 1 ]; then (cd "$r" && git init -q && git add -A && git -c user.name=ci -c user.email=ci@invalid commit -qm s); fi
  run "F7g$g" FAIL "$r" --tiers fixtures
done

# F8 an empty tier list is a usage error, never a pass
r=$(new_repo f8 1 0)
run F8 USAGE "$r" --tiers ,

# F9 a reused --out directory and a tier that dies before it writes a status
r=$(new_repo f9 1 0)
mkdir -p "$r/fixtures"
printf 'x\n' > "$r/fixtures/a.txt"
(cd "$r/fixtures" && { sha256sum a.txt 2> /dev/null || shasum -a 256 a.txt; } > SHA256SUMS)
commit_all "$r"
"$r/ci/run-all.sh" --out "$W/out-F9" --tiers fixtures > "$W/F9-first.log" 2>&1 # first run: pass
printf 'CORRUPTED\n' > "$r/fixtures/a.txt"
# the tier script now fails to parse, so it never reaches ci_tier_begin
{ printf '#!/usr/bin/env bash\nif then\n'; cat "$r/ci/tiers/fixtures.sh"; } > "$r/ci/tiers/fixtures.sh.new" && mv "$r/ci/tiers/fixtures.sh.new" "$r/ci/tiers/fixtures.sh"
commit_all "$r"
run F9 FAIL "$r" --tiers fixtures # same --out as the first run
# (run() above uses $W/out-F9 too: the directory is the reused one)

# F10 an unknown tier status is not a pass
d="$W/out-F10"
rm -rf "$d" && mkdir -p "$d/t"
printf 'schema=akantu-rs-ci-summary/1\n' > "$d/provenance.txt"
printf 'skipped\n' > "$d/t/status"
printf 'x\tpass\t0\t1\t\n' > "$d/t/steps.tsv"
printf 't\tskipped\t1\t\n' > "$d/summary.tsv"
python3 -I "$here/tools/summarize.py" "$d" > "$W/F10.log" 2>&1
rc=$?
report F10 FAIL "$(tail -n 1 "$W/F10.log")" "$rc"

# F11 every step n/a: nothing ran
r=$(new_repo f11 1 0)
run F11 INCOMPLETE "$r" --tiers fixtures # a tree without fixtures/: n/a

# F12 the `gates` tier judges its `cargo test` lines like the test tier
# (analyser + floor per line), never by cargo's exit code alone.
gates_repo() { # gates_repo <name> <line of feature-gates.txt>
  local r
  r=$(new_repo "$1" 1 1)
  printf '# synthetic gate list (ci/fault_injection.sh)\n%s\n' "$2" > "$r/ci/tools/feature-gates.txt"
  echo "$r"
}
r=$(gates_repo f12 "cargo test -p demo --locked --no-fail-fast")
commit_all "$r"
run F12 PASS "$r" --tiers gates
# a line that selects one `--test` target runs no doc tests (feature-gates.txt
# line 10 shape): scoped, it passes
r=$(gates_repo f12t "cargo test -p demo --locked --no-fail-fast --test basic")
commit_all "$r"
run F12t PASS "$r" --tiers gates
r=$(gates_repo f12e "cargo test -p demo --locked --no-fail-fast")
# shellcheck disable=SC2059
printf "$early" > "$r/crates/demo/tests/early.rs"
commit_all "$r"
run F12e FAIL "$r" --tiers gates
r=$(gates_repo f12n "cargo test -p demo --locked --no-fail-fast --test no_such_target")
commit_all "$r"
run F12n FAIL "$r" --tiers gates
r=$(gates_repo f12c "cargo check -p demo --features no_such_feature --locked")
commit_all "$r"
run F12c FAIL "$r" --tiers gates
# a line without --locked breaks the format of ci/tools/feature-gates.txt
r=$(gates_repo f12l "cargo check -p demo")
commit_all "$r"
run F12l FAIL "$r" --tiers gates

echo "fault_injection: $bad case(s) not detected as expected (scratch $W)"
[ "$bad" = 0 ]
