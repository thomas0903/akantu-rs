#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# pybench.sh <suite_python args>: the Python suite with the venv interpreter
export MPLBACKEND=Agg AKANTU_SUITE_COMMIT=a1b454b0c
cd $HOME/w/suite-setup/akantu-rs
exec $HOME/w/suite-setup/venv/bin/python python/benchmarks/suite_python.py "$@"
