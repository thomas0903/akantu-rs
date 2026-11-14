// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu's JOSS scalability test (EPFL/LSMS):
// performance-testing-cohesive @ 483bace8, cohesive_extrinsic.cc (class
// `Chrono`), as instrumented by the akantu-rs harness
// benchmarks/cohesive-perf/src/cohesive_extrinsic.cc.

//! Per-phase accumulated wall-clock timers with the upstream "Chrono" CSV
//! output (`timmings_*.csv` rows).
//!
//! Upstream semantics: [`Chrono::store_time`] adds the time elapsed since
//! the previous `store_time` (or [`Chrono::start`]) to the phase and
//! restarts the clock, so consecutive phases tile the run. Phases are kept
//! in a `std::map<std::string, …>`, i.e. sorted byte-wise by name — a
//! `BTreeMap<String, …>` sorts identically. The CSV prints the accumulated
//! seconds with the default stream precision (`%g`, 6 significant digits)
//! and the number of measurements of each phase.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use akantu_parser::stream::format_double;

/// Accumulated phase timers (upstream `Chrono`).
#[derive(Clone, Debug)]
pub struct Chrono {
    start: Instant,
    measures: BTreeMap<String, (Duration, u32)>,
    prank: usize,
    psize: usize,
    verbose: bool,
}

impl Chrono {
    /// Timers of rank `prank` out of `psize`; `verbose` prints upstream's
    /// "Passing the first … chrono!" line on rank 0.
    #[must_use]
    pub fn new(prank: usize, psize: usize, verbose: bool) -> Self {
        Self {
            start: Instant::now(),
            measures: BTreeMap::new(),
            prank,
            psize,
            verbose,
        }
    }

    /// Upstream `start()`: restart the clock.
    pub fn start(&mut self) {
        self.start = Instant::now();
    }

    /// Upstream `store_time(type)`: accumulate the time since the last
    /// call into `phase` and restart the clock.
    pub fn store_time(&mut self, phase: &str) {
        let end = Instant::now();
        let dt = end.duration_since(self.start);
        if let Some((total, n)) = self.measures.get_mut(phase) {
            *total += dt;
            *n += 1;
        } else {
            self.measures.insert(phase.to_owned(), (dt, 1));
            if self.prank == 0 && self.verbose {
                println!(
                    "Passing the first {phase} chrono! [{}]",
                    format_double(dt.as_secs_f64())
                );
            }
        }
        self.start = Instant::now();
    }

    /// Accumulated seconds and number of measurements of `phase`.
    #[must_use]
    pub fn get(&self, phase: &str) -> Option<(f64, u32)> {
        self.measures.get(phase).map(|(d, n)| (d.as_secs_f64(), *n))
    }

    /// Every phase in output order: (name, seconds, repetitions).
    pub fn phases(&self) -> impl Iterator<Item = (&str, f64, u32)> {
        self.measures
            .iter()
            .map(|(k, (d, n))| (k.as_str(), d.as_secs_f64(), *n))
    }

    /// Upstream `printself_csv`: header line and value line, each ended by a
    /// newline (`"psize", "<phase>", "<phase> nb_rep", …` then
    /// `<psize>, <seconds>, <reps>, …`).
    #[must_use]
    pub fn to_csv(&self) -> String {
        let mut s = String::from("\"psize\"");
        for name in self.measures.keys() {
            let _ = write!(s, ", \"{name}\", \"{name} nb_rep\"");
        }
        s.push('\n');
        let _ = write!(s, "{}", self.psize);
        for (d, n) in self.measures.values() {
            let _ = write!(s, ", {}, {n}", format_double(d.as_secs_f64()));
        }
        s.push('\n');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_is_sorted_by_name_and_counts_repetitions() {
        let mut c = Chrono::new(1, 4, false);
        c.store_time("static_solve");
        c.store_time("check_cohesive_stress");
        c.store_time("check_cohesive_stress");
        c.store_time("before_step");
        let csv = c.to_csv();
        let mut lines = csv.lines();
        assert_eq!(
            lines.next().unwrap(),
            "\"psize\", \"before_step\", \"before_step nb_rep\", \"check_cohesive_stress\", \
             \"check_cohesive_stress nb_rep\", \"static_solve\", \"static_solve nb_rep\""
        );
        let values: Vec<&str> = lines.next().unwrap().split(", ").collect();
        assert_eq!(values.len(), 7);
        assert_eq!(values[0], "4");
        assert_eq!((values[2], values[4], values[6]), ("1", "2", "1"));
        assert_eq!(c.get("check_cohesive_stress").map(|(_, n)| n), Some(2));
    }
}
