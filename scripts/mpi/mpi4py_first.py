# SPDX-License-Identifier: LGPL-3.0-or-later
# mpi4py-first lifecycle scenario (INT-009, `docs/ARCHITECTURE.md`):
# mpi4py initialises MPI, then the akantu-comm probe (a cdylib built from
# crates/akantu-comm/examples/mpi4py_probe.rs) is loaded through ctypes.
#
#   mpirun -np N python3 scripts/mpi/mpi4py_first.py <libmpi4py_probe.so> <multiple|funneled>
import ctypes
import sys
import threading

import mpi4py

mpi4py.rc.thread_level = sys.argv[2]
from mpi4py import MPI  # noqa: E402  (initialises MPI: mpi4py first)

lib = ctypes.CDLL(sys.argv[1])
probe = lib.akantu_comm_mpi4py_probe
probe.argtypes = [ctypes.c_int]
probe.restype = ctypes.c_int

results = {}
if sys.argv[2] == "funneled":
    t = threading.Thread(target=lambda: results.__setitem__("worker", probe(1)))
    t.start()
    t.join()
results["main"] = probe(0)

# akantu must not have finalised MPI: mpi4py still communicates.
alive = not MPI.Is_finalized()
total = MPI.COMM_WORLD.allreduce(1) if alive else -1
ok = alive and all(v == 0 for v in results.values()) and total == MPI.COMM_WORLD.Get_size()
level = {MPI.THREAD_SINGLE: "single", MPI.THREAD_FUNNELED: "funneled",
         MPI.THREAD_SERIALIZED: "serialized", MPI.THREAD_MULTIPLE: "multiple"}[MPI.Query_thread()]
print(f"MPI4PY {'PASS' if ok else 'FAIL'} rank={MPI.COMM_WORLD.Get_rank()} "
      f"thread_level={level} results={results}", flush=True)
sys.exit(0 if ok else 1)
