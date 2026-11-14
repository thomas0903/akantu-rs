// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Shared helpers of
// the Rust ports of examples/c++/solid_mechanics_cohesive_model/** and
// examples/c++/scalability_test: the standard-output transcript
// of an example, the count of cohesive elements, the per-step energy
// record of the oracle variants (oracle/probes/examples/*_full.cc) and a
// text file flushed line by line (upstream `std::ofstream << std::endl`).

//! Shared helpers of the cohesive examples.
//!
//! * [`Transcript`], what the upstream example prints on its standard
//!   output, printed (unless `--quiet`) **and** recorded, so the
//!   integration tests compare it line by line with the oracle's
//!   `stdout.txt`;
//! * [`nb_cohesive_elements`], upstream
//!   `mesh.getNbElement(dim, _not_ghost, _ek_cohesive)`;
//! * [`EnergyRecord`], the per-step quantities the oracle variants
//!   `<example>_full` write at 17 significant digits (`energy_full.csv`);
//! * [`LineFile`], a text output written as upstream's `std::ofstream`
//!   with `std::endl`: created when opened, each line on disk once written.

#![allow(dead_code)] // shared by several examples; each uses a subset

use std::io::Write as _;
use std::path::{Path, PathBuf};

use akantu::core::{ElementKind, Error, GhostType, Real, Result, TypeFilter};
use akantu::mesh::Mesh;

/// Standard output of an example: printed when `verbose`, always recorded.
#[derive(Clone, Debug, Default)]
pub struct Transcript {
    verbose: bool,
    /// The lines, in order (without their newline).
    pub lines: Vec<String>,
    /// Text printed without a newline yet ([`Transcript::partial`]): it
    /// starts the next recorded line.
    pending: String,
}

impl Transcript {
    /// New transcript; `verbose` prints every line as it is recorded.
    #[must_use]
    pub fn new(verbose: bool) -> Self {
        Self {
            verbose,
            lines: Vec::new(),
            pending: String::new(),
        }
    }

    /// Upstream `std::cout << line << "\n"`.
    pub fn line(&mut self, line: impl Into<String>) {
        let line = line.into();
        if self.verbose {
            println!("{line}");
        }
        let mut full = std::mem::take(&mut self.pending);
        full.push_str(&line);
        self.lines.push(full);
    }

    /// Upstream `std::cout << text; std::cout.flush()` without a newline
    /// (a progress line ending in `\r`): printed at once, recorded as the
    /// start of the next line.
    pub fn partial(&mut self, text: impl AsRef<str>) {
        let text = text.as_ref();
        if self.verbose {
            print!("{text}");
            // a failed flush of the terminal only delays the progress text
            let _ = std::io::stdout().flush();
        }
        self.pending.push_str(text);
    }
}

/// Upstream `mesh.getNbElement(dim, _not_ghost, _ek_cohesive)`: the number
/// of not-ghost cohesive elements of every cohesive type of dimension
/// `dim`.
#[must_use]
pub fn nb_cohesive_elements(mesh: &Mesh, dim: usize) -> usize {
    let types: Vec<_> = mesh
        .element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: GhostType::NotGhost,
            kind: Some(ElementKind::Cohesive),
        })
        .collect();
    types
        .into_iter()
        .map(|ty| mesh.nb_element(ty, GhostType::NotGhost))
        .sum()
}

/// One line of the 17-digit energy record of an oracle variant
/// (`energy_full.csv`: `step,nb_cohesive,dissipated,reversible,potential,
/// kinetic`), written after the step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnergyRecord {
    /// Step number (1-based, as the upstream loops).
    pub step: usize,
    /// Number of cohesive elements after the step.
    pub nb_cohesive: usize,
    /// `getEnergy("dissipated")`.
    pub dissipated: Real,
    /// `getEnergy("reversible")`.
    pub reversible: Real,
    /// `getEnergy("potential")`.
    pub potential: Real,
    /// `getEnergy("kinetic")`.
    pub kinetic: Real,
}

impl EnergyRecord {
    /// The record of the model after step `step` (the energies the oracle
    /// variants query, in the same order).
    ///
    /// # Errors
    /// Energy or mesh errors.
    pub fn of(
        model: &mut akantu::cohesive::SolidMechanicsModelCohesive,
        step: usize,
    ) -> Result<Self> {
        let dim = model.smm.spatial_dimension();
        let nb_cohesive = {
            let mesh_h = model.smm.physics.base.mesh.clone();
            let mesh = mesh_h.read()?;
            nb_cohesive_elements(&mesh, dim)
        };
        let dissipated = model.energy("dissipated")?;
        let reversible = model.energy("reversible")?;
        let potential = model.energy("potential")?;
        let kinetic = model.energy("kinetic")?;
        Ok(Self {
            step,
            nb_cohesive,
            dissipated,
            reversible,
            potential,
            kinetic,
        })
    }

    /// The four energies, in the column order of `energy_full.csv`.
    #[must_use]
    pub fn energies(&self) -> [Real; 4] {
        [
            self.dissipated,
            self.reversible,
            self.potential,
            self.kinetic,
        ]
    }
}

/// A text file written line by line, each line flushed to the file as it
/// is written (upstream `std::ofstream fout(path); fout << … << std::endl`):
/// an aborted run leaves the lines written so far.
#[derive(Debug)]
pub struct LineFile {
    path: PathBuf,
    file: std::fs::File,
}

impl LineFile {
    /// Create (truncate) `path`.
    ///
    /// # Errors
    /// I/O errors.
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = std::fs::File::create(&path).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Self { path, file })
    }

    /// Write `line` and a newline, then flush (`<< std::endl`).
    ///
    /// # Errors
    /// I/O errors.
    pub fn line(&mut self, line: impl AsRef<str>) -> Result<()> {
        let mut text = String::with_capacity(line.as_ref().len() + 1);
        text.push_str(line.as_ref());
        text.push('\n');
        self.file
            .write_all(text.as_bytes())
            .and_then(|()| self.file.flush())
            .map_err(|source| Error::Io {
                path: self.path.display().to_string(),
                source,
            })
    }
}
