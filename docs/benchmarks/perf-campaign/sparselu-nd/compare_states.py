# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs project tooling (not ported from Akantu): compares two files of
# little-endian f64 values (a suite's --dump-state output, or the solution
# vectors that the diagnostic patch appends with ND_DUMP_X) value by value.
#
#   python3 -I compare_states.py REF OTHER [N]
#
# Prints the number of values, the largest |REF|, the largest difference
# |OTHER - REF| relative to that largest |REF| (the scale of the rules of
# fixtures/fast_mode/*/tolerances.json, "max_abs"), and the number of
# bit-identical values. With N, the files are split into consecutive
# vectors of N values (one per solve) and each is reported.
import struct
import sys


def read(path):
    with open(path, "rb") as f:
        data = f.read()
    return struct.unpack("<%dd" % (len(data) // 8), data[: len(data) // 8 * 8])


def report(label, a, b):
    scale = max((abs(x) for x in a), default=0.0)
    diff = max((abs(x - y) for x, y in zip(a, b)), default=0.0)
    exact = sum(1 for x, y in zip(a, b) if struct.pack("<d", x) == struct.pack("<d", y))
    rel = diff / scale if scale > 0 else float("nan")
    print("%s n %d max|ref| %.6e max|diff| %.3e rel %.3e exact %d" % (label, len(a), scale, diff, rel, exact))


def main():
    if len(sys.argv) < 3:
        print("usage: compare_states.py REF OTHER [N]", file=sys.stderr)
        return 2
    a, b = read(sys.argv[1]), read(sys.argv[2])
    if len(a) != len(b):
        print("length differs: %d %d" % (len(a), len(b)))
        return 1
    if len(sys.argv) > 3:
        n = int(sys.argv[3])
        for k in range(len(a) // n):
            report("solve %d" % k, a[k * n:(k + 1) * n], b[k * n:(k + 1) * n])
    else:
        report("all", a, b)
    return 0


if __name__ == "__main__":
    sys.exit(main())
