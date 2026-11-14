# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (D-101 (1), PY-053) mpirun scenarios of the import /
# exit lifecycle of the MPI build of the Python module (port-only checks
# through `_test_common.mpiLifecycle()`; the oracle module runs the
# scenarios that use only upstream API). Upstream behaviour:
# python/akantu/__init__.py imports mpi4py before `__initialize` when
# `has_mpi()` (mpi4py initialises MPI and finalises it at interpreter exit),
# src/common/aka_common.cc `initialize` (the static communicator),
# src/synchronizer/mpi_communicator_data.hh (MPI finalised by akantu only
# if akantu initialised it). The port's rules: akantu-comm's lifecycle
# (imported vs owned environment, D-015), the exit hook (D-067), the
# module's atexit release.
#
#   mpirun -np N python3 lifecycle.py <scenario>
#
# Scenarios (each rank prints `LIFECYCLE PASS <scenario> rank=<r>` or
# `LIFECYCLE FAIL <scenario> rank=<r>: <why>`; the job must also end with
# status 0 on every rank, i.e. no double MPI_Finalize, no rank left
# unfinalised):
# * mpi4py_first      upstream's order (the module's import imports mpi4py,
#                     which initialises MPI): imported environment, a mesh
#                     distributed and freed, every akantu handle released
#                     by the module's atexit (0 guards), MPI still alive
#                     for mpi4py, which finalises it after the teardown;
# * mpi4py_first_kept the same with the distributed mesh kept in a global
#                     until the teardown;
# * akantu_first      mpi4py imported without initialising
#                     (`mpi4py.rc.initialize = rc.finalize = False`): the
#                     module initialises MPI (owned, `funneled`) and
#                     finalises it at exit once its last handle is gone
#                     (here at its atexit callback); mpi4py does not;
# * akantu_first_kept the same, mpi4py not importable at all and the mesh
#                     kept until the teardown (finalised when the teardown
#                     frees it, or by the exit hook);
# * funneled_thread   akantu first (`funneled`): `Mesh.distribute()` from a
#                     Python worker thread raises `akantu.Exception` on
#                     every rank, then succeeds on the main thread;
# * funneled_mpi4py   mpi4py first with `rc.thread_level = "funneled"`: the
#                     same refusal on the imported environment;
# * stale_view        a view of the nodes taken before `distribute()` keeps
#                     its values (stale but safe), a new one is the local
#                     part; a second `distribute()` raises.
import atexit
import os
import sys

scenario = sys.argv[1]
rank = int(os.environ.get("OMPI_COMM_WORLD_RANK", "0"))
size = int(os.environ.get("OMPI_COMM_WORLD_SIZE", "1"))
failures = []
reported = []


def check(cond, why):
    if not cond:
        failures.append(why)


def report():
    if reported:
        return
    reported.append(True)
    if failures:
        print(f"LIFECYCLE FAIL {scenario} rank={rank}: {'; '.join(failures)}",
              flush=True)
    else:
        print(f"LIFECYCLE PASS {scenario} rank={rank}", flush=True)


def lifecycle():
    import akantu as aka
    tc = getattr(aka, "_test_common", None)
    if tc is None or not hasattr(tc, "mpiLifecycle"):
        return None  # the oracle module
    return tc.mpiLifecycle()


# Registered before akantu is imported: Python runs atexit callbacks in
# reverse order, so this one runs after the module's own (the release).
def at_exit_check():
    state = lifecycle()
    if scenario in ("mpi4py_first", "funneled_mpi4py"):
        from mpi4py import MPI
        check(not MPI.Is_finalized(), "MPI finalised before mpi4py's turn")
        if state is not None:
            check(state == ("imported", 0), f"lifecycle at exit {state}")
    elif scenario == "akantu_first":
        from mpi4py import MPI
        if state is not None:
            check(state == ("finalized", 0), f"lifecycle at exit {state}")
            check(MPI.Is_finalized(), "akantu did not finalise its MPI")
    elif scenario in ("mpi4py_first_kept",):
        if state is not None:
            check(state[0] == "imported" and state[1] >= 1,
                  f"lifecycle at exit {state}")
    report()


atexit.register(at_exit_check)

if scenario.startswith("akantu_first") or scenario == "funneled_thread":
    if scenario == "akantu_first_kept":
        sys.modules["mpi4py"] = None  # not importable: akantu alone
    else:
        import mpi4py
        mpi4py.rc.initialize = False
        mpi4py.rc.finalize = False
elif scenario == "funneled_mpi4py":
    import mpi4py
    mpi4py.rc.thread_level = "funneled"

import numpy as np  # noqa: E402

import akantu as aka  # noqa: E402

check(aka.has_mpi(), "has_mpi() is False")
state = lifecycle()
baseline = state  # the handles the runtime holds after the import
if state is not None:
    owned = scenario.startswith("akantu_first") or scenario == "funneled_thread"
    check(state[0] == ("owned" if owned else "imported"),
          f"lifecycle after import {state}")
if "mpi4py" in sys.modules and sys.modules["mpi4py"] is not None:
    from mpi4py import MPI
    check(MPI.Is_initialized(), "MPI not initialised after import")
    check(MPI.COMM_WORLD.Get_size() == size, "world size")
    if scenario in ("akantu_first", "funneled_thread"):
        check(MPI.Query_thread() == MPI.THREAD_FUNNELED,
              f"thread level {MPI.Query_thread()}")


def plate():
    mesh = aka.Mesh(2)
    if rank == 0:
        mesh.read("plate_hole.msh")
    return mesh


if scenario in ("mpi4py_first", "akantu_first", "mpi4py_first_kept",
                "akantu_first_kept"):
    mesh = plate()
    mesh.distribute()
    check(mesh.isDistributed(), "not distributed")
    n_local = mesh.getNbNodes()
    check(n_local > 0, "no local node")
    if scenario.endswith("_kept"):
        KEPT = mesh  # freed by the interpreter's teardown
    else:
        del mesh
        import gc
        gc.collect()
        state = lifecycle()
        if state is not None:
            # the mesh released every handle it held: back to the import's
            check(state == baseline, f"guards after del {state} vs {baseline}")
elif scenario in ("funneled_thread", "funneled_mpi4py"):
    import threading
    mesh = plate()
    got = {}

    def worker():
        try:
            mesh.distribute()
            got["worker"] = "accepted"
        except aka.Exception as e:
            got["worker"] = str(e)

    t = threading.Thread(target=worker)
    t.start()
    t.join()
    check(got.get("worker") not in (None, "accepted"),
          f"worker distribute: {got.get('worker')}")
    check(not mesh.isDistributed(), "distributed by the refused call")
    mesh.distribute()
    check(mesh.isDistributed(), "main-thread distribute")
    del mesh
elif scenario == "stale_view":
    mesh = plate()
    old = mesh.getNodes()
    before = np.array(old)
    mesh.distribute()
    check(np.array_equal(old, before), "the old view changed")
    new = mesh.getNodes()
    check(new.shape[0] == mesh.getNbNodes(), "new view size")
    if size > 1 and rank == 0:
        check(new.shape[0] < before.shape[0], "rank 0 kept every node")
    try:
        mesh.distribute()
        check(False, "a second distribute was accepted")
    except aka.Exception as e:
        check("already distribute" in str(e), f"second distribute: {e}")
    del mesh, old, new
else:
    check(False, f"unknown scenario {scenario}")

if scenario not in ("mpi4py_first", "akantu_first", "mpi4py_first_kept",
                    "funneled_mpi4py"):
    report()
