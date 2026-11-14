# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: analysis of the variation study of upstream's run-dependent
# periodic `_sum` (oracle/probes/parallel/variation-study.sh,
# D-039 amendment). Stdlib only; run with python3 -I.
#
# Usage: python3 -I variation_analysis.py <study_dir> [--np P] [--fixtures DIR]
#   <study_dir>/run<k>/stage/parallel/<scenario>/np<P>/rank<r>/{manifest.json,data/*.npy}
#
# Per scenario and rank it checks that
#   (a) every payload except the unmasked `_sum` is byte-identical in every run;
#   (b) the entries (node, component) of the unmasked `_sum` that vary between
#       runs are exactly the entries of the recorded mask
#       `periodic/nondyadic_sum_run_dependent` (masked entries that never
#       varied are reported: the mask is then a superset, never a subset);
#   (c) `periodic/nondyadic_after_sum_deterministic` equals the unmasked `_sum`
#       bitwise at every unmasked entry and is +0.0 at every masked entry;
#   (d) an independent recomputation of the run-dependent masters from the
#       recorded per-rank contributions and periodic schemes (every arrival
#       order of the remote contributions) equals the recorded mask, and the
#       values observed at the owner lie in the computed set;
#   (e) with --fixtures DIR (the committed fixtures root): the recorded
#       version-3 payloads (`nondyadic_after_sum`, `nondyadic_sum_order_dependent`)
#       equal the committed ones byte for byte.
# Prints a summary (one block per scenario) and exits 1 on any violation.
import ast
import itertools
import pathlib
import struct
import sys

UNMASKED = "periodic__nondyadic_after_sum_unmasked.npy"
MASK = "periodic__nondyadic_sum_run_dependent.npy"
DET = "periodic__nondyadic_after_sum_deterministic.npy"
CONTRIB = "periodic__nondyadic_sum_contributions.npy"


def npy(path):
    b = path.read_bytes()
    if b[:6] != b"\x93NUMPY":
        raise ValueError(f"{path}: not an npy file")
    if b[6] == 1:
        hl = struct.unpack("<H", b[8:10])[0]
        off = 10
    else:
        hl = struct.unpack("<I", b[8:12])[0]
        off = 12
    h = ast.literal_eval(b[off:off + hl].decode("latin1"))
    n = 1
    for s in h["shape"]:
        n *= s
    fmt = {"<f8": "d", "<i8": "q", "<i4": "i", "|u1": "B", "<u1": "B", "|b1": "?"}[h["descr"]]
    return list(struct.unpack("<%d%s" % (n, fmt), b[off + hl:])), tuple(h["shape"])


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def main(argv):
    if len(argv) < 2:
        print(__doc__ or "usage: variation_analysis.py <study_dir> [--np P] [--fixtures DIR]")
        return 2
    study = pathlib.Path(argv[1])
    np_ = 4
    fixtures = None
    rest = argv[2:]
    while rest:
        if rest[0] == "--np":
            np_ = int(rest[1])
            rest = rest[2:]
        elif rest[0] == "--fixtures":
            fixtures = pathlib.Path(rest[1])
            rest = rest[2:]
        else:
            print(f"unknown argument {rest[0]}")
            return 2
    runs = sorted((p for p in study.glob("run*") if (p / "stage").is_dir()),
                  key=lambda p: int(p.name[3:]))
    if len(runs) < 2:
        print("fewer than 2 runs")
        return 1
    scenarios = sorted(p.name for p in (runs[0] / "stage/parallel").iterdir() if p.is_dir())
    bad = []
    total = dict(cases=0, masters=0, masked=0, varying=0, superset=0)
    print(f"variation study: {len(runs)} runs at {np_} ranks, {len(scenarios)} scenarios")
    for sc in scenarios:
        ranks = []
        for r in range(np_):
            per_run = [run / "stage/parallel" / sc / f"np{np_}" / f"rank{r}" / "data" for run in runs]
            names = sorted(f.name for f in per_run[0].iterdir())
            for d in per_run[1:]:
                if sorted(f.name for f in d.iterdir()) != names:
                    bad.append(f"{sc} rank {r}: payload sets differ between runs")
            # (a) everything but the unmasked sum identical
            for name in names:
                if name == UNMASKED:
                    continue
                ref = (per_run[0] / name).read_bytes()
                for k, d in enumerate(per_run[1:], start=2):
                    if (d / name).read_bytes() != ref:
                        bad.append(f"{sc} rank {r}: {name} differs between runs 1 and {k}")
            data = per_run[0]
            unm = [npy(d / UNMASKED)[0] for d in per_run]
            mask, shape = npy(data / MASK)
            det = npy(data / DET)[0]
            nbc = shape[1] if len(shape) == 2 else 1
            varying = {i for i in range(len(mask)) if len({bits(u[i]) for u in unm}) > 1}
            masked = {i for i, m in enumerate(mask) if m != 0}
            if varying - masked:
                bad.append(f"{sc} rank {r}: entries vary but are not masked: "
                           f"{sorted((i // nbc, i % nbc) for i in varying - masked)[:8]}")
            for i in range(len(det)):
                if i in masked:
                    if bits(det[i]) != 0:
                        bad.append(f"{sc} rank {r}: masked entry {i} recorded as {det[i]!r}")
                elif bits(det[i]) != bits(unm[0][i]):
                    bad.append(f"{sc} rank {r}: deterministic entry {i} differs from the unmasked sum")
            if fixtures is not None:
                committed = fixtures / "parallel" / sc / f"np{np_}" / f"rank{r}" / "data"
                for name in ("periodic__nondyadic_after_sum.npy",
                             "periodic__nondyadic_sum_order_dependent.npy"):
                    if (committed / name).read_bytes() != (data / name).read_bytes():
                        bad.append(f"{sc} rank {r}: version-3 payload {name} differs from the committed one")
            g = npy(data / "nodes__global_ids.npy")[0]
            send, recv = {}, {}
            for f in data.glob("periodic_scheme__*.npy"):
                _, kind, q = f.stem.split("__")
                (send if kind == "send" else recv)[int(q)] = npy(f)[0]
            ranks.append(dict(g=g, unm=unm, mask=mask, masked=masked, varying=varying, nbc=nbc,
                              contrib=npy(data / CONTRIB)[0], send=send, recv=recv,
                              pairs=npy(data / "periodic__pairs.npy")[0]))
        # (d) independent recomputation of the run-dependent masters
        nbc = ranks[0]["nbc"]
        owner, own, expected, remote = {}, {}, {}, {}
        for r, rk in enumerate(ranks):
            for q, nodes in rk["send"].items():
                for n in nodes:
                    G = rk["g"][n]
                    owner[G] = r
                    own[G] = [rk["contrib"][n * nbc + c] for c in range(nbc)]
                    expected.setdefault(G, set()).add(q)
            for p, nodes in rk["recv"].items():
                for n in nodes:
                    G = rk["g"][n]
                    remote.setdefault(G, {})[r] = ([rk["contrib"][n * nbc + c] for c in range(nbc)], p)
        dep = [set() for _ in range(nbc)]
        info = {}
        for G in sorted(owner):
            got = remote.get(G, {})
            if set(got) != expected[G] or any(p != owner[G] for _, p in got.values()):
                bad.append(f"{sc}: periodic schemes not symmetric at gid {G}")
                continue
            qs = sorted(got)
            per = []
            for c in range(nbc):
                vals = set()
                for perm in itertools.permutations(qs):
                    s = own[G][c]
                    for q in perm:
                        s = s + got[q][0][c]
                    vals.add(bits(s))
                o = ranks[owner[G]]
                i = o["g"].index(G) * nbc + c
                obs = {bits(u[i]) for u in o["unm"]}
                fold = own[G][c]
                for q in qs:
                    fold = fold + got[q][0][c]
                nonzero = sum(1 for q in qs if got[q][0][c] != 0.0)
                per.append((len(vals), len(obs), obs <= vals, nonzero, bits(fold) in vals))
                if not obs <= vals:
                    bad.append(f"{sc}: gid {G} component {c}: observed values outside the permutation set")
                if len(vals) > 1:
                    dep[c].add(G)
            info[G] = (owner[G], qs, per)
        # the mask derived from `dep` (a node takes its master's value) equals the recorded one
        for r, rk in enumerate(ranks):
            want = [0] * len(rk["mask"])
            master_of = {}
            pr = rk["pairs"]
            for j in range(0, len(pr), 2):
                master_of[pr[j + 1]] = pr[j]
            for n, G in enumerate(rk["g"]):
                src = rk["g"][master_of[n]] if n in master_of else G
                for c in range(nbc):
                    if G in dep[c] or src in dep[c]:
                        want[n * nbc + c] = 1
            if want != list(rk["mask"]):
                bad.append(f"{sc} rank {r}: recomputed mask differs from the recorded one")
        masters = sorted(set().union(*dep))
        n_masked = sum(len(rk["masked"]) for rk in ranks)
        n_varying = sum(len(rk["varying"]) for rk in ranks)
        n_superset = sum(len(rk["masked"] - rk["varying"]) for rk in ranks)
        total["cases"] += 1
        total["masters"] += len(masters)
        total["masked"] += n_masked
        total["varying"] += n_varying
        total["superset"] += n_superset
        print(f"{sc}: run-dependent masters {len(masters)} {masters}; masked entries {n_masked}; "
              f"entries that vary in {len(runs)} runs {n_varying}; masked but never varied {n_superset}")
        for G in sorted(info):
            o, qs, per = info[G]
            if len(qs) < 2:
                continue
            desc = "; ".join(f"c{c}: non-zero remote {nz}, possible {pv}, observed {ov}, "
                             f"rank-order fold in set {fin}" for c, (pv, ov, _, nz, fin) in enumerate(per))
            print(f"   gid {G}: owner {o}, remote ranks {qs}: {desc}")
    print(f"TOTAL: cases {total['cases']}, run-dependent masters {total['masters']}, masked entries "
          f"{total['masked']}, varying entries {total['varying']}, masked but never varied "
          f"{total['superset']}")
    if bad:
        print(f"VIOLATIONS ({len(bad)}):")
        for b in bad[:60]:
            print("  " + b)
        return 1
    print("OK: every payload but the unmasked _sum identical in every run; varying entries == mask"
          + (" (mask == varying set)" if total["superset"] == 0 else " (mask is a superset, see above)"))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
