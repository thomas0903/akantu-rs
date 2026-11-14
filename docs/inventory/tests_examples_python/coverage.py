#!/usr/bin/env python3
# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs inventory helper (R4). Not ported code; it scans the Akantu
# reference tree (EPFL/LSMS) and maps files to inventory IDs.
"""Map every file of python/, examples/, test/, extra_packages/*/test to inventory IDs.

Usage: coverage.py <akantu_root> <out.md>
Prints unmapped files on stderr and exits 1 if any.
"""
import os
import re
import sys

root = sys.argv[1]
out = sys.argv[2]

files = []
for top in ["python", "examples", "test"]:
    for dp, dn, fn in os.walk(os.path.join(root, top)):
        for f in fn:
            files.append(os.path.relpath(os.path.join(dp, f), root))
for pkg in sorted(os.listdir(os.path.join(root, "extra_packages"))):
    t = os.path.join(root, "extra_packages", pkg, "test")
    if os.path.isdir(t):
        for dp, dn, fn in os.walk(t):
            for f in fn:
                files.append(os.path.relpath(os.path.join(dp, f), root))
for f in ["setup.py", "pyproject.toml", "setup.cfg", "MANIFEST.in",
          "requirements.txt", "dev-requirements.txt"]:
    files.append(f)
files = sorted(set(files))

# ordered rules: (regex, ids); first match wins
R = []


def r(pat, ids):
    R.append((re.compile(pat), ids))


# ---------------------------------------------------------------- packaging
r(r"^(setup\.py|pyproject\.toml|setup\.cfg|MANIFEST\.in|requirements\.txt|dev-requirements\.txt)$", "PY-001")
# ---------------------------------------------------------------- python/
r(r"^python/CMakeLists\.txt$", "PY-002")
r(r"^python/py_akantu_pybind11_compatibility\.hh$", "PY-002")
r(r"^python/akantu/__init__\.py$", "PY-003, PY-090, PY-094, PY-095, PY-096")
r(r"^python/py_aka_array\.hh$", "PY-005, PY-006")
r(r"^python/py_aka_common\.(cc|hh)$", "PY-004, PY-010…PY-025, PY-030")
r(r"^python/py_aka_error\.(cc|hh)$", "PY-026")
r(r"^python/py_akantu\.(cc|hh)$", "PY-004, PY-007")
r(r"^python/py_boundary_conditions\.(cc|hh)$", "PY-069")
r(r"^python/py_constitutive_law\.(cc|hh)$", "PY-071, PY-072, PY-073")
r(r"^python/py_constitutive_law_selector\.(cc|hh)$", "PY-070")
r(r"^python/py_constitutive_laws_handler\.hh$", "PY-074")
r(r"^python/py_contact_mechanics_model\.(cc|hh)$", "PY-086, PY-087, PY-088")
r(r"^python/py_dof_manager\.(cc|hh)$", "PY-060…PY-064")
r(r"^python/py_dumpable\.(cc|hh)$", "PY-048, PY-049")
r(r"^python/py_fe_engine\.(cc|hh)$", "PY-056, PY-057")
r(r"^python/py_fragment_manager\.(cc|hh)$", "PY-082")
r(r"^python/py_group_manager\.(cc|hh)$", "PY-050, PY-051, PY-052")
r(r"^python/py_heat_transfer_model\.(cc|hh)$", "PY-083, PY-084")
r(r"^python/py_integration_scheme\.(cc|hh)$", "PY-028, PY-058")
r(r"^python/py_material\.(cc|hh)$", "PY-076, PY-077, PY-078, PY-079")
r(r"^python/py_mesh\.(cc|hh)$", "PY-053, PY-054, PY-055")
r(r"^python/py_model\.(cc|hh)$", "PY-065…PY-068")
r(r"^python/py_model_couplers\.(cc|hh)$", "PY-089")
r(r"^python/py_parser\.(cc|hh)$", "PY-027, PY-031, PY-040…PY-043")
r(r"^python/py_phase_field_model\.(cc|hh)$", "PY-091, PY-093")
r(r"^python/py_phasefield\.(cc|hh)$", "PY-092")
r(r"^python/py_solid_mechanics_model\.(cc|hh)$", "PY-075")
r(r"^python/py_solid_mechanics_model_cohesive\.(cc|hh)$", "PY-080, PY-081")
r(r"^python/py_solver\.(cc|hh)$", "PY-044…PY-047")
r(r"^python/py_structural_mechanics_model\.(cc|hh)$", "PY-085")
# ---------------------------------------------------------------- examples
E = "examples/"
r(r"^examples/(c\+\+/|python/)?README\.rst$", "EX-050")
r(r"^examples/(c\+\+/|python/)?CMakeLists\.txt$", "EX-029")
r(r"^examples/c\+\+/[^/]+/(README\.rst)$", "EX-050")
r(r"^examples/python/[^/]+/(README\.rst)$", "EX-050")
r(r"^examples/c\+\+/[^/]+/CMakeLists\.txt$", "EX-029")
r(r"^examples/python/[^/]+/CMakeLists\.txt$", "EX-029")
# contact c++
r(r"^examples/c\+\+/contact_mechanics_model/hertz/contact_explicit_dynamic\.cc$", "EX-001")
r(r"^examples/c\+\+/contact_mechanics_model/hertz/contact_explicit_static\.cc$", "EX-002")
r(r"^examples/c\+\+/contact_mechanics_model/hertz/(cohesive_contact_explicit_dynamic\.cc|cohesive-contact\.geo|material-cohesive\.dat)$", "EX-003")
r(r"^examples/c\+\+/contact_mechanics_model/hertz/(hertz\.geo|material\.dat)$", "EX-001, EX-002")
r(r"^examples/c\+\+/contact_mechanics_model/hertz/(CMakeLists\.txt)$", "EX-001, EX-002, EX-003, EX-029")
r(r"^examples/c\+\+/contact_mechanics_model/hertz/(README\.rst|images/.*)$", "EX-001, EX-050")
r(r"^examples/c\+\+/contact_mechanics_model/friction/(CMakeLists\.txt)$", "EX-004, EX-029")
r(r"^examples/c\+\+/contact_mechanics_model/friction/(README\.rst|images/.*)$", "EX-004, EX-050")
r(r"^examples/c\+\+/contact_mechanics_model/friction/", "EX-004")
# diffusion
r(r"^examples/c\+\+/diffusion_model/heat_diffusion_dynamics_2d\.cc$", "EX-005")
r(r"^examples/c\+\+/diffusion_model/heat_diffusion_static_2d\.cc$", "EX-006")
r(r"^examples/c\+\+/diffusion_model/heat_diffusion_dynamics_3d\.cc$", "EX-007")
r(r"^examples/c\+\+/diffusion_model/square\.geo$", "EX-005, EX-006")
r(r"^examples/c\+\+/diffusion_model/cube\.geo$", "EX-007")
r(r"^examples/c\+\+/diffusion_model/cube_tet\.geo$", "EX-007 (unused), GOLD-015")
r(r"^examples/c\+\+/diffusion_model/material\.dat$", "EX-005, EX-006, EX-007")
r(r"^examples/c\+\+/diffusion_model/images/", "EX-005…EX-007, EX-050")
r(r"^examples/c\+\+/embedded_model/CMakeLists\.txt$", "EX-008, EX-029")
r(r"^examples/c\+\+/embedded_model/", "EX-008")
r(r"^examples/c\+\+/node_to_node/CMakeLists\.txt$", "EX-009, EX-029")
r(r"^examples/c\+\+/node_to_node/", "EX-009")
r(r"^examples/c\+\+/phase_field_model/phase_field_notch\.cc$", "EX-010")
r(r"^examples/c\+\+/phase_field_model/phase_field_parallel\.cc$", "EX-011")
r(r"^examples/c\+\+/phase_field_model/(square_notch\.geo|material_notch\.dat)$", "EX-010, EX-011")
r(r"^examples/c\+\+/phase_field_model/images/", "EX-010, EX-050")
r(r"^examples/c\+\+/scalability_test/(plot\.py|run\.sh|scalability\.sh)$", "EX-049")
r(r"^examples/c\+\+/scalability_test/images/", "EX-018, EX-050")
r(r"^examples/c\+\+/scalability_test/", "EX-018")
# cohesive c++
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_extrinsic/(README\.rst|images/.*)$", "EX-015, EX-050")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_extrinsic/CMakeLists\.txt$", "EX-015, EX-029")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_extrinsic/", "EX-015")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/(README\.rst|images/.*)$", "EX-016, EX-050")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/CMakeLists\.txt$", "EX-016, EX-029")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/", "EX-016")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_intrinsic/(README\.rst|images/.*)$", "EX-017, EX-050")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_intrinsic/CMakeLists\.txt$", "EX-017, EX-029")
r(r"^examples/c\+\+/solid_mechanics_cohesive_model/cohesive_intrinsic/", "EX-017")
# smm c++
S = r"^examples/c\+\+/solid_mechanics_model/"
r(S + r"boundary_conditions/(README\.rst|images/.*)$", "EX-013, EX-014, EX-050")
r(S + r"boundary_conditions/CMakeLists\.txt$", "EX-029")
r(S + r"boundary_conditions/python_user_defined_bc/CMakeLists\.txt$", "EX-012, EX-029")
r(S + r"boundary_conditions/python_user_defined_bc/", "EX-012")
r(S + r"boundary_conditions/predefined_bc/CMakeLists\.txt$", "EX-013, EX-029")
r(S + r"boundary_conditions/predefined_bc/", "EX-013")
r(S + r"boundary_conditions/user_defined_bc/CMakeLists\.txt$", "EX-014, EX-029")
r(S + r"boundary_conditions/user_defined_bc/", "EX-014")
for d, i in [("explicit", "EX-019"), ("implicit", "EX-020"), ("static", "EX-021"), ("parallel", "EX-022")]:
    r(S + d + r"/(README\.rst|images/.*)$", i + ", EX-050")
    r(S + d + r"/CMakeLists\.txt$", i + ", EX-029")
    r(S + d + r"/", i)
r(S + r"new_material/viscoelastic_maxwell/CMakeLists\.txt$", "EX-024, EX-029")
r(S + r"new_material/viscoelastic_maxwell/", "EX-024")
r(S + r"new_material/(README\.rst|images/.*)$", "EX-023, EX-050")
r(S + r"new_material/CMakeLists\.txt$", "EX-023, EX-024, EX-029")
r(S + r"new_material/", "EX-023")
r(S + r"io/(README\.rst|images/.*)$", "EX-025, EX-026, EX-027, EX-050")
r(S + r"io/CMakeLists\.txt$", "EX-029")
r(S + r"io/dumper/dumper_low_level\.cc$", "EX-025")
r(S + r"io/dumper/dumpable_interface\.cc$", "EX-026")
r(S + r"io/dumper/CMakeLists\.txt$", "EX-025, EX-026, EX-029")
r(S + r"io/dumper/", "EX-025, EX-026")
r(S + r"io/parser/CMakeLists\.txt$", "EX-027, EX-029")
r(S + r"io/parser/", "EX-027")
r(r"^examples/c\+\+/structural_mechanics_model/CMakeLists\.txt$", "EX-028, EX-029")
r(r"^examples/c\+\+/structural_mechanics_model/", "EX-028")
# python examples
P = r"^examples/python/"
r(P + r"solid_mechanics_model/stiffness_matrix/(README\.rst)$", "EX-030, EX-050")
r(P + r"solid_mechanics_model/stiffness_matrix/CMakeLists\.txt$", "EX-030, EX-029")
r(P + r"solid_mechanics_model/stiffness_matrix/", "EX-030")
r(P + r"solid_mechanics_model/dynamics/(README\.rst|images/.*)$", "EX-031, EX-050")
r(P + r"solid_mechanics_model/dynamics/CMakeLists\.txt$", "EX-031, EX-029")
r(P + r"solid_mechanics_model/dynamics/", "EX-031")
r(P + r"solid_mechanics_model/plate-hole/(README\.rst|images/.*)$", "EX-032, EX-050")
r(P + r"solid_mechanics_model/plate-hole/CMakeLists\.txt$", "EX-032, EX-029")
r(P + r"solid_mechanics_model/plate-hole/", "EX-032")
r(P + r"solid_mechanics_model/custom-material/(custom-material\.py|bar\.geo)$", "EX-033")
r(P + r"solid_mechanics_model/custom-material/(bi-material\.py|square\.geo)$", "EX-034")
r(P + r"solid_mechanics_model/custom-material/material\.dat$", "EX-033, EX-034")
r(P + r"solid_mechanics_model/custom-material/(README\.rst|images/.*)$", "EX-033, EX-034, EX-050")
r(P + r"solid_mechanics_model/custom-material/CMakeLists\.txt$", "EX-033, EX-034, EX-029")
r(P + r"solid_mechanics_model/solver_callback/(README\.rst)$", "EX-035, EX-050")
r(P + r"solid_mechanics_model/solver_callback/CMakeLists\.txt$", "EX-035, EX-029")
r(P + r"solid_mechanics_model/solver_callback/", "EX-035")
r(P + r"solid_mechanics_model/eigen_modes/(README\.rst|images/.*)$", "EX-036, EX-050")
r(P + r"solid_mechanics_model/eigen_modes/CMakeLists\.txt$", "EX-036, EX-029")
r(P + r"solid_mechanics_model/eigen_modes/", "EX-036")
r(P + r"solid_mechanics_cohesive_model/cohesive/(custom_material\.py|local_material\.dat)$", "EX-038")
r(P + r"solid_mechanics_cohesive_model/cohesive/plate\.geo$", "EX-037, EX-038")
r(P + r"solid_mechanics_cohesive_model/cohesive/(README\.rst|images/.*)$", "EX-037, EX-050")
r(P + r"solid_mechanics_cohesive_model/cohesive/CMakeLists\.txt$", "EX-037, EX-029")
r(P + r"solid_mechanics_cohesive_model/cohesive/", "EX-037")
r(P + r"solid_mechanics_cohesive_model/fragmentation/(README\.rst|images/.*)$", "EX-039, EX-050")
r(P + r"solid_mechanics_cohesive_model/fragmentation/CMakeLists\.txt$", "EX-039, EX-029")
r(P + r"solid_mechanics_cohesive_model/fragmentation/", "EX-039")
r(P + r"solid_mechanics_cohesive_model/multi-material/(README\.rst)$", "EX-040, EX-050")
r(P + r"solid_mechanics_cohesive_model/multi-material/CMakeLists\.txt$", "EX-040, EX-029")
r(P + r"solid_mechanics_cohesive_model/multi-material/", "EX-040")
r(P + r"phase_field_model/(phasefield-static\.py|plate_static\.geo|material_static\.dat)$", "EX-041")
r(P + r"phase_field_model/(phasefield-dynamic\.py|plate\.geo|material\.dat)$", "EX-042")
r(P + r"phase_field_model/images/phasefield-static", "EX-041, EX-050")
r(P + r"phase_field_model/images/phasefield-dynamic", "EX-042, EX-050")
r(P + r"phase_field_model/(README\.rst)$", "EX-041, EX-042, EX-050")
r(P + r"phase_field_model/CMakeLists\.txt$", "EX-041, EX-042, EX-029")
r(P + r"contact_mechanics_model/compression/(detection-explicit\..*)$", "EX-044")
r(P + r"contact_mechanics_model/compression/(README\.rst|images/.*)$", "EX-043, EX-050")
r(P + r"contact_mechanics_model/compression/CMakeLists\.txt$", "EX-043, EX-044, EX-029")
r(P + r"contact_mechanics_model/compression/", "EX-043")
r(P + r"contact_mechanics_model/friction/(README\.rst)$", "EX-045, EX-050")
r(P + r"contact_mechanics_model/friction/CMakeLists\.txt$", "EX-045, EX-029")
r(P + r"contact_mechanics_model/friction/", "EX-045")
r(P + r"structural_mechanics_model/structural_mechanics\.py$", "EX-046")
r(P + r"structural_mechanics_model/structural_mechanics_softening\.py$", "EX-047")
r(P + r"structural_mechanics_model/structural_mechanics_dynamics\.py$", "EX-048")
r(P + r"structural_mechanics_model/images/displacements_softening", "EX-047, EX-050")
r(P + r"structural_mechanics_model/images/displacements\.png", "EX-046, EX-050")
r(P + r"structural_mechanics_model/CMakeLists\.txt$", "EX-046, EX-047, EX-048, EX-029")
# ---------------------------------------------------------------- test/ci
r(r"^test/ci/code_quality:latest/", "CI-015")
r(r"^test/ci/debian:stable/", "CI-011")
r(r"^test/ci/ubuntu:lts/", "CI-012")
r(r"^test/ci/manylinux_2_28_x86_64:latest/", "CI-013")
r(r"^test/ci/macosx/", "CI-014")
r(r"^test/ci/ctest2junit\.xsl$", "CI-020")
r(r"^test/ci/ompi_init\.supp$", "CI-025")
r(r"^test/ci/includes_for_ci/", "TEST-234")
r(r"^test/python_fe/", "TEST-232")
r(r"^test/(CMakeLists\.txt|test_gtest_main\.cc|test_gtest_utils\.hh)$", "TEST-233")
# ---------------------------------------------------------------- test_common
T = r"^test/test_common/"
r(T + r"(test_grid\.cc|circle\.geo)$", "TEST-001")
r(T + r"test_csr\.cc$", "TEST-002")
r(T + r"test_array\.cc$", "TEST-003")
r(T + r"test_tensors\.cc$", "TEST-004")
r(T + r"test_voigt_helper\.cc$", "TEST-005")
r(T + r"CMakeLists\.txt$", "TEST-001…TEST-005, TEST-233")
# ---------------------------------------------------------------- test_fe_engine
F = r"^test/test_fe_engine/"
r(F + r"test_mesh_data\.cc$", "TEST-010, TEST-011")
r(F + r"test_mesh_boundary\.cc$", "TEST-012")
r(F + r"(cube|cube_physical_names)\.geo$", "TEST-012, TEST-013")
r(F + r"test_facet_element_mapping\.cc$", "TEST-013")
r(F + r"test_fe_engine_precomputation\.cc$", "TEST-014")
r(F + r"py_engine/", "TEST-014, TEST-232")
r(F + r"(test_fe_engine_precomputation_structural\.cc|test_fe_engine_structural_fixture\.hh)$", "TEST-015")
r(F + r"_(bernoulli_beam_2|bernoulli_beam_3|discrete_kirchhoff_triangle_18)\.msh$", "TEST-015, MDATA-001")
r(F + r"test_fe_engine_inradius\.cc$", "TEST-016")
r(F + r"_\w+\.csv$", "TEST-016, GOLD-002")
r(F + r"test_fe_engine_gauss_integration\.cc$", "TEST-017")
r(F + r"test_gradient\.cc$", "TEST-018")
r(F + r"test_integrate\.cc$", "TEST-019")
r(F + r"test_inverse_map\.cc$", "TEST-020")
r(F + r"test_variational_operators\.cc$", "TEST-021")
r(F + r"test_fe_engine_fixture\.hh$", "TEST-014, TEST-016…TEST-021, TEST-210")
r(F + r"_\w+\.msh$", "TEST-014…TEST-021, MDATA-001")
r(F + r"test_interpolate\.cc$", "TEST-210")
r(F + r"test_interpolate_bernoulli_beam_2\.(cc)$", "TEST-211")
r(F + r"test_interpolate_bernoulli_beam_2\.verified$", "TEST-211, GOLD-003")
r(F + r"test_fe_engine_precomputation_bernoulli_[23]\.cc$", "TEST-212")
r(F + r"generate_test$", "TEST-213")
r(F + r"\w+\.geo$", "MDATA-001 (source geometries of committed element meshes; not used by CMake)")
r(F + r"CMakeLists\.txt$", "TEST-010…TEST-021, TEST-233")
# ---------------------------------------------------------------- test_geometry
G = r"^test/test_geometry/"
r(G + r"test_geometry_predicates\.cc$", "TEST-025")
r(G + r"test_geometry_intersection\.cc$", "TEST-026")
r(G + r"test_segment_intersection_triangle_3\.cc$", "TEST-027")
r(G + r"test_segment_intersection_tetrahedron_4\.cc$", "TEST-028")
r(G + r"test_geometry_triangle\.msh$", "TEST-026, TEST-027, MDATA-005")
r(G + r"test_geometry_tetrahedron\.msh$", "TEST-028, MDATA-005")
r(G + r"CMakeLists\.txt$", "TEST-025…TEST-028, TEST-233")
# ---------------------------------------------------------------- test_io
r(r"^test/test_io/CMakeLists\.txt$", "TEST-233")
r(r"^test/test_io/test_dumper/test_dumper\.msh$", "TEST-030, MDATA-005")
r(r"^test/test_io/test_dumper/CMakeLists\.txt$", "TEST-030, TEST-233")
r(r"^test/test_io/test_dumper/", "TEST-030")
I = r"^test/test_io/test_parser/"
r(I + r"test_parser\.cc$", "TEST-031")
r(I + r"test_parser\.verified$", "TEST-031, GOLD-001")
r(I + r"test_parser_gtest\.cc$", "TEST-032")
r(I + r"test_parser\.py$", "TEST-033")
r(I + r"input_file\.dat$", "TEST-031, TEST-033")
r(I + r"CMakeLists\.txt$", "TEST-031…TEST-033, TEST-233")
# ---------------------------------------------------------------- test_mesh_utils
M = r"^test/test_mesh_utils/"
r(M + r"CMakeLists\.txt$", "TEST-035, TEST-036, TEST-233")
r(M + r"(test_purify_mesh\.cc|purify_mesh\.msh)$", "TEST-035")
r(M + r"(test_mesh_iterators\.cc|iterators_mesh\.geo)$", "TEST-036")
r(M + r"test_mesh_io/test_mesh_io_msh\.cc$", "TEST-037")
r(M + r"test_mesh_io/cube\.geo$", "TEST-037")
r(M + r"test_mesh_io/test_mesh_io_msh_physical_names\.(cc)$", "TEST-038")
r(M + r"test_mesh_io/cube_physical_names\.geo$", "TEST-038")
r(M + r"test_mesh_io/test_mesh_io_msh_physical_names\.verified$", "TEST-038, GOLD-001")
r(M + r"test_mesh_io/CMakeLists\.txt$", "TEST-037, TEST-038, TEST-233")
r(M + r"test_buildfacets/test_buildfacets_gtest\.verified$", "TEST-039, GOLD-001")
r(M + r"test_buildfacets/\w+\.msh$", "TEST-039, MDATA-002")
r(M + r"test_buildfacets/CMakeLists\.txt$", "TEST-039, TEST-233")
r(M + r"test_buildfacets/", "TEST-039")
r(M + r"test_mesh_partitionate/(test_mesh_partitionate_scotch\.cc|triangle\.geo)$", "TEST-040")
r(M + r"test_mesh_partitionate/(test_mesh_partitionate_mesh_data\.cc|quad\.geo)$", "TEST-041")
r(M + r"test_mesh_partitionate/CMakeLists\.txt$", "TEST-040, TEST-041, TEST-233")
r(M + r"test_mesh_periodic/(test_mesh_periodic\.cc|test_mesh_periodic_fixture\.hh|line\.geo|square\.geo)$", "TEST-042")
r(M + r"test_mesh_periodic/cube\.geo$", "TEST-042, TEST-043, TEST-044")
r(M + r"test_mesh_periodic/(test_periodic_static\.cc|test_periodic_static_fixture\.hh)$", "TEST-043")
r(M + r"test_mesh_periodic/material\.dat$", "TEST-043, TEST-044")
r(M + r"test_mesh_periodic/test_periodic_mass\.cc$", "TEST-044")
r(M + r"test_mesh_periodic/CMakeLists\.txt$", "TEST-042…TEST-044, TEST-233")
r(M + r"test_segment_nodetype/", "TEST-214")
# ---------------------------------------------------------------- test_model
r(r"^test/test_model/CMakeLists\.txt$", "TEST-233")
PT = r"^test/test_model/patch_tests/"
r(PT + r"patch_test_linear_elastic_explicit\.cc$", "TEST-050")
r(PT + r"patch_test_linear_elastic_implicit\.cc$", "TEST-051")
r(PT + r"patch_test_linear_anisotropic_explicit\.cc$", "TEST-052")
r(PT + r"patch_test_linear_anisotropic_implicit\.cc$", "TEST-053")
r(PT + r"test_lumped_mass\.cc$", "TEST-054")
r(PT + r"patch_test_linear_heat_transfer_explicit\.cc$", "TEST-055")
r(PT + r"patch_test_linear_heat_transfer_static\.cc$", "TEST-056")
r(PT + r"patch_test_linear_heat_transfer_implicit\.cc$", "TEST-057")
r(PT + r"(patch_test_linear_fixture\.hh|patch_test_linear_solid_mechanics_fixture\.hh)$", "TEST-050…TEST-054")
r(PT + r"patch_test_linear_heat_transfer_fixture\.hh$", "TEST-055…TEST-057")
r(PT + r"test_patch_linear_elastic_explicit\.py$", "TEST-058")
r(PT + r"test_patch_linear_elastic_static\.py$", "TEST-059")
r(PT + r"test_patch_linear_anisotropic_explicit\.py$", "TEST-060")
r(PT + r"test_patch_linear_heat_transfer_explicit\.py$", "TEST-061")
r(PT + r"test_patch_linear_heat_transfer_static\.py$", "TEST-062")
r(PT + r"test_patch_linear_heat_transfer_implicit\.py$", "TEST-063")
r(PT + r"patch_test_linear_fixture\.py$", "TEST-058…TEST-063")
r(PT + r"patch_test_linear_solid_mechanics_fixture\.py$", "TEST-058…TEST-060")
r(PT + r"patch_test_linear_heat_transfer_fixture\.py$", "TEST-061…TEST-063")
r(PT + r"data/_\w+\.msh$", "MDATA-003 (TEST-050…TEST-063)")
r(PT + r"data/bar_\w+\.geo$", "MDATA-003 (TEST-109, TEST-128…TEST-131)")
r(PT + r"data/material_check_stress_\w+\.dat$", "TEST-050, TEST-051, TEST-058, TEST-059")
r(PT + r"data/material_anisotropic_\d\.dat$", "TEST-052, TEST-053, TEST-060")
r(PT + r"data/material_lumped\.dat$", "TEST-054")
r(PT + r"data/heat_transfer_input\.dat$", "TEST-055…TEST-057, TEST-061…TEST-063")
r(PT + r"data/CMakeLists\.txt$", "MDATA-003, TEST-233")
r(PT + r"CMakeLists\.txt$", "TEST-050…TEST-063, TEST-233")
C = r"^test/test_model/test_common/"
r(C + r"test_dof_manager\.cc$", "TEST-070")
r(C + r"test_dof_mesh_distribute\.cc$", "TEST-071")
r(C + r"mesh\.geo$", "TEST-070, TEST-071")
r(C + r"CMakeLists\.txt$", "TEST-070, TEST-071, TEST-233")
MS = C + r"test_model_solver/"
r(MS + r"test_dof_manager_default\.cc$", "TEST-072")
r(MS + r"test_dof_manager_default\.verified$", "TEST-072, GOLD-001")
r(MS + r"(test_model_solver\.cc|test_model_solver_my_model\.hh)$", "TEST-073, TEST-074")
r(MS + r"test_model_solver_(mumps|petsc)\.verified$", "TEST-073, GOLD-001")
r(MS + r"test_model_solver_dynamic\.cc$", "TEST-074")
r(MS + r"test_model_solver_dynamic_\w+\.verified$", "TEST-074, GOLD-001")
r(MS + r"test_model_solver_dynamic_petsc\.cc$", "TEST-231")
r(MS + r"test_model_solver(_dynamic)?\.py$", "TEST-232")
r(MS + r"CMakeLists\.txt$", "TEST-072…TEST-074, TEST-233")
NL = C + r"test_non_local_toolbox/"
r(NL + r"(test_non_local_neighborhood_base\.cc|material\.dat|plot_neighborhoods\.py|my_model\.hh)$", "TEST-075")
r(NL + r"test_non_local_neighborhood_base\.verified$", "TEST-075, GOLD-001")
r(NL + r"(test_weight_computation\.cc|material_weight_computation\.dat)$", "TEST-076")
r(NL + r"test_weight_computation\.verified$", "TEST-076, GOLD-001")
r(NL + r"(test_non_local_averaging\.cc|material_avg\.dat)$", "TEST-077")
r(NL + r"test_material\.(cc|hh)$", "TEST-077, TEST-079")
r(NL + r"test_remove_damage_weight_function\.cc$", "TEST-078")
r(NL + r"test_remove_damage_weight_function\.verified$", "TEST-078, GOLD-001")
r(NL + r"(test_material_damage\.(cc|hh)|material_remove_damage\.dat)$", "TEST-078, TEST-080")
r(NL + r"(test_build_neighborhood_parallel\.cc|material_parallel_test\.dat|parallel_test\.msh)$", "TEST-079")
r(NL + r"(test_pair_computation\.cc|pair_test\.msh)$", "TEST-080")
r(NL + r"plate\.msh$", "TEST-075…TEST-078, MDATA-005")
r(NL + r"CMakeLists\.txt$", "TEST-075…TEST-080, TEST-233")
CM = r"^test/test_model/test_contact_mechanics_model/"
r(CM + r"(test_explicit_dynamic\.cc|flat_on_flat\.geo|material\.dat)$", "TEST-085")
r(CM + r"(test_explicit_friction\.cc|sliding-block-2D\.geo|material-friction\.dat)$", "TEST-086")
r(CM + r"CMakeLists\.txt$", "TEST-085, TEST-086, TEST-219, TEST-233")
r(CM + r"test_coupler/(test_contact_coupling\.cc|coupling\.geo|material\.dat)$", "TEST-215")
r(CM + r"test_coupler/(test_coupled_stiffness\.cc|flat_on_flat\.geo|material_stiffness\.dat)$", "TEST-216")
r(CM + r"test_coupler/CMakeLists\.txt$", "TEST-215, TEST-216")
r(CM + r"test_detector/", "TEST-217")
r(CM + r"test_resolution/", "TEST-218")
r(CM + r"test_selector/", "TEST-219")
PF = r"^test/test_model/test_phase_field_model/"
r(PF + r"(test_phasefield_selector\.cc|phasefield_selector\.(dat|msh))$", "TEST-090")
r(PF + r"test_phase_solid_coupling\.cc$", "TEST-091")
r(PF + r"material_coupling\.dat$", "TEST-091, TEST-096")
r(PF + r"(test_phase_field_anisotropic\.cc|material_hybrid\.dat)$", "TEST-092")
r(PF + r"material_penalization\.dat$", "TEST-092, TEST-098")
r(PF + r"(test_phase_field_deviatoric_split\.cc|material_deviatoric\.dat)$", "TEST-093")
r(PF + r"(test_phase_field_optimal\.cc|material_optimal\.dat)$", "TEST-094")
r(PF + r"test_optimal_damage\.msh$", "TEST-094, TEST-095, MDATA-005")
r(PF + r"(test_phase_field_optimal_linear\.cc|material_optimal_linear\.dat)$", "TEST-095")
r(PF + r"test_phase_solid_explicit\.cc$", "TEST-096")
r(PF + r"(test_multi_material\.cc|material_multiple\.dat|test_two_element\.msh)$", "TEST-097")
r(PF + r"(test_penalization\.cc|test_four_elements\.msh)$", "TEST-098")
r(PF + r"(test_phase_field_linear\.cc|material_linear\.dat)$", "TEST-099")
r(PF + r"(test_phase_field_tao\.cc|material_linear_tao\.dat)$", "TEST-100")
r(PF + r"test_one_element\.msh$", "TEST-091…TEST-093, TEST-096, TEST-099, TEST-100, MDATA-005")
r(PF + r"CMakeLists\.txt$", "TEST-090…TEST-100, TEST-233")
SM = r"^test/test_model/test_solid_mechanics_model/"
r(SM + r"(test_solid_mechanics_model_reassign_material\.cc|cube_two_materials\.geo|two_materials\.dat)$", "TEST-105")
r(SM + r"(test_solid_mechanics_model_material_eigenstrain\.cc|material_elastic_plane_strain\.dat)$", "TEST-106")
r(SM + r"cube_3d_tet_4\.msh$", "TEST-106, TEST-107, MDATA-005")
r(SM + r"(test_solid_mechanics_model_material_large_rotation\.cc|material_elastic\.dat)$", "TEST-107")
r(SM + r"(test_material_selector\.cc|material_selector\.(dat|msh))$", "TEST-108")
r(SM + r"(test_solid_mechanics_model_dynamics\.cc|test_solid_mechanics_model_dynamics_material\.dat)$", "TEST-109")
r(SM + r"test_solid_mechanics_model_fixture\.hh$", "TEST-109, TEST-128…TEST-131")
r(SM + r"CMakeLists\.txt$", "TEST-105…TEST-109, TEST-233")
CO = SM + r"test_cohesive/"
r(CO + r"(test_cohesive\.cc|test_cohesive_fixture\.hh|material_0\.dat|material_1\.dat|material_0_finite_def\.dat)$", "TEST-115")
r(CO + r"data/", "TEST-115, MDATA-004")
r(CO + r"CMakeLists\.txt$", "TEST-115, TEST-233")
r(CO + r"test_cohesive_buildfragments/", "TEST-116")
r(CO + r"test_cohesive_insertion/(test_cohesive_insertion_along_physical_surfaces\.cc|3d_spherical_inclusion\.geo|input_file\.dat)$", "TEST-117")
r(CO + r"test_cohesive_insertion/(test_group_update\.cc|bar\.geo|material\.dat)$", "TEST-118")
r(CO + r"test_cohesive_insertion/CMakeLists\.txt$", "TEST-117, TEST-118")
r(CO + r"test_materials/", "TEST-119")
r(CO + r"test_cohesive_1d_element/", "TEST-220")
r(CO + r"test_cohesive_extrinsic/", "TEST-221")
r(CO + r"test_cohesive_extrinsic_implicit/K_matrix_verified\.dat$", "TEST-222, GOLD-006")
r(CO + r"test_cohesive_extrinsic_implicit/", "TEST-222")
r(CO + r"test_cohesive_intrinsic/", "TEST-223")
r(CO + r"test_cohesive_intrinsic_impl/", "TEST-224")
r(CO + r"test_cohesive_linear_friction/", "TEST-225")
r(CO + r"test_parallel_cohesive/.*output_dir_verified/", "TEST-226, GOLD-007")
r(CO + r"test_parallel_cohesive/", "TEST-226")
EM = SM + r"test_embedded_interface/"
r(EM + r"(test_embedded_element_matrix\.cc|triangle\.msh|embedded_element\.dat)$", "TEST-125")
r(EM + r"matrix$", "TEST-126, GOLD-008")
r(EM + r"(test_embedded_interface_model\.(cc|sh)|embedded_mesh\.msh|material\.dat)$", "TEST-126")
r(EM + r"(test_embedded_interface_model_prestress\.cc|embedded_mesh_prestress(_reinforcement)?\.msh|prestress\.dat)$", "TEST-127")
r(EM + r"CMakeLists\.txt$", "TEST-125…TEST-127, TEST-233")
EN = SM + r"test_energies/"
r(EN + r"test_solid_mechanics_model_linear_elastic_potential_energy(_material\.dat|\.cc)$", "TEST-128")
r(EN + r"test_solid_mechanics_model_kinetic_energy(_material\.dat|\.cc)$", "TEST-129")
r(EN + r"test_solid_mechanics_model_work_quasistatic\.cc$", "TEST-130")
r(EN + r"test_solid_mechanics_model_work_dynamics\.cc$", "TEST-131")
r(EN + r"test_solid_mechanics_model_work_material\.dat$", "TEST-130, TEST-131")
r(EN + r"CMakeLists\.txt$", "TEST-128…TEST-131, TEST-233")
MA = SM + r"test_materials/"
r(MA + r"(test_local_material\.cc|local_material_damage\.(cc|hh)|local_material_damage_inline_impl\.hh|mesh_section_gap\.geo|material\.dat)$", "TEST-135")
r(MA + r"(test_interpolate_stress\.cc|interpolation\.geo|material_interpolate\.dat)$", "TEST-136")
r(MA + r"(test_material_orthotropic\.cc|square\.geo|orthotropic\.dat)$", "TEST-137")
r(MA + r"(test_material_mazars\.cc|material_mazars\.dat)$", "TEST-138")
r(MA + r"test_multi_material_elastic\.(cc|geo|dat)$", "TEST-139")
r(MA + r"(test_finite_deformation\.cc|cube\.geo|1_tetrahedron\.msh|material_finite_deformation\.dat)$", "TEST-140")
r(MA + r"test_elastic_materials\.cc$", "TEST-141")
r(MA + r"test_finite_def_materials\.cc$", "TEST-142")
r(MA + r"(test_damage_materials\.cc|py_mazars\.py)$", "TEST-143")
r(MA + r"test_plastic_materials\.cc$", "TEST-144")
r(MA + r"test_material_thermal\.cc$", "TEST-145")
r(MA + r"test_material_fixtures\.hh$", "TEST-141…TEST-145")
r(MA + r"CMakeLists\.txt$", "TEST-135…TEST-145, TEST-233")
r(MA + r"test_material_elasto_plastic_linear_isotropic_hardening/.*\.verified$", "TEST-146, GOLD-001")
r(MA + r"test_material_elasto_plastic_linear_isotropic_hardening/", "TEST-146")
r(MA + r"test_material_non_local/(test_material_damage_non_local\.cc|mesh_section_gap\.msh|material_damage_non_local\.dat)$", "TEST-147")
r(MA + r"test_material_non_local/(test_material_non_local\.cc|custom_non_local_test_material\.(cc|hh)|mesh\.geo|material\.dat)$", "TEST-148")
r(MA + r"test_material_non_local/CMakeLists\.txt$", "TEST-147, TEST-148")
r(MA + r"test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation\.cc$", "TEST-149")
r(MA + r"test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation_tension\.cc$", "TEST-150")
r(MA + r"test_material_viscoelastic/", "TEST-149, TEST-150")
r(MA + r"test_material_viscoelastic_maxwell/", "TEST-151")
ST = r"^test/test_model/test_structural_mechanics_model/"
r(ST + r"test_structural_mechanics_model_kirchhoff_shell_patch_test_4_5_5\.cc$", "TEST-227")
r(ST + r"\w+\.msh$", "TEST-152, MDATA-001")
r(ST + r"CMakeLists\.txt$", "TEST-152, TEST-233")
r(ST, "TEST-152")
r(r"^test/test_python_interface/", "TEST-153")
SO = r"^test/test_solver/"
r(SO + r"(test_sparse_matrix_profile\.cc)$", "TEST-155")
r(SO + r"triangle\.geo$", "TEST-155, TEST-156")
r(SO + r"test_sparse_matrix_assemble\.cc$", "TEST-156")
r(SO + r"(test_sparse_matrix_product\.cc|bar\.msh)$", "TEST-157")
r(SO + r"test_sparse_matrix_product\.verified$", "TEST-157, GOLD-001")
r(SO + r"test_sparse_solver_mumps\.cc$", "TEST-158")
r(SO + r"test_sparse_solver_eigen\.cc$", "TEST-159")
r(SO + r"test_sparse_solver_petsc\.cc$", "TEST-160")
r(SO + r"test_petsc_matrix_profile(_parallel)?\.verified$", "TEST-228, GOLD-003")
r(SO + r"(test_petsc_\w+\.(cc|sh)|test_solver_petsc(_parallel)?\.(cc|sh)|square\.geo)$", "TEST-228")
r(SO + r"1D_bar\.geo$", "TEST-228, GOLD-015")
r(SO + r"test_sparse_matrix_profile_parallel\.cc$", "TEST-229")
r(SO + r"CMakeLists\.txt$", "TEST-155…TEST-160, TEST-233")
SY = r"^test/test_synchronizer/"
r(SY + r"(test_dof_synchronizer\.cc|bar\.msh)$", "TEST-165")
r(SY + r"test_data_accessor\.hh$", "TEST-165, TEST-167, TEST-170")
r(SY + r"test_communicator\.cc$", "TEST-166")
r(SY + r"(test_synchronizer_communication\.cc|cube\.geo)$", "TEST-167")
r(SY + r"test_synchronizers_fixture\.hh$", "TEST-167…TEST-170")
r(SY + r"test_node_synchronizer\.cc$", "TEST-168")
r(SY + r"test_data_distribution\.cc$", "TEST-169")
r(SY + r"(test_facet_synchronizer\.cc|facet\.geo)$", "TEST-170")
r(SY + r"(test_grid_synchronizer(_check_neighbors)?\.cc|test_grid_tools\.hh|test_dof_synchronizer_communication\.cc|test_dof_data_accessor\.hh)$", "TEST-230")
r(SY + r"CMakeLists\.txt$", "TEST-165…TEST-170, TEST-233")
# ---------------------------------------------------------------- extra packages
NT = r"^extra_packages/traction-at-split-node-contact/test/"
r(NT + r"CMakeLists\.txt$", "TEST-233")
r(NT + r"test_ntn_model/(test_ntn_basics\.cc|test_ntn_fixture\.hh|ntn_test_[23]d\.msh|data/material_ntn_model\.dat)$", "TEST-175")
r(NT + r"test_ntn_model/(test_ntn_friction\.cc|test_ntn_friction_fixture\.hh)$", "TEST-176, TEST-177")
r(NT + r"test_ntn_model/(ntn_test_compression\.msh|data/material_ntn_friction_slip_weakening\.dat)$", "TEST-176")
r(NT + r"test_ntn_model/test_ntn_compression\.verified$", "GOLD-005")
r(NT + r"test_ntn_model/data/material_ntn_friction_rate_and_state_revised_n_shaped_\w+\.dat$", "TEST-177 (commented out), GOLD-015")
r(NT + r"test_ntn_model/(data/material_ntn_friction_rate_and_state_\w+\.dat|ntn_test_ras\.geo)$", "TEST-177")
r(NT + r"test_ntn_model/CMakeLists\.txt$", "TEST-175…TEST-177, TEST-233")
XM = r"^extra_packages/extra-materials/test/"
r(XM + r"CMakeLists\.txt$", "TEST-233")
r(XM + r"test_material_FE2/(test_material_FE2\.cc|material\.dat|square\.msh)$", "TEST-180")
r(XM + r"test_material_FE2/periodic_plate\.msh$", "TEST-180, TEST-183")
r(XM + r"test_material_FE2/(test_eigenstrain_homogenization\.cc|mesoscale_materials\.dat|one_inclusion\.msh)$", "TEST-181")
r(XM + r"test_material_FE2/(test_elastic_homogenization\.cc|homogenized_plate\.geo|material_orthotropic\.dat)$", "TEST-182")
r(XM + r"test_material_FE2/(test_node_selection\.cc|periodic_plate\.geo|material_test_boundary\.dat)$", "TEST-183")
r(XM + r"test_material_FE2/CMakeLists\.txt$", "TEST-180…TEST-183")
r(XM + r"test_material_FE2/", "TEST-180…TEST-183")
r(XM + r"test_material_damage/test_material_damage_iterative\.cc$", "TEST-184, TEST-185")
r(XM + r"test_material_damage/test_material_damage_iterative_parallel\.sh$", "TEST-185")
r(XM + r"test_material_damage/(material\.dat|plate\.msh)$", "TEST-184, TEST-185, TEST-186")
r(XM + r"test_material_damage/(test_material_damage_iterative_non_local_serial\.cc|material_non_local\.dat)$", "TEST-186")
r(XM + r"test_material_damage/(test_material_damage_iterative_non_local_parallel\.(cc|sh)|two_materials\.dat|one_circular_inclusion\.msh)$", "TEST-187")
r(XM + r"test_material_damage/test_material_iterative_stiffness_reduction\.verified$", "TEST-188, GOLD-001")
r(XM + r"test_material_damage/(test_material_iterative_stiffness_reduction\.cc|material_stiffness_reduction\.dat|two_elements\.msh)$", "TEST-188")
r(XM + r"test_material_damage/energy_study\.geo$", "TEST-184…TEST-188 (add_mesh single_agg, no consumer)")
r(XM + r"test_material_damage/CMakeLists\.txt$", "TEST-184…TEST-188")
r(XM + r"test_material_damage/", "TEST-184…TEST-188")
IG = r"^extra_packages/igfem/test/"
r(IG + r"CMakeLists\.txt$", "TEST-233")
r(IG + r"patch_tests/(test_igfem_triangle_5\.cc|plate\.geo)$", "TEST-190")
r(IG + r"patch_tests/(test_igfem_triangle_4\.cc|test_mesh\.msh)$", "TEST-191")
r(IG + r"patch_tests/material\.dat$", "TEST-190, TEST-191")
r(IG + r"patch_tests/(test_interface_position\.(cc|sh|msh)|material_test_interface_position\.dat)$", "TEST-192")
r(IG + r"patch_tests/", "TEST-190…TEST-192")
r(IG + r"test_fe_engine/test_interpolate\.(cc|verified)$", "TEST-193")
r(IG + r"test_fe_engine/test_integrate\.(cc|verified)$", "TEST-194")
r(IG + r"test_fe_engine/test_fe_engine_precomputation\.(cc|verified)$", "TEST-195")
r(IG + r"test_fe_engine/test_gradient\.(cc|verified)$", "TEST-196")
r(IG + r"test_fe_engine/mesh_info_igfem_triangle_[45]\.txt$", "TEST-193…TEST-196, GOLD-014")
r(IG + r"test_fe_engine/", "TEST-193…TEST-196")
r(IG + r"test_geometry/test_spherical_kernel\.cc$", "TEST-197")
r(IG + r"test_geometry/test_generation_igfem_mesh_from_sphere_static\.cc$", "TEST-198")
r(IG + r"test_geometry/test_generation_igfem_mesh_from_sphere\.cc$", "TEST-199")
r(IG + r"test_geometry/", "TEST-197…TEST-199")
SI = IG + r"test_solid_mechanics_model_igfem/"
r(SI + r"(test_solid_mechanics_model_igfem\.(cc|sh|verified)|plate\.msh)$", "TEST-200")
r(SI + r"material\.dat$", "TEST-200, TEST-203")
r(SI + r"test_transfer_internals\.(cc|sh)$", "TEST-201")
r(SI + r"(material_damage\.dat|fine_mesh\.msh)$", "TEST-201, TEST-202")
r(SI + r"test_eigenstrain\.cc$", "TEST-202")
r(SI + r"test_igfem_element_orientation(_parallel)?\.(cc|sh|msh)$", "TEST-203")
r(SI + r"(test_ASR_damage\.cc|material_ASR\.dat|one_inclusion\.msh)$", "TEST-204")
r(SI, "TEST-205")

# explicit rules for unused / source-only files (inserted with top priority)
_EXTRA = [
    (r"^test/test_fe_engine/(pentahedron|segment)\.msh$", "MDATA-001 (unused committed mesh), GOLD-015"),
    (r"^test/test_model/patch_tests/data/material\.dat$", "TEST-050…TEST-054 (generic material file, unused), GOLD-015"),
    (r"^test/test_model/test_common/test_non_local_toolbox/fine_mesh\.msh$", "TEST-075…TEST-080 (unused mesh), GOLD-015"),
    (r"^test/test_model/test_common/test_non_local_toolbox/plate\.geo$", "TEST-075…TEST-078 (source of committed plate.msh), MDATA-005"),
    (r"^test/test_model/test_phase_field_model/test_(one|two|four)_elements?\.geo$", "TEST-091…TEST-100 (sources of committed meshes), MDATA-005"),
    (r"^test/test_model/test_solid_mechanics_model/material_selector\.geo$", "TEST-108 (source of committed mesh), MDATA-005"),
    (r"^test/test_model/test_solid_mechanics_model/test_cohesive/material\.dat$", "TEST-115 (unused), GOLD-015"),
    (r"^test/test_model/test_solid_mechanics_model/test_materials/1_tetrehedron\.geo$", "TEST-140 (source of 1_tetrahedron.msh), MDATA-005"),
    (r"^test/test_model/test_solid_mechanics_model/test_materials/barre_trou\.geo$", "TEST-135 (unused; target uses mesh_section_gap.geo), GOLD-015"),
    (r"^test/test_model/test_solid_mechanics_model/test_materials/material_thermal\.dat$", "TEST-145 (unused; test builds material in code), GOLD-015"),
    (r"^test/test_model/test_solid_mechanics_model/test_materials/test_material_non_local/mesh_section_gap\.geo$", "TEST-147 (source of committed mesh), MDATA-005"),
    (r"^test/test_synchronizer/data_split\.msh$", "TEST-169 (unused), GOLD-015"),
]
for _pat, _ids in reversed(_EXTRA):
    R.insert(0, (re.compile(_pat), _ids))

unmapped = []
rows = []
for f in files:
    for pat, ids in R:
        if pat.search(f):
            rows.append((f, ids))
            break
    else:
        unmapped.append(f)

if unmapped:
    sys.stderr.write("UNMAPPED (%d):\n" % len(unmapped))
    for u in unmapped:
        sys.stderr.write("  " + u + "\n")
    sys.exit(1)

# group by top-level area
groups = {}
for f, ids in rows:
    if f.startswith("python/") or "/" not in f:
        g = "python/ and root packaging files"
    elif f.startswith("test/") and f.count("/") == 1:
        g = "test (top level)"
    elif f.startswith("examples/c++"):
        g = "examples/c++"
    elif f.startswith("examples/python"):
        g = "examples/python"
    elif f.startswith("examples/"):
        g = "examples (top level)"
    elif f.startswith("test/ci/"):
        g = "test/ci"
    elif f.startswith("test/"):
        g = "test/" + f.split("/")[1]
    else:
        g = "/".join(f.split("/")[:3])
    groups.setdefault(g, []).append((f, ids))

with open(out, "w") as fh:
    fh.write("\n---\n\n## Appendix: file coverage\n\n")
    fh.write(
        "Every file under `python/`, `examples/`, `test/` and "
        "`extra_packages/*/test` (plus the root Python packaging files) mapped "
        "to at least one inventory ID. Generated by "
        "`docs/inventory/tests_examples_python/coverage.py` (ordered regex "
        "rules, first match wins; the script fails if any file is unmapped). "
        f"Total: **{len(rows)} files**, 0 unmapped.\n\n")
    fh.write("| Area | Files |\n|---|---|\n")
    for g in groups:
        fh.write(f"| {g} | {len(groups[g])} |\n")
    for g, items in groups.items():
        fh.write(f"\n### App.{list(groups).index(g) + 1} `{g}` ({len(items)} files)\n\n")
        fh.write("| File | ID(s) |\n|---|---|\n")
        for f, ids in items:
            fh.write(f"| `{f}` | {ids} |\n")
print(len(rows), "files mapped")
