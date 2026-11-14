# SPDX-License-Identifier: LGPL-3.0-or-later
# akantu-rs: the runs of upstream's Python examples
# (examples/python/** of Akantu, EPFL/LSMS, copied unchanged below
# python/examples/) compared between the oracle module and the port.
"""Case table of the Python example comparisons (docs/TESTING.md).

Every case runs one **unchanged** upstream script (``script``, relative to
``python/examples``) in a fresh directory holding its input files
(``inputs``: name in the run directory -> repository path; meshes are the
frozen Gmsh 4.13.1 meshes of D-009, input files the verbatim copies of the
parser corpus). What has to differ from upstream's own run is stated per
case and applied by the harness, never by editing the script:

``replace``
    marked variant input files (``python/examples/variants/``) used instead
    of upstream's file of that name (e.g. a material file with a ``model``
    section that pins the sparse solver to Eigen: upstream's ``_auto``
    resolves to MUMPS in the oracle build, the port's solve is bitwise
    against Eigen's).
``preparse``
    a variant input file parsed (``akantu.parseInput``) before the script
    runs, for scripts that never parse one themselves.
``compat``
    the port's upstream-compatibility switch (``setUpstreamCompat(True)``,
    a port extension, D-003 B) on every coupler the script creates; absent
    upstream, so the oracle side is unchanged.
``gmsh``
    the frozen mesh that a ``gmsh -2 <name>.geo`` call of the script
    produces in the oracle image (a ``gmsh`` stand-in on ``PATH`` copies it).
``stop_line``
    (binding gaps, group 5) the script line at which the port refuses a
    call: the port must raise there; the oracle run is stopped before the
    same line; the state reached (``probes`` and the script's numeric
    variables) is compared. ``refusal`` (exception type, message fragment)
    pins what the port raises there.
``oracle``
    the oracle case the port's run is compared with (default: the case
    itself). The port's ``_auto`` solver is Eigen, so an unmodified run of a
    script with a linear solve is compared with the oracle's Eigen-pinned
    variant.
``check``
    ``exact`` (every output equal bitwise), ``raises`` (upstream's run
    raises: both runs raise the same exception at the same line; compared:
    the status line, standard output, the files written before, the
    script's numeric variables and the case's ``probes``, evaluated on the
    script's namespace where it raised),
    ``independent`` (the port's
    corrected default differs from upstream by design: checked by
    ``python/tests/examples``), ``diagnostic`` (oracle only: recorded, e.g.
    the oracle's MUMPS run against its Eigen run).
"""

# The oracle runs' Python environment (akantu-oracle:dev, Debian trixie);
# `env_skip` / `env_ignore` of a case name the parts of it a result depends
# on (python/tests/examples compares those only in the same environment, as
# the Rust example tests compare libm-dependent values on x86_64 glibc only).
#
# `platform`: the oracle ran on x86_64 Linux glibc. NumPy's
# transcendental and BLAS-backed kernels (np.sin / np.exp, np.einsum,
# np.linalg.norm) give other last bits on other platforms (arm64 macOS: up to
# 3 ulp, measured with python/examples/numpy_platform_check.py: docs/TESTING.md
# "Python examples", "Where the comparisons hold"), so the cases whose result
# depends on such a value computed by NumPy ALONE name it in `env_skip` /
# `env_ignore` (class X1 of docs/NUMERICAL_VALIDATION.md); on the oracle's
# platform every one of them is compared bitwise as before.
#
# `numpy_avx512f`: on x86_64 Linux too, NumPy's float64
# `np.exp` takes its AVX-512F kernel only where the CPU has AVX-512F; without
# it (e.g. AMD Zen 3; inferred, not observed, for the GitHub runner of the
# first `python` job, whose log does not name its CPU) the pulse of
# dynamics.py / custom-material.py differs in 149 of 4,221 nodes by up to
# 2 ulp (np.sin and np.einsum do not change; measured with
# numpy_platform_check.py and NumPy's NPY_DISABLE_CPU_FEATURES /
# NPY_ENABLE_CPU_FEATURES: docs/NUMERICAL_VALIDATION.md).
# The oracle ran on an AVX-512F CPU (AMD EPYC 9455) and its pulse is the
# AVX-512F kernel's. Only the two pulse cases name this key; the phase-field
# norms (BLAS, OPENBLAS_CORETYPE pinned) do not depend on it.
ORACLE_ENVIRONMENT = {"python": "3.13.5", "numpy": "2.2.4", "scipy": "1.15.3",
                      "matplotlib": True, "numpy_size1_to_scalar": True,
                      "platform": "x86_64-linux-glibc", "numpy_avx512f": True}


def numpy_avx512f():
    """Whether NumPy runs its AVX-512F kernels in this process: the CPU has
    the feature, NumPy was built with it (baseline or dispatch target) and
    NPY_DISABLE_CPU_FEATURES / NPY_ENABLE_CPU_FEATURES did not turn it off.
    None when NumPy does not say (then it differs from the oracle's True)."""
    try:
        from numpy._core import _multiarray_umath as mu
    except ImportError:
        try:
            from numpy.core import _multiarray_umath as mu
        except ImportError:
            return None
    features = getattr(mu, "__cpu_features__", None)
    if not isinstance(features, dict):
        return None  # NumPy does not say (not: "no AVX-512F")
    built = (tuple(getattr(mu, "__cpu_baseline__", ()))
             + tuple(getattr(mu, "__cpu_dispatch__", ())))
    return bool(features.get("AVX512F")) and "AVX512F" in built


def environment():
    """This interpreter's values of the entries of ORACLE_ENVIRONMENT (the
    examples test compares them; ci/tools/python_env.py prints them)."""
    import importlib.util
    import platform
    import sys
    import warnings

    import numpy as np

    libc = platform.libc_ver()[0]
    env = {"python": platform.python_version(), "numpy": np.__version__,
           "numpy_size1_to_scalar": True,
           "platform": "-".join(x for x in (platform.machine(), sys.platform, libc) if x),
           "numpy_avx512f": numpy_avx512f()}
    try:
        import scipy
        env["scipy"] = scipy.__version__
    except ImportError:
        env["scipy"] = None
    env["matplotlib"] = importlib.util.find_spec("matplotlib") is not None
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", DeprecationWarning)
        try:
            int(np.zeros(1))  # NumPy >= 2.5: TypeError (deprecated since 1.25)
        except TypeError:
            env["numpy_size1_to_scalar"] = False
    return env

PARSER = "crates/akantu-parser/tests/fixtures/examples/python"
MESHES = "fixtures/meshes/gmsh-4.13.1/examples/python"
VARIANTS = "python/examples/variants"
FIXTURES = "fixtures/python/examples"


def _case(row, script, inputs, **kw):
    d = {"row": row, "script": script, "inputs": inputs, "replace": {},
         "preparse": None, "compat": False, "gmsh": None, "argv": [],
         "stop_line": None, "probes": [], "capture": [], "oracle": None,
         "check": "exact", "variant": None, "ignore": {}, "env_skip": [],
         "env_ignore": {}, "stdout_mask": [], "refusal": None}
    d.update(kw)
    return d


SMM = "solid_mechanics_model"
COH = "solid_mechanics_cohesive_model"
PF = "phase_field_model"
CM = "contact_mechanics_model"
ST = "structural_mechanics_model"

_static_plate = {"material.dat": f"{PARSER}/{SMM}/plate-hole/material.dat",
                 "plate.msh": f"{MESHES}/{SMM}/plate-hole/plate.msh"}
_bar_cb = {"material.dat": f"{PARSER}/{SMM}/solver_callback/material.dat",
           "bar.msh": f"{MESHES}/{SMM}/solver_callback/bar.msh"}
_coh_plate = {"material.dat": f"{PARSER}/{COH}/cohesive/material.dat",
              "plate.msh": f"{MESHES}/{COH}/cohesive/plate.msh"}
_mm_plate = {"material.dat": f"{PARSER}/{COH}/multi-material/material.dat",
             "plate.msh": f"{MESHES}/{COH}/multi-material/plate.msh"}
_pf_static = {"material_static.dat": f"{PARSER}/{PF}/material_static.dat",
              "plate_static.msh": f"{MESHES}/{PF}/plate_static.msh"}
_pf_dynamic = {"material.dat": f"{PARSER}/{PF}/material.dat",
               "plate.msh": f"{MESHES}/{PF}/plate.msh"}
_eigen_modes = {"material.dat": f"{PARSER}/{SMM}/eigen_modes/material.dat",
                "image_saver.py": f"python/examples/{SMM}/eigen_modes/image_saver.py"}
_eigen_modes_mesh = f"{FIXTURES}/eigen_modes/inputs/bar.msh"
_pf_probes = ["solid.getDisplacement()", "solid.getVelocity()",
              "solid.getAcceleration()", "phase.getDamage()",
              "solid.getEnergy('kinetic')", "solid.getEnergy('potential')",
              "phase.getEnergy('dissipated')"]
# EX-038: the insertion of the Python cohesive law, at
# the exception of line 191 (upstream binds no getNormals, UD-980)
_coh_custom_probes = [
    "mesh.getNbElement(element_type=aka._cohesive_2d_4)",
    "mesh.getConnectivity(aka._cohesive_2d_4)",
    "mesh.getNbNodes()",
    "model.getElementInserter().getCheckFacets(aka._segment_2)",
    "model.getStressOnFacets(aka._segment_2)",
    "model.getDisplacement()",
    "model.getVelocity()",
]
# phasefield-static.py:97-98 prints / stores np.linalg.norm of the iteration's
# displacement and damage increments. Measured on arm64 macOS: the increments
# (final.delta_disp, final.delta_dam), every dump and the stdout (iteration
# counts) are identical to the x86_64 oracle's, only the two norms of the SAME
# arrays differ by 1 ulp; the case stays compared there with these two values
# ignored.
_pf_norms = {"platform": ["final.error_disp", "final.error_dam"]}
_compression = {"compression.dat": f"{PARSER}/{CM}/compression/compression.dat",
                "compression.msh": f"{MESHES}/{CM}/compression/compression.msh"}

CASES = {
    # -- group 1: nothing missing in the bindings --------------------------
    "stiffness_matrix": _case(
        "EX-030", f"{SMM}/stiffness_matrix/stiffness_matrix.py",
        {"material.dat": f"{PARSER}/{SMM}/stiffness_matrix/material.dat",
         "plate.msh": f"{MESHES}/{SMM}/stiffness_matrix/plate.msh"},
        capture=["sparse"]),
    "dynamics": _case(
        "EX-031", f"{SMM}/dynamics/dynamics.py",
        {"material.dat": f"{PARSER}/{SMM}/dynamics/material.dat",
         "bar.msh": f"{MESHES}/{SMM}/dynamics/bar.msh"},
        # the initial pulse is np.sin * np.exp (dynamics.py:76-78); np.exp
        # takes NumPy's AVX-512F kernel where the CPU has it (F-2)
        env_skip=["platform", "numpy_avx512f"]),
    "plate_hole": _case(
        "EX-032", f"{SMM}/plate-hole/plate.py", _static_plate,
        oracle="plate_hole_eigen"),
    "plate_hole_eigen": _case(
        "EX-032", f"{SMM}/plate-hole/plate.py", _static_plate,
        replace={"material.dat": f"{VARIANTS}/plate-hole/material.dat"},
        variant="Eigen pinned (material.dat + model section)"),
    "custom_material": _case(
        "EX-033", f"{SMM}/custom-material/custom-material.py",
        {"material.dat": f"{PARSER}/{SMM}/custom-material/material.dat",
         "bar.msh": f"{MESHES}/{SMM}/custom-material/bar.msh"},
        # the initial pulse is np.sin * np.exp (custom-material.py:147-149) and
        # the law reduces with np.einsum; np.exp takes NumPy's AVX-512F kernel
        # where the CPU has it (F-2; np.einsum does not change with it)
        env_skip=["platform", "numpy_avx512f"]),
    "bi_material": _case(
        "EX-034", f"{SMM}/custom-material/bi-material.py",
        {"square.msh": f"{MESHES}/{SMM}/custom-material/square.msh"},
        oracle="bi_material_eigen"),
    "bi_material_eigen": _case(
        "EX-034", "variants/custom-material/bi-material.py",
        {"square.msh": f"{MESHES}/{SMM}/custom-material/square.msh",
         "material_eigen.dat": f"{VARIANTS}/custom-material/material_eigen.dat"},
        variant="marked variant script: the dict form of initFull replaced by the "
                "same law from a parsed file with Eigen pinned (the dict form takes "
                "no solver options; a parsed model section makes upstream's dict "
                "form refuse: 'No constitutive_law to initialize')"),
    "solver_callback": _case(
        "EX-035", f"{SMM}/solver_callback/solver_callback.py", _bar_cb,
        oracle="solver_callback_eigen",
        env_skip=["numpy_size1_to_scalar"]),
    "solver_callback_eigen": _case(
        "EX-035", f"{SMM}/solver_callback/solver_callback.py", _bar_cb,
        replace={"material.dat": f"{VARIANTS}/solver_callback/material.dat"},
        variant="Eigen pinned (material.dat + model section)",
        env_skip=["numpy_size1_to_scalar"]),
    "multi_material": _case(
        "EX-040", f"{COH}/multi-material/plate.py", _mm_plate,
        oracle="multi_material_eigen"),
    "multi_material_eigen": _case(
        "EX-040", f"{COH}/multi-material/plate.py", _mm_plate,
        replace={"material.dat": f"{VARIANTS}/multi-material/material.dat"},
        variant="Eigen pinned (material.dat + model section)"),
    "phasefield_static": _case(
        "EX-041", f"{PF}/phasefield-static.py", _pf_static,
        oracle="phasefield_static_eigen",
        env_ignore=_pf_norms),
    "phasefield_static_eigen": _case(
        "EX-041", f"{PF}/phasefield-static.py", _pf_static,
        replace={"material_static.dat": f"{VARIANTS}/phase_field_model/material_static.dat"},
        compat=True,
        variant="Eigen pinned (material_static.dat + model sections); port: setUpstreamCompat(True)",
        env_ignore=_pf_norms),

    # -- group 2: with the cohesive-element dumper -------------------------
    "cohesive_plate": _case(
        "EX-037", f"{COH}/cohesive/plate.py", _coh_plate,
        oracle="cohesive_plate_eigen"),
    "cohesive_plate_eigen": _case(
        "EX-037", f"{COH}/cohesive/plate.py", _coh_plate,
        replace={"material.dat": f"{VARIANTS}/cohesive/material.dat"},
        variant="Eigen pinned (material.dat + model section)"),
    "fragmentation": _case(
        "EX-039", f"{COH}/fragmentation/fragmentation.py",
        {"material.dat": f"{PARSER}/{COH}/fragmentation/material.dat",
         "fragmentation_mesh.msh": f"{MESHES}/{COH}/fragmentation/fragmentation_mesh.msh"},
        ignore={"final.nodes": "UD-085: a view of the mesh nodes taken before the "
                "insertions grew the mesh; upstream's view dangles (freed storage), "
                "the port's is stale by design (D-047)"}),
    # -- group 3: eigen modes (Gmsh, SciPy eigsh) ---------------------------
    "eigen_modes": _case(
        "EX-036", f"{SMM}/eigen_modes/eigen_modes.py", _eigen_modes,
        gmsh=_eigen_modes_mesh, capture=["sparse"], oracle="eigen_modes_eigen",
        env_skip=["numpy", "scipy", "matplotlib"]),
    "eigen_modes_eigen": _case(
        "EX-036", f"{SMM}/eigen_modes/eigen_modes.py", _eigen_modes,
        replace={"material.dat": f"{VARIANTS}/eigen_modes/material.dat"},
        gmsh=_eigen_modes_mesh, capture=["sparse"],
        variant="Eigen pinned (material.dat + model section)",
        env_skip=["numpy", "scipy", "matplotlib"]),
    # -- group 4: the examples unblocked (D-078), run to their end ---
    # EX-042: the coupler's `model.dump()` acts on the solid's default
    # series (akantu_couplers::dump, UD-855)
    "phasefield_dynamic_full": _case(
        "EX-042", f"{PF}/phasefield-dynamic.py", _pf_dynamic,
        oracle="phasefield_dynamic_eigen_full"),
    "phasefield_dynamic_eigen_full": _case(
        "EX-042", f"{PF}/phasefield-dynamic.py", _pf_dynamic,
        replace={"material.dat": f"{VARIANTS}/phase_field_model/material.dat"},
        compat=True,
        variant="Eigen pinned (material.dat + model sections); port: setUpstreamCompat(True)"),
    # EX-043, EX-045: the coupler dumpers; the port's contact corrections
    # (UD-363, UD-365, ...) change these runs by design, so the comparison
    # runs with the port's setUpstreamCompat(True) on the coupler
    "compression_full": _case(
        "EX-043", f"{CM}/compression/compression.py", _compression, compat=True,
        variant="port: setUpstreamCompat(True) on the coupler"),
    # EX-044: the contact dumper (line 34) and the mesh's element group
    # `contact_surface` that the surface selector creates (line 37;
    # the port mirrors the selector's surface there), then the
    # normals, gaps and contact elements printed; the group's nodes and
    # elements are compared through probes
    "detection_explicit_full": _case(
        "EX-044", f"{CM}/compression/detection-explicit.py",
        {"detection-explicit.dat": f"{PARSER}/{CM}/compression/detection-explicit.dat",
         "detection-explicit.msh":
             "fixtures/mesh_io/examples/python/contact_mechanics_model/compression/"
             "detection-explicit/inputs/detection-explicit.msh"},
        compat=True, variant="port: setUpstreamCompat(True) on the contact model",
        # the script prints and keeps the wall-clock time of the search
        stdout_mask=[r"^Search time = %s seconds "],
        ignore={"final.start_time": "wall clock (time.time())",
                "final.finish_time": "wall clock (time.time())"},
        probes=["contact_surface.getNodeGroup().getNodes()",
                "contact_surface.getElements()(aka._segment_2, aka._not_ghost)"]),
    "bloc_friction_full": _case(
        "EX-045", f"{CM}/friction/bloc_friction.py",
        {"material.dat": f"{PARSER}/{CM}/friction/material.dat",
         "bloc.msh": f"{MESHES}/{CM}/friction/bloc.msh"},
        compat=True, variant="port: setUpstreamCompat(True) on the coupler"),
    # EX-043, EX-045 in the port's default mode: the
    # corrected contact (UD-363, UD-365, ...) runs to the end; equal to
    # upstream's run until the first contact, different after it by design
    # (python/tests/examples: test_contact_examples_default_mode)
    "compression_default": _case(
        "EX-043", f"{CM}/compression/compression.py", _compression,
        check="independent", oracle="compression_full"),
    "bloc_friction_default": _case(
        "EX-045", f"{CM}/friction/bloc_friction.py",
        {"material.dat": f"{PARSER}/{CM}/friction/material.dat",
         "bloc.msh": f"{MESHES}/{CM}/friction/bloc.msh"},
        check="independent", oracle="bloc_friction_full"),
    # EX-046...048: writable getElementMaterial / getExternalForce views
    # (implicit dynamics: the port's unmodified runs are compared with the
    # oracle's Eigen-pinned runs, the unmodified oracle runs are MUMPS)
    "structural_mechanics_full": _case(
        "EX-046", f"{ST}/structural_mechanics.py", {},
        oracle="structural_mechanics_eigen_full",
        env_ignore={"matplotlib": ["final.HAS_MATPLOTLIB"]}),
    "structural_mechanics_eigen_full": _case(
        "EX-046", f"{ST}/structural_mechanics.py", {},
        preparse=f"{VARIANTS}/structural_mechanics_model/eigen.dat",
        variant="Eigen pinned (a parsed model section)",
        env_ignore={"matplotlib": ["final.HAS_MATPLOTLIB"]}),
    "structural_mechanics_softening_full": _case(
        "EX-047", f"{ST}/structural_mechanics_softening.py", {},
        oracle="structural_mechanics_softening_eigen_full",
        env_ignore={"matplotlib": ["final.has_matplotlib"]}),
    "structural_mechanics_softening_eigen_full": _case(
        "EX-047", f"{ST}/structural_mechanics_softening.py", {},
        preparse=f"{VARIANTS}/structural_mechanics_model/eigen.dat",
        variant="Eigen pinned (a parsed model section)",
        env_ignore={"matplotlib": ["final.has_matplotlib"]}),
    "structural_mechanics_dynamics_full": _case(
        "EX-048", f"{ST}/structural_mechanics_dynamics.py", {},
        oracle="structural_mechanics_dynamics_eigen_full",
        env_ignore={"matplotlib": ["final.has_matplotlib"]}),
    "structural_mechanics_dynamics_eigen_full": _case(
        "EX-048", f"{ST}/structural_mechanics_dynamics.py", {},
        preparse=f"{VARIANTS}/structural_mechanics_model/eigen.dat",
        variant="Eigen pinned (a parsed model section)",
        env_ignore={"matplotlib": ["final.has_matplotlib"]}),
    # EX-038: Python subclass of MaterialCohesive (runtime cohesive law).
    # Upstream's run raises at line 191 (UD-980: its `computeTraction`
    # calls `self.getNormals`, which upstream's MaterialCohesive does not
    # bind): both runs raise there, everything before is compared (the
    # static solve, its dumps, the Python `checkInsertion` and insertion)
    "cohesive_custom_material_full": _case(
        "EX-038", f"{COH}/cohesive/custom_material.py",
        {"local_material.dat": f"{PARSER}/{COH}/cohesive/local_material.dat",
         "plate.msh": f"{MESHES}/{COH}/cohesive/plate.msh"},
        check="raises", probes=_coh_custom_probes,
        oracle="cohesive_custom_material_eigen_full"),
    "cohesive_custom_material_eigen_full": _case(
        "EX-038", f"{COH}/cohesive/custom_material.py",
        {"local_material.dat": f"{PARSER}/{COH}/cohesive/local_material.dat",
         "plate.msh": f"{MESHES}/{COH}/cohesive/plate.msh"},
        replace={"local_material.dat": f"{VARIANTS}/cohesive/local_material.dat"},
        check="raises", probes=_coh_custom_probes,
        variant="Eigen pinned (local_material.dat + solver section)"),
}
