#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# flat.sh <perf.data> <cpu> [limit]: flat self profile of one CPU, demangled, with sample counts
# (~/w/dist/symfs holds the binary and the image's libraries at their container paths)
perf report -i "$1" --symfs ~/w/dist/symfs --no-children --cpu "$2" --stdio -n --percent-limit "${3:-0.4}" -g none --sort dso,sym 2>/dev/null \
  | grep -v "^#" | grep -v "^$" | ~/.cargo/bin/rustfilt | sed -E 's/::h[0-9a-f]{16}//g; s/\.llvm\.[0-9]+//g' | cut -c1-200
