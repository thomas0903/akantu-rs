# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu).
# callers.py: read `perf script` output; for samples whose leaf matches a pattern, count caller chains
import sys, collections, re
pat = sys.argv[1]; depth = int(sys.argv[2]) if len(sys.argv) > 2 else 6; lead = int(sys.argv[3]) if len(sys.argv) > 3 else 1
cnt = collections.Counter(); total = 0; hits = 0
frames = []
def flush():
    global total, hits
    if not frames: return
    total += 1
    idx = next((i for i, f in enumerate(frames[:lead]) if pat in f), None)
    if idx is not None:
        hits += 1
        chain = []
        for f in frames[idx+1:]:
            if 'libc.so' in f: continue
            chain.append(f)
            if len(chain) >= depth: break
        cnt[' <- '.join(chain)] += 1
for line in sys.stdin:
    if not line.strip():
        flush(); frames = []; continue
    if line[0] in ' \t':
        parts = line.strip().split(None, 1)
        if len(parts) == 2:
            sym = re.sub(r'\+0x[0-9a-f]+', '', parts[1])
            sym = re.sub(r'::h[0-9a-f]{16}', '', sym)
            frames.append(sym[:160])
flush()
print(f'{hits} of {total} samples match {pat}')
for k, v in cnt.most_common(15):
    print(f'{100*v/total:6.2f}%  {k}')
