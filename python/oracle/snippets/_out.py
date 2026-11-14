# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: output helper of the oracle comparison snippets.
"""``out(key, value)`` prints one ``key = repr`` line (floats as
``float.hex`` lists so that the comparison can be bitwise)."""

import numpy as np


def _conv(v):
    if isinstance(v, np.ndarray):
        if v.dtype.kind == "f":
            return {"f64": [float(x).hex() for x in v.ravel()],
                    "shape": list(v.shape)}
        return {"dtype": str(v.dtype), "shape": list(v.shape),
                "values": v.ravel().tolist()}
    if isinstance(v, float):
        return {"f64": [v.hex()], "shape": []}
    if isinstance(v, (list, tuple)):
        return [_conv(x) for x in v]
    if isinstance(v, (np.floating,)):
        return {"f64": [float(v).hex()], "shape": []}
    if isinstance(v, (np.integer,)):
        return int(v)
    return v


def out(key, value):
    print(f"{key} = {_conv(value)!r}", flush=True)
