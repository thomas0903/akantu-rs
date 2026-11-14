// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: memory instrumentation of the cohesive scalability driver
// (the C++ harness benchmarks/cohesive-perf measures the same
// quantities from outside the process).

//! Resident memory of this process at the driver's phase boundaries
//! (Linux `/proc/self/status`; `None` elsewhere).
//!
//! * `VmRSS`: resident set size now; `VmHWM`: its peak since the process
//!   started or since the last reset;
//! * reset of the peak: writing `5` to `/proc/self/clear_refs` (Linux ≥ 4.0)
//!   sets `VmHWM` to the current `VmRSS`, so that the peak of a later phase
//!   (the explicit loop) is measured apart from the static solve's.
//!
//! Reading these files neither allocates on the numerical path nor changes
//! any result; each read costs a few microseconds and happens outside the
//! timed phases' work (between Chrono marks).

/// A `/proc/self/status` field in kiB (`VmRSS`, `VmHWM`, …).
#[must_use]
pub fn status_kib(field: &str) -> Option<u64> {
    status_kib_impl(field)
}

#[cfg(target_os = "linux")]
fn status_kib_impl(field: &str) -> Option<u64> {
    let text = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_status_kib(&text, field)
}

#[cfg(not(target_os = "linux"))]
fn status_kib_impl(_field: &str) -> Option<u64> {
    None
}

/// `field:   1234 kB` of a `/proc/<pid>/status` text.
#[cfg(any(target_os = "linux", test))]
fn parse_status_kib(text: &str, field: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let rest = line.strip_prefix(field)?.strip_prefix(':')?;
        let mut it = rest.split_whitespace();
        let value = it.next()?.parse().ok()?;
        (it.next() == Some("kB")).then_some(value)
    })
}

/// Reset `VmHWM` to the current `VmRSS`; `false` if the kernel or the
/// sandbox refused it (the later peak then covers the whole run).
#[must_use]
pub fn reset_peak() -> bool {
    reset_peak_impl()
}

#[cfg(target_os = "linux")]
fn reset_peak_impl() -> bool {
    std::fs::write("/proc/self/clear_refs", b"5").is_ok()
}

#[cfg(not(target_os = "linux"))]
fn reset_peak_impl() -> bool {
    false
}

/// Memory of one rank at the driver's phase boundaries (kiB).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryRecord {
    /// `VmRSS` after the model set-up and boundary conditions, just before
    /// the static solve (mesh + model, no solver).
    pub rss_before_static: Option<u64>,
    /// `VmHWM` right after the static solve: peak of read, distribution,
    /// set-up and static solve.
    pub hwm_static: Option<u64>,
    /// `VmRSS` right after the static solve (the solver keeps its factors,
    /// as upstream's static time-step solver does).
    pub rss_after_static: Option<u64>,
    /// Whether the peak was reset after the static solve.
    pub peak_reset: bool,
    /// `VmHWM` at the end of the loop: the loop's peak if `peak_reset`,
    /// else the peak of the whole run.
    pub hwm_end: Option<u64>,
    /// `VmRSS` at the end of the loop.
    pub rss_end: Option<u64>,
}

impl MemoryRecord {
    /// The fields in output order, as `(key, value)` (`None` = not
    /// measured on this platform).
    #[must_use]
    pub fn fields(&self) -> [(&'static str, Option<u64>); 5] {
        [
            ("rss_before_static", self.rss_before_static),
            ("hwm_static", self.hwm_static),
            ("rss_after_static", self.rss_after_static),
            ("hwm_end", self.hwm_end),
            ("rss_end", self.rss_end),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_fields() {
        let text = "Name:\tcohesive_extrin\nVmPeak:\t  123456 kB\nVmHWM:\t   2048 kB\nVmRSS:\t    1024 kB\n";
        assert_eq!(parse_status_kib(text, "VmHWM"), Some(2048));
        assert_eq!(parse_status_kib(text, "VmRSS"), Some(1024));
        assert_eq!(parse_status_kib(text, "VmSwap"), None);
        // a field name that is a prefix of another one is not matched
        assert_eq!(parse_status_kib(text, "Vm"), None);
        assert_eq!(parse_status_kib("VmRSS:\t12 MB\n", "VmRSS"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn reads_this_process() {
        let rss = status_kib("VmRSS").expect("VmRSS on Linux");
        let hwm = status_kib("VmHWM").expect("VmHWM on Linux");
        // the peak never decreases (no reset in this test process)
        assert!(rss > 0 && hwm >= rss);
    }
}
