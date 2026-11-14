// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: interoperability of the `.npy` reader with files written by
// NumPy 2.2.4 (`np.lib.format.write_array`, format versions 1.0, 2.0 and
// 3.0) in the oracle image. The files in
// tests/data/numpy were generated once with:
//
//   arrs = {"negzero_0d": np.array(-0.0), "empty_2x0x3": np.zeros((2, 0, 3)),
//           "fortran_2x3x4": np.asfortranarray(
//               np.arange(24, dtype="<f8").reshape(2, 3, 4) * 0.5 - 3.0)}
//   for v in [(1, 0), (2, 0), (3, 0)]: write_array(f, arrs[n], version=v)

use std::path::PathBuf;

use akantu_validation::npy::read_npy;

fn data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/numpy")
        .join(name)
}

#[test]
fn numpy_v1_v2_v3_files_read_back_exactly() {
    for v in 1..=3 {
        let z = read_npy(data(&format!("negzero_0d_v{v}.npy"))).unwrap();
        assert!(z.shape.is_empty(), "v{v}: 0-d");
        let zv = z.to_f64().unwrap();
        assert_eq!(zv.len(), 1);
        assert_eq!(
            zv[0].to_bits(),
            (-0.0f64).to_bits(),
            "v{v}: sign of zero kept"
        );

        let e = read_npy(data(&format!("empty_2x0x3_v{v}.npy"))).unwrap();
        assert_eq!(e.shape, vec![2, 0, 3], "v{v}");
        assert!(e.to_f64().unwrap().is_empty());

        let f = read_npy(data(&format!("fortran_2x3x4_v{v}.npy"))).unwrap();
        assert_eq!(f.shape, vec![2, 3, 4], "v{v}");
        // Converted to C order: element (i, j, k) = (12 i + 4 j + k) / 2 - 3.
        let fv = f.to_f64().unwrap();
        for i in 0..2 {
            for j in 0..3 {
                for k in 0..4 {
                    let n = 12 * i + 4 * j + k;
                    #[allow(clippy::cast_precision_loss)] // n < 24
                    let want = n as f64 * 0.5 - 3.0;
                    assert_eq!(fv[n].to_bits(), want.to_bits(), "v{v} ({i},{j},{k})");
                }
            }
        }
    }
}
