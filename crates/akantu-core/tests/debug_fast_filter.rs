// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): src/common/aka_error.hh
// (AKANTU_DEBUG_* macros: printed iff debugger.testLevel(level, module)).

//! The macros' lock-free level pre-filter
//! (`akantu_core::debug::may_print`) follows the **process-default**
//! debugger (its own binary: this test changes the process default, which
//! the library's unit tests never do), through `set_debug_level`,
//! `configure_initialize` (upstream `initialize`'s level sequence,
//! `AKA_DEBUG_LEVEL` included) and a thread scope that shadows it.

use akantu_core::debug::{
    self, DebugLevel, Debugger, DebuggerScope, configure_initialize, may_print, set_debug_level,
};

fn agree(what: &str) {
    let current = debug::debug_level();
    for l in [-7, 0, 1, 4, 5, 6, 8, 42, 100, 1337, 9999] {
        assert_eq!(
            may_print(DebugLevel(l)),
            current >= DebugLevel(l),
            "{what}: level {current}, message {l}"
        );
    }
}

#[test]
fn fast_filter_follows_the_process_default() {
    agree("initial (dblWarning)");
    set_debug_level(DebugLevel::ERROR);
    agree("dblError");
    set_debug_level(DebugLevel(1337));
    agree("dblTest");
    configure_initialize(0, 1, 5, None, false);
    agree("initialize, --aka_debug_level 5");
    configure_initialize(1, 2, 4, Some("  100abc"), true);
    agree("initialize, AKA_DEBUG_LEVEL='  100abc'");
    assert_eq!(debug::debug_level(), DebugLevel::DUMP);
    {
        let mut d = Debugger::new();
        d.set_debug_level(DebugLevel::ERROR);
        let _scope = DebuggerScope::new(d);
        agree("a thread scope shadows the process default");
        assert!(!may_print(DebugLevel::INFO));
    }
    agree("back to the process default");
    assert!(may_print(DebugLevel::INFO));
    // another thread without a scope sees the process default
    let other = std::thread::spawn(|| may_print(DebugLevel::DUMP) && !may_print(DebugLevel(101)))
        .join()
        .unwrap();
    assert!(other, "other threads read the process-default mirror");
    set_debug_level(DebugLevel::WARNING);
    agree("restored");
}
