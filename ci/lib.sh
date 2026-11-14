#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; replaces the role of upstream's
# cmake/akantu_test_driver.sh and .gitlab/ci.d/templates.yaml job scripts,
# parity rows CI-006, CI-019, CI-020): shared helpers of the tier scripts in
# ci/tiers/. Sourced, never executed. Bash 3.2 compatible (macOS /bin/bash).
#
# A tier script calls, in this order:
#   ci_tier_begin <tier>
#   ci_step <name> [--host] [--stdout FILE] [--timeout S] [--ok-exit "0 N"] -- <command...>
#   ci_skip <name> <reason>      # prerequisite missing: the tier is incomplete
#   ci_na <name> <reason>        # does not apply to this tree: ignored
#   ci_tier_end                  # writes <tier dir>/status, returns 0/1/3
#
# Environment (set by ci/run-all.sh; defaults allow running a tier alone):
#   CI_REPO         repository root (default: parent of ci/)
#   CI_OUT          output root (default: $CI_REPO/target/ci/manual)
#   CI_BACKEND      host | docker (default host). docker runs every non-host
#                   step through scripts/mpi/in-docker.sh (repository
#                   mounted read-only at /src, cargo target in a volume).
#   CI_IMAGE        image for docker steps (default akantu-rs-mpi:dev)
#   CI_TARGET_NAME  docker: cargo target /target/<name> (default ci);
#                   host: CARGO_TARGET_DIR is left as the caller set it
#   CI_STEP_TIMEOUT seconds per step (default 3600; needs `timeout`)
#   CI_DRY_RUN=1    record the commands without running them
#
# Files of a tier directory <CI_OUT>/<tier>/: steps.tsv (step, status, exit
# code, seconds, reason), commands.tsv (step, backend, exact command), one
# <step>.log per step, status (pass | fail | incomplete | na). The step
# status is pass (exit 0), fail (any other exit code, or a timeout), skip
# (not run: a prerequisite is missing) or na (not applicable to this tree).
# A tier is `fail` if a step failed, else `incomplete` if a step was
# skipped, else `pass` (`na` if every step was na). Nothing is ever waived.

CI_REPO="${CI_REPO:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
CI_OUT="${CI_OUT:-$CI_REPO/target/ci/manual}"
CI_BACKEND="${CI_BACKEND:-host}"
CI_IMAGE="${CI_IMAGE:-akantu-rs-mpi:dev}"
CI_TARGET_NAME="${CI_TARGET_NAME:-ci}"
CI_STEP_TIMEOUT="${CI_STEP_TIMEOUT:-3600}"
CI_DRY_RUN="${CI_DRY_RUN:-0}"
export CI_REPO CI_OUT CI_BACKEND CI_IMAGE CI_TARGET_NAME CI_STEP_TIMEOUT CI_DRY_RUN

ci_log() { printf '[ci %s] %s\n' "$(date -u +%H:%M:%SZ)" "$*" >&2; }
ci_die() {
  ci_log "error: $*"
  exit 2
}

# Quote one word for a shell (used only for the recorded command lines).
ci_shq() {
  case "$1" in
    '' | *[!A-Za-z0-9_./:=,+@%-]*) printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\\\\''/g")" ;;
    *) printf '%s' "$1" ;;
  esac
}
ci_join() {
  local out="" w
  for w in "$@"; do out="$out${out:+ }$(ci_shq "$w")"; done
  printf '%s' "$out"
}

# File name of a step log: anything but [A-Za-z0-9._-] becomes '_'.
ci_slug() { printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_'; }

ci_have() { command -v "$1" > /dev/null 2>&1; }

# Is the backend able to run cargo? (docker: image present; host: cargo)
ci_backend_ready() {
  case "$CI_BACKEND" in
    host) ci_have cargo ;;
    docker) ci_have docker && docker image inspect "$CI_IMAGE" > /dev/null 2>&1 ;;
    *) return 1 ;;
  esac
}

ci_tier_begin() {
  CI_TIER="$1"
  CI_TIER_DIR="$CI_OUT/$CI_TIER"
  mkdir -p "$CI_TIER_DIR" || ci_die "cannot create $CI_TIER_DIR"
  : > "$CI_TIER_DIR/steps.tsv"
  : > "$CI_TIER_DIR/commands.tsv"
  rm -f "$CI_TIER_DIR/status"
  date -u +%Y-%m-%dT%H:%M:%SZ > "$CI_TIER_DIR/started"
  CI_TIER_FAILED=0 CI_TIER_SKIPPED=0 CI_TIER_RAN=0
  export CI_TIER CI_TIER_DIR
  ci_log "tier $CI_TIER: backend=$CI_BACKEND out=$CI_TIER_DIR"
}

_ci_record() { # name status rc seconds reason
  printf '%s\t%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" "$5" >> "$CI_TIER_DIR/steps.tsv"
}

ci_skip() {
  _ci_record "$1" skip - 0 "$2"
  CI_TIER_SKIPPED=1
  ci_log "  skip $1: $2"
}

ci_na() {
  _ci_record "$1" na - 0 "$2"
  ci_log "  n/a  $1: $2"
}

# A check that cannot be satisfied although its inputs exist (a required
# tool of the tree is missing, a count is below its floor): a failure, not
# an incomplete tier.
ci_fail() {
  _ci_record "$1" fail - 0 "$2"
  CI_TIER_FAILED=1 CI_TIER_RAN=1
  ci_log "  fail $1: $2"
}

# ci_step <name> [--host] [--stdout FILE] [--timeout S] -- <command...>
#   --host     run on the host even with the docker backend (tools that are
#              not in the image or must see host paths: shellcheck, python
#              analysis of captured files, checksums)
#   --stdout F write the command's stdout to F (stderr goes to the log);
#              otherwise stdout and stderr are merged into <name>.log (for
#              docker steps the merge happens inside the container so that
#              cargo's and libtest's lines keep their order)
#   --ok-exit L exit codes (space-separated) that count as `pass`; only for
#              a step whose output a LATER step judges (cargo test exits 101
#              on test failures; the analyser step then decides). Default 0.
ci_step() {
  local name="$1" host=0 out="" tmo="$CI_STEP_TIMEOUT" ok="0" log rc t0 secs status reason=""
  shift
  while [ $# -gt 0 ]; do
    case "$1" in
      --host)
        host=1
        shift
        ;;
      --stdout)
        out="$2"
        shift 2
        ;;
      --timeout)
        tmo="$2"
        shift 2
        ;;
      --ok-exit)
        ok="$2"
        shift 2
        ;;
      --)
        shift
        break
        ;;
      *) ci_die "ci_step $name: unknown option $1" ;;
    esac
  done
  [ $# -gt 0 ] || ci_die "ci_step $name: no command"
  log="$CI_TIER_DIR/$(ci_slug "$name").log"
  local backend="$CI_BACKEND"
  [ "$host" = 1 ] && backend=host
  local -a cmd
  # The container of THIS step has a name of its own (in-docker.sh honours
  # AKANTU_RS_CONTAINER_NAME), so that a timeout kills only it and never the
  # container of a concurrent job on the same host.
  local cname
  cname="akantu-rs-mpi-ci-$$-$(ci_slug "$name")-$(date +%s)"
  if [ "$backend" = docker ]; then
    if [ -n "$out" ]; then
      cmd=(env "AKANTU_RS_MPI_IMAGE=$CI_IMAGE" "AKANTU_RS_MPI_TARGET=/target/$CI_TARGET_NAME" "AKANTU_RS_CONTAINER_NAME=$cname"
        "$CI_REPO/scripts/mpi/in-docker.sh" "$@")
    else
      cmd=(env "AKANTU_RS_MPI_IMAGE=$CI_IMAGE" "AKANTU_RS_MPI_TARGET=/target/$CI_TARGET_NAME" "AKANTU_RS_CONTAINER_NAME=$cname"
        "$CI_REPO/scripts/mpi/in-docker.sh" sh -c 'exec "$@" 2>&1' sh "$@")
    fi
  else
    cmd=("$@")
  fi
  printf '%s\t%s\t%s\n' "$name" "$backend" "$(ci_join "$@")${out:+ > ${out##*/}}" >> "$CI_TIER_DIR/commands.tsv"
  if [ "$CI_DRY_RUN" = 1 ]; then
    _ci_record "$name" skip - 0 "dry run"
    CI_TIER_SKIPPED=1
    ci_log "  dry  $name: $(ci_join "${cmd[@]}")"
    return 0
  fi
  ci_log "  run  $name"
  t0=$(date +%s)
  local -a tcmd=()
  if ci_have timeout; then tcmd=(timeout -k 20 "$tmo"); fi
  (
    cd "$CI_REPO" || exit 127
    if [ -n "$out" ]; then
      ${tcmd[@]+"${tcmd[@]}"} "${cmd[@]}" > "$out" 2> "$log" < /dev/null
    else
      ${tcmd[@]+"${tcmd[@]}"} "${cmd[@]}" > "$log" 2>&1 < /dev/null
    fi
  )
  rc=$?
  secs=$(($(date +%s) - t0))
  case " $ok " in
    *" $rc "*) status=pass ;;
    *) status=fail ;;
  esac
  [ "$rc" = 0 ] || [ "$status" = fail ] || reason="exit $rc (accepted: judged by a later step)"
  if [ "$status" = fail ]; then
    CI_TIER_FAILED=1
    if [ "$rc" = 124 ] || [ "$rc" = 137 ]; then
      reason="timed out or killed after ${secs} s (limit $tmo s)"
      # cargo runs as PID 1 in the container and ignores TERM: a timed-out
      # docker step leaves its container behind.
      # Kill exactly the container this step started.
      if [ "$backend" = docker ]; then
        docker kill "$cname" > /dev/null 2>&1 || true
      fi
    else
      reason="exit $rc"
    fi
    printf '\n[ci] step %s: %s\n' "$name" "$reason" >> "$log"
  fi
  CI_TIER_RAN=1
  _ci_record "$name" "$status" "$rc" "$secs" "$reason"
  ci_log "  $status $name (exit $rc, ${secs} s)"
  return 0
}

ci_tier_end() {
  local status
  if [ "$CI_TIER_FAILED" = 1 ]; then
    status=fail
  elif [ "$CI_TIER_SKIPPED" = 1 ]; then
    status=incomplete
  elif [ "$CI_TIER_RAN" = 1 ]; then
    status=pass
  else
    status=na
  fi
  printf '%s\n' "$status" > "$CI_TIER_DIR/status"
  date -u +%Y-%m-%dT%H:%M:%SZ > "$CI_TIER_DIR/finished"
  ci_log "tier $CI_TIER: $status"
  case "$status" in
    pass | na) return 0 ;;
    fail) return 1 ;;
    *) return 3 ;;
  esac
}

# Check a single-step outcome after ci_step (for tiers whose later steps
# need an earlier one): prints the recorded status of step <name>.
ci_step_status() {
  awk -F '\t' -v n="$1" '$1 == n { s = $2 } END { print s }' "$CI_TIER_DIR/steps.tsv"
}

# Path of a host-side output file of the current tier.
ci_file() { printf '%s/%s' "$CI_TIER_DIR" "$1"; }

# The cargo used by host steps (respects rust-toolchain.toml through rustup).
CI_CARGO="${CI_CARGO:-cargo}"
