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
fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ sits under the workspace root")
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
        Some("gate") => gate::run(),
        Some("docs") => docs::run(),
        other => {
            if let Some(name) = other {
                eprintln!("xtask: `{name}` is not a task");
            }
            eprintln!("usage: cargo xtask <gate|docs>");
            return 2;
        }
    };
    u8::from(!passed)
}

#[cfg(test)]
mod tests {
    use super::dispatch;

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
        assert_eq!(dispatch(None), 2);
    }
}
