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

    passed &= cargo_row("fmt", &["fmt", "--all", "--", "--check"], |_| {});
    passed &= cargo_row(
        "clippy",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        |_| {},
    );
    passed &= cargo_row(
        "doc",
        &[
            "doc",
            "--workspace",
            "--no-deps",
            "--document-private-items",
        ],
        |c| {
            c.env("RUSTDOCFLAGS", "-Dwarnings");
        },
    );
    // `BEVY2D_BLESS` makes a snapshot corpus rewrite its expectations instead
    // of comparing against them. Somebody who blessed a moment ago should not
    // have the gate agree with whatever they blessed.
    passed &= cargo_row("test", &["test", "--workspace"], |c| {
        c.env_remove("BEVY2D_BLESS");
    });
    passed &= no_unsafe();

    if passed {
        println!("gate: passed");
    } else {
        println!("gate: FAILED");
    }
    passed
}

/// Run one cargo invocation as a row, printing `ok` or `FAIL` and a tail.
fn cargo_row(name: &str, args: &[&str], tweak: impl FnOnce(&mut Command)) -> bool {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(args).current_dir(workspace_root());
    tweak(&mut cmd);

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
/// it reaches a user's game, and the tests for this very function have to spell
/// the keyword out to have anything to match.
fn no_unsafe() -> bool {
    let root = workspace_root();
    let mut sources = Vec::new();
    for dir in ["crates", "src"] {
        collect_rust_files(&root.join(dir), &mut sources);
    }
    sources.sort();

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
    use super::unsafe_lines;

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

    /// A file with nothing to find reports nothing, which is what stops the
    /// row passing while it looks at the wrong thing.
    #[test]
    fn an_ordinary_file_has_no_hits() {
        assert_eq!(
            unsafe_lines("//! A crate.\n\npub fn f() -> u32 {\n    1\n}\n"),
            Vec::<usize>::new()
        );
    }
}
