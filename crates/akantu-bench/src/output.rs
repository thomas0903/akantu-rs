// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from the akantu-rs harness copy of Akantu's JOSS scalability test
// (EPFL/LSMS): benchmarks/cohesive-perf/src/cohesive_extrinsic.cc
// (energies.csv, insertions.csv, summary.json, summary_rank<r>.json).

//! Output files of the cohesive scalability driver, byte-compatible with
//! the C++ harness: numbers are printed as `std::ostream` does with
//! `std::setprecision(p)` (C `%.{p}g`, [`format_general`]).

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use akantu_core::{Error, Real, Result};
use akantu_parser::stream::format_general;

/// `%.{p}g` (upstream `std::setprecision(p)` with the default float field).
#[must_use]
pub fn g(x: Real, p: usize) -> String {
    format_general(x, p)
}

/// One `energies.csv` row: step, `ed`, `ep`, `ek`, `ew`, `et`.
pub type EnergyRow = (usize, [Real; 5]);

/// Optional first line (`note` followed by a newline) of an output file.
fn note_line(note: Option<&str>) -> String {
    note.map(|n| format!("{n}\n")).unwrap_or_default()
}

/// `energies.csv`: header and one row per step, `precision` significant
/// digits (harness `energy_precision`, upstream default 6). `note` (a `#`
/// comment) precedes the header: the driver marks runs that are not the
/// upstream problem with it (`rust_static_solve = false`).
#[must_use]
pub fn energies_csv(rows: &[EnergyRow], precision: usize, note: Option<&str>) -> String {
    let mut s = note_line(note);
    s.push_str("step, ed, ep, ek, ew, et\n");
    for (step, [ed, ep, ek, ew, et]) in rows {
        let p = precision;
        let _ = writeln!(
            s,
            "{step}, {}, {}, {}, {}, {}",
            g(*ed, p),
            g(*ep, p),
            g(*ek, p),
            g(*ew, p),
            g(*et, p)
        );
    }
    s
}

/// `insertions.csv`: every step with insertions, the global number of
/// inserted elements and the cumulative count (`note` as in
/// [`energies_csv`]).
#[must_use]
pub fn insertions_csv(insertions: &[(usize, usize)], note: Option<&str>) -> String {
    let mut s = note_line(note);
    s.push_str("step, nb_inserted, nb_cohesive_total\n");
    let mut total = 0_usize;
    for (step, nb) in insertions {
        total += nb;
        let _ = writeln!(s, "{step}, {nb}, {total}");
    }
    s
}

/// Physical summary of a run (rank 0 `summary.json`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    /// Number of ranks.
    pub psize: usize,
    /// Mesh path as written in the input.
    pub mesh: String,
    /// `max_steps`.
    pub max_steps: usize,
    /// `cohesive_insertion`.
    pub cohesive_insertion: bool,
    /// `blocked_node` rule.
    pub blocked_node: String,
    /// Node→element map maintenance mode (the C++ harness:
    /// `upstream`/`joss`; the port keeps no node→element map: `none`).
    pub fill_nodes_to_elements: String,
    /// Δt.
    pub time_step: Real,
    /// Global nodes before the loop.
    pub nb_global_nodes_start: usize,
    /// Global nodes after the loop.
    pub nb_global_nodes_end: usize,
    /// Sum of the owned nodes over the ranks.
    pub nb_owned_nodes_sum: usize,
    /// Regular elements of dimension 3 (global).
    pub nb_regular_elements_dim3: usize,
    /// Cohesive elements (global).
    pub nb_cohesive_elements: usize,
    /// Steps with insertions.
    pub nb_insertion_steps: usize,
    /// First step with an insertion (0: none).
    pub first_insertion_step: usize,
    /// Dissipated energy at the end.
    pub energy_dissipated: Real,
    /// Potential energy at the end.
    pub energy_potential: Real,
    /// Kinetic energy at the end.
    pub energy_kinetic: Real,
    /// ‖u‖₂ over owned nodes.
    pub displacement_l2_norm: Real,
    /// max |uᵢ| over owned nodes.
    pub displacement_max_abs: Real,
    /// Σ u per component over owned nodes.
    pub displacement_sum: [Real; 3],
    /// Wall time of the loop (s).
    pub loop_time_s: Real,
    /// Wall time before the loop (s).
    pub init_time_s: Real,
    /// Port-specific: the model that ran (`cohesive`/`solid`).
    pub model: String,
    /// Port-specific: cohesive compatibility mode.
    pub cohesive_compat: String,
    /// Port-specific: the effective placement of a doubled node's
    /// accumulated Neumann load (`upstream` / `follows_elements`, D-105).
    pub doubled_node_load: String,
    /// Port-specific: whether the static preload ran (`rust_static_solve`).
    pub static_solve: bool,
    /// Port-specific: sparse solver of the static solve as requested
    /// (`input` = the input file's `sparse_solver_type`, i.e. `auto`).
    pub sparse_solver: String,
    /// Port-specific: the sparse solver that actually ran the
    /// static solve (`auto` resolved; `none` without a static solve).
    pub static_sparse_solver: String,
    /// Port-specific: the ordering of the `eigen` solver requested
    /// (`rust_sparse_ordering`, else `AKANTU_SPARSE_ORDERING`, else
    /// `default`: COLAMD in the bitwise precision, `auto` in the fast one;
    /// D-125). The ordering an analysis used is in the solver statistics.
    pub sparse_ordering: String,
    /// Port-specific: the static solver's statistics (MUMPS `INFOG` /
    /// `RINFOG` figures, rank 0; empty for the pure-Rust solvers).
    pub static_solver_stats: Vec<(&'static str, Real)>,
    /// Port-specific: global order of the static matrix and its entry
    /// counts per rank (rank-partial AIJ entries: sum and maximum).
    pub static_matrix: Option<MatrixStats>,
    /// Port-specific: resident memory per rank (kiB), maximum and sum over
    /// the ranks of each [`crate::memory::MemoryRecord`] field.
    pub memory: MemorySummary,
    /// Port-specific: the per-copy check of the loads, only
    /// when the input asks for it (`rust_load_check_*`); without it
    /// `summary.json` has none of its keys.
    pub load_check: Option<crate::load_check::LoadCheckSummary>,
    /// Port-specific: the execution policy the run's models started with
    /// (thread count per rank, precision mode, SIMD level), so that a
    /// campaign can check the requested settings took effect.
    pub exec: ExecSummary,
}

/// The execution policy of a run (`summary.json`, `rust_exec`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecSummary {
    /// Threads per rank.
    pub threads: usize,
    /// `bitwise` or `fast`.
    pub precision: String,
    /// The SIMD level of the kernels (`scalar`, `avx2`, ...).
    pub simd: String,
}

impl ExecSummary {
    /// The process default policy (`AKANTU_NUM_THREADS`,
    /// `AKANTU_PRECISION`, `AKANTU_SIMD`); `unknown` fields if the
    /// environment is invalid (the model creation reports that error).
    #[must_use]
    pub fn process_default() -> Self {
        match akantu_core::exec::ExecPolicy::process_default() {
            Ok(p) => Self {
                threads: p.threads(),
                precision: p.precision().name().into(),
                simd: p.simd().name().into(),
            },
            Err(_) => Self {
                threads: 0,
                precision: "unknown".into(),
                simd: "unknown".into(),
            },
        }
    }

    fn to_json(&self) -> String {
        format!(
            "{{\"threads\": {}, \"precision\": \"{}\", \"simd\": \"{}\"}}",
            self.threads,
            json_escape(&self.precision),
            json_escape(&self.simd)
        )
    }
}

/// Size of the static matrix (`summary.json`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatrixStats {
    /// Global order (equations).
    pub order: u64,
    /// Sum over the ranks of the rank-partial entry counts.
    pub nnz_sum: u64,
    /// Largest rank-partial entry count.
    pub nnz_max: u64,
}

/// Memory of all ranks (`summary.json`, kiB).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemorySummary {
    /// Whether every rank reset its peak after the static solve.
    pub peak_reset: bool,
    /// `(field, max over ranks, sum over ranks)` for the fields measured on
    /// every rank.
    pub fields: Vec<(&'static str, u64, u64)>,
}

impl MemorySummary {
    fn to_json(&self) -> String {
        let mut s = format!("{{\"peak_reset\": {}", b(self.peak_reset));
        for (k, max, sum) in &self.fields {
            let _ = write!(s, ", \"{k}\": {{\"max\": {max}, \"sum\": {sum}}}");
        }
        s.push('}');
        s
    }
}

fn stats_json(stats: &[(&'static str, Real)]) -> String {
    let mut s = String::from("{");
    for (i, (k, v)) in stats.iter().enumerate() {
        let _ = write!(
            s,
            "{}\"{k}\": {}",
            if i == 0 { "" } else { ", " },
            g(*v, 17)
        );
    }
    s.push('}');
    s
}

fn b(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}

impl Summary {
    /// The C++ harness's `summary.json` (17 significant digits) followed by
    /// the port's own keys.
    #[must_use]
    pub fn to_json(&self) -> String {
        let p = 17;
        let mut s = String::from("{\n");
        let _ = writeln!(s, "  \"psize\": {},", self.psize);
        let _ = writeln!(s, "  \"mesh\": \"{}\",", json_escape(&self.mesh));
        let _ = writeln!(s, "  \"max_steps\": {},", self.max_steps);
        let _ = writeln!(
            s,
            "  \"cohesive_insertion\": {},",
            b(self.cohesive_insertion)
        );
        let _ = writeln!(s, "  \"blocked_node\": \"{}\",", self.blocked_node);
        let _ = writeln!(
            s,
            "  \"fill_nodes_to_elements\": \"{}\",",
            self.fill_nodes_to_elements
        );
        let _ = writeln!(s, "  \"time_step\": {},", g(self.time_step, p));
        let _ = writeln!(
            s,
            "  \"nb_global_nodes_start\": {},",
            self.nb_global_nodes_start
        );
        let _ = writeln!(
            s,
            "  \"nb_global_nodes_end\": {},",
            self.nb_global_nodes_end
        );
        let _ = writeln!(s, "  \"nb_owned_nodes_sum\": {},", self.nb_owned_nodes_sum);
        let _ = writeln!(
            s,
            "  \"nb_regular_elements_dim3\": {},",
            self.nb_regular_elements_dim3
        );
        let _ = writeln!(
            s,
            "  \"nb_cohesive_elements\": {},",
            self.nb_cohesive_elements
        );
        let _ = writeln!(s, "  \"nb_insertion_steps\": {},", self.nb_insertion_steps);
        let _ = writeln!(
            s,
            "  \"first_insertion_step\": {},",
            self.first_insertion_step
        );
        let _ = writeln!(
            s,
            "  \"energy_dissipated\": {},",
            g(self.energy_dissipated, p)
        );
        let _ = writeln!(
            s,
            "  \"energy_potential\": {},",
            g(self.energy_potential, p)
        );
        let _ = writeln!(s, "  \"energy_kinetic\": {},", g(self.energy_kinetic, p));
        let _ = writeln!(
            s,
            "  \"displacement_l2_norm\": {},",
            g(self.displacement_l2_norm, p)
        );
        let _ = writeln!(
            s,
            "  \"displacement_max_abs\": {},",
            g(self.displacement_max_abs, p)
        );
        let [a, bb, c] = self.displacement_sum;
        let _ = writeln!(
            s,
            "  \"displacement_sum\": [{}, {}, {}],",
            g(a, p),
            g(bb, p),
            g(c, p)
        );
        let _ = writeln!(s, "  \"loop_time_s\": {},", g(self.loop_time_s, p));
        let _ = writeln!(s, "  \"init_time_s\": {},", g(self.init_time_s, p));
        let _ = writeln!(s, "  \"rust_model\": \"{}\",", self.model);
        let _ = writeln!(
            s,
            "  \"rust_cohesive_compat\": \"{}\",",
            self.cohesive_compat
        );
        let _ = writeln!(
            s,
            "  \"rust_doubled_node_load\": \"{}\",",
            self.doubled_node_load
        );
        let _ = writeln!(s, "  \"rust_static_solve\": {},", b(self.static_solve));
        let _ = writeln!(s, "  \"rust_sparse_solver\": \"{}\",", self.sparse_solver);
        let _ = writeln!(
            s,
            "  \"rust_static_sparse_solver\": \"{}\",",
            self.static_sparse_solver
        );
        let _ = writeln!(
            s,
            "  \"rust_sparse_ordering\": \"{}\",",
            self.sparse_ordering
        );
        let _ = writeln!(
            s,
            "  \"rust_static_solver_stats\": {},",
            stats_json(&self.static_solver_stats)
        );
        if let Some(m) = self.static_matrix {
            let _ = writeln!(
                s,
                "  \"rust_static_matrix\": {{\"order\": {}, \"nnz_sum\": {}, \"nnz_max\": {}}},",
                m.order, m.nnz_sum, m.nnz_max
            );
        }
        if let Some(lc) = &self.load_check {
            s.push_str(&lc.json_lines());
        }
        let _ = writeln!(s, "  \"rust_exec\": {},", self.exec.to_json());
        let _ = writeln!(s, "  \"rust_memory_kib\": {}", self.memory.to_json());
        s.push_str("}\n");
        s
    }
}

/// A blocked node of `summary_rank<r>.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockedNodeInfo {
    /// Local id.
    pub local: usize,
    /// Global id.
    pub global: i64,
    /// Coordinates.
    pub x: [Real; 3],
}

/// Per-rank summary (`summary_rank<r>.json`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RankSummary {
    /// Rank.
    pub rank: usize,
    /// `blocked_node` rule.
    pub blocked_node_rule: String,
    /// Blocked nodes of this rank.
    pub blocked_y_nodes: Vec<BlockedNodeInfo>,
    /// Local nodes (incl. ghosts).
    pub nb_nodes: usize,
    /// Owned nodes (local or master).
    pub nb_owned_nodes: usize,
    /// Regular not-ghost elements of dimension 3.
    pub nb_regular_elements_not_ghost: usize,
    /// All not-ghost elements of dimension 3 (regular + cohesive).
    pub nb_elements_dim3_not_ghost: usize,
    /// All ghost elements of dimension 3.
    pub nb_elements_dim3_ghost: usize,
    /// Not-ghost cohesive elements.
    pub nb_cohesive_not_ghost: usize,
    /// Port-specific: resident memory of this rank at the
    /// phase boundaries.
    pub memory: crate::memory::MemoryRecord,
    /// Port-specific: rank-partial entries of the static matrix.
    pub static_matrix_nnz: Option<u64>,
    /// Port-specific: whether the static preload ran (`rust_static_solve`,
    /// appended after the harness's keys).
    pub static_solve: bool,
}

impl RankSummary {
    /// The C++ harness's single-line `summary_rank<r>.json`; the
    /// `fill_nodes_to_elements` block reports mode `none` with zero calls
    /// (the port keeps no node→element map).
    #[must_use]
    pub fn to_json(&self) -> String {
        let p = 17;
        let mut s = format!(
            "{{\"rank\": {}, \"blocked_node_rule\": \"{}\", \"blocked_y_nodes\": [",
            self.rank, self.blocked_node_rule
        );
        for (i, n) in self.blocked_y_nodes.iter().enumerate() {
            let _ = write!(
                s,
                "{}{{\"local\": {}, \"global\": {}, \"X\": [{}, {}, {}]}}",
                if i == 0 { "" } else { ", " },
                n.local,
                n.global,
                g(n.x[0], p),
                g(n.x[1], p),
                g(n.x[2], p)
            );
        }
        let _ = writeln!(
            s,
            "], \"fill_nodes_to_elements\": {{\"mode\": \"none\", \"event_calls\": 0, \
             \"event_calls_skipped\": 0, \"event_time_s\": 0, \"other_calls\": 0, \
             \"other_time_s\": 0}}, \"nb_nodes\": {}, \"nb_owned_nodes\": {}, \
             \"nb_regular_elements_not_ghost\": {}, \"nb_elements_dim3_not_ghost\": {}, \
             \"nb_elements_dim3_ghost\": {}, \"nb_cohesive_not_ghost\": {}, \
             \"rust_memory_kib\": {}, \"rust_static_matrix_nnz\": {}, \"rust_static_solve\": {}}}",
            self.nb_nodes,
            self.nb_owned_nodes,
            self.nb_regular_elements_not_ghost,
            self.nb_elements_dim3_not_ghost,
            self.nb_elements_dim3_ghost,
            self.nb_cohesive_not_ghost,
            memory_json(&self.memory),
            self.static_matrix_nnz
                .map_or_else(|| "null".to_string(), |n| n.to_string()),
            b(self.static_solve)
        );
        s
    }
}

/// One rank's [`crate::memory::MemoryRecord`] as a JSON object (kiB;
/// `null` = not measured on this platform).
fn memory_json(m: &crate::memory::MemoryRecord) -> String {
    let mut s = format!("{{\"peak_reset\": {}", b(m.peak_reset));
    for (k, v) in m.fields() {
        let _ = write!(
            s,
            ", \"{k}\": {}",
            v.map_or_else(|| "null".to_string(), |v| v.to_string())
        );
    }
    s.push('}');
    s
}

fn json_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
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
    o
}

/// Write `text` to `dir/name`.
///
/// # Errors
/// I/O errors.
pub fn write_file(dir: &Path, name: &str, text: &str) -> Result<()> {
    let path = dir.join(name);
    fs::write(&path, text).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn energies_use_c_general_format() {
        let rows = vec![
            (
                0,
                [0.0, 501_943.523_482_941_5, 0.0, 0.0, 501_943.523_482_941_5],
            ),
            (1, [0.0, 1.0e-7, 8.493_373_587_491_18e-6, -2.5, 1e21]),
        ];
        assert_eq!(
            energies_csv(&rows, 17, None),
            "step, ed, ep, ek, ew, et\n\
             0, 0, 501943.5234829415, 0, 0, 501943.5234829415\n\
             1, 0, 9.9999999999999995e-08, 8.4933735874911799e-06, -2.5, 1e+21\n"
        );
        assert_eq!(
            energies_csv(&rows[..1], 6, None),
            "step, ed, ep, ek, ew, et\n0, 0, 501944, 0, 0, 501944\n"
        );
        assert_eq!(
            energies_csv(&rows[..1], 6, Some("# note")),
            "# note\nstep, ed, ep, ek, ew, et\n0, 0, 501944, 0, 0, 501944\n"
        );
    }

    #[test]
    fn insertions_accumulate() {
        assert_eq!(
            insertions_csv(&[(710, 1), (775, 2)], None),
            "step, nb_inserted, nb_cohesive_total\n710, 1, 1\n775, 2, 3\n"
        );
        assert_eq!(
            insertions_csv(&[], Some("# note")),
            "# note\nstep, nb_inserted, nb_cohesive_total\n"
        );
    }

    #[test]
    fn rank_summary_ends_with_the_static_solve_flag() {
        let r = RankSummary {
            static_solve: false,
            ..RankSummary::default()
        };
        assert!(r.to_json().ends_with(", \"rust_static_solve\": false}\n"));
    }

    #[test]
    fn summary_records_the_execution_policy() {
        let s = Summary {
            exec: ExecSummary {
                threads: 4,
                precision: "fast".into(),
                simd: "avx2".into(),
            },
            ..Summary::default()
        };
        assert!(s.to_json().contains(
            "  \"rust_exec\": {\"threads\": 4, \"precision\": \"fast\", \"simd\": \"avx2\"},\n  \"rust_memory_kib\""
        ));
        // the process default of a test process: one thread unless the
        // environment says otherwise, a known precision name
        let d = ExecSummary::process_default();
        assert!(d.threads >= 1);
        assert!(d.precision == "bitwise" || d.precision == "fast");
    }
}
