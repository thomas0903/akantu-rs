# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (UD-1174) oracle snippet: the
# generic `ParameterRegistry.get(name)` of parameters of every type on both
# modules (python/py_parser.cc:88-110; Parameter::isOfType,
# parameter_registry_tmpl.hh:98-108). Upstream raises for every non-Real
# parameter; the port returns the value (category B): the keys of the
# non-Real parameters (Plane_Stress, delta_T, finite_deformation) DIFFER by
# design, the Real ones and the access errors are equal (messages compared
# without upstream's exception-class prefix and source location). Inputs (work dir): elastic.dat, mesh_dcb_2d.msh.
import akantu as aka
from _out import out


def core(msg):
    """The message without upstream's decorations (`akantu::debug::<X> :
    ` before it, ` [<file>:<line>]` after it)."""
    msg = msg.splitlines()[0]
    if " : " in msg and msg.startswith("akantu::"):
        msg = msg.split(" : ", 1)[1]
    if " [" in msg:
        msg = msg.split(" [", 1)[0]
    return msg


def outcome(f):
    try:
        r = f()
        return ["ok", r if isinstance(r, (bool, int, float, str)) else None]
    except Exception as e:  # noqa: BLE001
        return [type(e).__name__, core(str(e))]


aka.parseInput("elastic.dat")
mesh = aka.Mesh(2)
mesh.read("mesh_dcb_2d.msh")
model = aka.SolidMechanicsModel(mesh)
model.initFull(_analysis_method=aka._static)
mat = model.getMaterial(0)
for n in ("E", "nu", "rho", "Plane_Stress", "delta_T", "finite_deformation",
          "inelastic_deformation", "lambda", "name"):
    out(f"ud1174.get.{n}", outcome(lambda n=n: mat.get(n)))
