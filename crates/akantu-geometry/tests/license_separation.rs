// SPDX-License-Identifier: LGPL-3.0-or-later
// Licence-separation guard for the geometry port (decision record D-042; no
// upstream counterpart).

//! D-042 guard (declared GPL switches).
//!
//! 1. This LGPL crate contains no GPL-licensed source file.
//! 2. No LGPL build can pull in a GPL-3.0-or-later workspace crate
//!    (`akantu-geometry-cgal`, or any other member whose manifest declares a
//!    GPL licence) **except through a declared GPL switch**: a
//!    `(crate, feature)` pair of [`GPL_SWITCHES`], an opt-in Cargo feature
//!    whose builds are GPL-3.0-or-later as a whole (D-042). The check works
//!    on the **resolved** dependency graph:
//!    * the workspace manifests are read through
//!      `cargo metadata --no-deps` (no network, no download), and Cargo's
//!      feature resolution is replayed over the workspace members
//!      ([`resolve`]): `dep:` entries, implicit features of optional
//!      dependencies, `dep/feature`, weak `dep?/feature`, `default`,
//!      `default-features = false`, renamed dependencies, normal, build and
//!      (for the root) dev dependencies. It is conservative where Cargo is
//!      finer: every `[target.…]` table counts as active and features are
//!      unified across dependency kinds, so it can only find *more*
//!      packages than Cargo, never fewer;
//!    * for the umbrella crate, and for every switch, the same feature sets
//!      are also resolved by `cargo tree` itself (host target), which must
//!      agree: every member it reaches is reached by [`resolve`].
//!
//!    The rule, checked by the functions `check_*` (also run on hand-made
//!    workspaces that encode the bypass forms, see
//!    `switch_rule_rejects_every_bypass_form`):
//!    * (a) every declared switch is a feature of an LGPL member and does
//!      reach a GPL member, for the resolver and for `cargo tree` (live
//!      detection: no stale entry);
//!    * (b) no default feature set of any LGPL member reaches a GPL member
//!      or activates a declared switch: a switch is neither in its own
//!      crate's `default` nor enabled, directly or transitively, by the
//!      default features of a crate that depends on it;
//!    * (c) every LGPL member with **all its features except its own
//!      declared switches**, dev- and build-dependencies included, reaches
//!      no GPL member (members without switches: `--all-features`, as
//!      before). Any other path fails: an unconditional or renamed
//!      dependency, an optional one behind an unlisted feature, an unlisted
//!      feature that turns on a switch of the same crate or of another
//!      crate;
//!    * (d) umbrella policy: its default features, each of its features
//!      alone and all of them together except its switches (with
//!      `cargo tree` agreement); `igfem` implies `cgal` and enables
//!      `akantu-igfem/cgal`; `embedded` stays in `default` and does not
//!      imply `cgal`.
//!
//!    Third-party crates cannot depend on these unpublished path crates, so
//!    the workspace members are the whole relevant graph; the third-party
//!    licence audit is `docs/DEPENDENCIES.md`.
//!
//! The per-file SPDX/notice check of the GPL crate is
//! `akantu-geometry-cgal/tests/license_headers.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const UMBRELLA: &str = "akantu";
const GPL_CRATE: &str = "akantu-geometry-cgal";
/// Declared GPL switches (D-042): the only
/// `(crate, feature)` pairs that are allowed to (and must) make a build
/// GPL-3.0-or-later. Each stays out of every default feature set. Adding an
/// entry is a licensing decision of the project (`docs/DECISIONS.md`)
/// and must be mirrored in `NOTICE.md` and `docs/DEPENDENCIES.md`.
const GPL_SWITCHES: [(&str, &str); 3] = [
    (UMBRELLA, "cgal"),
    (UMBRELLA, "igfem"),
    ("akantu-igfem", "cgal"),
];

/// The declared switches of `krate`.
fn switches_of<'a>(switches: &[(&str, &'a str)], krate: &str) -> Vec<&'a str> {
    switches
        .iter()
        .filter(|(c, _)| *c == krate)
        .map(|(_, f)| *f)
        .collect()
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn lgpl_crate_has_no_gpl_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rs_files(&root.join("src"), &mut files);
    rs_files(&root.join("tests"), &mut files);
    assert!(files.len() >= 10);
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(
            text.starts_with("// SPDX-License-Identifier: LGPL-3.0-or-later\n"),
            "{} must be LGPL-3.0-or-later",
            f.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Workspace model from `cargo metadata --no-deps`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Normal,
    Build,
    Dev,
}

#[derive(Debug, Clone)]
struct Dep {
    /// Package name (what Cargo resolves, whatever the dependency is called).
    package: String,
    /// Name used in the manifest's feature table: `rename` or the package.
    key: String,
    kind: Kind,
    optional: bool,
    default_features: bool,
    features: Vec<String>,
    target: Option<String>,
}

#[derive(Debug, Clone)]
struct Member {
    license: String,
    features: BTreeMap<String, Vec<String>>,
    deps: Vec<Dep>,
}

struct Workspace {
    root: PathBuf,
    members: BTreeMap<String, Member>,
}

fn cargo() -> Command {
    // `CARGO` is set by cargo for the processes it runs (and at compile
    // time); it is the cargo that is running this test.
    let bin = std::env::var_os("CARGO").unwrap_or_else(|| env!("CARGO").into());
    Command::new(bin)
}

fn run(cmd: &mut Command) -> String {
    let out = cmd
        .output()
        .unwrap_or_else(|e| panic!("cannot run {cmd:?}: {e}"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "{cmd:?} failed ({}):\n{stderr}{}",
        out.status,
        offline_hint(&stderr)
    );
    String::from_utf8(out.stdout).expect("cargo output is UTF-8")
}

/// `cargo tree --offline` needs every package of the feature set in the
/// local registry, including those the test build itself never downloaded
/// (`mpi`, `python`): the CI tiers run `cargo fetch --locked` first
/// (docs/TESTING.md). The check is not skipped.
fn offline_hint(stderr: &str) -> &'static str {
    if stderr.contains("--offline was specified") {
        "\nhint: the cargo registry lacks a package of Cargo.lock that this feature set \
         needs; run `cargo fetch --locked` once (docs/TESTING.md, \"State at release\")"
    } else {
        ""
    }
}

fn workspace() -> Workspace {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf();
    let json = run(cargo()
        .current_dir(&root)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--offline",
        ])
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml")));
    let meta: Value = serde_json::from_str(&json).expect("cargo metadata JSON");
    let ids: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .expect("workspace_members")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let mut members = BTreeMap::new();
    for p in meta["packages"].as_array().expect("packages") {
        if !ids.contains(p["id"].as_str().unwrap()) {
            continue;
        }
        let strings = |v: &Value| -> Vec<String> {
            v.as_array()
                .unwrap()
                .iter()
                .map(|s| s.as_str().unwrap().to_string())
                .collect()
        };
        let features = p["features"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), strings(v)))
            .collect();
        let deps = p["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                let package = d["name"].as_str().unwrap().to_string();
                Dep {
                    key: d["rename"].as_str().map_or(package.clone(), str::to_string),
                    package,
                    kind: match d["kind"].as_str() {
                        None => Kind::Normal,
                        Some("build") => Kind::Build,
                        Some("dev") => Kind::Dev,
                        Some(k) => panic!("unknown dependency kind {k}"),
                    },
                    optional: d["optional"].as_bool().unwrap(),
                    default_features: d["uses_default_features"].as_bool().unwrap(),
                    features: strings(&d["features"]),
                    target: d["target"].as_str().map(str::to_string),
                }
            })
            .collect();
        members.insert(
            p["name"].as_str().unwrap().to_string(),
            Member {
                license: p["license"].as_str().unwrap_or("").to_string(),
                features,
                deps,
            },
        );
    }
    Workspace { root, members }
}

/// A licence expression that grants no LGPL-compatible alternative: some
/// `GPL-…`/`AGPL-…` identifier and no `OR`.
fn is_gpl(license: &str) -> bool {
    let tokens: Vec<&str> = license
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '/')
        .filter(|t| !t.is_empty())
        .collect();
    tokens
        .iter()
        .any(|t| t.starts_with("GPL-") || t.starts_with("AGPL-"))
        && !tokens.contains(&"OR")
}

impl Workspace {
    fn gpl_members(&self) -> BTreeSet<String> {
        self.members
            .iter()
            .filter(|(_, m)| is_gpl(&m.license))
            .map(|(n, _)| n.clone())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Feature resolution over the workspace members.

/// Requested features of the root package (`cargo … -p ROOT`).
#[derive(Debug, Clone)]
struct Request {
    features: Vec<String>,
    default: bool,
    all_features: bool,
}

impl Request {
    fn default_features() -> Self {
        Self {
            features: Vec::new(),
            default: true,
            all_features: false,
        }
    }
    fn only<S: AsRef<str>>(features: &[S]) -> Self {
        Self {
            features: features.iter().map(|f| f.as_ref().to_string()).collect(),
            default: false,
            all_features: false,
        }
    }
    fn all() -> Self {
        Self {
            features: Vec::new(),
            default: true,
            all_features: true,
        }
    }
    /// `--all-features` minus the features in `excluded`: the default set
    /// plus every other feature of `m`, implicit features of optional
    /// dependencies included (`cargo metadata` lists them as
    /// `"dep": ["dep:dep"]`; an optional dependency that no feature names is
    /// added as well, to stay conservative).
    fn all_except(m: &Member, excluded: &[&str]) -> Self {
        let mut features: Vec<String> = m
            .features
            .keys()
            .filter(|f| *f != "default" && !excluded.contains(&f.as_str()))
            .cloned()
            .collect();
        for d in m.deps.iter().filter(|d| d.optional) {
            let named = m.features.contains_key(&d.key)
                || m.features
                    .values()
                    .flatten()
                    .any(|v| v.strip_prefix("dep:") == Some(d.key.as_str()));
            if !named && !excluded.contains(&d.key.as_str()) && !features.contains(&d.key) {
                features.push(d.key.clone());
            }
        }
        Self {
            features,
            default: true,
            all_features: false,
        }
    }
    /// The same request as `cargo tree` arguments.
    fn cargo_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        if self.all_features {
            a.push("--all-features".to_string());
        } else {
            if !self.default {
                a.push("--no-default-features".to_string());
            }
            if !self.features.is_empty() {
                a.push("--features".to_string());
                a.push(self.features.join(","));
            }
        }
        a
    }
}

#[derive(Debug, Default)]
struct Resolution {
    /// Active member -> the member that activated it (root -> itself).
    parent: BTreeMap<String, String>,
    /// Active features per member.
    features: BTreeMap<String, BTreeSet<String>>,
}

impl Resolution {
    fn path_to(&self, pkg: &str) -> String {
        let mut path = vec![pkg.to_string()];
        let mut cur = pkg;
        while let Some(p) = self.parent.get(cur) {
            if p == cur {
                break;
            }
            path.push(p.clone());
            cur = p;
        }
        path.reverse();
        path.join(" -> ")
    }
}

enum Work {
    Feature(String, String),
    Activate(String, usize),
}

fn resolve(ws: &Workspace, root: &str, req: &Request) -> Resolution {
    let mut res = Resolution::default();
    let mut active_deps: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut weak: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut work: Vec<Work> = Vec::new();
    let allowed = |pkg: &str, kind: Kind| kind != Kind::Dev || pkg == root;
    let member = |p: &str| ws.members.get(p);

    let m_root = member(root).unwrap_or_else(|| panic!("{root} is not a workspace member"));
    res.parent.insert(root.to_string(), root.to_string());
    res.features.entry(root.to_string()).or_default();
    for (i, d) in m_root.deps.iter().enumerate() {
        if !d.optional || req.all_features {
            work.push(Work::Activate(root.to_string(), i));
        }
    }
    for f in &req.features {
        assert!(
            m_root.features.contains_key(f)
                || m_root.deps.iter().any(|d| d.optional && &d.key == f),
            "{root} has no feature `{f}`"
        );
        work.push(Work::Feature(root.to_string(), f.clone()));
    }
    if req.default {
        work.push(Work::Feature(root.to_string(), "default".to_string()));
    }
    if req.all_features {
        for f in m_root.features.keys() {
            work.push(Work::Feature(root.to_string(), f.clone()));
        }
    }

    while let Some(item) = work.pop() {
        match item {
            Work::Activate(pkg, i) => {
                let d = &ws.members[&pkg].deps[i];
                if !allowed(&pkg, d.kind) || !active_deps.entry(pkg.clone()).or_default().insert(i)
                {
                    continue;
                }
                let Some(target) = member(&d.package) else {
                    continue; // third-party crate
                };
                if !res.parent.contains_key(&d.package) {
                    res.parent.insert(d.package.clone(), pkg.clone());
                    res.features.entry(d.package.clone()).or_default();
                    for (j, dd) in target.deps.iter().enumerate() {
                        if !dd.optional {
                            work.push(Work::Activate(d.package.clone(), j));
                        }
                    }
                }
                for f in &d.features {
                    work.push(Work::Feature(d.package.clone(), f.clone()));
                }
                if d.default_features {
                    work.push(Work::Feature(d.package.clone(), "default".to_string()));
                }
                for (key, f) in weak.get(&pkg).cloned().unwrap_or_default() {
                    if key == d.key {
                        work.push(Work::Feature(d.package.clone(), f));
                    }
                }
            }
            Work::Feature(pkg, feat) => {
                if !res
                    .features
                    .entry(pkg.clone())
                    .or_default()
                    .insert(feat.clone())
                {
                    continue;
                }
                let m = &ws.members[&pkg];
                let deps_named = |key: &str| -> Vec<usize> {
                    m.deps
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| d.key == key && allowed(&pkg, d.kind))
                        .map(|(i, _)| i)
                        .collect()
                };
                let Some(values) = m.features.get(&feat) else {
                    // Implicit feature of an optional dependency (no `dep:`
                    // anywhere), or `default` of a crate without one.
                    for i in deps_named(&feat) {
                        work.push(Work::Activate(pkg.clone(), i));
                    }
                    continue;
                };
                for v in values {
                    if let Some(dep) = v.strip_prefix("dep:") {
                        for i in deps_named(dep) {
                            work.push(Work::Activate(pkg.clone(), i));
                        }
                    } else if let Some((dep, f)) = v.split_once("?/") {
                        weak.entry(pkg.clone())
                            .or_default()
                            .push((dep.to_string(), f.to_string()));
                        for i in deps_named(dep) {
                            if active_deps.get(&pkg).is_some_and(|s| s.contains(&i)) {
                                work.push(Work::Feature(m.deps[i].package.clone(), f.to_string()));
                            }
                        }
                    } else if let Some((dep, f)) = v.split_once('/') {
                        if m.features.contains_key(dep) {
                            work.push(Work::Feature(pkg.clone(), dep.to_string()));
                        }
                        for i in deps_named(dep) {
                            work.push(Work::Activate(pkg.clone(), i));
                            work.push(Work::Feature(m.deps[i].package.clone(), f.to_string()));
                        }
                    } else {
                        work.push(Work::Feature(pkg.clone(), v.clone()));
                    }
                }
            }
        }
    }
    res
}

/// Workspace members in `cargo tree -p ROOT -e normal,build,dev` (host
/// target), resolved by Cargo itself.
fn cargo_tree_members(ws: &Workspace, root: &str, req: &Request) -> BTreeSet<String> {
    let out = run(cargo()
        .current_dir(&ws.root)
        .args(["tree", "--offline", "-q", "-e", "normal,build,dev"])
        .args(["--prefix", "none", "-f", "{p}", "-p", root])
        .args(req.cargo_args())
        .arg("--manifest-path")
        .arg(ws.root.join("Cargo.toml")));
    let names: BTreeSet<String> = out
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('['))
        .map(|l| l.split_whitespace().next().unwrap().to_string())
        .collect();
    assert!(
        names.contains(root),
        "cargo tree output without {root}:\n{out}"
    );
    names
        .into_iter()
        .filter(|n| ws.members.contains_key(n))
        .collect()
}

/// GPL members reachable for `req`, with a path for each (resolver and, if
/// asked, Cargo's own `cargo tree`).
fn gpl_reached(ws: &Workspace, root: &str, req: &Request, with_cargo_tree: bool) -> Vec<String> {
    let gpl = ws.gpl_members();
    let res = resolve(ws, root, req);
    let mut found: Vec<String> = gpl
        .iter()
        .filter(|g| res.parent.contains_key(*g))
        .map(|g| format!("{root} {:?}: {}", req.cargo_args(), res.path_to(g)))
        .collect();
    if with_cargo_tree {
        let tree = cargo_tree_members(ws, root, req);
        let own: BTreeSet<String> = res.parent.keys().cloned().collect();
        let missed: Vec<&String> = tree.difference(&own).collect();
        assert!(
            missed.is_empty(),
            "resolver unsound for {root} {:?}: cargo tree reaches {missed:?}",
            req.cargo_args()
        );
        for g in gpl.intersection(&tree) {
            found.push(format!(
                "{root} {:?}: cargo tree reaches {g}",
                req.cargo_args()
            ));
        }
    }
    found
}

// ---------------------------------------------------------------------------
// The declared-switch rule. Each check returns the list of
// violations, so that it can also be run on hand-made workspaces.

/// LGPL members, by name.
fn lgpl_members(ws: &Workspace) -> Vec<&str> {
    ws.members
        .iter()
        .filter(|(_, m)| !is_gpl(&m.license))
        .map(|(n, _)| n.as_str())
        .collect()
}

/// (a) Every declared switch is a feature of an LGPL member and reaches a
/// GPL member (resolver; the real test adds `cargo tree`).
fn check_switches_are_declared_and_live(ws: &Workspace, switches: &[(&str, &str)]) -> Vec<String> {
    let mut problems = Vec::new();
    for (krate, feature) in switches {
        let Some(m) = ws.members.get(*krate) else {
            problems.push(format!(
                "switch {krate}/{feature}: {krate} is not a workspace member"
            ));
            continue;
        };
        if is_gpl(&m.license) {
            problems.push(format!(
                "switch {krate}/{feature}: {krate} is not an LGPL crate"
            ));
        }
        if *feature == "default" || !m.features.contains_key(*feature) {
            problems.push(format!(
                "switch {krate}/{feature}: no such (non-default) feature"
            ));
            continue;
        }
        if gpl_reached(ws, krate, &Request::only(&[feature]), false).is_empty() {
            problems.push(format!(
                "switch {krate}/{feature} reaches no GPL member (stale allow-list entry)"
            ));
        }
    }
    problems
}

/// (b) No default feature set of an LGPL member reaches a GPL member or
/// activates a declared switch (its own crate's `default`, or the default
/// features of any crate that depends on it, directly or transitively).
fn check_no_default_reaches_gpl_or_a_switch(
    ws: &Workspace,
    switches: &[(&str, &str)],
) -> Vec<String> {
    let gpl = ws.gpl_members();
    let mut problems = Vec::new();
    for name in lgpl_members(ws) {
        let res = resolve(ws, name, &Request::default_features());
        for g in gpl.iter().filter(|g| res.parent.contains_key(*g)) {
            problems.push(format!("{name} (default features): {}", res.path_to(g)));
        }
        for (krate, feature) in switches {
            if res
                .features
                .get(*krate)
                .is_some_and(|f| f.contains(*feature))
            {
                problems.push(format!(
                    "{name} (default features) enables the GPL switch {krate}/{feature} ({})",
                    res.path_to(krate)
                ));
            }
        }
    }
    problems
}

/// (c) Every LGPL member with all its features except its own declared
/// switches (`--all-features` when it has none), dev- and
/// build-dependencies included, reaches no GPL member. Returns the
/// violations and the number of crates checked.
fn check_all_but_own_switches_never_reach_gpl(
    ws: &Workspace,
    switches: &[(&str, &str)],
) -> (Vec<String>, usize) {
    let mut problems = Vec::new();
    let mut checked = 0;
    for name in lgpl_members(ws) {
        let own = switches_of(switches, name);
        let req = if own.is_empty() {
            Request::all()
        } else {
            Request::all_except(&ws.members[name], &own)
        };
        problems.extend(gpl_reached(ws, name, &req, false));
        checked += 1;
    }
    (problems, checked)
}

#[test]
fn gpl_crate_is_declared_gpl() {
    let ws = workspace();
    let m = ws
        .members
        .get(GPL_CRATE)
        .expect("the GPL crate is a member");
    assert_eq!(m.license, "GPL-3.0-or-later");
    assert!(ws.gpl_members().contains(GPL_CRATE));
    assert!(!ws.gpl_members().contains(UMBRELLA));
    assert!(!is_gpl("LGPL-3.0-or-later"));
    assert!(!is_gpl("LGPL-3.0-or-later AND MPL-2.0"));
    assert!(!is_gpl("MIT OR GPL-2.0-or-later"));
    assert!(is_gpl("GPL-3.0-or-later"));
    assert!(is_gpl("GPL-3.0-or-later AND MIT"));
}

/// Rule (a) with `cargo tree` agreement, and rule (b).
#[test]
fn declared_gpl_switches_are_live_and_never_on_by_default() {
    let ws = workspace();
    let mut problems = check_switches_are_declared_and_live(&ws, &GPL_SWITCHES);
    for (krate, feature) in GPL_SWITCHES {
        if !ws
            .members
            .get(krate)
            .is_some_and(|m| m.features.contains_key(feature))
        {
            continue; // reported above
        }
        let reached = gpl_reached(&ws, krate, &Request::only(&[feature]), true);
        if !reached
            .iter()
            .any(|r| r.contains(GPL_CRATE) && r.contains("cargo tree"))
        {
            problems.push(format!(
                "switch {krate}/{feature}: cargo tree does not reach {GPL_CRATE}: {reached:?}"
            ));
        }
    }
    problems.extend(check_no_default_reaches_gpl_or_a_switch(&ws, &GPL_SWITCHES));
    assert!(
        problems.is_empty(),
        "D-042: declared GPL switches {GPL_SWITCHES:?}:\n{}",
        problems.join("\n")
    );
}

/// Rule (c).
#[test]
fn every_lgpl_crate_without_its_declared_gpl_switches_never_reaches_gpl() {
    let ws = workspace();
    let (problems, checked) = check_all_but_own_switches_never_reach_gpl(&ws, &GPL_SWITCHES);
    assert!(checked >= 20, "only {checked} LGPL crates checked");
    assert!(
        problems.is_empty(),
        "D-042: an LGPL crate (all features except its declared GPL switches {GPL_SWITCHES:?}, \
         normal/build/dev dependencies) reaches a GPL crate:\n{}",
        problems.join("\n")
    );
}

/// `Request::all_except(m, &[])` is `--all-features` for every member (same
/// packages, same features): rule (c) only ever removes the declared
/// switches.
#[test]
fn all_features_except_nothing_is_all_features() {
    let ws = workspace();
    for (name, m) in &ws.members {
        let all = resolve(&ws, name, &Request::all());
        let except = resolve(&ws, name, &Request::all_except(m, &[]));
        let keys = |r: &Resolution| r.parent.keys().cloned().collect::<BTreeSet<_>>();
        assert_eq!(keys(&all), keys(&except), "{name}: packages");
        assert_eq!(all.features, except.features, "{name}: features");
    }
}

/// Rule (d), feature sets: the umbrella with `cargo tree` agreement.
#[test]
fn default_and_lgpl_feature_sets_of_the_umbrella_never_reach_gpl() {
    let ws = workspace();
    let umbrella = &ws.members[UMBRELLA];
    let switches = switches_of(&GPL_SWITCHES, UMBRELLA);
    let lgpl_features: Vec<&str> = umbrella
        .features
        .keys()
        .map(String::as_str)
        .filter(|f| *f != "default" && !switches.contains(f))
        .collect();
    assert!(lgpl_features.contains(&"embedded"));
    let mut requests = vec![
        Request::default_features(),
        Request::only(&lgpl_features),
        Request::all_except(umbrella, &switches),
    ];
    requests.extend(lgpl_features.iter().map(|f| Request::only(&[f])));
    let mut found = Vec::new();
    for req in &requests {
        found.extend(gpl_reached(&ws, UMBRELLA, req, true));
    }
    assert!(
        found.is_empty(),
        "D-042: an LGPL feature set of the umbrella reaches a GPL crate:\n{}",
        found.join("\n")
    );
}

/// Rule (d), project policy.
#[test]
fn umbrella_policy_igfem_implies_cgal_and_embedded_stays_lgpl() {
    let ws = workspace();
    assert_eq!(
        switches_of(&GPL_SWITCHES, UMBRELLA),
        ["cgal", "igfem"],
        "umbrella switches"
    );
    let default = resolve(&ws, UMBRELLA, &Request::default_features());
    let active = &default.features[UMBRELLA];
    let igfem = resolve(&ws, UMBRELLA, &Request::only(&["igfem"]));
    assert!(
        igfem.features[UMBRELLA].contains("cgal"),
        "`igfem` must imply `cgal`"
    );
    assert!(
        igfem
            .features
            .get("akantu-igfem")
            .is_some_and(|f| f.contains("cgal")),
        "`igfem` must enable `akantu-igfem/cgal` (mesh generation)"
    );
    assert!(
        active.contains("embedded"),
        "`embedded` must stay in `default`"
    );
    let embedded = resolve(&ws, UMBRELLA, &Request::only(&["embedded"]));
    assert!(
        !embedded.features[UMBRELLA].contains("cgal"),
        "`embedded` must not imply `cgal`"
    );
}

/// The three checks on hand-made workspaces: the clean model passes, and each
/// bypass form fails at least one check (analogues of the mutations M1 to
/// M5 and of the three `IG01-*` forms of `license_separation_mutations.py`).
#[test]
fn switch_rule_rejects_every_bypass_form() {
    let dep = |package: &str, key: &str, kind: Kind, optional: bool, features: &[&str]| Dep {
        package: package.into(),
        key: key.into(),
        kind,
        optional,
        default_features: true,
        features: features.iter().map(|s| (*s).to_string()).collect(),
        target: None,
    };
    let set = |m: &mut Member, k: &str, vals: &[&str]| {
        m.features
            .insert(k.into(), vals.iter().map(|s| (*s).to_string()).collect());
    };
    let mk = |license: &str| Member {
        license: license.into(),
        features: BTreeMap::new(),
        deps: Vec::new(),
    };
    // gpl: the GPL crate. lib: like akantu-igfem (optional GPL dependency
    // behind its switch `cgal`). top: like the umbrella (switches `cgal`,
    // `g`; `e` in default). other: an unrelated LGPL crate.
    let mut base = BTreeMap::new();
    base.insert("gpl".to_string(), mk("GPL-3.0-or-later"));
    let mut lib = mk("LGPL-3.0-or-later");
    set(&mut lib, "cgal", &["dep:gpl"]);
    lib.deps.push(dep("gpl", "gpl", Kind::Normal, true, &[]));
    base.insert("lib".to_string(), lib);
    let mut top = mk("LGPL-3.0-or-later");
    set(&mut top, "default", &["e"]);
    set(&mut top, "e", &[]);
    set(&mut top, "cgal", &["dep:geo"]);
    set(&mut top, "g", &["cgal", "dep:lib", "lib/cgal"]);
    top.deps.push(dep("lib", "lib", Kind::Normal, true, &[]));
    top.deps.push(dep("gpl", "geo", Kind::Normal, true, &[]));
    base.insert("top".to_string(), top);
    base.insert("other".to_string(), mk("LGPL-3.0-or-later"));
    let switches: [(&str, &str); 3] = [("top", "cgal"), ("top", "g"), ("lib", "cgal")];

    let problems = |members: &BTreeMap<String, Member>, switches: &[(&str, &str)]| {
        let ws = Workspace {
            root: PathBuf::new(),
            members: members.clone(),
        };
        let mut p = check_switches_are_declared_and_live(&ws, switches);
        p.extend(check_no_default_reaches_gpl_or_a_switch(&ws, switches));
        p.extend(check_all_but_own_switches_never_reach_gpl(&ws, switches).0);
        p
    };
    assert_eq!(problems(&base, &switches), Vec::<String>::new());

    let mutated = |name: &str, f: &dyn Fn(&mut BTreeMap<String, Member>)| {
        let mut m = base.clone();
        f(&mut m);
        let p = problems(&m, &switches);
        assert!(!p.is_empty(), "bypass form `{name}` passes the rule");
        p
    };
    // Mutants M1–M3: an unconditional GPL dependency (inline, table, renamed).
    mutated("M1/M2 unconditional dependency", &|m| {
        m.get_mut("other")
            .unwrap()
            .deps
            .push(dep("gpl", "gpl", Kind::Normal, false, &[]));
    });
    mutated("M3 renamed unconditional dependency", &|m| {
        m.get_mut("other")
            .unwrap()
            .deps
            .push(dep("gpl", "geocgal", Kind::Normal, false, &[]));
    });
    mutated("unconditional dev-dependency", &|m| {
        m.get_mut("other")
            .unwrap()
            .deps
            .push(dep("gpl", "gpl", Kind::Dev, false, &[]));
    });
    mutated("target-specific build-dependency", &|m| {
        let mut d = dep("gpl", "gpl", Kind::Build, false, &[]);
        d.target = Some("cfg(windows)".into());
        m.get_mut("other").unwrap().deps.push(d);
    });
    // Mutant M4: a switch in the umbrella's default.
    let p = mutated("M4 switch in default", &|m| {
        set(m.get_mut("top").unwrap(), "default", &["e", "cgal"]);
    });
    assert!(
        p.iter()
            .any(|s| s.contains("enables the GPL switch top/cgal"))
    );
    // Mutant M5: an LGPL feature implies a switch (`embedded = [.., "cgal"]`).
    mutated("M5 unlisted feature implies a switch", &|m| {
        set(m.get_mut("top").unwrap(), "e", &["cgal"]);
    });
    // A listed feature turned on by default (own crate).
    let p = mutated("listed feature on by default", &|m| {
        set(m.get_mut("lib").unwrap(), "default", &["cgal"]);
    });
    assert!(
        p.iter()
            .any(|s| s.contains("lib (default features) enables the GPL switch lib/cgal"))
    );
    // A second, unlisted optional path — in the same crate …
    mutated("second path, same crate", &|m| {
        set(m.get_mut("lib").unwrap(), "gel", &["cgal"]);
    });
    // … through a new optional dependency of another crate …
    mutated("second path, other crate", &|m| {
        let o = m.get_mut("other").unwrap();
        o.deps.push(dep("gpl", "gpl", Kind::Normal, true, &[]));
        set(o, "aabb", &["dep:gpl"]);
    });
    // … or as an implicit feature (no `dep:` anywhere).
    mutated("second path, implicit feature", &|m| {
        m.get_mut("other")
            .unwrap()
            .deps
            .push(dep("gpl", "gpl", Kind::Normal, true, &[]));
    });
    // … or an unlisted feature of the umbrella forwarding to lib's switch.
    mutated("second path, forwarded switch", &|m| {
        set(m.get_mut("top").unwrap(), "gel", &["lib/cgal"]);
    });
    // A listed feature enabled transitively by another crate's
    // default — through a default feature …
    let p = mutated("switch enabled by another crate's default", &|m| {
        let o = m.get_mut("other").unwrap();
        o.deps.push(dep("lib", "lib", Kind::Normal, true, &[]));
        set(o, "default", &["igfem-gel"]);
        set(o, "igfem-gel", &["dep:lib", "lib/cgal"]);
    });
    assert!(
        p.iter()
            .any(|s| s.contains("other (default features) enables the GPL switch lib/cgal"))
    );
    // … through a dependency declaration …
    mutated("switch enabled by a dependency declaration", &|m| {
        m.get_mut("other")
            .unwrap()
            .deps
            .push(dep("lib", "lib", Kind::Normal, false, &["cgal"]));
    });
    // … or through the umbrella's default turning on its own switch `g`.
    let p = mutated(
        "switch enabled transitively by the umbrella's default",
        &|m| {
            set(m.get_mut("top").unwrap(), "default", &["e", "g"]);
        },
    );
    assert!(
        p.iter()
            .any(|s| s.contains("top (default features) enables the GPL switch lib/cgal"))
    );

    // The allow-list itself: an unlisted switch fails, a stale or bogus
    // entry fails.
    assert!(!problems(&base, &[("top", "cgal"), ("top", "g")]).is_empty());
    assert!(
        !problems(
            &base,
            &[("top", "cgal"), ("top", "g"), ("lib", "cgal"), ("top", "e")]
        )
        .is_empty()
    );
    assert!(
        !problems(
            &base,
            &[
                ("top", "cgal"),
                ("top", "g"),
                ("lib", "cgal"),
                ("lib", "nope")
            ]
        )
        .is_empty()
    );
    assert!(
        !problems(
            &base,
            &[("top", "cgal"), ("top", "g"), ("lib", "cgal"), ("gpl", "x")]
        )
        .is_empty()
    );
    assert!(
        !problems(
            &base,
            &[
                ("top", "cgal"),
                ("top", "g"),
                ("lib", "cgal"),
                ("top", "default")
            ]
        )
        .is_empty()
    );
}

/// The resolver against hand-made manifests: every way of declaring a
/// dependency that the mutations M1 to M5 use, plus target tables,
/// weak features and `dep/feature` forwarding.
#[test]
fn resolver_follows_cargo_feature_rules() {
    let dep = |package: &str, key: &str, kind: Kind, optional: bool, features: &[&str]| Dep {
        package: package.into(),
        key: key.into(),
        kind,
        optional,
        default_features: true,
        features: features.iter().map(|s| (*s).to_string()).collect(),
        target: None,
    };
    let feats = |v: &[(&str, &[&str])]| -> BTreeMap<String, Vec<String>> {
        v.iter()
            .map(|(k, vals)| {
                (
                    (*k).to_string(),
                    vals.iter().map(|s| (*s).to_string()).collect(),
                )
            })
            .collect()
    };
    let mk = |license: &str, features, deps| Member {
        license: license.into(),
        features,
        deps,
    };
    let mut members = BTreeMap::new();
    members.insert(
        "gpl".to_string(),
        mk("GPL-3.0-or-later", feats(&[]), vec![]),
    );
    members.insert(
        "emb".to_string(),
        mk(
            "LGPL-3.0-or-later",
            feats(&[("cgal", &["dep:geo"]), ("default", &[])]),
            vec![dep("gpl", "geo", Kind::Normal, true, &[])],
        ),
    );
    members.insert(
        "devonly".to_string(),
        mk(
            "LGPL-3.0-or-later",
            feats(&[]),
            vec![dep("gpl", "gpl", Kind::Dev, false, &[])],
        ),
    );
    members.insert(
        "targeted".to_string(),
        mk("LGPL-3.0-or-later", feats(&[]), {
            let mut d = dep("gpl", "gpl", Kind::Build, false, &[]);
            d.target = Some("cfg(windows)".into());
            vec![d]
        }),
    );
    members.insert(
        "implicit".to_string(),
        mk(
            "LGPL-3.0-or-later",
            feats(&[("default", &["gpl"])]),
            vec![dep("gpl", "gpl", Kind::Normal, true, &[])],
        ),
    );
    members.insert(
        "top".to_string(),
        mk(
            "LGPL-3.0-or-later",
            feats(&[
                ("default", &["e"]),
                ("e", &["dep:emb"]),
                ("weak", &["emb?/cgal"]),
                ("fwd", &["emb/cgal"]),
                ("dev", &["dep:devonly"]),
            ]),
            vec![
                dep("emb", "emb", Kind::Normal, true, &[]),
                dep("devonly", "devonly", Kind::Normal, true, &[]),
            ],
        ),
    );
    let ws = Workspace {
        root: PathBuf::new(),
        members,
    };
    let reaches = |root: &str, req: Request| resolve(&ws, root, &req).parent.contains_key("gpl");
    assert!(!reaches("top", Request::default_features()));
    assert!(
        !reaches("top", Request::only(&["weak"])),
        "weak feature alone"
    );
    assert!(
        reaches("top", Request::only(&["weak", "e"])),
        "weak feature + dependency"
    );
    assert!(
        reaches("top", Request::only(&["fwd"])),
        "dep/feature activates the dependency"
    );
    assert!(reaches("top", Request::all()));
    assert!(
        !reaches("top", Request::only(&["dev"])),
        "dev-dependencies of a dependency"
    );
    assert!(
        reaches("devonly", Request::default_features()),
        "dev-dependencies of the root"
    );
    assert!(
        reaches("targeted", Request::default_features()),
        "target tables count"
    );
    assert!(
        reaches("implicit", Request::default_features()),
        "implicit feature"
    );
    assert!(!reaches("implicit", Request::only::<&str>(&[])));
    assert!(!reaches("emb", Request::default_features()));
    assert!(
        reaches("emb", Request::all()),
        "renamed optional dependency under --all-features"
    );
}
