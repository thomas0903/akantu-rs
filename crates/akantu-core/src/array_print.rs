// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_array_tmpl.hh
// (Array<T>::printself, ArrayPrintHelper<is_scal>::print_content),
// src/common/aka_circular_array_inline_impl.hh (CircularArray<T>::printself),
// src/common/aka_common_inline_impl.hh (printMemorySize<T>).

//! Upstream `Array<T>::printself` (CORE-013): the text
//! that `std::cout << array` produces and that upstream golden files compare
//! byte for byte (`test/test_solver/test_sparse_matrix_product.verified`,
//! GOLD-001 / TEST-157).
//!
//! ```text
//! Array<double> [
//!  + id             : vector
//!  + size           : 121
//!  + nb_component   : 2
//!  + allocated size : 121
//!  + memory size    : 15.12KiByte
//!  + address        : 0x55d2…            (only below dblTest)
//!  + values         : {{2, 2}, {2, 2}}   (only at dblDump or dblTest,
//! ]                                       only for scalar/enum types)
//! ```
//!
//! * The memory size is `printMemorySize<T>(getMemorySize())`, i.e. the
//!   byte count `allocated_size · nb_component · sizeof(T)` multiplied by
//!   `sizeof(T)` once more (UD-457, kept: it is what the golden files show).
//! * `allocated size` is upstream's allocation (`std::vector` capacity in
//!   tuples), which depends on upstream's growth history; the port's
//!   [`Array::capacity`] follows Rust's, so callers comparing with an
//!   upstream text pass upstream's value ([`ArrayPrint::allocated_size`]).
//! * The address is the port's (category C: addresses never match).
//! * Values are printed with the stream's own precision (default 6, `%g`)
//!   and flags: upstream restores them before `print_content`.
//! * The address and values lines follow the debug level **and the module
//!   filter** as in the oracle build ([`ArrayPrint::module`], by default the
//!   user translation unit's [`USER_MODULE`]).

use std::fmt::Write as _;

use crate::array::Array;
use crate::circular_array::CircularArray;
use crate::debug::{self, DebugLevel};
use crate::element::Element;
use crate::random::fmt_c::format_g;
use crate::strings::print_memory_size_bytes;

/// Element types `Array::printself` knows: upstream's demangled C++ type
/// name and whether `print_content` prints values (scalars and enums).
pub trait PrintValue: Sized {
    /// `debug::demangle(typeid(T).name())` on the oracle platform
    /// (x86-64 / aarch64 Linux, LP64).
    const CPP_TYPE_NAME: &'static str;
    /// `sizeof(T)` of the upstream type (the memory size line uses it
    /// twice, UD-457); the Rust size by default.
    const CPP_SIZEOF: usize = std::mem::size_of::<Self>();
    /// `is_scal or std::is_enum<T>`: whether `print_content` prints the
    /// values (`false`: `ArrayPrintHelper<false>`, no values line).
    const PRINTS_VALUES: bool = true;
    /// The value as `std::ostream << value` prints it with `precision`
    /// (used by floating-point types only).
    fn print_value(&self, precision: usize) -> String;
}

macro_rules! print_integer {
    ($($t:ty => $name:literal),* $(,)?) => {$(
        impl PrintValue for $t {
            const CPP_TYPE_NAME: &'static str = $name;
            fn print_value(&self, _precision: usize) -> String {
                self.to_string()
            }
        }
    )*};
}

print_integer!(
    i16 => "short",
    u16 => "unsigned short",
    i32 => "int",
    u32 => "unsigned int",
    i64 => "long",
    u64 => "unsigned long",
    isize => "long",
    usize => "unsigned long",
);

impl PrintValue for f64 {
    const CPP_TYPE_NAME: &'static str = "double";
    fn print_value(&self, precision: usize) -> String {
        format_g(*self, precision)
    }
}

impl PrintValue for f32 {
    const CPP_TYPE_NAME: &'static str = "float";
    fn print_value(&self, precision: usize) -> String {
        // `ostream << float` formats the value promoted to double
        format_g(f64::from(*self), precision)
    }
}

impl PrintValue for bool {
    const CPP_TYPE_NAME: &'static str = "bool";
    fn print_value(&self, _precision: usize) -> String {
        // no `boolalpha`: 1 / 0
        if *self { "1" } else { "0" }.to_owned()
    }
}

impl PrintValue for Element {
    const CPP_TYPE_NAME: &'static str = "akantu::Element";
    /// `{ElementType type; Idx element; GhostType ghost_type;}`: three
    /// 4-byte members (the port's `Element` is smaller).
    const CPP_SIZEOF: usize = 12;
    const PRINTS_VALUES: bool = false;
    fn print_value(&self, _precision: usize) -> String {
        String::new()
    }
}

/// Options of [`Array::print_self`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArrayPrint {
    /// Indentation (upstream `indent`, spaces).
    pub indent: usize,
    /// Debug level deciding the address and values lines; `None`: the
    /// level in effect ([`debug::debug_level`], upstream's global
    /// debugger).
    pub debug_level: Option<DebugLevel>,
    /// Upstream's allocated size in tuples; `None`: the port's
    /// [`Array::capacity`] (or the size for zero-component arrays).
    pub allocated_size: Option<usize>,
    /// Precision of the caller's stream for the values (default 6).
    pub precision: usize,
    /// Debug module of the printing code: upstream compiles `printself`
    /// (a header template) with the `AKANTU_MODULE` of the including file,
    /// and `AKANTU_DEBUG_TEST(dblDump)` tests that module against the
    /// module filter ([`debug::add_module_to_debug`]). Default
    /// [`USER_MODULE`] (`"AKANTU_MODULE"`): `std::cout << array` in a user
    /// program, where `AKANTU_MODULE` is undefined and stringizes to its own
    /// name (120 / 120 oracle texts). Library callers
    /// pass the module their upstream translation unit is compiled with
    /// (the package name, e.g. `"core"`, `"solid_mechanics"`).
    pub module: &'static str,
}

impl Default for ArrayPrint {
    fn default() -> Self {
        Self {
            indent: 0,
            debug_level: None,
            allocated_size: None,
            precision: 6,
            module: USER_MODULE,
        }
    }
}

/// The debug module of a user translation unit: `AKANTU_DEBUG_MODULE` is
/// `AKANTU_STRINGIZE(AKANTU_MODULE)` (`aka_error.hh:274`), and outside
/// Akantu's own sources (compiled with `AKANTU_MODULE=<package>`,
/// `src/CMakeLists.txt:234`) the macro is undefined, so the string is its
/// own name.
pub const USER_MODULE: &str = "AKANTU_MODULE";

/// Upstream `testLevel`'s module condition for a level above `dblCritical`:
/// no module filter, or `module` in it.
fn module_is_debugged(module: &str) -> bool {
    debug::with_current(|d| {
        let mut filter = d.modules_to_debug().peekable();
        filter.peek().is_none() || d.modules_to_debug().any(|m| m == module)
    })
}

impl<T: PrintValue> Array<T> {
    /// Upstream `printself(stream, indent)` (see the module documentation;
    /// `std::cout << array` is `print_self(&ArrayPrint::default())`).
    #[must_use]
    pub fn print_self(&self, opts: &ArrayPrint) -> String {
        let nbc = self.nb_component();
        let allocated = opts.allocated_size.unwrap_or_else(|| {
            if nbc == 0 {
                self.len()
            } else {
                self.capacity()
            }
        });
        let mut s = String::new();
        print_array(
            &mut s,
            self.id(),
            self.as_slice(),
            self.len(),
            nbc,
            allocated,
            opts,
        );
        s
    }
}

impl<T: PrintValue> CircularArray<T> {
    /// Upstream `CircularArray<T>::printself(stream, indent)`
    /// (`aka_circular_array_inline_impl.hh:76-88`): the two positions, then
    /// the underlying `Array<T>::printself(stream, indent + 1)` of the
    /// physical storage (storage order, not the logical order). The
    /// allocated size defaults to the number of tuples (upstream's
    /// `Array(size, nb_component, value, id)` allocates exactly `size`
    /// tuples).
    #[must_use]
    pub fn print_self(&self, opts: &ArrayPrint) -> String {
        let space = " ".repeat(opts.indent);
        let mut s = String::new();
        let _ = writeln!(s, "{space}CircularArray<{}> [", T::CPP_TYPE_NAME);
        let _ = writeln!(s, "{space} + start_position : {}", self.start_position());
        let _ = writeln!(s, "{space} + end_position   : {}", self.end_position());
        let inner = ArrayPrint {
            indent: opts.indent + 1,
            ..*opts
        };
        let allocated = opts.allocated_size.unwrap_or(self.len());
        print_array(
            &mut s,
            self.id(),
            self.as_slice(),
            self.len(),
            self.nb_component(),
            allocated,
            &inner,
        );
        let _ = writeln!(s, "{space}]");
        s
    }
}

/// Body of upstream `Array<T>::printself` for `len` tuples of `nbc`
/// components stored in `values`, with upstream's `allocated` size.
fn print_array<T: PrintValue>(
    s: &mut String,
    id: &str,
    values: &[T],
    len: usize,
    nbc: usize,
    allocated: usize,
    opts: &ArrayPrint,
) {
    let space = " ".repeat(opts.indent);
    let level = opts.debug_level.unwrap_or_else(debug::debug_level);
    // The oracle build (RelWithDebInfo, AKANTU_NDEBUG undefined,
    // aka_error.hh:342-346): AKANTU_DEBUG_LEVEL_IS_TEST() is
    // testLevel(dblTest) with testLevel's default module "core", and
    // AKANTU_DEBUG_TEST(dblDump) is testLevel(dblDump, AKANTU_DEBUG_MODULE)
    // with the module of the including file (`opts.module`); both levels
    // are above dblCritical, so the module filter applies.
    let is_test = level >= DebugLevel::TEST && module_is_debugged("core");
    let dump = level >= DebugLevel::DUMP && module_is_debugged(opts.module);
    // getMemorySize() = allocated · nb_component · sizeof(T), then
    // printMemorySize<T> multiplies by sizeof(T) again (UD-457)
    let memory = allocated.saturating_mul(nbc).saturating_mul(T::CPP_SIZEOF);
    let _ = writeln!(s, "{space}Array<{}> [", T::CPP_TYPE_NAME);
    let _ = writeln!(s, "{space} + id             : {id}");
    let _ = writeln!(s, "{space} + size           : {len}");
    let _ = writeln!(s, "{space} + nb_component   : {nbc}");
    let _ = writeln!(s, "{space} + allocated size : {allocated}");
    let _ = writeln!(
        s,
        "{space} + memory size    : {}",
        print_memory_size_bytes(memory.saturating_mul(T::CPP_SIZEOF))
    );
    if !is_test {
        let _ = writeln!(s, "{space} + address        : {:p}", values.as_ptr());
    }
    if (dump || is_test) && T::PRINTS_VALUES {
        let tuples: Vec<String> = (0..len)
            .map(|i| {
                let t = &values[i * nbc..(i + 1) * nbc];
                let v: Vec<String> = t.iter().map(|x| x.print_value(opts.precision)).collect();
                format!("{{{}}}", v.join(", "))
            })
            .collect();
        let _ = writeln!(s, "{space} + values         : {{{}}}", tuples.join(", "));
    }
    let _ = writeln!(s, "{space}]");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element_type::{ElementType, GhostType};

    fn test_level() -> ArrayPrint {
        ArrayPrint {
            debug_level: Some(DebugLevel::TEST),
            ..ArrayPrint::default()
        }
    }

    #[test]
    fn layout_at_the_test_level() {
        let mut a = Array::from_vec(vec![1.0 / 3.0, -0.0, 2.5e-7, 1e20], 2).unwrap();
        a.set_id("a");
        assert_eq!(
            a.print_self(&ArrayPrint {
                allocated_size: Some(2),
                ..test_level()
            }),
            "Array<double> [\n + id             : a\n + size           : 2\n \
             + nb_component   : 2\n + allocated size : 2\n + memory size    : 256.00Byte\n \
             + values         : {{0.333333, -0}, {2.5e-07, 1e+20}}\n]\n"
        );
        let mut i = Array::from_vec(vec![3_i32, -4, 5], 1).unwrap();
        i.set_id("i");
        let t = i.print_self(&ArrayPrint {
            indent: 2,
            allocated_size: Some(3),
            ..test_level()
        });
        assert!(
            t.starts_with("  Array<int> [\n   + id             : i\n"),
            "{t}"
        );
        assert!(
            t.ends_with("   + values         : {{3}, {-4}, {5}}\n  ]\n"),
            "{t}"
        );
        // empty array: values line with an empty list
        let e: Array<f64> = Array::new(3);
        assert!(
            e.print_self(&test_level())
                .contains(" + values         : {}\n")
        );
    }

    #[test]
    fn debug_level_decides_address_and_values() {
        let a = Array::from_vec(vec![1.0, 2.0], 1).unwrap();
        let warn = a.print_self(&ArrayPrint {
            debug_level: Some(DebugLevel::WARNING),
            ..ArrayPrint::default()
        });
        assert!(warn.contains(" + address        : 0x") && !warn.contains("values"));
        let dump = a.print_self(&ArrayPrint {
            debug_level: Some(DebugLevel::DUMP),
            ..ArrayPrint::default()
        });
        assert!(
            dump.contains(" + address        : ")
                && dump.contains(" + values         : {{1}, {2}}")
        );
        // the level in effect (thread-scoped)
        let _ctx = debug::DebugLevelContext::new(DebugLevel::TEST);
        let t = a.print_self(&ArrayPrint::default());
        assert!(!t.contains("address") && t.contains("values"));
    }

    /// The module filter decides like
    /// upstream's `testLevel` (`aka_error.hh:213-220`): `IS_TEST` asks for
    /// module "core", `DEBUG_TEST(dblDump)` for the printing code's module.
    #[test]
    fn module_filter_applies_like_upstream_test_level() {
        let a = Array::from_vec(vec![1.0, 2.0], 1).unwrap();
        let at = |level: DebugLevel, modules: &[&str], module: &'static str| {
            let mut d = debug::Debugger::new();
            d.set_debug_level(level);
            for m in modules {
                d.add_module_to_debug(m);
            }
            let _scope = debug::DebuggerScope::new(d);
            a.print_self(&ArrayPrint {
                module,
                ..ArrayPrint::default()
            })
        };
        let has = |t: &str, addr: bool, values: bool| {
            t.contains(" + address ") == addr && t.contains(" + values ") == values
        };
        // no filter: the level alone decides
        assert!(has(&at(DebugLevel::TEST, &[], "core"), false, true));
        assert!(has(&at(DebugLevel::DUMP, &[], "solid"), true, true));
        // "core" filtered in: dblTest is reached for IS_TEST
        assert!(has(&at(DebugLevel::TEST, &["core"], "core"), false, true));
        // a filter without "core" nor the printing module: upstream prints
        // the address and no values even at dblTest
        let t = at(DebugLevel::TEST, &["mesh"], "solid");
        assert!(has(&t, true, false), "{t}");
        // the printing module filtered in but not "core": values through
        // DEBUG_TEST(dblDump), address because IS_TEST is false
        assert!(has(&at(DebugLevel::TEST, &["solid"], "solid"), true, true));
        assert!(has(&at(DebugLevel::DUMP, &["solid"], "solid"), true, true));
        assert!(has(&at(DebugLevel::DUMP, &["mesh"], "solid"), true, false));
        // "core" filtered in, printing module not: no address, values via IS_TEST
        assert!(has(&at(DebugLevel::TEST, &["core"], "solid"), false, true));
        assert!(has(&at(DebugLevel::DUMP, &["core"], "solid"), true, false));
    }

    /// Upstream `CircularArray<T>::printself` prints the two
    /// positions and the underlying array (storage order) one level deeper.
    #[test]
    fn circular_array_printself_layout() {
        let mut c = CircularArray::new(3, 2, 0.0, "ring").unwrap();
        for i in 0..3 {
            for j in 0..2 {
                *c.get_mut(i, j).unwrap() = f64::from(u8::try_from(10 * i + j).unwrap());
            }
        }
        c.make_step();
        assert_eq!(
            c.print_self(&test_level()),
            "CircularArray<double> [\n + start_position : 1\n + end_position   : 0\n \
             Array<double> [\n  + id             : ring\n  + size           : 3\n  \
             + nb_component   : 2\n  + allocated size : 3\n  + memory size    : 384.00Byte\n  \
             + values         : {{0, 1}, {10, 11}, {20, 21}}\n ]\n]\n"
        );
        let mut i = CircularArray::new(2, 1, 7_i32, "").unwrap();
        *i.get_mut(1, 0).unwrap() = -1;
        let t = i.print_self(&ArrayPrint {
            indent: 2,
            debug_level: Some(DebugLevel::WARNING),
            ..ArrayPrint::default()
        });
        assert!(
            t.starts_with(
                "  CircularArray<int> [\n   + start_position : 0\n   + end_position   : 1\n   Array<int> [\n"
            ),
            "{t}"
        );
        assert!(
            t.contains("    + address        : 0x") && !t.contains("values"),
            "{t}"
        );
        assert!(t.ends_with("   ]\n  ]\n"), "{t}");
    }

    #[test]
    fn class_types_print_no_values() {
        let el = Element::new(ElementType::Triangle3, 0, GhostType::NotGhost);
        let mut a: Array<Element> = Array::new(1);
        a.push_value(el);
        let t = a.print_self(&ArrayPrint {
            allocated_size: Some(1),
            ..test_level()
        });
        assert!(t.starts_with("Array<akantu::Element> [\n"));
        assert!(!t.contains("values"), "{t}");
    }
}
