// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu's JOSS scalability test (EPFL/LSMS):
// performance-testing-cohesive @ 483bace8, cohesive_extrinsic.cc (`main`,
// `initialize("material.dat", argc, argv)`).

//! `cohesive_extrinsic`: the Rust driver of the cohesive scalability test.
//!
//! ```text
//! cohesive_extrinsic [--aka_input_file FILE] [--rust_model auto|cohesive|solid]
//!     [--rust_partitioner scotch|rcb|file:PATH] [--rust_sparse_solver auto|eigen|faer|mumps]
//!     [--rust_cohesive_compat upstream|corrected]
//!     [--rust_doubled_node_load compat|upstream|follows_elements] [--rust_no_static_solve]
//!     [--local-ranks N] [--quiet]
//! ```
//!
//! Upstream reads `material.dat` unless `--aka_input_file` is given; the
//! mesh path and the outputs are relative to the working directory. The
//! `--rust_*` options override the `rust_*` user-section parameters;
//! `--rust_no_static_solve` (`rust_static_solve = false`) skips the static
//! preload (timing mode for meshes whose static solve is out of reach of the
//! port's sparse solvers; not the upstream problem).
//! With the `mpi` feature the program runs on the MPI world (`mpirun -np N
//! cohesive_extrinsic …`); `--local-ranks N` runs N ranks as threads of
//! one process (akantu-rs local world, for tests without MPI).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use akantu_bench::config::{parse_compat, parse_doubled_node_load, parse_model, parse_partitioner};
use akantu_bench::{Config, run};
use akantu_comm::{Communicator, run_local};
use akantu_core::{Error, Result};

#[derive(Debug, Default)]
struct Cli {
    input: Option<PathBuf>,
    model: Option<String>,
    partitioner: Option<String>,
    sparse_solver: Option<String>,
    sparse_ordering: Option<String>,
    compat: Option<String>,
    doubled_node_load: Option<String>,
    no_static_solve: bool,
    local_ranks: Option<usize>,
    quiet: bool,
}

fn parse_cli() -> Result<Cli> {
    let mut cli = Cli::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let (key, inline) = match a.split_once('=') {
            Some((k, v)) => (k.to_owned(), Some(v.to_owned())),
            None => (a.clone(), None),
        };
        let mut value = || -> Result<String> {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| Error::invalid(format!("{key} needs a value")))
        };
        match key.as_str() {
            "--aka_input_file" => cli.input = Some(PathBuf::from(value()?)),
            "--rust_model" => cli.model = Some(value()?),
            "--rust_partitioner" => cli.partitioner = Some(value()?),
            "--rust_sparse_solver" => cli.sparse_solver = Some(value()?),
            "--rust_sparse_ordering" => cli.sparse_ordering = Some(value()?),
            "--rust_cohesive_compat" => cli.compat = Some(value()?),
            "--rust_doubled_node_load" => cli.doubled_node_load = Some(value()?),
            "--local-ranks" => {
                cli.local_ranks = Some(
                    value()?
                        .parse()
                        .map_err(|e| Error::invalid(format!("--local-ranks: {e}")))?,
                );
            }
            "--quiet" => cli.quiet = true,
            "--rust_no_static_solve" => cli.no_static_solve = true,
            "-h" | "--help" => {
                println!(
                    "usage: cohesive_extrinsic [--aka_input_file FILE] [--rust_model auto|cohesive|solid] \
                     [--rust_partitioner scotch|rcb|file:PATH] [--rust_sparse_solver auto|eigen|faer|mumps] \
                     [--rust_sparse_ordering colamd|nested_dissection|auto] \
                     [--rust_cohesive_compat upstream|corrected] \
                     [--rust_doubled_node_load compat|upstream|follows_elements] \
                     [--rust_no_static_solve] [--local-ranks N] [--quiet]"
                );
                std::process::exit(0);
            }
            _ => return Err(Error::invalid(format!("unknown argument {a}"))),
        }
    }
    Ok(cli)
}

fn config(cli: &Cli) -> Result<Config> {
    let cwd = std::env::current_dir().map_err(|source| Error::Io {
        path: ".".into(),
        source,
    })?;
    let input = cli
        .input
        .clone()
        .unwrap_or_else(|| PathBuf::from("material.dat"));
    let mut cfg = Config::from_file(&input, &cwd, &cwd)?;
    if let Some(v) = &cli.model {
        cfg.model = parse_model(v)?;
    }
    if let Some(v) = &cli.partitioner {
        cfg.partitioner = parse_partitioner(v, Path::new(&cwd))?;
    }
    if let Some(v) = &cli.sparse_solver {
        cfg.sparse_solver = Some(v.parse()?);
    }
    if let Some(v) = &cli.sparse_ordering {
        cfg.sparse_ordering = Some(v.parse()?);
    }
    if let Some(v) = &cli.compat {
        cfg.compat = parse_compat(v)?;
    }
    if let Some(v) = &cli.doubled_node_load {
        cfg.doubled_node_load = parse_doubled_node_load(v)?;
    }
    if cli.no_static_solve {
        cfg.static_solve = false;
    }
    cfg.verbose = !cli.quiet;
    Ok(cfg)
}

#[cfg(feature = "mpi")]
fn world() -> Result<Communicator> {
    Communicator::mpi_world(akantu_comm::MpiOptions::default())
}

#[cfg(not(feature = "mpi"))]
fn world() -> Result<Communicator> {
    Ok(Communicator::serial())
}

fn main() -> ExitCode {
    let cli = match parse_cli() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cohesive_extrinsic: {e}");
            return ExitCode::from(2);
        }
    };
    let cfg = match config(&cli) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cohesive_extrinsic: {e}");
            return ExitCode::FAILURE;
        }
    };
    let results: Vec<Result<()>> = match cli.local_ranks {
        Some(n) if n > 1 => run_local(n, Duration::from_secs(24 * 3600), |c| {
            run(&c, &cfg).map(|_| ())
        }),
        _ => vec![world().and_then(|c| run(&c, &cfg).map(|_| ()))],
    };
    let mut status = ExitCode::SUCCESS;
    for (r, res) in results.into_iter().enumerate() {
        if let Err(e) = res {
            eprintln!("cohesive_extrinsic (rank {r}): {e}");
            status = ExitCode::FAILURE;
        }
    }
    status
}
