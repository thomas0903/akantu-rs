// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Benchmark harness of
// the model suite (no upstream counterpart).

//! Private harness of `suite_models` (the contract of the crate's README,
//! section "Benchmark suite"): command line, phase timers, FNV-1a digest,
//! one JSON object per (case, repetition, phase). Kept small: a later task
//! moves the cases onto the shared harness (`akantu_bench::suite`).

use std::fmt::Write as _;
use std::io::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::{Error, Result};

/// Problem size of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Size {
    /// The whole case in about a second (tests).
    S,
    /// The timing size.
    M,
    /// About 8× `m` (scaling studies).
    L,
}

impl Size {
    /// `s`, `m` or `l`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::S => "s",
            Self::M => "m",
            Self::L => "l",
        }
    }

    /// Value of `(s, m, l)` for this size.
    #[must_use]
    pub fn pick<T>(self, s: T, m: T, l: T) -> T {
        match self {
            Self::S => s,
            Self::M => m,
            Self::L => l,
        }
    }
}

/// Command line of the suite binaries.
#[derive(Clone, Debug)]
pub struct Opts {
    /// `--list`.
    pub list: bool,
    /// `--case a,b` or `all` (empty: none given).
    pub cases: Vec<String>,
    /// `--size`.
    pub size: Size,
    /// `--reps`.
    pub reps: usize,
    /// `--out FILE` (JSON lines appended; stdout always gets them too).
    pub out: Option<PathBuf>,
    /// `--loop-seconds S`: repeat each loop phase for at least `S` seconds.
    pub loop_seconds: Option<f64>,
    /// `--threads N` (recorded only).
    pub threads: usize,
}

impl Opts {
    /// Parse `args` (without the program name).
    ///
    /// # Errors
    /// Unknown option, missing or malformed value.
    pub fn parse(args: &[String]) -> Result<Self> {
        let mut o = Self {
            list: false,
            cases: Vec::new(),
            size: Size::S,
            reps: 1,
            out: None,
            loop_seconds: None,
            threads: 1,
        };
        let mut it = args.iter();
        let bad = |what: &str| Error::invalid(format!("suite_models: {what}"));
        while let Some(a) = it.next() {
            let mut value = || {
                it.next()
                    .cloned()
                    .ok_or_else(|| bad(&format!("{a} needs a value")))
            };
            match a.as_str() {
                "--list" => o.list = true,
                "--case" => o.cases = value()?.split(',').map(str::to_owned).collect(),
                "--size" => {
                    o.size = match value()?.as_str() {
                        "s" => Size::S,
                        "m" => Size::M,
                        "l" => Size::L,
                        v => return Err(bad(&format!("size {v} (s, m or l)"))),
                    }
                }
                "--reps" => o.reps = value()?.parse().map_err(|_| bad("--reps N"))?,
                "--out" => o.out = Some(PathBuf::from(value()?)),
                "--loop-seconds" => {
                    o.loop_seconds = Some(value()?.parse().map_err(|_| bad("--loop-seconds S"))?);
                }
                "--threads" => o.threads = value()?.parse().map_err(|_| bad("--threads N"))?,
                "-h" | "--help" => o.list = true,
                v => return Err(bad(&format!("unknown argument {v}"))),
            }
        }
        Ok(o)
    }
}

/// One case of the suite.
pub struct Case {
    /// Stable identifier.
    pub name: &'static str,
    /// One line: what the case exposes.
    pub description: &'static str,
    /// The case: builds everything from scratch, records its phases and
    /// returns its digest.
    pub run: fn(&mut Run) -> Result<Digest>,
}

/// Sizes of the problem of a run (for the JSON lines).
#[derive(Clone, Copy, Debug, Default)]
pub struct Counts {
    /// Elements (global, all types the case counts).
    pub elements: usize,
    /// Nodes (global).
    pub nodes: usize,
    /// Degrees of freedom (global).
    pub dofs: usize,
}

#[derive(Clone, Debug)]
struct Phase {
    name: String,
    calls: usize,
    seconds: f64,
}

/// One repetition of one case.
pub struct Run {
    /// Options of the binary.
    pub opts: Opts,
    /// The world communicator (serial without MPI).
    pub comm: Communicator,
    /// Problem sizes (set by the case).
    pub counts: Counts,
    /// Sparse direct solver of the case (`none` without a sparse solve;
    /// set by `common::eigen_options`).
    pub sparse_solver: &'static str,
    phases: Vec<Phase>,
}

impl Run {
    /// A fresh run.
    #[must_use]
    pub fn new(opts: Opts, comm: Communicator) -> Self {
        Self {
            opts,
            comm,
            counts: Counts::default(),
            sparse_solver: "none",
            phases: Vec::new(),
        }
    }

    /// Size of the run.
    #[must_use]
    pub fn size(&self) -> Size {
        self.opts.size
    }

    /// Time a set-up phase executed once.
    ///
    /// # Errors
    /// The error of `f`.
    pub fn once<T>(&mut self, name: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let t0 = Instant::now();
        let r = f()?;
        self.push(name, 1, t0.elapsed().as_secs_f64());
        Ok(r)
    }

    /// Time a loop phase: three warm-up calls `f(0..3)` (not timed), then
    /// `calls` timed calls `f(3..3 + calls)` (with `--loop-seconds S`:
    /// repeated until `S` seconds have passed). The argument is the running
    /// index of the call.
    ///
    /// # Errors
    /// The error of `f`.
    pub fn looped(
        &mut self,
        name: &str,
        calls: usize,
        mut f: impl FnMut(usize) -> Result<()>,
    ) -> Result<()> {
        let tw = Instant::now();
        for i in 0..3 {
            f(i)?;
        }
        self.note(&format!(
            "{name}: warm-up {:.6} s per call",
            tw.elapsed().as_secs_f64() / 3.0
        ));
        let t0 = Instant::now();
        let mut done = 0;
        loop {
            for _ in 0..calls {
                f(3 + done)?;
                done += 1;
            }
            if calls == 0 || !self.keep_looping(t0)? {
                break;
            }
        }
        self.push(name, done, t0.elapsed().as_secs_f64());
        Ok(())
    }

    /// With `--loop-seconds S`: whether a loop started at `t0` continues
    /// (fewer than `S` seconds have passed). Rank 0 decides and broadcasts,
    /// so that every rank makes the same number of collective calls (as
    /// `akantu_bench::suite`). Without the option: `false`, no
    /// communication.
    ///
    /// # Errors
    /// Communication errors.
    pub fn keep_looping(&self, t0: Instant) -> Result<bool> {
        let Some(target) = self.opts.loop_seconds else {
            return Ok(false);
        };
        let mut more = [u8::from(t0.elapsed().as_secs_f64() < target)];
        self.comm.broadcast(&mut more, 0)?;
        Ok(more[0] != 0)
    }

    /// Replace every phase time by its maximum over the ranks (the slowest
    /// rank, as `akantu_bench::suite`). Collective; the ranks must have
    /// recorded the same phase list.
    ///
    /// # Errors
    /// Phase lists that differ between ranks, communication errors.
    pub fn reduce_over_ranks(&mut self) -> Result<()> {
        if self.comm.size() == 1 {
            return Ok(());
        }
        let mut n = [self.phases.len() as u64];
        let mut n_max = n;
        self.comm.all_reduce(&mut n, ReduceOp::Min)?;
        self.comm.all_reduce(&mut n_max, ReduceOp::Max)?;
        if n != n_max {
            return Err(Error::invalid(
                "the ranks recorded different phase lists".to_owned(),
            ));
        }
        let mut secs: Vec<f64> = self.phases.iter().map(|p| p.seconds).collect();
        self.comm.all_reduce(&mut secs, ReduceOp::Max)?;
        for (p, s) in self.phases.iter_mut().zip(secs) {
            p.seconds = s;
        }
        Ok(())
    }

    /// Record a loop phase timed by the case itself (pieces of one loop
    /// body timed separately): `calls` calls in `seconds`.
    pub fn record(&mut self, name: &str, calls: usize, seconds: f64) {
        self.push(name, calls, seconds);
    }

    fn push(&mut self, name: &str, calls: usize, seconds: f64) {
        self.note(&format!("{name}: {calls} calls, {seconds:.6} s"));
        if let Some(p) = self.phases.iter_mut().find(|p| p.name == name) {
            p.calls += calls;
            p.seconds += seconds;
        } else {
            self.phases.push(Phase {
                name: name.to_owned(),
                calls,
                seconds,
            });
        }
    }

    /// A diagnostic line on stderr (rank 0) when `SUITE_VERBOSE` is set:
    /// counts that show the case does what it says (contact elements,
    /// Newton iterations, inserted cohesive elements).
    pub fn note(&self, msg: &str) {
        if self.comm.rank() == 0 && std::env::var_os("SUITE_VERBOSE").is_some() {
            eprintln!("note: {msg}");
        }
    }

    /// JSON lines of the run (one per phase).
    #[must_use]
    pub fn json_lines(&self, case: &str, rep: usize, digest: &Digest) -> Vec<String> {
        let mut out = Vec::new();
        for p in &self.phases {
            let mut s = String::new();
            let _ = write!(
                s,
                "{{\"suite\":\"models\",\"case\":\"{case}\",\"size\":\"{}\",\"rep\":{rep},\
                 \"phase\":\"{}\",\"calls\":{},\"seconds\":{:.6},\"n_elements\":{},\
                 \"n_nodes\":{},\"n_dofs\":{},\"ranks\":{},\"threads\":{},\
                 \"digest\":\"{}\",\"commit\":\"{}\",\"features\":\"{}\",\
                 \"sparse_solver\":\"{}\"}}",
                self.opts.size.name(),
                p.name,
                p.calls,
                p.seconds,
                self.counts.elements,
                self.counts.nodes,
                self.counts.dofs,
                self.comm.size(),
                self.opts.threads,
                digest.hex(),
                commit(),
                features(),
                self.sparse_solver,
            );
            out.push(s);
        }
        out
    }
}

/// FNV-1a, 64 bits, over the bytes of the result arrays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Digest(u64);

impl Default for Digest {
    fn default() -> Self {
        Self::new()
    }
}

impl Digest {
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    /// Empty digest (the FNV offset basis).
    #[must_use]
    pub fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    /// Absorb raw bytes.
    pub fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= u64::from(x);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    /// Absorb reals (little-endian bit patterns).
    pub fn reals(&mut self, v: &[f64]) -> &mut Self {
        for x in v {
            self.bytes(&x.to_le_bytes());
        }
        self
    }

    /// Absorb unsigned integers (as `u64`, little-endian).
    pub fn ints(&mut self, v: impl IntoIterator<Item = u64>) -> &mut Self {
        for x in v {
            self.bytes(&x.to_le_bytes());
        }
        self
    }

    /// Hexadecimal form (16 digits).
    #[must_use]
    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Commit of the binary (recorded at build time, `akantu_bench::suite::commit`).
fn commit() -> &'static str {
    akantu_bench::suite::commit()
}

/// Cargo features the binary was built with.
#[must_use]
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

/// `main` of a suite binary over `cases`.
#[must_use]
pub fn main(cases: &[Case], comm: impl FnOnce() -> Result<Communicator>) -> std::process::ExitCode {
    use std::process::ExitCode;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opts = match Opts::parse(&args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    if opts.list || opts.cases.is_empty() {
        for c in cases {
            println!("{:<34} {}", c.name, c.description);
        }
        return ExitCode::SUCCESS;
    }
    let selected: Vec<&Case> = if opts.cases.iter().any(|c| c == "all") {
        cases.iter().collect()
    } else {
        let mut v = Vec::new();
        for name in &opts.cases {
            match cases.iter().find(|c| c.name == name) {
                Some(c) => v.push(c),
                None => {
                    eprintln!("suite_models: unknown case {name} (--list)");
                    return ExitCode::from(2);
                }
            }
        }
        v
    };
    let comm = match comm() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("suite_models: communicator: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut out = match &opts.out {
        Some(p) => match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
        {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("suite_models: {}: {e}", p.display());
                return ExitCode::FAILURE;
            }
        },
        None => None,
    };
    let root = comm.rank() == 0;
    let mut status = ExitCode::SUCCESS;
    for case in selected {
        for rep in 0..opts.reps {
            let mut run = Run::new(opts.clone(), comm.clone());
            match (case.run)(&mut run).and_then(|d| run.reduce_over_ranks().map(|()| d)) {
                Ok(d) => {
                    if root {
                        for l in run.json_lines(case.name, rep, &d) {
                            println!("{l}");
                            if let Some(f) = out.as_mut() {
                                let _ = writeln!(f, "{l}");
                            }
                        }
                    }
                }
                Err(Error::Unsupported(why)) => {
                    if root {
                        eprintln!("skip: {}: {why}", case.name);
                    }
                    break;
                }
                Err(e) => {
                    eprintln!("suite_models: case {} rep {rep}: {e}", case.name);
                    status = ExitCode::FAILURE;
                    break;
                }
            }
        }
    }
    status
}
