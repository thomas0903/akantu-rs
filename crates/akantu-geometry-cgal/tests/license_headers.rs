// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042). Licence-separation guard for the
// CGAL GPL port (no upstream counterpart).

//! D-042 guard: every source file of this crate carries the GPL SPDX
//! header, the manifest declares GPL-3.0-or-later, and every file deriving
//! from CGAL names its CGAL source and copyright.

use std::path::Path;

fn rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn every_file_is_gpl_and_cgal_files_carry_notices() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("license = \"GPL-3.0-or-later\""));
    let mut files = Vec::new();
    rs_files(&root.join("src"), &mut files);
    rs_files(&root.join("tests"), &mut files);
    assert!(files.len() >= 8);
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(
            text.starts_with("// SPDX-License-Identifier: GPL-3.0-or-later\n"),
            "{} lacks the GPL SPDX header",
            f.display()
        );
        if text.contains("GPL-DERIVED") {
            assert!(
                text.contains("CGAL 5.6.2") && text.contains("Copyright (c)"),
                "{}",
                f.display()
            );
        }
    }
    for gpl_module in ["src/aabb.rs", "src/spherical.rs"] {
        let text = std::fs::read_to_string(root.join(gpl_module)).unwrap();
        assert!(
            text.contains("GPL-DERIVED"),
            "{gpl_module} must mark its GPL-derived parts"
        );
    }
}
