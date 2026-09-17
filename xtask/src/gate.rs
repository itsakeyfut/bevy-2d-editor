//! The rows of the gate.
//!
//! **Every row runs.** The rows are collected into an array before a verdict is
//! taken, so one invocation reports everything that is wrong rather than the
//! first thing. The order therefore decides only what a reader sees first, not
//! how long a failing run takes: the four cargo rows come in rising cost, and
//! `no-unsafe`, which spawns no process at all, sits after them because it is
//! the workspace's own policy rather than something cargo can be asked.
//!
//! Each row's command is built by its own named function, so that what the row
//! actually runs is a thing a test can read. A row whose arguments drift is a
//! row that goes on printing `ok` about something other than its name.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use crate::workspace_root;

/// Run every row, and report whether all of them passed.
///
/// There is no MSRV row. `docs/specs/decisions.md` has the project tracking
/// Bevy's current development rather than pinning to a release, so a floor
/// written here would be fiction the moment Bevy moves. `rust-toolchain.toml`
/// pins the channel instead, which is the claim that can actually be kept.
pub fn run() -> bool {
    println!("gate:");
    let rows = [
        row("fmt", &mut fmt_command()),
        row("clippy", &mut clippy_command()),
        row("doc", &mut doc_command()),
        row("test", &mut test_command()),
        no_unsafe(),
    ];
    let passed = verdict(&rows);
    if passed {
        println!("gate: passed");
    } else {
        println!("gate: FAILED");
    }
    passed
}

/// The gate passes only when every row did.
///
/// Mutation: change `all` to `any`, or take only the last row, and
/// `one_failing_row_fails_the_gate` fails.
fn verdict(rows: &[bool]) -> bool {
    rows.iter().all(|ok| *ok)
}

/// A cargo invocation rooted at the workspace, whichever directory the person
/// who typed `cargo xtask` happened to be standing in.
fn cargo(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(args).current_dir(workspace_root());
    cmd
}

/// The `fmt` row.
fn fmt_command() -> Command {
    cargo(&["fmt", "--all", "--", "--check"])
}

/// The `clippy` row.
///
/// `-D warnings` is the row: `missing_docs` and the lints in the workspace
/// manifest are warnings, and without this they stay warnings.
fn clippy_command() -> Command {
    cargo(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--",
        "-D",
        "warnings",
    ])
}

/// The `doc` row.
///
/// `RUSTDOCFLAGS=-Dwarnings` is the row. Without it rustdoc reports a broken
/// intra-doc link, exits 0, and the gate agrees with it.
fn doc_command() -> Command {
    let mut cmd = cargo(&[
        "doc",
        "--workspace",
        "--no-deps",
        "--document-private-items",
    ]);
    cmd.env("RUSTDOCFLAGS", "-Dwarnings");
    cmd
}

/// The `test` row.
///
/// `BEVY2D_BLESS` makes a snapshot corpus rewrite its expectations instead of
/// comparing against them. Somebody who blessed a moment ago should not have
/// the gate agree with whatever they blessed, so the variable is removed from
/// the child rather than merely left unset here.
fn test_command() -> Command {
    let mut cmd = cargo(&["test", "--workspace"]);
    cmd.env_remove("BEVY2D_BLESS");
    cmd
}

/// Run one row, printing `ok` or `FAIL` and what it said.
fn row(name: &str, cmd: &mut Command) -> bool {
    let out = match cmd.output() {
        Ok(out) => out,
        Err(err) => {
            fail(name, &format!("could not run cargo: {err}"));
            return false;
        }
    };
    if out.status.success() {
        println!("  ok    {name}");
        return true;
    }
    let mut log = String::from_utf8_lossy(&out.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&out.stderr));
    fail(name, &log);
    false
}

/// Report a failed row, with everything it said.
///
/// All of it, not a tail. A tail of the last lines of a cargo log is the
/// `could not compile, 8 previous errors` summary and none of the errors, and
/// the person reading it is often looking at a runner they cannot cheaply
/// rerun.
fn fail(name: &str, log: &str) {
    println!("  FAIL  {name}");
    for line in log.lines() {
        println!("        {line}");
    }
}

/// No source file in the workspace's crates contains `unsafe`.
///
/// Nothing here needs it: Bevy's API is safe, and `TilemapChunk`, reflection
/// and the asset system are all reachable without it. A block that appears is a
/// sign that something the engine already does is being done again by hand, so
/// the gate asks rather than assuming.
///
/// `xtask/` is not scanned. It is the tooling rather than the editor, none of
/// it reaches a user's game, and the tests for [`unsafe_lines`] have to spell
/// the keyword out to have anything to match.
///
/// **A file this cannot read is a failure, and a scan that examined nothing is
/// a failure.** Neither is caution for its own sake. The check this crate
/// replaces spent its last weeks printing `ok` about a workspace it was no
/// longer looking at, and "I skipped that one" is the same sentence said more
/// quietly: a stray source file saved in the wrong encoding is enough to hide a
/// live `unsafe` block behind a green row.
fn no_unsafe() -> bool {
    let sources = scanned_sources();
    if sources.is_empty() {
        println!("  FAIL  no-unsafe");
        println!("        examined no source files, so this row is about nothing");
        return false;
    }

    let root = workspace_root();
    let mut hits = Vec::new();
    for path in &sources {
        let shown = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string()
            .replace('\\', "/");
        hits.extend(hits_for(&shown, fs::read_to_string(path)));
    }

    if hits.is_empty() {
        println!("  ok    no-unsafe");
        return true;
    }
    println!("  FAIL  no-unsafe");
    for hit in &hits {
        println!("        {hit}");
    }
    false
}

/// What one source file contributes to the row, given what reading it produced.
///
/// Mutation: return `Vec::new()` for the `Err` arm, and
/// `a_file_that_cannot_be_read_is_a_hit` fails. That arm is the one that
/// matters: a file that will not decode is a file nothing in the workspace
/// examines, and skipping it quietly is how a green row comes to mean nothing.
fn hits_for(shown: &str, read: io::Result<String>) -> Vec<String> {
    match read {
        Ok(text) => unsafe_lines(&text)
            .into_iter()
            .map(|line| format!("{shown}:{line}"))
            .collect(),
        Err(err) => vec![format!(
            "{shown}: could not be read, so it was not checked: {err}"
        )],
    }
}

/// Every `.rs` file the `no-unsafe` row examines.
///
/// Split out so that "it examined something" is a claim with a test behind it.
fn scanned_sources() -> Vec<PathBuf> {
    let root = workspace_root();
    let mut sources = Vec::new();
    for dir in ["crates", "src"] {
        collect_rust_files(&root.join(dir), &mut sources);
    }
    sources.sort();
    sources
}

/// The 1-based lines of `text` that open an `unsafe` block or declare one.
///
/// Every shape the keyword takes, not the three that come to mind. Edition 2024
/// is what makes that worth spelling out: `unsafe extern` blocks are how FFI is
/// declared in it, `unsafe static` goes with them, and `#[unsafe(no_mangle)]`
/// spells an attribute. A pattern that knew only `fn`, `impl` and a block would
/// let an `unsafe trait` through while claiming the workspace had none.
///
/// The `[^_[:alnum:]]` guard is what keeps `get_unsafe(` and `unsafely` out.
///
/// Mutation: drop that guard, and
/// `a_word_that_merely_contains_the_keyword_is_not_a_hit` fails. Drop `trait`
/// or `extern` from the alternation, and `every_shape_of_the_keyword_is_a_hit`
/// fails. Neither fails anything else.
fn unsafe_lines(text: &str) -> Vec<usize> {
    let pattern =
        Regex::new(r"(^|[^_[:alnum:]])unsafe(\s*[{(]|\s+(fn|impl|trait|extern|static|async))")
            .expect("the pattern is a literal and compiles");
    text.lines()
        .enumerate()
        .filter(|(_, line)| pattern.is_match(line))
        .map(|(i, _)| i + 1)
        .collect()
}

/// Every `.rs` file under `dir`, recursively. A missing directory is not an
/// error: `src/` stops existing the moment a workspace becomes virtual.
fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::io;
    use std::process::Command;

    use super::{
        clippy_command, doc_command, fmt_command, hits_for, scanned_sources, test_command,
        unsafe_lines, verdict,
    };

    fn args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    /// A block and a declaration are both hits, and the line number is 1-based.
    ///
    /// Mutation: return `i` instead of `i + 1` in `unsafe_lines`, and this
    /// fails.
    #[test]
    fn a_declaration_is_a_hit() {
        let src = "fn a() {}\nunsafe fn b() {}\nunsafe impl Send for A {}\n";
        assert_eq!(unsafe_lines(src), vec![2, 3]);
    }

    /// An opened block is a hit whether or not anything precedes it.
    #[test]
    fn an_opened_block_is_a_hit() {
        let src = "unsafe { ptr.read() }\n    let x = unsafe { ptr.read() };\n";
        assert_eq!(unsafe_lines(src), vec![1, 2]);
    }

    /// Every shape the keyword takes is a hit, not the three that come to mind.
    ///
    /// `unsafe trait` has always been Rust, and edition 2024 made
    /// `unsafe extern` the way FFI is declared. A pattern that knew only `fn`,
    /// `impl` and a block passed both straight through while the row above it
    /// claimed the workspace had no `unsafe` in it.
    ///
    /// Mutation: remove `trait`, `extern`, `static` or `async` from the
    /// alternation, and this fails naming the line that got through.
    #[test]
    fn every_shape_of_the_keyword_is_a_hit() {
        let src = concat!(
            "pub unsafe trait Sync {}\n",
            "unsafe extern \"C\" {\n",
            "    unsafe static COUNT: u32;\n",
            "}\n",
            "pub unsafe async fn f() {}\n",
            "#[unsafe(no_mangle)]\n",
        );
        assert_eq!(unsafe_lines(src), vec![1, 2, 3, 5, 6]);
    }

    /// A word that merely contains the keyword is not a hit.
    ///
    /// Mutation: drop the `[^_[:alnum:]]` guard from the pattern, and this
    /// fails on the first line.
    #[test]
    fn a_word_that_merely_contains_the_keyword_is_not_a_hit() {
        let src = "fn get_unsafe() {}\nlet unsafely = 1;\nfn safe() {}\n";
        assert_eq!(unsafe_lines(src), Vec::<usize>::new());
    }

    /// A file with nothing to find reports nothing.
    #[test]
    fn an_ordinary_file_has_no_hits() {
        assert_eq!(
            unsafe_lines("//! A crate.\n\npub fn f() -> u32 {\n    1\n}\n"),
            Vec::<usize>::new()
        );
    }

    /// A file that cannot be read is a hit, not a file to skip.
    ///
    /// A stray source file saved as UTF-16, which one wrong choice in an editor
    /// on this project's own development platform produces, is not read by
    /// `fmt`, `clippy`, `doc` or `test` either. Skipping it here is the one
    /// place a live `unsafe` block can sit behind five green rows.
    ///
    /// Mutation: return `Vec::new()` for the `Err` arm of `hits_for`, and this
    /// fails. Nothing else does.
    #[test]
    fn a_file_that_cannot_be_read_is_a_hit() {
        let err = io::Error::new(
            io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        );
        let hits = hits_for("crates/core/src/stray.rs", Err(err));
        assert_eq!(hits.len(), 1, "got {hits:?}");
        assert!(hits[0].starts_with("crates/core/src/stray.rs: could not be read"));
    }

    /// A file that reads is reported by line.
    #[test]
    fn a_file_that_reads_is_reported_by_line() {
        let hits = hits_for(
            "crates/core/src/lib.rs",
            Ok("fn a() {}\nunsafe fn b() {}\n".into()),
        );
        assert_eq!(hits, vec!["crates/core/src/lib.rs:2".to_owned()]);
    }

    /// The `no-unsafe` row examines the crates that are actually there.
    ///
    /// Without this the row is free to look at nothing and print `ok`, which is
    /// how the dependency-graph check it sits beside died: written against
    /// names that had since been changed, it went on passing.
    ///
    /// Mutation: misspell either directory in `scanned_sources`, and this
    /// fails. Nothing else does.
    #[test]
    fn the_unsafe_scan_reads_the_crates_that_are_there() {
        let sources = scanned_sources();
        assert!(
            !sources.is_empty(),
            "the no-unsafe row examined no files at all"
        );
        assert!(
            sources.iter().any(|p| p.ends_with("lib.rs")),
            "the no-unsafe row found no crate root: {sources:?}"
        );
    }

    /// One failing row fails the gate, wherever it sits.
    ///
    /// Mutation: change `all` to `any` in `verdict`, or make `run` keep only
    /// the last row's result, and this fails.
    #[test]
    fn one_failing_row_fails_the_gate() {
        assert!(verdict(&[true, true, true, true, true]));
        assert!(!verdict(&[false, true, true, true, true]));
        assert!(!verdict(&[true, true, true, true, false]));
        assert!(!verdict(&[true, false, true, true, true]));
    }

    /// Each row runs the command its name promises.
    ///
    /// The row named `test` can become `cargo check --workspace` without any
    /// other test noticing: it would still exit 0, still print `ok`, and still
    /// have stopped running the suite.
    ///
    /// Mutation: change any subcommand or drop `-D warnings` or `--check`, and
    /// this fails naming the row.
    #[test]
    fn each_row_runs_the_command_its_name_promises() {
        assert_eq!(args(&fmt_command()), ["fmt", "--all", "--", "--check"]);
        assert_eq!(
            args(&clippy_command()),
            [
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings"
            ]
        );
        assert_eq!(
            args(&doc_command()),
            [
                "doc",
                "--workspace",
                "--no-deps",
                "--document-private-items"
            ]
        );
        assert_eq!(args(&test_command()), ["test", "--workspace"]);
    }

    /// The `doc` row makes a rustdoc warning an error.
    ///
    /// Mutation: drop the `env` call in `doc_command`, and this fails. Nothing
    /// else does, because every other row still passes on a workspace whose
    /// documentation has quietly stopped building.
    #[test]
    fn the_doc_row_makes_a_rustdoc_warning_an_error() {
        let cmd = doc_command();
        let flag = cmd
            .get_envs()
            .find(|(key, _)| *key == OsStr::new("RUSTDOCFLAGS"))
            .and_then(|(_, value)| value);
        assert_eq!(flag, Some(OsStr::new("-Dwarnings")));
    }

    /// The `test` row refuses a blessing inherited from the shell.
    ///
    /// `env_remove` records the key with no value, which is what unsetting it
    /// in the child looks like from here.
    ///
    /// Mutation: drop the `env_remove` call, and this fails. Nothing else does,
    /// and what it prevents is a snapshot corpus agreeing with whatever
    /// somebody blessed a moment ago.
    #[test]
    fn the_test_row_refuses_an_inherited_blessing() {
        let cmd = test_command();
        let entry = cmd
            .get_envs()
            .find(|(key, _)| *key == OsStr::new("BEVY2D_BLESS"));
        assert_eq!(entry, Some((OsStr::new("BEVY2D_BLESS"), None)));
    }
}
