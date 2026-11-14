# SPDX-License-Identifier: LGPL-3.0-or-later
# Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
# counterpart: case catalogue of the material-point probe
# (oracle/probes/cpp/material_probe.cc). For every case it writes the
# material .dat and the prescribed displacement-gradient history
# (grad_u.npy, [step, dim, dim]) and runs the probe binary.
#
# Usage (inside akantu-oracle:dev):
#   python3 -I material_cases.py <material_probe_binary> <fixtures_root> [case ...]
# Parameters of upstream material tests are reused where they exist
# (test/test_model/test_solid_mechanics_model/test_materials/).
from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np


def ramp(n: int, peak: float, unload: bool = True) -> np.ndarray:
    """Load 0 -> peak in n steps, then (optionally) back to -peak/2."""
    up = np.linspace(0.0, peak, n + 1)[1:]
    if not unload:
        return up
    down = np.linspace(peak, -0.5 * peak, n + 1)[1:]
    return np.concatenate([up, down])


def history(dim: int, path: str, n: int = 10, peak: float = 1e-3) -> np.ndarray:
    """Displacement-gradient histories H_k (u = H_k X)."""
    s = ramp(n, peak)
    H = np.zeros((s.size, dim, dim))
    if path == "uniaxial":
        H[:, 0, 0] = s
    elif path == "shear":
        H[:, 0, dim - 1] = s
        if dim > 1:
            H[:, dim - 1, 0] = 0.5 * s
    elif path == "hydrostatic":
        for d in range(dim):
            H[:, d, d] = s
    elif path == "composed":
        rng = np.random.default_rng(20261109)
        base = rng.uniform(-1.0, 1.0, (dim, dim))
        H[:] = s[:, None, None] * base[None]
    else:
        raise ValueError(path)
    return H


ELASTIC = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n{extra}]\n"
PLASTIC = ("material plastic_linear_isotropic_hardening [\n  name = plastic\n  rho = 7800\n"
           "  E = 2.1e11\n  nu = 0.3\n  sigma_y = 3e8\n  h = 1e9\n{extra}]\n")
NEOHOOKEAN = "material neohookean [\n  name = rubber\n  rho = 1000\n  E = 1e6\n  nu = 0.45\n{extra}]\n"
MAZARS = ("material mazars [\n  name = concrete\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  K0 = 1e-4\n"
          "  At = 1.0\n  Bt = 5e3\n  Ac = 0.8\n  Bc = 1391.3\n  beta = 1.\n{extra}]\n")
MARIGO = ("material marigo [\n  name = marigo\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n  Sd = 5e3\n"
          "  Yd = 50\n{extra}]\n")

CASES: dict[str, dict] = {}
for dim in (1, 2, 3):
    for path in ("uniaxial", "shear", "hydrostatic", "composed"):
        if dim == 1 and path != "uniaxial":
            continue
        CASES[f"elastic/{dim}d_{path}"] = dict(dim=dim, dat=ELASTIC, path=path, internals=[])
        CASES[f"plastic_linear_isotropic_hardening/{dim}d_{path}"] = dict(
            dim=dim, dat=PLASTIC, path=path, peak=5e-3,
            internals=["inelastic_strain", "iso_hardening", "plastic_energy", "d_plastic_energy"])
        CASES[f"mazars/{dim}d_{path}"] = dict(dim=dim, dat=MAZARS, path=path, peak=2e-3,
                                              internals=["damage", "Ehat"])
        CASES[f"marigo/{dim}d_{path}"] = dict(dim=dim, dat=MARIGO, path=path, peak=2e-3,
                                              internals=["damage", "Yd", "Sd"])
    if dim > 1:
        CASES[f"neohookean/{dim}d_composed"] = dict(dim=dim, dat=NEOHOOKEAN, path="composed",
                                                    peak=0.2, internals=[])
CASES["elastic/2d_uniaxial_plane_stress"] = dict(dim=2, dat=ELASTIC, path="uniaxial", internals=[],
                                                 extra="  Plane_Stress = true\n")
CASES["mazars/2d_uniaxial_plane_stress"] = dict(dim=2, dat=MAZARS, path="uniaxial", peak=2e-3,
                                                internals=["damage", "Ehat"],
                                                extra="  Plane_Stress = true\n")

# ---: elastic family (probe v3, --wave-speeds) ----------------------
# Thermal pre-step of `elastic` (material_thermal.hh): uniform delta_T.
THERMAL = ("material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n"
           "  alpha = 1.2e-5\n  delta_T = 50\n{extra}]\n")
THERMAL_INTERNALS = ["delta_T", "sigma_th", "epsilon_th"]
for dim, path, extra, suffix in ((1, "uniaxial", "", ""), (2, "uniaxial", "", ""),
                                 (2, "composed", "  Plane_Stress = true\n", "_plane_stress"),
                                 (3, "hydrostatic", "", ""), (3, "composed", "", "")):
    CASES[f"elastic_thermal/{dim}d_{path}{suffix}"] = dict(
        dim=dim, dat=THERMAL, path=path, internals=THERMAL_INTERNALS, extra=extra,
        wave_speeds=True)
# St-Venant-Kirchhoff (`elastic` + finite_deformation): the law writes the
# second Piola-Kirchhoff stress (internal piola_kirchhoff_2); `stress` stays 0.
for dim, extra, suffix in ((1, "", ""), (2, "", ""), (2, "  Plane_Stress = true\n", "_plane_stress"),
                           (3, "", "")):
    path = "uniaxial" if dim == 1 else "composed"
    CASES[f"elastic_finite_deformation/{dim}d_{path}{suffix}"] = dict(
        dim=dim, dat=ELASTIC, path=path, peak=5e-2, internals=["piola_kirchhoff_2"],
        extra="  finite_deformation = true\n" + extra, wave_speeds=True)
# eigen_grad_u parameter copied into the internal (material.cc).
CASES["elastic/2d_composed_eigen_grad_u"] = dict(
    dim=2, dat=ELASTIC, path="composed", internals=["eigen_grad_u"],
    extra="  eigen_grad_u = [[1e-4, 2e-5], [0, -3e-4]]\n")
CASES["elastic/3d_composed_eigen_grad_u"] = dict(
    dim=3, dat=ELASTIC, path="composed", internals=["eigen_grad_u"],
    extra="  eigen_grad_u = [[1e-4, 2e-5, 0], [0, -3e-4, 5e-5], [-1e-5, 0, 2e-4]]\n")
# elastic_anisotropic: the patch-test inputs (test/test_model/patch_tests/data/
# material_anisotropic_{1,2,3}.dat) and the full stiffness of
# test_elastic_materials.cc (scaled) in a rotated frame.
ANISO_1D = ("material elastic_anisotropic [\n  name = aluminum\n  rho = 1.6465129043044597\n"
            "  C11 = 105.092023\n  n1 = [-1]\n]\n")
ANISO_2D = ("material elastic_anisotropic [\n  name = aluminum\n  rho = 1.6465129043044597\n"
            "  C11 = 105.092023\n  C12 = 59.4637759\n  C22 = 105.092023\n  C33 = 30.6596356\n"
            "  n1 = [-1, 1]\n  n2 = [ 1, 1]\n]\n")
ANISO_3D = ("material elastic_anisotropic [\n  name = aluminum\n  rho = 1.6465129043044597\n"
            "  C11 = 105.092023\n  C12 = 59.4637759\n  C13 = 59.4637759\n  C14 = -9.47874286e-11\n"
            "  C15 = -9.34769857e-10\n  C16 = -3.49522561e-10\n  C22 = 105.092023\n"
            "  C23 = 59.4637759\n  C24 = -1.33279629e-10\n  C25 = 6.52290813e-11\n"
            "  C26 = -1.36100714e-10\n  C33 = 105.092023\n  C34 = 1.410383e-10\n"
            "  C35 = -3.02540691e-13\n  C36 = -8.72969656e-11\n  C44 = 30.6596356\n"
            "  C45 = 2.20340964e-11\n  C46 = -2.62259488e-10\n  C55 = 30.6596356\n"
            "  C56 = 8.56508319e-11\n  C66 = 30.6596356\n"
            "  n1 = [-1, 1,  0]\n  n2 = [ 1, 1,  1]\n  n3 = [ 1, 1, -2]\n]\n")
ANISO_2D_FULL = ("material elastic_anisotropic [\n  name = full\n  rho = 2.7e3\n"
                 "  C11 = 1.0e10\n  C12 = 0.3e10\n  C13 = 0.4e10\n  C22 = 2.0e10\n"
                 "  C23 = 0.1e10\n  C33 = 1.5e10\n  n1 = [0.8, 0.6]\n  n2 = [-0.6, 0.8]\n]\n")
ANISO_3D_FULL = ("material elastic_anisotropic [\n  name = full\n  rho = 2.9e3\n"
                 "  C11 = 1.0e10\n  C12 = 0.3e10\n  C13 = 0.4e10\n  C14 = 0.3e10\n"
                 "  C15 = 0.2e10\n  C16 = 0.1e10\n  C22 = 2.0e10\n  C23 = 0.1e10\n"
                 "  C24 = 0.2e10\n  C25 = 0.3e10\n  C26 = 0.2e10\n  C33 = 1.5e10\n"
                 "  C34 = 0.1e10\n  C35 = 0.4e10\n  C36 = 0.3e10\n  C44 = 2.4e10\n"
                 "  C45 = 0.1e10\n  C46 = 0.4e10\n  C55 = 0.9e10\n  C56 = 0.1e10\n"
                 "  C66 = 1.2e10\n  n1 = [2, 2, 1]\n  n2 = [-2, 1, 2]\n  n3 = [1, -2, 2]\n]\n")
CASES["elastic_anisotropic/1d_uniaxial"] = dict(dim=1, dat=ANISO_1D, path="uniaxial",
                                                internals=[], wave_speeds=True)
CASES["elastic_anisotropic/2d_composed"] = dict(dim=2, dat=ANISO_2D, path="composed",
                                                internals=[], wave_speeds=True)
CASES["elastic_anisotropic/3d_composed"] = dict(dim=3, dat=ANISO_3D, path="composed",
                                                internals=[], wave_speeds=True)
CASES["elastic_anisotropic/2d_composed_full"] = dict(dim=2, dat=ANISO_2D_FULL, path="composed",
                                                     internals=[], wave_speeds=True)
CASES["elastic_anisotropic/3d_composed_full"] = dict(dim=3, dat=ANISO_3D_FULL, path="composed",
                                                     internals=[], wave_speeds=True)
# elastic_orthotropic: test_mat_2 of test_material_orthotropic's
# orthotropic.dat (3-component axes truncated in 2-D, expressions), a
# rotated 2-D and a rotated 3-D material; eigen_grad_u is ignored by the
# orthotropic update (recorded).
ORTHO_2D_DAT = ("material elastic_orthotropic [\n  name = test_mat_2\n  n1 = [0, 1, 0]\n"
                "  n2 = [-1, 0, 0]\n  rho  = 3.8328782866680342684\n"
                "  E1   = 10 * 9.7874197348334701168\n  E2   = 9.7874197348334701168\n"
                "  nu12 = 0.33333333333333333333\n  G12  = 3.6702824005625512938\n]\n")
ORTHO_2D = ("material elastic_orthotropic [\n  name = ortho\n  rho = 1500\n  E1 = 1.5e11\n"
            "  E2 = 1e10\n  nu12 = 0.3\n  G12 = 5e9\n  n1 = [0.8, 0.6]\n  n2 = [-0.6, 0.8]\n{extra}]\n")
ORTHO_3D = ("material elastic_orthotropic [\n  name = ortho\n  rho = 2300\n  E1 = 1e11\n"
            "  E2 = 2e11\n  E3 = 3e11\n  nu12 = 0.1\n  nu13 = 0.2\n  nu23 = 0.3\n"
            "  G12 = 2e10\n  G13 = 3e10\n  G23 = 1e10\n  n1 = [2, 2, 1]\n  n2 = [-2, 1, 2]\n"
            "  n3 = [1, -2, 2]\n{extra}]\n")
CASES["elastic_orthotropic/2d_composed_test_mat_2"] = dict(dim=2, dat=ORTHO_2D_DAT, path="composed",
                                                           internals=[], wave_speeds=True)
CASES["elastic_orthotropic/2d_composed"] = dict(dim=2, dat=ORTHO_2D, path="composed",
                                                internals=[], wave_speeds=True)
CASES["elastic_orthotropic/3d_composed"] = dict(dim=3, dat=ORTHO_3D, path="composed",
                                                internals=[], wave_speeds=True)
CASES["elastic_orthotropic/3d_composed_eigen_grad_u"] = dict(
    dim=3, dat=ORTHO_3D, path="composed", internals=["eigen_grad_u"], wave_speeds=True,
    extra="  eigen_grad_u = [[1e-4, 0, 0], [0, -2e-4, 0], [0, 0, 3e-5]]\n")


# ---: inelastic laws (damage, plasticity, Neo-Hookean kernels) -------
# Extra strain paths (appended: the original `history` keeps its paths and
# values; main() resolves the module-global name at run time).
_history_v1 = history


def _cyclic_scale(n: int, peak: float) -> np.ndarray:
    """0 -> peak -> -peak -> 0.6 peak (load, reverse, reload)."""
    a = np.linspace(0.0, peak, n + 1)[1:]
    b = np.linspace(peak, -peak, 2 * n + 1)[1:]
    c = np.linspace(-peak, 0.6 * peak, n + 1)[1:]
    return np.concatenate([a, b, c])


def _rotation(dim: int, theta: float) -> np.ndarray:
    R = np.eye(dim)
    if dim >= 2:
        c, s = np.cos(theta), np.sin(theta)
        R[0, 0], R[0, 1], R[1, 0], R[1, 1] = c, -s, s, c
    return R


def history(dim: int, path: str, n: int = 10, peak: float = 1e-3) -> np.ndarray:  # noqa: F811
    """`_history_v1` plus the paths."""
    if path == "cyclic":
        s = _cyclic_scale(n // 2 if n > 2 else 1, peak)
        rng = np.random.default_rng(20261110)
        base = rng.uniform(-1.0, 1.0, (dim, dim))
        return s[:, None, None] * base[None]
    if path == "cyclic_uniaxial":
        s = _cyclic_scale(n // 2 if n > 2 else 1, peak)
        H = np.zeros((s.size, dim, dim))
        H[:, 0, 0] = s
        return H
    if path == "compression":
        # compression-dominated load / unload / reload (trace < 0 throughout)
        rng = np.random.default_rng(20261111)
        base = rng.uniform(-1.0, 1.0, (dim, dim)) - 1.5 * np.eye(dim)
        up = np.linspace(0.0, peak, n + 1)[1:]
        down = np.linspace(peak, 0.3 * peak, n // 2 + 1)[1:]
        re = np.linspace(0.3 * peak, 1.5 * peak, n // 2 + 1)[1:]
        s = np.concatenate([up, down, re])
        return s[:, None, None] * base[None]
    if path == "rotation":
        # rigid rotation by up to `peak` radians: H = R - I
        thetas = np.linspace(0.0, peak, 2 * n + 1)[1:]
        return np.stack([_rotation(dim, t) - np.eye(dim) for t in thetas])
    if path in FINITE_RETURN_PATHS:
        return np.array(FINITE_RETURN_PATHS[path], dtype="<f8")
    if path == "simple_shear":
        s = ramp(n, peak)
        H = np.zeros((s.size, dim, dim))
        H[:, 0, dim - 1] = s
        return H
    return _history_v1(dim, path, n=n, peak=peak)


SEED = "seed = 4242\n"
DRUCKER_PRAGER = ("material plastic_drucker_prager [\n  name = dp\n  rho = 2500\n  E = 3e10\n"
                  "  nu = 0.2\n  phi = 30\n  fc = 3e7\n{extra}]\n")
PLASTIC_MAZARS = ("material plastic_mazars [\n  name = pm\n  rho = 3000\n  E = 25e9\n  nu = 0.2\n"
                  "  sigma_y = 3e7\n  h = 2e9\n  K0 = 1e-4\n  At = 1.0\n  Bt = 5e3\n  Ac = 0.8\n"
                  "  Bc = 1391.3\n  beta = 1.\n{extra}]\n")
NEOHOOKEAN_KERNEL = ("material neohookean_kernel [\n  name = rubber\n  rho = 1000\n  E = 1e6\n"
                     "  nu = 0.45\n{extra}]\n")
PLASTIC_INTERNALS = ["inelastic_strain", "iso_hardening", "plastic_energy", "d_plastic_energy"]
# The upstream internals "damage dissipated energy" and "integral of sigma"
# have spaces in their names, which fixture quantity names cannot carry.
DAMAGE_INTERNALS = ["damage"]
THERMAL_EXTRA = "  alpha = 1e-5\n  delta_T = 80\n"

# Drucker-Prager (no upstream test): radial and non-radial return, apex
# (hydrostatic tension), compression-dominated load/unload/reload. 1-D is
# degenerate upstream (J2 = 0 -> 0/0 in the gradient) and not recorded.
for dim in (2, 3):
    for path, peak in (("compression", -2e-3), ("shear", 2e-3), ("hydrostatic", 1e-3)):
        CASES[f"plastic_drucker_prager/{dim}d_{path}"] = dict(
            dim=dim, dat=DRUCKER_PRAGER, path=path, peak=abs(peak) if path == "compression" else peak,
            internals=PLASTIC_INTERNALS, wave_speeds=True)
    CASES[f"plastic_drucker_prager/{dim}d_compression_no_radial_return"] = dict(
        dim=dim, dat=DRUCKER_PRAGER, path="compression", peak=2e-3, internals=PLASTIC_INTERNALS,
        extra="  radial_return = false\n")
CASES["plastic_drucker_prager/2d_compression_plane_stress"] = dict(
    dim=2, dat=DRUCKER_PRAGER, path="compression", peak=2e-3, internals=PLASTIC_INTERNALS,
    extra="  Plane_Stress = true\n")

# plastic_mazars (MAT-015, no upstream test): J2 parent + Mazars damage.
for dim in (1, 2, 3):
    path = "uniaxial" if dim == 1 else "composed"
    CASES[f"plastic_mazars/{dim}d_{path}"] = dict(
        dim=dim, dat=PLASTIC_MAZARS, path=path, peak=3e-3,
        internals=PLASTIC_INTERNALS + DAMAGE_INTERNALS + ["Ehat"], wave_speeds=True)
CASES["plastic_mazars/3d_cyclic"] = dict(dim=3, dat=PLASTIC_MAZARS, path="cyclic", peak=3e-3,
                                         internals=PLASTIC_INTERNALS + DAMAGE_INTERNALS)
CASES["plastic_mazars/2d_composed_plane_stress"] = dict(
    dim=2, dat=PLASTIC_MAZARS, path="composed", peak=3e-3,
    internals=PLASTIC_INTERNALS + DAMAGE_INTERNALS, extra="  Plane_Stress = true\n")
# yield before damage (K0 above the yield strain, strong hardening): the J2
# parent and the elastic-strain coupling of the equivalent strain are active
PLASTIC_MAZARS_YIELD = PLASTIC_MAZARS.replace("  K0 = 1e-4\n", "  K0 = 1.3e-3\n").replace(
    "  h = 2e9\n", "  h = 1e10\n")
for dim in (1, 2, 3):
    path = "uniaxial" if dim == 1 else "composed"
    CASES[f"plastic_mazars/{dim}d_{path}_yield_first"] = dict(
        dim=dim, dat=PLASTIC_MAZARS_YIELD, path=path, peak=6e-3,
        internals=PLASTIC_INTERNALS + DAMAGE_INTERNALS + ["Ehat"])
CASES["plastic_mazars/3d_cyclic_yield_first"] = dict(
    dim=3, dat=PLASTIC_MAZARS_YIELD, path="cyclic", peak=6e-3,
    internals=PLASTIC_INTERNALS + DAMAGE_INTERNALS)
CASES["plastic_mazars/3d_random_k0"] = dict(
    dim=3, dat=SEED + PLASTIC_MAZARS.replace("  K0 = 1e-4\n", "  K0 = 1e-4 uniform [0, 5e-5]\n"),
    path="composed", peak=3e-3, internals=DAMAGE_INTERNALS + ["K0"])

# J2, UD-030 legacy finite-deformation branch, cases in which the finite pass
# changes the result. Upstream's finite
# pass (Green strain + F, its 2-D deviator with tr/3 and no zz term) only
# leaves its `iso_hardening` for the small-strain pass, whose yield test reads
# it (UD-138): the small pass yields iff `e_s > iso_f - iso_t` (`e_s` its
# trial excess over `sigma_y + iso_t`; `iso_f - iso_t = h dp_f`). In this
# branch the trial stress of both passes is the stress of the *increment*
# only: the previous stress is the `stress` internal, which finite mode never
# writes (the result goes to `piola_kirchhoff_2`), so it stays zero.
# h = 5e10: h / (3 mu + h) = 0.171. Steps chosen with the port's bitwise
# kernels (2-D plane strain, rows of H as u = H X; `zz_j2_design` scan, not
# committed; margins in docs/NUMERICAL_VALIDATION.md):
#   shear: one step of shear gamma = 1.221292e-3 just beyond the small yield
#     (e_s = 4.1709e7 Pa) with a rotation a = 0.06 (H01 = gamma + a,
#     H10 = gamma - a) that raises the finite measure (e_f = 2.4389e8,
#     h dp_f = 4.1718e7): the small pass stays elastic by 9.5e3 Pa, inside
#     the window where Smith's denominator 3 mu + 2 h (mutant S02b) leaves a
#     smaller iso_f (S02b, S02d, S02e die here); then a larger rotation
#     (a = 0.07, finite pass elastic) and the unload to zero (finite yields,
#     small elastic: S02d, S02e); then, from rest, two one-step inputs on the
#     decision boundary to the last bit (e_s = iso_f - iso_t), each undone in
#     two elastic half steps: where 3 mu (1 + eps) (S02) or a single Newton
#     iteration (S02c) changes iso_f by a few ulps, the small pass yields
#     (found on the probe's own FE gradient of H, reproduced bit for bit by
#     the port's FE engine: one_element_2d.msh, u = H X);
#   compression: uniaxial -10% (both yield), then -11.8%: at the
#     pre-compressed state the Green increment is (1 - X) times smaller, so
#     the finite pass is elastic (e_f = -2.08e9) while the small trial is just
#     elastic (e_s = -2.89e7 > 0.171 e_f: S02f, which forces the finite
#     return, dies here), then back to -10%.
FINITE_RETURN_PATHS = {
    "finite_return_shear": [
        [[0.0, 0.061221292], [-0.058778708, 0.0]],
        [[0.0, 0.071221292], [-0.068778708, 0.0]],
        [[0.0, 0.0], [0.0, 0.0]],
        [[0.0, 0.03910483417998984], [-0.03689516582001002, 0.0]],
        [[0.0, 0.01955241708999492], [-0.01844758291000501, 0.0]],
        [[0.0, 0.0], [0.0, 0.0]],
        [[0.0, 0.033490407128090253], [-0.031309592871909805, 0.0]],
        [[0.0, 0.016745203564045127], [-0.015654796435954903, 0.0]],
        [[0.0, 0.0], [0.0, 0.0]],
    ],
    "finite_return_compression": [
        [[-0.1, 0.0], [0.0, 0.0]], [[-0.118, 0.0], [0.0, 0.0]], [[-0.1, 0.0], [0.0, 0.0]],
    ],
}
PLASTIC_FINITE_RETURN = PLASTIC.replace("  h = 1e9\n", "  h = 5e10\n")
for fpath in FINITE_RETURN_PATHS:
    CASES[f"plastic_linear_isotropic_hardening/2d_{fpath}_finite_deformation"] = dict(
        dim=2, dat=PLASTIC_FINITE_RETURN, path=fpath, peak=0.0,
        internals=PLASTIC_INTERNALS + ["piola_kirchhoff_2", "green_strain"],
        extra="  finite_deformation = true\n")

# J2: cyclic histories (Bauschinger with isotropic hardening), plane stress,
# and the UD-030 legacy finite-deformation branch.
for dim in (1, 2, 3):
    path = "cyclic_uniaxial" if dim == 1 else "cyclic"
    CASES[f"plastic_linear_isotropic_hardening/{dim}d_{path}"] = dict(
        dim=dim, dat=PLASTIC, path=path, peak=5e-3, internals=PLASTIC_INTERNALS, wave_speeds=True)
    fpath = "uniaxial" if dim == 1 else "composed"
    CASES[f"plastic_linear_isotropic_hardening/{dim}d_{fpath}_finite_deformation"] = dict(
        dim=dim, dat=PLASTIC, path=fpath, peak=2e-2,
        internals=PLASTIC_INTERNALS + ["piola_kirchhoff_2", "green_strain"],
        extra="  finite_deformation = true\n")
CASES["plastic_linear_isotropic_hardening/2d_cyclic_plane_stress"] = dict(
    dim=2, dat=PLASTIC, path="cyclic", peak=5e-3, internals=PLASTIC_INTERNALS,
    extra="  Plane_Stress = true\n")

# Marigo / Mazars: cyclic histories, plane stress, damage_in_y, epsilon_c
# (Yc limit), random Yd / K0 with a fixed input-file seed (D-006).
for law, dat in (("marigo", MARIGO), ("mazars", MAZARS)):
    CASES[f"{law}/3d_cyclic"] = dict(dim=3, dat=dat, path="cyclic", peak=2e-3,
                                     internals=DAMAGE_INTERNALS, wave_speeds=True)
    CASES[f"{law}/1d_cyclic_uniaxial"] = dict(dim=1, dat=dat, path="cyclic_uniaxial", peak=2e-3,
                                              internals=DAMAGE_INTERNALS)
CASES["marigo/2d_composed_plane_stress"] = dict(dim=2, dat=MARIGO, path="composed", peak=2e-3,
                                                internals=["damage", "Y"],
                                                extra="  Plane_Stress = true\n")
CASES["marigo/3d_composed_damage_in_y"] = dict(dim=3, dat=MARIGO, path="composed", peak=2e-3,
                                               internals=["damage", "Y"],
                                               extra="  damage_in_y = true\n")
CASES["marigo/3d_composed_epsilon_c"] = dict(dim=3, dat=MARIGO, path="composed", peak=2e-3,
                                             internals=["damage", "Y"],
                                             extra="  epsilon_c = 1e-4\n")
CASES["marigo/3d_random_yd"] = dict(
    dim=3, dat=SEED + MARIGO.replace("  Yd = 50\n", "  Yd = 50 uniform [0, 100]\n"),
    path="composed", peak=2e-3, internals=["damage", "Yd", "Y"])
CASES["mazars/3d_random_k0"] = dict(
    dim=3, dat=SEED + MAZARS.replace("  K0 = 1e-4\n", "  K0 = 1e-4 uniform [0, 5e-5]\n"),
    path="composed", peak=2e-3, internals=["damage", "K0"])

# UD-038 (thermal state of the inelastic laws), upstream behaviour: damage
# laws never refresh sigma_th; J2 uses previous - current in the trial and
# no thermal term in the final stress; Drucker-Prager the latter.
for law, dat, internals in (("marigo", MARIGO, DAMAGE_INTERNALS),
                            ("mazars", MAZARS, DAMAGE_INTERNALS),
                            ("plastic_mazars", PLASTIC_MAZARS, PLASTIC_INTERNALS + DAMAGE_INTERNALS),
                            ("plastic_linear_isotropic_hardening", PLASTIC, PLASTIC_INTERNALS)):
    CASES[f"{law}/3d_composed_thermal"] = dict(dim=3, dat=dat, path="composed", peak=2e-3,
                                               internals=internals + ["sigma_th", "epsilon_th"],
                                               extra=THERMAL_EXTRA)
CASES["plastic_drucker_prager/3d_compression_thermal"] = dict(
    dim=3, dat=DRUCKER_PRAGER, path="compression", peak=2e-3,
    internals=PLASTIC_INTERNALS + ["sigma_th", "epsilon_th"], extra=THERMAL_EXTRA)

# Neo-Hookean: upstream's kernels through the probe-only keyword
# `neohookean_kernel` (cpp/material_probe_wpmat2.cc, UD-026).
CASES["neohookean_kernel/1d_uniaxial"] = dict(dim=1, dat=NEOHOOKEAN_KERNEL, path="uniaxial",
                                              peak=0.2, internals=["piola_kirchhoff_2"],
                                              wave_speeds=True)
for dim in (2, 3):
    CASES[f"neohookean_kernel/{dim}d_composed"] = dict(
        dim=dim, dat=NEOHOOKEAN_KERNEL, path="composed", peak=0.2,
        internals=["piola_kirchhoff_2"] + (["third_axis_deformation"] if dim == 2 else []),
        wave_speeds=True)
    CASES[f"neohookean_kernel/{dim}d_rotation"] = dict(
        dim=dim, dat=NEOHOOKEAN_KERNEL, path="rotation", peak=1.2,
        internals=["piola_kirchhoff_2"])
CASES["neohookean_kernel/2d_composed_plane_stress"] = dict(
    dim=2, dat=NEOHOOKEAN_KERNEL, path="composed", peak=0.2,
    internals=["piola_kirchhoff_2", "third_axis_deformation"], extra="  Plane_Stress = true\n")
CASES["neohookean_kernel/2d_simple_shear_plane_stress"] = dict(
    dim=2, dat=NEOHOOKEAN_KERNEL, path="simple_shear", peak=0.5,
    internals=["piola_kirchhoff_2", "third_axis_deformation"], extra="  Plane_Stress = true\n")
# ---: viscoelastic, anisotropic damage, phase-field (probe v4) ------
# Keys used by these cases (all optional): "H" (explicit [step, dim, dim]
# history instead of "path"), "time_steps" ([step]), "internal_init"
# ({name: [ncomp]}), "internal_set" ({name: [step, ncomp]}), "energies",
# "post_internals", "post_energies", "repeat_stress", "steady_state_at",
# "rho", "matrix_changed".
def mat3_time_steps(nsteps: int, base: float) -> np.ndarray:
    """Recorded, varying time steps (repeated values keep the Maxwell
    stiffness unchanged, changes flag it)."""
    pattern = np.array([1.0, 0.5, 0.5, 2.0, 1.0, 1.0, 0.25, 0.25, 1.5])
    return base * np.resize(pattern, nsteps)


def mat3_hold(dim: int, path: str, n: int, peak: float, hold: int) -> np.ndarray:
    """Ramp to `peak` in n steps, hold `hold` steps (relaxation), unload to
    -peak/2 in n steps."""
    H = history(dim, path, n=n, peak=peak)
    up, down = H[:n], H[n:]
    return np.concatenate([up, np.repeat(up[-1:], hold, axis=0), down])


SLS = ("material sls_deviatoric [\n  name = anything\n  rho = 7800\n  E = 15\n  nu = 0.25\n"
       "  Eta = 10\n  Ev = 5\n{extra}]\n")
SLS_POLYMER = ("material sls_deviatoric [\n  name = polymer\n  rho = 1200\n  E = 3e9\n  nu = 0.35\n"
               "  Eta = 1e10\n  Ev = 1e9\n{extra}]\n")
SLS_INTERNALS = ["stress_dev", "history_integral", "dissipated_energy"]
SLS_ENERGIES = ["dissipated", "dissipated_sls_deviatoric"]
for dim, path, dat, extra, suffix, base in (
        (1, "uniaxial", SLS, "", "", 0.5),
        (2, "shear", SLS, "", "", 0.5),
        (2, "composed", SLS, "", "", 0.5),
        (2, "composed", SLS, "  Plane_Stress = true\n", "_plane_stress", 0.5),
        (3, "hydrostatic", SLS, "", "", 0.5),
        (3, "composed", SLS, "", "", 0.5),
        (3, "composed", SLS_POLYMER, "", "_polymer", 2.0)):
    H = mat3_hold(dim, path, 10, 1e-3, 6)
    CASES[f"sls_deviatoric/{dim}d_{path}{suffix}"] = dict(
        dim=dim, dat=dat, extra=extra, H=H, path=path, internals=SLS_INTERNALS,
        energies=SLS_ENERGIES, time_steps=mat3_time_steps(H.shape[0], base))
# Repeated residual evaluations in one step evolve the upstream history
# again (UD-063): three computeAllStresses per step.
_H = mat3_hold(2, "composed", 6, 1e-3, 3)
CASES["sls_deviatoric/2d_composed_repeat_stress"] = dict(
    dim=2, dat=SLS, H=_H, path="composed", internals=SLS_INTERNALS, energies=SLS_ENERGIES,
    time_steps=mat3_time_steps(_H.shape[0], 0.5), repeat_stress=3)
# setToSteadyState after the first (pre-strained) step, then relaxation
# under a held strain (test_material_standard_linear_solid_deviatoric_
# relaxation_tension calls setToSteadyState before its loop).
for dim in (2, 3):
    _H = np.repeat(history(dim, "composed", n=1, peak=1e-3)[:1], 12, axis=0)
    _H[6:] *= 0.5
    CASES[f"sls_deviatoric/{dim}d_steady_state"] = dict(
        dim=dim, dat=SLS, H=_H, path="held", internals=SLS_INTERNALS, energies=SLS_ENERGIES,
        time_steps=mat3_time_steps(_H.shape[0], 0.5), steady_state_at=0)

# viscoelastic_maxwell: parameters of test_material_viscoelastic_maxwell
# (material_viscoelastic_maxwell.dat), a nu != 0 variant and two branches.
MAXWELL = ("material viscoelastic_maxwell [\n  name = anything\n  rho = 7800\n  Einf = 15e6\n"
           "  nu = {nu}\n  Eta = [{eta}]\n  Ev = [{ev}]\n{{extra}}]\n")
MAXWELL_INTERNALS = ["sigma_v", "epsilon_v", "dissipated_energy", "mechanical_work",
                     "potential_energy"]
MAXWELL_POST = ["sigma_v", "epsilon_v", "dissipated_energy", "mechanical_work",
                "potential_energy"]
MAXWELL_ENERGIES = ["dissipated", "work"]
MAXWELL_POST_ENERGIES = ["potential", "dissipated", "work"]


def maxwell_dat(nu: str, eta: str, ev: str) -> str:
    return MAXWELL.format(nu=nu, eta=eta, ev=ev)


def maxwell_steps(nsteps: int, base: float, tiny_at: int | None = None) -> np.ndarray:
    dt = mat3_time_steps(nsteps, base)
    if tiny_at is not None:
        dt[tiny_at] = 1e-17  # exp(-dt/lambda) == 1: E_additional = Ev branch
    return dt


def sym_branches(dim: int, nb: int, seed: int, scale: float) -> np.ndarray:
    """Initial branch stresses: nb symmetric dim x dim tensors, stored as
    the Akantu internal (column-major per branch, branches consecutive)."""
    rng = np.random.default_rng(seed)
    out = []
    for _ in range(nb):
        a = rng.uniform(-1.0, 1.0, (dim, dim))
        a = 0.5 * (a + a.T) * scale
        out.extend(a.T.reshape(-1))  # column-major
    return np.array(out)


for dim, path, nu, eta, ev, suffix, tiny in (
        (1, "uniaxial", "0.0", "40e6", "10e6", "", None),
        (2, "composed", "0.0", "40e6", "10e6", "", 5),
        (2, "composed", "0.3", "40e6, 2e6", "10e6, 5e6", "_two_branches", None),
        (3, "composed", "0.3", "40e6", "10e6", "", None),
        (3, "shear", "0.25", "40e6, 2e6", "10e6, 5e6", "_two_branches", 7)):
    H = mat3_hold(dim, path, 8, 1e-2, 5)
    CASES[f"viscoelastic_maxwell/{dim}d_{path}{suffix}"] = dict(
        dim=dim, dat=maxwell_dat(nu, eta, ev), H=H, path=path, internals=MAXWELL_INTERNALS,
        post_internals=MAXWELL_POST, energies=MAXWELL_ENERGIES,
        post_energies=MAXWELL_POST_ENERGIES, matrix_changed=True,
        time_steps=maxwell_steps(H.shape[0], 0.5, tiny))
# Non-zero initial branch stresses: exercises the upstream stress kernel,
# compliance and energies with sigma_v != 0 (upstream never updates sigma_v,
# UD-029, so the initial value persists).
for dim, nu, eta, ev, nb in ((2, "0.3", "40e6, 2e6", "10e6, 5e6", 2), (3, "0.25", "40e6", "10e6", 1)):
    H = mat3_hold(dim, "composed", 6, 1e-2, 3)
    CASES[f"viscoelastic_maxwell/{dim}d_composed_initial_branch_stress"] = dict(
        dim=dim, dat=maxwell_dat(nu, eta, ev), H=H, path="composed",
        internals=MAXWELL_INTERNALS, post_internals=MAXWELL_POST, energies=MAXWELL_ENERGIES,
        post_energies=MAXWELL_POST_ENERGIES, matrix_changed=True,
        time_steps=maxwell_steps(H.shape[0], 0.5),
        internal_init={"sigma_v": sym_branches(dim, nb, 20261110 + dim, 1e5)})

# phasefield (material_phasefield*, energy splits of src/model/phase_field):
# the damage is written by the coupler in the phase-field model; here it is
# prescribed per step (--internal-set damage), increasing.
PHASEFIELD = ("material phasefield {split} [\n  name = plate\n  rho = 1.\n  E = 210.0\n"
              "  nu = 0.3\n{{extra}}]\n")
PF_INTERNALS = ["damage", "damage dissipated energy", "integral of sigma", "sigma_th",
                "epsilon_th"]


def pf_damage(nsteps: int) -> np.ndarray:
    d = np.minimum(0.97, np.linspace(0.0, 1.2, nsteps) ** 1.5)
    return d.reshape(nsteps, 1)


for split in ("no_split", "volumetric_deviatoric"):
    for dim, path, extra, suffix in (
            (1, "uniaxial", "", ""),
            (2, "composed", "", ""),
            (2, "composed", "  Plane_Stress = true\n", "_plane_stress"),
            (2, "hydrostatic", "  eta = 1e-3\n", "_eta"),
            (3, "composed", "", ""),
            (3, "hydrostatic", "  eta = 1e-3\n", "_eta"),
            (2, "composed", "  alpha = 1e-5\n  delta_T = 30\n", "_thermal"),
            (3, "composed", "  alpha = 1e-5\n  delta_T = 30\n", "_thermal"),
            (2, "composed", "  finite_deformation = true\n", "_finite_deformation"),
            (2, "composed", "  degrade_mass = true\n  eta = 1e-2\n", "_degrade_mass")):
        H = history(dim, path, n=10, peak=1e-3 if "finite" not in suffix else 5e-2)
        c = dict(dim=dim, dat=PHASEFIELD.format(split=split), extra=extra, H=H, path=path,
                 internals=PF_INTERNALS, energies=["dissipated"],
                 post_internals=["damage dissipated energy", "integral of sigma"],
                 post_energies=["dissipated"],
                 internal_set={"damage": pf_damage(H.shape[0])})
        if "finite" in suffix:
            c["internals"] = PF_INTERNALS + ["piola_kirchhoff_2"]
        if "degrade_mass" in suffix:
            c["rho"] = True
            c["matrix_changed"] = True
        CASES[f"phasefield_{split}/{dim}d_{path}{suffix}"] = c

# anisotropic_damage (material_anisotropic_damage*): no upstream test. As
# coded upstream the stress reads the never-written `elastic_stress`
# internal and the spectral helpers leave matrix entries uninitialised
# (UD-031/UD-058); the cases record what the oracle produces where it does
# not depend on uninitialised memory: the `damage` internal does (it differed
# between two identical runs) and is not recorded, and the 3-D paths throw
# "complex eigenvectors" at a data-dependent step (0 and 12 in the
# runs), so only 1-D and 2-D cases are kept.
# The option `mazars-drucker-prager` cannot be written in an input file
# (section options are identifiers without '-', input_file_parser.hh) and
# its `k` is parsable only, so only the default `mazars` option is probed.
ANISO_DAMAGE = ("material anisotropic_damage [\n  name = concrete\n  rho = 2400\n"
                "  E = 30e9\n  nu = 0.2\n  Dc = 0.99\n{extra}]\n")
AD_INTERNALS = ["elastic_stress", "equivalent_strain", "trace_damage"]
for dim, path in ((1, "uniaxial"), (2, "uniaxial"), (2, "composed")):
    H = history(dim, path, n=10, peak=2e-4)
    CASES[f"anisotropic_damage_mazars/{dim}d_{path}"] = dict(
        dim=dim, dat=ANISO_DAMAGE, H=H, path=path, internals=AD_INTERNALS)

# ---: post-step work / dissipation ------
# MaterialDamage::updateEnergies (material_damage_tmpl.hh) runs in
# afterSolveStep: "integral of sigma" += ½(σ + σ_prev):Δ∇u and
# "damage dissipated energy" = ∫σ − ψ, integrated by getEnergy("dissipated").
# Every marigo / mazars / plastic_mazars case (all load, unload and, for the
# cyclic paths, reverse) records them after afterSolveStep(true) through the
# probe-v4 post-step mechanism (names with spaces are mapped to space-free
# quantity names, manifest parameter internal_names). Additive: the
# existing quantities of these cases and every other case are unchanged.
DAMAGE_POST_INTERNALS = ["damage dissipated energy", "integral of sigma"]
DAMAGE_POST_ENERGIES = ["dissipated"]
for _name, _case in CASES.items():
    if _name.split("/")[0] in ("marigo", "mazars", "plastic_mazars"):
        _case["post_internals"] = DAMAGE_POST_INTERNALS
        _case["post_energies"] = DAMAGE_POST_ENERGIES

# --- -------------------------------------------------------------
# (1) MaterialPlastic::updateEnergies (material_plastic.cc)
# runs in afterSolveStep, so the plastic work recorded with the stresses
# lags one step; record it after afterSolveStep(true) too, with
# getEnergy("plastic"). Additive, like the records above.
PLASTIC_POST_INTERNALS = ["plastic_energy", "d_plastic_energy"]
PLASTIC_POST_ENERGIES = ["plastic"]
for _name, _case in CASES.items():
    if _name.split("/")[0] in ("plastic_linear_isotropic_hardening", "plastic_drucker_prager",
                               "plastic_mazars"):
        _case["post_internals"] = list(_case.get("post_internals", [])) + PLASTIC_POST_INTERNALS
        _case["post_energies"] = list(_case.get("post_energies", [])) + PLASTIC_POST_ENERGIES

# (2) The committed cases of these laws
# were generated on aarch64 before the post-step records existed. Their
# twins `<law>/x86_64/<case>` are the same cases generated on the pinned
# x86_64 oracle (D-043, D-050) with every post-step record; the aarch64
# goldens are kept unchanged beside them (ORACLE_BUILD section 11).
X86_TWIN_LAWS = ("marigo", "mazars", "plastic_mazars", "plastic_linear_isotropic_hardening",
                 "plastic_drucker_prager")
for _name in [n for n in CASES if n.split("/")[0] in X86_TWIN_LAWS]:
    _law, _rest = _name.split("/", 1)
    CASES[f"{_law}/x86_64/{_rest}"] = dict(CASES[_name])

# (3) Thermal loading of viscoelastic_maxwell in upstream's
# behaviour (UD-327 compat mode: the parent's thermal stress added to the
# instantaneous-modulus stress, material_viscoelastic_maxwell.cc). New
# cases (x86_64), next to the isothermal ones.
for dim, path, nu, eta, ev, suffix in (
        (1, "uniaxial", "0.0", "40e6", "10e6", ""),
        (2, "composed", "0.3", "40e6, 2e6", "10e6, 5e6", "_two_branches"),
        (3, "composed", "0.3", "40e6", "10e6", "")):
    H = mat3_hold(dim, path, 8, 1e-2, 5)
    CASES[f"viscoelastic_maxwell/{dim}d_{path}{suffix}_thermal"] = dict(
        dim=dim, dat=maxwell_dat(nu, eta, ev), extra=THERMAL_EXTRA, H=H, path=path,
        internals=MAXWELL_INTERNALS + ["sigma_th", "epsilon_th"], post_internals=MAXWELL_POST,
        energies=MAXWELL_ENERGIES, post_energies=MAXWELL_POST_ENERGIES, matrix_changed=True,
        time_steps=maxwell_steps(H.shape[0], 0.5))

# (4) Goldens of the
# UD-031 zero-initialisation VARIANT of the oracle (D-003 A: upstream's
# tensorPlusOp leaves the off-diagonal entries of A_diag uninitialised; the
# variant patch only zero-initialises them, which is the port's defined
# behaviour). The variant carries a second patch (UD-814,
# MatrixBase::eig returns the solver's uninitialised
# eigenpairs after NoConvergence, reached in 3-D once UD-763 makes the damage
# NaN; the patch returns zero eigenpairs instead). Generated only on an
# install carrying both patches (checked below against the VARIANT record of
# the installed library), labelled in the manifest
# (parameters.oracle_variant, with the UD-814 patch under
# additional_patches). With the damage defined, `damage` is recorded and the
# 3-D paths are probed; the mazars-drucker-prager option (which an input file
# cannot spell) is instantiated through the C++ factory (probe
# --factory-option).
UD031_VARIANT = {
    "patch": "oracle/patches/variants/ud031-anisotropic-damage-zero-init.patch",
    "sha256": "727840ded3d48b360f831275a04a3337249b297af2201bd81696bb10ae7578af",
    "defect": "UD-031",
}
UD814_PATCH = {
    "patch": "oracle/patches/variants/ud814-eig-noconvergence-zero-init.patch",
    "sha256": "a0d3bea4671f13aaad7d3cbcfcf9c9aa7acee7a538f7bde7de810e8b45011f07",
    "defect": "UD-814",
}
VARIANT_PATCHES = (UD031_VARIANT, UD814_PATCH)
UD031_MARKER = "akantu-rs UD-031 variant"
UD031_FILE = "src/model/solid_mechanics/materials/material_damage/material_anisotropic_damage_tmpl.hh"
ANISO_DAMAGE_DP = ANISO_DAMAGE.replace("  Dc = 0.99\n", "  Dc = 0.99\n  k = 0.1\n")
for dim, path, dat, opt, suffix in (
        (1, "uniaxial", ANISO_DAMAGE, None, ""),
        (2, "uniaxial", ANISO_DAMAGE, None, ""),
        (2, "composed", ANISO_DAMAGE, None, ""),
        (3, "uniaxial", ANISO_DAMAGE, None, ""),
        (3, "composed", ANISO_DAMAGE, None, ""),
        (2, "composed", ANISO_DAMAGE_DP, "mazars-drucker-prager", "_drucker_prager"),
        (3, "composed", ANISO_DAMAGE_DP, "mazars-drucker-prager", "_drucker_prager")):
    H = history(dim, path, n=10, peak=2e-4)
    c = dict(dim=dim, dat=dat, H=H, path=path, internals=AD_INTERNALS + ["damage"],
             oracle_variant=UD031_VARIANT, oracle_variant_extra=[UD814_PATCH])
    if opt:
        c["factory_option"] = opt
    CASES[f"anisotropic_damage_mazars/variant_ud031/{dim}d_{path}{suffix}"] = c


def variant_build_state() -> bool | None:
    """True if the INSTALLED oracle library of this run is the UD-031 variant
    (UD-031 + UD-814 patches), False if the build is regular, None if it
    cannot be told or is inconsistent (refuse every case then).

    The source marker alone is not enough (a failed
    rebuild after patching leaves the marker but the unpatched library), so
    the decision rests on /build/install-<config>/VARIANT, written by
    oracle/scripts/build-ud031-variant.sh after the install: it must list
    exactly the two patches with the sha256 of this checkout, and the sha256
    it records for the library must be the one of the installed library.
    """
    import hashlib
    import os
    config = os.environ.get("AKANTU_PROBE_CONFIG", "strict")
    marker_file = Path(f"/build/src-{config}") / UD031_FILE
    marked = marker_file.is_file() and UD031_MARKER in marker_file.read_text(
        encoding="utf-8", errors="replace")
    record = Path(f"/build/install-{config}/VARIANT")
    if not record.is_file():
        if marked:
            print("material_cases: patched sources but no VARIANT record of the install "
                  f"({record}): the variant library was not installed", file=sys.stderr)
            return None
        return False
    lines = record.read_text(encoding="utf-8").splitlines()
    patches = [ln.split() for ln in lines if ln.startswith("patch ")]
    wanted = [["patch", v["patch"], f"sha256={v['sha256']}"] for v in VARIANT_PATCHES]
    if patches != wanted:
        print(f"material_cases: {record} lists {patches}, expected {wanted}", file=sys.stderr)
        return None
    libs = [ln.split() for ln in lines if ln.startswith("library ")]
    if len(libs) != 1 or len(libs[0]) < 3 or not libs[0][2].startswith("sha256="):
        print(f"material_cases: {record} has no library record", file=sys.stderr)
        return None
    lib = Path(f"/build/install-{config}/lib") / libs[0][1]
    if not lib.is_file():
        print(f"material_cases: {lib} (named in {record}) is missing", file=sys.stderr)
        return None
    digest = hashlib.sha256(lib.read_bytes()).hexdigest()
    if libs[0][2] != f"sha256={digest}":
        print(f"material_cases: installed {lib.name} sha256={digest} is not the variant "
              f"library of {record} ({libs[0][2]})", file=sys.stderr)
        return None
    if not marked:
        print(f"material_cases: {record} present but {marker_file} lacks the marker",
              file=sys.stderr)
        return None
    return True


def main() -> int:
    if len(sys.argv) < 3:
        print(f"usage: {sys.argv[0]} <material_probe> <fixtures_root> [case ...]", file=sys.stderr)
        return 2
    probe, root = sys.argv[1], sys.argv[2]
    wanted = sys.argv[3:] or list(CASES)
    # Variant goldens only from a variant build, regular goldens never from
    # one (ORACLE_BUILD "Oracle variants"); AKANTU_PROBE_ALLOW_ON_VARIANT=1
    # stages regular cases on a variant build for the inertness check only.
    variant = variant_build_state()
    allow_on_variant = __import__("os").environ.get("AKANTU_PROBE_ALLOW_ON_VARIANT") == "1"
    for name in wanted:
        is_variant_case = "oracle_variant" in CASES[name]
        if variant is None:
            print(f"material_cases: {name}: the oracle install is neither a regular build nor "
                  "a complete UD-031 variant install (see above)", file=sys.stderr)
            return 2
        if is_variant_case and variant is not True:
            print(f"material_cases: {name} needs the UD-031 variant install "
                  "(/build/install-*/VARIANT, oracle/scripts/build-ud031-variant.sh)",
                  file=sys.stderr)
            return 2
        if not is_variant_case and variant is True and not allow_on_variant:
            print(f"material_cases: {name} is a regular case but the build carries the UD-031 "
                  "variant patch (set AKANTU_PROBE_ALLOW_ON_VARIANT=1 to stage it for the "
                  "inertness check)", file=sys.stderr)
            return 2
    failures = 0
    for name in wanted:
        c = CASES[name]
        with tempfile.TemporaryDirectory() as tmpd:
            tmp = Path(tmpd)
            dat = tmp / "material.dat"
            dat.write_text(c["dat"].format(extra=c.get("extra", "")), encoding="utf-8")
            H = c["H"] if "H" in c else history(c["dim"], c["path"], peak=c.get("peak", 1e-3))
            gfile = tmp / "grad_u.npy"
            np.save(gfile, np.ascontiguousarray(H, dtype="<f8"), allow_pickle=False)
            cmd = [probe, root, f"material/{name}", str(c["dim"]), str(dat), str(gfile),
                   "--description",
                   f"material point: {name} ({c['path']} load/unload path, {H.shape[0]} steps)"]
            for i in c["internals"]:
                cmd += ["--internal", i]
            if c.get("wave_speeds"):
                cmd += ["--wave-speeds"]
            # probe v4 options
            if "time_steps" in c:
                tfile = tmp / "time_steps.npy"
                np.save(tfile, np.ascontiguousarray(c["time_steps"], dtype="<f8"),
                        allow_pickle=False)
                cmd += ["--time-steps", str(tfile)]
            for flag, key in (("--internal-init", "internal_init"),
                              ("--internal-set", "internal_set")):
                for k, (iname, values) in enumerate(c.get(key, {}).items()):
                    vfile = tmp / f"{key}_{k}.npy"
                    np.save(vfile, np.ascontiguousarray(values, dtype="<f8"), allow_pickle=False)
                    cmd += [flag, iname, str(vfile)]
            for e in c.get("energies", []):
                cmd += ["--energy", e]
            for i in c.get("post_internals", []):
                cmd += ["--post-internal", i]
            for e in c.get("post_energies", []):
                cmd += ["--post-energy", e]
            if "repeat_stress" in c:
                cmd += ["--repeat-stress", str(c["repeat_stress"])]
            if "steady_state_at" in c:
                cmd += ["--steady-state-at", str(c["steady_state_at"])]
            if c.get("rho"):
                cmd += ["--rho"]
            if c.get("matrix_changed"):
                cmd += ["--matrix-changed"]
            # probe v5 options
            if "factory_option" in c:
                cmd += ["--factory-option", c["factory_option"]]
            if "oracle_variant" in c:
                v = c["oracle_variant"]
                cmd += ["--oracle-variant", v["patch"], v["sha256"], v["defect"]]
                for v in c.get("oracle_variant_extra", []):
                    cmd += ["--oracle-variant", v["patch"], v["sha256"], v["defect"]]
            r = subprocess.run(cmd, check=False)
            if r.returncode != 0:
                print(f"material_cases: {name} failed (exit {r.returncode})", file=sys.stderr)
                failures += 1
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
