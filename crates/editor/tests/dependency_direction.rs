//! The dependency direction in `docs/specs/crates.md` §3, asserted.
//!
//! §3 says a `use` against this direction is a compile error. Cargo delivers
//! half of that for free and the half it delivers is the upward edges:
//! `runtime -> editor`, `data -> runtime` and `core -> data` are cycles and it
//! refuses them. The sideways edges are not cycles and it accepts them, so
//! `editor_ui -> data` compiles, and `docs/specs/architecture.md` §6, which
//! rests on the widgets not knowing the model, quietly stops holding.
//!
//! These tests live here rather than at the workspace root because a virtual
//! workspace has no root package: a `tests/` directory beside `Cargo.toml` is
//! not an error, it is **silently ignored**, and `cargo test --workspace` runs
//! nothing from it. `editor` is the top of the graph and the only crate that
//! depends on every other, so reading the whole workspace from here is not a
//! reach.
//!
//! # Why `cargo metadata` and not the manifests
//!
//! A dependency declared under a target table is invisible to a scan of the
//! top-level `[dependencies]` table and visible to metadata:
//!
//! ```toml
//! [target.'cfg(windows)'.dependencies]
//! log = "0.4"
//! ```
//!
//! A guard with a hole that size stays green while the thing it guards is gone,
//! which is the one failure a guard must not have.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;

/// The internal dependencies each crate is allowed, from
/// `docs/specs/crates.md` §3.
///
/// Written out rather than computed. A test that asks the code what to expect
/// agrees with it however wrong both are.
const EXPECTED: &[(&str, &[&str])] = &[
    ("b2d_core", &[]),
    ("b2d_data", &["b2d_core"]),
    ("b2d_runtime", &["b2d_core", "b2d_data"]),
    ("b2d_editor_ui", &[]),
    (
        "b2d_editor",
        &["b2d_core", "b2d_data", "b2d_editor_ui", "b2d_runtime"],
    ),
];

/// The workspace as cargo resolves it, members only.
///
/// Panics carrying cargo's own message when cargo refuses to produce it, which
/// is what happens the moment somebody adds an upward edge: a cycle stops
/// metadata existing, so every test in this file fails at this point and says
/// `cyclic package dependency`.
fn metadata() -> Value {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata should run");
    assert!(
        out.status.success(),
        "cargo metadata failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("cargo metadata should be JSON")
}

/// Every workspace member, as `(name, manifest_path, dependency names)`.
///
/// Dev-dependencies are skipped: one never reaches a consumer of a published
/// crate, and this file is itself the reason `b2d_editor` has one. Build
/// dependencies are kept, because those do reach a consumer's build.
fn members(meta: &Value) -> Vec<(String, String, BTreeSet<String>)> {
    meta["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| {
            let deps = p["dependencies"]
                .as_array()
                .expect("dependencies")
                .iter()
                .filter(|d| d["kind"].as_str() != Some("dev"))
                .map(|d| d["name"].as_str().expect("name").to_owned())
                .collect();
            (
                p["name"].as_str().expect("name").to_owned(),
                p["manifest_path"]
                    .as_str()
                    .expect("manifest_path")
                    .to_owned(),
                deps,
            )
        })
        .collect()
}

/// Whether a manifest sits under `crates/`, whichever separator this platform
/// spells a path with.
fn is_under_crates(manifest_path: &str) -> bool {
    let p = manifest_path.replace('\\', "/");
    p.contains("/crates/")
}

/// Each crate depends on exactly the crates `docs/specs/crates.md` §3 allows it.
///
/// Equality rather than containment, so an edge that should be there and is not
/// fails too. That is what guards the three upward edges from the other side:
/// adding one stops `cargo metadata` before this runs, and removing one lands
/// here.
///
/// Mutation: add `b2d_data.workspace = true` to `crates/editor_ui/Cargo.toml`,
/// or remove `b2d_core` from `crates/data/Cargo.toml`. Either fails this test
/// and nothing else.
#[test]
fn the_internal_dependency_graph_is_what_the_specification_says() {
    let meta = metadata();
    for (name, _, deps) in members(&meta) {
        let Some((_, allowed)) = EXPECTED.iter().find(|(n, _)| *n == name) else {
            continue; // a package outside the graph is the next test's business
        };
        let internal: BTreeSet<&str> = deps
            .iter()
            .map(String::as_str)
            .filter(|d| d.starts_with("b2d_"))
            .collect();
        let allowed: BTreeSet<&str> = allowed.iter().copied().collect();
        assert_eq!(
            internal, allowed,
            "{name} depends on the wrong set of crates in this workspace. \
             docs/specs/crates.md §3 fixes the direction"
        );
    }
}

/// `b2d_core` depends on nothing, internal or external.
///
/// §3 says "nothing, not even Bevy". Being reachable without a renderer is the
/// crate's reason to exist, and one dependency is what takes that away.
///
/// Mutation: add `log = "0.4"` to `crates/core/Cargo.toml`, under
/// `[dependencies]` or under `[target.'cfg(windows)'.dependencies]`. Both fail
/// this test and nothing else; the second is the one a manifest scan misses.
#[test]
fn core_depends_on_nothing_at_all() {
    let meta = metadata();
    let (_, _, deps) = members(&meta)
        .into_iter()
        .find(|(n, _, _)| n == "b2d_core")
        .expect("b2d_core is a workspace member");
    assert!(
        deps.is_empty(),
        "b2d_core depends on {deps:?}, and docs/specs/crates.md §3 says it \
         depends on nothing, not even Bevy"
    );
}

/// Every crate under `crates/` has a row in the graph.
///
/// `docs/specs/crates.md` §3 ends by saying that anything with no place in the
/// graph is not a crate yet. This is where that stops being advice.
///
/// The second assertion is what stops this test passing vacuously. The loop
/// skips anything outside `crates/`, so an `is_under_crates` that answers
/// `false` for everything would skip every package and report success having
/// examined nothing.
///
/// Mutation: add a sixth crate under `crates/` and put it in the workspace, and
/// the first assertion fails. Break `is_under_crates`, and the second does.
/// Neither fails anything else.
#[test]
fn every_package_under_crates_has_a_place_in_the_graph() {
    let meta = metadata();
    let mut seen = BTreeSet::new();
    for (name, manifest, _) in members(&meta) {
        if !is_under_crates(&manifest) {
            continue;
        }
        assert!(
            EXPECTED.iter().any(|(n, _)| *n == name),
            "{name} sits under crates/ and has no row in docs/specs/crates.md §3. \
             Decide where it goes in the graph, or it is not a crate yet"
        );
        seen.insert(name);
    }
    let expected: BTreeSet<String> = EXPECTED.iter().map(|(n, _)| (*n).to_owned()).collect();
    assert_eq!(
        seen, expected,
        "this test examined the wrong set of crates, so whatever it reported is \
         about something other than the workspace"
    );
}

/// A dev-dependency is not a dependency for this purpose.
///
/// `b2d_editor` has one, `serde_json`, and this file is the reason for it. If
/// dev-dependencies counted, the crate holding the guard would be the first
/// thing the guard complained about, and the fix would be to weaken the guard.
///
/// Mutation: remove the `kind != dev` filter in `members`. This test fails and
/// nothing else, which is the point: without it that filter is unguarded.
#[test]
fn a_dev_dependency_is_not_a_dependency() {
    let meta = metadata();
    let (_, _, deps) = members(&meta)
        .into_iter()
        .find(|(n, _, _)| n == "b2d_editor")
        .expect("b2d_editor is a workspace member");
    assert!(
        !deps.contains("serde_json"),
        "b2d_editor's dev-dependency on serde_json reached the graph check"
    );
}

/// A manifest path is read the same way whichever separator the platform uses.
///
/// Windows spells `manifest_path` with backslashes, so a check written against
/// `/crates/` alone answers `false` for every package on the machine this is
/// developed on and `true` for every package on Linux CI. The test above would
/// pass on both while guarding nothing on one of them.
///
/// Mutation: drop the `replace` in `is_under_crates`. This test fails, and so
/// does the vacuity assertion above.
#[test]
fn a_manifest_path_is_read_the_same_way_on_every_platform() {
    assert!(is_under_crates("/w/crates/core/Cargo.toml"));
    assert!(is_under_crates(r"D:\w\crates\core\Cargo.toml"));
    assert!(!is_under_crates("/w/xtask/Cargo.toml"));
    assert!(!is_under_crates(r"D:\w\xtask\Cargo.toml"));
}
