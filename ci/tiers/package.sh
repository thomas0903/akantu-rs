#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-021, CI-022, CI-029: packaging, never publishing).
# package: cargo package --list per workspace member (dry run, no publish) + readiness report
#
# `cargo package --list -p <member>` for every member (it works for
# publish = false members too; building .crate files is not possible for
# members with path dependencies while every member is publish = false,
# because cargo's local overlay registry only takes publishable members),
# then ci/tools/package_report.py: sizes, gzip estimate, readiness findings
# (report.json). Nothing is uploaded; `cargo publish` is never called.
# Then a downstream-consumer check (CI-029: "test downstream consumption
# separately"): a scratch crate outside the workspace depends on the
# umbrella `akantu` by path with default features, then with
# --no-default-features, and must compile against the workspace's locked
# versions (Cargo.lock copied in; --offline with CI_CARGO_OFFLINE=1).
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin package

if ! ci_backend_ready; then
  ci_skip package "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
  ci_tier_end
  exit $?
fi
if ! ci_have python3; then
  ci_skip package "python3 is needed on the host for the report"
  ci_tier_end
  exit $?
fi

meta="$(ci_file metadata.json)"
ci_step metadata --stdout "$meta" -- cargo metadata --no-deps --format-version 1 --locked
if [ "$(ci_step_status metadata)" != pass ]; then
  ci_tier_end
  exit $?
fi
dirty=()
if [ -n "$(git -C "$CI_REPO" status --porcelain --untracked-files=normal 2> /dev/null)" ]; then
  dirty=(--allow-dirty)
  echo "the tree was dirty: --allow-dirty was passed (not a clean dry run)" > "$(ci_file dirty.txt)"
fi
lists="$(ci_file lists)"
mkdir -p "$lists"
members="$(python3 -c 'import json,sys; m=json.load(open(sys.argv[1])); ids=set(m["workspace_members"]); print(" ".join(sorted(p["name"] for p in m["packages"] if p["id"] in ids)))' "$meta")"
for m in $members; do
  ci_step "list:$m" --stdout "$lists/$m.list" -- cargo package --list -p "$m" --locked ${dirty[@]+"${dirty[@]}"}
done
ci_step report --host -- python3 "$here/tools/package_report.py" "$meta" "$lists" "$CI_REPO" --json "$(ci_file report.json)"

# Downstream consumer, created where cargo runs (/tmp of the container or
# of the host); the repository path is the cargo working directory. The
# scratch crate is removed on every exit path, its manifest is written with
# printf (no sed on the repository path), and the copy of Cargo.lock may
# only prune (the downstream crate needs fewer packages) and add the scratch
# crate, never re-resolve (ci/tools/lock_unchanged.py): `--locked`
# cannot be used because the scratch root package is not in the lockfile.
off=""
[ "${CI_CARGO_OFFLINE:-0}" = 1 ] && off="--offline"
for mode in default no-default; do
  # shellcheck disable=SC2016
  ci_step "downstream:$mode" -- sh -c '
    set -e
    mode=$1; off=$2
    repo=$(pwd)
    case "$repo" in *\"* | *\\*) echo "a repository path with a quote or backslash is not supported" >&2; exit 1 ;; esac
    d=$(mktemp -d); trap "rm -rf \"$d\"" EXIT
    mkdir -p "$d/src"
    if [ "$mode" = default ]; then
      dep="akantu = { path = \"$repo/crates/akantu\" }"
    else
      dep="akantu = { path = \"$repo/crates/akantu\", default-features = false }"
    fi
    printf "[package]\nname = \"akantu-downstream-check\"\nversion = \"0.0.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n%s\n\n[workspace]\n" "$dep" > "$d/Cargo.toml"
    printf "fn main() {\n    let _ = std::any::type_name::<akantu::core::Error>();\n}\n" > "$d/src/main.rs"
    cp Cargo.lock "$d/Cargo.lock"
    cat "$d/Cargo.toml"
    cargo check --manifest-path "$d/Cargo.toml" $off
    python3 -I ci/tools/lock_unchanged.py Cargo.lock "$d/Cargo.lock" akantu-downstream-check
  ' sh "$mode" "$off"
done

ci_tier_end
