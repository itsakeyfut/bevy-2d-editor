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

/// Whether a dependency is hard or sits behind a feature.
///
/// Spelled out rather than a `bool`, because `("avian2d", true)` in the table
/// below does not say which way round `true` runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Optionality {
    Required,
    Optional,
}

/// A dependency, as far as these tests care: what it is called and whether it
/// is optional.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Dep {
    name: String,
    optionality: Optionality,
}

/// The external dependencies each crate a user's game reaches is allowed, from
/// `docs/specs/crates.md` §3.
///
/// `EXPECTED` above holds the direction; this holds the weight. A game reaches
/// `b2d_runtime` and, through it, `b2d_data` and `b2d_core`, so those three are
/// the whole of what a user's build carries from this workspace. `b2d_editor`
/// and `b2d_editor_ui` are deliberately absent: nothing a game compiles reaches
/// them, and a list there would have to be edited every time the editor gained
/// a dependency while buying none of §2's contract about what a consumer pulls
/// in. What that costs is that an external dependency added to either is held
/// by nothing.
///
/// Every list is empty today. That is the reason for writing it now rather than
/// later: §3 has `bevy` arriving in `data` and `runtime`, and
/// `docs/specs/level-editor.md` §3 has `avian2d` arriving behind a default-on
/// feature, and a list agreed while it is empty costs one line each.
///
/// What this list does **not** hold is the contents of the `default` feature.
/// Taking `avian2d` out of `default` changes what a game carries and passes
/// here. No crate in the workspace has a `[features]` table yet, so holding it
/// now would mean a mechanism with no subject; the issue that brings `avian2d`
/// in is where that gets decided, and it arrives with the first feature table.
/// Direct dependencies only, too: what `bevy` pulls in behind itself is not
/// this list's business.
const SHIPPED: &[(&str, &[(&str, Optionality)])] =
    &[("b2d_core", &[]), ("b2d_data", &[]), ("b2d_runtime", &[])];

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

/// The non-dev dependencies of one package, as `cargo metadata` reports them.
///
/// Dev-dependencies are skipped: one never reaches a consumer of a published
/// crate, and this file is itself the reason `b2d_editor` has one. Build
/// dependencies are kept, because those do reach a consumer's build.
///
/// A dependency declared under a target table is one entry like any other.
/// Metadata flattens the target tables, which is the reason this file reads
/// metadata rather than manifests, stated in the module comment above.
fn deps_of(pkg: &Value) -> BTreeSet<Dep> {
    pkg["dependencies"]
        .as_array()
        .expect("dependencies")
        .iter()
        .filter(|d| d["kind"].as_str() != Some("dev"))
        .map(|d| Dep {
            name: d["name"].as_str().expect("name").to_owned(),
            optionality: if d["optional"].as_bool() == Some(true) {
                Optionality::Optional
            } else {
                Optionality::Required
            },
        })
        .collect()
}

/// Every workspace member, as `(name, manifest_path, dependencies)`.
fn members(meta: &Value) -> Vec<(String, String, BTreeSet<Dep>)> {
    meta["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| {
            (
                p["name"].as_str().expect("name").to_owned(),
                p["manifest_path"]
                    .as_str()
                    .expect("manifest_path")
                    .to_owned(),
                deps_of(p),
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
/// "In this workspace" is decided by what `cargo metadata` calls a member, not
/// by the `b2d_` prefix. `docs/specs/crates.md` §4 is a naming rule and §3 is a
/// dependency rule, and a workspace member that does not carry the prefix,
/// `xtask` being the one that exists, would be invisible to a check keyed on
/// spelling.
///
/// Mutation: add `b2d_data.workspace = true` to `crates/editor_ui/Cargo.toml`,
/// remove `b2d_core` from `crates/data/Cargo.toml`, or add
/// `xtask = { path = "../../xtask" }` to `crates/runtime/Cargo.toml`. Each
/// fails this test and nothing else.
#[test]
fn the_internal_dependency_graph_is_what_the_specification_says() {
    let meta = metadata();
    let workspace: BTreeSet<String> = members(&meta).into_iter().map(|(n, _, _)| n).collect();
    for (name, _, deps) in members(&meta) {
        let Some((_, allowed)) = EXPECTED.iter().find(|(n, _)| *n == name) else {
            continue; // a package outside the graph is the next test's business
        };
        let internal: BTreeSet<&str> = deps
            .iter()
            .map(|d| d.name.as_str())
            .filter(|d| workspace.contains(*d))
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
        !deps.iter().any(|d| d.name == "serde_json"),
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

/// Each crate a user's game reaches carries exactly the external dependencies
/// `SHIPPED` allows it.
///
/// Equality rather than containment, for the reason the graph test gives: a
/// dependency named in the list and missing from the manifest is a
/// disagreement worth seeing too. Workspace members are dropped first, because
/// those are `EXPECTED`'s business and holding them twice means two places to
/// keep in step.
///
/// Mutation: add `serde_json = "1.0.151"` to `crates/runtime/Cargo.toml`, or to
/// `crates/data/Cargo.toml`, or add `("regex", Optionality::Required)` to a
/// `SHIPPED` row with no dependency behind it. Each fails this test, naming the
/// crate. The same in `crates/core/Cargo.toml` fails this and
/// `core_depends_on_nothing_at_all`, which is the one crate held from both
/// sides.
#[test]
fn a_crate_a_game_reaches_carries_exactly_the_external_dependencies_the_list_allows() {
    let meta = metadata();
    let workspace: BTreeSet<String> = members(&meta).into_iter().map(|(n, _, _)| n).collect();
    for (name, allowed) in SHIPPED {
        let (_, _, deps) = members(&meta)
            .into_iter()
            .find(|(n, _, _)| n == name)
            .unwrap_or_else(|| panic!("{name} is named in SHIPPED and is not a workspace member"));
        let external: BTreeSet<Dep> = deps
            .into_iter()
            .filter(|d| !workspace.contains(&d.name))
            .collect();
        let allowed: BTreeSet<Dep> = allowed
            .iter()
            .map(|(n, o)| Dep {
                name: (*n).to_owned(),
                optionality: *o,
            })
            .collect();
        assert_eq!(
            external, allowed,
            "{name} carries the external dependencies {external:?}, and the list \
             in this file allows {allowed:?}. docs/specs/crates.md §3 fixes what \
             a user's game pulls in: add it to SHIPPED if that is the decision, \
             or take it out of the manifest"
        );
    }
}

/// Every crate a game reaches through `b2d_runtime` has a row in `SHIPPED`.
///
/// The vacuity guard. The test above walks `SHIPPED`, so a crate missing from
/// it is not checked and not reported; `runtime` gaining an internal crate
/// would quietly leave that crate's external dependencies held by nothing.
///
/// The closure is computed over `EXPECTED`, which is the other written table
/// and is itself held against `cargo metadata` by
/// `the_internal_dependency_graph_is_what_the_specification_says`. That is
/// deliberate: a membership check computed from the manifests this file checks
/// would agree with them however wrong both are.
///
/// Mutation: remove a row from `SHIPPED`, or add `b2d_editor_ui` to
/// `EXPECTED`'s `b2d_runtime` row. The first fails this test alone; the second
/// fails this and the graph test, which reads that row against the manifest.
#[test]
fn every_crate_a_game_reaches_through_runtime_has_a_row_in_the_shipped_list() {
    let mut reached = BTreeSet::new();
    let mut frontier = vec!["b2d_runtime"];
    while let Some(name) = frontier.pop() {
        if !reached.insert(name) {
            continue;
        }
        let (_, deps) = EXPECTED
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| {
                panic!("{name} is reachable from b2d_runtime and has no row in EXPECTED")
            });
        frontier.extend(deps.iter().copied());
    }
    let listed: BTreeSet<&str> = SHIPPED.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        reached, listed,
        "SHIPPED is about a different set of crates than the ones a game \
         reaches through b2d_runtime, so whatever it holds is not the contract \
         in docs/specs/crates.md §3"
    );
}

/// A dependency `cargo metadata` marks optional is read as optional.
///
/// Every `SHIPPED` row is empty today, so nothing else in this file would
/// notice `deps_of` reading `optionality` as a constant `Required`, and the
/// column would stay unguarded until the first optional dependency arrived.
/// `avian2d` arriving behind a default-on feature
/// (`docs/specs/level-editor.md` §3) is what that column is for.
///
/// Mutation: read `optionality` as a constant `Required` in `deps_of`, or drop
/// the `kind != dev` filter. The first fails this test alone; the second fails
/// this and `a_dev_dependency_is_not_a_dependency`.
#[test]
fn a_dependency_marked_optional_in_metadata_is_read_as_optional() {
    let pkg = serde_json::json!({
        "dependencies": [
            { "name": "avian2d", "kind": null, "optional": true },
            { "name": "bevy", "kind": null, "optional": false },
            { "name": "serde_json", "kind": "dev", "optional": false },
        ]
    });
    let deps = deps_of(&pkg);
    assert_eq!(
        deps,
        BTreeSet::from([
            Dep {
                name: "avian2d".to_owned(),
                optionality: Optionality::Optional,
            },
            Dep {
                name: "bevy".to_owned(),
                optionality: Optionality::Required,
            },
        ])
    );
}
