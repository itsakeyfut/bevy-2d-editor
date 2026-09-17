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
/// `cargo xtask all` is one step of one job, so without this the only way to
/// find out which check failed is to open the log of the job that failed.
/// One line per failed check puts the name on the summary page instead.
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
type Task = (&'static str, fn() -> bool);

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
const TASKS: [Task; 2] = [("gate", gate::run), ("docs", docs::run)];

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
        Some(name) => match TASKS.iter().find(|(task, _)| *task == name) {
            Some((_, run)) => run(),
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
/// Built from [`TASKS`] rather than written out, so a task added to the table
/// is a task the usage line already knows about.
fn usage() -> u8 {
    let tasks: Vec<&str> = TASKS.iter().map(|(task, _)| *task).collect();
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
/// This is what the workflow names, so that the list of tasks is written down
/// once. Taking the table as an argument is what makes it testable: `dispatch`
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

    use super::{TASKS, all, annotation, dispatch, docs, exit_code, gate, workspace_root};

    /// The workflow, read once for the tests that assert what it runs.
    fn workflow() -> String {
        let path = workspace_root()
            .join(".github")
            .join("workflows")
            .join("ci.yml");
        std::fs::read_to_string(&path).expect("the workflow is in the repository")
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
    /// pass. That is the shape RK-001 is about, and this asserts the table is
    /// the set that was meant.
    ///
    /// Mutation: remove either row from `TASKS`, and this fails.
    #[test]
    fn the_tasks_are_the_ones_the_workflow_runs() {
        let tasks: Vec<&str> = TASKS.iter().map(|(task, _)| *task).collect();
        assert_eq!(tasks, ["gate", "docs"]);
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
    }

    /// The workflow runs the one command, on the three platforms the
    /// specification names.
    ///
    /// `docs/specs/dev-environment.md` §3 decided both, and nothing else here
    /// can tell when a platform quietly leaves the matrix: the remaining legs
    /// stay green and the run still reports a pass. The development platform is
    /// the one that matters, which is why the rows are named rather than
    /// counted.
    ///
    /// Mutation: remove `windows-latest` from the matrix, change the step to
    /// `cargo xtask gate`, or pin `runs-on` to one platform, and this fails
    /// naming what went missing.
    #[test]
    fn the_workflow_runs_every_task_on_every_platform() {
        let workflow = workflow();
        assert!(
            workflow.contains("cargo xtask all"),
            "the workflow does not run every task"
        );
        for os in ["ubuntu-latest", "macos-latest", "windows-latest"] {
            assert!(workflow.contains(os), "the matrix does not name {os}");
        }
        // The names above are the matrix list, and a list is not a platform
        // until something runs on it. Pinning `runs-on` to one runner leaves
        // all three names in the file and every leg on the same machine, which
        // is the one failure this whole workflow exists to prevent.
        assert!(
            workflow.contains("runs-on: ${{ matrix.os }}"),
            "the legs are not wired to the matrix, so they all run on one platform"
        );
    }

    /// A leg that did not succeed fails the check branch protection requires.
    ///
    /// `needs.<job>.result` has four values, so a condition that enumerates
    /// the ones somebody thought of is a condition that lets the rest through.
    /// A skipped leg passing here is a required check going green over a
    /// commit nothing built.
    ///
    /// Mutation: enumerate the failing results instead, or drop the condition,
    /// and this fails.
    #[test]
    fn a_leg_that_did_not_succeed_fails_the_required_check() {
        let workflow = workflow();
        assert!(
            workflow.contains("if: needs.gate.result != 'success'"),
            "the required check does not insist that every leg succeeded"
        );
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
