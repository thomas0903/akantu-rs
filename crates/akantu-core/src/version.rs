// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_config.hh.in
// (AKANTU_VERSION_*), src/common/aka_config.cc.in (getVersion),
// cmake/semver.py (version string layout).

//! Version of the Akantu reference this port tracks (CORE-002).
//!
//! Upstream generates the version at configure time from `git describe`
//! (`cmake/semver.py`): `MAJOR.MINOR.PATCH<prerelease>+<distance>.g<sha>`
//! (plus `.dirty` for a modified tree). The port tracks the reference
//! commit `35a85178` = `v5.0.7-post1-554-g35a851780`; [`get_version`]
//! returns the string a clean build of that commit reports. The oracle
//! build reports `5.0.7-post1+554.g35a851780.dirty` because it compiles a
//! patched copy of the tree (`oracle/patches`, docs/ORACLE_BUILD.md): the
//! `.dirty` build metadata is a property of that build, not of the library.
//! The port's own crate version is `akantu::VERSION`.

/// Upstream `AKANTU_VERSION_MAJOR`.
pub const AKANTU_VERSION_MAJOR: u32 = 5;
/// Upstream `AKANTU_VERSION_MINOR`.
pub const AKANTU_VERSION_MINOR: u32 = 0;
/// Upstream `AKANTU_VERSION_PATCH`.
pub const AKANTU_VERSION_PATCH: u32 = 7;
/// Upstream `AKANTU_VERSION_PRERELEASE`.
pub const AKANTU_VERSION_PRERELEASE: &str = "-post1";
/// Upstream `AKANTU_VERSION = MAJOR·10000 + MINOR·100 + PATCH`.
pub const AKANTU_VERSION: u32 =
    AKANTU_VERSION_MAJOR * 10000 + AKANTU_VERSION_MINOR * 100 + AKANTU_VERSION_PATCH;
/// Build metadata of the reference commit (`+<distance>.g<sha>`).
pub const AKANTU_VERSION_LOCAL: &str = "+554.g35a851780";

/// Upstream `getVersion()` of a clean build of the reference commit:
/// `"5.0.7-post1+554.g35a851780"`.
#[must_use]
pub fn get_version() -> &'static str {
    "5.0.7-post1+554.g35a851780"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parts_are_consistent() {
        assert_eq!(AKANTU_VERSION, 50007);
        assert_eq!(
            get_version(),
            format!(
                "{AKANTU_VERSION_MAJOR}.{AKANTU_VERSION_MINOR}.{AKANTU_VERSION_PATCH}\
                 {AKANTU_VERSION_PRERELEASE}{AKANTU_VERSION_LOCAL}"
            )
        );
    }
}
