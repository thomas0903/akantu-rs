# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: self-test of oracle/probes/python/akantu_probe.py. Writes
# fixtures/selftest/python_writer_roundtrip, read back bit-exactly by
# crates/akantu-validation/tests/selftest_fixtures.rs.
#
# Run (needs numpy; e.g. inside akantu-oracle:dev):
#   PYTHONPATH=oracle/probes/python python3 -I oracle/probes/selftest/writer_selftest.py fixtures
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "python"))
import numpy as np  # noqa: E402

from akantu_probe import FixtureWriter  # noqa: E402


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <fixtures_root>", file=sys.stderr)
        return 2
    w = FixtureWriter(sys.argv[1], "selftest/python_writer_roundtrip",
                      "Self-test of the Python fixture writer (same content "
                      "as the C++ self-test)", "writer_selftest_py", "1")
    w.manifest["generator"]["command"] = (
        "python3 oracle/probes/selftest/writer_selftest.py fixtures")
    w.parameters["note"] = "values are fixed by construction"
    w.parameters["pi"] = float(np.pi)
    special = np.array([0.0, -0.0, 1.0, 0.1, -1.0 / 3.0,
                        np.finfo(float).smallest_subnormal,
                        np.finfo(float).tiny, np.finfo(float).max,
                        np.finfo(float).eps, np.nextafter(1.0, 2.0),
                        np.nan, np.inf, -np.inf])
    w.add("special", special, ["entry"], "table",
          description="special IEEE-754 values", tolerance_class="exact")
    u = np.array([[n + 0.25 * c for c in range(2)] for n in range(4)])
    w.add("u", u, ["node", "component"], "nodal", components=["x", "y"],
          ordering=[{"axis": "node", "kind": "global_id",
                     "key_quantity": "node_ids"}])
    w.add("node_ids", np.array([10, 11, 12, 13], dtype=np.int64), ["node"],
          "nodal")
    w.add("i32", np.array([[-2**31, -1], [0, 2**31 - 1]], dtype=np.int32),
          ["row", "col"], "table")
    w.add("u8", np.array([0, 255], dtype=np.uint8), ["entry"], "table")
    w.add("flags", np.array([True, False, True]), ["entry"], "table")
    w.add("empty", np.zeros((0, 3)), ["entry", "component"], "table")
    w.add("scalar", np.array(42.5), [], "scalar")
    w.add("residuals/offsets", np.array([0, 2, 2, 3], dtype=np.int64),
          ["entry"], "ragged_offsets")
    w.add("residuals", np.array([1.0, 0.5, 0.25]), ["iteration"],
          "ragged_values", ragged={"offsets": "residuals/offsets",
                                   "row_axis": "step"})
    w.tags.append("selftest")
    print("wrote", w.finish())
    return 0


if __name__ == "__main__":
    sys.exit(main())
