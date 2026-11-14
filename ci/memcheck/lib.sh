#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-010, CI-020):
# the judging helpers of ci/memcheck/run-in-image.sh, in a file
# of their own so that ci/selftest.sh can exercise them without valgrind or
# Docker. Sourced, never executed. Bash 3.2 compatible.
#
#   mc_summaries <log>                    number of processes that printed an "ERROR SUMMARY:"
#   mc_errors <log>                       sum of their error counts
#   mc_suppressions <log>                 "name=count ..." of the used suppressions (valgrind -v)
#   mc_judge <ranks> <rc> <log> <want>    prints "<PASS|FAIL> <errors or reason>"; the rules:
#       * exit code 0, and valgrind's own error exit code (99) is a failure;
#       * exactly one ERROR SUMMARY per rank (a rank whose valgrind died
#         before its summary is caught even if mpirun exits 0);
#       * no memcheck error (the suppression file's entries are not counted);
#       * the program's own result line matches <want> (a regex, or - for none)
#   mc_build_artifacts <label> <cargo test args...>
#       the test executables of a --no-run build into MC_ARTS ("<target> <path>"
#       lines); returns 1 and prints the reason when the build exits non-zero or
#       lists no test executable (an empty list must never mean "nothing to
#       run": a renamed target or a failed build would pass silently).
#       Needs $logdir (build logs go there).

# One summary per PROCESS: with `-v` valgrind prints the "ERROR SUMMARY" line
# twice (before and after its used-suppressions list), so lines are keyed by the
# `==<pid>==` prefix and the first line of a pid counts; a line without a
# prefix counts on its own (a 2-rank run prints 4 summaries).
_mc_summary_lines() {
  grep -a 'ERROR SUMMARY:' "$1" 2> /dev/null \
    | awk '{ if (match($0, /^==[0-9]+==/)) k = substr($0, RSTART, RLENGTH); else k = "line" NR; if (!(k in seen)) { seen[k] = 1; print } }'
}
mc_summaries() { _mc_summary_lines "$1" | wc -l | tr -d ' '; }

mc_errors() {
  _mc_summary_lines "$1" | sed -E 's/.*ERROR SUMMARY: ([0-9]+) errors.*/\1/' | awk '{ s += $1 } END { print s + 0 }'
}

# valgrind -v: "--123-- used_suppression:  3 <name> <file>:<line>", once per process
mc_suppressions() {
  grep -a 'used_suppression:' "$1" 2> /dev/null | sed -E 's/.*used_suppression: *([0-9]+) ([^ ]+).*/\2 \1/' \
    | awk '{ c[$1] += $2 } END { for (n in c) printf "%s=%d ", n, c[n] }' | sed 's/ $//'
}

mc_judge() {
  local ranks="$1" rc="$2" log="$3" want="$4" n errs
  n="$(mc_summaries "$log")"
  errs="$(mc_errors "$log")"
  if [ "$n" = 0 ]; then
    echo "FAIL no-valgrind-summary"
    return
  fi
  if [ "$n" != "$ranks" ]; then
    echo "FAIL $n-summaries-for-$ranks-ranks"
    return
  fi
  if [ "$errs" != 0 ]; then
    echo "FAIL $errs"
    return
  fi
  if [ "$rc" != 0 ]; then
    echo "FAIL $errs"
    return
  fi
  if [ "$want" != - ] && ! grep -aqE "$want" "$log"; then
    echo "FAIL $errs"
    return
  fi
  echo "PASS $errs"
}

MC_ARTS=""
mc_build_artifacts() {
  # $logdir is set by the sourcing script (run-in-image.sh or the self-test)
  # shellcheck disable=SC2154
  local rc json="$logdir/build-$1.json" err="$logdir/build-$1.log"
  shift
  MC_ARTS=""
  cargo test -q --locked --no-run --message-format=json "$@" > "$json" 2> "$err"
  rc=$?
  if [ "$rc" != 0 ]; then
    echo "cargo test --no-run $* exited $rc (see $err)"
    return 1
  fi
  MC_ARTS="$(jq -r 'select(.reason=="compiler-artifact" and .profile.test==true and .executable!=null) | .target.name + " " + .executable' "$json" 2> /dev/null)"
  if [ -z "$MC_ARTS" ]; then
    echo "cargo test --no-run $* listed no test executable (see $json)"
    return 1
  fi
  return 0
}
