// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_error.hh, aka_error.cc
// (DebugLevel, debug::Debugger: testLevel, printMessage, setLogFile,
// setParallelContext, printBacktrace flag, modules to debug,
// DebugLevelContext, printBacktrace, terminate handler, AKANTU_DEBUG_* and
// AKANTU_LOCATION macros), src/common/aka_common.cc (initialize: debug
// level, --aka_print_backtrace, AKA_DEBUG_LEVEL).

//! User-visible debug output of upstream's `aka_error` (CORE-033, CORE-044).
//!
//! # Levels
//! [`DebugLevel`] holds upstream's `DebugLevel` values (`dblError` = 0 …
//! `dblWarning` = 4, `dblInfo` = 5, `dblTrace` = 8, `dblDump` = 100,
//! `dblTest` = 1337). A message of level `l` for module `m` is printed iff
//! [`Debugger::test_level`]: `level ≥ l` and (`l ≤ dblCritical`, or no
//! module filter, or `m` is in it) — upstream `testLevel`. The default
//! level is `dblWarning`; upstream `initialize` sets `dblError`, then
//! `--aka_debug_level` (default `dblWarning`), then `AKA_DEBUG_LEVEL` read
//! with glibc `atoi`: [`Debugger::configure_initialize`].
//!
//! # Messages
//! [`Debugger::print_message`] writes, as upstream `printMessage`,
//! `<parallel context>{<µs since the epoch>} <prefix> <info>\n` where the
//! macros make `info` = `<message> (<function>(): <file>:<line>)`
//! (`AKANTU_LOCATION`). Prefixes: [`aka_debug!`](crate::aka_debug) `"   "`,
//! [`aka_debug_info!`](crate::aka_debug_info) `"---"`,
//! [`aka_debug_warning!`](crate::aka_debug_warning) `"/!\"`,
//! [`aka_debug_trace!`](crate::aka_debug_trace) `">>>"`,
//! [`aka_debug_in!`](crate::aka_debug_in) / [`aka_debug_out!`](crate::aka_debug_out)
//! `"==>"` / `"<=="`, [`aka_debug_error!`](crate::aka_debug_error) `"!!! "`
//! (`AKANTU_ERROR`), [`aka_exception_message!`](crate::aka_exception_message)
//! `"###"` at `dblWarning` (`throwException`, non-silent). The parallel
//! context is `<pid>[R<rank>|S<size>] ` (rank right-aligned on
//! `ceil(log10(size))` characters), set by `initialize` on every run
//! (serial runs too: `[R0|S1]`). Output goes to standard error, or to a log
//! file ([`Debugger::set_log_file`], upstream `setLogFile`), or to a
//! [`MessageBuffer`] ([`Debugger::set_out_buffer`], upstream
//! `setOutStream`). The `log` crate's records (used by the readers and the
//! parser for upstream's `AKANTU_DEBUG_WARNING`s) reach the same output once
//! [`install_log_bridge`] is called (the facade's `initialize` does it).
//!
//! # Backtraces
//! The `print_backtrace` flag (upstream `debugger.printBacktrace(bool)`,
//! `--aka_print_backtrace`) controls what upstream controls with it: an
//! error report captures and prints the stack ([`get_backtrace`],
//! [`format_backtrace`], upstream `printBacktrace`, e.g. the Python
//! exception translator), and the uncaught-error report
//! ([`format_uncaught`], upstream's terminate handler) adds the random seed
//! and the current stack. The frame text (addresses, symbol names,
//! locations) is Rust's, not glibc's `backtrace_symbols` + `addr2line`
//! (category C, UD-274).
//!
//! # State and threads
//! Upstream has one process-global `debug::debugger`. Here the process
//! default ([`set_debug_level`], [`set_log_file`], … without a scope) is a
//! `RwLock`-protected global, as the facade's runtime state (D-023), and a
//! thread can install its own debugger for a region ([`DebuggerScope`],
//! [`DebugLevelContext`] — upstream's RAII level override — and
//! [`capture`]): while a scope is active, every free function of this module
//! called on that thread reads and changes the scope's debugger only, so
//! tests running in parallel threads never see each other's settings or
//! output.
//!
//! **In-process ranks** (local worlds, `akantu_comm::run_local`): upstream
//! has one debugger per process, i.e. per rank. Each rank thread must run
//! under its own scope carrying its rank and size ([`rank_scope`]), so its
//! messages get its parallel context and `initialize` configures that
//! rank's debugger instead of racing on the process default.
//! `akantu_comm::run_local` installs it for every rank (a copy of the
//! calling thread's debugger: level, filter, output); code that spawns
//! rank threads itself calls [`rank_scope`] first.
//!
//! **Cost of a filtered-out macro.** The macros first compare the message
//! level with a lock-free mirror of the level in effect ([`may_print`]: an
//! atomic for the process default, written under its write lock, and a
//! thread-local copy of the innermost scope's level); only a message that
//! passes takes the lock (or the scope borrow) for the exact
//! [`test_level`] with the module filter, then formats.
//!
//! # Differences (documented)
//! * File names are Rust's `file!()` (workspace-relative), function names
//!   the last segment of the enclosing item's path; module names are the
//!   crate name without `akantu_` (`core`, `mesh`, …; upstream: the CMake
//!   package name, `core`, `implicit`, …).
//! * Errors are values (D-018): creating an [`crate::Error`] prints
//!   nothing; layers that turn an error into an exception or a process
//!   failure print upstream's `###` line or terminate report with
//!   [`aka_exception_message!`](crate::aka_exception_message) /
//!   [`format_uncaught`].
//! * [`Debugger::set_log_file`] reports a file that cannot be created
//!   (upstream's `std::ofstream` fails silently and later messages are
//!   lost; UD-273).

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::fmt::{self, Write as _};
use std::fs::File;
use std::io::Write as _;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Error, Result};
use crate::random::atol;
use crate::types::Rank;

/// Upstream `akantu::DebugLevel` (`aka_error.hh:34-64`). Any `int` is a
/// valid level (upstream converts `atoi`/`long` values to the enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DebugLevel(pub i32);

impl DebugLevel {
    /// `dbl0`.
    pub const LEVEL_0: Self = Self(0);
    /// `dblError`.
    pub const ERROR: Self = Self(0);
    /// `dblAssert`.
    pub const ASSERT: Self = Self(0);
    /// `dbl1`.
    pub const LEVEL_1: Self = Self(1);
    /// `dblException`.
    pub const EXCEPTION: Self = Self(1);
    /// `dblCritical`.
    pub const CRITICAL: Self = Self(1);
    /// `dbl2`.
    pub const LEVEL_2: Self = Self(2);
    /// `dblMajor`.
    pub const MAJOR: Self = Self(2);
    /// `dbl3`.
    pub const LEVEL_3: Self = Self(3);
    /// `dblCall`.
    pub const CALL: Self = Self(3);
    /// `dblSecondary`.
    pub const SECONDARY: Self = Self(3);
    /// `dblHead`.
    pub const HEAD: Self = Self(3);
    /// `dbl4`.
    pub const LEVEL_4: Self = Self(4);
    /// `dblWarning`.
    pub const WARNING: Self = Self(4);
    /// `dbl5`.
    pub const LEVEL_5: Self = Self(5);
    /// `dblInfo`.
    pub const INFO: Self = Self(5);
    /// `dbl6`.
    pub const LEVEL_6: Self = Self(6);
    /// `dblIn`.
    pub const IN: Self = Self(6);
    /// `dblOut`.
    pub const OUT: Self = Self(6);
    /// `dbl7`.
    pub const LEVEL_7: Self = Self(7);
    /// `dbl8`.
    pub const LEVEL_8: Self = Self(8);
    /// `dblTrace`.
    pub const TRACE: Self = Self(8);
    /// `dbl9`.
    pub const LEVEL_9: Self = Self(9);
    /// `dblAccessory`.
    pub const ACCESSORY: Self = Self(9);
    /// `dbl10`.
    pub const LEVEL_10: Self = Self(10);
    /// `dblDebug`.
    pub const DEBUG: Self = Self(42);
    /// `dbl100`.
    pub const LEVEL_100: Self = Self(100);
    /// `dblDump`.
    pub const DUMP: Self = Self(100);
    /// `dblTest` (`Array::printself` hides addresses, prints values).
    pub const TEST: Self = Self(1337);

    /// Upstream `DebugLevel(long)` of `--aka_debug_level`: the C++
    /// conversion to the `int`-sized enum keeps the low 32 bits (GCC).
    #[must_use]
    #[allow(clippy::cast_possible_truncation)] // C++ long → int-sized enum, modulo 2^32
    pub fn from_long(value: i64) -> Self {
        Self(value as i32)
    }

    /// Upstream `DebugLevel(std::atoi(getenv("AKA_DEBUG_LEVEL")))`: glibc
    /// `atoi` = `(int) strtol(s, NULL, 10)` (leading white space, sign,
    /// digits, rest ignored; `""` → 0; saturated `long` truncated to
    /// `int`).
    #[must_use]
    pub fn from_env_value(value: &str) -> Self {
        Self::from_long(atol(value))
    }
}

impl Default for DebugLevel {
    /// `dblWarning`, upstream's `Debugger` default.
    fn default() -> Self {
        Self::WARNING
    }
}

impl From<i32> for DebugLevel {
    fn from(v: i32) -> Self {
        Self(v)
    }
}

impl fmt::Display for DebugLevel {
    /// The integer value (upstream streams the enum as an `int`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// In-memory message sink (upstream `setOutStream` with a string stream);
/// clones share the text.
#[derive(Clone, Debug, Default)]
pub struct MessageBuffer(Arc<Mutex<String>>);

impl MessageBuffer {
    /// Empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Text written so far.
    #[must_use]
    pub fn contents(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Text written so far, removed from the buffer.
    #[must_use]
    pub fn take(&self) -> String {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn push(&self, s: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_str(s);
    }
}

/// Where messages go.
#[derive(Clone, Debug)]
enum Sink {
    Stderr,
    File(Arc<Mutex<File>>, PathBuf),
    Buffer(MessageBuffer),
}

impl Sink {
    /// Write without ever panicking (a closed standard error or a full disk
    /// loses the message, as upstream's stream would).
    fn write(&self, s: &str) {
        match self {
            Sink::Stderr => {
                let _ = std::io::stderr().lock().write_all(s.as_bytes());
            }
            Sink::File(f, _) => {
                let mut f = f.lock().unwrap_or_else(PoisonError::into_inner);
                let _ = f.write_all(s.as_bytes());
                let _ = f.flush();
            }
            Sink::Buffer(b) => b.push(s),
        }
    }
}

/// Upstream `debug::Debugger`: level, module filter, backtrace flag,
/// parallel context and output stream.
#[derive(Clone, Debug)]
pub struct Debugger {
    level: DebugLevel,
    print_backtrace: bool,
    modules_to_debug: BTreeSet<String>,
    parallel_context: String,
    sink: Sink,
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}

/// Upstream `Int pad = std::ceil(std::log10(size))` computed exactly: the
/// smallest `k` with `10^k ≥ size` (0 for `size ≤ 1`).
fn rank_pad(size: usize) -> usize {
    let mut k = 0;
    let mut p: u128 = 1;
    while p < size as u128 {
        p *= 10;
        k += 1;
    }
    k
}

/// Microseconds since the Unix epoch, truncated (upstream:
/// `size_t(duration<double, micro>(now).count())`).
fn timestamp_us() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => u64::try_from(d.as_micros()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

impl Debugger {
    /// Upstream `Debugger()`: level `dblWarning`, standard error, no
    /// parallel context, no backtraces, no module filter.
    #[must_use]
    pub fn new() -> Self {
        Self {
            level: DebugLevel::WARNING,
            print_backtrace: false,
            modules_to_debug: BTreeSet::new(),
            parallel_context: String::new(),
            sink: Sink::Stderr,
        }
    }

    /// Upstream `setDebugLevel`.
    pub fn set_debug_level(&mut self, level: DebugLevel) {
        self.level = level;
    }

    /// Upstream `getDebugLevel`.
    #[must_use]
    pub fn debug_level(&self) -> DebugLevel {
        self.level
    }

    /// Upstream `testLevel(level, module)`: the level is reached and the
    /// module is selected (levels up to `dblCritical` ignore the filter).
    #[must_use]
    pub fn test_level(&self, level: DebugLevel, module: &str) -> bool {
        let level_reached = self.level >= level;
        let correct_module = level <= DebugLevel::CRITICAL
            || self.modules_to_debug.is_empty()
            || self.modules_to_debug.contains(module);
        level_reached && correct_module
    }

    /// The line upstream's `printMessage` writes for `prefix` and `info` at
    /// the given time stamp.
    #[must_use]
    pub fn format_message(&self, prefix: &str, info: &str, timestamp_us: u64) -> String {
        format!(
            "{}{{{timestamp_us}}} {prefix} {info}\n",
            self.parallel_context
        )
    }

    /// Upstream `printMessage(prefix, level, info, module)`: written iff
    /// [`Debugger::test_level`].
    pub fn print_message(&self, prefix: &str, level: DebugLevel, info: &str, module: &str) {
        if self.test_level(level, module) {
            self.sink
                .write(&self.format_message(prefix, info, timestamp_us()));
        }
    }

    /// Upstream `setLogFile(filename)`: later messages go to a new (or
    /// truncated) file.
    ///
    /// # Errors
    /// [`Error::Io`] if the file cannot be created (upstream: silently
    /// lost messages, UD-273); the output is unchanged then.
    pub fn set_log_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let f = File::create(path).map_err(|e| Error::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        self.sink = Sink::File(Arc::new(Mutex::new(f)), path.to_path_buf());
        Ok(())
    }

    /// The log file in use, if any.
    #[must_use]
    pub fn log_file(&self) -> Option<&Path> {
        match &self.sink {
            Sink::File(_, p) => Some(p),
            _ => None,
        }
    }

    /// Messages go to a new in-memory buffer (upstream `setOutStream`),
    /// returned for reading.
    pub fn set_out_buffer(&mut self) -> MessageBuffer {
        let b = MessageBuffer::new();
        self.sink = Sink::Buffer(b.clone());
        b
    }

    /// Messages go to standard error again (upstream's default stream).
    pub fn set_out_stderr(&mut self) {
        self.sink = Sink::Stderr;
    }

    /// Upstream `setParallelContext(rank, size)`:
    /// `<pid>[R<rank right-aligned on ceil(log10(size))>|S<size>] `.
    pub fn set_parallel_context(&mut self, rank: Rank, size: usize) {
        let pad = rank_pad(size);
        self.parallel_context = format!("<{}>[R{rank:>pad$}|S{size}] ", std::process::id());
    }

    /// The current parallel context prefix (empty until set).
    #[must_use]
    pub fn parallel_context(&self) -> &str {
        &self.parallel_context
    }

    /// Upstream `printBacktrace(bool)`.
    pub fn set_print_backtrace(&mut self, on_off: bool) {
        self.print_backtrace = on_off;
    }

    /// Upstream `printBacktrace()`.
    #[must_use]
    pub fn print_backtrace(&self) -> bool {
        self.print_backtrace
    }

    /// Upstream `addModuleToDebug`.
    pub fn add_module_to_debug(&mut self, id: &str) {
        self.modules_to_debug.insert(id.to_owned());
    }

    /// Upstream `removeModuleToDebug`.
    pub fn remove_module_to_debug(&mut self, id: &str) {
        self.modules_to_debug.remove(id);
    }

    /// The module filter (sorted, as upstream's `std::set`).
    pub fn modules_to_debug(&self) -> impl Iterator<Item = &str> + '_ {
        self.modules_to_debug.iter().map(String::as_str)
    }

    /// Upstream `listModules()`: every module of the filter followed by a
    /// new line, on the debugger's output.
    pub fn list_modules(&self) {
        let mut s = String::new();
        for m in &self.modules_to_debug {
            s.push_str(m);
            s.push('\n');
        }
        self.sink.write(&s);
    }

    /// The debug-related steps of upstream `initialize`
    /// (`aka_common.cc:70-125`), in order: parallel context of
    /// `rank`/`size`, level `dblError`, backtrace flag
    /// (`--aka_print_backtrace`), level `--aka_debug_level` (`cli_level`,
    /// default `dblWarning` = 4), then, when the environment variable
    /// `AKA_DEBUG_LEVEL` is set (`env_level`), its glibc-`atoi` value.
    pub fn configure_initialize(
        &mut self,
        rank: Rank,
        size: usize,
        cli_level: i64,
        env_level: Option<&str>,
        print_backtrace: bool,
    ) {
        self.set_parallel_context(rank, size);
        self.set_debug_level(DebugLevel::ERROR);
        self.set_print_backtrace(print_backtrace);
        self.set_debug_level(DebugLevel::from_long(cli_level));
        if let Some(env) = env_level {
            self.set_debug_level(DebugLevel::from_env_value(env));
        }
    }
}

// ---------------------------------------------------------------------------
// process default and thread scopes

static GLOBAL: OnceLock<RwLock<Debugger>> = OnceLock::new();

fn global() -> &'static RwLock<Debugger> {
    GLOBAL.get_or_init(|| RwLock::new(Debugger::new()))
}

/// The process-default debugger's level, mirrored for the macros'
/// lock-free filter ([`may_print`]); written under the global write lock
/// whenever the process default changes.
static PROCESS_LEVEL: AtomicI32 = AtomicI32::new(DebugLevel::WARNING.0);

/// [`SCOPE_LEVEL`] when no scope is active on the thread.
const NO_SCOPE: i64 = i64::MIN;

thread_local! {
    static SCOPES: RefCell<Vec<Debugger>> = const { RefCell::new(Vec::new()) };
    /// The level of this thread's innermost scope ([`NO_SCOPE`]: none),
    /// kept equal to `SCOPES.last()`'s level by every change of the stack
    /// or of its top.
    static SCOPE_LEVEL: Cell<i64> = const { Cell::new(NO_SCOPE) };
}

/// Record the innermost scope's level after a change of the scope stack.
fn sync_scope_level(scopes: &[Debugger]) {
    let level = scopes.last().map_or(NO_SCOPE, |d| i64::from(d.level.0));
    SCOPE_LEVEL.with(|c| c.set(level));
}

/// Lock-free pre-filter of the macros: `false` when the debugger in
/// effect on this thread cannot print a message of `level` (its level is
/// below it), so a filtered-out macro neither formats its arguments nor
/// takes the process-wide lock or the scope borrow.
/// `true` is only a candidate: the module filter and the level are then
/// checked exactly by [`test_level`].
#[doc(hidden)]
#[inline]
#[must_use]
pub fn may_print(level: DebugLevel) -> bool {
    let scoped = SCOPE_LEVEL.with(Cell::get);
    let current = if scoped == NO_SCOPE {
        i64::from(PROCESS_LEVEL.load(Ordering::Relaxed))
    } else {
        scoped
    };
    current >= i64::from(level.0)
}

/// Run `f` on the debugger in effect on this thread (the innermost scope,
/// else the process default). `f` must not call back into this module (it
/// runs under the scope borrow or the global lock). During or after the
/// destruction of the thread's thread-local data (a message from a
/// destructor running at thread or process exit) the process default is
/// used instead of panicking (`LocalKey::try_with`).
pub(crate) fn with_current<R>(f: impl FnOnce(&Debugger) -> R) -> R {
    let mut f = Some(f);
    let scoped = SCOPES
        .try_with(|s| {
            s.borrow()
                .last()
                .map(|d| (f.take().expect("called once"))(d))
        })
        .ok()
        .flatten();
    match scoped {
        Some(r) => r,
        None => (f.take().expect("called once"))(
            &global().read().unwrap_or_else(PoisonError::into_inner),
        ),
    }
}

/// Run `f` on the debugger in effect on this thread, mutably (changes stay
/// in the innermost scope when one is active). Same restriction and same
/// fallback to the process default during thread-local destruction as
/// [`with_current`].
pub(crate) fn with_current_mut<R>(f: impl FnOnce(&mut Debugger) -> R) -> R {
    let mut f = Some(f);
    let scoped = SCOPES
        .try_with(|s| {
            let mut s = s.borrow_mut();
            let r = s.last_mut().map(|d| (f.take().expect("called once"))(d));
            if r.is_some() {
                sync_scope_level(&s);
            }
            r
        })
        .ok()
        .flatten();
    match scoped {
        Some(r) => r,
        None => {
            let mut g = global().write().unwrap_or_else(PoisonError::into_inner);
            let r = (f.take().expect("called once"))(&mut g);
            // under the write lock: the mirror follows every change
            PROCESS_LEVEL.store(g.level.0, Ordering::Relaxed);
            r
        }
    }
}

/// A copy of the debugger in effect on this thread (settings and output).
#[must_use]
pub fn current() -> Debugger {
    with_current(Debugger::clone)
}

/// A debugger installed on the current thread until the guard is dropped
/// (not `Send`: the scope belongs to its thread).
#[derive(Debug)]
#[must_use = "the scope ends when the guard is dropped"]
pub struct DebuggerScope {
    depth: usize,
    _not_send: PhantomData<*const ()>,
}

impl DebuggerScope {
    /// Install `debugger` for the current thread.
    pub fn new(debugger: Debugger) -> Self {
        let depth = SCOPES.with(|s| {
            let mut s = s.borrow_mut();
            s.push(debugger);
            sync_scope_level(&s);
            s.len() - 1
        });
        Self {
            depth,
            _not_send: PhantomData,
        }
    }
}

impl Drop for DebuggerScope {
    fn drop(&mut self) {
        SCOPES.with(|s| {
            let mut s = s.borrow_mut();
            s.truncate(self.depth);
            sync_scope_level(&s);
        });
    }
}

/// The debugger scope of one in-process rank: a copy
/// of `base` (level, module filter, backtrace flag, output) with the
/// parallel context of `rank` in a world of `size`, installed on the
/// current thread until the guard is dropped. Rank threads of a local
/// world run under one (see the module documentation, "State and
/// threads").
pub fn rank_scope(base: &Debugger, rank: Rank, size: usize) -> DebuggerScope {
    let mut d = base.clone();
    d.set_parallel_context(rank, size);
    DebuggerScope::new(d)
}

/// Upstream `debug::DebugLevelContext`: the current debugger with another
/// level until the guard is dropped (scoped to the current thread; the
/// previous level is back afterwards).
#[derive(Debug)]
#[must_use = "the level is restored when the guard is dropped"]
pub struct DebugLevelContext {
    _scope: DebuggerScope,
}

impl DebugLevelContext {
    /// Override the level with `level`.
    pub fn new(level: DebugLevel) -> Self {
        let mut d = current();
        d.set_debug_level(level);
        Self {
            _scope: DebuggerScope::new(d),
        }
    }
}

/// Run `f` with a thread-scoped debugger at `level` writing to a buffer
/// (otherwise a copy of the current settings); returns `f`'s result and the
/// text written.
pub fn capture<R>(level: DebugLevel, f: impl FnOnce() -> R) -> (R, String) {
    let mut d = current();
    d.set_debug_level(level);
    let buffer = d.set_out_buffer();
    let scope = DebuggerScope::new(d);
    let r = f();
    drop(scope);
    (r, buffer.take())
}

/// Upstream `debug::setDebugLevel` (current scope, else process default).
pub fn set_debug_level(level: DebugLevel) {
    with_current_mut(|d| d.set_debug_level(level));
}

/// Upstream `debug::getDebugLevel`.
#[must_use]
pub fn debug_level() -> DebugLevel {
    with_current(Debugger::debug_level)
}

/// Upstream `AKANTU_DEBUG_TEST(level)` for `module`.
#[must_use]
pub fn test_level(level: DebugLevel, module: &str) -> bool {
    with_current(|d| d.test_level(level, module))
}

/// Upstream `AKANTU_DEBUG_LEVEL_IS_TEST()` (`testLevel(dblTest)`).
#[must_use]
pub fn level_is_test() -> bool {
    test_level(DebugLevel::TEST, "core")
}

/// Upstream `debugger.printMessage(prefix, level, info, module)`.
pub fn print_message(prefix: &str, level: DebugLevel, info: &str, module: &str) {
    with_current(|d| d.print_message(prefix, level, info, module));
}

/// Upstream `debugger.setLogFile(filename)`.
///
/// # Errors
/// As [`Debugger::set_log_file`].
pub fn set_log_file(path: impl AsRef<Path>) -> Result<()> {
    with_current_mut(|d| d.set_log_file(path))
}

/// Upstream `debugger.setParallelContext(rank, size)`.
pub fn set_parallel_context(rank: Rank, size: usize) {
    with_current_mut(|d| d.set_parallel_context(rank, size));
}

/// Upstream `debugger.printBacktrace(bool)` (Python `debug.printBacktrace`).
pub fn set_print_backtrace(on_off: bool) {
    with_current_mut(|d| d.set_print_backtrace(on_off));
}

/// Upstream `debugger.printBacktrace()`.
#[must_use]
pub fn print_backtrace_enabled() -> bool {
    with_current(Debugger::print_backtrace)
}

/// Upstream `debugger.addModuleToDebug(id)`.
pub fn add_module_to_debug(id: &str) {
    with_current_mut(|d| d.add_module_to_debug(id));
}

/// Upstream `debugger.removeModuleToDebug(id)`.
pub fn remove_module_to_debug(id: &str) {
    with_current_mut(|d| d.remove_module_to_debug(id));
}

/// The debug steps of upstream `initialize` on the current debugger (see
/// [`Debugger::configure_initialize`]).
pub fn configure_initialize(
    rank: Rank,
    size: usize,
    cli_level: i64,
    env_level: Option<&str>,
    print_backtrace: bool,
) {
    with_current_mut(|d| d.configure_initialize(rank, size, cli_level, env_level, print_backtrace));
}

// ---------------------------------------------------------------------------
// locations (AKANTU_LOCATION) and modules

/// Module name of a `module_path!()` for the module filter: the crate name
/// without the `akantu_` prefix (`akantu_core::bbox` → `core`).
#[must_use]
pub fn module_of(module_path: &str) -> &str {
    let krate = module_path.split("::").next().unwrap_or(module_path);
    krate.strip_prefix("akantu_").unwrap_or(krate)
}

/// The enclosing function's name from the type name of a nested item
/// (`a::b::insert::f` → `insert`, closures skipped); used by the macros.
#[doc(hidden)]
#[must_use]
pub fn function_name(nested_item_type_name: &str) -> &str {
    let path = nested_item_type_name
        .strip_suffix("::__aka_f")
        .unwrap_or(nested_item_type_name);
    path.rsplit("::")
        .find(|seg| !seg.starts_with("{{closure}}") && !seg.is_empty())
        .unwrap_or(path)
}

/// Upstream `AKANTU_LOCATION`: `(<function>(): <file>:<line>)`.
#[must_use]
pub fn location(function: &str, file: &str, line: u32) -> String {
    format!("({function}(): {file}:{line})")
}

/// The name of the enclosing function (upstream `__func__`).
#[doc(hidden)]
#[macro_export]
macro_rules! __aka_function_name {
    () => {{
        fn __aka_f() {}
        fn __aka_type_name_of<T>(_: T) -> &'static str {
            ::std::any::type_name::<T>()
        }
        $crate::debug::function_name(__aka_type_name_of(__aka_f))
    }};
}

/// Upstream `AKANTU_LOCATION` at the call site.
#[macro_export]
macro_rules! aka_location {
    () => {
        $crate::debug::location($crate::__aka_function_name!(), file!(), line!())
    };
}

/// Upstream `AKANTU_DEBUG_(prefix, level, info)`: `info` followed by the
/// location, formatted only when the level is active.
#[doc(hidden)]
#[macro_export]
macro_rules! __aka_debug_with_prefix {
    ($prefix:expr, $level:expr, $($arg:tt)+) => {{
        let __aka_level: $crate::debug::DebugLevel = $level;
        // lock-free level pre-filter; the module name
        // is computed once per call site
        if $crate::debug::may_print(__aka_level) {
            static __AKA_MODULE: ::std::sync::OnceLock<&'static str> =
                ::std::sync::OnceLock::new();
            let __aka_module =
                *__AKA_MODULE.get_or_init(|| $crate::debug::module_of(module_path!()));
            if $crate::debug::test_level(__aka_level, __aka_module) {
                $crate::debug::print_message(
                    $prefix,
                    __aka_level,
                    &format!("{} {}", format_args!($($arg)+), $crate::aka_location!()),
                    __aka_module,
                );
            }
        }
    }};
}

/// Upstream `AKANTU_DEBUG(level, info)` (prefix `"   "`).
#[macro_export]
macro_rules! aka_debug {
    ($level:expr, $($arg:tt)+) => {
        $crate::__aka_debug_with_prefix!("   ", $level, $($arg)+)
    };
}

/// Upstream `AKANTU_DEBUG_INFO(info)` (`dblInfo`, prefix `"---"`).
#[macro_export]
macro_rules! aka_debug_info {
    ($($arg:tt)+) => {
        $crate::__aka_debug_with_prefix!("---", $crate::debug::DebugLevel::INFO, $($arg)+)
    };
}

/// Upstream `AKANTU_DEBUG_WARNING(info)` (`dblWarning`, prefix `"/!\"`).
#[macro_export]
macro_rules! aka_debug_warning {
    ($($arg:tt)+) => {
        $crate::__aka_debug_with_prefix!("/!\\", $crate::debug::DebugLevel::WARNING, $($arg)+)
    };
}

/// Upstream `AKANTU_DEBUG_TRACE(info)` (`dblTrace`, prefix `">>>"`).
#[macro_export]
macro_rules! aka_debug_trace {
    ($($arg:tt)+) => {
        $crate::__aka_debug_with_prefix!(">>>", $crate::debug::DebugLevel::TRACE, $($arg)+)
    };
}

/// Upstream `AKANTU_DEBUG_IN()` (`dblIn`, prefix `"==>"`, `<function>()`).
#[macro_export]
macro_rules! aka_debug_in {
    () => {
        $crate::__aka_debug_with_prefix!(
            "==>",
            $crate::debug::DebugLevel::IN,
            "{}()",
            $crate::__aka_function_name!()
        )
    };
}

/// Upstream `AKANTU_DEBUG_OUT()` (`dblOut`, prefix `"<=="`, `<function>()`).
#[macro_export]
macro_rules! aka_debug_out {
    () => {
        $crate::__aka_debug_with_prefix!(
            "<==",
            $crate::debug::DebugLevel::OUT,
            "{}()",
            $crate::__aka_function_name!()
        )
    };
}

/// The message upstream `AKANTU_ERROR(info)` prints before throwing
/// (`dblError`, prefix `"!!! "`); the caller then returns its error.
#[macro_export]
macro_rules! aka_debug_error {
    ($($arg:tt)+) => {
        $crate::__aka_debug_with_prefix!("!!! ", $crate::debug::DebugLevel::ERROR, $($arg)+)
    };
}

/// The message upstream's non-silent `AKANTU_EXCEPTION(info)` prints before
/// throwing (`throwException`: `dblWarning`, prefix `"###"`, location);
/// for layers that turn an error into an exception (D-018: errors are
/// values, creating one prints nothing).
#[macro_export]
macro_rules! aka_exception_message {
    ($($arg:tt)+) => {
        $crate::__aka_debug_with_prefix!("###", $crate::debug::DebugLevel::WARNING, $($arg)+)
    };
}

// ---------------------------------------------------------------------------
// backtraces and the uncaught-error report

/// The current stack, one entry per frame (upstream `getBacktrace`; the
/// frame text is Rust's, UD-274). Empty when the platform cannot capture.
#[must_use]
pub fn get_backtrace() -> Vec<String> {
    let text = std::backtrace::Backtrace::force_capture().to_string();
    let mut frames: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let is_frame_start = t
            .split_once(':')
            .is_some_and(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if is_frame_start {
            let (_, rest) = t.split_once(':').unwrap_or(("", t));
            frames.push(rest.trim().to_owned());
        } else if let Some(last) = frames.last_mut() {
            let _ = write!(last, " ({})", t);
        }
    }
    frames
}

/// Upstream `printBacktrace(backtrace)` text: `BACKTRACE :  <n> stack
/// frames.`, one `  [<i>] <frame>` line per frame (`i` from 1, right-aligned
/// on the number of digits of `n`), `END BACKTRACE`.
#[must_use]
pub fn format_backtrace(frames: &[String]) -> String {
    let w = frames.len().max(1).to_string().len();
    let mut s = format!("BACKTRACE :  {} stack frames.\n", frames.len());
    for (i, f) in frames.iter().enumerate() {
        let _ = writeln!(s, "  [{:>w$}] {f}", i + 1);
    }
    s.push_str("END BACKTRACE\n");
    s
}

/// Upstream `printBacktrace(backtrace)`: [`format_backtrace`] on standard
/// error (upstream writes to `std::cerr`, not the debugger's stream).
pub fn print_backtrace(frames: &[String]) {
    let _ = std::io::stderr()
        .lock()
        .write_all(format_backtrace(frames).as_bytes());
}

/// Upstream's terminate-handler report for an uncaught `akantu::Exception`
/// (`aka_error.cc:224-260`): the backtrace captured when the error was
/// raised (`captured`, empty unless the backtrace flag was on), `!! Uncaught
/// akantu::Exception of type <type_name> !!`, `what(): "<what>"`, and, when
/// the backtrace flag is on, `Random generator seed: <seed>` and the
/// current stack (`current`).
#[must_use]
pub fn format_uncaught(
    type_name: &str,
    what: &str,
    captured: &[String],
    print_backtrace: bool,
    seed: i64,
    current: &[String],
) -> String {
    let mut s = format_backtrace(captured);
    let _ = writeln!(
        s,
        "!! Uncaught akantu::Exception of type {type_name} !!\nwhat(): \"{what}\""
    );
    if print_backtrace {
        let _ = writeln!(s, "Random generator seed: {seed}");
        s.push_str(&format_backtrace(current));
    }
    s
}

// ---------------------------------------------------------------------------
// bridge from the `log` facade

/// Routes `log` records into the debugger in effect on the logging thread:
/// `error` → `dblError` `"!!! "`, `warn` → `dblWarning` `"/!\"`, `info` →
/// `dblInfo` `"---"`, `debug` → `dblTrace` `">>>"`, `trace` → `dblDump`
/// `"   "`; the location is `(<module path>: <file>:<line>)`.
#[derive(Debug, Default)]
pub struct LogBridge;

fn log_level(level: log::Level) -> (DebugLevel, &'static str) {
    match level {
        log::Level::Error => (DebugLevel::ERROR, "!!! "),
        log::Level::Warn => (DebugLevel::WARNING, "/!\\"),
        log::Level::Info => (DebugLevel::INFO, "---"),
        log::Level::Debug => (DebugLevel::TRACE, ">>>"),
        log::Level::Trace => (DebugLevel::DUMP, "   "),
    }
}

impl log::Log for LogBridge {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        let (level, _) = log_level(metadata.level());
        test_level(level, module_of(metadata.target()))
    }

    fn log(&self, record: &log::Record<'_>) {
        let (level, prefix) = log_level(record.level());
        let module = module_of(record.target());
        if test_level(level, module) {
            let info = format!(
                "{} ({}: {}:{})",
                record.args(),
                record.module_path().unwrap_or(record.target()),
                record.file().unwrap_or("?"),
                record.line().unwrap_or(0)
            );
            print_message(prefix, level, &info, module);
        }
    }

    fn flush(&self) {}
}

static LOG_BRIDGE: LogBridge = LogBridge;

/// Install [`LogBridge`] as the process's `log` logger (all levels pass
/// the facade; the debugger filters).
///
/// # Errors
/// [`Error::InvalidArgument`] if another logger is already installed.
pub fn install_log_bridge() -> Result<()> {
    log::set_logger(&LOG_BRIDGE)
        .map_err(|e| Error::InvalidArgument(format!("cannot install the debug log bridge: {e}")))?;
    log::set_max_level(log::LevelFilter::Trace);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_match_upstream() {
        assert_eq!(DebugLevel::default(), DebugLevel::WARNING);
        let v = [
            (DebugLevel::ERROR, 0),
            (DebugLevel::ASSERT, 0),
            (DebugLevel::EXCEPTION, 1),
            (DebugLevel::CRITICAL, 1),
            (DebugLevel::MAJOR, 2),
            (DebugLevel::CALL, 3),
            (DebugLevel::SECONDARY, 3),
            (DebugLevel::HEAD, 3),
            (DebugLevel::WARNING, 4),
            (DebugLevel::INFO, 5),
            (DebugLevel::IN, 6),
            (DebugLevel::OUT, 6),
            (DebugLevel::LEVEL_7, 7),
            (DebugLevel::TRACE, 8),
            (DebugLevel::ACCESSORY, 9),
            (DebugLevel::LEVEL_10, 10),
            (DebugLevel::DEBUG, 42),
            (DebugLevel::DUMP, 100),
            (DebugLevel::TEST, 1337),
        ];
        for (l, n) in v {
            assert_eq!(l.0, n);
        }
        // AKA_DEBUG_LEVEL with glibc atoi; --aka_debug_level (long → int)
        assert_eq!(DebugLevel::from_env_value(" 12abc"), DebugLevel(12));
        assert_eq!(DebugLevel::from_env_value(""), DebugLevel(0));
        assert_eq!(DebugLevel::from_env_value("-3"), DebugLevel(-3));
        assert_eq!(DebugLevel::from_env_value("4294967297"), DebugLevel(1));
        assert_eq!(DebugLevel::from_long(1 << 32 | 5), DebugLevel(5));
    }

    #[test]
    fn test_level_and_module_filter() {
        let mut d = Debugger::new();
        assert!(d.test_level(DebugLevel::WARNING, "core"));
        assert!(!d.test_level(DebugLevel::INFO, "core"));
        d.set_debug_level(DebugLevel::DUMP);
        d.add_module_to_debug("mesh");
        assert!(d.test_level(DebugLevel::INFO, "mesh"));
        assert!(!d.test_level(DebugLevel::INFO, "core"));
        // levels up to dblCritical ignore the filter
        assert!(d.test_level(DebugLevel::CRITICAL, "core"));
        assert!(!d.test_level(DebugLevel::MAJOR, "core"));
        d.remove_module_to_debug("mesh");
        assert!(d.test_level(DebugLevel::INFO, "core"));
        assert!(!d.test_level(DebugLevel::TEST, "core"));
    }

    #[test]
    fn message_format_and_parallel_context() {
        let mut d = Debugger::new();
        assert_eq!(
            d.format_message("/!\\", "careful (f(): a.rs:3)", 1234),
            "{1234} /!\\ careful (f(): a.rs:3)\n"
        );
        let pid = std::process::id();
        d.set_parallel_context(0, 1);
        assert_eq!(d.parallel_context(), format!("<{pid}>[R0|S1] "));
        d.set_parallel_context(3, 12);
        assert_eq!(d.parallel_context(), format!("<{pid}>[R 3|S12] "));
        d.set_parallel_context(7, 10);
        assert_eq!(d.parallel_context(), format!("<{pid}>[R7|S10] "));
        d.set_parallel_context(5, 101);
        assert_eq!(d.parallel_context(), format!("<{pid}>[R  5|S101] "));
        assert_eq!(
            d.format_message("---", "x", 7),
            format!("<{pid}>[R  5|S101] {{7}} --- x\n")
        );
        assert_eq!(
            (rank_pad(0), rank_pad(1), rank_pad(2), rank_pad(100)),
            (0, 0, 1, 2)
        );
    }

    #[test]
    fn macros_print_prefix_message_and_location() {
        fn emitting_function() {
            crate::aka_debug_warning!("value {} too large", 3);
            crate::aka_debug_info!("info");
            crate::aka_debug_in!();
        }
        let ((), text) = capture(DebugLevel::INFO, emitting_function);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        let strip = |l: &str| -> String {
            let (ts, rest) = l.split_once("} ").unwrap();
            assert!(ts.starts_with('{') && ts[1..].bytes().all(|b| b.is_ascii_digit()));
            rest.to_owned()
        };
        let file = file!();
        assert!(strip(lines[0]).starts_with(&format!(
            "/!\\ value 3 too large (emitting_function(): {file}:"
        )));
        assert!(strip(lines[1]).starts_with(&format!("--- info (emitting_function(): {file}:")));
        // dblIn needs level 6
        let ((), text) = capture(DebugLevel::IN, emitting_function);
        assert!(
            text.lines()
                .nth(2)
                .is_some_and(|l| strip(l).starts_with(&format!(
                    "==> emitting_function() (emitting_function(): {file}:"
                )))
        );
        // nothing below the level
        let ((), text) = capture(DebugLevel::ERROR, emitting_function);
        assert!(text.is_empty());
        assert_eq!(module_of("akantu_core::debug::tests"), "core");
        assert_eq!(module_of("akantu"), "akantu");
        assert_eq!(function_name("a::b::insert::__aka_f"), "insert");
        assert_eq!(function_name("a::run::{{closure}}::__aka_f"), "run");
    }

    /// The lock-free pre-filter agrees with the
    /// debugger in effect through scope pushes, level changes inside a
    /// scope, nested `DebugLevelContext`s, `capture` and scope drops (a
    /// stale mirror would drop messages: `may_print` must be `true`
    /// whenever the level is reached), and a filtered-out macro formats
    /// nothing.
    #[test]
    fn fast_filter_follows_the_scopes() {
        let levels = [-1, 0, 1, 4, 5, 6, 8, 100, 1337, 5000];
        let agree = |what: &str| {
            let current = debug_level();
            for l in levels {
                assert_eq!(
                    may_print(DebugLevel(l)),
                    current >= DebugLevel(l),
                    "{what}: level {current}, message {l}"
                );
            }
        };
        let mut d = Debugger::new();
        d.set_debug_level(DebugLevel::ERROR);
        let _sink = d.set_out_buffer();
        let outer = DebuggerScope::new(d);
        agree("outer scope at dblError");
        set_debug_level(DebugLevel::TRACE);
        agree("outer scope raised to dblTrace");
        {
            let _c = DebugLevelContext::new(DebugLevel(1337));
            agree("level context dblTest");
            {
                let _c2 = DebugLevelContext::new(DebugLevel(-1));
                agree("nested level context -1");
                set_debug_level(DebugLevel::DUMP);
                agree("nested context raised");
            }
            agree("back in the first context");
        }
        agree("back in the outer scope");
        let ((), text) = capture(DebugLevel::WARNING, || {
            agree("capture at dblWarning");
            crate::aka_debug_info!("hidden {}", 1);
        });
        assert!(text.is_empty(), "{text}");
        agree("after capture");
        drop(outer);
        let scoped = SCOPE_LEVEL.with(Cell::get);
        assert_eq!(scoped, NO_SCOPE, "no scope left on this thread");

        // a filtered-out macro does not format its arguments
        struct Counted<'a>(&'a Cell<usize>);
        impl fmt::Display for Counted<'_> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.set(self.0.get() + 1);
                f.write_str("x")
            }
        }
        let n = Cell::new(0);
        let ((), text) = capture(DebugLevel::WARNING, || {
            crate::aka_debug_info!("{}", Counted(&n));
            crate::aka_debug_trace!("{}", Counted(&n));
        });
        assert_eq!((n.get(), text.as_str()), (0, ""));
        let ((), text) = capture(DebugLevel::INFO, || {
            crate::aka_debug_info!("{}", Counted(&n));
        });
        assert_eq!(n.get(), 1, "{text}");
    }

    #[test]
    fn scopes_are_per_thread_and_nest() {
        let before = debug_level();
        {
            let _ctx = DebugLevelContext::new(DebugLevel::DUMP);
            assert_eq!(debug_level(), DebugLevel::DUMP);
            set_debug_level(DebugLevel::TRACE); // changes the scope only
            assert_eq!(debug_level(), DebugLevel::TRACE);
            let other = std::thread::spawn(debug_level).join().unwrap();
            assert_ne!(other, DebugLevel::TRACE);
            {
                let _inner = DebugLevelContext::new(DebugLevel::TEST);
                assert!(level_is_test());
            }
            assert_eq!(debug_level(), DebugLevel::TRACE);
        }
        assert_eq!(debug_level(), before);
    }

    #[test]
    fn initialize_sequence_and_log_file() {
        let mut d = Debugger::new();
        d.configure_initialize(0, 1, 4, None, false);
        assert_eq!(d.debug_level(), DebugLevel::WARNING);
        d.configure_initialize(1, 2, 5, Some("  8 "), true);
        assert_eq!(d.debug_level(), DebugLevel::TRACE);
        assert!(d.print_backtrace());
        assert!(d.parallel_context().ends_with("[R1|S2] "));
        let dir = std::env::temp_dir().join(format!("akantu-rs-dbg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("log.txt");
        d.set_log_file(&path).unwrap();
        d.print_message("---", DebugLevel::INFO, "to the file", "core");
        d.list_modules();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.ends_with(" --- to the file\n"), "{text}");
        assert_eq!(d.log_file(), Some(path.as_path()));
        // an impossible file is reported, the output is unchanged
        assert!(d.set_log_file(dir.join("missing/dir/log.txt")).is_err());
        assert_eq!(d.log_file(), Some(path.as_path()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backtrace_and_uncaught_report_layout() {
        assert_eq!(
            format_backtrace(&[]),
            "BACKTRACE :  0 stack frames.\nEND BACKTRACE\n"
        );
        let frames: Vec<String> = (1..=10).map(|i| format!("f{i}")).collect();
        let s = format_backtrace(&frames);
        assert!(s.starts_with("BACKTRACE :  10 stack frames.\n  [ 1] f1\n"));
        assert!(s.ends_with("  [10] f10\nEND BACKTRACE\n"));
        assert_eq!(
            format_uncaught("akantu::debug::Exception", "boom", &[], false, 7, &[]),
            "BACKTRACE :  0 stack frames.\nEND BACKTRACE\n!! Uncaught akantu::Exception of type \
             akantu::debug::Exception !!\nwhat(): \"boom\"\n"
        );
        let r = format_uncaught("T", "w", &[], true, 42, &["g".to_owned()]);
        assert!(r.contains("Random generator seed: 42\nBACKTRACE :  1 stack frames.\n  [1] g\n"));
        // capturing works or yields nothing (no panic)
        let _ = get_backtrace();
    }

    /// A destructor running during the thread's thread-local destruction
    /// (e.g. a communicator dropped at thread or process exit, D-067) may
    /// still consult the debugger: it falls back to the process default
    /// instead of panicking (a panic there aborts the process). Registered
    /// before the scope stack, the probe is destroyed after it on targets
    /// that destroy thread locals in reverse order of first use (glibc).
    #[test]
    fn debugger_answers_during_thread_local_destruction() {
        struct ConsultOnDrop(std::sync::mpsc::Sender<bool>);
        impl Drop for ConsultOnDrop {
            fn drop(&mut self) {
                let in_scope = with_current(|d| d.test_level(DebugLevel::ERROR, "core"));
                with_current_mut(|_| ());
                let _ = self.0.send(in_scope);
            }
        }
        thread_local! {
            static PROBE: RefCell<Option<ConsultOnDrop>> = const { RefCell::new(None) };
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            PROBE.with(|p| *p.borrow_mut() = Some(ConsultOnDrop(tx)));
            // the scope stack is created (and registered) after the probe
            let scope = rank_scope(&current(), 1, 2);
            assert!(with_current(|d| d.test_level(DebugLevel::ERROR, "core")));
            drop(scope);
        })
        .join()
        .expect("no panic in the thread or its thread-local destructors");
        assert_eq!(rx.recv_timeout(std::time::Duration::from_secs(5)), Ok(true));
    }
}
