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

use DefaultFeatures::{Off, On};
use Optionality::{Optional, Required};

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

/// Whether a dependency is pulled in with its `default` feature or without it.
///
/// Spelled out for the same reason as `Optionality`: `("bevy", Required, true)`
/// in the table below does not say which way round `true` runs. `Off` is
/// `default-features = false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum DefaultFeatures {
    On,
    Off,
}

/// A dependency, as far as these tests care: what it is called, whether it is
/// optional, and what weight is selected on it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Dep {
    name: String,
    optionality: Optionality,
    default_features: DefaultFeatures,
    features: BTreeSet<String>,
}

/// One entry of a `SHIPPED` row: a dependency the crate may carry, and the
/// weight it may carry it with.
///
/// Named because the gate runs clippy with `-D warnings` and
/// `clippy::type_complexity` refuses the nested tuple written out in place.
/// The rows below stay tuples: two enums of different types cannot be written
/// in the wrong order without a compile error, which a pair of `bool`s or a
/// pair of same-typed fields would not give.
type Allowed = (
    &'static str,
    Optionality,
    DefaultFeatures,
    &'static [&'static str],
);

/// Every dependency each crate a user's game reaches is allowed to carry, from
/// `docs/specs/crates.md` §3: its name, its optionality, and the weight it is
/// pulled in with.
///
/// `EXPECTED` above holds the direction; this holds what comes with it. A game
/// reaches `GAME_ENTRY_POINTS` and everything under them, which today is
/// `b2d_runtime` and, through it, `b2d_data` and `b2d_core`. `b2d_editor`
/// and `b2d_editor_ui` are deliberately absent: nothing a game compiles reaches
/// them, and a list there would have to be edited every time the editor gained
/// a dependency while buying none of §2's contract about what a consumer pulls
/// in. What that costs is that an external dependency added to either is held
/// by nothing.
///
/// **Workspace members are held here rather than delegated to `EXPECTED`.** §3
/// has `data` putting its editor-only parts behind a feature so that a user's
/// game compiles only what loading needs, and the line that defeats that
/// decision is `b2d_data = { workspace = true, features = ["editor"] }` in
/// `crates/runtime/Cargo.toml`, which is an **internal** edge. A list that
/// dropped members before comparing could not see it, whatever it held about
/// features. The price is that `b2d_core` and `b2d_data` are named both here
/// and in `EXPECTED`; both tables are compared against the same
/// `cargo metadata` output, so they cannot come to disagree without one of them
/// failing.
///
/// The external lists are still empty, so `bevy` arriving in `data` and
/// `runtime` (§3) and `avian2d` arriving behind a default-on feature
/// (`docs/specs/level-editor.md` §3) each cost one line. Every row's weight is
/// the default one, which is why
/// `a_dependency_pulled_in_with_features_is_read_with_them` and
/// `a_shipped_row_carries_its_feature_selection_into_the_comparison` exist: a
/// column no row uses is a column nothing exercises.
///
/// What this list still does not hold is **what a crate's own `default` feature
/// contains**. Taking `avian2d` out of `default` changes what a game carries
/// and passes here. No crate in the workspace has a `[features]` table yet, so
/// closing that now means a mechanism with no subject; the issue that brings
/// `avian2d` in decides it, and it arrives with the first feature table.
///
/// Direct dependencies only, too: what `bevy` pulls in behind itself is not
/// this list's business.
const SHIPPED: &[(&str, &[Allowed])] = &[
    ("b2d_core", &[]),
    ("b2d_data", &[("b2d_core", Required, On, &[])]),
    (
        "b2d_runtime",
        &[
            ("b2d_core", Required, On, &[]),
            ("b2d_data", Required, On, &[]),
        ],
    ),
];

/// The crates a user's game depends on directly.
///
/// `docs/specs/crates.md` §3 has `runtime` as the one a game depends on, and
/// names `runtime_myphysics` as a future adapter that "sits beside `runtime`
/// the same way". A game that opts into such an adapter reaches it directly
/// rather than through `runtime`, so the closure below starts from every entry
/// point in this list. Adding one here is what pulls its crates into `SHIPPED`.
///
/// `xtask/src/gate.rs` writes this list out a second time, for the `game` row
/// that compiles each entry point the way a game resolves it. The two are held
/// equal by `the_gates_entry_point_list_agrees_with_the_dependency_graphs` there, so
/// adding a name here without adding it there fails that test rather than
/// leaving the new crate compiled by nothing.
const GAME_ENTRY_POINTS: &[&str] = &["b2d_runtime"];

/// One `SHIPPED` row's entries, in the shape `deps_of` produces.
///
/// Extracted so that the optionality a row carries is read by something a test
/// can call with a non-empty row. Inline, it was only ever applied to the empty
/// rows above, so nothing could tell whether it read the column at all.
fn allowed_deps(row: &[Allowed]) -> BTreeSet<Dep> {
    row.iter()
        .map(|(n, o, df, fs)| Dep {
            name: (*n).to_owned(),
            optionality: *o,
            default_features: *df,
            features: fs.iter().map(|f| (*f).to_owned()).collect(),
        })
        .collect()
}

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
/// A dependency declared under a target table is one entry like any other:
/// metadata flattens the target tables, so there is no special case here.
fn deps_of(pkg: &Value) -> BTreeSet<Dep> {
    pkg["dependencies"]
        .as_array()
        .expect("dependencies")
        .iter()
        .filter(|d| d["kind"].as_str() != Some("dev"))
        .map(|d| Dep {
            name: d["name"].as_str().expect("name").to_owned(),
            optionality: if d["optional"].as_bool() == Some(true) {
                Optional
            } else {
                Required
            },
            default_features: if d["uses_default_features"]
                .as_bool()
                .expect("uses_default_features")
            {
                On
            } else {
                Off
            },
            features: d["features"]
                .as_array()
                .expect("features")
                .iter()
                .map(|f| f.as_str().expect("feature name").to_owned())
                .collect(),
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

/// Each crate a user's game reaches carries exactly the dependencies `SHIPPED`
/// allows it, with exactly the weight it allows.
///
/// Equality rather than containment, for the reason the graph test gives: a
/// dependency named in the list and missing from the manifest is a
/// disagreement worth seeing too.
///
/// Workspace members are **not** dropped before comparing. `runtime -> data` is
/// an internal edge, and it is where a game's copy of §3's feature split in
/// `data` is decided, so a comparison that delegated members to `EXPECTED`,
/// which holds names, could not see a feature turned on across it.
/// `SHIPPED`'s doc comment carries what including them costs.
///
/// What this holds is the **declared** edge, and that is not the only way the
/// split can die. If `runtime` used `data`'s editor-only code directly, this
/// test would stay green, and so would the clippy row, which passes
/// `--all-features`, and the test row, which builds the workspace, where
/// `b2d_editor` turns the feature on and cargo unifies it onto `data`. The
/// other half is the `game` row of `cargo xtask gate`: it runs
/// `cargo check -p <entry point>`, which resolves `data` the way a game does
/// and refuses it.
///
/// Mutation: add `serde_json = "1.0.151"` to `crates/runtime/Cargo.toml`, or to
/// `crates/data/Cargo.toml`, or add `("regex", Required, On, &[])` to a
/// `SHIPPED` row with no dependency behind it. Each fails this test, naming the
/// crate. So does each of the two that this test exists for: `features =
/// ["editor"]` on `runtime`'s edge to `data`, against a `[features] editor =
/// []` table in `crates/data/Cargo.toml`, and `default-features = false` on
/// `b2d_data` in the root `[workspace.dependencies]`. The same in
/// `crates/core/Cargo.toml` fails this and `core_depends_on_nothing_at_all`,
/// which is the one crate held from both sides.
///
/// `default-features = false` at the member rather than in the workspace table
/// is not in that list because cargo refuses it outright: `default-features =
/// false cannot override workspace's default-features`. That edge is a resolve
/// error before this test runs.
#[test]
fn a_crate_a_game_reaches_carries_exactly_the_dependencies_the_list_allows() {
    let meta = metadata();
    for (name, allowed) in SHIPPED {
        let (_, _, carried) = members(&meta)
            .into_iter()
            .find(|(n, _, _)| n == name)
            .unwrap_or_else(|| panic!("{name} is named in SHIPPED and is not a workspace member"));
        let allowed = allowed_deps(allowed);
        assert_eq!(
            carried, allowed,
            "{name} carries the dependencies {carried:?}, and the list \
             in this file allows {allowed:?}. docs/specs/crates.md §3 fixes what \
             a user's game pulls in, features included: add it to SHIPPED if that is the decision, \
             or take it out of the manifest"
        );
    }
}

/// Every crate a game reaches has a row in `SHIPPED`.
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
/// Mutation: remove a row from `SHIPPED`, add `b2d_editor_ui` to `EXPECTED`'s
/// `b2d_runtime` row, or add an entry point to `GAME_ENTRY_POINTS` whose crates
/// have no rows. The first and third fail this test alone; the second fails
/// this and the graph test, which reads that row against the manifest.
#[test]
fn every_crate_a_game_reaches_has_a_row_in_the_shipped_list() {
    let mut reached = BTreeSet::new();
    let mut frontier = GAME_ENTRY_POINTS.to_vec();
    while let Some(name) = frontier.pop() {
        if !reached.insert(name) {
            continue;
        }
        let (_, deps) = EXPECTED
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| {
                panic!("{name} is reachable from a game entry point and has no row in EXPECTED")
            });
        frontier.extend(deps.iter().copied());
    }
    let listed: BTreeSet<&str> = SHIPPED.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        reached, listed,
        "SHIPPED is about a different set of crates than the ones a game \
         actually reaches from GAME_ENTRY_POINTS, so whatever it holds is not \
         the contract in docs/specs/crates.md §3"
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
            { "name": "avian2d", "kind": null, "optional": true,
              "uses_default_features": true, "features": [] },
            { "name": "bevy", "kind": null, "optional": false,
              "uses_default_features": true, "features": [] },
            { "name": "serde_json", "kind": "dev", "optional": false,
              "uses_default_features": true, "features": [] },
        ]
    });
    let deps = deps_of(&pkg);
    assert_eq!(
        deps,
        BTreeSet::from([
            Dep {
                name: "avian2d".to_owned(),
                optionality: Optional,
                default_features: On,
                features: BTreeSet::new(),
            },
            Dep {
                name: "bevy".to_owned(),
                optionality: Required,
                default_features: On,
                features: BTreeSet::new(),
            },
        ])
    );
}

/// A `SHIPPED` row's optionality reaches the comparison.
///
/// Every row is empty today, so `allowed_deps` runs only over empty slices and
/// nothing else in this file notices it ignoring the column. That matters in
/// one direction in particular: a dependency written `Optional` in `SHIPPED`
/// while the manifest declares it hard would compare equal and pass, which is
/// exactly the contract `docs/specs/level-editor.md` §3 states about `avian2d`
/// being a feature rather than a hard dependency.
///
/// Mutation: replace `optionality: *o` in `allowed_deps` with a constant
/// `Optionality::Required`. This test fails and nothing else does.
#[test]
fn a_shipped_row_carries_its_optionality_into_the_comparison() {
    assert_eq!(
        allowed_deps(&[("avian2d", Optional, On, &[])]),
        BTreeSet::from([Dep {
            name: "avian2d".to_owned(),
            optionality: Optional,
            default_features: On,
            features: BTreeSet::new(),
        }])
    );
}

/// A dependency's feature selection is read out of metadata as metadata
/// reports it.
///
/// Every `SHIPPED` row selects nothing and leaves `default` on, so nothing else
/// in this file would notice `deps_of` reading either column as a constant.
/// `docs/specs/crates.md` §3 puts `data`'s editor-only parts behind a feature
/// so a game does not compile them, and a feature turned on across
/// `runtime -> data` is what takes that back.
///
/// The fixture's `optional` and `uses_default_features` deliberately disagree.
/// With both `false`, reading one field in place of the other is invisible
/// here and fails only the test named for optionality, which is a name that
/// does not describe the defect.
///
/// Mutation: read `default_features` as a constant `On` in `deps_of`, or build
/// `features` as `BTreeSet::new()`. Each fails this test alone. Read
/// `default_features` from `d["optional"]`, and this test fails along with
/// `a_dependency_marked_optional_in_metadata_is_read_as_optional`.
#[test]
fn a_dependency_pulled_in_with_features_is_read_with_them() {
    let pkg = serde_json::json!({
        "dependencies": [
            { "name": "bevy", "kind": null, "optional": true,
              "uses_default_features": false,
              "features": ["bevy_asset", "bevy_sprite"] },
        ]
    });
    assert_eq!(
        deps_of(&pkg),
        BTreeSet::from([Dep {
            name: "bevy".to_owned(),
            optionality: Optional,
            default_features: Off,
            features: ["bevy_asset", "bevy_sprite"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }])
    );
}

/// A `SHIPPED` row's feature selection reaches the comparison.
///
/// The other half of the column, and unguarded for the same reason:
/// `allowed_deps` only ever runs over rows that select nothing. A row written
/// `Off` while the manifest leaves `default` on has to compare unequal, or the
/// table can say a game carries less than it does.
///
/// Mutation: replace `default_features: *df` in `allowed_deps` with a constant
/// `On`, or `features` with `BTreeSet::new()`. Each fails this test alone.
#[test]
fn a_shipped_row_carries_its_feature_selection_into_the_comparison() {
    assert_eq!(
        allowed_deps(&[("bevy", Required, Off, &["bevy_asset"])]),
        BTreeSet::from([Dep {
            name: "bevy".to_owned(),
            optionality: Required,
            default_features: Off,
            features: BTreeSet::from(["bevy_asset".to_owned()]),
        }])
    );
}
