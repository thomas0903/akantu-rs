// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Shared scaffolding of
// the Rust ports of examples/c++/**: what the upstream examples get from
// `akantu::initialize(input_file, argc, argv)` (src/common/aka_common.cc) and
// from their build rules (cmake/AkantuExampleMacros.cmake and
// cmake/AkantuSimulationMacros.cmake: run directory, FILES_TO_COPY,
// add_mesh outputs, DIRECTORIES_TO_CREATE).

//! Shared scaffolding of the examples.
//!
//! An upstream example runs in its build directory, where CMake copied its
//! input files (`FILES_TO_COPY`) and Gmsh generated its meshes
//! (`add_mesh`). A Rust example reads its input file from its source
//! directory (`crates/akantu/examples/<name>/`), its mesh from the frozen
//! Gmsh 4.13.1 meshes of the oracle (`fixtures/meshes/gmsh-4.13.1/
//! examples/c++/<upstream dir>/`, decision D-009) and writes its outputs
//! (`paraview/`, CSV files) below an output directory, the current
//! directory by default — exactly what the upstream binary writes in its
//! run directory.
//!
//! Command line (upstream `initialize` options plus run controls):
//!
//! * `--aka_input_file <file>`: input file (default: the example's copy);
//! * `--output-dir <dir>`: where outputs are written (default `.`);
//! * `--mesh-dir <dir>`: where meshes are read (default: frozen meshes);
//! * `--steps <n>`: stop after at most `n` steps (reduced runs);
//! * `--no-dump`: skip the ParaView dumps;
//! * `--upstream-compat`: reproduce the upstream defects of the example
//!   (decision D-003, category B; see each example);
//! * `--solver <eigen|mumps|...>`: pin the sparse direct solver of the
//!   implicit examples (default: upstream `_auto`);
//! * `--quiet`: no progress output.

#![allow(dead_code)] // shared by every example; each uses a subset

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use akantu::core::enums::{MeshIoType, SparseSolverType};
use akantu::core::{Error, Real, Result};
use akantu::mesh::Mesh;
use akantu::parser::Parser;
use akantu::parser::cli::{ArgumentParser, ArgumentSpec, ArgumentType, Nargs};
use akantu::parser::param::{ParamKind, ParamValue};

/// Root of the `akantu` facade crate.
const CRATE_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// Frozen Gmsh meshes of the upstream examples (D-009).
pub fn frozen_meshes() -> PathBuf {
    Path::new(CRATE_DIR).join("../../fixtures/meshes/gmsh-4.13.1/examples/c++")
}

/// Run configuration of an example.
#[derive(Clone, Debug)]
pub struct Example {
    /// Example name (cargo example name = upstream ctest name).
    pub name: &'static str,
    /// Directory of the example's sources and input files.
    pub source_dir: PathBuf,
    /// Directory holding the example's meshes.
    pub mesh_dir: PathBuf,
    /// Input file (upstream `initialize("<file>", argc, argv)`).
    pub input_file: PathBuf,
    /// Directory receiving every output (upstream: the run directory).
    pub output_dir: PathBuf,
    /// Upper bound on the number of steps (`None`: the upstream count).
    pub steps: Option<usize>,
    /// Write the ParaView dumps.
    pub dump: bool,
    /// Reproduce the upstream defects of the example (D-003 B).
    pub upstream_compat: bool,
    /// Print progress messages (upstream standard output).
    pub verbose: bool,
    /// Sparse direct solver of the implicit examples (`None`: upstream
    /// `_auto`).
    pub sparse_solver: Option<SparseSolverType>,
    /// Everything the example writes to its standard output, recorded
    /// whether printed or not ([`Example::say`], [`Example::print_raw`];
    /// clones share it). The tests compare it with the oracle's
    /// `stdout.txt`.
    pub transcript: std::sync::Arc<std::sync::Mutex<String>>,
    /// Everything the example writes to its standard error
    /// ([`Example::say_err`]; the contact examples print their energies
    /// there), compared with the oracle's `stderr.txt`.
    pub stderr_transcript: std::sync::Arc<std::sync::Mutex<String>>,
}

impl Example {
    /// Default configuration of example `name` whose upstream sources live
    /// in `examples/c++/<upstream_dir>` and whose input file is
    /// `input_file` (in the example's source directory).
    #[must_use]
    pub fn new(name: &'static str, upstream_dir: &str, input_file: &str) -> Self {
        let source_dir = Path::new(CRATE_DIR).join("examples").join(name);
        Self {
            name,
            input_file: source_dir.join(input_file),
            source_dir,
            mesh_dir: frozen_meshes().join(upstream_dir),
            output_dir: PathBuf::from("."),
            steps: None,
            dump: true,
            upstream_compat: false,
            verbose: true,
            sparse_solver: None,
            transcript: std::sync::Arc::default(),
            stderr_transcript: std::sync::Arc::default(),
        }
    }

    /// Apply the command-line arguments `args` (without the program name).
    ///
    /// # Errors
    /// Unknown or malformed arguments.
    pub fn with_args(mut self, args: &[String]) -> Result<Self> {
        let mut p = ArgumentParser::default();
        akantu::parser::cli::register_akantu_options(&mut p)?;
        p.add_argument(
            ArgumentSpec::new("--output-dir", "directory receiving the outputs").default_value("."),
        )?;
        p.add_argument(
            ArgumentSpec::new("--mesh-dir", "directory holding the meshes").default_value(""),
        )?;
        p.add_argument(
            ArgumentSpec::new("--steps", "stop after at most this number of steps")
                .arg_type(ArgumentType::Integer)
                .default_value("-1"),
        )?;
        p.add_argument(
            ArgumentSpec::new("--solver", "sparse direct solver (eigen, mumps, ...)")
                .default_value(""),
        )?;
        for (flag, help) in [
            ("--no-dump", "skip the ParaView dumps"),
            (
                "--upstream-compat",
                "reproduce the upstream defects of the example",
            ),
            ("--quiet", "no progress output"),
        ] {
            p.add_argument(
                ArgumentSpec::new(flag, help)
                    .nargs(Nargs::Exactly(0))
                    .arg_type(ArgumentType::Boolean)
                    .default_value("false")
                    .const_value("true"),
            )?;
        }
        p.parse(args)?;
        if p.get_bool("help").unwrap_or(false) {
            // upstream cppargparse `_exit()`: the help on stderr, exit
            // status 0
            eprintln!("{}", p.help_string());
            std::process::exit(0);
        }
        let input = p.get_string("--aka_input_file")?;
        if !input.is_empty() {
            self.input_file = PathBuf::from(input);
        }
        let mesh_dir = p.get_string("--mesh-dir")?;
        if !mesh_dir.is_empty() {
            self.mesh_dir = PathBuf::from(mesh_dir);
        }
        self.output_dir = PathBuf::from(p.get_string("--output-dir")?);
        let steps = p.get_int("--steps")?;
        self.steps = usize::try_from(steps).ok();
        self.dump = !p.get_bool("--no-dump")?;
        self.upstream_compat = p.get_bool("--upstream-compat")?;
        self.verbose = !p.get_bool("--quiet")?;
        let solver = p.get_string("--solver")?;
        if !solver.is_empty() {
            self.sparse_solver = Some(solver.parse()?);
        }
        Ok(self)
    }

    /// Configuration of a test run: outputs below `output_dir`, no progress
    /// output.
    #[must_use]
    pub fn for_test(mut self, output_dir: &Path) -> Self {
        self.output_dir = output_dir.to_path_buf();
        self.verbose = false;
        self
    }

    /// Path of the mesh file `file`.
    #[must_use]
    pub fn mesh_path(&self, file: &str) -> PathBuf {
        self.mesh_dir.join(file)
    }

    /// Upstream `Mesh mesh(dim); mesh.read(file)`.
    ///
    /// # Errors
    /// Unreadable or invalid mesh files.
    pub fn read_mesh(&self, spatial_dimension: usize, file: &str) -> Result<Mesh> {
        let mut mesh = Mesh::new(spatial_dimension, "mesh")?;
        akantu::io::read_mesh(&mut mesh, &self.mesh_path(file), MeshIoType::Auto)?;
        Ok(mesh)
    }

    /// The parsed input file (upstream static parser after `initialize`).
    ///
    /// # Errors
    /// Unreadable or invalid input files.
    pub fn parser(&self) -> Result<Parser> {
        Parser::parse_file(&self.input_file)
    }

    /// Create the output directory.
    ///
    /// # Errors
    /// I/O errors.
    pub fn create_output_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.output_dir).map_err(|source| Error::Io {
            path: self.output_dir.display().to_string(),
            source,
        })
    }

    /// Path of the output `file` (relative to the run directory).
    #[must_use]
    pub fn output(&self, file: &str) -> PathBuf {
        self.output_dir.join(file)
    }

    /// Directory of the ParaView dumps (upstream default `./paraview`).
    #[must_use]
    pub fn paraview_dir(&self) -> PathBuf {
        self.output("paraview")
    }

    /// Number of steps to run: the upstream count, bounded by `--steps`.
    #[must_use]
    pub fn steps_or(&self, upstream: usize) -> usize {
        self.steps.map_or(upstream, |s| s.min(upstream))
    }

    /// Model options for `method` with the pinned sparse solver, if any
    /// (upstream `initFull(_analysis_method = method)`).
    #[must_use]
    pub fn model_options(
        &self,
        method: akantu::core::enums::AnalysisMethod,
    ) -> akantu::model::ModelOptions {
        let mut options = akantu::model::ModelOptions::new(method);
        options.solver_options.sparse_solver_type = self.sparse_solver;
        options
    }

    /// Print a progress line (upstream `std::cout << ... << std::endl`).
    pub fn say(&self, line: impl AsRef<str>) {
        self.print_raw(format!("{}\n", line.as_ref()));
    }

    /// Print `text` as is and flush (upstream `std::cout << ... <<
    /// std::flush`).
    pub fn print_raw(&self, text: impl AsRef<str>) {
        use std::io::Write as _;
        if self.verbose {
            print!("{}", text.as_ref());
            let _ = std::io::stdout().flush();
        }
        if let Ok(mut t) = self.transcript.lock() {
            t.push_str(text.as_ref());
        }
    }

    /// Print `text` as is to the standard error (upstream `std::cerr <<
    /// ...`), recorded whether printed or not.
    pub fn say_err(&self, text: impl AsRef<str>) {
        if self.verbose {
            eprint!("{}", text.as_ref());
        }
        if let Ok(mut t) = self.stderr_transcript.lock() {
            t.push_str(text.as_ref());
        }
    }

    /// The standard error written so far ([`Example::say_err`]).
    #[must_use]
    pub fn stderr_transcript(&self) -> String {
        self.stderr_transcript
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default()
    }

    /// The standard output written so far.
    #[must_use]
    pub fn transcript(&self) -> String {
        self.transcript
            .lock()
            .map(|t| t.clone())
            .unwrap_or_default()
    }
}

/// Descriptions upstream registers with the parameters of the laws used by
/// the examples (`registerParam(name, …, description)` in
/// `src/model/solid_mechanics/material.cc`, `materials/material_elastic.cc`,
/// `materials/material_thermal.cc`, `plane_stress_toolbox.hh`,
/// `material_viscoelastic_maxwell.cc` and the
/// example's `local_material_damage.cc`); the port's laws register the same
/// strings, which the parameter trait does not expose. A name absent here
/// has no description (upstream `name`, `Yd`, `Sd`).
const UPSTREAM_PARAM_DESCRIPTIONS: &[(&str, &str)] = &[
    ("E", "Young's modulus"),
    // material_viscoelastic_maxwell.cc
    ("Einf", "Stiffness of the elastic element"),
    ("Eta", "Viscosity of a Maxwell element"),
    ("Ev", "Stiffness of a Maxwell element"),
    ("Plane_Stress", "Is plane stress"),
    ("alpha", "Thermal expansion coefficient"),
    ("delta_T", "Uniform temperature field"),
    ("eigen_grad_u", "EigenGradU"),
    ("finite_deformation", "Is finite deformation"),
    ("inelastic_deformation", "Is inelastic deformation"),
    ("kapa", "Bulk coefficient"),
    ("lambda", "First Lamé coefficient"),
    ("mu", "Second Lamé coefficient"),
    ("nu", "Poisson's ratio"),
    ("previous_dt", "Time step of previous solveStep"),
    ("rho", "Density"),
];

/// Upstream `Parameter::printself` prefix of one parameter line: access
/// flags, the name, the bracketed description right-aligned after a
/// 10-byte name field, the ` : ` separator right-aligned in a 50-byte
/// field (`std::stringstream::width`, lengths in bytes, so `Lamé` counts
/// 5).
fn parameter_prefix(flags: &str, name: &str, description: &str) -> String {
    let mut field = name.to_owned();
    if !description.is_empty() {
        let bracket = " [";
        let width = 10usize.saturating_sub(name.len());
        field.push_str(&" ".repeat(width.saturating_sub(bracket.len())));
        field.push_str(bracket);
        field.push_str(description);
        field.push(']');
    }
    let width = 50usize.saturating_sub(field.len());
    let sep = " : ";
    format!(
        " {flags} {field}{}{sep}",
        " ".repeat(width.saturating_sub(sep.len()))
    )
}

/// A parameter value as upstream's `ParameterTyped<T>::printself` streams
/// it: reals and integers with the default precision 6, booleans as
/// `true`/`false`, vectors `[ a b ]`, matrices row by row
/// `[ [ a b ] [ c d ] ]`.
fn parameter_value(v: &ParamValue) -> String {
    match v {
        ParamValue::Real(x) => cout(*x),
        ParamValue::Int(i) => i.to_string(),
        ParamValue::Bool(b) => b.to_string(),
        ParamValue::String(t) | ParamValue::Enum(t) => t.clone(),
        ParamValue::VectorReal(x) => {
            let mut s = String::from("[ ");
            for v in x {
                s += &format!("{} ", cout(*v));
            }
            s + "]"
        }
        ParamValue::MatrixReal(m) => {
            let mut s = String::from("[ ");
            for i in 0..m.rows() {
                s += "[ ";
                for j in 0..m.cols() {
                    s += &format!("{} ", cout(m.at(i, j)));
                }
                s += "] ";
            }
            s + "]"
        }
        other => format!("{other:?}"),
    }
}

/// Upstream `std::cout << model.getMaterial(i)` (`ConstitutiveLaw::printself`
/// at indent 0 and `ParameterRegistry::printself` at indent 1): the
/// `Constitutive Law [ … ]` block with every registered parameter in
/// upstream's `std::map` order (bytes), its access flags, description and
/// value. Values come from the law's parameter registry; parameters that
/// are not readable through it come from the common law state (`rho`,
/// `name`, `finite_deformation`, `inelastic_deformation`,
/// `eigen_grad_u`) or from `private_value` (a law's own parse-only
/// parameters, e.g. the example law's `E`, `nu`). A real parameter that is
/// also an internal field of the law (`delta_T`: upstream registers the
/// `InternalField` object) prints as upstream's `InternalField::printself`
/// (id and the number of element types allocated per ghost type).
///
/// # Errors
/// A parameter whose value cannot be obtained.
pub fn law_printout(
    law: &dyn akantu::solid::Material,
    private_value: &dyn Fn(&str) -> Option<ParamValue>,
) -> Result<String> {
    use akantu::core::GhostType;
    use akantu::model::constitutive_law::InternalFieldRef;
    let base = law.base();
    let mut out = String::from("Constitutive Law [\n");
    out += &format!(" + id                : {}\n", base.id);
    out += &format!(" + name              : {}\n", base.name);
    out += &format!(" + spatial dimension : {}\n", base.spatial_dimension);
    out += " + default FE Engine : \n";
    let mut fields: Vec<(String, usize, usize)> = Vec::new();
    law.for_each_internal(&mut |name, f| {
        if let InternalFieldRef::Real(f) = f {
            let types = |g| {
                f.values()
                    .element_types(akantu::core::TypeFilter::new().ghost(g))
                    .count()
            };
            fields.push((
                name.to_owned(),
                types(GhostType::NotGhost),
                types(GhostType::Ghost),
            ));
        }
    });
    let mut names = law.param_names();
    names.sort_unstable();
    for name in names {
        let info = law
            .param_info(name)
            .ok_or_else(|| Error::invalid(format!("parameter {name} without info")))?;
        let description = UPSTREAM_PARAM_DESCRIPTIONS
            .iter()
            .find_map(|(n, d)| (*n == name).then_some(*d))
            .unwrap_or("");
        let value = match (info.kind, fields.iter().find(|(n, ..)| n == name)) {
            (ParamKind::Real, Some((_, nt, ng))) => {
                format!(
                    "InternalField [ {}:{name} {{{nt} types - {ng} ghost types}} ]",
                    base.id
                )
            }
            _ => {
                let v = match name {
                    "rho" => ParamValue::Real(base.rho),
                    "name" => ParamValue::String(base.name.clone()),
                    "finite_deformation" => ParamValue::Bool(base.finite_deformation),
                    "inelastic_deformation" => ParamValue::Bool(base.inelastic_deformation),
                    "eigen_grad_u" => ParamValue::MatrixReal(base.eigen_grad_u_param.clone()),
                    _ => match private_value(name) {
                        Some(v) => v,
                        None => law.get_param(name)?,
                    },
                };
                parameter_value(&v)
            }
        };
        out += &format!(
            " {}{value}\n",
            parameter_prefix(&info.access.flags_string(), name, description)
        );
    }
    out += "]\n";
    Ok(out)
}

/// An output file written line by line (upstream `std::ofstream`).
#[derive(Debug)]
pub struct TextOutput {
    path: PathBuf,
    text: String,
}

impl TextOutput {
    /// New (empty) file `path`, written by [`TextOutput::close`].
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            text: String::new(),
        }
    }

    /// Append `line` and a newline.
    pub fn line(&mut self, line: impl AsRef<str>) {
        self.text.push_str(line.as_ref());
        self.text.push('\n');
    }

    /// Write the file.
    ///
    /// # Errors
    /// I/O errors.
    pub fn close(self) -> Result<()> {
        std::fs::write(&self.path, self.text).map_err(|source| Error::Io {
            path: self.path.display().to_string(),
            source,
        })
    }
}

/// A floating-point value as printed by `std::ostream << x` with the
/// default precision 6 (C `%g`).
#[must_use]
pub fn cout(x: Real) -> String {
    akantu::parser::stream::format_general(x, 6)
}

/// Program entry point: run `f` with the command-line arguments and turn
/// an error into a failure exit code (upstream: an uncaught exception
/// aborts the example with a non-zero status).
pub fn main_with(f: impl FnOnce(&[String]) -> Result<()>) -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match f(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
