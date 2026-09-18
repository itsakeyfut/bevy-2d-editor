//! The rows of the gate.
//!
//! **Every row runs.** The rows are collected into an array before a verdict is
//! taken, so one invocation reports everything that is wrong rather than the
//! first thing. The order therefore decides only what a reader sees first, not
//! how long a failing run takes: the five cargo rows come in rising cost, and
//! `no-unsafe`, which spawns no process at all, sits after them because it is
//! the workspace's own policy rather than something cargo can be asked.
//!
//! Each row's command is built by its own named function, so that what the row
//! actually runs is a thing a test can read. A row whose arguments drift is a
//! row that goes on printing `ok` about something other than its name.
//!
//! **Every row that can be is `--locked`.** Without it cargo resolves and
//! rewrites `Cargo.lock` on the way past, so a lockfile that disagrees with
//! what was committed is silently corrected and the gate passes about a
//! resolution nobody chose. It is unconditional rather than switched on by a
//! `CI` environment variable, because a gate that means one thing at a
//! terminal and another on a runner is two gates, and `CLAUDE.md` defines done
//! as the exit code of one command. What that costs: after a dependency is
//! added to a manifest, the next run fails with cargo's own
//! `cannot update the lock file ... because --locked was passed to prevent
//! this` until `cargo fetch` has run.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use crate::{annotation, on_github, workspace_root};

/// Run every row, and report whether all of them passed.
///
/// There is no MSRV row. `docs/specs/decisions.md` has the project tracking
/// Bevy's current development rather than pinning to a release, so a floor
/// written here would be fiction the moment Bevy moves. `rust-toolchain.toml`
/// pins the channel instead, which is the claim that can actually be kept.
pub fn run() -> bool {
    println!("gate:");
    let rows: [bool; ROW_COUNT] = [
        row("fmt", &mut fmt_command()),
        game(),
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

/// How many rows `run` takes a verdict over.
///
/// Written rather than inferred, so that dropping a row from the array is a
/// type error rather than a gate that goes on printing `gate: passed` about one
/// thing fewer. Changing this to agree with a dropped row fails
/// `one_failing_row_fails_the_gate`, which is the second half of the same
/// guard.
const ROW_COUNT: usize = 6;

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
///
/// The only row without `--locked`, because `cargo fmt` does not take it:
/// `cargo fmt --all --locked -- --check` answers
/// `error: unexpected argument '--locked' found`. That is a fact about cargo
/// rather than a choice, and it is said here so the absence does not read as
/// an oversight. Nothing is lost by it: `fmt` does not resolve dependencies,
/// so it cannot rewrite `Cargo.lock` either.
fn fmt_command() -> Command {
    cargo(&["fmt", "--all", "--", "--check"])
}

/// The crates a user's game depends on directly.
///
/// A second copy of `GAME_ENTRY_POINTS` in
/// `crates/editor/tests/dependency_direction.rs`, written out rather than read
/// from it, and held equal to it by
/// `the_gates_entry_point_list_agrees_with_the_dependency_graphs`. Two independently
/// written lists compared against each other is the shape RK-001 asks for: a
/// list read out of the file it is checking agrees with that file however wrong
/// both are.
pub(crate) const GAME_ENTRY_POINTS: &[&str] = &["b2d_runtime"];

/// The `game` row's command, for one entry point.
///
/// `check` rather than `build`: a `use` across a feature boundary fails at name
/// resolution, so linking catches nothing this row is about, and it costs a
/// link per entry point on each of the three platforms
/// `docs/specs/dev-environment.md` §3 runs.
///
/// There is no `--no-default-features` invocation beside it. What a crate's own
/// `default` contains is deferred in `docs/specs/open-questions.md` §1 with its
/// trigger, the first `[features]` table; answering it here would be bringing
/// that forward.
fn game_command(pkg: &str) -> Command {
    cargo(&["check", "-p", pkg, "--locked"])
}

/// Every crate a game reaches, resolved the way a game resolves it.
///
/// `docs/specs/crates.md` §3 has `data` putting its editor-only parts behind a
/// feature so a user's game compiles only what loading needs. `SHIPPED` in
/// `crates/editor/tests/dependency_direction.rs` holds the **declared** edge;
/// no other row here resolves the configuration a game actually gets. The
/// `clippy` row passes `--all-features`, and the `test` row builds the
/// workspace, where `b2d_editor` turns the feature on and cargo unifies it onto
/// `data`. So `runtime` using `data`'s editor-only code leaves every other row
/// green and fails in somebody's project instead.
///
/// The row is an addition rather than an edit to `clippy`. Dropping
/// `--all-features` there would make that row see the split and stop it linting
/// the feature-on configuration, and both configurations matter.
///
/// **What this assembly is not held to: anything at all.** Nothing calls it, so
/// replacing this body with `true` leaves every test in the workspace passing,
/// and so does passing a list written in place instead of `GAME_ENTRY_POINTS`.
/// Both were applied and the suite stayed green. The pieces it is built from
/// are each guarded: `game_rows`' two arms, `game_command`'s arguments,
/// `game_row_name`'s spelling, and the two lists held equal. The assembly is
/// not.
///
/// Closing it takes a test that calls this, which runs cargo inside the suite.
/// That is cheaper than it sounds: measured on this tree, such a test ran in
/// 0.04s, and `cargo xtask gate` with it in place took 5.3s in total, because
/// the row has already run the identical invocation by then and cargo reuses
/// it. What it costs instead is a unit test that spawns a process and resolves
/// a lockfile, and that trade is what is left open here.
fn game() -> bool {
    game_rows(GAME_ENTRY_POINTS, |pkg| {
        row(&game_row_name(pkg), &mut game_command(pkg))
    })
}

/// What one entry point's row is called.
///
/// The package is in the name so that the annotation `failure_report` puts on
/// the Checks page reads `gate: the game:b2d_runtime row failed` and says which
/// crate without the log being opened. Cargo's own error names it too, so what
/// this buys is not knowing rather than not being able to find out.
///
/// Built rather than formatted in place for the reason [`failure_report`] is:
/// otherwise dropping the package from the name fails nothing.
///
/// Mutation: return `"game".to_owned()`, and
/// `a_game_row_is_named_after_the_crate_it_compiled` fails.
fn game_row_name(pkg: &str) -> String {
    format!("game:{pkg}")
}

/// The `game` row's verdict over a given list of entry points.
///
/// What each entry point costs is passed in rather than called here, the way
/// `hits_for` takes the result of reading a file. Both arms are then claims a
/// test can make without spawning a cargo process per assertion, and the
/// empty-list arm has no cheap mutation from outside, the way `no_unsafe`'s
/// does not either.
///
/// **A row that examined no entry points is a failure**, for the reason
/// `no_unsafe` gives about a scan that examined no files: a list that quietly
/// emptied would print nothing and pass.
///
/// `&` rather than `&&`, so a second entry point still runs after the first has
/// failed. That is the rule `verdict` here and `all` in `main.rs` already keep:
/// one invocation reports everything that is wrong.
///
/// Mutation: change `&` to `&&`, and
/// `every_entry_point_runs_even_after_one_has_failed` fails.
fn game_rows(entry_points: &[&str], mut check: impl FnMut(&str) -> bool) -> bool {
    if entry_points.is_empty() {
        fail(
            "game",
            "examined no entry points, so this row is about nothing",
        );
        return false;
    }
    entry_points.iter().fold(true, |ok, pkg| ok & check(pkg))
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
        "--locked",
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
        "--locked",
    ]);
    cmd.env("RUSTDOCFLAGS", "-Dwarnings");
    cmd
}

/// The `test` row.
///
/// `--no-fail-fast` is the same rule the rest of this file keeps, one level
/// down. `cargo test --workspace` stops at the first failing **target**, not
/// the first failing test, so one crate going red hides every other crate in
/// the same run. Measured: with a failing test in `b2d_editor` and another in
/// `xtask`, the row reported the first and exited, and
/// `error: 2 targets failed:` is what it says with the flag.
///
/// `BEVY2D_BLESS` makes a snapshot corpus rewrite its expectations instead of
/// comparing against them. Somebody who blessed a moment ago should not have
/// the gate agree with whatever they blessed, so the variable is removed from
/// the child rather than merely left unset here.
fn test_command() -> Command {
    let mut cmd = cargo(&["test", "--workspace", "--locked", "--no-fail-fast"]);
    cmd.env_remove("BEVY2D_BLESS");
    cmd
}

/// Run one row, printing `ok` or `FAIL` and what it said.
///
/// # What a Windows exit code of `0xC0000409` in the log below means
///
/// It means a compiler process aborted, and nothing more than that. Windows
/// spells the code `STATUS_STACK_BUFFER_OVERRUN`, which is the report code
/// attached to `__fastfail`, and `__fastfail` is how `abort` is implemented on
/// `x86_64-pc-windows-msvc`. **The name is a spelling, not a diagnosis**, and
/// reading it as one cost a session here: see #27, where the cause of the one
/// confirmed occurrence is still open.
///
/// Two commands measure it, which is why they are written here rather than the
/// conclusion alone:
///
/// ```text
/// fn main() { std::process::abort(); }                   -> 0xC0000409, silently
/// Vec::<u8>::with_capacity(400 * 1024 * 1024 * 1024)     -> "memory allocation of ... failed"
///                                                        -> 0xC0000409
/// ```
///
/// So the log printed below separates the two cases on its own: an allocation
/// failure says `memory allocation of N bytes failed` before it goes, and a
/// bare `0xC0000409` came from somewhere that aborted without a word. Keeping
/// the log is therefore the whole of the evidence, and
/// `cargo xtask all > run.log 2>&1` is how.
///
/// **A rustc stack overflow does not look like this**, whatever the name
/// suggests. Measured on the crate that carries the `bsn!` invocations:
///
/// ```text
/// RUST_MIN_STACK=1048576 cargo build -p b2d_editor --bin b2d_editor -j1
///   -> exit 101, "thread ... has overflowed its stack"
/// RUST_MIN_STACK=2097152 cargo build -p b2d_editor --bin b2d_editor -j1
///   -> exit 0
/// ```
///
/// Exit 101 and a sentence, rather than a code and silence. Whatever else is
/// wrong when this appears, the rustc stack is not the thing to reach for.
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
/// Every failing row goes through here, `no-unsafe` included, which is what
/// makes one annotation in [`failure_report`] enough to name any of them on
/// the Checks page.
///
/// All of it, not a tail. A tail of the last lines of a cargo log is the
/// `could not compile, 8 previous errors` summary and none of the errors, and
/// the person reading it is often looking at a runner they cannot cheaply
/// rerun.
fn fail(name: &str, log: &str) {
    println!("{}", failure_report(name, log, on_github()));
}

/// Everything a failed row says, as one string.
///
/// Built rather than printed so that the annotation has a test. Without one,
/// deleting the line that emits it leaves every test in the workspace passing
/// and the Checks page silent about which row failed, which is the acceptance
/// criterion this exists for.
///
/// Mutation: drop the annotation, and
/// `a_failed_row_names_itself_to_the_checks_page` fails.
fn failure_report(name: &str, log: &str, on_github: bool) -> String {
    let mut report = format!("  FAIL  {name}");
    for line in log.lines() {
        report.push_str(&format!("\n        {line}"));
    }
    if let Some(line) = annotation(on_github, &format!("gate: the {name} row failed")) {
        report.push('\n');
        report.push_str(&line);
    }
    report
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
        fail(
            "no-unsafe",
            "examined no source files, so this row is about nothing",
        );
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
    fail("no-unsafe", &hits.join("\n"));
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
        GAME_ENTRY_POINTS, ROW_COUNT, clippy_command, doc_command, failure_report, fmt_command, fs,
        game_command, game_row_name, game_rows, hits_for, scanned_sources, test_command,
        unsafe_lines, verdict, workspace_root,
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

    /// A `game` row with no entry points fails rather than passing quietly.
    ///
    /// RK-001, in the shape this row can take it: a list that emptied would
    /// spawn nothing, print nothing, and leave the gate green about the
    /// configuration a user's game compiles.
    ///
    /// Mutation: drop the `is_empty` arm from `game_rows`, and this fails.
    /// Nothing else does.
    #[test]
    fn a_game_row_with_no_entry_points_fails() {
        let mut visited = Vec::new();
        let passed = game_rows(&[], |pkg| {
            visited.push(pkg.to_owned());
            true
        });
        assert!(!passed, "a row that examined nothing reported success");
        assert!(visited.is_empty(), "got {visited:?}");
    }

    /// Every entry point runs, even after an earlier one has failed.
    ///
    /// The same rule `verdict` here and `all` in `main.rs` keep: one invocation
    /// reports everything that is wrong. With one entry point today it is
    /// unobservable, which is exactly why it is asserted now rather than when
    /// `runtime_myphysics` arrives and the second crate stops being checked.
    ///
    /// Mutation: change `&` to `&&` in `game_rows`, and this fails naming the
    /// entry point that was never examined.
    #[test]
    fn every_entry_point_runs_even_after_one_has_failed() {
        let mut visited = Vec::new();
        let passed = game_rows(&["first", "second"], |pkg| {
            visited.push(pkg.to_owned());
            false
        });
        assert!(!passed);
        assert_eq!(visited, ["first", "second"]);
    }

    /// Every entry point passing is the row passing.
    #[test]
    fn a_game_row_passes_when_every_entry_point_does() {
        assert!(game_rows(&["first", "second"], |_| true));
    }

    /// The gate's own entry point list agrees with the dependency graph's.
    ///
    /// Named for what it compares, which is two written-out lists, and not for
    /// what a reader might hope it compares. It does **not** hold that the row
    /// wired into the gate reads either list; `game`'s doc comment says what is
    /// left open there. A test name here is held to what it asserts.
    ///
    /// Two independently written lists, compared against each other. Reading
    /// `GAME_ENTRY_POINTS` out of `dependency_direction.rs` and using it as the
    /// row's own list would agree with that file however wrong it was, which is
    /// the second half of RK-001; so the list here is written out and this
    /// holds the two equal.
    ///
    /// The non-empty assertion is not decoration: a pattern that stopped
    /// matching would compare an empty set against an empty set and pass.
    ///
    /// The text is taken to the `;` rather than to the end of the line, because
    /// rustfmt wraps the declaration once the list has more than one entry.
    ///
    /// **Comments are the hazard here, in three ways, and each one was measured
    /// reporting `ok` while the two lists genuinely disagreed.** A comment
    /// quoting the declaration is found ahead of the real item, so the needle
    /// begins with a newline and matches only an item at column zero, and the
    /// number of matches is asserted rather than trusting that. A comment
    /// *inside* the list containing a semicolon, which this repository's prose
    /// does constantly, truncates the text at that semicolon, so line comments
    /// are stripped before the `;` is looked for. A comment inside the list
    /// quoting a crate name would otherwise be read as an entry, which the same
    /// stripping handles.
    ///
    /// That is RK-001 arriving inside the guard written to apply RK-001: the
    /// set being compared was not the set anybody meant.
    ///
    /// Mutation: add a name to `GAME_ENTRY_POINTS` on either side and not the
    /// other, empty either one, drop the leading newline from `DECLARATION`, or
    /// drop the comment stripping and put a `;` in a comment inside the list,
    /// and this fails.
    #[test]
    fn the_gates_entry_point_list_agrees_with_the_dependency_graphs() {
        /// The declaration as an item, not as prose quoting one.
        const DECLARATION: &str = "
const GAME_ENTRY_POINTS";

        let path = workspace_root()
            .join("crates")
            .join("editor")
            .join("tests")
            .join("dependency_direction.rs");
        let source = fs::read_to_string(&path)
            .expect("the dependency graph's entry points live in dependency_direction.rs");
        let found = source.matches(DECLARATION).count();
        assert_eq!(
            found,
            1,
            "{} holds {found} declarations of GAME_ENTRY_POINTS, and this test reads whichever comes first",
            path.display()
        );
        let rest = source
            .split_once(DECLARATION)
            .map(|(_, rest)| rest)
            .expect("dependency_direction.rs declares GAME_ENTRY_POINTS");
        let uncommented: String = rest
            .lines()
            .map(|line| line.split("//").next().unwrap_or(line))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let declaration = uncommented
            .split_once(';')
            .map(|(decl, _)| decl)
            .expect("the declaration of GAME_ENTRY_POINTS ends in a semicolon");

        let named: Vec<&str> = declaration.split('"').skip(1).step_by(2).collect();
        assert!(
            !named.is_empty(),
            "no entry point was read out of {}, so this test compares nothing: {declaration}",
            path.display()
        );
        assert_eq!(
            named, GAME_ENTRY_POINTS,
            "the gate checks {GAME_ENTRY_POINTS:?} and the dependency graph says a game reaches {named:?}. A crate a game depends on directly is compiled the way a game resolves it or it is not checked at all"
        );
    }

    /// One failing row fails the gate, wherever it sits.
    ///
    /// Mutation: change `all` to `any` in `verdict`, or make `run` keep only
    /// the last row's result, and this fails.
    #[test]
    fn one_failing_row_fails_the_gate() {
        assert_eq!(
            ROW_COUNT, 6,
            "a row was added to or taken from `run` without this test being told,              so the position it occupies is asserted by nothing"
        );
        assert!(verdict(&[true; ROW_COUNT]));
        for i in 0..ROW_COUNT {
            let mut rows = [true; ROW_COUNT];
            rows[i] = false;
            assert!(!verdict(&rows), "the gate passed with row {i} failing");
        }
    }

    /// Each row runs the command its name promises.
    ///
    /// The row named `test` can become `cargo check --workspace` without any
    /// other test noticing: it would still exit 0, still print `ok`, and still
    /// have stopped running the suite.
    ///
    /// Mutation: change any subcommand, or drop `-D warnings`, `--check`,
    /// `--locked` or `--no-fail-fast`, and this fails naming the row.
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
                "--locked",
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
                "--document-private-items",
                "--locked"
            ]
        );
        assert_eq!(
            args(&test_command()),
            ["test", "--workspace", "--locked", "--no-fail-fast"]
        );
        assert_eq!(
            args(&game_command("b2d_runtime")),
            ["check", "-p", "b2d_runtime", "--locked"]
        );
    }

    /// The `game` row compiles the entry point it was handed.
    ///
    /// Said with a name that is not today's only entry point, because
    /// `game_command` can ignore its argument and hardcode `b2d_runtime`
    /// without the assertion above noticing: it passes that same name in. The
    /// day `runtime_myphysics` arrives, the row would compile one crate twice
    /// and print `ok` about the other, which is RK-001 in this row's shape and
    /// the reason acceptance criterion 3 of #15 exists.
    ///
    /// Mutation: ignore `pkg` in `game_command` and name a crate in its place,
    /// and this fails. Nothing else does.
    #[test]
    fn a_game_row_compiles_the_entry_point_it_was_handed() {
        assert_eq!(
            args(&game_command("runtime_myphysics")),
            ["check", "-p", "runtime_myphysics", "--locked"]
        );
    }

    /// A `game` row is named after the crate it compiled.
    ///
    /// The Checks page gets one annotation per failing row, built from the row
    /// name. Without the package in it, a red build says only that `game`
    /// failed, and which crate is a log away.
    ///
    /// Mutation: return `"game".to_owned()` from `game_row_name`, and this
    /// fails. Nothing else does.
    #[test]
    fn a_game_row_is_named_after_the_crate_it_compiled() {
        assert_eq!(game_row_name("runtime_myphysics"), "game:runtime_myphysics");
        assert!(
            failure_report(
                &game_row_name("b2d_runtime"),
                "error: x
",
                true
            )
            .ends_with(
                "
::error::gate: the game:b2d_runtime row failed"
            )
        );
    }

    /// Every row that cargo lets be `--locked` is `--locked`.
    ///
    /// Said separately from the row above, which is an exact argument list and
    /// would keep passing with a `--locked` moved to a row that happened to be
    /// rewritten. This one is about the property: a lockfile that disagrees
    /// with what was committed fails the gate rather than being corrected on
    /// the way past.
    ///
    /// `fmt` is the exception and is asserted as one, because `cargo fmt`
    /// rejects the flag. An exception nobody wrote down is an exception that
    /// spreads.
    ///
    /// Mutation: drop `--locked` from any of the three, and this fails naming
    /// the row.
    #[test]
    fn every_row_that_can_refuse_a_stale_lockfile_does() {
        for (name, cmd) in [
            ("clippy", clippy_command()),
            ("doc", doc_command()),
            ("test", test_command()),
            (&game_row_name("b2d_runtime"), game_command("b2d_runtime")),
        ] {
            assert!(
                args(&cmd).iter().any(|a| a == "--locked"),
                "the {name} row would rewrite Cargo.lock instead of failing on it"
            );
        }
        assert!(
            !args(&fmt_command()).iter().any(|a| a == "--locked"),
            "cargo fmt rejects --locked, so the fmt row cannot carry it"
        );
    }

    /// A failed row names itself to the Checks page, and only there.
    ///
    /// The row output a person reads at a terminal is unchanged either way: the
    /// annotation is an extra line and not a rewrite of the existing ones.
    ///
    /// Mutation: drop the annotation from `failure_report`, and this fails.
    /// Nothing else does, and what it prevents is a red build whose Checks page
    /// says only that something failed.
    #[test]
    fn a_failed_row_names_itself_to_the_checks_page() {
        let on_runner = failure_report("clippy", "error: something\n", true);
        assert!(
            on_runner.ends_with("\n::error::gate: the clippy row failed"),
            "{on_runner}"
        );
        assert!(on_runner.starts_with("  FAIL  clippy\n        error: something"));

        let locally = failure_report("clippy", "error: something\n", false);
        assert_eq!(locally, "  FAIL  clippy\n        error: something");
    }

    /// The invocation that starts the gate refuses a stale lockfile too.
    ///
    /// `cargo xtask` is itself `cargo run`, and it resolves before any row of
    /// the gate exists to be `--locked`. Measured, with the flag on every row
    /// and not on the alias: editing `Cargo.lock` to claim `regex 1.13.0` and
    /// running `cargo xtask gate` printed `gate: passed` and left the lockfile
    /// resolved back to `1.13.1`. Every row was `--locked` and not one of them
    /// ever saw the stale file.
    ///
    /// Mutation: drop `--locked` from the alias in `.cargo/config.toml`, and
    /// this fails. Nothing else does, which is the whole reason it is here: the
    /// gate goes on passing, about a resolution nobody committed.
    #[test]
    fn the_alias_that_starts_the_gate_refuses_a_stale_lockfile() {
        let path = workspace_root().join(".cargo").join("config.toml");
        let config =
            fs::read_to_string(&path).expect("the xtask alias lives in .cargo/config.toml");
        let alias = config
            .lines()
            .find(|line| line.starts_with("xtask = "))
            .expect("the xtask alias is defined in .cargo/config.toml");
        assert!(
            alias.contains("--locked"),
            "the alias resolves before the gate does, so it carries the flag too: {alias}"
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
