//! The gate, as a crate the gate itself checks.
//!
//! `CLAUDE.md` defines "done" as the exit code of one command. That only holds
//! while the command is somewhere a reviewer can read it and a runner can run
//! it, and it was not: the scripts lived in `.claude/`, which is not committed.
//!
//! Being a crate rather than a script is the point, not a preference.
//! `cargo clippy --workspace --all-targets` lints these checks and
//! `cargo test --workspace` runs their tests, so a check that stops examining
//! anything fails a **named** test instead of printing `ok`. The script this
//! replaces had exactly that defect: its dependency-graph check was written
//! against the directory names, survived the rename in
//! `docs/specs/crates.md` §4, and went on reporting `ok` about a workspace it
//! was no longer looking at.
//!
//! That check is gone rather than ported. The graph is asserted by
//! `crates/editor/tests/dependency_direction.rs`, which the `test` row runs,
//! and a second copy of the graph is a second copy to disagree.

mod commands;
mod docs;
mod gate;

use std::path::Path;
use std::process::ExitCode;

/// The workspace root.
///
/// Taken from this crate's manifest directory at compile time rather than from
/// the working directory, because `cargo run` hands the binary whichever
/// directory cargo was invoked from. `git rev-parse` would answer too, and
/// would make the checks need a git checkout to run in.
///
/// What this costs: the path is baked into the binary, so moving the whole
/// checkout, `target/` included, leaves a cached binary pointing at where the
/// tree used to be. Cargo rebuilds on a source change and not on a move, so
/// `cargo xtask docs` then says `docs/ does not exist` until something is
/// touched. Loud, and recoverable. The quiet version is the one to know about:
/// if a second checkout sits at the old path, the checks run against that tree
/// and report about a workspace nobody asked them to look at.
fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits under the workspace root")
}

/// A failed check, said again in the syntax the Checks page reads.
///
/// The job is named for its check, so which check failed is already on the
/// Checks page. What this adds is what it found, as one line on the run's
/// summary, without the log being opened.
///
/// Whether this is a runner is a parameter rather than a call to
/// [`std::env::var_os`] inside, so that both answers have a test and neither
/// depends on what the process running the suite happens to carry in its
/// environment.
///
/// Mutation: return `Some` for both, and `a_local_run_is_not_annotated` fails.
pub(crate) fn annotation(on_github: bool, message: &str) -> Option<String> {
    on_github.then(|| format!("::error::{message}"))
}

/// Whether there is a Checks page reading this run.
///
/// `GITHUB_ACTIONS` is what the runner sets, and it is the only thing asked
/// about: a workflow that stops setting it loses the annotations and keeps
/// every exit code, which is the right way round for a line whose only job is
/// to be read.
pub(crate) fn on_github() -> bool {
    std::env::var_os("GITHUB_ACTIONS").is_some()
}

/// A task: the name it is typed as, and what running it reports.
pub(crate) type Task = (&'static str, fn() -> bool);

/// Every task `cargo xtask` runs, in the order `all` runs them.
///
/// Written out as a table because `all` runs the table rather than naming the
/// tasks a second time. A task cannot quietly leave `all` while still existing
/// on its own, which was reachable while the `all` arm listed them itself:
/// dropping `docs` from it left the whole suite green and the documents
/// unchecked on every runner.
///
/// Mutation: remove a row, and `the_tasks_are_the_ones_the_workflow_runs`
/// fails.
pub(crate) const TASKS: [Task; 3] = [
    ("gate", gate::run),
    ("docs", docs::run),
    ("commands", commands::run),
];

/// Every task `cargo xtask <name>` runs: the table above, then each row of
/// the gate on its own.
///
/// A row is a task so that CI can run it as a job named for it. It is not in
/// [`TASKS`] because `all` runs that table, and the gate already runs its rows.
pub(crate) fn every_task() -> impl Iterator<Item = &'static Task> {
    TASKS.iter().chain(gate::ROWS.iter())
}

/// The task typed as `name`, if there is one.
///
/// What `dispatch` looks a name up with, split out so the lookup has a test.
/// `dispatch` itself cannot be called with a real task from the suite, because
/// the gate runs `cargo test`, which runs the test.
///
/// Mutation: search `TASKS` alone here, and
/// `every_check_the_workflow_runs_is_a_task_xtask_dispatches` fails naming
/// `fmt`. The workflow's legs would otherwise each print `fmt is not a task`
/// and exit 2.
fn task_named(name: &str) -> Option<fn() -> bool> {
    every_task()
        .find(|(task, _)| *task == name)
        .map(|(_, run)| *run)
}

/// Every check CI runs as a job of its own: the gate's rows, then every task
/// besides the gate.
///
/// Derived rather than written out, so that a task added to [`TASKS`] or a row
/// added to [`gate::ROWS`] is a check the workflow is then required to run.
/// Only the tests need it, because CI reads its own copy of the list and
/// `the_workflow_runs_every_check_xtask_lists` holds the two equal.
#[cfg(test)]
pub(crate) fn checks() -> Vec<&'static str> {
    gate::ROWS
        .iter()
        .chain(TASKS.iter().filter(|(task, _)| *task != "gate"))
        .map(|(task, _)| *task)
        .collect()
}

fn main() -> ExitCode {
    ExitCode::from(dispatch(std::env::args().nth(1).as_deref()))
}

/// What `main` does, as a value, so that the exit codes have a test.
///
/// 0 when the task passed, 1 when it failed, and 2 when there is no such task.
/// The last is separate on purpose: a workflow step that misspells the task
/// should not be told the gate passed, and "the gate failed" and "the gate
/// never ran" are different pieces of news.
fn dispatch(task: Option<&str>) -> u8 {
    let passed = match task {
        Some("all") => all(&TASKS),
        Some(name) => match task_named(name) {
            Some(run) => run(),
            None => {
                eprintln!("xtask: `{name}` is not a task");
                return usage();
            }
        },
        None => return usage(),
    };
    exit_code(passed)
}

/// Say what the tasks are, and report that none of them ran.
///
/// Built from [`every_task`] rather than written out, so a task added to the
/// table is a task the usage line already knows about.
fn usage() -> u8 {
    let tasks: Vec<&str> = every_task().map(|(task, _)| *task).collect();
    eprintln!("usage: cargo xtask <all|{}>", tasks.join("|"));
    2
}

/// Run every task in `tasks`, and report whether all of them passed.
///
/// **Every one runs.** A task that says nothing because an earlier one failed
/// is a task somebody has to run a second time to find out about, and on a
/// runner that means pushing a commit to ask a question. It is the same reason
/// [`gate::run`] collects its rows before taking a verdict, and it is why the
/// fold is `&` rather than `&&`: the right-hand side of `&&` would not be
/// evaluated once something had failed.
///
/// This is what somebody at a terminal runs to see everything CI would. Taking
/// the table as an argument is what makes it testable: `dispatch`
/// cannot be called with `all` from a test, because `gate` runs `cargo test`,
/// which runs the test.
///
/// Mutation: write `passed && run()` instead, and
/// `a_failed_task_does_not_silence_the_next_one` fails.
fn all(tasks: &[Task]) -> bool {
    tasks.iter().fold(true, |passed, (_, run)| run() & passed)
}

/// A verdict as a process exit code: 0 passed, 1 failed.
///
/// One line, and it is the line `CLAUDE.md` rests on when it defines done as an
/// exit code rather than as a feeling. Inverting it leaves every row still
/// printing what it printed.
///
/// Mutation: drop the `!`, and `a_verdict_becomes_the_exit_code_it_means`
/// fails. Nothing else does.
fn exit_code(passed: bool) -> u8 {
    u8::from(!passed)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::{
        TASKS, all, annotation, commands, dispatch, docs, exit_code, gate, task_named,
        workspace_root,
    };
    use regex::Regex;
    use yaml_rust2::{Yaml, YamlLoader};

    /// The workflow, parsed.
    ///
    /// Through a parser rather than as text, because a text scrape in this
    /// crate has already been defeated by comments, three ways. A commented-out
    /// step or matrix entry is not in the tree this returns, which is the
    /// right answer: it does not run.
    fn workflow() -> Yaml {
        let path = workspace_root()
            .join(".github")
            .join("workflows")
            .join("ci.yml");
        let text = std::fs::read_to_string(&path).expect("the workflow is in the repository");
        YamlLoader::load_from_str(&text)
            .expect("the workflow is YAML")
            .into_iter()
            .next()
            .expect("the workflow is one document")
    }

    /// A YAML list of strings, as strings.
    fn strings(list: &Yaml) -> Vec<&str> {
        list.as_vec()
            .map(|items| items.iter().filter_map(Yaml::as_str).collect())
            .unwrap_or_default()
    }

    /// The check job's legs, as `(check, platform)` pairs, read from the
    /// matrix's `include` list in the order it writes them.
    ///
    /// Through the parser, so a commented-out leg is not a leg.
    fn legs(workflow: &Yaml) -> Vec<(&str, &str)> {
        workflow["jobs"]["check"]["strategy"]["matrix"]["include"]
            .as_vec()
            .map(|legs| {
                legs.iter()
                    .filter_map(|leg| Some((leg["check"].as_str()?, leg["os"].as_str()?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The position of the first step of `job` that `matches`.
    fn step(job: &Yaml, matches: impl Fn(&Yaml) -> bool) -> Option<usize> {
        job["steps"].as_vec()?.iter().position(matches)
    }

    /// Whether the second task of `a_failed_task_does_not_silence_the_next_one`
    /// ran. A static because `all` takes function pointers, which cannot carry
    /// a captured environment, and only that test touches it.
    static SECOND_TASK_RAN: AtomicBool = AtomicBool::new(false);

    fn fails() -> bool {
        false
    }

    fn passes() -> bool {
        true
    }

    fn records_that_it_ran() -> bool {
        SECOND_TASK_RAN.store(true, Ordering::SeqCst);
        true
    }

    /// Every check the workflow runs is a task xtask dispatches, the gate
    /// with them.
    ///
    /// A list of names is not a set of tasks until a name typed at a terminal
    /// or in a job reaches its function, and the lookup is the step between.
    ///
    /// Mutation: search `TASKS` alone in `task_named`, and this fails naming
    /// `fmt`.
    #[test]
    fn every_check_the_workflow_runs_is_a_task_xtask_dispatches() {
        for name in crate::checks().into_iter().chain(["gate"]) {
            assert!(
                task_named(name).is_some(),
                "`cargo xtask {name}` would print that {name} is not a task and exit 2"
            );
        }
    }

    /// A task that does not exist is not a pass.
    ///
    /// Only the unknown arms are exercised. Calling `dispatch(Some("gate"))`
    /// would run the gate, which runs this test, which would run the gate.
    ///
    /// Mutation: return 0 instead of 2, and this fails. What it prevents is a
    /// workflow that types `cargo xtask gates` and reports a green build.
    #[test]
    fn a_task_that_does_not_exist_is_not_a_pass() {
        assert_eq!(dispatch(Some("gat")), 2);
        assert_eq!(dispatch(Some("al")), 2);
        assert_eq!(dispatch(None), 2);
    }

    /// A task that failed does not silence the one after it.
    ///
    /// One run says everything that is wrong. The alternative is a runner that
    /// reports a formatting failure, and only after a second push reports the
    /// broken link that was there all along.
    ///
    /// Mutation: write `passed && run()` in `all`, and this fails saying the
    /// second task did not run.
    #[test]
    fn a_failed_task_does_not_silence_the_next_one() {
        SECOND_TASK_RAN.store(false, Ordering::SeqCst);
        let passed = all(&[("fails", fails), ("records", records_that_it_ran)]);
        assert!(
            SECOND_TASK_RAN.load(Ordering::SeqCst),
            "the second task did not run"
        );
        assert!(!passed, "a failed task passed the run");
    }

    /// Every task has to pass for the run to pass.
    ///
    /// Mutation: return the first or the last verdict from `all`, and this
    /// fails.
    #[test]
    fn every_task_has_to_pass_for_the_run_to_pass() {
        assert!(all(&[("passes", passes), ("passes", passes)]));
        assert!(!all(&[("passes", passes), ("fails", fails)]));
        assert!(!all(&[("fails", fails), ("passes", passes)]));
        assert!(!all(&[("fails", fails), ("fails", fails)]));
    }

    /// The tasks are the ones the workflow runs.
    ///
    /// `all` runs this table, so a task that leaves it leaves every runner
    /// without saying so: the remaining tasks stay green and the run reports a
    /// pass: a check reporting success about a set it never examined. This
    /// asserts the table is the set that was meant.
    ///
    /// Mutation: remove either row from `TASKS`, and this fails.
    #[test]
    fn the_tasks_are_the_ones_the_workflow_runs() {
        let tasks: Vec<&str> = TASKS.iter().map(|(task, _)| *task).collect();
        assert_eq!(tasks, ["gate", "docs", "commands"]);
    }

    /// Each task runs the check its name promises.
    ///
    /// The names alone are not the table: swapping the two functions behind
    /// them leaves `cargo xtask gate` checking the documents and
    /// `cargo xtask docs` running the gate, with every name still in place.
    /// No test can call `dispatch(Some("gate"))` to find that out, because the
    /// gate runs `cargo test`, so the wiring is asserted by address instead.
    ///
    /// Mutation: swap the two functions in `TASKS`, and this fails.
    #[test]
    fn each_task_runs_the_check_its_name_promises() {
        let by_name = |wanted: &str| {
            TASKS
                .iter()
                .find(|(task, _)| *task == wanted)
                .map(|(_, run)| *run)
                .expect("the task is in the table")
        };
        assert!(
            std::ptr::fn_addr_eq(by_name("gate"), gate::run as fn() -> bool),
            "the gate task does not run the gate"
        );
        assert!(
            std::ptr::fn_addr_eq(by_name("docs"), docs::run as fn() -> bool),
            "the docs task does not check the documents"
        );
        assert!(
            std::ptr::fn_addr_eq(by_name("commands"), commands::run as fn() -> bool),
            "the commands task does not check the commands"
        );
    }

    /// The workflow runs every check xtask lists, and nothing else, each leg
    /// once.
    ///
    /// The workflow's matrix is a second copy of the list of checks, which is
    /// what `docs/specs/dev-environment.md` §3 once rejected a job per check
    /// for. This is what makes the copy safe: the two are read independently,
    /// xtask's from [`crate::checks`] and the workflow's through a parser, and
    /// held equal in both directions.
    ///
    /// Mutation: delete the `clippy` leg, and this fails naming it. List the
    /// `fmt` leg twice, and this fails. Add a task to `TASKS` or a row to
    /// `ROWS` without a leg, and this fails naming it. Commenting a leg out is
    /// a deletion, not a disguise.
    #[test]
    fn the_workflow_runs_every_check_xtask_lists() {
        let workflow = workflow();
        let legs = legs(&workflow);
        let listed: Vec<&str> = legs.iter().map(|(check, _)| *check).collect();
        let checks = crate::checks();
        for check in &checks {
            assert!(
                listed.contains(check),
                "xtask lists `{check}` and the workflow runs no job for it"
            );
        }
        for check in &listed {
            assert!(
                checks.contains(check),
                "the workflow runs `{check}` and xtask does not list it, so `cargo xtask all` skips it"
            );
        }
        let mut distinct = legs.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            legs.len(),
            "a leg is listed twice: {legs:?}"
        );
    }

    /// The workflow runs the tests on the three platforms the specification
    /// names and every other check on Linux, each as a job named for both.
    ///
    /// `docs/specs/dev-environment.md` §3 decided which check runs where, and
    /// nothing else here can tell when a leg quietly leaves the matrix or
    /// lands on the wrong runner: the remaining legs stay green. The legs are
    /// compared as a set with the one §3 implies, built from xtask's own list
    /// of checks, so a check added there is expected on Linux without this
    /// test being edited, and an empty `include` cannot pass.
    ///
    /// The names are data, and a name is not a platform until something runs
    /// on it, so `runs-on` is held to the matrix too: pinning it to one runner
    /// leaves every leg's name in place and every leg on one machine, which is
    /// how an earlier version of this test passed while checking nothing.
    ///
    /// Mutation: drop the `windows-latest` `test` leg, add a `clippy` leg on
    /// `macos-latest`, pin `runs-on` to `ubuntu-latest`, name the job without
    /// `matrix.check`, or run `cargo xtask all` in place of the matrix's check,
    /// and this fails naming what went wrong.
    #[test]
    fn the_workflow_runs_the_tests_on_every_platform_and_the_rest_on_linux() {
        let workflow = workflow();
        let job = &workflow["jobs"]["check"];
        let mut wanted: Vec<(&str, &str)> = crate::checks()
            .into_iter()
            .filter(|check| *check != "test")
            .map(|check| (check, "ubuntu-latest"))
            .chain(
                ["ubuntu-latest", "macos-latest", "windows-latest"]
                    .into_iter()
                    .map(|os| ("test", os)),
            )
            .collect();
        wanted.sort_unstable();
        let mut legs = legs(&workflow);
        legs.sort_unstable();
        assert_eq!(legs, wanted, "the matrix's legs");
        assert_eq!(
            job["runs-on"].as_str(),
            Some("${{ matrix.os }}"),
            "the legs are not wired to the matrix, so they all run on one platform"
        );
        let name = job["name"].as_str().unwrap_or_default();
        for part in ["${{ matrix.check }}", "${{ matrix.os }}"] {
            assert!(
                name.contains(part),
                "the job is named `{name}`, so the Checks page does not say {part}"
            );
        }
        assert!(
            step(job, |step| step["run"].as_str()
                == Some("cargo xtask ${{ matrix.check }}"))
            .is_some(),
            "no step runs the check its job is named for"
        );
    }

    /// The workflow caches what an engine costs to build.
    ///
    /// `docs/specs/dev-environment.md` §3 decided against a build cache and
    /// made the reversal conditional on the change that brings Bevy into the
    /// workspace. A trigger that fires and is not acted on is the thing
    /// `docs/specs/open-questions.md` §1 calls the same as forgetting, and
    /// nothing else here would notice the step going missing: the legs stay
    /// green and only the minutes change.
    ///
    /// The action is pinned to an exact tag, the way the checkout above is, so
    /// that the version this ran with is readable from the repository.
    ///
    /// Mutation: remove the step, or loosen the pin to a major tag, and this
    /// fails.
    #[test]
    fn the_workflow_caches_what_an_engine_costs_to_build() {
        let workflow = workflow();
        assert!(
            step(&workflow["jobs"]["check"], |step| {
                step["uses"].as_str() == Some("Swatinem/rust-cache@v2.9.2")
            })
            .is_some(),
            "the workflow does not cache the build, or does not pin the action"
        );
    }

    /// The engine features a file asks for, as that file writes them.
    ///
    /// Both the manifest and the specification write the same list, in the
    /// same shape, and neither is generated from the other.
    fn features_in(text: &str) -> Vec<String> {
        let list = Regex::new(r"(?s)features = \[([^\]]*)\]").expect("a literal pattern");
        let entry = Regex::new(r#""([^"]+)""#).expect("a literal pattern");
        list.captures(text).map_or_else(Vec::new, |caps| {
            entry
                .captures_iter(&caps[1])
                .map(|found| found[1].to_owned())
                .collect()
        })
    }

    /// A manifest's lines with its comment lines taken out, so that a comment
    /// quoting a dependency line is not read as one.
    fn without_comments(text: &str) -> String {
        text.lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Every crate that takes the engine gets the features the specification
    /// settled, and not the engine's defaults, as cargo resolves it.
    ///
    /// `docs/specs/ui.md` §3 writes the list and says what dropping the rest
    /// costs, measured. The root's `[workspace.dependencies]` writes it again
    /// because that is where cargo reads it. Neither is derived from the
    /// other, so they are a real cross-check rather than a file agreeing with
    /// itself.
    ///
    /// **Read from `cargo metadata`, after inheritance, not from the
    /// manifests.** A member can add features on top of what it inherits, and
    /// TOML can say the same dependency indented, renamed through `package`,
    /// or as a `[dependencies.bevy]` table. A line scrape of the manifests,
    /// which is what this test first was, let an indented line through and
    /// held nothing about the defaults, both measured. What cargo resolves is
    /// the same value however it was written.
    ///
    /// The lockfile cannot stand in for this. `bevy_feathers` is still
    /// resolved with the feature removed, measured, so an assertion about the
    /// lock would be one that cannot fail.
    ///
    /// Mutation: drop a feature from the root's `bevy` entry, or drop its
    /// `default-features = false`, and this fails. Mutation: give
    /// `crates/editor_ui/Cargo.toml` an indented
    /// `  bevy = { workspace = true, features = ["3d"] }`, and this fails
    /// naming `b2d_editor_ui`.
    #[test]
    fn every_crate_that_takes_the_engine_gets_the_features_the_specification_settled() {
        let mut spec = features_in(
            &std::fs::read_to_string(workspace_root().join("docs/specs/ui.md"))
                .expect("the file is in the repository"),
        );
        assert!(
            !spec.is_empty(),
            "no feature list was read from the specification, so this asserts nothing"
        );
        spec.sort();

        let out = std::process::Command::new(env!("CARGO"))
            .args(["metadata", "--format-version", "1", "--no-deps"])
            .current_dir(workspace_root())
            .output()
            .expect("cargo metadata runs");
        assert!(
            out.status.success(),
            "cargo metadata failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let metadata: serde_json::Value =
            serde_json::from_slice(&out.stdout).expect("cargo metadata prints JSON");

        let mut taking = std::collections::BTreeSet::new();
        for package in metadata["packages"].as_array().expect("a package list") {
            let name = package["name"].as_str().expect("a package name");
            for dependency in package["dependencies"]
                .as_array()
                .expect("a dependency list")
            {
                if dependency["name"] != "bevy" {
                    continue;
                }
                let mut features: Vec<String> = dependency["features"]
                    .as_array()
                    .expect("a feature list")
                    .iter()
                    .map(|feature| feature.as_str().expect("a feature name").to_owned())
                    .collect();
                features.sort();
                assert_eq!(
                    features, spec,
                    "{name} and docs/specs/ui.md §3 ask for different engine features"
                );
                assert_eq!(
                    dependency["uses_default_features"], false,
                    "{name} takes the engine's default features, which ui.md §3 does not"
                );
                taking.insert(name.to_owned());
            }
        }
        assert!(
            taking.len() >= 2,
            "only {taking:?} take the engine, so this holds less than it reads as"
        );
    }

    /// A crate takes the engine only by inheriting the root's entry, so the
    /// version and the features are written in one place.
    ///
    /// Whether a member inherited is not in `cargo metadata`: a written-out
    /// line with the same version and features resolves to the same thing.
    /// So this one reads the manifests, and keeps to what text can say
    /// reliably: comment lines are taken out, the leading space TOML allows is
    /// trimmed, and a line names the engine when its key is `bevy` itself, its
    /// table is `[...bevy]`, or it says `package = "bevy"`. A neighbour such
    /// as `bevy_egui` is not the engine and is not read.
    ///
    /// Mutation: replace `bevy.workspace = true` in
    /// `crates/editor_ui/Cargo.toml` with a written-out
    /// `bevy = { version = .. }` line, indented or not, or with
    /// `engine = { package = "bevy", .. }`, and this fails naming that
    /// manifest.
    #[test]
    fn a_crate_takes_the_engine_only_by_inheriting_the_roots_entry() {
        let key = Regex::new(r#"^"?bevy"?\s*[.=]"#).expect("a literal pattern");
        let table = Regex::new(r#"^\[[^\]]*\."?bevy"?\]$"#).expect("a literal pattern");
        let renamed = Regex::new(r#"package\s*=\s*"bevy""#).expect("a literal pattern");

        let mut inheriting = std::collections::BTreeSet::new();
        for crate_dir in std::fs::read_dir(workspace_root().join("crates"))
            .expect("the directory is in the repository")
        {
            let manifest = crate_dir
                .expect("the entry is readable")
                .path()
                .join("Cargo.toml");
            let text = without_comments(
                &std::fs::read_to_string(&manifest).expect("every crate has a manifest"),
            );
            for line in text
                .lines()
                .map(str::trim)
                .filter(|line| key.is_match(line) || table.is_match(line) || renamed.is_match(line))
            {
                assert_eq!(
                    line,
                    "bevy.workspace = true",
                    "{} names the engine other than by inheriting the root's entry",
                    manifest.display()
                );
                inheriting.insert(manifest.clone());
            }
        }
        assert!(
            inheriting.len() >= 2,
            "only {} manifest(s) inherit the engine, so this holds less than it reads as",
            inheriting.len()
        );
    }

    /// A leg that did not succeed fails the check branch protection requires.
    ///
    /// `needs.<job>.result` has four values, so a condition that enumerates
    /// the ones somebody thought of is a condition that lets the rest through.
    /// A skipped leg passing here is a required check going green over a
    /// commit nothing built. The matrix job's result is one result over every
    /// leg, so naming that job is naming all of them.
    ///
    /// Mutation: enumerate the failing results instead, drop the condition,
    /// or drop `check` from `needs`, and this fails.
    #[test]
    fn a_leg_that_did_not_succeed_fails_the_required_check() {
        let workflow = workflow();
        let required = &workflow["jobs"]["ci"];
        assert!(
            strings(&required["needs"]).contains(&"check"),
            "the required check does not wait for the checks"
        );
        assert!(
            step(required, |step| {
                step["if"].as_str() == Some("needs.check.result != 'success'")
            })
            .is_some(),
            "the required check does not insist that every leg succeeded"
        );
    }

    /// The cache runs after the toolchain it is keyed on.
    ///
    /// `Swatinem/rust-cache` builds its key from the installed toolchain, so a
    /// step that runs before the toolchain is installed keys the cache on
    /// whatever the runner happened to ship with. Every leg stays green and
    /// the cache quietly stops matching, which is the whole of what the step
    /// was added for.
    ///
    /// Both positions are indices into the parsed list of steps, so the prose
    /// above either of them cannot be mistaken for the step itself.
    ///
    /// Mutation: move the cache step above the toolchain step, and this fails.
    #[test]
    fn the_cache_runs_after_the_toolchain_it_is_keyed_on() {
        let workflow = workflow();
        let job = &workflow["jobs"]["check"];
        let toolchain = step(job, |step| step["run"].as_str() == Some("rustup show"))
            .expect("the workflow installs the toolchain it pins");
        let cache = step(job, |step| {
            step["uses"]
                .as_str()
                .is_some_and(|uses| uses.starts_with("Swatinem/rust-cache@"))
        })
        .expect("the workflow caches the build");
        assert!(
            cache > toolchain,
            "the cache step runs before the toolchain is installed, so its key is the runner's"
        );
    }

    /// The engine arrives without the features it was taken without.
    ///
    /// `docs/specs/ui.md` §3 takes Bevy without its defaults and records what
    /// that costs, measured. Nothing else notices a dropped feature coming
    /// back: the build stays green and only the minutes change.
    ///
    /// **Read from `cargo tree`, which is what gets built**, rather than from
    /// the manifest that asks for it, so a feature arriving through something
    /// else is caught as well. The lockfile is not that set: `bevy_internal` names
    /// `bevy_dev_tools` through weak features such as `bevy_dev_tools?/webgl`,
    /// the lockfile carries a dependency named that way whether or not it is
    /// built, and `bevy_dev_tools` takes `bevy_audio` unconditionally. So
    /// `bevy_audio` sits in `Cargo.lock` while `cargo tree -i bevy_audio`
    /// prints nothing, measured on 0.20.1. The first assertion is the vacuity
    /// guard: a tree this could not read would otherwise pass every line
    /// below it.
    ///
    /// Mutation: add `audio` to the root's `bevy` feature list, and this
    /// fails.
    #[test]
    fn the_engine_arrives_without_the_features_it_was_taken_without() {
        let out = std::process::Command::new(env!("CARGO"))
            .args([
                "tree",
                "--workspace",
                "--edges",
                "normal",
                "--prefix",
                "none",
                "--format",
                "{p}",
            ])
            .current_dir(workspace_root())
            .output()
            .expect("cargo tree runs");
        assert!(
            out.status.success(),
            "cargo tree failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let built = String::from_utf8(out.stdout).expect("cargo tree prints UTF-8");
        let names: Vec<&str> = built
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .collect();
        assert!(
            names.contains(&"bevy_internal"),
            "the engine is not in what is built, so this asserts nothing"
        );
        for absent in ["bevy_audio", "bevy_gltf"] {
            assert!(
                !names.contains(&absent),
                "{absent} is built, and docs/specs/ui.md §3 says it is not taken"
            );
        }
    }

    /// A failed check is said again where the Checks page reads it.
    ///
    /// `cargo xtask all` is one step, so this line is the whole of what the
    /// acceptance criterion asks for: which check failed, without the log
    /// being opened.
    ///
    /// Mutation: change the prefix, and this fails. What it prevents is a
    /// message GitHub renders as an ordinary log line, which is the failure
    /// that looks exactly like success from here.
    #[test]
    fn a_failed_check_is_said_again_where_the_checks_page_reads_it() {
        assert_eq!(
            annotation(true, "gate: the clippy row failed").as_deref(),
            Some("::error::gate: the clippy row failed")
        );
    }

    /// A local run is not annotated.
    ///
    /// Mutation: return `Some` for both in `annotation`, and this fails. What
    /// it prevents is `::error::` appearing in the output somebody reads at a
    /// terminal, where it means nothing and hides the row it sits beside.
    #[test]
    fn a_local_run_is_not_annotated() {
        assert_eq!(annotation(false, "gate: the clippy row failed"), None);
    }

    /// A verdict becomes the exit code it means.
    ///
    /// No test can call `dispatch(Some("gate"))` without running the gate, so
    /// this is the only place the translation from "the rows passed" to "what
    /// the process returns" is asserted. Both 0 and 1 are checked: a code that
    /// is always 0 and one that is inverted are different mistakes and this
    /// catches each.
    ///
    /// Mutation: drop the `!` in `exit_code`, and this fails.
    #[test]
    fn a_verdict_becomes_the_exit_code_it_means() {
        assert_eq!(exit_code(true), 0);
        assert_eq!(exit_code(false), 1);
    }
}
