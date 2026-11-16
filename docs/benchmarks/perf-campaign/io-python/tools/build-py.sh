#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): measurement script of the
# I/O and Python optimisation (docs/benchmarks/perf-campaign/io-python), kept as run.
# build-py.sh <commit> <venv-name>: build the akantu Python extension at <commit> into ~/w/io/<venv-name>
set -euo pipefail
. ~/.cargo/env
W=~/w/io
D=$W/py; [ "$2" = venv-a ] || D=$W/py-$2
[ -d $D ] || git -C $W/akantu-rs worktree add -q --detach $D
git -C $W/akantu-rs fetch -q origin
cd $D
git checkout -q --detach "$1"
V=$W/$2
[ -x $V/bin/maturin ] || { rm -rf $V; python3 -m venv --without-pip --system-site-packages $V && $V/bin/python -m pip install -q --ignore-installed maturin numpy scipy pytest greenlet matplotlib; }
export CARGO_TARGET_DIR=$W/target-$2 VIRTUAL_ENV=$V PATH=$V/bin:$PATH
cd python && maturin develop --release -q 2>&1 | tail -3
python -c "import akantu; print(\"akantu\", akantu.__file__)"
