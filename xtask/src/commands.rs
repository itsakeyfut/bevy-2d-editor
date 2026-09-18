//! Check that a command named in prose is one that can be run here.
//!
//! A document that tells a reader to run something is making a claim about
//! this repository, and it is the one kind of claim nothing else looks at.
//! `docs/adr/README.md` told readers to run a script under `.claude/`, which
//! is not committed and had already been replaced; `cargo xtask docs` reported
//! `problems: 0` about it the whole time, because it resolves links and
//! section numbers and never reads a command.
//!
//! # Why this is not part of `docs`
//!
//! `Docs`, in `docs.rs`, means `docs/`, in the name of its fields and in all
//! seven of its checks. Commands are said mostly elsewhere: measured over the
//! tracked tree when this was written, 18 in `.rs` doc comments, 14 in `.md`,
//! one in a comment in `.cargo/config.toml` and one in an issue template. A
//! separate corpus with its own summary line keeps those seven checks looking
//! at what they were written for.
//!
//! # What prose is, per file type
//!
//! A `.md` file is prose throughout. A `.rs` file is prose only in its
//! comments: a backticked span inside a string literal is a test fixture, and
//! the fixtures for this very check would otherwise be read as claims about
//! the repository they are written to test. `.toml` is prose in its `#`
//! comments. A `.yml` file is prose throughout, because the claim this check
//! was extended to catch lives in an issue template's `placeholder` block
//! rather than in a comment.
//!
//! # What it deliberately does not do
//!
//! It answers only what this repository can answer: whether a task, a package
//! or a file exists here. Whether `git` is installed is not that, and a check
//! that pretended otherwise would be confidently wrong. Nor does it judge the
//! sentence around a command: a real command described in a false sentence is
//! a reader's job, and pretending a script can tell would produce the same
//! kind of wrong answer.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use regex::Regex;

/// What a file has to be named to be read.
///
/// Mutation: drop `"rs"`, and `the_corpus_holds_the_files_it_meant` fails.
const READ: [&str; 4] = ["md", "rs", "toml", "yml"];

/// Commands that are named on purpose and cannot be run.
///
/// A row is the file, the command exactly as it is written, and why it is
/// there. A row that matches nothing is itself a problem: an exemption that
/// outlives the line it was written for sends the next reader to preserve
/// something that is already gone, which is the reason an entry in
/// `.claude/review/knowledge.md` names an anchor.
///
/// Mutation: change either of the first two fields, and
/// `an_exemption_that_matches_nothing_is_reported` fails.
const NOT_RUNNABLE: [(&str, &str, &str); 1] = [(
    "xtask/src/main.rs",
    "cargo xtask gates",
    "names a misspelling on purpose: that it is not a task is the subject",
)];

/// The prose of the workspace, and everything a command can be checked against.
pub struct Prose {
    /// Every readable file whose extension is in [`READ`], keyed by its path
    /// from the workspace root with `/` separators.
    files: BTreeMap<String, String>,
    /// Every path the walk saw, files and directories alike. What a claim
    /// about a script is answered against.
    present: BTreeSet<String>,
    /// Every package name declared by a manifest under the walk. What a claim
    /// about `-p` is answered against.
    packages: BTreeSet<String>,
    /// What went wrong while reading, rather than while checking: a file that
    /// would not decode, or a `.gitignore` this cannot interpret. Kept rather
    /// than skipped, because a file that silently leaves the corpus is RK-001.
    read_problems: Vec<String>,
}

/// Read the prose, check it, and report whether every command named in it can
/// be run.
pub fn run() -> bool {
    let on_github = crate::on_github();
    let prose = Prose::read(crate::workspace_root());
    let problems = prose.problems();
    if problems.is_empty() {
        println!("  (nothing)");
    } else {
        for problem in &problems {
            println!("  {problem}");
        }
    }
    println!(
        "\n{}",
        closing(
            &summary(prose.files.len(), prose.claims().len(), problems.len()),
            problems.len(),
            on_github,
        )
    );
    problems.is_empty()
}

/// What a run ends with: the summary, and the annotation naming this check
/// when there is a Checks page to read it.
///
/// Built rather than printed so the annotation has a test, for the reason
/// `docs.rs` gives about its own.
///
/// Mutation: drop the annotation, and
/// `a_failing_run_names_the_commands_to_the_checks_page` fails.
fn closing(summary: &str, problems: usize, on_github: bool) -> String {
    let mut out = summary.to_owned();
    if problems > 0
        && let Some(line) =
            crate::annotation(on_github, "commands: a command named here cannot be run")
    {
        out.push('\n');
        out.push_str(&line);
    }
    out
}

/// The line a person greps for after a run.
///
/// `commands` counts the claims this could answer, not every backticked
/// span: `commands: 0` beside `problems: 0` is a check that read nothing.
fn summary(files: usize, commands: usize, problems: usize) -> String {
    format!("files: {files}  commands: {commands}  problems: {problems}")
}

impl Prose {
    /// Read the workspace under `root`.
    ///
    /// What is not walked comes from `.gitignore` rather than from a list
    /// here, because a list here is a second copy to keep in step, and the day
    /// the two disagree is the day an ignored file is read and this check
    /// fails over prose nobody committed. `CLAUDE.md` is the live example: it
    /// names four scripts under `.claude/` that have not existed since the
    /// checks moved into the repository, and it is not in the repository
    /// either.
    ///
    /// `git ls-files` would answer too and is not used: `workspace_root`
    /// already refuses to need a git checkout, and RK-002 records that a
    /// tier-2 review runs in a copied tree with no `.git` in it. There,
    /// `git ls-files` returns nothing and this would report success about a
    /// corpus it never had.
    pub fn read(root: &Path) -> Prose {
        let mut prose = Prose {
            files: BTreeMap::new(),
            present: BTreeSet::new(),
            packages: BTreeSet::new(),
            read_problems: Vec::new(),
        };
        let skip = prose.excluded(root);
        prose.walk(root, root, &skip);
        prose
    }

    /// The root entries the walk does not descend into.
    ///
    /// Every line of `.gitignore` here is root-anchored, so a line beginning
    /// `/` names one of these and **a line that does not is a problem**: this
    /// cannot tell what such a line excludes, and guessing would either read
    /// an ignored file or stop reading a committed one.
    ///
    /// `.git` is added because it is never listed in `.gitignore` and is not
    /// prose.
    ///
    /// Mutation: accept a line that is not root-anchored, and
    /// `a_gitignore_line_that_is_not_root_anchored_is_reported` fails.
    fn excluded(&mut self, root: &Path) -> BTreeSet<String> {
        let mut skip = BTreeSet::from([".git".to_owned()]);
        let path = root.join(".gitignore");
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) => {
                self.read_problems
                    .push(format!(".gitignore could not be read: {err}"));
                return skip;
            }
        };
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line.strip_prefix('/') {
                Some(entry) => {
                    skip.insert(entry.trim_end_matches('/').to_owned());
                }
                None => self.read_problems.push(format!(
                    ".gitignore line {} is not root-anchored, so the corpus cannot tell what it excludes: {line}",
                    number + 1
                )),
            }
        }
        skip
    }

    fn walk(&mut self, root: &Path, dir: &Path, skip: &BTreeSet<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.display().to_string().replace('\\', "/");
            if skip.contains(&rel) {
                continue;
            }
            self.present.insert(rel.clone());
            if path.is_dir() {
                self.walk(root, &path, skip);
                continue;
            }
            if rel.ends_with("Cargo.toml") {
                self.read_package(&path);
            }
            if !READ.iter().any(|ext| rel.ends_with(&format!(".{ext}"))) {
                continue;
            }
            match fs::read_to_string(&path) {
                Ok(text) => {
                    self.files.insert(rel, text);
                }
                Err(err) => self
                    .read_problems
                    .push(format!("{rel} is there and could not be read: {err}")),
            }
        }
    }

    /// Record the package a manifest declares, if it declares one.
    ///
    /// The needle is anchored at column zero. A `name` indented under some
    /// other table is not the package's, and RK-001's newest instance is a
    /// check that read Rust source as text and was defeated by three ordinary
    /// comments.
    fn read_package(&mut self, path: &Path) {
        let Ok(text) = fs::read_to_string(path) else {
            // The read failure is reported by `walk`, which reads it again.
            return;
        };
        let name = Regex::new("(?m)^name\\s*=\\s*\"([^\"]+)\"").expect("a literal pattern");
        if let Some(caps) = name.captures(&text) {
            self.packages.insert(caps[1].to_owned());
        }
    }

    /// Every claim the corpus makes that this repository can answer: the file
    /// that makes it, the command as written, and what it says exists.
    ///
    /// A span this cannot answer for is not a claim and is not counted.
    /// `cargo build` and `git diff` are real, and whether they run is not
    /// something this repository decides; counting them would make the summary
    /// line say a thousand things were examined when a dozen were.
    fn claims(&self) -> Vec<(&str, String, Claim)> {
        let mut out = Vec::new();
        for (path, text) in &self.files {
            for command in commands(path, text) {
                if let Some(claim) = claim(&command) {
                    out.push((path.as_str(), command, claim));
                }
            }
        }
        out
    }

    /// Every command named in prose that cannot be run here.
    ///
    /// Mutation: return `Vec::new()`, and every test in this module fails.
    pub fn problems(&self) -> Vec<String> {
        let mut bad = self.read_problems.clone();
        let mut matched = [false; NOT_RUNNABLE.len()];
        let mut named_xtask = false;

        for (file, command, claim) in self.claims() {
            // A command carrying a placeholder is a shape rather than a claim.
            // `dependency_direction.rs` names one, to say which packages the
            // gate's game row checks rather than to be typed.
            if command.contains('<') && command.contains('>') {
                continue;
            }
            if let Some(row) = NOT_RUNNABLE
                .iter()
                .position(|(path, exempt, _)| *path == file && *exempt == command)
            {
                matched[row] = true;
                continue;
            }
            if command.starts_with("cargo xtask") {
                named_xtask = true;
            }
            let problem = match claim {
                Claim::Task(task) => (task != "all"
                    && !crate::TASKS.iter().any(|(name, _)| *name == task))
                .then(|| format!("{file} names `{command}`, and {task} is not a task")),
                Claim::Script(target) => (!self.present.contains(&target))
                    .then(|| format!("{file} names `{command}`, and {target} is not here")),
                Claim::Package(package) => (!self.packages.contains(&package)).then(|| {
                    format!("{file} names `{command}`, and there is no package {package}")
                }),
            };
            if let Some(problem) = problem {
                bad.push(problem);
            }
        }

        if named_xtask && !self.defines_the_xtask_alias() {
            bad.push(
                "prose names `cargo xtask`, and .cargo/config.toml does not define that alias"
                    .to_owned(),
            );
        }
        for (row, seen) in matched.iter().enumerate() {
            if !seen {
                let (file, command, _) = NOT_RUNNABLE[row];
                bad.push(format!(
                    "the exemption for `{command}` in {file} matches nothing"
                ));
            }
        }
        bad
    }

    /// Whether `cargo xtask` resolves to anything.
    ///
    /// Without the alias every command beginning that way is unrunnable, and
    /// the task check above would go on saying each of them is fine.
    ///
    /// Mutation: return `true` unconditionally, and
    /// `prose_naming_the_alias_needs_the_alias_to_be_there` fails.
    fn defines_the_xtask_alias(&self) -> bool {
        let alias = Regex::new("(?m)^xtask\\s*=").expect("a literal pattern");
        self.files
            .get(".cargo/config.toml")
            .is_some_and(|text| alias.is_match(text))
    }
}

/// What a command says exists here.
///
/// Three shapes carry a claim this repository can answer, and everything else
/// is read and passed over.
enum Claim {
    /// `cargo xtask <task>`: the table in `main.rs` has to have it.
    ///
    /// Mutation: accept every task, and
    /// `a_task_that_is_not_in_the_table_is_reported` fails.
    Task(String),
    /// A path handed to `bash` or `sh`, or run with a leading `./`: the file
    /// has to be in the repository.
    ///
    /// Mutation: stop looking at `present`, and
    /// `a_command_naming_a_file_that_is_not_here_is_reported` fails.
    Script(String),
    /// `-p x`: the workspace has to declare that package.
    ///
    /// Mutation: stop looking at `packages`, and
    /// `a_package_that_is_not_in_the_workspace_is_reported` fails.
    Package(String),
}

/// What one command claims, when it claims anything answerable.
///
/// A task name is read here and checked against the table itself rather than
/// scraped out of the source that declares it, which is the sharpest clause of
/// RK-001: the guard written to hold the gate's entry point list read the
/// other file as text, and three ordinary comments defeated it.
fn claim(command: &str) -> Option<Claim> {
    let words: Vec<&str> = command.split_whitespace().collect();
    match words.as_slice() {
        ["cargo", "xtask", task, ..] => Some(Claim::Task((*task).to_owned())),
        ["bash" | "sh", target, ..] if !target.starts_with('-') => {
            Some(Claim::Script((*target).to_owned()))
        }
        [target, ..] if target.starts_with("./") && !target.ends_with('/') => {
            Some(Claim::Script(target.trim_start_matches("./").to_owned()))
        }
        ["cargo", ..] => words
            .windows(2)
            .find(|pair| pair[0] == "-p" || pair[0] == "--package")
            .map(|pair| Claim::Package(pair[1].to_owned())),
        _ => None,
    }
}

/// Every command-shaped span in one file's prose.
///
/// A fenced block tagged as a shell is read as commands, one per line, before
/// the fences are stripped: a block that says it is a shell session is the
/// strongest form of the claim. No tracked file has one today, and
/// `CLAUDE.md`'s own gate block is exactly that shape, so a document that
/// gains one should not gain a blind spot with it.
///
/// Mutation: drop the shell-fence arm, and `a_shell_block_is_read` fails.
fn commands(path: &str, text: &str) -> Vec<String> {
    let inline = Regex::new("`([^`\n]+)`").expect("a literal pattern");
    let fence = Regex::new("(?s)```.*?```").expect("a literal pattern");
    let shell = Regex::new("(?s)```(?:sh|bash|console)\r?\n(.*?)```").expect("a literal pattern");

    let mut out = Vec::new();
    let body = prose(path, text);
    if path.ends_with(".md") {
        for caps in shell.captures_iter(&body) {
            for line in caps[1].lines() {
                let line = line.trim().trim_start_matches("$ ").trim();
                let line = line.split('#').next().unwrap_or("").trim();
                if !line.is_empty() {
                    out.push(line.to_owned());
                }
            }
        }
    }
    let body = fence.replace_all(&body, "");
    for caps in inline.captures_iter(&body) {
        out.push(caps[1].trim().to_owned());
    }
    out
}

/// The part of a file that is prose, which depends on what the file is.
///
/// A `.rs` file is prose in its comments only. Its string literals hold the
/// fixtures of checks like this one, and reading those would make this check
/// report about the repository its own tests describe rather than about this
/// one.
///
/// Mutation: return `text` for every path, and
/// `a_string_literal_in_rust_is_not_prose` fails.
fn prose<'a>(path: &str, text: &'a str) -> std::borrow::Cow<'a, str> {
    let comments = |marker: &str| -> std::borrow::Cow<'a, str> {
        text.lines()
            .filter_map(|line| line.trim_start().strip_prefix(marker))
            .collect::<Vec<&str>>()
            .join("\n")
            .into()
    };
    if path.ends_with(".rs") {
        comments("//")
    } else if path.ends_with(".toml") {
        comments("#")
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::{NOT_RUNNABLE, Prose, closing, commands, summary};
    use std::collections::{BTreeMap, BTreeSet};

    impl Prose {
        /// Build from literals. Every file path and the directories above it
        /// are present; `extra` covers what is not read.
        fn of(files: &[(&str, &str)], extra: &[&str], packages: &[&str]) -> Prose {
            let mut present: BTreeSet<String> = extra.iter().map(|p| (*p).to_owned()).collect();
            let mut read = BTreeMap::new();
            for (path, text) in files {
                read.insert((*path).to_owned(), (*text).to_owned());
                let mut dir = *path;
                while let Some((parent, _)) = dir.rsplit_once('/') {
                    present.insert(parent.to_owned());
                    dir = parent;
                }
                present.insert((*path).to_owned());
            }
            Prose {
                files: read,
                present,
                packages: packages.iter().map(|p| (*p).to_owned()).collect(),
                read_problems: Vec::new(),
            }
        }
    }

    /// The alias definition, so that a fixture naming a task is not also
    /// reporting that the alias is missing.
    const ALIAS: &str = "[alias]\nxtask = \"run --quiet --locked --package xtask --\"\n";

    /// A corpus with the alias in it, the exemption satisfied, and nothing
    /// else to say.
    fn base() -> Vec<(&'static str, &'static str)> {
        vec![(".cargo/config.toml", ALIAS), ("xtask/src/main.rs", MAIN)]
    }

    /// Stands in for the file the exemption names, so that every fixture below
    /// starts from a corpus where the exemption matches something.
    const MAIN: &str = "/// A workflow that types `cargo xtask gates` reports a green build.\n";

    /// A corpus whose commands can all be run reports nothing.
    ///
    /// This is the vacuity guard: without it every test below would pass
    /// against a `problems` that reports a problem for everything.
    #[test]
    fn prose_naming_commands_that_exist_has_no_problems() {
        assert_eq!(
            Prose::of(&base(), &[], &[]).problems(),
            Vec::<String>::new()
        );
    }

    /// A command naming a file that is not in the repository is reported.
    ///
    /// This is the issue's own proof: `docs/adr/README.md` told readers to run
    /// a script under a directory that is not committed, and seven checks over
    /// the same documents said `problems: 0`.
    ///
    /// Mutation: return `None` from `judge_script`, and this fails.
    #[test]
    fn a_command_naming_a_file_that_is_not_here_is_reported() {
        let mut files = base();
        files.push((
            "docs/adr/README.md",
            "# Records\n\nRun `bash .claude/scripts/docs.sh`.\n",
        ));
        let found = Prose::of(&files, &[], &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("docs/adr/README.md")
                && p.contains(".claude/scripts/docs.sh")
                && p.contains("is not here")),
            "got {found:?}"
        );
    }

    /// The same, written with a leading `./` rather than an interpreter.
    ///
    /// Mutation: drop the `./` arm from `judge`, and this fails.
    #[test]
    fn a_relative_command_naming_a_file_that_is_not_here_is_reported() {
        let mut files = base();
        files.push(("docs/specs/ui.md", "# UI\n\nRun `./tools/build.sh`.\n"));
        let found = Prose::of(&files, &[], &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("tools/build.sh")),
            "got {found:?}"
        );
    }

    /// A script that is there is not reported.
    #[test]
    fn a_command_naming_a_file_that_is_here_is_not_reported() {
        let mut files = base();
        files.push(("docs/specs/ui.md", "# UI\n\nRun `bash tools/build.sh`.\n"));
        let found = Prose::of(&files, &["tools/build.sh"], &[]).problems();
        assert_eq!(found, Vec::<String>::new());
    }

    /// A task that is not in the table is reported.
    ///
    /// The acceptance criterion names this case: a document naming a task
    /// called `lint` has to fail, because the table does not have one.
    ///
    /// Mutation: accept every task in `judge_task`, and this fails.
    #[test]
    fn a_task_that_is_not_in_the_table_is_reported() {
        let mut files = base();
        files.push(("docs/specs/ui.md", "# UI\n\nRun `cargo xtask lint`.\n"));
        let found = Prose::of(&files, &[], &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("lint is not a task")),
            "got {found:?}"
        );
    }

    /// Every task in the table is accepted, and so is `all`.
    ///
    /// Read from `crate::TASKS` rather than written out here. Writing the
    /// names out would make this pass while a document naming a newly added
    /// task failed, which is the check telling the truth about the wrong set.
    ///
    /// Mutation: compare against a literal list in `judge_task`, and adding a
    /// task makes this fail.
    #[test]
    fn every_task_in_the_table_is_accepted() {
        let names: Vec<&str> = crate::TASKS.iter().map(|(name, _)| *name).collect();
        assert!(!names.is_empty(), "there are no tasks to check against");
        for name in names.iter().chain(["all"].iter()) {
            let text = format!("# UI\n\nRun `cargo xtask {name}`.\n");
            let files = [
                (".cargo/config.toml", ALIAS),
                ("xtask/src/main.rs", MAIN),
                ("docs/specs/ui.md", text.as_str()),
            ];
            assert_eq!(
                Prose::of(&files, &[], &[]).problems(),
                Vec::<String>::new(),
                "the task {name} was rejected"
            );
        }
    }

    /// A package that the workspace does not declare is reported.
    ///
    /// The live instance was an issue template naming `-p editor`, which the
    /// `b2d_` rename in `docs/specs/crates.md` §4 turned into `b2d_editor`.
    ///
    /// Mutation: return `None` from `judge_package`, and this fails.
    #[test]
    fn a_package_that_is_not_in_the_workspace_is_reported() {
        let mut files = base();
        files.push((
            ".github/ISSUE_TEMPLATE/task.yml",
            "placeholder: |\n  `cargo test -p editor tilemap::`\n",
        ));
        let found = Prose::of(&files, &[], &["b2d_editor"]).problems();
        assert!(
            found.iter().any(|p| p.contains("no package editor")),
            "got {found:?}"
        );
    }

    /// A package the workspace does declare is not reported.
    #[test]
    fn a_package_that_is_in_the_workspace_is_not_reported() {
        let mut files = base();
        files.push((
            "docs/specs/ui.md",
            "# UI\n\nRun `cargo test -p b2d_editor`.\n",
        ));
        assert_eq!(
            Prose::of(&files, &[], &["b2d_editor"]).problems(),
            Vec::<String>::new()
        );
    }

    /// A command carrying a placeholder is a shape, not a claim.
    ///
    /// `crates/editor/tests/dependency_direction.rs` names one to say which
    /// packages the gate's game row checks.
    ///
    /// Mutation: drop the placeholder rule in `problems`, and this fails.
    #[test]
    fn a_shape_is_not_a_claim() {
        let mut files = base();
        files.push((
            "crates/editor/tests/dependency_direction.rs",
            "/// The other half is `cargo check -p <entry point>`.\n",
        ));
        assert_eq!(Prose::of(&files, &[], &[]).problems(), Vec::<String>::new());
    }

    /// A directory is not a command.
    ///
    /// `xtask/src/docs.rs` names `./specs/` twice, as a link target.
    ///
    /// Mutation: drop the trailing-slash condition in `judge`, and this fails.
    #[test]
    fn a_directory_is_not_a_command() {
        let mut files = base();
        files.push((
            "xtask/src/docs.rs",
            "/// A link may point at a directory, as `docs/README.md` does at `./specs/`.\n",
        ));
        assert_eq!(Prose::of(&files, &[], &[]).problems(), Vec::<String>::new());
    }

    /// A command named on purpose and known not to run is exempt.
    ///
    /// Mutation: drop the `NOT_RUNNABLE` lookup in `problems`, and this fails:
    /// the file the row names is in every fixture here.
    #[test]
    fn an_exempt_command_is_not_reported() {
        assert_eq!(
            Prose::of(&base(), &[], &[]).problems(),
            Vec::<String>::new()
        );
        assert!(
            MAIN.contains(NOT_RUNNABLE[0].1),
            "the fixture no longer carries the exempt command"
        );
    }

    /// An exemption that matches nothing is reported.
    ///
    /// The acceptance criterion asks for a way to mark a command as not
    /// runnable that is not turning the check off. A row that outlives its
    /// line would be exactly that, quietly.
    ///
    /// Mutation: drop the `matched` loop in `problems`, and this fails.
    #[test]
    fn an_exemption_that_matches_nothing_is_reported() {
        let files = vec![(".cargo/config.toml", ALIAS)];
        let found = Prose::of(&files, &[], &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("matches nothing")),
            "got {found:?}"
        );
    }

    /// An exemption belongs to the file it was written for.
    ///
    /// Without this, a row excuses its command everywhere: any document could
    /// name the misspelling and be waved through, which is the acceptance
    /// criterion's "turn the check off" arriving quietly.
    ///
    /// Mutation: drop the file comparison from the `NOT_RUNNABLE` lookup, and
    /// this fails.
    #[test]
    fn an_exemption_does_not_travel_to_another_file() {
        let mut files = base();
        files.push((
            "docs/specs/ui.md",
            "# UI

Run `cargo xtask gates`.
",
        ));
        let found = Prose::of(&files, &[], &[]).problems();
        assert!(
            found
                .iter()
                .any(|p| p.contains("docs/specs/ui.md") && p.contains("gates is not a task")),
            "got {found:?}"
        );
    }

    /// Prose naming the alias needs the alias to be there.
    ///
    /// Mutation: return `true` from `defines_the_xtask_alias`, and this fails.
    #[test]
    fn prose_naming_the_alias_needs_the_alias_to_be_there() {
        let files = vec![
            (".cargo/config.toml", "[build]\n"),
            ("xtask/src/main.rs", MAIN),
            ("docs/specs/ui.md", "# UI\n\nRun `cargo xtask docs`.\n"),
        ];
        let found = Prose::of(&files, &[], &[]).problems();
        assert!(
            found
                .iter()
                .any(|p| p.contains("does not define that alias")),
            "got {found:?}"
        );
    }

    /// A string literal in Rust is not prose.
    ///
    /// The fixtures of this very check are string literals naming commands
    /// that do not exist. Reading them would make the check report about the
    /// repository its own tests describe.
    ///
    /// Mutation: return `text` for every path in `prose`, and this fails.
    #[test]
    fn a_string_literal_in_rust_is_not_prose() {
        let mut files = base();
        files.push((
            "xtask/src/commands.rs",
            "fn f() { let s = \"run `cargo xtask lint` here\"; }\n",
        ));
        assert_eq!(Prose::of(&files, &[], &[]).problems(), Vec::<String>::new());
    }

    /// A shell block is read, one command per line.
    ///
    /// Mutation: drop the shell-fence arm in `commands`, and this fails.
    #[test]
    fn a_shell_block_is_read() {
        let text =
            "# Gate\n\n```sh\ncargo xtask all       # everything\nbash tools/build.sh\n```\n";
        let found = commands("docs/specs/ui.md", text);
        assert_eq!(found, ["cargo xtask all", "bash tools/build.sh"]);
    }

    /// A block that is not a shell block says nothing about commands.
    ///
    /// `docs/` holds 71 `text` blocks and 11 `rust` ones, and a directory tree
    /// drawn in one of them is not a claim that anything can be run.
    #[test]
    fn a_block_that_is_not_a_shell_is_not_read() {
        let text = "# Tree\n\n```text\ncargo xtask lint\n```\n";
        assert!(commands("docs/specs/ui.md", text).is_empty());
    }

    /// A backticked span inside a fenced block is not read.
    ///
    /// `docs/` holds 71 blocks tagged `text` and 11 tagged `rust`, and a tree
    /// or a signature drawn in one of them may contain anything. `docs.rs`
    /// strips fences before it reads prose for the same reason.
    ///
    /// Mutation: stop stripping fences in `commands`, and this fails.
    #[test]
    fn a_span_inside_a_fence_is_not_read() {
        let text = "# Tree

```text
run `cargo xtask lint` here
```
";
        assert!(commands("docs/specs/ui.md", text).is_empty());
    }

    /// A failing run names this check to the Checks page.
    ///
    /// Mutation: drop the annotation in `closing`, and this fails.
    #[test]
    fn a_failing_run_names_the_commands_to_the_checks_page() {
        assert_eq!(
            closing("files: 1  commands: 1  problems: 1", 1, true),
            "files: 1  commands: 1  problems: 1\n::error::commands: a command named here cannot be run"
        );
        assert_eq!(
            closing("files: 1  commands: 1  problems: 0", 0, true),
            "files: 1  commands: 1  problems: 0"
        );
    }

    /// The summary says how much was read, not only what was wrong.
    #[test]
    fn the_summary_says_how_much_was_read() {
        assert_eq!(summary(41, 23, 0), "files: 41  commands: 23  problems: 0");
    }

    /// The count in the summary is the claims this could answer.
    ///
    /// It is the number a person reads to decide the check examined anything,
    /// so counting every backticked span would have it say a thousand things
    /// were read when a dozen were. That is RK-001 in the one place the whole
    /// check reports about itself.
    ///
    /// Mutation: count every span in `claims`, and this fails.
    #[test]
    fn the_count_is_the_claims_this_could_answer() {
        let files = [
            (".cargo/config.toml", ALIAS),
            ("xtask/src/main.rs", MAIN),
            (
                "docs/specs/ui.md",
                "# UI

`TilemapChunk`, `Handle<Image>`, and `cargo xtask docs`.
",
            ),
        ];
        let prose = Prose::of(&files, &[], &[]);
        assert_eq!(
            prose.claims().len(),
            2,
            "got {:?}",
            prose
                .claims()
                .iter()
                .map(|(file, command, _)| format!("{file}: {command}"))
                .collect::<Vec<String>>()
        );
    }

    /// A `.gitignore` line this cannot interpret is reported.
    ///
    /// Guessing would either read an ignored file, and fail over prose nobody
    /// committed, or stop reading a committed one, and report success about a
    /// corpus with a hole in it.
    ///
    /// Mutation: ignore the line instead of reporting it, and this fails.
    #[test]
    fn a_gitignore_line_that_is_not_root_anchored_is_reported() {
        let dir = std::env::temp_dir().join("b2d-commands-gitignore");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        std::fs::write(dir.join(".gitignore"), "/target\n*.tmp\n").expect("a .gitignore");
        let found = Prose::read(&dir).problems();
        std::fs::remove_dir_all(&dir).expect("the temporary directory goes away");
        assert!(
            found
                .iter()
                .any(|p| p.contains("line 2") && p.contains("not root-anchored")),
            "got {found:?}"
        );
    }

    /// The corpus holds the files it meant, and not the ignored ones.
    ///
    /// Read from the real tree, and asserted against names taken from
    /// somewhere other than the corpus itself: the two `xtask` sources, the
    /// alias file the gate depends on, and a document. RK-001 is a check that
    /// reports success about a set it never examined.
    ///
    /// Mutation: drop `"rs"` from `READ`, or stop skipping what `.gitignore`
    /// excludes, and this fails.
    #[test]
    fn the_corpus_holds_the_files_it_meant() {
        let prose = Prose::read(crate::workspace_root());
        for wanted in [
            ".cargo/config.toml",
            "xtask/src/main.rs",
            "xtask/src/commands.rs",
            "docs/adr/README.md",
            ".github/ISSUE_TEMPLATE/task.yml",
        ] {
            assert!(prose.files.contains_key(wanted), "{wanted} was not read");
        }
        for unwanted in ["CLAUDE.md", "TODO.md"] {
            assert!(
                !prose.files.contains_key(unwanted),
                "{unwanted} is excluded by .gitignore and was read anyway"
            );
        }
        assert!(
            !prose.files.keys().any(|path| path.starts_with("target/")),
            "target/ was walked"
        );
        assert!(
            prose.claims().len() > 10,
            "only {} commands were found in the whole tree",
            prose.claims().len()
        );
    }

    /// The packages are the ones the gate reaches.
    ///
    /// Asserted against the gate's entry point list rather than against the
    /// manifests this read, so a manifest walk that found nothing fails here
    /// rather than passing quietly.
    ///
    /// Mutation: anchor the `name` needle anywhere but column zero, or stop
    /// reading manifests, and this fails.
    #[test]
    fn the_packages_are_the_ones_the_gate_reaches() {
        let prose = Prose::read(crate::workspace_root());
        for entry in crate::gate::GAME_ENTRY_POINTS {
            assert!(
                prose.packages.contains(*entry),
                "{entry} is an entry point and is not a package this read"
            );
        }
        assert!(
            prose.packages.contains("xtask"),
            "the workspace's own tooling is not a package this read"
        );
    }

    /// The repository names only commands it has.
    ///
    /// The check over the real tree, which is the whole point of it. Without
    /// this, every test above could pass over literals while the repository
    /// itself said something that cannot be run.
    ///
    /// Mutation: put the `.claude/scripts/docs.sh` line back into
    /// `docs/adr/README.md`, and this fails naming the document and the
    /// command.
    #[test]
    fn the_repository_names_only_commands_it_has() {
        assert_eq!(
            Prose::read(crate::workspace_root()).problems(),
            Vec::<String>::new()
        );
    }
}
