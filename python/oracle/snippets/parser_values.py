# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs oracle snippet: parser values through the Python API
# (python/py_parser.cc; test/test_io/test_parser/test_parser.py).
import akantu as aka

from _out import out

aka.parseInput("input_file.dat")
p = aka.getStaticParser()
for sec in [aka.ParserType.material, aka.ParserType.model, aka.ParserType.mesh]:
    try:
        s = p.getSubSections(sec)
        out(f"section.{sec}.E", s.getParameter("E").asReal())
        out(f"section.{sec}.X135", s.getParameter("X135").asReal())
    except Exception as e:
        out(f"section.{sec}.error", type(e).__name__)
for name, kind in [("toto", "asReal"), ("general", "asInt"), ("vect", "asVector"),
                   ("mat", "asMatrix"), ("rand1", "asReal")]:
    try:
        v = getattr(p.getParameter(name), kind)()
        out(f"param.{name}", v)
    except Exception as e:
        out(f"param.{name}.error", type(e).__name__)
