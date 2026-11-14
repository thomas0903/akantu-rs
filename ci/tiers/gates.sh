#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-003, CI-018, CI-026: feature-gated test targets).
# gates: every command of ci/tools/feature-gates.txt (feature-gated clippy / test / check lines), one step each
#
# The merge gate of the port keeps the feature-gated cargo commands in
# ci/tools/feature-gates.txt (targets that no workspace member enables by
# default: IGFEM + cgal, the umbrella's cgal and igfem lines, the release-only
# cohesive tests, the python feature's clippy and Rust tests, the facade's
# extra-materials-non-local, the spatial convergence study in release). This
# tier runs every line in the project's MPI image (CPython headers for PyO3,
# MPI; none of the lines needs MUMPS or Scotch). Each line must start with
# `cargo `, contain `--locked` (and `--no-fail-fast` for `cargo test`) and use
# only plain words (the format of ci/tools/feature-gates.txt); a line that does not
# fails its step. ci/tools/gate_map.py and ci/selftest.sh check that every
# line has a workflow job (the table in ci/github/README.md).
#
# JUDGEMENT: a `cargo test` line is judged like the `test` tier:
# `cargo metadata`, the line's own `--no-run --message-format=json` build
# (step feature-N-build), the run (step feature-N, exit 0 or 101 accepted: the
# analyser decides), then ci/tools/analyze_tests.py in a job directory of
# the line (line-N/: it reads features.tsv, feature-N.log,
# feature-N-artifacts.jsonl; metadata and baseline scoped to the line's
# packages / `--test` targets by ci/tools/release_scope.py) against the
# ONE waiver list ci/tools/known-failures.txt: every test binary and every
# doc-test crate of the selected packages must have run and reported, so a
# binary that exits 0 early (the UD-018 shape) fails the tier. A floor key per
# line (`gates-N` in ci/floors.txt, ci/tools/count_tests.py) catches a line
# whose selection matches nothing. The analyser is required: the tier fails
# without ci/tools/analyze_tests.py + known-failures.txt.
# Clippy and check lines are judged by their exit code (nothing else to count).
#
# A line is skipped here only when the `release` tier runs the identical
# command under a stronger judgement (analyser + floor, COH_ALL=1): the
# facade's release run, `cargo test --release -p akantu` (the examples'
# whole-run comparisons). The line is DERIVED from ci/tiers/release.sh (its
# `release_run facade` selection), not written here a second time, so a
# narrowed facade selection there makes this tier run the gate line itself;
# workflows run that `release` leg before a merge (features.yml).
# Image: the default image of the backend (akantu-rs-mpi:dev); CI_GATES_IMAGE
# overrides it.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
if [ "$CI_BACKEND" = docker ]; then
  CI_IMAGE="${CI_GATES_IMAGE:-$CI_IMAGE}"
  CI_TARGET_NAME="${CI_TARGET_NAME:-ci}-gates"
fi
ci_tier_begin gates
list="${CI_GATES_FILE:-$here/tools/feature-gates.txt}"
tooling="${AKANTU_CI_ANALYSER_DIR:-$here/tools}"
# The command the release tier runs as its `facade` run, as a sorted word set
# (cargo test --release --locked --no-fail-fast <selection of release_run facade>).
sorted_words() { printf '%s\n' "$@" | LC_ALL=C sort | tr '\n' ' '; }
facade_sel="$(sed -n 's/^release_run facade [^ ]* //p' "$here/tiers/release.sh")"
# shellcheck disable=SC2086 # the selection is a plain list of cargo words
facade_set="$(sorted_words cargo test --release --locked --no-fail-fast $facade_sel)"
if [ ! -f "$list" ]; then
  ci_fail gates "no $list"
  ci_tier_end
  exit $?
fi
if ! ci_backend_ready; then
  ci_skip gates "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
  ci_tier_end
  exit $?
fi
if [ ! -f "$tooling/analyze_tests.py" ] || [ ! -f "$tooling/known-failures.txt" ]; then
  ci_fail analyze "no $tooling/analyze_tests.py + known-failures.txt: without the analyser an early-exit test binary would pass on cargo's exit code alone; the gates tier needs both files"
  ci_tier_end
  exit $?
fi
if ! ci_have python3; then
  ci_skip analyze "python3 is needed on the host for the analyser"
  ci_tier_end
  exit $?
fi
kf="$tooling/known-failures.txt"
sha="$( (sha256sum < "$kf" || shasum -a 256 < "$kf") 2> /dev/null)"
echo "baseline=$kf sha256=${sha%% *}" > "$(ci_file baseline.txt)"
: > "$(ci_file features.tsv)"
ci_step metadata --stdout "$(ci_file metadata.json)" -- cargo metadata --no-deps --format-version 1 --locked
n=0
tests=()
while IFS= read -r line; do
  case "$line" in '' | '#'*) continue ;; esac
  n=$((n + 1))
  # A plain list of cargo words without quoting (feature-gates.txt's format).
  # shellcheck disable=SC2206
  words=($line)
  if [ "${words[0]}" != cargo ]; then
    ci_fail "feature-$n" "not a cargo command: $line"
    continue
  fi
  if ! [[ "$line" =~ ^cargo(\ [A-Za-z0-9_.,:=/@+-]+)+$ ]]; then
    ci_fail "feature-$n" "not a plain cargo command (one space between words, no quotes or shell syntax): $line"
    continue
  fi
  case " $line " in *" --locked "*) ;; *)
    ci_fail "feature-$n" "no --locked: $line"
    continue
    ;;
  esac
  if [ "${words[1]}" = test ]; then
    case " $line " in *" --no-fail-fast "*) ;; *)
      ci_fail "feature-$n" "a cargo test line without --no-fail-fast: $line"
      continue
      ;;
    esac
  fi
  if [ "$(sorted_words "${words[@]}")" = "$facade_set" ]; then
    ci_na "feature-$n" "run by the release tier (run facade, analyser + floor): $line"
    continue
  fi
  if [ "${words[1]}" = test ]; then
    # build words: everything before a `--` (cargo test's own arguments follow it)
    build=()
    for w in "${words[@]}"; do
      [ "$w" != -- ] || break
      build+=("$w")
    done
    printf '%s\ttest\t%s\n' "$n" "$line" >> "$(ci_file features.tsv)"
    ci_step "feature-$n-build" --stdout "$(ci_file "feature-$n-artifacts.jsonl")" -- "${build[@]}" --no-run --message-format=json
    # 101 = test failures: accepted here only because the analyser judges them
    # against the reviewed baseline (anything else fails there).
    ci_step "feature-$n" --ok-exit "0 101" -- "${words[@]}"
    tests+=("$n")
  else
    printf '%s\tcmd\t%s\n' "$n" "$line" >> "$(ci_file features.tsv)"
    ci_step "feature-$n" -- "${words[@]}"
  fi
done < "$list"
[ "$n" -gt 0 ] || ci_fail gates "$list holds no command"
if [ "${#tests[@]}" -gt 0 ]; then
  if [ "$CI_DRY_RUN" = 1 ]; then
    ci_skip analyze "dry run"
    ci_skip floor "dry run"
  else
    # One analyser run per `cargo test` line, in a job directory of its own
    # whose metadata and baseline are scoped to the line's selection
    # (ci/tools/release_scope.py, the release tier's mechanism): a line that
    # selects `--test NAME` of a package runs no doc tests of it, which the
    # workspace-wide metadata would still demand (the line-10 shape:
    # `cargo test --release -p akantu-solid ... --test spatial_convergence`).
    for t in "${tests[@]}"; do
      d="$(ci_file "line-$t")"
      mkdir -p "$d"
      line="$(awk -F '\t' -v n="$t" '$1 == n { print $3 }' "$(ci_file features.tsv)")"
      # shellcheck disable=SC2206
      lw=($line)
      pkgs=() tsel=()
      i=0
      while [ "$i" -lt "${#lw[@]}" ]; do
        case "${lw[$i]}" in
          -p | --package) pkgs+=("${lw[$((i + 1))]:-}") ;;
          --test) tsel+=("${lw[$((i + 1))]:-}") ;;
          --) break ;;
        esac
        i=$((i + 1))
      done
      spec=()
      if [ "${#pkgs[@]}" = 1 ] && [ "${#tsel[@]}" -gt 0 ]; then
        spec=("${pkgs[0]}:test=$(IFS=';'; echo "${tsel[*]}")")
      elif [ "${#tsel[@]}" = 0 ] && [ "${#pkgs[@]}" -gt 0 ]; then
        spec=("${pkgs[@]}")
      fi
      if [ "${#spec[@]}" = 0 ]; then
        ci_fail "feature-$t:scope" "cannot derive the selection of '$line' (want -p PKG [--test NAME]... or -p PKG...)"
        continue
      fi
      ci_step "feature-$t:scope" --host -- python3 -I "$here/tools/release_scope.py" --metadata "$(ci_file metadata.json)" --known-failures "$kf" --out "$d" "${spec[@]//;/,}"
      cp "$(ci_file "feature-$t.log")" "$d/feature-$t.log" 2> /dev/null || : > "$d/feature-$t.log"
      cp "$(ci_file "feature-$t-artifacts.jsonl")" "$d/feature-$t-artifacts.jsonl" 2> /dev/null || : > "$d/feature-$t-artifacts.jsonl"
      printf '%s\ttest\t%s\n' "$t" "$line" > "$d/features.tsv"
      awk -F '\t' -v a="feature-$t-build" -v b="feature-$t" 'BEGIN { OFS = "\t" } ($1 == a || $1 == b) && ($2 == "pass" || $2 == "fail") { print $1, $3, $4 }' "$(ci_file steps.tsv)" > "$d/summary.tsv"
      : > "$d/checks.txt"
      ci_step "feature-$t:analyze" --host -- python3 "$tooling/analyze_tests.py" --job-dir "$d" --repo "$CI_REPO" --known-failures "$d/known-failures.txt"
      ci_step "feature-$t:floor" --host -- python3 -I "$here/tools/count_tests.py" "$(ci_file "feature-$t.log")" --floors "$here/floors.txt" --key "gates-$t"
    done
  fi
fi
ci_tier_end
