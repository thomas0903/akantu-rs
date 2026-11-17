// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Benchmark-suite
// harness of the port; no upstream counterpart.

//! Shared harness of the whole-library benchmark suite (`suite_*` binaries).
//!
//! Every suite binary has the same command line:
//!
//! ```text
//! suite_<area> --list
//! suite_<area> --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE]
//!              [--loop-seconds S] [--threads N] [--grain finest|auto]
//! ```
//!
//! and writes one JSON object per line and per (case, repetition, phase):
//!
//! ```text
//! {"suite":"solid","case":"explicit_tet4_ss","size":"m","rep":0,"phase":"solve_step",
//!  "calls":200,"seconds":1.2345,"n_elements":162000,"n_nodes":29791,"n_dofs":89373,
//!  "ranks":1,"threads":1,"grain":"auto","precision":"bitwise","digest":"9f3c...",
//!  "commit":"59dd2229c","features":"",
//!  "profile":"release"}
//! ```
//!
//! A case is a function of a [`Ctx`]: it times its set-up phases with
//! [`Ctx::once`], its loop phases with [`Ctx::repeat`] (three untimed
//! warm-up calls, then `calls` timed calls) or [`Ctx::repeat_primary`] (the
//! same, extended to `--loop-seconds` for profiling), records the problem
//! size with [`Ctx::set_counts`] and the FNV-1a digest of its final state
//! with [`Ctx::set_digest`]. Each repetition runs the whole case from a new
//! model, so the digest does not depend on `--reps`. With
//! `--loop-seconds` the primary loops run longer and the digest describes
//! that longer run (it is printed, but is not comparable).
//!
//! At P > 1 ranks every rank runs the case; each phase reports the largest
//! time over the ranks, and the line is written by rank 0.

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::exec::Grain;
use akantu_core::{Error, Result};
use akantu_linalg::eigen_sparse_lu::SparseOrdering;

/// Warm-up calls before a loop phase is timed (suite contract).
pub const WARMUP_CALLS: usize = 3;

/// Problem size of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// The whole case under about a second (tests).
    S,
    /// The timing size.
    M,
    /// Scaling studies (about 8× `m`).
    L,
}

impl Size {
    /// Name on the command line and in the output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Size::S => "s",
            Size::M => "m",
            Size::L => "l",
        }
    }
}

impl std::str::FromStr for Size {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "s" => Ok(Size::S),
            "m" => Ok(Size::M),
            "l" => Ok(Size::L),
            _ => Err(Error::invalid(format!("size `{s}`: expected s, m or l"))),
        }
    }
}

/// Options of a run (`--case ...`).
#[derive(Clone, Debug)]
pub struct Options {
    /// Requested cases (`all` expanded by [`run_main`]).
    pub cases: Vec<String>,
    /// Problem size.
    pub size: Size,
    /// Repetitions of every case.
    pub reps: usize,
    /// Output file (appended); standard output when absent.
    pub out: Option<PathBuf>,
    /// Extend the primary loop phases to this many seconds (profiling).
    pub loop_seconds: Option<f64>,
    /// `--threads N`: the thread count of the process default execution
    /// policy ([`apply_exec_options`]); `None` keeps the environment's.
    pub threads: Option<usize>,
    /// `--grain finest|auto`: the split grain of that policy; `None` keeps
    /// the environment's.
    pub grain: Option<Grain>,
    /// `--ordering colamd|nested_dissection|auto`: the column ordering of
    /// the `eigen` sparse solver of every solver case
    /// ([`set_sparse_ordering`]); `None` keeps the solver's default.
    pub ordering: Option<SparseOrdering>,
    /// `--dump-state FILE` ([`set_state_dump`]).
    pub dump_state: Option<PathBuf>,
}

static SPARSE_ORDERING: std::sync::OnceLock<SparseOrdering> = std::sync::OnceLock::new();

/// Selects the sparse solver ordering of the solver cases of this process
/// (`--ordering`, set once before the first case; D-125).
pub fn set_sparse_ordering(o: SparseOrdering) {
    let _ = SPARSE_ORDERING.set(o);
}

/// The ordering of [`set_sparse_ordering`] (`None`: the solver's default,
/// the digests of the default runs).
#[must_use]
pub fn sparse_ordering() -> Option<SparseOrdering> {
    SPARSE_ORDERING.get().copied()
}

static STATE_DUMP: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);

/// `--dump-state FILE`: every real a digest absorbs is also appended to
/// `FILE` (little-endian `f64`), to compare the states of two runs value by
/// value (e.g. two orderings); the digest is unchanged.
///
/// # Errors
/// The file cannot be created.
pub fn set_state_dump(path: &std::path::Path) -> Result<()> {
    let f = std::fs::File::create(path)
        .map_err(|e| Error::invalid(format!("--dump-state {}: {e}", path.display())))?;
    if let Ok(mut g) = STATE_DUMP.lock() {
        *g = Some(f);
    }
    Ok(())
}

/// Appends `v` to the [`set_state_dump`] file, if any.
pub fn dump_state_reals(v: &[f64]) {
    if let Ok(mut g) = STATE_DUMP.lock()
        && let Some(f) = g.as_mut()
    {
        let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
        let _ = f.write_all(&bytes);
    }
}

/// Name of a split grain in the command line and the JSON lines.
#[must_use]
pub const fn grain_name(g: Grain) -> &'static str {
    match g {
        Grain::Auto => "auto",
        Grain::Finest => "finest",
    }
}

/// The precision mode (`bitwise` or `fast`) of the process default
/// execution policy, which every model of a run starts with; recorded in the
/// JSON lines so that a campaign can check the requested mode took effect
/// (`unknown` if the environment is invalid).
#[must_use]
pub fn precision_name() -> &'static str {
    akantu_core::exec::ExecPolicy::process_default().map_or("unknown", |p| p.precision().name())
}

/// The ordering of the `eigen` solver in force for the solver cases (the
/// result lines' `ordering`): `--ordering`, else the process default
/// `AKANTU_SPARSE_ORDERING`, else `auto` in the fast precision and `colamd`
/// in the bitwise one (D-125).
#[must_use]
pub fn ordering_name() -> String {
    if let Some(o) = sparse_ordering() {
        return o.name().into();
    }
    match std::env::var(akantu_linalg::eigen_sparse_lu::ENV_ORDERING) {
        Ok(v) if !v.trim().is_empty() => v.trim().to_owned(),
        _ if precision_name() == "fast" => "auto".into(),
        _ => "colamd".into(),
    }
}

/// Apply `--threads` / `--grain` to the process default execution policy
/// (call it before any model is created) and return the thread count and
/// grain models created afterwards use: the given values, else those of the
/// environment (`AKANTU_NUM_THREADS`, `AKANTU_EXEC_GRAIN`, default 1 and
/// `auto`).
///
/// # Errors
/// An explicit value that differs from the environment variable of the
/// same setting (the message names both), an invalid environment, or
/// threads the system refuses.
pub fn apply_exec_options(threads: Option<usize>, grain: Option<Grain>) -> Result<(usize, Grain)> {
    use akantu_core::exec::{ENV_GRAIN, ENV_THREADS, ExecPolicy};
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    if let (Some(n), Some(v)) = (threads, env(ENV_THREADS))
        && v.trim().parse::<usize>().ok() != Some(n)
    {
        return Err(Error::invalid(format!(
            "--threads {n} conflicts with {ENV_THREADS}={v}"
        )));
    }
    if let (Some(g), Some(v)) = (grain, env(ENV_GRAIN))
        && v.parse::<Grain>().ok() != Some(g)
    {
        return Err(Error::invalid(format!(
            "--grain {} conflicts with {ENV_GRAIN}={v}",
            grain_name(g)
        )));
    }
    let mut p = ExecPolicy::process_default()?;
    if let Some(n) = threads {
        p = p.with_threads(n)?;
    }
    if let Some(g) = grain {
        p = p.with_grain(g);
    }
    if threads.is_some() || grain.is_some() {
        ExecPolicy::set_process_default(p);
    }
    Ok((p.threads(), p.grain()))
}

/// A parsed command line.
#[derive(Clone, Debug)]
pub enum Command {
    /// `--list`.
    List,
    /// `--case ...`.
    Run(Options),
}

fn usage(bin: &str) -> String {
    format!(
        "usage: {bin} --list\n       {bin} --case <name>[,<name>...]|all --size s|m|l [--reps N] \
         [--out FILE] [--loop-seconds S] [--threads N] [--grain finest|auto] \
         [--ordering colamd|nested_dissection|auto] [--dump-state FILE]"
    )
}

/// Parse the command line (without the program name).
///
/// # Errors
/// Unknown or malformed arguments (the message holds the usage).
pub fn parse_args(bin: &str, args: impl IntoIterator<Item = String>) -> Result<Command> {
    let mut it = args.into_iter();
    let mut list = false;
    let mut cases: Option<Vec<String>> = None;
    let mut size: Option<Size> = None;
    let mut reps = 1usize;
    let mut out = None;
    let mut loop_seconds = None;
    let mut threads = None;
    let mut grain = None;
    let mut ordering = None;
    let mut dump_state = None;
    let bad = |what: &str| Error::invalid(format!("{what}\n{}", usage(bin)));
    while let Some(k) = it.next() {
        let mut value = || it.next().ok_or_else(|| bad(&format!("{k}: missing value")));
        match k.as_str() {
            "--list" => list = true,
            "--case" | "--cases" => {
                cases = Some(
                    value()?
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect(),
                );
            }
            "--size" => size = Some(value()?.parse()?),
            "--reps" => {
                reps = value()?.parse().map_err(|_| bad("--reps: not a count"))?;
            }
            "--out" => out = Some(PathBuf::from(value()?)),
            "--loop-seconds" => {
                let s: f64 = value()?
                    .parse()
                    .map_err(|_| bad("--loop-seconds: not a number"))?;
                if !(s > 0.0 && s.is_finite()) {
                    return Err(bad("--loop-seconds: must be positive"));
                }
                loop_seconds = Some(s);
            }
            "--threads" => {
                let n: usize = value()?
                    .parse()
                    .map_err(|_| bad("--threads: not a count"))?;
                if n == 0 {
                    return Err(bad("--threads must be at least 1"));
                }
                threads = Some(n);
            }
            "--grain" => {
                grain = Some(
                    value()?
                        .parse::<Grain>()
                        .map_err(|e| bad(&format!("--grain: {e}")))?,
                );
            }
            "--ordering" => ordering = Some(value()?.parse::<SparseOrdering>()?),
            "--dump-state" => dump_state = Some(PathBuf::from(value()?)),
            "-h" | "--help" => return Err(bad("")),
            _ => return Err(bad(&format!("unknown argument `{k}`"))),
        }
    }
    if list {
        return Ok(Command::List);
    }
    let cases = cases.ok_or_else(|| bad("--case is required"))?;
    let size = size.ok_or_else(|| bad("--size is required"))?;
    if reps == 0 {
        return Err(bad("--reps must be at least 1"));
    }
    Ok(Command::Run(Options {
        cases,
        size,
        reps,
        out,
        loop_seconds,
        threads,
        grain,
        ordering,
        dump_state,
    }))
}

/// What a case did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Ran; its phases are written.
    Done,
    /// Not available in this build or at this rank count (reason).
    Skip(String),
}

/// A case: name, one-line description (which cost it exposes), function.
#[derive(Clone, Copy)]
pub struct Case {
    /// Stable identifier.
    pub name: &'static str,
    /// One line: what the case exposes.
    pub description: &'static str,
    /// The case.
    pub run: fn(&mut Ctx<'_>) -> Result<Outcome>,
}

impl std::fmt::Debug for Case {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Case").field("name", &self.name).finish()
    }
}

/// One timed phase of a run.
#[derive(Clone, Debug, PartialEq)]
pub struct PhaseRecord {
    /// Phase identifier.
    pub phase: String,
    /// Calls (1 for set-up phases).
    pub calls: usize,
    /// Seconds over all calls.
    pub seconds: f64,
}

/// A JSON value of an extra field.
#[derive(Clone, Debug, PartialEq)]
pub enum Extra {
    /// Integer.
    Int(i64),
    /// Real.
    Real(f64),
    /// Text.
    Text(String),
}

/// Context of one repetition of one case.
#[derive(Debug)]
pub struct Ctx<'a> {
    opts: &'a Options,
    comm: &'a Communicator,
    rep: usize,
    phases: Vec<PhaseRecord>,
    counts: [u64; 3],
    digest: Option<u64>,
    extras: Vec<(String, Extra)>,
}

impl<'a> Ctx<'a> {
    /// A fresh context (used by [`run_main`] and tests).
    #[must_use]
    pub fn new(opts: &'a Options, comm: &'a Communicator, rep: usize) -> Self {
        Self {
            opts,
            comm,
            rep,
            phases: Vec::new(),
            counts: [0; 3],
            digest: None,
            extras: Vec::new(),
        }
    }

    /// Problem size.
    #[must_use]
    pub fn size(&self) -> Size {
        self.opts.size
    }

    /// `s`, `m` or `l` by the problem size.
    pub fn pick<T>(&self, s: T, m: T, l: T) -> T {
        match self.opts.size {
            Size::S => s,
            Size::M => m,
            Size::L => l,
        }
    }

    /// The run's communicator (the MPI world, or serial).
    #[must_use]
    pub fn comm(&self) -> &Communicator {
        self.comm
    }

    /// Repetition index.
    #[must_use]
    pub fn rep(&self) -> usize {
        self.rep
    }

    /// Whether `--loop-seconds` is set (profiling run).
    #[must_use]
    pub fn profiling(&self) -> bool {
        self.opts.loop_seconds.is_some()
    }

    /// Time a set-up phase (one call).
    ///
    /// # Errors
    /// `f`'s error.
    pub fn once<T>(&mut self, phase: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let t = Instant::now();
        let r = f()?;
        self.record(phase, 1, t.elapsed().as_secs_f64());
        Ok(r)
    }

    /// Time a loop phase: [`WARMUP_CALLS`] calls `f(false)`, then `calls`
    /// timed calls `f(true)` (the flag lets a case time sub-phases only in
    /// timed calls). Returns the number of timed calls.
    ///
    /// # Errors
    /// `f`'s error.
    pub fn repeat(
        &mut self,
        phase: &str,
        calls: usize,
        f: impl FnMut(bool) -> Result<()>,
    ) -> Result<usize> {
        self.repeat_impl(phase, calls, false, f)
    }

    /// [`Ctx::repeat`] for a case's main loop: with `--loop-seconds S` it
    /// continues in blocks of `calls` calls until `S` seconds have passed
    /// (the decision is rank 0's, broadcast, so that every rank makes the
    /// same number of collective calls).
    ///
    /// # Errors
    /// `f`'s error, communicator errors.
    pub fn repeat_primary(
        &mut self,
        phase: &str,
        calls: usize,
        f: impl FnMut(bool) -> Result<()>,
    ) -> Result<usize> {
        self.repeat_impl(phase, calls, true, f)
    }

    fn repeat_impl(
        &mut self,
        phase: &str,
        calls: usize,
        primary: bool,
        mut f: impl FnMut(bool) -> Result<()>,
    ) -> Result<usize> {
        for _ in 0..WARMUP_CALLS {
            f(false)?;
        }
        let block = calls.max(1);
        let t = Instant::now();
        let mut done = 0;
        for _ in 0..calls {
            f(true)?;
        }
        done += calls;
        if primary && let Some(target) = self.opts.loop_seconds {
            loop {
                let mut more = [u8::from(t.elapsed().as_secs_f64() < target)];
                self.comm.broadcast(&mut more, 0)?;
                if more[0] == 0 {
                    break;
                }
                for _ in 0..block {
                    f(true)?;
                }
                done += block;
            }
        }
        self.record(phase, done, t.elapsed().as_secs_f64());
        Ok(done)
    }

    /// Record a phase timed by the case itself (sub-phases of a loop).
    pub fn record(&mut self, phase: &str, calls: usize, seconds: f64) {
        self.phases.push(PhaseRecord {
            phase: phase.to_string(),
            calls,
            seconds,
        });
    }

    /// Problem size: elements, nodes and DOFs (global counts; at P > 1
    /// each rank passes its owned counts and [`run_main`] sums them).
    pub fn set_counts(&mut self, n_elements: usize, n_nodes: usize, n_dofs: usize) {
        self.counts = [n_elements as u64, n_nodes as u64, n_dofs as u64];
    }

    /// The digest of the final state (rank 0's value is written).
    pub fn set_digest(&mut self, digest: &Digest) {
        self.digest = Some(digest.value());
    }

    /// An extra field written on every line of the repetition (iteration
    /// counts, insertions, ...).
    pub fn extra(&mut self, key: &str, value: Extra) {
        self.extras.retain(|(k, _)| k != key);
        self.extras.push((key.to_string(), value));
    }

    /// The recorded phases.
    #[must_use]
    pub fn phases(&self) -> &[PhaseRecord] {
        &self.phases
    }

    /// The digest, once set.
    #[must_use]
    pub fn digest(&self) -> Option<u64> {
        self.digest
    }
}

/// 64-bit FNV-1a over little-endian bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Digest(u64);

impl Default for Digest {
    fn default() -> Self {
        Self::new()
    }
}

impl Digest {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;

    /// The empty digest.
    #[must_use]
    pub fn new() -> Self {
        Self(Self::OFFSET)
    }

    /// Hash raw bytes.
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
        self
    }

    /// Hash the bits of reals.
    pub fn reals(&mut self, v: &[f64]) -> &mut Self {
        dump_state_reals(v);
        for x in v {
            self.bytes(&x.to_bits().to_le_bytes());
        }
        self
    }

    /// Hash flags (one byte each).
    pub fn bools(&mut self, v: &[bool]) -> &mut Self {
        for &x in v {
            self.bytes(&[u8::from(x)]);
        }
        self
    }

    /// Hash integers.
    pub fn u64s(&mut self, v: &[u64]) -> &mut Self {
        for x in v {
            self.bytes(&x.to_le_bytes());
        }
        self
    }

    /// The hash value.
    #[must_use]
    pub fn value(&self) -> u64 {
        self.0
    }

    /// Hexadecimal form (16 digits).
    #[must_use]
    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Gather `values` (`ncomp` per entity) of the entities this rank owns to
/// rank 0, ordered by global id (`global[i]` of entity `i`, `owned[i]`
/// whether this rank owns it): the input of a rank-count-independent
/// digest. `Some(values)` on rank 0, `None` elsewhere.
///
/// # Errors
/// Inconsistent lengths, communicator errors.
pub fn gather_owned_by_global_id(
    comm: &Communicator,
    global: &[u64],
    owned: &[bool],
    values: &[f64],
    ncomp: usize,
) -> Result<Option<Vec<f64>>> {
    if global.len() != owned.len() || values.len() != global.len() * ncomp {
        return Err(Error::invalid(
            "gather_owned_by_global_id: inconsistent lengths",
        ));
    }
    let mut ids = Vec::new();
    let mut vals = Vec::new();
    for (i, (&g, &o)) in global.iter().zip(owned).enumerate() {
        if o {
            ids.push(g);
            vals.extend_from_slice(&values[i * ncomp..(i + 1) * ncomp]);
        }
    }
    let all_ids = comm.gather_v(&ids, 0)?;
    let all_vals = comm.gather_v(&vals, 0)?;
    let (Some(all_ids), Some(all_vals)) = (all_ids, all_vals) else {
        return Ok(None);
    };
    let mut rows: Vec<(u64, &[f64])> = Vec::new();
    for (ids, vals) in all_ids.iter().zip(&all_vals) {
        for (k, &g) in ids.iter().enumerate() {
            rows.push((g, &vals[k * ncomp..(k + 1) * ncomp]));
        }
    }
    rows.sort_by_key(|r| r.0);
    Ok(Some(
        rows.into_iter().flat_map(|r| r.1.iter().copied()).collect(),
    ))
}

/// The build's commit (`git rev-parse --short=9 HEAD` at build time).
#[must_use]
pub fn commit() -> &'static str {
    option_env!("AKANTU_BENCH_COMMIT").unwrap_or("unknown")
}

/// The enabled crate features (`mpi`, `scotch`, `mumps`), comma-separated.
#[must_use]
pub fn features() -> String {
    let mut f = Vec::new();
    if cfg!(feature = "mpi") {
        f.push("mpi");
    }
    if cfg!(feature = "scotch") {
        f.push("scotch");
    }
    if cfg!(feature = "mumps") {
        f.push("mumps");
    }
    f.join(",")
}

fn json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_real(out: &mut String, x: f64) {
    if x.is_finite() {
        let _ = write!(out, "{x:?}");
    } else {
        out.push_str("null");
    }
}

/// One output line (JSON object, no newline).
#[must_use]
#[allow(clippy::too_many_arguments)] // one argument per field of the contract's line
pub fn json_line(
    suite: &str,
    case: &str,
    size: Size,
    rep: usize,
    rec: &PhaseRecord,
    counts: [u64; 3],
    ranks: usize,
    (threads, grain): (usize, Grain),
    digest: Option<u64>,
    extras: &[(String, Extra)],
) -> String {
    let mut s = String::with_capacity(320);
    s.push_str("{\"suite\":");
    json_str(&mut s, suite);
    s.push_str(",\"case\":");
    json_str(&mut s, case);
    let _ = write!(
        s,
        ",\"size\":\"{}\",\"rep\":{rep},\"phase\":",
        size.as_str()
    );
    json_str(&mut s, &rec.phase);
    let _ = write!(s, ",\"calls\":{},\"seconds\":", rec.calls);
    json_real(&mut s, rec.seconds);
    let _ = write!(
        s,
        ",\"n_elements\":{},\"n_nodes\":{},\"n_dofs\":{},\"ranks\":{ranks},\"threads\":{threads},\
         \"grain\":\"{}\",\"precision\":\"{}\",\"digest\":",
        counts[0],
        counts[1],
        counts[2],
        grain_name(grain),
        precision_name()
    );
    match digest {
        Some(d) => {
            let _ = write!(s, "\"{d:016x}\"");
        }
        None => s.push_str("null"),
    }
    s.push_str(",\"commit\":");
    json_str(&mut s, commit());
    s.push_str(",\"features\":");
    json_str(&mut s, &features());
    s.push_str(",\"ordering\":");
    json_str(&mut s, &ordering_name());
    s.push_str(",\"profile\":");
    json_str(
        &mut s,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
    );
    for (k, v) in extras {
        s.push(',');
        json_str(&mut s, k);
        s.push(':');
        match v {
            Extra::Int(i) => {
                let _ = write!(s, "{i}");
            }
            Extra::Real(x) => json_real(&mut s, *x),
            Extra::Text(t) => json_str(&mut s, t),
        }
    }
    s.push('}');
    s
}

/// Result of one repetition of one case, after the reduction over ranks.
#[derive(Clone, Debug)]
pub struct RepResult {
    /// Outcome.
    pub outcome: Outcome,
    /// Phases (largest time over the ranks).
    pub phases: Vec<PhaseRecord>,
    /// Global counts.
    pub counts: [u64; 3],
    /// Rank 0's digest.
    pub digest: Option<u64>,
    /// Rank 0's extras.
    pub extras: Vec<(String, Extra)>,
}

/// Run one repetition of `case` (collective) and reduce its records.
///
/// # Errors
/// The case's error; phases that differ between ranks.
pub fn run_case(case: &Case, opts: &Options, comm: &Communicator, rep: usize) -> Result<RepResult> {
    let mut ctx = Ctx::new(opts, comm, rep);
    let outcome = (case.run)(&mut ctx)?;
    let mut n = [ctx.phases.len() as u64];
    let mut n_max = n;
    comm.all_reduce(&mut n, ReduceOp::Min)?;
    comm.all_reduce(&mut n_max, ReduceOp::Max)?;
    if n != n_max {
        return Err(Error::invalid(format!(
            "case {}: the ranks recorded different phase lists",
            case.name
        )));
    }
    let mut secs: Vec<f64> = ctx.phases.iter().map(|p| p.seconds).collect();
    comm.all_reduce(&mut secs, ReduceOp::Max)?;
    let mut counts = ctx.counts;
    comm.all_reduce(&mut counts, ReduceOp::Sum)?;
    let phases = ctx
        .phases
        .iter()
        .zip(secs)
        .map(|(p, s)| PhaseRecord {
            phase: p.phase.clone(),
            calls: p.calls,
            seconds: s,
        })
        .collect();
    Ok(RepResult {
        outcome,
        phases,
        counts,
        digest: ctx.digest,
        extras: ctx.extras,
    })
}

/// The main function of a suite binary: parse the command line, list or
/// run the cases, write the lines. Returns the process exit status.
pub fn run_main(suite: &str, cases: &[Case], comm: &Communicator) -> std::process::ExitCode {
    use std::process::ExitCode;
    let bin = format!("suite_{suite}");
    let cmd = match parse_args(&bin, std::env::args().skip(1)) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{bin}: {e}");
            return ExitCode::from(2);
        }
    };
    let root = comm.rank() == 0;
    let opts = match cmd {
        Command::List => {
            if root {
                for c in cases {
                    println!("{:<34} {}", c.name, c.description);
                }
            }
            return ExitCode::SUCCESS;
        }
        Command::Run(o) => o,
    };
    if let Some(o) = opts.ordering {
        set_sparse_ordering(o);
    }
    if let Some(p) = &opts.dump_state
        && let Err(e) = set_state_dump(p)
    {
        eprintln!("{bin}: {e}");
        return ExitCode::from(2);
    }
    // before any model is created: the policy of every model of the run
    let exec = match apply_exec_options(opts.threads, opts.grain) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{bin}: {e}");
            return ExitCode::from(2);
        }
    };
    let selected: Vec<&Case> = if opts.cases.iter().any(|c| c == "all") {
        cases.iter().collect()
    } else {
        let mut v = Vec::new();
        for name in &opts.cases {
            match cases.iter().find(|c| c.name == name) {
                Some(c) => v.push(c),
                None => {
                    eprintln!("{bin}: unknown case `{name}` (see --list)");
                    return ExitCode::from(2);
                }
            }
        }
        v
    };
    let mut sink: Box<dyn std::io::Write> = match (&opts.out, root) {
        (_, false) => Box::new(std::io::sink()),
        (Some(p), true) => match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            Ok(f) => Box::new(std::io::BufWriter::new(f)),
            Err(e) => {
                eprintln!("{bin}: {}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        },
        (None, true) => Box::new(std::io::stdout()),
    };
    for case in selected {
        for rep in 0..opts.reps {
            let r = match run_case(case, &opts, comm, rep) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{bin}: case {} (rank {}): {e}", case.name, comm.rank());
                    return ExitCode::FAILURE;
                }
            };
            if let Outcome::Skip(reason) = &r.outcome {
                if root {
                    eprintln!("skip: {}: {reason}", case.name);
                }
                break;
            }
            if !root {
                continue;
            }
            for p in &r.phases {
                let line = json_line(
                    suite,
                    case.name,
                    opts.size,
                    rep,
                    p,
                    r.counts,
                    comm.size(),
                    exec,
                    r.digest,
                    &r.extras,
                );
                if writeln!(sink, "{line}").is_err() {
                    eprintln!("{bin}: cannot write the output");
                    return ExitCode::FAILURE;
                }
            }
            let _ = sink.flush();
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn parses_the_contract_command_line() {
        let Command::Run(o) = parse_args(
            "suite_x",
            args(&[
                "--case",
                "a,b",
                "--size",
                "m",
                "--reps",
                "5",
                "--out",
                "f.jsonl",
                "--loop-seconds",
                "2.5",
                "--threads",
                "4",
            ]),
        )
        .unwrap() else {
            panic!("not a run");
        };
        assert_eq!(o.cases, ["a", "b"]);
        assert_eq!(o.size, Size::M);
        assert_eq!(o.reps, 5);
        assert_eq!(o.loop_seconds, Some(2.5));
        assert_eq!(o.threads, Some(4));
        assert_eq!(o.grain, None);
        let Command::Run(o) = parse_args(
            "suite_x",
            args(&["--case", "a", "--size", "s", "--grain", "finest"]),
        )
        .unwrap() else {
            panic!("not a run");
        };
        assert_eq!((o.threads, o.grain), (None, Some(Grain::Finest)));
        assert!(
            parse_args(
                "suite_x",
                args(&["--case", "a", "--size", "s", "--threads", "0"])
            )
            .is_err()
        );
        assert!(
            parse_args(
                "suite_x",
                args(&["--case", "a", "--size", "s", "--grain", "x"])
            )
            .is_err()
        );
        assert!(matches!(
            parse_args("suite_x", args(&["--list"])).unwrap(),
            Command::List
        ));
        assert!(parse_args("suite_x", args(&["--case", "a"])).is_err());
        assert!(parse_args("suite_x", args(&["--size", "x", "--case", "a"])).is_err());
        assert!(parse_args("suite_x", args(&["--bogus"])).is_err());
    }

    #[test]
    fn fnv1a_reference_values() {
        // FNV-1a 64 test vectors: "" and "a"
        assert_eq!(Digest::new().value(), 0xcbf2_9ce4_8422_2325);
        assert_eq!(Digest::new().bytes(b"a").value(), 0xaf63_dc4c_8601_ec8c);
        let mut d = Digest::new();
        d.reals(&[1.0, -0.0]);
        let mut e = Digest::new();
        e.bytes(&1.0f64.to_bits().to_le_bytes())
            .bytes(&(-0.0f64).to_bits().to_le_bytes());
        assert_eq!(d, e);
    }

    #[test]
    fn json_line_shape() {
        let rec = PhaseRecord {
            phase: "solve_step".into(),
            calls: 3,
            seconds: 0.5,
        };
        let l = json_line(
            "solid",
            "c\"x",
            Size::S,
            1,
            &rec,
            [1, 2, 3],
            1,
            (3, Grain::Finest),
            Some(0xab),
            &[("iterations".into(), Extra::Int(4))],
        );
        assert!(
            l.starts_with("{\"suite\":\"solid\",\"case\":\"c\\\"x\",\"size\":\"s\",\"rep\":1,")
        );
        assert!(l.contains("\"phase\":\"solve_step\",\"calls\":3,\"seconds\":0.5,"));
        assert!(l.contains("\"digest\":\"00000000000000ab\""));
        assert!(l.contains("\"threads\":3,\"grain\":\"finest\","));
        assert!(l.ends_with(",\"iterations\":4}"));
    }

    #[test]
    fn repeat_warms_up_and_counts() {
        let opts = Options {
            cases: vec![],
            size: Size::S,
            reps: 1,
            out: None,
            loop_seconds: None,
            threads: None,
            grain: None,
            ordering: None,
            dump_state: None,
        };
        let comm = Communicator::serial();
        let mut ctx = Ctx::new(&opts, &comm, 0);
        let mut calls = (0, 0);
        let n = ctx
            .repeat_primary("p", 4, |timed| {
                if timed {
                    calls.1 += 1;
                } else {
                    calls.0 += 1;
                }
                Ok(())
            })
            .unwrap();
        assert_eq!(n, 4);
        assert_eq!(calls, (WARMUP_CALLS, 4));
        assert_eq!(ctx.phases()[0].calls, 4);
    }

    #[test]
    fn gather_orders_by_global_id() {
        let comm = Communicator::serial();
        let v = gather_owned_by_global_id(
            &comm,
            &[5, 1, 3],
            &[true, true, false],
            &[50., 51., 10., 11., 30., 31.],
            2,
        )
        .unwrap()
        .unwrap();
        assert_eq!(v, [10., 11., 50., 51.]);
    }
}
