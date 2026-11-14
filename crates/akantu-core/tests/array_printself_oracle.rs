// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_array_tmpl.hh
// (Array<T>::printself) and src/common/aka_circular_array_inline_impl.hh
// (CircularArray<T>::printself), as printed by `std::cout << array` from a
// user program; oracle texts in `tests/data/oracle/v104_printself.txt`.

//! CORE-013, CORE-037: `print_self` with the default
//! [`ArrayPrint`] reproduces the 120 oracle texts of upstream's `printself`
//! called from a user translation unit (`AKANTU_MODULE` undefined, so its
//! debug module is the literal `"AKANTU_MODULE"`): `Array<double>`,
//! `Array<int>`, `CircularArray<double>`, `CircularArray<int>` × debug
//! levels warning / dump / test × module filters none / core / mesh /
//! AKANTU_MODULE / core + AKANTU_MODULE × indents 0 / 2
//! (`tests/data/oracle/v104_printself.txt`, provenance in its header;
//! addresses masked). With the former default module `"core"`, 24 of the
//! 120 texts differed.

use akantu_core::array_print::ArrayPrint;
use akantu_core::circular_array::CircularArray;
use akantu_core::debug::{DebugLevel, Debugger, DebuggerScope};
use akantu_core::{Array, Real};

const ORACLE: &str = include_str!("data/oracle/v104_printself.txt");

fn mask(t: &str) -> String {
    t.split_inclusive('\n')
        .map(|l| match l.split_once(" + address        : ") {
            Some((sp, _)) => format!("{sp} + address        : ADDR\n"),
            None => l.to_owned(),
        })
        .collect()
}

#[test]
fn printself_from_user_code_matches_the_oracle() {
    let mut real =
        Array::from_vec(vec![1. / 3., -0., 2.5e-7, 1e20, 123_456_789., 1e-5], 2).unwrap();
    real.set_id("real");
    let mut ints = Array::from_vec(vec![3_i32, -4], 1).unwrap();
    ints.set_id("ints");
    let mut ring = CircularArray::new(3, 2, 0.0 as Real, "ring").unwrap();
    for i in 0..3 {
        for j in 0..2 {
            *ring.get_mut(i, j).unwrap() = 10. * i as Real + j as Real;
        }
    }
    ring.make_step();
    let mut iring = CircularArray::new(2, 1, 7_i32, "").unwrap();
    *iring.get_mut(1, 0).unwrap() = -1;

    let mut n = 0;
    let mut diffs = Vec::new();
    let mut lines = ORACLE.lines();
    while let Some(l) = lines.next() {
        let Some(head) = l.strip_prefix("BEGIN ") else {
            continue;
        };
        let mut body = String::new();
        for b in lines.by_ref() {
            if b == "END" {
                break;
            }
            body.push_str(b);
            body.push('\n');
        }
        let f: Vec<&str> = head.split(' ').collect();
        let name = f[0];
        let level: i32 = f[1].trim_start_matches("level=").parse().unwrap();
        let filter = f[2].trim_start_matches("filter=");
        let indent: usize = f[3].trim_start_matches("indent=").parse().unwrap();
        let modules: Vec<&str> = match filter {
            "none" => vec![],
            other => other.split('+').collect(),
        };
        let mut d = Debugger::new();
        d.set_debug_level(DebugLevel(level));
        for m in &modules {
            d.add_module_to_debug(m);
        }
        let _scope = DebuggerScope::new(d);
        // Array: upstream's std::vector allocation is an input of the
        // printer (read from the text, as core_oracle.rs); CircularArray:
        // the port's own default (allocated size = number of tuples)
        let allocated = body
            .lines()
            .find_map(|l| l.trim().strip_prefix("+ allocated size : "))
            .map(|v| v.parse::<usize>().unwrap());
        let o = ArrayPrint {
            indent,
            allocated_size: if name == "real" || name == "ints" {
                allocated
            } else {
                None
            },
            ..ArrayPrint::default()
        };
        let mine = mask(&match name {
            "real" => real.print_self(&o),
            "ints" => ints.print_self(&o),
            "ring" => ring.print_self(&o),
            "iring" => iring.print_self(&o),
            other => panic!("{other}"),
        });
        n += 1;
        if mine != body {
            diffs.push(format!("--- {head}\n{mine}--- oracle\n{body}"));
        }
    }
    assert_eq!(n, 120, "oracle texts");
    assert!(
        diffs.is_empty(),
        "{} of 120 texts differ:\n{}",
        diffs.len(),
        diffs[..diffs.len().min(3)].join("\n")
    );
}
