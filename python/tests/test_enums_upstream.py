# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs test (PY-010…PY-027). Pins every enum bound
# by upstream's Python module (names, integer values, `str`/`name`, class
# attributes, module-level aliases from `export_values()`) against the dump
# `fixtures/python/enums/upstream_enums.json`, derived from upstream's
# binding sources (python/py_aka_common.cc, py_aka_error.cc, py_parser.cc and
# the C++ enum definitions) by `fixtures/python/enums/dump_upstream_enums.py`.
"""Bound enums of the Python module against upstream's enum definitions."""

import json

import akantu as aka
import pytest
from akantu import py11_akantu as py11

import _paths

DUMP = json.loads(
    (_paths.REPO / "fixtures/python/enums/upstream_enums.json").read_text()
)
ENUMS = DUMP["enums"]


def _scope(entry):
    return (
        py11.RandomDistributionType
        if entry["scope"] == "RandomDistributionType"
        else py11
    )


def _first_name(entry, value):
    return entry["str_name_of_value"][str(value)]


def test_dump_covers_the_seventeen_enum_rows():
    # PY-010 .. PY-027 bind 17 enums + the debug sub-module (PY-026): the
    # dump has one entry per bound enum, in registration order.
    names = [e["name"] for e in ENUMS]
    assert len(names) == len(set(names)) == 18
    assert "RandomDistributionTypes" in names


@pytest.mark.parametrize("entry", ENUMS, ids=[e["name"] for e in ENUMS])
def test_members_names_values_and_str(entry):
    scope = _scope(entry)
    cls = getattr(scope, entry["name"])
    expected = {m["name"]: m["value"] for m in entry["members"]}

    # `__members__`: the same names with the same integer values
    members = dict(cls.__members__)
    assert {n: int(v) for n, v in members.items()} == expected
    # registration order, except upstream appends `_not_defined` of
    # ElementType last (`AKANTU_ALL_ELEMENT_TYPE(_not_defined)`) while the
    # port lists it first; that one name is excluded from the order check.
    up_order = [m["name"] for m in entry["members"] if m["name"] != "_not_defined"]
    rs_order = [n for n in members if n != "_not_defined"]
    assert rs_order == up_order

    for name, value in expected.items():
        member = getattr(cls, name)  # class attribute
        assert isinstance(member, cls)
        assert int(member) == value
        assert member.value == value
        first = _first_name(entry, value)
        assert member.name == first
        assert str(member) == f"{entry['name']}.{first}"
        # construction from the integer value gives the same member
        again = cls(value)
        assert int(again) == value
        assert again.name == first
        # members are hashable and usable as indices (pybind11 enums)
        assert hash(member) == hash(value)
        assert member.__index__() == value


@pytest.mark.parametrize(
    "entry", [e for e in ENUMS if e["scope"] == "module"], ids=lambda e: e["name"]
)
def test_class_enums_do_not_compare_to_integers(entry):
    # pybind11 `py::enum_` of a C++ `enum class` (`ModelType`, `ParserType`,
    # ...) is not convertible to int in comparisons; plain C enums are.
    cls = getattr(py11, entry["name"])
    m0 = entry["members"][0]
    member = getattr(cls, m0["name"])
    if entry["class_enum"]:
        assert member != m0["value"]
    else:
        assert member == m0["value"]
    assert member == cls(m0["value"])


def test_module_level_aliases_of_export_values():
    exports = DUMP["module_exports"]
    # every exported member is a module attribute of `py11` and of `akantu`
    # (the `__init__` copies the module's public names), resolving to the
    # LAST registered enum on a name collision (`aka.model` is
    # `ParserType.model`, `aka.auto` is `SparseSolverType.auto`, ...).
    for name, target in exports.items():
        for mod in (py11, aka):
            obj = getattr(mod, name)
            assert type(obj).__name__ == target["enum"], (mod.__name__, name)
            assert int(obj) == target["value"], (mod.__name__, name)
    assert type(py11.model).__name__ == "ParserType"
    assert type(py11.auto).__name__ == "SparseSolverType"
    assert type(py11.static).__name__ == "TimeStepSolverType"
    # access-type aliases (module-level `_pat_*`, arithmetic enum)
    assert int(py11._pat_readable | py11._pat_parsable) == 0x1100
    assert int(py11._pat_parsmod) == 0x1110


def test_random_distribution_members_live_in_the_submodule_only():
    entry = next(e for e in ENUMS if e["name"] == "RandomDistributionTypes")
    sub = py11.RandomDistributionType
    for m in entry["members"]:
        member = getattr(sub, m["name"])  # export_values() into the submodule
        assert type(member).__name__ == "RandomDistributionTypes"
        assert int(member) == m["value"]
        assert not hasattr(py11, m["name"])
