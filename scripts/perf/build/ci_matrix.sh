#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): the whole-tier bitwise
# checks of the build-profile study, one configuration after the other
# (ci_variant.sh), on a machine with the project images (a boat sandbox:
# 55 GB of disk, so each configuration's target directory in the image's
# target volume is deleted before the next one starts).
#   ci_matrix.sh REPO OUT_ROOT TIERS NAME...
# Writes OUT_ROOT/<name>/ (run-all.sh output) and OUT_ROOT/matrix.tsv
# (name, exit status, last line of run-all.sh, disk use before cleaning).
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$1" outroot="$2" tiers="$3"; shift 3
mkdir -p "$outroot"
img="${AKANTU_RS_MPI_IMAGE:-akantu-rs-mpi:dev}"
for name in "$@"; do
  slug="$(printf '%s' "$name" | tr '+' '-')"
  st=0
  "$here/ci_variant.sh" "$repo" "$name" "$tiers" "$outroot/$slug" "$HOME/ci-scratch" > "$outroot/$slug.log" 2>&1 || st=$?
  last="$(grep -E '^CI: ' "$outroot/$slug.log" | tail -1)"
  disk="$(df -h / | awk 'NR == 2 {print $5}')"
  printf '%s\t%s\t%s\t%s\n' "$name" "$st" "$last" "$disk" >> "$outroot/matrix.tsv"
  docker run --rm -v akantu-rs-mpi-target:/target "$img" rm -rf "/target/${TARGET_PREFIX:-build}-$slug"
done
echo "matrix done" >> "$outroot/matrix.tsv"
