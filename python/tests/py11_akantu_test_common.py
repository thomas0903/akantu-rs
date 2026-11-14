# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: stand-in for upstream's test extension `py11_akantu_test_common`
# (test/test_python_interface/test_common.cc), which the port builds into
# the main extension as the private sub-module `_test_common`.
from akantu import py11_akantu as _py11_akantu

globals().update(
    {k: v for k, v in vars(_py11_akantu._test_common).items()
     if not k.startswith("__")}
)
