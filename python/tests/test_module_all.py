# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs tests: the module namespace of
# python/akantu/__init__.py (upstream code, copied unchanged) copies every
# name of `dir(py11_akantu)`, `__all__` included. PyO3 builds an `__all__` for
# the extension module (`PyModule::add`), where upstream's pybind11 module
# defines none, and five classes that are registered under upstream's
# templated names (`ConstitutiveLawakantu::DiffusionModel`, ...) after being
# added under their Rust-side names left five stale entries in it, so that
# `from akantu import *` raised AttributeError and pdoc could not resolve
# five names.
"""No stale names in the module namespace; the star import works."""

import akantu
import akantu.py11_akantu as ext

TEMPLATED_NAMES = (
    "ConstitutiveLawakantu::SolidMechanicsModel",
    "ConstitutiveLawsHandlerakantu::Materialakantu::Model",
    "ConstitutiveLawakantu::DiffusionModel",
    "ConstitutiveLawsHandlerakantu::DiffusionLawakantu::Model",
    "ConstitutiveLawsHandlerakantu::PhaseFieldakantu::Model",
    "ConstitutiveLawakantu::PhaseFieldModel",
)


def test_extension_module_defines_no_all():
    # pybind11's module has none: a star import takes every public name
    assert not hasattr(ext, "__all__")


def test_every_name_of_the_namespace_resolves():
    names = list(dir(akantu)) + list(getattr(akantu, "__all__", ()))
    missing = [n for n in names if not hasattr(akantu, n)]
    assert missing == []


def test_templated_class_names_are_registered_once():
    for name in TEMPLATED_NAMES:
        cls = getattr(akantu, name)
        assert cls.__name__ == name
    # the Rust-side names the five classes were created under are gone
    assert not hasattr(akantu, "ConstitutiveLaw_DiffusionModel")
    assert not hasattr(akantu, "ConstitutiveLawsHandler_Material_Model")


def test_star_import_works():
    namespace = {}
    exec("from akantu import *", namespace)
    assert "SolidMechanicsModel" in namespace
    assert "Mesh" in namespace
    for name in TEMPLATED_NAMES:
        assert name in namespace
