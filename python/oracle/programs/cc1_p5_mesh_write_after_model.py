# SPDX-License-Identifier: LGPL-3.0-or-later
# Python concurrency case: a write into a retained mesh view after a model was created
# on the mesh. Upstream: the view aliases the mesh nodes (allowed; the
# model uses the new coordinates from its next computation). Port: the
# write cannot be applied (set-up operation refused once a model uses the
# mesh) — this program records what happens next: is the error raised once,
# or does every later call on the mesh and on the model fail, even after
# the view was dropped (mirror never refreshed)?
import gc


from cc1_common import check, static_model

mesh, model = static_model()
nodes = mesh.getNodes()
nodes[0, 0] += 1e-9
errors = []
for what, call in [
    ("model.solveStep", lambda: model.solveStep()),
    ("model.getEnergy", lambda: model.getEnergy("potential")),
    ("mesh.getNbNodes", lambda: mesh.getNbNodes()),
]:
    try:
        call()
        errors.append((what, None))
    except Exception as e:  # noqa: BLE001
        errors.append((what, f"{type(e).__name__}: {e}"))
del nodes
gc.collect()
after = []
for what, call in [
    ("model.solveStep", lambda: model.solveStep()),
    ("mesh.getNbNodes", lambda: mesh.getNbNodes()),
    ("mesh.getNodes", lambda: mesh.getNodes()),
]:
    try:
        call()
        after.append((what, None))
    except Exception as e:  # noqa: BLE001
        after.append((what, f"{type(e).__name__}: {e}"))
for w, e in errors:
    print("WITH_VIEW", w, e)
for w, e in after:
    print("AFTER_DEL", w, e)
check("p5_model_usable_after_view_dropped",
      all(e is None for _, e in after), "")
