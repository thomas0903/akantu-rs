# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case: another Python thread writes (plain item assignment,
# GIL held, not a GIL-releasing NumPy operation, so outside UD-085) into a
# retained model view while the main thread is inside `applyBC` running a
# Python functor that releases the GIL (time.sleep). Upstream: the view
# aliases the array, the write lands. With export mirrors the write goes to
# the mirror, which the call's sync-out then overwrites.
import threading
import time

import akantu as aka
from cc1_common import check, out, static_model

mesh, model = static_model()
vel_like = model.getExternalForce()  # retained view, not touched by Dirichlet BCs
inside = threading.Event()
written = threading.Event()


def writer():
    inside.wait(10)
    vel_like[7, 1] = 123.0  # plain assignment from another thread
    written.set()


class Slow(aka.DirichletFunctor):
    def __call__(self, node, flags, primal, coord):
        primal[0] = 0.0
        flags[0] = True
        if not inside.is_set():
            inside.set()
            written.wait(10)  # releases the GIL; the writer runs now
            time.sleep(0.01)


t = threading.Thread(target=writer)
t.start()
model.applyBC(Slow(), "edge")
t.join()
v = float(vel_like[7, 1])
out("thread_write", [v])
check("p6_write_from_other_thread_during_callback_is_kept", v == 123.0,
      f"fext[7,1]={v} (written={written.is_set()})")
