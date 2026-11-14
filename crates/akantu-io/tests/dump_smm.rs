// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests
// of the derived solid-mechanics dump fields (DUMP-024…030) against the
// oracle cases fixtures/dump/smm2d and smm3d (oracle/probes/dump): the
// resolver serves the oracle's flattened grad_u / stress internals,
// material indices and law parameters, and the dumpers' files are compared
// with the files of SolidMechanicsModel's dumpers.

mod dump_common;

use std::collections::BTreeMap;
use std::path::Path;

use akantu_core::{ElementKind, ElementType, Error, GhostType, Real, Result};
use akantu_io::{
    DumpBuffer, Dumper, DumperParaview, FieldDescriptor, FieldResolver, FieldSource,
    solid_mechanics_field,
};
use akantu_mesh::{Mesh, MeshAccessor};
use dump_common::{Inputs, assert_same_file, fixtures, key, m2_mesh, scratch};

const NG: GhostType = GhostType::NotGhost;

struct Smm {
    inputs: Inputs,
    dim: usize,
    nb_materials: usize,
}

impl Smm {
    fn mat(&self, ty: ElementType) -> Vec<i32> {
        self.inputs.ints(&key("mat", ty))
    }

    fn per_material(&self, name: &str) -> Vec<Real> {
        (0..self.nb_materials)
            .map(|m| self.inputs.value(&format!("{name}:{m}")))
            .collect()
    }
}

impl FieldResolver for Smm {
    fn resolve(
        &self,
        _: &Mesh,
        f: &FieldDescriptor,
        ty: Option<ElementType>,
        _: GhostType,
    ) -> Result<DumpBuffer> {
        let i = &self.inputs;
        Ok(match (&f.source, ty) {
            (FieldSource::Dof { family, order: 0 }, None) if family == "displacement" => {
                DumpBuffer::Real {
                    nb_component: self.dim,
                    values: i.reals("displacement"),
                }
            }
            (FieldSource::Blocked { .. }, None) => DumpBuffer::Bool {
                nb_component: self.dim,
                values: i.bools("blocked_dofs"),
            },
            (FieldSource::ModelElemental(n), Some(t)) if n == "material_index" => DumpBuffer::Int {
                nb_component: 1,
                values: self.mat(t),
            },
            (FieldSource::Internal { name, law: None }, Some(t))
                if name == "grad_u" || name == "stress" =>
            {
                DumpBuffer::Real {
                    nb_component: self.dim * self.dim,
                    values: i.reals(&key(name, t)),
                }
            }
            _ => return Err(Error::not_found("smm field", f.name.clone())),
        })
    }

    fn law_parameter(
        &self,
        _: &Mesh,
        ty: ElementType,
        _: GhostType,
        name: &str,
    ) -> Result<Vec<Real>> {
        let per = match name {
            "nu" => self.per_material("nu"),
            "Plane_Stress" => self.per_material("plane_stress"),
            _ => return Err(Error::not_found("law parameter", name)),
        };
        Ok(self
            .mat(ty)
            .iter()
            .map(|&m| per[usize::try_from(m).unwrap()])
            .collect())
    }
}

fn field(name: &str, padding: bool) -> FieldDescriptor {
    solid_mechanics_field(name, None, padding, |n| n == "grad_u" || n == "stress").unwrap()
}

/// The three dumpers of the probe's `smm_dumpers` (default base64 model
/// dumper, ASCII unpadded, ASCII padded), with upstream's layout
/// (`compat`) or the corrected one.
fn run_dumpers(mesh: &Mesh, r: &Smm, out: &Path, base: &str, strain_tensor: bool, compat: bool) {
    let dim = mesh.spatial_dimension();
    let pv = out.to_str().unwrap();
    let mut model = DumperParaview::new(base, pv, true);
    model.set_compat_layout(compat);
    model.register_mesh(dim, NG, ElementKind::Regular).unwrap();
    model.register_field(field("displacement", true)).unwrap();
    model.register_field(field("blocked_dofs", false)).unwrap();
    model.register_field(field("stress", true)).unwrap();
    if strain_tensor {
        model.register_field(field("strain", true)).unwrap();
        model.register_field(field("Green strain", true)).unwrap();
    }
    for n in [
        "principal strain",
        "principal Green strain",
        "Von Mises stress",
        "grad_u",
        "material_index",
        "partitions",
    ] {
        model.register_field(field(n, false)).unwrap();
    }
    model.dump(mesh, r, None, None).unwrap();

    let mut ascii = DumperParaview::new(&format!("{base}_ascii"), pv, true);
    ascii.set_compat_layout(compat);
    ascii.set_text_mode().unwrap();
    ascii.register_mesh(dim, NG, ElementKind::Regular).unwrap();
    for n in [
        "displacement",
        "stress",
        "strain",
        "Green strain",
        "principal strain",
        "principal Green strain",
        "Von Mises stress",
        "grad_u",
    ] {
        ascii.register_field(field(n, false)).unwrap();
    }
    ascii.dump(mesh, r, None, None).unwrap();

    let mut pad = DumperParaview::new(&format!("{base}_ascii_pad"), pv, true);
    pad.set_compat_layout(compat);
    pad.set_text_mode().unwrap();
    pad.register_mesh(dim, NG, ElementKind::Regular).unwrap();
    pad.register_field(field("displacement", true)).unwrap();
    pad.register_field(field("stress", true)).unwrap();
    if strain_tensor {
        pad.register_field(field("strain", true)).unwrap();
    }
    pad.dump(mesh, r, None, None).unwrap();
}

/// `DataArray` name → (opening tag, contents) of a `.vtu` file.
fn data_arrays(path: &Path) -> BTreeMap<String, (String, String)> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut out = BTreeMap::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<DataArray ") {
        let tag_end = rest[start..].find(">\n").unwrap() + start;
        let tag = &rest[start..=tag_end];
        let name = tag
            .split("Name=\"")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let body_end = rest[tag_end..].find("\n    </DataArray>").unwrap() + tag_end;
        out.insert(
            name.to_owned(),
            (tag.to_owned(), rest[tag_end + 2..body_end].to_owned()),
        );
        rest = &rest[body_end..];
    }
    out
}

/// Decode an IOHelper base64 data array (header + payload in one stream)
/// into its payload doubles.
fn decode_f64(b64: &str) -> Vec<f64> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            _ => 63,
        }
    };
    let mut bytes = Vec::new();
    for g in b64.trim().as_bytes().chunks(4) {
        let pad = g.iter().filter(|&&c| c == b'=').count();
        let n = g.iter().take(4 - pad).fold(0u32, |a, &c| (a << 6) | val(c)) << (6 * pad);
        let b = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        bytes.extend_from_slice(&b[..3 - pad]);
    }
    bytes[4..]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|c| f64::from_le_bytes(*c))
        .collect()
}

/// Upstream (compat layout) writes the homogenised padded 2-D stress as
/// 81 values per element: the 3×3 block at `9i + j`. In-plane entries and
/// σzz of plane-strain elements are deterministic; the out-of-plane
/// shears (and σzz of plane-stress elements) are uninitialised memory
/// upstream and zero in akantu-rs.
fn check_padded_stress(got: &[String], want: &[String], plane_stress: &[bool], zero: &str) {
    assert_eq!(got.len(), want.len());
    assert_eq!(got.len(), plane_stress.len() * 81);
    for (e, &ps) in plane_stress.iter().enumerate() {
        for k in 0..81 {
            let (i, j) = (k / 9, k % 9);
            let g = &got[81 * e + k];
            let w = &want[81 * e + k];
            let deterministic = i >= 3 || j >= 3 || (i < 2 && j < 2) || (i == 2 && j == 2 && !ps);
            if deterministic {
                assert_eq!(g, w, "element {e} entry ({i},{j})");
            } else {
                assert_eq!(
                    g, zero,
                    "element {e} entry ({i},{j}): akantu-rs pads with zeros"
                );
            }
        }
    }
}

fn smm2d_mesh() -> Mesh {
    m2_mesh(false)
}

/// Elements of the 2-D case in dump order with their law's
/// `Plane_Stress` flag (triangles, then quadrangles).
fn plane_stress_flags(r: &Smm) -> Vec<bool> {
    let ps = r.per_material("plane_stress");
    [ElementType::Triangle3, ElementType::Quadrangle4]
        .into_iter()
        .flat_map(|t| r.mat(t))
        .map(|m| ps[usize::try_from(m).unwrap()] != 0.0)
        .collect()
}

#[test]
fn smm2d_derived_fields_match_oracle() {
    let r = Smm {
        inputs: Inputs::load("smm2d"),
        dim: 2,
        nb_materials: 2,
    };
    let mesh = smm2d_mesh();
    let dir = scratch("smm2d");
    run_dumpers(&mesh, &r, &dir, "smm2d", false, true);
    // corrected layout: valid files (checked by dump_properties-style
    // readers and oracle/probes/dump/check_vtu.py)
    run_dumpers(&mesh, &r, &scratch("smm2d_corrected"), "smm2d", true, false);
    let want = fixtures().join("smm2d/paraview");
    for p in [
        "smm2d_0000.pvtu",
        "smm2d_ascii_0000.pvtu",
        "smm2d_ascii_pad_0000.pvtu",
    ] {
        assert_same_file(&dir.join(p), &want.join(p));
    }
    // ASCII unpadded: no padder involved, every byte deterministic.
    let p = "smm2d_ascii-VTU/smm2d_ascii_0000.vtu";
    assert_same_file(&dir.join(p), &want.join(p));
    let flags = plane_stress_flags(&r);
    assert_eq!(flags, vec![false, true, false, false, true]);
    for (p, binary) in [
        ("smm2d-VTU/smm2d_0000.vtu", true),
        ("smm2d_ascii_pad-VTU/smm2d_ascii_pad_0000.vtu", false),
    ] {
        let g = data_arrays(&dir.join(p));
        let w = data_arrays(&want.join(p));
        assert_eq!(g.keys().collect::<Vec<_>>(), w.keys().collect::<Vec<_>>());
        for (name, (gt, gb)) in &g {
            let (wt, wb) = &w[name];
            assert_eq!(gt, wt, "{p}: {name}");
            if name != "stress" {
                assert_eq!(gb, wb, "{p}: {name}");
                continue;
            }
            if binary {
                let fmt = |v: Vec<f64>| {
                    v.iter()
                        .map(|x| format!("{:016x}", x.to_bits()))
                        .collect::<Vec<_>>()
                };
                check_padded_stress(
                    &fmt(decode_f64(gb)),
                    &fmt(decode_f64(wb)),
                    &flags,
                    "0000000000000000",
                );
            } else {
                let split = |s: &str| s.split_whitespace().map(str::to_owned).collect::<Vec<_>>();
                check_padded_stress(&split(gb), &split(wb), &flags, "0.000000000000000e+00");
            }
        }
    }
}

fn smm3d_mesh(i: &Inputs) -> Mesh {
    let mut m = Mesh::new(3, "smm3d").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in i.reals("nodes").chunks(3) {
            a.push_node(x).unwrap();
        }
        a.connectivity_mut(ElementType::Hexahedron8, NG)
            .push_tuple(&[0, 1, 2, 3, 4, 5, 6, 7]);
        let mut t = a.connectivity_mut(ElementType::Tetrahedron4, NG);
        t.push_tuple(&[4, 5, 7, 8]);
        t.push_tuple(&[5, 6, 7, 8]);
    }
    m
}

/// 3-D: no padder is involved, so with upstream's layout the whole tree
/// (strain fields as 81 values per cell, principal strains as 9) is
/// byte-identical.
#[test]
fn smm3d_derived_fields_match_oracle() {
    let r = Smm {
        inputs: Inputs::load("smm3d"),
        dim: 3,
        nb_materials: 2,
    };
    let mesh = smm3d_mesh(&r.inputs);
    let dir = scratch("smm3d");
    run_dumpers(&mesh, &r, &dir, "smm3d", true, true);
    run_dumpers(&mesh, &r, &scratch("smm3d_corrected"), "smm3d", true, false);
    dump_common::assert_same_tree(&dir, &fixtures().join("smm3d/paraview"));
}

/// Upstream's padded 2-D strain aborts at field registration (UD-112:
/// bare `throw;` in `FieldComputeProxy::connectToFunctor`,
/// `fixtures/dump/smm2d_strain_pad`);
/// akantu-rs pads to 3×3 (DUMP-030): plane stress εzz = ν/(ν−1)(εxx+εyy),
/// plane strain εzz = 0, zero out-of-plane shears, 9 components. Checked
/// bitwise against a direct recomputation from the oracle's `grad_u`
/// (per point, then averaged from +0 like the homogeniser).
#[test]
fn smm2d_padded_strain_is_written() {
    let status = std::fs::read_to_string(fixtures().join("smm2d_strain_pad/status.txt")).unwrap();
    assert!(status.contains("exit_code: 134"), "{status}");
    let r = Smm {
        inputs: Inputs::load("smm2d"),
        dim: 2,
        nb_materials: 2,
    };
    let mesh = smm2d_mesh();
    let dir = scratch("smm2d_strain_pad");
    let mut d = DumperParaview::new("pad", dir.to_str().unwrap(), true);
    d.register_mesh(2, NG, ElementKind::Regular).unwrap();
    d.register_field(field("strain", true)).unwrap();
    d.dump(&mesh, &r, None, None).unwrap();
    let a = data_arrays(&dir.join("pad-VTU/pad_0000.vtu"));
    let (tag, body) = &a["strain"];
    assert!(tag.contains("NumberOfComponents=\"9\""), "{tag}");
    let got = decode_f64(body);
    let nu = r.per_material("nu");
    let ps = r.per_material("plane_stress");
    let mut expected = Vec::new();
    for ty in [ElementType::Triangle3, ElementType::Quadrangle4] {
        let g = r.inputs.reals(&key("grad_u", ty));
        let mats = r.mat(ty);
        let nq = g.len() / 4 / mats.len();
        for (e, &m) in mats.iter().enumerate() {
            let m = usize::try_from(m).unwrap();
            let mut acc = [0.0; 9];
            for q in 0..nq {
                let b = &g[(e * nq + q) * 4..(e * nq + q + 1) * 4];
                let exx = 0.5 * (b[0] + b[0]);
                let exy = 0.5 * (b[2] + b[1]);
                let eyx = 0.5 * (b[1] + b[2]);
                let eyy = 0.5 * (b[3] + b[3]);
                let ezz = if ps[m] != 0.0 {
                    nu[m] / (nu[m] - 1.) * (exx + eyy)
                } else {
                    0.0
                };
                let blk = [exx, eyx, 0.0, exy, eyy, 0.0, 0.0, 0.0, ezz];
                for (a, v) in acc.iter_mut().zip(blk) {
                    *a += v;
                }
            }
            #[allow(clippy::cast_precision_loss)]
            expected.extend(acc.iter().map(|a| a / nq as f64));
        }
    }
    assert_eq!(got.len(), expected.len());
    for (k, (g, w)) in got.iter().zip(&expected).enumerate() {
        assert_eq!(g.to_bits(), w.to_bits(), "value {k}: {g} vs {w}");
    }
}
