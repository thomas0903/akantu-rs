#!/usr/bin/env bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs CI tooling, not ported from Akantu (row CI-012).
# Runs INSIDE the digest-pinned ubuntu:22.04 container of
# ci/github/distro-run.sh: /src is the repository (read-only), /out the
# results. Installs what a developer on this distribution would (apt packages
# of the native features, rustup from static.rust-lang.org checked against its
# sha256), copies the tree to a writable place and runs the `distro` preset
# (tiers `features` and `test`, host backend) on the bare distribution.
#
# MUMPS and PETSc are not installed here (their versions live in the project
# images, MUMPS 5.7 / PETSc 3.22), so the feature-matrix checks that need them
# are declared NOT APPLICABLE to this preset (CI_NATIVE_NA, ci/tiers/features.sh)
# and the job can pass; a failure of anything else stands out.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get -qq update
apt-get -qq -y --no-install-recommends install \
  ca-certificates curl build-essential git jq pkg-config \
  openmpi-bin libopenmpi-dev libclang-dev clang \
  libscotch-dev libptscotch-dev python3 python3-dev python3-venv python3-tomli
# The analyser (ci/tools/analyze_tests.py, used by the `test`
# tier) imports `tomllib`, which CPython 3.11 introduced; Ubuntu 22.04 has
# CPython 3.10. Debian's `tomli` is the same code under its old name: a
# one-line `tomllib` module on the analyser's PYTHONPATH (the tools run with
# `python3 -I` do not need it). Without it the analyser step fails with "No module named 'tomllib'".
mkdir -p /opt/shim
printf 'from tomli import *  # noqa: F403\nfrom tomli import TOMLDecodeError, load, loads  # noqa: F401\n' > /opt/shim/tomllib.py
export PYTHONPATH=/opt/shim
# rustup-init 1.29.0 for x86_64-unknown-linux-gnu, checked against its sha256
# (the installer is never piped into a shell; same pin as packaging.yml)
cd /tmp
curl -sSfO https://static.rust-lang.org/rustup/archive/1.29.0/x86_64-unknown-linux-gnu/rustup-init
echo "4acc9acc76d5079515b46346a485974457b5a79893cfb01112423c89aeb5aa10  rustup-init" | sha256sum -c -
chmod +x rustup-init
./rustup-init -y --profile minimal --default-toolchain none --no-modify-path
export PATH="$HOME/.cargo/bin:$PATH"
# The repository is mounted read-only: cargo needs a writable tree (target,
# lock checks). `git` inside needs the safe.directory exception.
mkdir -p /work
cp -a /src/. /work/
git config --global --add safe.directory /work
cd /work
# Host cargo builds with full debug info and incremental caches filled a 41 GB
# disk (100 %, a failed build) in this very job: line tables
# only and no incremental compilation, as the project image's wrapper
# (scripts/mpi/in-docker.sh) does.
export CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_INCREMENTAL=0
export CI_NATIVE_NA=mumps,petsc
# The target directory lies OUTSIDE the working tree: cargo prints the paths of
# the test executables relative to the working directory otherwise, and the
# analyser (`test` tier) matches absolute paths (otherwise 446 binaries
# "never ran").
export CARGO_TARGET_DIR=/cargo-target
rc=0
ci/run-all.sh --backend host --preset distro --out /out/ci-out || rc=$?
exit "$rc"
