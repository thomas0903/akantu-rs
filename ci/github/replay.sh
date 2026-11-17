#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (rows CI-001...CI-029): the
# first run of every job's exact command outside GitHub.
#
#   ci/github/replay.sh <job> <out-dir>
#
# Runs the steps of ONE workflow job of ci/github/*.yml as the job runs them
# on a GitHub runner, on a machine that has Docker, a Rust toolchain and the
# repository checked out: the image step (ci/github/images.sh
# with the same image set), the job's environment, and the job's command word
# for word. `ci/selftest.sh` checks that every `ci/run-all.sh` /
# `ci/selftest.sh` / `ci/github/distro-run.sh` command of a workflow file occurs
# in this file (with `${{ matrix.* }}` as $LEG and `${{ github.sha }}` as
# $GITHUB_SHA), so a workflow edit that is not replayed here is reported.
#
# <job> is <workflow>:<job> or <workflow>:<job>:<leg> (the matrix leg of
# medium / memcheck), e.g. fast:fast  fast:msrv  features:gates  medium:medium:mpi
# memcheck:memcheck:mumps  docs:site  packaging:wheel-linux. Not replayable on Linux:
# packaging:wheel-macos (macOS runner; the same command was run on a Mac).
# The results are <out-dir>/ci-out/ (summary.json, one directory per tier);
# the exit code is the command's (0 PASS, 1 FAIL, 3 INCOMPLETE).
# $GITHUB_ACTIONS is unset: ci/github/free-disk.sh is a no-op (it would remove
# toolchains of a GitHub runner image, which a sandbox does not have).
set -uo pipefail
job="${1:?usage: replay.sh <workflow>:<job>[:<leg>] <out-dir>}"
out="${2:?usage: replay.sh <workflow>:<job>[:<leg>] <out-dir>}"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo" || exit 2
mkdir -p "$out"
out="$(cd "$out" && pwd)"
export RUNNER_TEMP="$out" GITHUB_WORKSPACE="$repo"
GITHUB_SHA="$(git rev-parse HEAD 2> /dev/null || echo unknown)"
export GITHUB_SHA
unset GITHUB_ACTIONS
IFS=: read -r wf jid LEG <<< "$job"
export AKANTU_RS_DOCKER_CPUS="${AKANTU_RS_DOCKER_CPUS:-4}" AKANTU_RS_DOCKER_MEMORY="${AKANTU_RS_DOCKER_MEMORY:-6g}"
images() { ci/github/images.sh "$HOME/akantu-images" "$@"; }

case "$wf:$jid" in
  fast:fast)
    images mpi
    python3 -m venv "$RUNNER_TEMP/ruff"
    "$RUNNER_TEMP/ruff/bin/pip" install -q "ruff==0.17.3"
    export PATH="$RUNNER_TEMP/ruff/bin:$PATH"
    ci/run-all.sh --backend docker --preset fast --out "$RUNNER_TEMP/ci-out" --label "github $GITHUB_SHA"
    ;;
  fast:selftest)
    export CI_SELFTEST_PULL_ACTIONLINT=1
    ci/selftest.sh
    ;;
  fast:msrv)
    export CI_INSTALL_TOOLCHAINS=1
    ci/run-all.sh --backend host --tiers msrv --out "$RUNNER_TEMP/ci-out"
    ;;
  features:features)
    images mpi mumps petsc
    ci/run-all.sh --backend docker --tiers features --out "$RUNNER_TEMP/ci-out"
    ;;
  features:gates)
    images mpi
    ci/run-all.sh --backend docker --tiers gates --out "$RUNNER_TEMP/ci-out"
    ;;
  features:facade-release)
    images mpi
    export CI_RELEASE_ONLY=facade
    ci/run-all.sh --backend docker --tiers release --out "$RUNNER_TEMP/ci-out"
    ;;
  features:exportall)
    images mpi
    ci/run-all.sh --backend docker --tiers exportall --out "$RUNNER_TEMP/ci-out"
    ;;
  medium:medium)
    images mpi scotch mumps petsc
    # shellcheck disable=SC2086 # $LEG is one tier name (the matrix leg)
    ci/run-all.sh --backend docker --tiers $LEG --out "$RUNNER_TEMP/ci-out"
    ;;
  memcheck:memcheck)
    images mpi scotch mumps petsc
    export CI_MEMCHECK_KIND="$LEG"
    ci/run-all.sh --backend docker --tiers memcheck --out "$RUNNER_TEMP/ci-out"
    ;;
  coverage:coverage)
    images mpi
    export CI_INSTALL_TOOLCHAINS=1
    ci/run-all.sh --backend docker --tiers coverage --out "$RUNNER_TEMP/ci-out"
    ;;
  python:python)
    export CI_WHEEL_PIP_BASE=numpy==2.2.4
    ci/run-all.sh --backend host --preset python --out "$RUNNER_TEMP/ci-out"
    ;;
  docs:rustdoc)
    ci/run-all.sh --backend host --tiers docs --out "$RUNNER_TEMP/ci-out"
    ;;
  docs:manual)
    cargo install mdbook --version 0.4.40 --locked --root "$RUNNER_TEMP/mdbook" || exit $?
    cargo install mdbook-katex --version 0.9.4 --locked --root "$RUNNER_TEMP/mdbook" || exit $?
    ci/github/manual-build.sh "$RUNNER_TEMP/mdbook/bin"
    ;;
  docs:site)
    # the `site` job of docs.yml (D-114): the manual, rustdoc, the Python API
    # documentation and the assembling step. The `deploy` job (the Pages
    # actions) runs only on GitHub and is not replayed.
    export CI_WHEEL_PIP_BASE=numpy==2.2.4
    cargo install mdbook --version 0.4.40 --locked --root "$RUNNER_TEMP/mdbook" || exit $?
    cargo install mdbook-katex --version 0.9.4 --locked --root "$RUNNER_TEMP/mdbook" || exit $?
    ci/github/manual-build.sh "$RUNNER_TEMP/mdbook/bin" || exit $?
    ci/run-all.sh --backend host --tiers docs --out "$RUNNER_TEMP/ci-out-docs" || exit $?
    ci/run-all.sh --backend host --preset python --out "$RUNNER_TEMP/ci-out" || exit $?
    ci/github/pages-site.sh "$RUNNER_TEMP/site" --python "$RUNNER_TEMP/ci-out/pydocs/html"
    ;;
  packaging:crates-dry-run)
    export CI_INSTALL_TOOLCHAINS=1
    ci/run-all.sh --backend host --tiers version,package,minver --out "$RUNNER_TEMP/ci-out"
    ;;
  packaging:wheel-linux)
    # the job runs in the digest-pinned manylinux image (repository variable
    # MANYLINUX_IMAGE); the workspace is a scratch copy of HEAD (the container
    # writes target/ and ci-out/ into it)
    : "${MANYLINUX_IMAGE:=quay.io/pypa/manylinux_2_28_x86_64@sha256:9555ba6c479fa69a2aa80ad1dc57154a60d562b19df159361d9379a02beb1e36}"
    work="$out/workspace"
    mkdir -p "$work"
    git archive HEAD | tar -x -C "$work"
    docker run --rm -v "$work:/w" -w /w -e HOME=/root "$MANYLINUX_IMAGE" bash -c '
      set -e
      curl -sSfO https://static.rust-lang.org/rustup/archive/1.29.0/x86_64-unknown-linux-gnu/rustup-init
      echo "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10  rustup-init" | sha256sum -c -
      chmod +x rustup-init
      ./rustup-init -y --profile minimal --default-toolchain none --no-modify-path
      export PATH="$HOME/.cargo/bin:$PATH"
      export GITHUB_WORKSPACE=/w
      export CI_WHEEL_PYTHON=/opt/python/cp312-cp312/bin/python CI_WHEEL_COMPAT=manylinux_2_28
      export CI_WHEEL_SMOKE="/opt/python/cp39-cp39/bin/python /opt/python/cp313-cp313/bin/python"
      export CI_WHEEL_PIP_BASE=numpy==2.2.4
      bash ci/run-all.sh --backend host --tiers wheel --out "$GITHUB_WORKSPACE/ci-out"
    '
    ;;
  distro:ubuntu-22-04)
    ci/github/distro-run.sh "$RUNNER_TEMP/distro-out"
    ;;
  expensive:ranks-8)
    images mpi scotch mumps
    ci/run-all.sh --backend docker --preset expensive --out "$RUNNER_TEMP/ci-out"
    ;;
  expensive:miri)
    export CI_INSTALL_TOOLCHAINS=1
    ci/run-all.sh --backend host --preset miri --out "$RUNNER_TEMP/ci-out"
    ;;
  # wheel-macos (macOS runner, not replayable here); its command, for the check
  # of ci/selftest.sh:
  #   CI_WHEEL_COMPAT=off bash ci/run-all.sh --backend host --tiers wheel --out "$RUNNER_TEMP/ci-out"
  *)
    echo "replay.sh: unknown job '$job' (see the header)" >&2
    exit 2
    ;;
esac
