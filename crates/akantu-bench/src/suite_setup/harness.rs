// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Benchmark harness of
// the set-up suite (no upstream counterpart).

//! Private harness of `suite_setup` (the contract of
//! `docs/briefs/perf/SUITE.md`): command line, case registry, phase timers,
//! FNV-1a digest and one JSON object per line and per (case, repetition,
//! phase). Kept small: the shared `suite` helper replaces it at merge time.
//!
//! Distributed runs: every rank runs the case; rank 0 prints. A phase's
//! `seconds` is the maximum over the ranks (`seconds_min` and
//! `seconds_rank0` are extra fields), and the digest is the FNV-1a of the
//! per-rank digests in rank order unless the case combines owned data in
//! global-id order itself.

use std::fmt::Write as _;
use std::io::Write as _;
use std::time::Instant;

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::{Error, Result};

/// The FNV-1a 64-bit offset basis.
pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a (64-bit) over a stream of bytes.
#[derive(Clone, Copy, Debug)]
pub struct Digest(u64);

impl Default for Digest {
    fn default() -> Self {
        Self(FNV_OFFSET)
    }
}

impl Digest {
    /// A fresh digest.
    pub fn new() -> Self {
        Self::default()
    }
    /// Feed bytes.
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        for &x in b {
            self.0 ^= u64::from(x);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
        self
    }
    /// Feed `f64` values (little-endian bit patterns).
    pub fn reals(&mut self, v: &[f64]) -> &mut Self {
        for x in v {
            self.bytes(&x.to_le_bytes());
        }
        self
    }
    /// Feed `i32` values (little-endian).
    pub fn ints(&mut self, v: &[i32]) -> &mut Self {
        for x in v {
            self.bytes(&x.to_le_bytes());
        }
        self
    }
    /// Feed one `u64` (little-endian).
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    /// The digest value.
    pub fn value(&self) -> u64 {
        self.0
    }
}

/// Problem size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// Whole case under about 1 s (tests).
    S,
    /// Timing size.
    M,
    /// Scaling size (about 8× `m`).
    L,
}

impl Size {
    /// The size's letter.
    pub fn letter(self) -> &'static str {
        match self {
            Size::S => "s",
            Size::M => "m",
            Size::L => "l",
        }
    }
    /// Pick a value per size.
    pub fn pick<T>(self, s: T, m: T, l: T) -> T {
        match self {
            Size::S => s,
            Size::M => m,
            Size::L => l,
        }
    }
}

/// One timed phase of a run.
#[derive(Clone, Debug)]
pub struct Phase {
    /// Stable identifier.
    pub name: &'static str,
    /// Number of timed calls (1 for a set-up phase).
    pub calls: usize,
    /// Wall time of the calls on this rank.
    pub seconds: f64,
}

/// What a case reports for one repetition.
#[derive(Clone, Debug, Default)]
pub struct CaseOut {
    /// The phases in execution order.
    pub phases: Vec<Phase>,
    /// Elements (global, of the case's main mesh).
    pub n_elements: usize,
    /// Nodes (global).
    pub n_nodes: usize,
    /// Degrees of freedom (global; 0 when the case has none).
    pub n_dofs: usize,
    /// Digest of the results on this rank (or of the whole run when
    /// `digest_global` is set).
    pub digest: u64,
    /// The digest already combines every rank's data (identical on every
    /// rank); otherwise the harness combines the per-rank digests in rank
    /// order.
    pub digest_global: bool,
    /// Extra numeric fields printed with every line (name, value).
    pub extra: Vec<(&'static str, f64)>,
}

impl CaseOut {
    /// Time a set-up phase (called once).
    pub fn phase<T>(&mut self, name: &'static str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let t = Instant::now();
        let r = f()?;
        self.phases.push(Phase {
            name,
            calls: 1,
            seconds: t.elapsed().as_secs_f64(),
        });
        Ok(r)
    }

    /// Time a loop phase: three untimed warm-up calls, then `calls` calls
    /// (or, with `--loop-seconds S`, as many as fit in `S` seconds, at
    /// least `calls`), the clock around the whole loop.
    pub fn loop_phase(
        &mut self,
        ctx: &Ctx,
        name: &'static str,
        calls: usize,
        mut f: impl FnMut() -> Result<()>,
    ) -> Result<()> {
        for _ in 0..3 {
            f()?;
        }
        let (n, seconds) = if let Some(limit) = ctx.loop_seconds {
            // a collective decision per batch, so that every rank runs the
            // same number of calls (exchange loops)
            let t = Instant::now();
            let mut n = 0;
            loop {
                for _ in 0..calls.max(1) {
                    f()?;
                }
                n += calls.max(1);
                let mut go = [u8::from(t.elapsed().as_secs_f64() < limit)];
                ctx.comm.all_reduce(&mut go, ReduceOp::Min)?;
                if go[0] == 0 {
                    break;
                }
            }
            (n, t.elapsed().as_secs_f64())
        } else {
            let t = Instant::now();
            for _ in 0..calls {
                f()?;
            }
            (calls, t.elapsed().as_secs_f64())
        };
        self.phases.push(Phase {
            name,
            calls: n,
            seconds,
        });
        Ok(())
    }
}

/// Context of a run.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// The communicator (serial, local world or MPI).
    pub comm: Communicator,
    /// Problem size.
    pub size: Size,
    /// `--threads` (recorded only).
    pub threads: usize,
    /// `--loop-seconds`.
    pub loop_seconds: Option<f64>,
}

impl Ctx {
    /// Rank of this process.
    pub fn rank(&self) -> usize {
        usize::try_from(self.comm.rank()).unwrap_or(0)
    }
    /// Number of ranks.
    pub fn ranks(&self) -> usize {
        self.comm.size()
    }
}

/// Whether a case runs at any rank count or only on one rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One rank only (`skip` at P > 1).
    Serial,
    /// Any rank count (the MPI cases).
    Parallel,
}

/// A registered case.
#[derive(Clone, Copy, Debug)]
pub struct Case {
    /// Stable name.
    pub name: &'static str,
    /// One line: which cost the case exposes.
    pub description: &'static str,
    /// Serial or parallel.
    pub kind: Kind,
    /// `Some(reason)` when the binary lacks a needed feature.
    pub unavailable: Option<&'static str>,
    /// The case body.
    pub run: fn(&Ctx) -> Result<CaseOut>,
}

/// Parsed command line.
#[derive(Clone, Debug)]
pub struct Cli {
    /// `--list`.
    pub list: bool,
    /// `--case` names (or `all`).
    pub cases: Vec<String>,
    /// `--size`.
    pub size: Size,
    /// `--reps`.
    pub reps: usize,
    /// `--out`.
    pub out: Option<String>,
    /// `--loop-seconds`.
    pub loop_seconds: Option<f64>,
    /// `--threads`.
    pub threads: usize,
    /// `--local-ranks N`: run N in-process ranks (the local world of
    /// `akantu-comm`) instead of the serial or MPI communicator.
    pub local_ranks: Option<usize>,
}

const USAGE: &str = "usage: suite_setup --list\n       suite_setup --case <name>[,<name>...]|all --size s|m|l [--reps N] [--out FILE] \
                     [--loop-seconds S] [--threads N] [--local-ranks N]";

/// Parse the command line.
pub fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<Cli> {
    let mut cli = Cli {
        list: false,
        cases: Vec::new(),
        size: Size::M,
        reps: 1,
        out: None,
        loop_seconds: None,
        threads: 1,
        local_ranks: None,
    };
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let (key, inline) = match a.split_once('=') {
            Some((k, v)) if k.starts_with("--") => (k.to_owned(), Some(v.to_owned())),
            _ => (a.clone(), None),
        };
        let mut value = || -> Result<String> {
            inline
                .clone()
                .or_else(|| it.next())
                .ok_or_else(|| Error::invalid(format!("{key} needs a value")))
        };
        let bad = |k: &str, e: &dyn std::fmt::Display| Error::invalid(format!("{k}: {e}"));
        match key.as_str() {
            "--list" => cli.list = true,
            "--case" => {
                cli.cases = value()?.split(',').map(str::to_owned).collect();
            }
            "--size" => {
                cli.size = match value()?.as_str() {
                    "s" => Size::S,
                    "m" => Size::M,
                    "l" => Size::L,
                    v => return Err(Error::invalid(format!("--size: unknown size {v}"))),
                };
            }
            "--reps" => cli.reps = value()?.parse().map_err(|e| bad("--reps", &e))?,
            "--out" => cli.out = Some(value()?),
            "--loop-seconds" => {
                cli.loop_seconds = Some(value()?.parse().map_err(|e| bad("--loop-seconds", &e))?);
            }
            "--threads" => cli.threads = value()?.parse().map_err(|e| bad("--threads", &e))?,
            "--local-ranks" => {
                cli.local_ranks = Some(value()?.parse().map_err(|e| bad("--local-ranks", &e))?);
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            _ => return Err(Error::invalid(format!("unknown argument {a}\n{USAGE}"))),
        }
    }
    if !cli.list && cli.cases.is_empty() {
        return Err(Error::invalid(format!(
            "--case or --list is required\n{USAGE}"
        )));
    }
    if cli.reps == 0 {
        return Err(Error::invalid("--reps must be at least 1"));
    }
    Ok(cli)
}

/// The commit recorded in the output: `AKANTU_SUITE_COMMIT`, else
/// `git rev-parse --short=9 HEAD` in the working directory, else
/// `unknown`.
pub fn commit() -> String {
    if let Ok(c) = std::env::var("AKANTU_SUITE_COMMIT") {
        return c;
    }
    std::process::Command::new("git")
        .args(["rev-parse", "--short=9", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// The Cargo features the binary was built with.
pub fn features() -> String {
    let mut f: Vec<&str> = Vec::new();
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

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn json_f64(v: f64) -> String {
    if v.is_finite() {
        format!("{v:?}")
    } else {
        "null".into()
    }
}

/// Run the selected cases on `comm` and print (rank 0) the JSON lines.
/// Returns an error on the first failing case.
pub fn run_cases(cli: &Cli, cases: &[Case], comm: &Communicator) -> Result<()> {
    let rank0 = comm.rank() == 0;
    if cli.list {
        if rank0 {
            for c in cases {
                let kind = match c.kind {
                    Kind::Serial => "",
                    Kind::Parallel => " [mpi]",
                };
                println!("{:<26} {}{kind}", c.name, c.description);
            }
        }
        return Ok(());
    }
    let selected: Vec<&Case> = if cli.cases.iter().any(|c| c == "all") {
        cases.iter().collect()
    } else {
        cli.cases
            .iter()
            .map(|n| {
                cases
                    .iter()
                    .find(|c| c.name == n)
                    .ok_or_else(|| Error::invalid(format!("unknown case {n} (see --list)")))
            })
            .collect::<Result<_>>()?
    };
    let all = cli.cases.iter().any(|c| c == "all");
    let mut sink: Box<dyn std::io::Write> = match (&cli.out, rank0) {
        (Some(path), true) => Box::new(std::io::BufWriter::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|source| Error::Io {
                    path: path.clone(),
                    source,
                })?,
        )),
        _ => Box::new(std::io::stdout()),
    };
    let commit = commit();
    let features = features();
    let ctx = Ctx {
        comm: comm.clone(),
        size: cli.size,
        threads: cli.threads,
        loop_seconds: cli.loop_seconds,
    };
    for case in selected {
        let skip = match (case.unavailable, case.kind) {
            (Some(reason), _) => Some(reason.to_owned()),
            (None, Kind::Serial) if comm.size() > 1 => {
                Some("a serial case (run it on one rank)".to_owned())
            }
            _ => None,
        };
        if let Some(reason) = skip {
            if rank0 {
                eprintln!("{}: skip: {reason}", case.name);
            }
            if all {
                continue;
            }
            return Err(Error::invalid(format!("{}: skip: {reason}", case.name)));
        }
        // with --loop-seconds, a case whose phases are all set-up phases is
        // repeated until the time is spent (collective decision), so that
        // `perf record` sees its set-up work
        let start = Instant::now();
        let mut rep = 0;
        loop {
            let out =
                (case.run)(&ctx).map_err(|e| Error::invalid(format!("{}: {e}", case.name)))?;
            emit(
                &ctx,
                case.name,
                rep,
                &out,
                &commit,
                &features,
                sink.as_mut(),
            )?;
            rep += 1;
            let more = match cli.loop_seconds {
                Some(s) => {
                    let mut go = [u8::from(start.elapsed().as_secs_f64() < s)];
                    comm.all_reduce(&mut go, ReduceOp::Min)?;
                    go[0] == 1
                }
                None => rep < cli.reps,
            };
            if !more {
                break;
            }
        }
    }
    sink.flush().map_err(|source| Error::Io {
        path: cli.out.clone().unwrap_or_else(|| "stdout".into()),
        source,
    })?;
    Ok(())
}

fn emit(
    ctx: &Ctx,
    case: &str,
    rep: usize,
    out: &CaseOut,
    commit: &str,
    features: &str,
    sink: &mut dyn std::io::Write,
) -> Result<()> {
    let comm = &ctx.comm;
    // phase lists must agree across ranks (same case code on every rank)
    let n = out.phases.len();
    let mut times: Vec<f64> = out.phases.iter().map(|p| p.seconds).collect();
    let mut tmax = times.clone();
    let mut tmin = times.clone();
    comm.all_reduce(&mut tmax, ReduceOp::Max)?;
    comm.all_reduce(&mut tmin, ReduceOp::Min)?;
    let digest = if out.digest_global || comm.size() == 1 {
        out.digest
    } else {
        let all = comm.all_gather(&[out.digest])?;
        let mut d = Digest::new();
        for v in all {
            d.u64(v);
        }
        d.value()
    };
    if comm.rank() != 0 {
        return Ok(());
    }
    for (k, p) in out.phases.iter().enumerate().take(n) {
        let mut line = String::new();
        let _ = write!(
            line,
            "{{\"suite\":\"setup\",\"case\":{},\"size\":\"{}\",\"rep\":{rep},\"phase\":{},\"calls\":{},\
             \"seconds\":{},\"seconds_min\":{},\"seconds_rank0\":{},\
             \"n_elements\":{},\"n_nodes\":{},\"n_dofs\":{},\"ranks\":{},\"threads\":{},\
             \"digest\":\"{digest:016x}\",\"commit\":{},\"features\":{}",
            json_str(case),
            ctx.size.letter(),
            json_str(p.name),
            p.calls,
            json_f64(tmax[k]),
            json_f64(tmin[k]),
            json_f64(times[k]),
            out.n_elements,
            out.n_nodes,
            out.n_dofs,
            comm.size(),
            ctx.threads,
            json_str(commit),
            json_str(features),
        );
        for (name, v) in &out.extra {
            let _ = write!(line, ",{}:{}", json_str(name), json_f64(*v));
        }
        line.push('}');
        writeln!(sink, "{line}").map_err(|source| Error::Io {
            path: "output".into(),
            source,
        })?;
    }
    times.clear();
    Ok(())
}
