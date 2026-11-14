// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential test: compares the Rust parser with dumps
// of the C++ reference parser (oracle/probes/parser/dump_parser.cc, run by
// oracle/probes/parser/run.sh against /build/install-strict of the
// akantu-oracle:dev image; provenance in tests/fixtures/parser/PROVENANCE.txt).

//! For every input file (`tests/fixtures/{test,examples,extra_packages}` =
//! upstream files, `tests/fixtures/extra/expressions.dat` = generated corpus
//! of literals, expressions, vectors, matrices, random parameters and stream
//! conversions, `tests/fixtures/extra/{depth_*,cycles,bytes}.dat` = deep
//! nesting, cyclic references and bytes ≥ 0x80) the oracle JSON `tests/fixtures/parser/<path>.json` holds the
//! tree (section types, names, options, parameters in upstream order with raw
//! values and positions, sub-sections in upstream order) and, for every
//! parameter, the outcome of every upstream conversion
//! (`Real`, `Vector<Real>`, `Matrix<Real>`, `RandomParameter<Real>`, `bool`,
//! `Int`, `UInt`, `std::string`, `std::vector<std::string>`).
//!
//! Comparison is **exact**: doubles by bit pattern, strings byte-wise, error
//! classes (`akantu` = `debug::Exception` ↔ soft [`ConversionError`],
//! `std` = Spirit X3 `expectation_failure` ↔ hard, `crash` = stack overflow
//! on cyclic references ↔ hard cyclic-reference error).
//!
//! Platform note: results of transcendental functions (`sin`, `exp`, `pow`,
//! `erf`, …) come from the platform libm, exactly as in C++. The oracle uses
//! glibc 2.41 (aarch64): on Linux every double is compared bitwise. On macOS
//! (Apple libm) the measured last-bit differences are enumerated one by one
//! in [`LIBM_EXCEPTIONS`] (file, conversion, component, both bit patterns,
//! responsible libm function); any other difference fails, and so does an
//! entry that is no longer needed. NaNs are compared by bit pattern, except
//! that the default quiet NaNs `0x7ff8000000000000` (aarch64) and
//! `0xfff8000000000000` (x86_64 SSE) match each other (decision D-046,
//! `akantu_validation::exact`; `extra/expressions.dat` `s0322`).
//! Conversions on which upstream overflows its stack for acyclic input are
//! listed in [`UPSTREAM_STACK_EXHAUSTION`].

mod common;

use akantu_core::tensor::MatrixX;
use akantu_parser::{
    ByteString, ConversionError, ParamRef, ParserSection, RandomParameter, SearchScope,
};
use serde_json::Value;

/// A measured difference between the host libm and the oracle's glibc 2.41
/// for one double of one conversion: the comparison is
/// bitwise everywhere except for exactly these components, each attributed
/// to the libm function whose last bit differs (see
/// `docs/NUMERICAL_VALIDATION.md`).
struct LibmException {
    file: &'static str,
    /// `<section path>/<parameter>.<conversion>`.
    path: &'static str,
    /// Index of the double in the conversion result (scalar: 0; vector:
    /// entry; matrix: row-major; random parameter: 0 = base value).
    component: usize,
    oracle: u64,
    port: u64,
    function: &'static str,
}

/// Exceptions measured on aarch64-apple-darwin (macOS 27.2, Apple libm)
/// against the glibc 2.41 aarch64 oracle. Every entry is a single call of
/// one libm function whose argument is the same double in both
/// implementations (`acos(.5)`, `asin(.5)`, `atan(.5)`, `atanh(.5)`,
/// `erf(3.)`, `lgamma(3.)` on exact literals; `erf(0.3)`, `erfc(0.3)` on
/// `0x3fd3333333333333`, the identically rounded literal `0.3`), the
/// difference is one unit in the last place, and the scalar (X3), vector
/// (Qi) and random-parameter base (Qi) conversions of the same expression
/// are listed separately.
#[cfg(target_os = "macos")]
const LIBM_EXCEPTIONS: &[LibmException] = &[
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0280.real",
        component: 0,
        oracle: 0x3ff0_c152_382d_7366,
        port: 0x3ff0_c152_382d_7365,
        function: "acos",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0280.random",
        component: 0,
        oracle: 0x3ff0_c152_382d_7366,
        port: 0x3ff0_c152_382d_7365,
        function: "acos",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/v0280.vector",
        component: 0,
        oracle: 0x3ff0_c152_382d_7366,
        port: 0x3ff0_c152_382d_7365,
        function: "acos",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0281.real",
        component: 0,
        oracle: 0x3fe0_c152_382d_7366,
        port: 0x3fe0_c152_382d_7365,
        function: "asin",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0281.random",
        component: 0,
        oracle: 0x3fe0_c152_382d_7366,
        port: 0x3fe0_c152_382d_7365,
        function: "asin",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/v0281.vector",
        component: 0,
        oracle: 0x3fe0_c152_382d_7366,
        port: 0x3fe0_c152_382d_7365,
        function: "asin",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0282.real",
        component: 0,
        oracle: 0x3fdd_ac67_0561_bb4f,
        port: 0x3fdd_ac67_0561_bb50,
        function: "atan",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0282.random",
        component: 0,
        oracle: 0x3fdd_ac67_0561_bb4f,
        port: 0x3fdd_ac67_0561_bb50,
        function: "atan",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/v0282.vector",
        component: 0,
        oracle: 0x3fdd_ac67_0561_bb4f,
        port: 0x3fdd_ac67_0561_bb50,
        function: "atan",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0297.real",
        component: 0,
        oracle: 0x3fe1_93ea_7aad_030a,
        port: 0x3fe1_93ea_7aad_030b,
        function: "atanh",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0297.random",
        component: 0,
        oracle: 0x3fe1_93ea_7aad_030a,
        port: 0x3fe1_93ea_7aad_030b,
        function: "atanh",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/v0297.vector",
        component: 0,
        oracle: 0x3fe1_93ea_7aad_030a,
        port: 0x3fe1_93ea_7aad_030b,
        function: "atanh",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0302.real",
        component: 0,
        oracle: 0x3fef_ffd1_ac41_35f9,
        port: 0x3fef_ffd1_ac41_35fa,
        function: "erf",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0304.real",
        component: 0,
        oracle: 0x3fe6_2e42_fefa_39ef,
        port: 0x3fe6_2e42_fefa_39f0,
        function: "lgamma_r",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0304.random",
        component: 0,
        oracle: 0x3fe6_2e42_fefa_39ef,
        port: 0x3fe6_2e42_fefa_39f0,
        function: "lgamma_r",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/v0304.vector",
        component: 0,
        oracle: 0x3fe6_2e42_fefa_39ef,
        port: 0x3fe6_2e42_fefa_39f0,
        function: "lgamma_r",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0312.real",
        component: 0,
        oracle: 0x3fd5_0838_881d_ea0f,
        port: 0x3fd5_0838_881d_ea0e,
        function: "erf",
    },
    LibmException {
        file: "extra/expressions.dat",
        path: "global/s0313.real",
        component: 0,
        oracle: 0x3fe5_7be3_bbf1_0af8,
        port: 0x3fe5_7be3_bbf1_0af9,
        function: "erfc",
    },
];

/// Linux (glibc, the oracle's libm): no exception, bitwise everywhere.
/// Other platforms have no measured exception list: the comparison is
/// bitwise and fails where their libm differs.
#[cfg(not(target_os = "macos"))]
const LIBM_EXCEPTIONS: &[LibmException] = &[];

struct Ctx<'a> {
    file: &'a str,
    mismatches: Vec<String>,
    checked: usize,
    /// Index into `LIBM_EXCEPTIONS` of every exception that was needed.
    used: std::cell::RefCell<Vec<usize>>,
}

impl Ctx<'_> {
    fn fail(&mut self, path: &str, what: String) {
        self.mismatches
            .push(format!("{}: {path}: {what}", self.file));
    }
}

/// Bytes of a probe JSON string (`json_string` in dump_parser.cc writes
/// every byte >= 0x7f as `\u00XX`, so each character is one byte).
fn json_bytes(v: &Value) -> Option<Vec<u8>> {
    v.as_str()?
        .chars()
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect()
}

fn bits_of(v: &Value) -> u64 {
    let s = v.as_str().expect("bit string");
    u64::from_str_radix(s.trim_start_matches("0x"), 16).expect("hex bits")
}

/// Exact comparison of decision D-046 (bitwise, NaNs included: payloads
/// must match, and only the default quiet NaN may differ in sign), except
/// for an enumerated platform-libm exception with exactly these bits.
fn same_double(ctx: &Ctx<'_>, path: &str, component: usize, oracle: u64, ours: f64) -> bool {
    let port = ours.to_bits();
    if akantu_validation::exact::exact_bits_eq(oracle, port) {
        return true;
    }
    let hit = LIBM_EXCEPTIONS.iter().position(|e| {
        e.file == ctx.file
            && e.path == path
            && e.component == component
            && e.oracle == oracle
            && e.port == port
    });
    match hit {
        Some(i) => {
            ctx.used.borrow_mut().push(i);
            true
        }
        None => {
            eprintln!(
                "    LibmException {{ file: {:?}, path: {path:?}, component: {component}, oracle: {oracle:#018x}, port: {port:#018x}, function: \"?\" }},",
                ctx.file
            );
            false
        }
    }
}

/// `printself` of the port's random parameter for comparison with an oracle
/// record whose base has the bits `oracle_base`: when the two bases match
/// only through D-046 (default quiet NaNs differing in sign; x86_64 prints
/// `-nan` where the aarch64 golden prints `nan`), the port's parameter is
/// printed with the oracle's base, so the printed form inherits the same
/// sign-of-default-NaN convention as the value; otherwise as is.
fn random_print_d046(oracle_base: u64, ours: &RandomParameter) -> String {
    let port = ours.base_value.to_bits();
    if port != oracle_base && akantu_validation::exact::exact_bits_eq(oracle_base, port) {
        RandomParameter {
            base_value: f64::from_bits(oracle_base),
            ..ours.clone()
        }
        .to_string()
    } else {
        ours.to_string()
    }
}

/// Conversions on which upstream overflows its stack although the input is
/// acyclic (Qi's per-level stack use is larger than X3's): category A, the
/// port evaluates them (values checked in tests/depth.rs).
const UPSTREAM_STACK_EXHAUSTION: &[(&str, &str)] =
    &[("extra/depth_paren_5000.dat", "global/deep.random")];

fn check_error(ctx: &mut Ctx<'_>, path: &str, class: &str, err: &ConversionError) {
    let ok = match class {
        "akantu" => !err.is_hard(),
        "std" => err.is_hard(),
        "crash" => err.is_hard() && err.is_cyclic_reference(),
        _ => false,
    };
    if !ok {
        ctx.fail(
            path,
            format!("oracle error class {class}, port error {err:?}"),
        );
    }
}

fn compare_conv<T>(
    ctx: &mut Ctx<'_>,
    path: &str,
    oracle: &Value,
    ours: Result<T, ConversionError>,
    eq: impl FnOnce(&mut Ctx<'_>, &Value, &T) -> bool,
    show: impl FnOnce(&T) -> String,
) {
    ctx.checked += 1;
    match (oracle.get("ok"), oracle.get("error"), ours) {
        (Some(o), _, Ok(v)) => {
            if !eq(ctx, o, &v) {
                let s = show(&v);
                ctx.fail(path, format!("oracle {o}, port {s}"));
            }
        }
        (Some(o), _, Err(e)) => ctx.fail(path, format!("oracle {o}, port error {e}")),
        (None, Some(class), Ok(_))
            if class == "crash" && UPSTREAM_STACK_EXHAUSTION.contains(&(ctx.file, path)) => {}
        (None, Some(class), Ok(v)) => {
            let s = show(&v);
            ctx.fail(path, format!("oracle error {class}, port {s}"));
        }
        (None, Some(class), Err(e)) => check_error(ctx, path, class.as_str().unwrap_or(""), &e),
        _ => ctx.fail(path, format!("malformed oracle entry {oracle}")),
    }
}

fn matrix_rows(m: &MatrixX) -> Vec<Vec<f64>> {
    (0..m.rows())
        .map(|i| (0..m.cols()).map(|j| m.at(i, j)).collect())
        .collect()
}

fn compare_parameter(ctx: &mut Ctx<'_>, path: &str, o: &Value, p: &ParamRef<'_>) {
    let q = p.parameter();
    let value = q.value.to_string_lossy().into_owned();
    if json_bytes(&o["name"]).as_deref() != Some(q.name.as_bytes())
        || json_bytes(&o["value"]).as_deref() != Some(q.value.as_bytes())
    {
        ctx.fail(
            path,
            format!(
                "name/value oracle {} = {:?}, port {} = {value:?}",
                o["name"], o["value"], q.name
            ),
        );
        return;
    }
    if o["line"] != q.line || o["column"] != q.column {
        ctx.fail(
            path,
            format!(
                "position oracle {}:{}, port {}:{}",
                o["line"], o["column"], q.line, q.column
            ),
        );
    }
    let _ = value;
    let real_path = format!("{path}.real");
    compare_conv(
        ctx,
        &real_path,
        &o["real"],
        p.try_get::<f64>(),
        |c, o, x| same_double(c, &real_path, 0, bits_of(o), *x),
        |x| format!("{x:e} ({:#018x})", x.to_bits()),
    );
    let vector_path = format!("{path}.vector");
    compare_conv(
        ctx,
        &vector_path,
        &o["vector"],
        p.try_get::<Vec<f64>>(),
        |c, o, x| {
            let a = o.as_array().unwrap();
            a.len() == x.len()
                && a.iter()
                    .zip(x)
                    .enumerate()
                    .all(|(i, (b, y))| same_double(c, &vector_path, i, bits_of(b), *y))
        },
        |x| format!("{x:?}"),
    );
    let matrix_path = format!("{path}.matrix");
    compare_conv(
        ctx,
        &matrix_path,
        &o["matrix"],
        p.try_get::<MatrixX>(),
        |c, o, m| {
            let rows = matrix_rows(m);
            let a = o.as_array().unwrap();
            let oracle: Vec<u64> = a
                .iter()
                .flat_map(|ra| ra.as_array().unwrap().iter().map(bits_of))
                .collect();
            let ours: Vec<f64> = rows.iter().flatten().copied().collect();
            a.len() == rows.len()
                && a.iter()
                    .zip(&rows)
                    .all(|(ra, r)| ra.as_array().unwrap().len() == r.len())
                && oracle
                    .iter()
                    .zip(&ours)
                    .enumerate()
                    .all(|(i, (b, y))| same_double(c, &matrix_path, i, *b, *y))
        },
        |m| format!("{:?}", matrix_rows(m)),
    );
    let random_path = format!("{path}.random");
    compare_conv(
        ctx,
        &random_path,
        &o["random"],
        p.try_get::<RandomParameter>(),
        |c, o, r| {
            let base = bits_of(&o["base"]);
            same_double(c, &random_path, 0, base, r.base_value)
                && json_bytes(&o["print"]).as_deref() == Some(random_print_d046(base, r).as_bytes())
        },
        |r| format!("{r}"),
    );
    compare_conv(
        ctx,
        &format!("{path}.bool"),
        &o["bool"],
        p.try_get::<bool>(),
        |_, o, b| o == b,
        |b| b.to_string(),
    );
    compare_conv(
        ctx,
        &format!("{path}.int"),
        &o["int"],
        p.try_get::<i32>(),
        |_, o, i| o == i,
        |i| i.to_string(),
    );
    compare_conv(
        ctx,
        &format!("{path}.uint"),
        &o["uint"],
        p.try_get::<u32>(),
        |_, o, i| o == i,
        |i| i.to_string(),
    );
    compare_conv(
        ctx,
        &format!("{path}.word"),
        &o["word"],
        p.try_get::<ByteString>(),
        |_, o, w| json_bytes(o).as_deref() == Some(w.as_bytes()),
        |w| format!("{w:?}"),
    );
    compare_conv(
        ctx,
        &format!("{path}.string_list"),
        &o["string_list"],
        p.try_get::<Vec<ByteString>>(),
        |_, o, l| {
            o.as_array().is_some_and(|a| {
                a.len() == l.len()
                    && a.iter()
                        .zip(l)
                        .all(|(x, y)| json_bytes(x).as_deref() == Some(y.as_bytes()))
            })
        },
        |l| format!("{l:?}"),
    );
}

fn compare_section(ctx: &mut Ctx<'_>, path: &str, o: &Value, s: &ParserSection) {
    if o["type"] != s.section_type.name()
        || o["name"] != s.name.as_str()
        || o["option"] != s.option.as_str()
    {
        ctx.fail(
            path,
            format!(
                "section oracle ({}, {}, {}), port ({}, {}, {})",
                o["type"], o["name"], o["option"], s.section_type, s.name, s.option
            ),
        );
        return;
    }
    let params = o["parameters"].as_array().unwrap();
    let ours = s.parameters_sorted();
    if params.len() != ours.len() {
        ctx.fail(
            path,
            format!(
                "{} parameters in oracle, {} in port",
                params.len(),
                ours.len()
            ),
        );
        return;
    }
    for (op, q) in params.iter().zip(ours) {
        let r = s.get_parameter(&q.name, SearchScope::Current).unwrap();
        compare_parameter(ctx, &format!("{path}/{}", q.name), op, &r);
    }
    let subs = o["sections"].as_array().unwrap();
    let ours = s.sub_sections_sorted();
    if subs.len() != ours.len() {
        ctx.fail(
            path,
            format!(
                "{} sub-sections in oracle, {} in port",
                subs.len(),
                ours.len()
            ),
        );
        return;
    }
    for (i, (os, q)) in subs.iter().zip(ours).enumerate() {
        compare_section(ctx, &format!("{path}/{}[{i}]", q.name), os, q);
    }
}

#[test]
fn parser_matches_cpp_oracle() {
    let fixtures = common::fixtures_dir();
    let mut files = common::upstream_inputs();
    let mut extra: Vec<String> = std::fs::read_dir(fixtures.join("extra"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".dat"))
        .map(|n| format!("extra/{n}"))
        .collect();
    extra.sort();
    assert!(extra.len() >= 8, "extra fixtures missing: {extra:?}");
    files.extend(extra);
    let mut all = Vec::new();
    let mut checked = 0;
    let mut used = Vec::new();
    for rel in &files {
        let json_path = fixtures.join("parser").join(format!("{rel}.json"));
        let text = std::fs::read_to_string(&json_path)
            .unwrap_or_else(|e| panic!("missing oracle dump {}: {e}", json_path.display()));
        // Dumps of deeply nested sections exceed serde_json's default
        // recursion limit (128).
        let mut de = serde_json::Deserializer::from_str(&text);
        de.disable_recursion_limit();
        let oracle: Value = de.into_iter().next().unwrap().unwrap();
        let parsed = common::parse_fixture(rel);
        let mut ctx = Ctx {
            file: rel,
            mismatches: Vec::new(),
            checked: 0,
            used: std::cell::RefCell::new(Vec::new()),
        };
        match (oracle.get("parse_error"), &parsed) {
            (Some(_), Err(_)) => {}
            (Some(_), Ok(_)) => ctx.fail("", "oracle rejects the file, port accepts it".into()),
            (None, Err(e)) => ctx.fail("", format!("port rejects the file: {e}")),
            (None, Ok(p)) => {
                if oracle["permissive"] != p.is_permissive() {
                    ctx.fail("", "permissive flag differs".into());
                }
                compare_section(&mut ctx, "global", &oracle["root"], &p.root);
            }
        }
        checked += ctx.checked;
        used.extend(ctx.used.into_inner());
        all.extend(ctx.mismatches);
    }
    used.sort_unstable();
    used.dedup();
    // The exception list is exact: every entry must still be needed.
    for (i, e) in LIBM_EXCEPTIONS.iter().enumerate() {
        if used.binary_search(&i).is_err() {
            all.push(format!(
                "{}: {}[{}]: stale libm exception ({}): the port now matches the oracle",
                e.file, e.path, e.component, e.function
            ));
        }
    }
    eprintln!(
        "{} files, {checked} conversions compared, {} enumerated platform-libm exceptions used",
        files.len(),
        used.len()
    );
    assert!(
        all.is_empty(),
        "{} mismatches:\n{}",
        all.len(),
        all.join("\n")
    );
}

/// D-046 for the printed random parameter (`s0322.random`): the
/// x86_64 port value `-nan` prints like the aarch64 golden `nan`; a payload
/// NaN or a number keeps its own printing.
#[test]
fn random_print_follows_the_default_nan_convention() {
    let x86 = RandomParameter {
        base_value: -f64::NAN,
        distribution: None,
    };
    assert!(x86.to_string().starts_with("-nan"), "{x86}");
    let golden = f64::NAN.to_bits();
    assert_eq!(
        random_print_d046(golden, &x86),
        "nan + uniform [ 0.00000000000000000e+00 0.00000000000000000e+00 ]"
    );
    let payload = RandomParameter {
        base_value: -f64::from_bits(golden | 1),
        distribution: None,
    };
    assert_eq!(random_print_d046(golden, &payload), payload.to_string());
    let one = RandomParameter {
        base_value: 1.0,
        distribution: None,
    };
    assert_eq!(random_print_d046(1.0f64.to_bits(), &one), one.to_string());
}

/// The exception mechanism accepts only the enumerated bits at the
/// enumerated place: an unrelated 1-ulp change, another value at a listed
/// place, a different NaN or the step from the largest finite value to
/// infinity are mismatches.
#[test]
fn libm_exceptions_are_exact() {
    let ctx = Ctx {
        file: "extra/expressions.dat",
        mismatches: Vec::new(),
        checked: 0,
        used: std::cell::RefCell::new(Vec::new()),
    };
    let one = 1.0f64.to_bits();
    assert!(same_double(&ctx, "global/s0001.real", 0, one, 1.0));
    assert!(!same_double(
        &ctx,
        "global/s0001.real",
        0,
        one,
        f64::from_bits(one + 1)
    ));
    // a NaN with another payload is a different NaN
    assert!(!same_double(
        &ctx,
        "global/s0001.real",
        0,
        f64::NAN.to_bits(),
        f64::from_bits(f64::NAN.to_bits() | 1)
    ));
    // D-046: the x86_64 default NaN (sign bit set) matches the aarch64
    // golden default NaN (`s0322`), and conversely
    assert!(same_double(
        &ctx,
        "global/s0322.real",
        0,
        f64::NAN.to_bits(),
        -f64::NAN
    ));
    assert!(same_double(
        &ctx,
        "global/s0322.real",
        0,
        (-f64::NAN).to_bits(),
        f64::NAN
    ));
    // but a payload NaN with the sign bit does not
    assert!(!same_double(
        &ctx,
        "global/s0322.real",
        0,
        f64::NAN.to_bits(),
        -f64::from_bits(f64::NAN.to_bits() | 1)
    ));
    assert!(!same_double(
        &ctx,
        "global/s0001.real",
        0,
        f64::MAX.to_bits(),
        f64::INFINITY
    ));
    assert!(same_double(
        &ctx,
        "global/s0001.real",
        0,
        f64::NAN.to_bits(),
        f64::NAN
    ));
    if let Some(e) = LIBM_EXCEPTIONS.first() {
        assert!(same_double(
            &ctx,
            e.path,
            e.component,
            e.oracle,
            f64::from_bits(e.port)
        ));
        // Same place, other bits; same bits, other component or file.
        assert!(!same_double(
            &ctx,
            e.path,
            e.component,
            e.oracle,
            f64::from_bits(e.port ^ 2)
        ));
        assert!(!same_double(
            &ctx,
            e.path,
            e.component + 1,
            e.oracle,
            f64::from_bits(e.port)
        ));
        let other = Ctx {
            file: "extra/bytes.dat",
            ..ctx
        };
        assert!(!same_double(
            &other,
            e.path,
            e.component,
            e.oracle,
            f64::from_bits(e.port)
        ));
    }
}
