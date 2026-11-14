#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs helper: patch a COPY of
# an akantu-rs checkout so that the listed oracle probes run with the D-050
# pin (for an oracle build without the build-time pin):
#   * Eigen::setCpuCacheSizes(16384, 524288, 524288) right after
#     akantu::initialize (the anchor used for the solver probes),
#     with the queried and the pinned sizes printed to stderr as evidence;
#   * OPENBLAS_CORETYPE=Haswell OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 passed
#     to the oracle container by the probe's generate.sh.
# With --unpinned-report the probe only prints the queried sizes (control run).
# Usage: pin_probes.py <checkout> <probe>[,<probe>...] [--unpinned-report]
import re, sys
root, probes = sys.argv[1], sys.argv[2].split(",")
pin = "--unpinned-report" not in sys.argv
SRC = {"phase_field": "phase_field_probe.cc", "core": "core_probe.cc",
       "solver": "solver_probe.cc"}
for p in probes:
    cc = f"{root}/oracle/probes/{p}/{SRC[p]}"
    s = open(cc).read()
    m = re.findall(r"\n(  initialize\([^;]*\);\n)", s)
    assert len(m) == 1, (cc, m)
    anchor = m[0]
    body = ('  {\n'
            '    std::cerr << "[va4] eigen cache sizes queried l1=" << Eigen::l1CacheSize()\n'
            '              << " l2=" << Eigen::l2CacheSize() << " l3=" << Eigen::l3CacheSize() << "\\n";\n')
    if pin:
        body += ('    Eigen::setCpuCacheSizes(16384, 524288, 524288);\n'
                 '    std::cerr << "[va4] eigen cache sizes pinned l1=" << Eigen::l1CacheSize()\n'
                 '              << " l2=" << Eigen::l2CacheSize() << " l3=" << Eigen::l3CacheSize() << "\\n";\n')
    body += '  }\n'
    s = s.replace(anchor, anchor + body, 1)
    if "#include <iostream>" not in s:
        s = s.replace("#include <", "#include <iostream>\n#include <", 1)
    open(cc, "w").write(s)
    gen = f"{root}/oracle/probes/{p}/generate.sh"
    g = open(gen).read()
    if pin and "OPENBLAS_CORETYPE" not in g:
        g2 = g.replace('-e HOME=/tmp', '-e HOME=/tmp -e OPENBLAS_CORETYPE=Haswell '
                       '-e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1', 1)
        assert g2 != g, gen
        open(gen, "w").write(g2)
    print("patched", cc, "pin" if pin else "report-only")
