#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs MPI tier support (no upstream counterpart):
# self-test of check_evidence.py on synthetic logs, run by run-mpi-tier.sh
# and on any host:  python3 scripts/mpi/test_check_evidence.py
#
# Includes the acceptance rule of the earlier runner (expected substring
# anywhere in the log + one EVIDENCE line per rank) to show the cases it
# wrongly accepted.
import os
import re
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
CHECK = os.path.join(HERE, "check_evidence.py")


def check(scenario, ranks, out, err=""):
    with tempfile.TemporaryDirectory() as d:
        o, e = os.path.join(d, "out"), os.path.join(d, "err")
        open(o, "w").write(out)
        open(e, "w").write(err)
        r = subprocess.run([sys.executable, CHECK, scenario, str(ranks), o, e],
                           capture_output=True, text=True)
        return r.returncode, r.stdout


def old_rule(want, ranks, log):
    """The earlier run-mpi-tier.sh: substring anywhere and n distinct ranks."""
    seen = set(re.findall(r"EVIDENCE rank=(\d+)", log))
    return want in log and len(seen) == ranks


MAIN_ABORT = "ABORT_INTERCEPT rank=0 thread_main=1 provided=1 errorcode=71\n"
GOOD_LOCAL = (
    "EVIDENCE rank=1 entering-agreement\n"
    "EVIDENCE rank=0 settled=Some(Transport) fast=true reborn_broken=true "
    "raw_kind=Some(Transport) broken=true\n"
)


class CheckEvidence(unittest.TestCase):
    def test_region_abort_uses_mpi_abort(self):
        # The region thread (MPI's main thread, holding
        # the SERIALIZED lock) must call MPI_Abort; the non-MPI fallback
        # ("another thread holds the MPI lock") fails the check.
        out = "EVIDENCE rank=0 region-abort level=Serialized\nEVIDENCE rank=1 waiting-for-abort\n"
        good = "ABORT_INTERCEPT rank=0 thread_main=1 provided=2 errorcode=71\n"
        bad = ("akantu: emergency termination without MPI_Abort (MPI_THREAD_SERIALIZED and "
               "another thread holds the MPI lock): process exits with code 71\n")
        self.assertEqual(check("region_abort_serialized", 2, out, good)[0], 0)
        self.assertEqual(check("region_abort_serialized", 2, out, bad)[0], 1)

    def test_good_log_passes(self):
        self.assertEqual(check("transport_local_result", 2, GOOD_LOCAL, MAIN_ABORT)[0], 0)

    def test_record_on_the_wrong_rank_fails(self):
        # The expected rank-0 record appears only on rank 1, rank 0 printed
        # something else: the old rule accepted this log.
        log = (
            "EVIDENCE rank=0 entering-agreement\n"
            "EVIDENCE rank=1 settled=Some(Transport) fast=true reborn_broken=true "
            "raw_kind=Some(Transport) broken=true\n"
        )
        want = "settled=Some(Transport) fast=true reborn_broken=true raw_kind=Some(Transport) broken=true"
        self.assertTrue(old_rule(want, 2, log), "old rule accepted it")
        self.assertEqual(check("transport_local_result", 2, log, MAIN_ABORT)[0], 1)

    def test_missing_rank_fails(self):
        log = GOOD_LOCAL.splitlines()[1] + "\n"
        self.assertEqual(check("transport_local_result", 2, log, MAIN_ABORT)[0], 1)

    def test_illegal_abort_fails(self):
        err = "ABORT_INTERCEPT rank=1 thread_main=0 provided=1 errorcode=71\n" + MAIN_ABORT
        self.assertEqual(check("transport_local_result", 2, GOOD_LOCAL, err)[0], 1)

    def test_no_abort_at_all_fails(self):
        self.assertEqual(check("transport_local_result", 2, GOOD_LOCAL, "")[0], 1)

    def test_funneled_worker_must_not_call_mpi_abort(self):
        out = "EVIDENCE rank=0 worker-abort level=Funneled\n"
        fallback = ("akantu: emergency termination without MPI_Abort (MPI_THREAD_Funneled "
                    "and this is not MPI's main thread): process exits with code 71\n")
        pmix = "akantu: asking the launcher to abort the job through PMIx_Abort (code 71)\n"
        self.assertEqual(check("worker_abort_funneled", 1, out, fallback + pmix)[0], 0)
        bad = "ABORT_INTERCEPT rank=0 thread_main=0 provided=1 errorcode=71\n"
        self.assertEqual(check("worker_abort_funneled", 1, out, bad + fallback + pmix)[0], 1)
        # The fallback must have asked the launcher through PMIx_Abort
        self.assertEqual(check("worker_abort_funneled", 1, out, fallback)[0], 1)

    def test_not_applicable_on_one_rank(self):
        out = "EVIDENCE rank=0 not-applicable single-rank\n"
        self.assertEqual(check("dropped_barrier_breaks_family", 1, out, "")[0], 0)


if __name__ == "__main__":
    unittest.main()
