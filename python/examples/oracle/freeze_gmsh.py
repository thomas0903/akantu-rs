# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (EX-036): run the unchanged upstream eigen_modes.py up
# to its `subprocess.call(["gmsh", "-2", "bar.geo"])`, let that call run the
# real Gmsh of the environment, then stop. Leaves bar.geo and bar.msh (the
# mesh the oracle's Gmsh produces from the script's own .geo) in the current
# directory. Usage: python3 freeze_gmsh.py <path of eigen_modes.py>
import subprocess
import sys

script = sys.argv[1]
real_call = subprocess.call


def call_then_stop(args, *a, **k):
    rc = real_call(args, *a, **k)
    raise SystemExit(rc)


subprocess.call = call_then_stop
sys.argv = [script]
with open(script, encoding="utf-8") as fh:
    code = compile(fh.read(), script, "exec")
exec(code, {"__name__": "__main__", "__file__": script})
