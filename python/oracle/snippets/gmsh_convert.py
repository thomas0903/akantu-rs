# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs (PY-095) oracle snippet:
# `akantu.convertGmshToAkantuMesh(gmsh)` (python/akantu/__init__.py, upstream
# code; the port runs it with `upstream_compat=True`, which is upstream's
# code unchanged) on gmsh models built through the gmsh Python API. Run with
# both modules in the same image (the gmsh Python module installed, e.g.
# Debian's python3-gmsh); compared with compare.py at tolerance 0.
#
# Upstream's function tests `if not types:` on the NumPy array the gmsh API
# returns (gmsh 4.13.1, NumPy 2.2.4): a model with more than one element
# type -- or none -- raises `ValueError` ("The truth value of an array ...
# is ambiguous") before converting anything (UD-1172). It also passes a
# physical group's tag as an entity tag (`get_element_types(*group)`): a
# group whose tag is not also the tag of an entity of its dimension raises
# "Surface <n> does not exist" (UD-1173). Both are category B:
# the port's default corrects them, its compat switch reproduces
# them, and this snippet compares the compat switch with the oracle. The
# cases:
# * `single`: one element type (quadrangles on two discrete surface
#   entities, physical groups `left` = 1 and `right` = 2 whose tags are
#   those of their entities), nodes and elements added by hand (no
#   meshing algorithm), so the gmsh model -- and the Akantu mesh built from
#   it -- does not depend on the platform; python/tests/test_gmsh_conversion.py
#   imports `single_type_model` / `describe` from this file and compares
#   with the oracle's recorded output (fixtures/python/gmsh_convert/oracle.txt);
# * `pyramid`, `tri10`: one unsupported element type (the `ValueError` and
#   `IndexError` branches);
# * `multi`, `empty`, `meshed` (a rectangle meshed by gmsh: points, lines and
#   triangles): the UD-1172 failure;
# * `group_tag`: `single` plus a group `all` (tag 3) of both entities: the
#   UD-1173 failure.
import inspect

import numpy as np

# gmsh element type ids (gmsh's MSH numbering)
LINE2, TRI3, QUAD4, PYRAMID5, POINT1, TRI10 = 1, 2, 3, 7, 15, 21


def _nodes(gmsh, entity_dim, entity, tags, xy):
    coords = []
    for x, y in xy:
        coords += [x, y, 0.0]
    gmsh.model.mesh.addNodes(entity_dim, entity, tags, coords)


def single_type_model(gmsh, only_type=QUAD4, all_group=False):
    """Quadrangles on two discrete surface entities, node and element tags
    deliberately not contiguous, physical groups `left` (tag 1, entity 1)
    and `right` (tag 2, entity 2); `all_group` adds `all` (tag 3, both
    entities). `only_type` replaces the quadrangles by one element of
    another type (the unsupported-type branches)."""
    gmsh.model.add("py095")
    s1 = gmsh.model.addDiscreteEntity(2)
    s2 = gmsh.model.addDiscreteEntity(2)
    _nodes(gmsh, 2, s1, [10, 11, 14, 15, 17, 18],
           [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.25), (1.0, 2.0),
            (0.0, 2.0)])
    _nodes(gmsh, 2, s2, [12, 16, 19], [(2.5, 0.0), (2.5, 1.0), (2.5, 2.0)])
    if only_type == QUAD4:
        gmsh.model.mesh.addElementsByType(s1, QUAD4, [101, 105],
                                          [10, 11, 15, 14, 14, 15, 17, 18])
        gmsh.model.mesh.addElementsByType(s2, QUAD4, [203, 202],
                                          [15, 16, 19, 17, 11, 12, 16, 15])
    elif only_type == PYRAMID5:
        vol = gmsh.model.addDiscreteEntity(3)
        _nodes(gmsh, 3, vol, [30], [(0.5, 0.5)])
        gmsh.model.mesh.addElementsByType(vol, PYRAMID5, [500],
                                          [10, 11, 15, 14, 30])
    elif only_type == TRI10:
        gmsh.model.mesh.addElementsByType(s1, TRI10, [600],
                                          [10, 11, 14, 15, 17, 18, 10, 11,
                                           14, 15])
    gmsh.model.addPhysicalGroup(2, [s1], tag=1, name="left")
    gmsh.model.addPhysicalGroup(2, [s2], tag=2, name="right")
    if all_group:
        gmsh.model.addPhysicalGroup(2, [s1, s2], tag=3, name="all")


def multi_type_model(gmsh):
    """Quadrangles, triangles, segments and a point (four element types)."""
    gmsh.model.add("py095_multi")
    surf = gmsh.model.addDiscreteEntity(2)
    curve = gmsh.model.addDiscreteEntity(1)
    point = gmsh.model.addDiscreteEntity(0)
    _nodes(gmsh, 2, surf, [10, 11, 12, 14, 15, 16, 17, 18],
           [(0.0, 0.0), (1.0, 0.0), (2.5, 0.0), (0.0, 1.0), (1.0, 1.25),
            (2.5, 1.0), (1.0, 2.0), (0.0, 2.0)])
    gmsh.model.mesh.addElementsByType(surf, QUAD4, [101, 105],
                                      [10, 11, 15, 14, 11, 12, 16, 15])
    gmsh.model.mesh.addElementsByType(surf, TRI3, [203, 202],
                                      [14, 15, 17, 14, 17, 18])
    gmsh.model.mesh.addElementsByType(curve, LINE2, [301, 300],
                                      [10, 11, 11, 12])
    gmsh.model.mesh.addElementsByType(point, POINT1, [400], [16])
    gmsh.model.addPhysicalGroup(2, [surf], name="body")
    gmsh.model.addPhysicalGroup(1, [curve], name="bottom")
    gmsh.model.addPhysicalGroup(0, [point], name="corner")


def describe(aka, mesh):
    """(key, value) pairs of everything the conversion produced (nodes,
    connectivities, element groups with their elements and nodes)."""
    items = [("dim", int(mesh.getNodes().shape[1])),
             ("nb_nodes", mesh.getNbNodes()),
             ("nodes", np.array(mesh.getNodes()))]
    for t in mesh.getConnectivities().elementTypes():
        items.append((f"conn.{t}", np.array(mesh.getConnectivity(t))))
    groups = sorted(mesh.iterateElementGroups(), key=lambda g: g.getName())
    for g in groups:
        name = g.getName()
        els = g.getElements()
        for t in els.elementTypes():
            items.append((f"group.{name}.{t}", np.array(els(t))))
        items.append((f"group.{name}.nodes",
                      np.array(g.getNodeGroup().getNodes())))
    return items


def convert_upstream(aka, gmsh):
    """Upstream's conversion: the oracle module's function, or the port's
    with its compat switch (`upstream_compat=True`, UD-1172 / UD-1173)."""
    f = aka.convertGmshToAkantuMesh
    if "upstream_compat" in inspect.signature(f).parameters:
        return f(gmsh, upstream_compat=True)
    return f(gmsh)


def attempt(aka, gmsh, convert=convert_upstream):
    """`convertGmshToAkantuMesh` on the current gmsh model (upstream's
    unless `convert` says otherwise): its description, or the exception
    (type and message)."""
    try:
        return describe(aka, convert(aka, gmsh)), None
    except Exception as e:  # upstream raises ValueError / IndexError
        return None, f"{type(e).__name__}: {e}"


def main():
    import gmsh

    import akantu as aka
    from _out import out

    gmsh.initialize()
    gmsh.option.setNumber("General.Terminal", 0)
    cases = [
        ("single", lambda: single_type_model(gmsh)),
        ("pyramid", lambda: single_type_model(gmsh, PYRAMID5)),
        ("tri10", lambda: single_type_model(gmsh, TRI10)),
        ("group_tag", lambda: single_type_model(gmsh, all_group=True)),
        ("multi", lambda: multi_type_model(gmsh)),
        ("empty", lambda: gmsh.model.add("empty")),
    ]
    for name, build in cases:
        build()
        items, error = attempt(aka, gmsh)
        if error is not None:
            out(f"{name}.error", error)
        else:
            for k, v in items:
                out(f"{name}.{k}", v)
        gmsh.model.remove()

    gmsh.model.add("meshed")
    gmsh.model.occ.addRectangle(0.0, 0.0, 0.0, 2.0, 1.0)
    gmsh.model.occ.synchronize()
    gmsh.model.addPhysicalGroup(2, [1], name="plate")
    gmsh.model.addPhysicalGroup(1, [1, 2, 3, 4], name="boundary")
    gmsh.option.setNumber("Mesh.MeshSizeMax", 0.4)
    gmsh.model.mesh.generate(2)
    items, error = attempt(aka, gmsh)
    out("meshed.error", error)
    gmsh.finalize()


if __name__ == "__main__":
    main()
