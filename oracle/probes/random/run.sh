#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs. Builds and runs the random-number oracle probes inside the
# oracle Docker image (GCC/libstdc++/glibc of Debian trixie) and refreshes the
# fixtures in crates/akantu-core/tests/fixtures/random/.
#
# Usage: oracle/probes/random/run.sh [--platform linux/amd64] [--image IMG]
#
# Variants (all must agree for the fixtures to be meaningful; the script
# reports any difference):
#   strict  : -O2 -ffp-contract=off   (the oracle's `strict` configuration;
#                                       this is what is committed)
#   default : -O2                     (GCC default -ffp-contract=fast)
#   O0      : -O0
set -euo pipefail
source "$(cd "$(dirname "${BASH_SOURCE[0]}")/../../scripts" && pwd)/oracle-env.sh"  # ORACLE_ENV_ARGS (D-050)

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
image="akantu-oracle:dev"
platform=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --platform) platform="--platform $2"; shift 2 ;;
    --image) image="$2"; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done

work="$(mktemp -d "${TMPDIR:-/tmp}/akantu-random-probe.XXXXXX")"
trap 'rm -rf "$work"' EXIT
cp "$here/dump_random.cc" "$here/libm_vectors.cc" "$here/libm_special.cc" "$work/"

# shellcheck disable=SC2086
docker run --rm ${ORACLE_ENV_ARGS[@]+"${ORACLE_ENV_ARGS[@]}"} $platform -v "$work":/w -w /w "$image" bash -euo pipefail -c '
  for v in strict default O0; do
    case $v in
      strict)  flags="-O2 -ffp-contract=off" ;;
      default) flags="-O2" ;;
      O0)      flags="-O0" ;;
    esac
    mkdir -p out-$v
    g++ -std=c++20 $flags -DPROBE_FLAGS="\"-std=c++20 $flags\"" -o dump_random-$v dump_random.cc
    ./dump_random-$v out-$v
    g++ -std=c++20 $flags -DPROBE_FLAGS="\"-std=c++20 $flags\"" -o libm_vectors-$v libm_vectors.cc
    ./libm_vectors-$v > out-$v/libm.json
    g++ -std=c++20 $flags -fno-builtin -DPROBE_FLAGS="\"-std=c++20 $flags\"" -o libm_special-$v libm_special.cc
    ./libm_special-$v > out-$v/libm_special_cartesian.json
  done
  # sed -n 1p reads all input: `| head -1` can SIGPIPE the writer under pipefail (exit 141).
  g++ --version | sed -n 1p > toolchain.txt
  ldd --version | sed -n 1p >> toolchain.txt
  uname -m >> toolchain.txt
'

status=0
for v in default O0; do
  for f in "$work"/out-strict/*.json; do
    b="$(basename "$f")"
    if ! cmp -s <(tail -n +2 "$f") <(tail -n +2 "$work/out-$v/$b"); then
      echo "DIFFERENCE: $b differs between strict and $v" >&2
      status=1
    fi
  done
done
cat "$work/toolchain.txt"

dest="$repo/crates/akantu-core/tests/fixtures/random"
if [[ -z "$platform" ]]; then
  mkdir -p "$dest"
  cp "$work"/out-strict/*.json "$dest/"
  echo "fixtures written to $dest"
else
  out="$repo/target/random-probe-$(echo "$platform" | tr -c 'a-z0-9' '_')"
  rm -rf "$out"; mkdir -p "$out"
  cp -r "$work"/out-* "$out/"
  echo "outputs for $platform written to $out (fixtures untouched)"
  for f in "$work"/out-strict/*.json; do
    b="$(basename "$f")"
    if ! cmp -s <(tail -n +2 "$f") <(tail -n +2 "$dest/$b"); then
      echo "DIFFERENCE: $b ($platform strict) differs from committed fixture" >&2
      status=1
    fi
  done
fi
exit $status
