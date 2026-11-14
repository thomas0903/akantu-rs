#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (parity row CI-002).
# lint: cargo fmt --check, clippy -D warnings, shellcheck + bash -n, ruff (python/)
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
# shellcheck source=ci/lib.sh
. "$here/lib.sh"
ci_tier_begin lint

if ci_backend_ready; then
  ci_step fmt -- cargo fmt --all --check
  ci_step clippy -- cargo clippy --workspace --all-targets --locked -- -D warnings
else
  ci_skip fmt "backend $CI_BACKEND cannot run cargo (no cargo / image $CI_IMAGE absent)"
  ci_skip clippy "backend $CI_BACKEND cannot run cargo"
fi

# Maintained shell tooling (tracked files under CI_SHELLCHECK_PATHS, default
# "ci scripts"), on the host. Not in the default scope: oracle/ (C++ oracle
# tooling, with 30+ shellcheck findings). Everything under ci/ and scripts/
# is clean with shellcheck 0.9.0 (the version of the ubuntu-24.04 runner
# image).
shell_list="$(ci_file shell-files.txt)"
# Outside a git checkout (e.g. an exported tree) every *.sh file counts.
# shellcheck disable=SC2086
if git -C "$CI_REPO" rev-parse --git-dir > /dev/null 2>&1; then
  (cd "$CI_REPO" && git ls-files -- ${CI_SHELLCHECK_PATHS:-ci scripts} | grep '\.sh$') > "$shell_list"
else
  (cd "$CI_REPO" && find ${CI_SHELLCHECK_PATHS:-ci scripts} -name '*.sh' -type f 2> /dev/null | sort) > "$shell_list"
fi
if [ -s "$shell_list" ]; then
  # shellcheck disable=SC2016
  ci_step bash-n --host -- sh -c 'cd "$1" && rc=0; while IFS= read -r f; do bash -n "$f" || rc=1; done < "$2"; exit $rc' sh "$CI_REPO" "$shell_list"
  if ci_have shellcheck; then
    # -x follows `source` lines; the scripts' own directives apply.
    # shellcheck disable=SC2016
    ci_step shellcheck --host -- sh -c 'cd "$1" && tr "\n" "\0" < "$2" | xargs -0 shellcheck -x' sh "$CI_REPO" "$shell_list"
  else
    ci_skip shellcheck "shellcheck not installed on the host"
  fi
else
  ci_na bash-n "no tracked shell scripts"
  ci_na shellcheck "no tracked shell scripts"
fi

# Python package of the bindings (upstream ruff job, line length 79): only
# trees that carry python/pyproject.toml.
if [ -f "$CI_REPO/python/pyproject.toml" ]; then
  if ci_have ruff; then
    ci_step ruff --host -- ruff check "$CI_REPO/python"
  else
    ci_skip ruff "python/pyproject.toml present but ruff is not installed"
  fi
else
  ci_na ruff "no python/pyproject.toml on this tree (no Python package)"
fi

ci_tier_end
