# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: architecture gate (docs/ARCHITECTURE.md;
# INT-008, PY-005/PY-097/PY-098): real PyO3/NumPy tests of the foreign-view
# protocol, the GIL/native-section rules and error translation.
"""Zero-copy views, staleness, zero-component arrays, callbacks re-entering
Rust while views are held, refused callbacks, borrow flags and exceptions
crossing the boundary."""

import gc
import threading

import akantu as aka
import numpy as np
import py11_akantu_test_common as t
import pytest


def lease(a):
    """The `_ArrayLease` base object of an akantu view."""
    while not type(a).__name__ == "_ArrayLease":
        a = a.base
    return a


# --- views -------------------------------------------------------------------


def test_views_are_writable_zero_copy_and_c_ordered():
    addr, a = t.createArray(4, 3)
    assert a.dtype == np.float64
    assert a.flags.c_contiguous and a.flags.writeable
    assert not a.flags.owndata
    assert a.ctypes.data == addr
    a[1, :] = (1.0, 2.0, 3.0)
    assert t.nativeSum(addr) == 6.0
    t.nativeFill(addr, 0.5)
    assert np.all(a == 0.5)
    b = t.getArray(addr)
    assert b.ctypes.data == addr
    b[0, 0] = 7.0
    assert a[0, 0] == 7.0
    assert not lease(a).is_stale()


def test_slices_share_the_lease_and_outlive_the_parent():
    addr, a = t.createArray(10, 2)
    s = a[2:5, 1]
    del a
    gc.collect()
    s[:] = 3.0
    assert t.nativeSum(addr) == 9.0
    assert type(lease(s)).__name__ == "_ArrayLease"


def test_read_only_views_cannot_be_made_writable():
    addr, a = t.createArray(2, 2)
    ro = t.getArrayReadOnly(addr)
    assert not ro.flags.writeable
    with pytest.raises(ValueError):
        ro[0, 0] = 1.0
    with pytest.raises(ValueError):
        ro.flags.writeable = True
    with pytest.raises(ValueError):
        ro[1:].flags.writeable = True
    a[0, 0] = 3.0
    assert ro[0, 0] == 3.0


def test_copy_mode_returns_independent_copies():
    addr, a = t.createArray(3, 2)
    aka.setArrayCopyMode(True)
    try:
        assert aka.getArrayCopyMode()
        c = t.getArray(addr)
        assert c.flags.owndata or c.base is None or not hasattr(
            c.base, "is_stale"
        )
        c[:] = 9.0
        assert t.nativeSum(addr) == 0.0
    finally:
        aka.setArrayCopyMode(False)
    v = t.getArray(addr)
    v[:] = 1.0
    assert t.nativeSum(addr) == 6.0


# --- staleness ---------------------------------------------------------------


def test_growth_beyond_capacity_makes_old_views_stale_but_safe():
    addr, a = t.createArray(4, 2)
    a[:] = 1.0
    t.resizeArray(addr, 10_000)
    assert lease(a).is_stale()
    # stale-but-safe: reads see the values at retirement
    assert np.all(a == 1.0)
    # writes no longer reach the array
    a[:] = 5.0
    assert t.nativeSum(addr) == 8.0
    # output-buffer entry points refuse a stale view
    with pytest.raises(aka.Exception, match="stale"):
        t.fillOutput(addr, a, 2.0)
    # re-fetching gives an attached view
    b = t.getArray(addr)
    assert b.shape == (10_000, 2)
    assert not lease(b).is_stale()
    t.fillOutput(addr, b, 2.0)
    assert np.all(b == 2.0)


def test_shrink_detaches_and_is_monotone():
    addr, a = t.createArray(6, 1)
    t.resizeArray(addr, 3)
    assert lease(a).is_stale()
    t.resizeArray(addr, 6)
    assert lease(a).is_stale()
    assert t.getArray(addr).shape == (6, 1)


def test_views_survive_owner_destruction():
    # the test registry keeps its arrays; a model or mesh deleted from Python
    # is exercised in test_mesh.py
    addr, a = t.createArray(2, 2)
    a[:] = 4.0
    b = a[1:]
    del a
    gc.collect()
    assert np.all(b == 4.0)


# --- zero-component arrays -----------------------------------------


def test_zero_component_arrays_keep_shape_and_detach_on_shrink():
    addr, a = t.createArray(5, 0)
    assert a.shape == (5, 0)
    assert a[1:3].shape == (2, 0)
    assert np.array(a).shape == (5, 0)
    assert t.copyArray(addr).shape == (5, 0)
    t.resizeArray(addr, 7)
    b = t.getArray(addr)
    assert b.shape == (7, 0)
    t.resizeArray(addr, 2)
    assert lease(b).is_stale()
    assert t.getArray(addr).shape == (2, 0)


def test_empty_arrays():
    addr, a = t.createArray(0, 3)
    assert a.shape == (0, 3)
    assert t.copyArray(addr).shape == (0, 3)


# --- callbacks and the GIL ---------------------------------------------------


def test_callback_reenters_rust_while_a_view_is_held():
    addr, a = t.createArray(3, 1)
    seen = []

    def cb():
        # foreign access through a view held across the native call
        a[:] = 2.0
        # nested binding call (native section inside the callback)
        seen.append(t.nativeSum(addr))
        b = t.getArray(addr)
        b[0, 0] = 5.0
        return "done"

    assert t.callFromNative(cb) == "done"
    assert seen == [6.0]
    assert t.nativeSum(addr) == 9.0
    assert a[0, 0] == 5.0


def test_callback_refused_while_a_native_guard_is_alive():
    addr, a = t.createArray(2, 1)
    called = []
    with pytest.raises(aka.Exception):
        t.callWhileGuarded(addr, lambda: called.append(1))
    assert called == []
    # the runtime recovered: views work and native calls succeed
    a[:] = 1.0
    assert t.nativeSum(addr) == 2.0
    assert not t.runtimeBusy()


def test_reentrant_mutable_access_to_the_same_owner_raises():
    addr, a = t.createArray(2, 1)
    other, _ = t.createArray(2, 1)
    errors = []

    def cb():
        try:
            t.nativeFill(addr, 3.0)
        except aka.Exception as e:
            errors.append(str(e))
        # a different owner is fine
        t.nativeFill(other, 1.0)
        # views of the borrowed owner remain usable (foreign phase)
        a[:] = 4.0

    t.callWhileBorrowed(addr, cb)
    assert len(errors) == 1 and "already borrowed" in errors[0]
    assert t.nativeSum(addr) == 8.0
    assert t.nativeSum(other) == 2.0


def test_python_exception_in_callback_keeps_its_type():
    class MyError(Exception):
        pass

    def cb():
        raise MyError("from python")

    with pytest.raises(MyError, match="from python"):
        t.callFromNative(cb)
    # nested: a callback inside a callback
    with pytest.raises(KeyError):
        t.callFromNative(lambda: t.callFromNative(lambda: {}["x"]))
    assert not t.runtimeBusy()


def test_swallowed_callback_exception_is_not_reraised_later():
    # a Python exception raised in a callback whose
    # error native code swallows must not be re-raised by a later,
    # unrelated error whose message happens to contain the carried message
    def boom():
        raise ValueError("v62-carried")

    assert t.swallowCallbackError(boom) is True  # the call itself succeeds
    mesh = aka.Mesh(2)
    with pytest.raises(aka.Exception) as e:
        mesh.getElementGroup("Python callback raised ValueError: v62-carried")
    assert not isinstance(e.value, ValueError)
    assert not t.runtimeBusy()


def test_native_errors_map_to_upstream_exceptions():
    with pytest.raises(aka.Exception, match="raised from native code"):
        t.raiseNative("invalid")
    with pytest.raises(aka.NLSNotConvergedException):
        t.raiseNative("not_converged")
    assert not issubclass(aka.NLSNotConvergedException, aka.Exception)
    assert aka.Exception.__module__ == "akantu.py11_akantu"


def test_threads_serialise_on_the_gil():
    addr, a = t.createArray(1000, 1)
    errors = []

    def work():
        try:
            for _ in range(200):
                b = t.getArray(addr)
                b[:] += 1.0
                t.nativeSum(addr)
                t.callFromNative(lambda: t.nativeFill(addr, 1.0))
        except Exception as e:  # pragma: no cover - reported below
            errors.append(e)

    threads = [threading.Thread(target=work) for _ in range(4)]
    for th in threads:
        th.start()
    for th in threads:
        th.join(timeout=60)
    assert not any(th.is_alive() for th in threads)
    assert errors == []
    assert not t.runtimeBusy()
