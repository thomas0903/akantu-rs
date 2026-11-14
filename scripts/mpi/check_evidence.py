#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs MPI tier support (no upstream counterpart):
# per-rank evidence of one mpi_abort_paths scenario run.
#
#   check_evidence.py <scenario> <ranks> <stdout log> <stderr log>
#
# Exit 0 if every rank recorded the evidence its role requires, every
# intercepted MPI_Abort (scripts/mpi/abort_shim.c) was legal for the thread
# level (MPI's main thread, or provided >= MPI_THREAD_SERIALIZED), and the
# scenario-specific abort-path expectations hold. Prints the reasons
# otherwise (exit 1).
import re
import sys

T = "kind=Some(Transport)"
WAIT = ["waiting-for-abort"]

# scenario -> function(rank, ranks) -> substrings that rank's evidence must
# contain (all of them, possibly on different EVIDENCE lines).
SPEC = {
    "transport_local_result": lambda r, n: (
        ["settled=Some(Transport) fast=true reborn_broken=true raw_kind=Some(Transport) broken=true"]
        if r == 0 else ["entering-agreement"]),
    "unagreeable_fatal": lambda r, n: (
        ["preflight_failed=true late_kind=Some(Transport) fast=true broken=true"]
        if r == 0 else ["preflight_failed=true late_kind=None fast=true broken=false"] + WAIT),
    "standalone_sync_peer_missing": lambda r, n: (
        ["not-applicable"] if n == 1 else
        [T, "broken=true timed_out=true"] if r == 0 else WAIT),
    "rendezvous_naive_deadlock": lambda r, n: [T + " after_ms=", "broken=true"],
    "transport_breaks_derived": lambda r, n: [
        "dup_kind=Some(Transport) parent_kind=Some(Transport) split_kind=Some(Transport) scope_born_broken=true"],
    "mpi_error_translation": lambda r, n: [
        T + " mentions_rank_error=true raw_kind=Some(Transport)"],
    "funneled_worker_policy": lambda r, n: [
        "scope_broken=true settled_kind=Some(Transport) main_thread_refusal=true"],
    "dropped_barrier_breaks_family": lambda r, n: (
        ["not-applicable"] if n == 1 else
        ["dup_broken=true parent_broken=true split_broken=true",
         "dup_kind=Some(Transport) parent_kind=Some(Transport) split_kind=Some(Transport) refused_fast=true"]
        if r == 0 else WAIT),
    "after_header_omitted_payload": lambda r, n: (
        ["not-applicable"] if n == 1 else
        [T + " parent_broken=true derived_broken=true"] if r == 0 else WAIT),
    "payload_stall_deadline": lambda r, n: (
        ["not-applicable"] if n == 1 else
        [T, "bounded=true"] if r == 0 else
        ["sender-stalled"] if r == 1 else WAIT),
    "worker_abort_funneled": lambda r, n: ["worker-abort level=Funneled"],
    "worker_abort_serialized": lambda r, n: (
        ["worker-abort level=Serialized"] if r == 0 else WAIT),
    "serialized_lock_held_abort": lambda r, n: (
        ["lock-held-abort level=Serialized"] if r == 0 else WAIT),
    "region_abort_serialized": lambda r, n: (
        ["region-abort level=Serialized"] if r == 0 else WAIT),
    "stdout_lock_held_abort": lambda r, n: (
        [T + " level=Funneled", "stdout-locked-abort"] if r == 0 else WAIT),
}

# Scenarios that never abort (single rank).
NOT_APPLICABLE_ON_ONE = {
    "standalone_sync_peer_missing", "dropped_barrier_breaks_family",
    "after_header_omitted_payload", "payload_stall_deadline",
}

FALLBACK = "emergency termination without MPI_Abort"
# The fallback asks the launcher to abort through PMIx_Abort (Open MPI
# 5 links libpmix; the MPI test image is Open MPI 5.0.7).
PMIX_ABORT = "asking the launcher to abort the job through PMIx_Abort"


def main():
    scenario, ranks, out_log, err_log = sys.argv[1], int(sys.argv[2]), sys.argv[3], sys.argv[4]
    out = open(out_log, encoding="utf-8", errors="replace").read()
    err = open(err_log, encoding="utf-8", errors="replace").read()
    problems = []
    evidence = {r: [] for r in range(ranks)}
    for m in re.finditer(r"EVIDENCE rank=(\d+) ([^\n]*)", out):
        r = int(m.group(1))
        if r not in evidence:
            problems.append(f"evidence from unexpected rank {r}")
            continue
        evidence[r].append(m.group(2))
    for r in range(ranks):
        joined = "\n".join(evidence[r])
        for want in SPEC[scenario](r, ranks):
            if want not in joined:
                problems.append(f"rank {r}: missing evidence {want!r} (got {evidence[r]!r})")
    intercepts = [
        (m.group(1), int(m.group(2)), int(m.group(3)))
        for m in re.finditer(
            r"ABORT_INTERCEPT rank=(\S+) thread_main=(-?\d+) provided=(-?\d+)", err)
    ]
    for rank, main, provided in intercepts:
        if main != 1 and provided < 2:
            problems.append(
                f"illegal MPI_Abort: rank {rank} called it off MPI's main thread "
                f"under thread level {provided}")
    applicable = not (ranks == 1 and scenario in NOT_APPLICABLE_ON_ONE)
    if applicable:
        if scenario == "worker_abort_funneled":
            if intercepts:
                problems.append(f"MPI_Abort called from a FUNNELED worker: {intercepts}")
            if FALLBACK not in err:
                problems.append("no non-MPI emergency termination recorded")
            if PMIX_ABORT not in err:
                problems.append("no PMIx_Abort request recorded (launcher workaround)")
        elif scenario == "worker_abort_serialized":
            if not any(rank == "0" and main == 0 and provided == 2
                       for rank, main, provided in intercepts):
                problems.append(f"expected a legal SERIALIZED worker MPI_Abort on rank 0: {intercepts}")
        elif scenario == "serialized_lock_held_abort":
            if any(rank == "0" for rank, _, _ in intercepts):
                problems.append(f"MPI_Abort called while the SERIALIZED lock was held: {intercepts}")
            if FALLBACK not in err or "holds the MPI lock" not in err:
                problems.append("no bounded non-MPI emergency termination recorded")
        elif scenario == "region_abort_serialized":
            # The region thread (MPI's main thread here) holds the
            # SERIALIZED lock itself: MPI_Abort at once, no fallback.
            if not any(rank == "0" and main == 1 and provided == 2
                       for rank, main, provided in intercepts):
                problems.append(f"expected MPI_Abort from the region thread on rank 0: {intercepts}")
            if FALLBACK in err:
                problems.append("non-MPI emergency termination instead of MPI_Abort")
        elif not any(main == 1 for _, main, _ in intercepts):
            problems.append(f"no main-thread MPI_Abort intercepted: {intercepts}")
    for p in problems:
        print(f"EVIDENCE-CHECK {scenario} np={ranks}: {p}")
    sys.exit(1 if problems else 0)


if __name__ == "__main__":
    main()
