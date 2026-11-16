#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): measurement script of the
# I/O and Python optimisation (docs/benchmarks/perf-campaign/io-python), kept as run.
# pysuite.sh <venv-name> <suite_python args>: the Python suite with that venv (the venv's own source tree)
V=$1; shift
D=~/w/io/py; [ "$V" = venv-a ] || D=~/w/io/py-$V
export MPLBACKEND=Agg AKANTU_SUITE_COMMIT=$(git -C $D rev-parse --short=9 HEAD)
cd $D
exec ~/w/io/$V/bin/python python/benchmarks/suite_python.py "$@"
