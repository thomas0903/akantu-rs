# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet: mesh read, shapes/dtypes, groups, data
# (upstream API as in test/test_python_interface/test_pybind.py and
# examples/python).
import akantu as aka

from _out import out

m = aka.Mesh(2)
m.read("mesh_dcb_2d.msh")
n = m.getNodes()
out("nodes.shape", list(n.shape))
out("nodes.dtype", str(n.dtype))
out("nodes.writeable", bool(n.flags.writeable))
out("nodes", n)
out("nb_nodes", m.getNbNodes())
out("nb_el.q8", m.getNbElement(element_type=aka._quadrangle_8))
out("nb_el.dim2", m.getNbElement(spatial_dimension=2))
out("nb_el.dim1", m.getNbElement(spatial_dimension=1))
c = m.getConnectivity(aka._quadrangle_8)
out("conn.q8", c)
out("conn.s3", m.getConnectivity(aka._segment_3))
cs = m.getConnectivities()
out("conns.id", cs.getID())
out("conns.types", [str(t) for t in cs.elementTypes()])
out("conns.types.dim1", [str(t) for t in cs.elementTypes(dim=1)])
out("egroups", [g.getName() for g in m.iterateElementGroups()])
out("ngroups", [g.getName() for g in m.iterateNodeGroups()])
for g in m.iterateElementGroups():
    out(f"group.{g.getName()}.len", len(g))
    out(f"group.{g.getName()}.nodes", g.getNodeGroup().getNodes())
    els = g.getElements()
    for t in els.elementTypes(dim=aka.Mesh.getSpatialDimension(aka._segment_3)):
        out(f"group.{g.getName()}.{t}", els(t))
    for t in els.elementTypes(dim=2):
        out(f"group.{g.getName()}.{t}", els(t))
out("lower", m.getLowerBounds())
out("upper", m.getUpperBounds())
out("dim.tri3", aka.Mesh.getSpatialDimension(aka._triangle_3))
m.fillNodesToElements()
lst = []
m.getAssociatedElements(10, lst)
out("assoc10", [str(e) for e in lst])
f = m.initMeshFacets()
out("facets.nb_el.dim1", f.getNbElement(spatial_dimension=1))
out("facets.conn.s3", f.getConnectivity(aka._segment_3))
m2 = aka.Mesh(2)
m2.read("mesh_dcb_2d.msh")
aka.MeshUtils.buildFacets(m2)
m2.createBoundaryGroupFromGeometry()
out("bfacets.nb_el.dim1", m2.getNbElement(spatial_dimension=1))
out("bgroups", [(g.getName(), len(g.getNodeGroup())) for g in m2.iterateElementGroups()])
out("boundary_0.nodes", m2.getElementGroup("boundary_0").getNodeGroup().getNodes())
e = aka.ElementTypeMapArrayReal()
e.initialize(m, nb_component=3, with_nb_element=True, default_value=1.5)
out("etma.id", e.getID())
out("etma.types", [str(t) for t in e.elementTypes()])
out("etma.q8.shape", list(e(aka._quadrangle_8).shape))
