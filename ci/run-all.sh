#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling (not ported from Akantu; the role of upstream's
# .gitlab-ci.yml stages, parity rows CI-001, CI-019, CI-020): run CI tiers
# and write a machine-readable summary. Bash 3.2 compatible.
#
#   ci/run-all.sh [--preset P | --tiers T1,T2,...] [--backend host|docker]
#                 [--out DIR] [--image IMG] [--target-name NAME]
#                 [--step-timeout S] [--label TEXT] [--dry-run] [--list]
#                 [-- extra args passed to the mpi/par/mumps/expensive tiers]
#
# Presets (default: fast):
#   fast       lint test fixtures parity licence
#   features   features msrv gates
#   medium     mpi par mumps release examples petsc exportall
#   packaging  package wheel minver version
#   python     wheel pydocs
#   docs       docs
#   miri       miri (nightly; unsafe code under Stacked and Tree Borrows)
#   memcheck   memcheck (valgrind, FFI unsafe code; CI_MEMCHECK_KIND=mpi|mumps|petsc)
#   coverage   coverage (cargo-llvm-cov)
#   distro     features test (second distribution, host backend)
#   expensive  expensive (8 ranks)
#   all        every tier above except miri, memcheck, coverage, distro and expensive
# Tiers: see ci/README.md (one script each in ci/tiers/).
#
# Backends: `host` runs cargo on this machine (a GitHub runner, a developer
# machine for the light tiers, a Linux host); `docker` runs cargo
# inside the project's Linux image through scripts/mpi/in-docker.sh (the
# authoritative x86_64 configuration, D-043). The mpi/par/mumps/expensive tiers
# always drive Docker themselves (scripts/mpi/run-*.sh).
#
# Output (default target/ci/<UTC stamp>; on a remote host pass a directory
# outside the repository): one directory per tier (ci/lib.sh), summary.tsv
# (tier, status, seconds, reason) and summary.json (schema
# "akantu-rs-ci-summary/1": provenance, tiers, steps, verdict). The last line
# printed is `CI: PASS`, `CI: INCOMPLETE (...)` or `CI: FAIL (...)`.
# Exit code: 0 PASS (every selected tier passed or does not apply), 1 FAIL
# (a step failed), 3 INCOMPLETE (nothing failed, but a step could not run:
# a missing prerequisite is never a pass), 2 usage error.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/.." && pwd)"

usage() {
  sed -n '7,10p' "$0" >&2
  exit 2
}

ALL_TIERS="lint test exportall fixtures parity licence features gates msrv minver package version wheel pydocs docs miri memcheck coverage mpi par mumps petsc release examples expensive"
preset_tiers() {
  case "$1" in
    fast) echo "lint test fixtures parity licence" ;;
    features) echo "features msrv gates" ;;
    medium) echo "mpi par mumps release examples petsc exportall" ;;
    packaging) echo "package wheel minver version" ;;
    python) echo "wheel pydocs" ;;
    docs) echo "docs" ;;
    miri) echo "miri" ;;
    memcheck) echo "memcheck" ;;
    coverage) echo "coverage" ;;
    distro) echo "features test" ;;
    expensive) echo "expensive" ;;
    all) echo "lint test exportall fixtures parity licence features gates msrv package version wheel pydocs minver docs mpi par mumps petsc release examples" ;;
    *) return 1 ;;
  esac
}

tiers="" preset="" backend="host" out="" image="" target_name="" step_timeout="" label="" dry=0 list=0
while [ $# -gt 0 ]; do
  case "$1" in
    --preset)
      [ $# -ge 2 ] || usage
      preset="$2"
      shift 2
      ;;
    --tiers)
      [ $# -ge 2 ] || usage
      tiers="$(printf '%s' "$2" | tr ',' ' ')"
      shift 2
      ;;
    --backend)
      [ $# -ge 2 ] || usage
      backend="$2"
      shift 2
      ;;
    --out)
      [ $# -ge 2 ] || usage
      out="$2"
      shift 2
      ;;
    --image)
      [ $# -ge 2 ] || usage
      image="$2"
      shift 2
      ;;
    --target-name)
      [ $# -ge 2 ] || usage
      target_name="$2"
      shift 2
      ;;
    --step-timeout)
      [ $# -ge 2 ] || usage
      step_timeout="$2"
      shift 2
      ;;
    --label)
      [ $# -ge 2 ] || usage
      label="$2"
      shift 2
      ;;
    --dry-run)
      dry=1
      shift
      ;;
    --list)
      list=1
      shift
      ;;
    --)
      shift
      break
      ;;
    -h | --help) usage ;;
    *)
      echo "run-all.sh: unknown option '$1'" >&2
      usage
      ;;
  esac
done

if [ "$list" = 1 ]; then
  for t in $ALL_TIERS; do
    printf '%-10s %s\n' "$t" "$(sed -n '4s/^# *//p' "$here/tiers/$t.sh")"
  done
  for p in fast features medium packaging python docs miri memcheck coverage distro expensive all; do
    printf 'preset %-10s %s\n' "$p" "$(preset_tiers "$p")"
  done
  exit 0
fi

[ -z "$tiers" ] || [ -z "$preset" ] || {
  echo "run-all.sh: --preset and --tiers are exclusive" >&2
  exit 2
}
if [ -z "$tiers" ]; then
  tiers="$(preset_tiers "${preset:-fast}")" || {
    echo "run-all.sh: unknown preset '$preset'" >&2
    exit 2
  }
fi
# An empty tier list runs nothing: never a pass (F8).
case "$(printf '%s' "$tiers" | tr -d ' ')" in
  '')
    echo "run-all.sh: no tier selected (empty --tiers list)" >&2
    exit 2
    ;;
esac
for t in $tiers; do
  case " $ALL_TIERS " in
    *" $t "*) ;;
    *)
      echo "run-all.sh: unknown tier '$t' (tiers: $ALL_TIERS)" >&2
      exit 2
      ;;
  esac
done
case "$backend" in host | docker) ;; *)
  echo "run-all.sh: --backend must be host or docker" >&2
  exit 2
  ;;
esac
case "$step_timeout" in
  *[!0-9]*)
    echo "run-all.sh: --step-timeout takes seconds" >&2
    exit 2
    ;;
esac
case "$target_name" in *[!A-Za-z0-9._-]*)
  echo "run-all.sh: --target-name takes a plain directory name" >&2
  exit 2
  ;;
esac

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
[ -n "$out" ] || out="$repo/target/ci/$stamp"
mkdir -p "$out" || exit 2
out="$(cd "$out" && pwd)"

export CI_REPO="$repo" CI_OUT="$out" CI_BACKEND="$backend" CI_DRY_RUN="$dry"
[ -z "$image" ] || export CI_IMAGE="$image"
[ -z "$target_name" ] || export CI_TARGET_NAME="$target_name"
[ -z "$step_timeout" ] || export CI_STEP_TIMEOUT="$step_timeout"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"

# Provenance, recorded before anything runs.
{
  echo "schema=akantu-rs-ci-summary/1"
  echo "label=$label"
  echo "started=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "backend=$backend"
  echo "image=$CI_IMAGE"
  echo "target_name=$CI_TARGET_NAME"
  echo "tiers=$tiers"
  echo "extra_args=$(ci_join "$@")"
  echo "dry_run=$dry"
  echo "git_head=$(git -C "$repo" rev-parse HEAD 2> /dev/null || echo unknown)"
  echo "git_tree=$(git -C "$repo" rev-parse 'HEAD^{tree}' 2> /dev/null || echo unknown)"
  echo "git_dirty_files=$(git -C "$repo" status --porcelain --untracked-files=normal 2> /dev/null | grep -c .)"
  echo "host=$(uname -s) $(uname -m) $(uname -r)"
  echo "host_cargo=$(cargo -V 2> /dev/null || echo none)"
  echo "host_rustc=$(rustc -V 2> /dev/null || echo none)"
  echo "host_python=$(python3 -V 2> /dev/null || echo none)"
  # Image ids only when Docker is used (no Docker call otherwise).
  case "$backend $tiers " in
    docker* | *" mpi "* | *" par "* | *" mumps "* | *" petsc "* | *" examples "* | *" memcheck "* | *" expensive "*) use_docker=1 ;;
    *) use_docker=0 ;;
  esac
  if [ "$use_docker" = 1 ] && ci_have docker; then
    for img in akantu-rs-mpi:dev akantu-rs-scotch:dev akantu-rs-mumps:dev akantu-rs-petsc:dev; do
      echo "image_id[$img]=$(docker image inspect "$img" --format '{{.Id}}' 2> /dev/null || echo absent)"
    done
  fi
} > "$out/provenance.txt"

: > "$out/summary.tsv"
for t in $tiers; do
  t0=$(date +%s)
  # A reused --out must not leak a stale status of an earlier run into this
  # one (a tier script that dies before ci_tier_begin writes none).
  rm -rf "${out:?}/$t"
  CI_EXTRA_ARGS="$(ci_join "$@")" bash "$here/tiers/$t.sh" "$@"
  rc=$?
  status="$(cat "$out/$t/status" 2> /dev/null || echo fail)"
  reason=""
  if [ ! -f "$out/$t/status" ]; then
    reason="tier script exited $rc without a status"
    mkdir -p "$out/$t"
    echo fail > "$out/$t/status"
  fi
  printf '%s\t%s\t%s\t%s\n' "$t" "$status" "$(($(date +%s) - t0))" "$reason" >> "$out/summary.tsv"
done
echo "finished=$(date -u +%Y-%m-%dT%H:%M:%SZ)" >> "$out/provenance.txt"

if ci_have python3; then
  python3 "$here/tools/summarize.py" "$out"
  exit $?
fi
# Without python3: the verdict from summary.tsv alone (no summary.json).
# Same rule as tools/summarize.py: only pass / na are acceptable, an unknown
# status is a failure, and at least one tier must have passed.
fails="$(awk -F '\t' '$2 != "pass" && $2 != "na" && $2 != "incomplete" { printf "%s ", $1 }' "$out/summary.tsv")"
inc="$(awk -F '\t' '$2 == "incomplete" { printf "%s ", $1 }' "$out/summary.tsv")"
npass="$(awk -F '\t' '$2 == "pass" { n++ } END { print n + 0 }' "$out/summary.tsv")"
if [ -n "$fails" ]; then
  echo "CI: FAIL (${fails% }) [no python3: summary.json not written]"
  exit 1
elif [ -n "$inc" ]; then
  echo "CI: INCOMPLETE (${inc% }) [no python3: summary.json not written]"
  exit 3
elif [ "$npass" = 0 ]; then
  echo "CI: INCOMPLETE (no tier passed: nothing ran) [no python3: summary.json not written]"
  exit 3
fi
echo "CI: PASS [no python3: summary.json not written]"
exit 0
