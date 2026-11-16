#!/bin/bash
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): suite-setup measurement script, kept as run.
# report.sh <stem>: text report of <stem>.data into <stem>.txt
f="$1.data"
{
  echo "# perf record -F 499 --call-graph dwarf; $(basename "$1")"
  echo "# phases of the profiled run (seconds, calls):"
  [ -f "$1.jsonl" ] && python3 -c "
import json,collections,sys
t=collections.OrderedDict()
for l in open('$1.jsonl'):
    d=json.loads(l); a=t.setdefault(d['phase'],[0,0]); a[0]+=d['seconds']; a[1]+=d['calls']
tot=sum(v[0] for v in t.values())
for k,v in t.items(): print('#   %-28s %9.3f s %7d calls %5.1f %%' % (k,v[0],v[1],100*v[0]/tot if tot else 0))
"
  echo
  echo "## flat profile (self), top symbols"
  sudo perf report -i "$f" --no-children --sort dso,symbol --stdio -g none --percent-limit 0.2 2>/dev/null | grep -v '^#' | grep -v '^$' | head -25
  echo
  echo "## inclusive (children), top entries"
  sudo perf report -i "$f" --children --sort symbol --stdio -g none --percent-limit 2 2>/dev/null | grep -v '^#' | grep -v '^$' | head -45
  echo
  echo "## call chains of the top self symbols (callers, >= 1 %)"
  sudo perf report -i "$f" --no-children --sort symbol --stdio -g caller,1,callee,function,percent --percent-limit 3 2>/dev/null | grep -v '^#' | grep -v '^$' | head -260
} | "${RUSTFILT:-rustfilt}" | sed -E "s/ {3,}-? +-? *$//; s/ {3,}$//" > "$1.txt"
