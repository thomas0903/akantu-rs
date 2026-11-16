#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# incl.sh <perf.data> <cpu> [limit] [grep]: inclusive (children) profile of one CPU, demangled
perf report -i "$1" --symfs ~/w/dist/symfs --children --cpu "$2" --stdio --percent-limit "${3:-1}" -g none --sort sym 2>/dev/null \
  | grep -v "^#" | grep -v "^$" | ~/.cargo/bin/rustfilt | sed -E 's/::h[0-9a-f]{16}//g; s/\.llvm\.[0-9]+//g' | grep -E "${4:-.}" | cut -c1-220
