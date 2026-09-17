//! The rows of the gate, in the order that fails cheapest first.
//!
//! The output is deliberately the same as the shell script this replaces, so
//! that nobody has to relearn what a passing run looks like.

use std::fs;
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
    let mut passed = true;

    passed &= row("fmt", &mut cargo(&["fmt", "--all", "--", "--check"]));
    passed &= row(
        "clippy",
        &mut cargo(&[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ]),
    );
    passed &= row("doc", &mut doc_command());
    passed &= row("test", &mut test_command());
    passed &= no_unsafe();

    if passed {
        println!("gate: passed");
    } else {
        println!("gate: FAILED");
    }
    passed
}

/// A cargo invocation rooted at the workspace, whichever directory the person
/// who typed `cargo xtask` happened to be standing in.
fn cargo(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(args).current_dir(workspace_root());
    cmd
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

/// Run one row, printing `ok` or `FAIL` and a tail of what it said.
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

/// Report a failed row, with the last 25 lines of what it said.
fn fail(name: &str, log: &str) {
    println!("  FAIL  {name}");
    let lines: Vec<&str> = log.lines().collect();
    for line in lines.iter().skip(lines.len().saturating_sub(25)) {
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
/// A scan that examined nothing reports `FAIL`, not `ok`. That is not caution
/// for its own sake: the check this crate replaces spent its last weeks
/// printing `ok` about a workspace it was no longer looking at, and a row that
/// cannot say whether it looked is that defect waiting to happen again.
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
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let shown = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string()
            .replace('\\', "/");
        for line in unsafe_lines(&text) {
            hits.push(format!("{shown}:{line}"));
        }
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
/// The `[^_[:alnum:]]` guard is what keeps `get_unsafe(` and `unsafely` out.
///
/// Mutation: drop that guard, and
/// `a_word_that_merely_contains_the_keyword_is_not_a_hit` fails. Drop the
/// `impl` alternative, and `a_declaration_is_a_hit` fails. Neither fails
/// anything else.
fn unsafe_lines(text: &str) -> Vec<usize> {
    let pattern = Regex::new(r"(^|[^_[:alnum:]])unsafe\s*[{(]|unsafe fn|unsafe impl")
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

    use super::{doc_command, scanned_sources, test_command, unsafe_lines};

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
