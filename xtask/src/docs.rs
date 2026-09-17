//! Check the documents against each other, mechanically.
//!
//! `docs/` was one file until it was split into sixteen, and the split is what
//! makes this necessary: a reference that used to be a number inside one
//! document is now a link into another, and nothing but a check notices when
//! one of them stops resolving. That split has already produced nested links
//! and section numbers pointing at sections that had moved, both of which read
//! as correct.
//!
//! It also checks what `docs/adr/README.md` promises and cannot keep on its
//! own: every record has a row and every row a record, the status in a record's
//! front matter matches its row, and every record answers Confirmation.
//!
//! What this cannot check is whether a document still says the right thing. A
//! section can survive and mean something else, and whether the mutation a
//! record names still fails the test it names is the same kind of question.
//! Those parts are a person's.
//!
//! # Why this is a map and not a directory
//!
//! [`Docs::problems`] is a pure function of [`Docs`], and [`Docs::read`] is the
//! only part that touches a filesystem. Every check below therefore has a test
//! built from string literals, with no fixture directory to create and nothing
//! to clean up. That is the difference between a check with a named test and a
//! check with a comment claiming it works.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use regex::Regex;

/// A `[label](target)` link on one line.
const LINK: &str = r"\[([^\]\n]+)\]\(([^)\n]+)\)";

/// The documents, and every path that exists beside them.
pub struct Docs {
    /// Every `.md` file under `docs/`, keyed by its path from the workspace
    /// root with `/` separators.
    markdown: BTreeMap<String, String>,
    /// Every path under `docs/`, files and directories alike. A link may point
    /// at a directory, as `docs/README.md` does at `./specs/`.
    present: BTreeSet<String>,
}

/// Read the documents, check them, and report whether they agree.
pub fn run() -> bool {
    let Some(docs) = Docs::read(crate::workspace_root()) else {
        println!("  docs/ does not exist");
        return false;
    };
    let problems = docs.problems();
    if problems.is_empty() {
        println!("  (nothing)");
    } else {
        for problem in &problems {
            println!("  {problem}");
        }
    }
    println!(
        "\nfiles: {}  records: {}  problems: {}",
        docs.markdown.len(),
        docs.records().len(),
        problems.len()
    );
    problems.is_empty()
}

impl Docs {
    /// Read `docs/` under `root`, or `None` when there is no `docs/`.
    pub fn read(root: &Path) -> Option<Docs> {
        let top = root.join("docs");
        if !top.is_dir() {
            return None;
        }
        let mut docs = Docs {
            markdown: BTreeMap::new(),
            present: BTreeSet::new(),
        };
        docs.present.insert("docs".to_owned());
        docs.walk(root, &top);
        Some(docs)
    }

    fn walk(&mut self, root: &Path, dir: &Path) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(root) else {
                continue;
            };
            let rel = rel.display().to_string().replace('\\', "/");
            self.present.insert(rel.clone());
            if path.is_dir() {
                self.walk(root, &path);
            } else if rel.ends_with(".md")
                && let Ok(text) = fs::read_to_string(&path)
            {
                self.markdown.insert(rel, text);
            }
        }
    }

    /// The record files in `docs/adr/`, as bare file names.
    fn records(&self) -> Vec<&str> {
        let name = Regex::new(r"^\d{4}-.*\.md$").expect("a literal pattern");
        self.markdown
            .keys()
            .filter_map(|path| path.strip_prefix("docs/adr/"))
            .filter(|leaf| !leaf.contains('/') && name.is_match(leaf))
            .collect()
    }

    /// Everything the documents promise about each other and do not keep.
    ///
    /// Mutation: return `Vec::new()` here, and every test in this module fails.
    /// That is what stops the check reporting success about nothing, which is
    /// how the dependency-graph check in the script this replaces spent its
    /// last weeks.
    pub fn problems(&self) -> Vec<String> {
        let link = Regex::new(LINK).expect("a literal pattern");
        let nested = Regex::new(r"\[[^\]\n]*\[[^\]\n]*\]\([^)\n]*\)").expect("a literal pattern");
        let heading = Regex::new(r"(?m)^#{2,3} (\d+(?:\.\d+)?)\. ").expect("a literal pattern");
        let trailing = Regex::new(r"§(\d+(?:\.\d+)?)\s*$").expect("a literal pattern");
        let anywhere = Regex::new(r"§(\d+(?:\.\d+)?)").expect("a literal pattern");

        let bodies: BTreeMap<String, String> = self
            .markdown
            .iter()
            .map(|(path, text)| (path.clone(), prose(text)))
            .collect();
        let sections: BTreeMap<String, BTreeSet<String>> = bodies
            .iter()
            .map(|(path, body)| {
                let found = heading
                    .captures_iter(body)
                    .map(|c| c[1].to_owned())
                    .collect();
                (path.clone(), found)
            })
            .collect();

        let mut bad = Vec::new();

        for (path, body) in &bodies {
            let here = path.rsplit_once('/').map_or("", |(dir, _)| dir);

            // 1. A link inside a link. The split produced these by rewriting a
            //    reference that had already been rewritten.
            for found in nested.find_iter(body) {
                let shown: String = found.as_str().chars().take(60).collect();
                bad.push(format!("{path} has a nested link: {shown}"));
            }

            for caps in link.captures_iter(body) {
                let (label, target) = (&caps[1], &caps[2]);
                if target.starts_with("http://")
                    || target.starts_with("https://")
                    || target.starts_with('#')
                {
                    continue;
                }
                let bare = target.split('#').next().unwrap_or("");
                if bare.is_empty() {
                    continue;
                }

                // 2. Every relative link resolves. Anchors are ignored: the
                //    anchor a heading generates is not something to reproduce.
                let full = normalize(here, bare);
                if !self.present.contains(&full) {
                    bad.push(format!("{path} links {target}, which does not exist"));
                    continue;
                }

                // 3. A link labelled `file.md §N` claims that section is there.
                if let Some(claim) = trailing.captures(label)
                    && let Some(has) = sections.get(&full)
                    && !has.contains(&claim[1])
                {
                    bad.push(format!(
                        "{path} links '{label}', and {full} has no section {}",
                        &claim[1]
                    ));
                }
            }

            // 4. A `§N` that is not inside a link refers to this file, so this
            //    file has to have it. Links go first: their numbers are check 3.
            let outside_links = link.replace_all(body, "");
            for caps in anywhere.captures_iter(&outside_links) {
                if !sections[path].contains(&caps[1]) {
                    bad.push(format!(
                        "{path} refers to §{}, which it does not have",
                        &caps[1]
                    ));
                }
            }

            // 5. Numbering is consecutive from 1, so that a reference written
            //    today still means the same section tomorrow.
            let mut tops: Vec<u32> = sections[path]
                .iter()
                .filter(|n| !n.contains('.'))
                .filter_map(|n| n.parse().ok())
                .collect();
            tops.sort_unstable();
            for (i, got) in tops.iter().enumerate() {
                let want = i as u32 + 1;
                if want != *got {
                    bad.push(format!("{path} jumps from §{} to §{got}", want - 1));
                    break;
                }
            }
        }

        self.check_decision_index(&link, &anywhere, &sections, &mut bad);
        self.check_records(&mut bad);
        bad
    }

    /// 6. Every row of the decision index points at a file that exists, and at
    ///    a section that is still there. The index is the one place that claims
    ///    to know where every decision lives.
    fn check_decision_index(
        &self,
        link: &Regex,
        anywhere: &Regex,
        sections: &BTreeMap<String, BTreeSet<String>>,
        bad: &mut Vec<String>,
    ) {
        const INDEX: &str = "docs/specs/decisions.md";
        let Some(text) = self.markdown.get(INDEX) else {
            bad.push(format!("{INDEX} is missing"));
            return;
        };
        let row = Regex::new(r"(?m)^\|([^|\n]+)\|([^|\n]+)\|[ \t\r]*$").expect("a literal pattern");

        let mut linked = 0;
        for caps in row.captures_iter(text) {
            // The `| --- | --- |` separator is a row and says nothing.
            if caps[1].trim().starts_with('-') {
                continue;
            }
            let Some(found) = link.captures(&caps[2]) else {
                continue;
            };
            linked += 1;
            let (label, target) = (found[1].to_owned(), found[2].to_owned());
            let full = normalize("docs/specs", &target);
            if !self.present.contains(&full) {
                bad.push(format!(
                    "the decision index points at {target}, which does not exist"
                ));
                continue;
            }
            if let Some(claim) = anywhere.captures(&label)
                && let Some(has) = sections.get(&full)
                && !has.contains(&claim[1])
            {
                bad.push(format!(
                    "the decision index says {label}, and that section is gone"
                ));
            }
        }
        if linked == 0 {
            bad.push(
                "the decision index has no rows, which is either a wipe or a bad parse".into(),
            );
        }
    }

    /// 7. The records. `docs/adr/README.md` makes three promises that rot
    ///    silently: every record has a row, every status matches, and every
    ///    record says what guards the decision now.
    fn check_records(&self, bad: &mut Vec<String>) {
        if !self.present.contains("docs/adr") {
            return;
        }
        const INDEX: &str = "docs/adr/README.md";
        let Some(index) = self.markdown.get(INDEX) else {
            bad.push(format!("{INDEX} is missing"));
            return;
        };
        let records = self.records();

        for leaf in &records {
            if !index.contains(&format!("({leaf})")) && !index.contains(&format!("(./{leaf})")) {
                bad.push(format!("{leaf} has no row in the ADR index"));
            }
        }
        let linked = Regex::new(r"\|\s*\[\d{4}\]\((?:\./)?([^)]+)\)").expect("a literal pattern");
        for caps in linked.captures_iter(index) {
            if !self.present.contains(&format!("docs/adr/{}", &caps[1])) {
                bad.push(format!(
                    "the ADR index links {}, which does not exist",
                    &caps[1]
                ));
            }
        }

        let front = Regex::new("(?m)^status:\\s*\"?([^\"\n]+)\"?").expect("a literal pattern");
        let mut by_status: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for leaf in &records {
            let text = &self.markdown[&format!("docs/adr/{leaf}")];
            let Some(caps) = front.captures(text) else {
                bad.push(format!("{leaf} has no status in its front matter"));
                continue;
            };
            let status = caps[1].trim();
            let word = status.split_whitespace().next().unwrap_or(status);
            by_status
                .entry(word.to_owned())
                .or_default()
                .push(&leaf[..4]);

            let row = index
                .lines()
                .find(|l| l.contains(&format!("({leaf})")) || l.contains(&format!("(./{leaf})")));
            if let Some(row) = row
                && !row.contains(word)
            {
                bad.push(format!(
                    "{leaf} is '{status}' but its index row does not say so"
                ));
            }

            // Confirmation is the section that goes stale without anyone
            // noticing: it answers what guards the decision *now*, not what was
            // true when it was written.
            match text.split_once("### Confirmation") {
                None => bad.push(format!("{leaf} has no Confirmation section")),
                Some((_, rest)) => {
                    let body = rest.split("\n### ").next().unwrap_or("");
                    if body.trim().is_empty() {
                        bad.push(format!("{leaf} leaves Confirmation empty"));
                    }
                }
            }
        }

        let Some(line) = index.lines().find(|l| l.starts_with("**By status**")) else {
            bad.push("the ADR index has no by-status line".into());
            return;
        };
        for (status, ids) in &by_status {
            for id in ids {
                if !line.contains(id) {
                    bad.push(format!(
                        "{id} is {status} and is missing from the by-status line"
                    ));
                }
            }
        }
    }
}

/// The document without its fenced blocks. A diagram may draw anything.
fn prose(text: &str) -> String {
    let fence = Regex::new(r"(?s)```.*?```").expect("a literal pattern");
    fence.replace_all(text, "").into_owned()
}

/// Resolve `rel` against the directory `here`, both with `/` separators.
///
/// Mutation: drop the `".." => { parts.pop(); }` arm, and
/// `a_link_that_climbs_out_of_its_directory_resolves` fails.
fn normalize(here: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = here.split('/').filter(|p| !p.is_empty()).collect();
    for part in rel.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::{Docs, normalize};
    use std::collections::{BTreeMap, BTreeSet};

    impl Docs {
        /// Build from literals. Every markdown path and the directories above
        /// it are present; `extra` covers what is not a document.
        fn of(markdown: &[(&str, &str)], extra: &[&str]) -> Docs {
            let mut present: BTreeSet<String> = extra
                .iter()
                .map(|p| (*p).to_owned())
                .collect::<BTreeSet<_>>();
            let mut files = BTreeMap::new();
            for (path, text) in markdown {
                files.insert((*path).to_owned(), (*text).to_owned());
                let mut dir = *path;
                while let Some((parent, _)) = dir.rsplit_once('/') {
                    present.insert(parent.to_owned());
                    dir = parent;
                }
                present.insert((*path).to_owned());
            }
            Docs {
                markdown: files,
                present,
            }
        }
    }

    /// A minimal decision index, so that check 6 is satisfied and the test
    /// under way is the only thing reporting.
    const INDEX: &str = "# Decisions\n\n| Decision | Where |\n| --- | --- |\n| A thing | [crates.md §1](./crates.md) |\n";

    /// A file with one section, numbered from 1.
    const CRATES: &str = "# Crates\n\n## 1. Layout\n\nText.\n";

    fn base() -> Vec<(&'static str, &'static str)> {
        vec![
            ("docs/specs/decisions.md", INDEX),
            ("docs/specs/crates.md", CRATES),
        ]
    }

    /// A healthy set of documents reports nothing.
    ///
    /// This is the vacuity guard: without it every test below would pass
    /// against a `problems` that reports a problem for everything.
    #[test]
    fn documents_that_agree_have_no_problems() {
        assert_eq!(Docs::of(&base(), &[]).problems(), Vec::<String>::new());
    }

    /// A link nested inside another link's label is reported.
    ///
    /// Mutation: delete the `nested` loop, and this fails.
    #[test]
    fn a_nested_link_is_reported() {
        let mut files = base();
        files.push((
            "docs/specs/ui.md",
            "# UI\n\nSee [[crates.md §1](./crates.md)](./crates.md).\n",
        ));
        let found = Docs::of(&files, &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("nested link")),
            "got {found:?}"
        );
    }

    /// A relative link to a file that is not there is reported.
    ///
    /// Mutation: delete the `present.contains` branch, and this fails.
    #[test]
    fn a_link_to_a_file_that_is_not_there_is_reported() {
        let mut files = base();
        files.push(("docs/specs/ui.md", "# UI\n\nSee [gone](./gone.md).\n"));
        let found = Docs::of(&files, &[]).problems();
        assert!(
            found
                .iter()
                .any(|p| p.contains("links ./gone.md, which does not exist")),
            "got {found:?}"
        );
    }

    /// A link may point at a directory, which is not a markdown file.
    ///
    /// `docs/README.md` links `./specs/`, and a check that only knew about
    /// documents would call the whole directory missing.
    #[test]
    fn a_link_to_a_directory_resolves() {
        let mut files = base();
        files.push(("docs/README.md", "# Docs\n\nSee [specs](./specs/).\n"));
        assert_eq!(Docs::of(&files, &[]).problems(), Vec::<String>::new());
    }

    /// A link labelled with a section the target does not have is reported.
    ///
    /// Mutation: delete the `trailing` check, and this fails.
    #[test]
    fn a_link_claiming_a_section_the_target_lacks_is_reported() {
        let mut files = base();
        files.push((
            "docs/specs/ui.md",
            "# UI\n\nSee [crates.md §9](./crates.md).\n",
        ));
        let found = Docs::of(&files, &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("has no section 9")),
            "got {found:?}"
        );
    }

    /// A `§N` inside a link label belongs to the file it points at, not to the
    /// file it is written in.
    ///
    /// Mutation: stop stripping links before check 4, and this fails. It is
    /// not hypothetical: doing exactly that reported 165 problems that were
    /// all this.
    #[test]
    fn a_section_number_inside_a_link_label_is_not_a_self_reference() {
        let mut files = base();
        files.push((
            "docs/specs/ui.md",
            "# UI\n\n## 1. Look\n\nSee [crates.md §1](./crates.md).\n",
        ));
        assert_eq!(Docs::of(&files, &[]).problems(), Vec::<String>::new());
    }

    /// A bare `§N` that this file does not have is reported.
    #[test]
    fn a_bare_section_reference_this_file_lacks_is_reported() {
        let mut files = base();
        files.push(("docs/specs/ui.md", "# UI\n\n## 1. Look\n\nSee §4.\n"));
        let found = Docs::of(&files, &[]).problems();
        assert!(
            found
                .iter()
                .any(|p| p.contains("refers to §4, which it does not have")),
            "got {found:?}"
        );
    }

    /// A fenced block is a diagram and may draw anything, including a `§`.
    ///
    /// Mutation: stop stripping fences in `prose`, and this fails.
    #[test]
    fn a_fenced_block_is_not_prose() {
        let mut files = base();
        files.push((
            "docs/specs/ui.md",
            "# UI\n\n## 1. Look\n\n```text\nsee §7\n## 5. Not a heading\n```\n",
        ));
        assert_eq!(Docs::of(&files, &[]).problems(), Vec::<String>::new());
    }

    /// A gap in the top-level numbering is reported.
    ///
    /// Mutation: delete the `break`, and this reports twice; delete the loop,
    /// and it reports nothing.
    #[test]
    fn a_gap_in_the_numbering_is_reported() {
        let mut files = base();
        files.push((
            "docs/specs/ui.md",
            "# UI\n\n## 1. One\n\n## 3. Three\n\n## 4. Four\n",
        ));
        let found = Docs::of(&files, &[]).problems();
        assert_eq!(
            found,
            vec!["docs/specs/ui.md jumps from §1 to §3".to_owned()]
        );
    }

    /// A decision index row pointing at a section that is gone is reported.
    #[test]
    fn a_decision_index_row_naming_a_vanished_section_is_reported() {
        let files = vec![
            (
                "docs/specs/decisions.md",
                "# Decisions\n\n| Decision | Where |\n| --- | --- |\n| A thing | [crates.md §9](./crates.md) |\n",
            ),
            ("docs/specs/crates.md", CRATES),
        ];
        let found = Docs::of(&files, &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("that section is gone")),
            "got {found:?}"
        );
    }

    /// A decision index with no rows is a wipe or a bad parse, and either way
    /// the check that walks it is reporting about nothing.
    ///
    /// Mutation: delete the `linked == 0` branch, and this fails.
    #[test]
    fn a_decision_index_with_no_rows_is_reported() {
        let files = vec![("docs/specs/decisions.md", "# Decisions\n\nNothing here.\n")];
        let found = Docs::of(&files, &[]).problems();
        assert!(
            found.iter().any(|p| p.contains("has no rows")),
            "got {found:?}"
        );
    }

    fn with_adr(record: &'static str, index: &'static str) -> Vec<(&'static str, &'static str)> {
        let mut files = base();
        files.push(("docs/adr/README.md", index));
        files.push(("docs/adr/0001-a-choice.md", record));
        files
    }

    const ADR_INDEX: &str = "# Records\n\n| # | Decision | Status | Confirmed by |\n| --- | --- | --- | --- |\n| [0001](./0001-a-choice.md) | A choice | accepted | a test |\n\n**By status**: accepted: 0001\n";

    const RECORD: &str = "---\nstatus: \"accepted\"\n---\n\n# A choice\n\n### Confirmation\n\nA named test fails when the choice is undone.\n";

    /// A healthy record and index report nothing, so that the three tests
    /// below are each reporting their own mutation.
    #[test]
    fn a_record_that_agrees_with_its_index_has_no_problems() {
        assert_eq!(
            Docs::of(&with_adr(RECORD, ADR_INDEX), &[]).problems(),
            Vec::<String>::new()
        );
    }

    /// A record with no row in the index is reported.
    ///
    /// Mutation: delete the `has no row` branch, and this fails.
    #[test]
    fn a_record_with_no_row_in_the_index_is_reported() {
        let index = "# Records\n\n| # | Decision | Status | Confirmed by |\n| --- | --- | --- | --- |\n\n**By status**: accepted: 0001\n";
        let found = Docs::of(&with_adr(RECORD, index), &[]).problems();
        assert!(
            found
                .iter()
                .any(|p| p.contains("has no row in the ADR index")),
            "got {found:?}"
        );
    }

    /// A record whose Confirmation is empty is reported.
    ///
    /// Mutation: delete the Confirmation branch, and this fails. Confirmation
    /// is the section that goes stale without anyone noticing.
    #[test]
    fn a_record_that_leaves_confirmation_empty_is_reported() {
        let record = "---\nstatus: \"accepted\"\n---\n\n# A choice\n\n### Confirmation\n\n### Next\n\nWords.\n";
        let found = Docs::of(&with_adr(record, ADR_INDEX), &[]).problems();
        assert!(
            found
                .iter()
                .any(|p| p.contains("leaves Confirmation empty")),
            "got {found:?}"
        );
    }

    /// A record whose status disagrees with its index row is reported.
    ///
    /// Mutation: delete the `row.contains(word)` branch, and this fails.
    #[test]
    fn a_record_whose_status_disagrees_with_its_row_is_reported() {
        let record = "---\nstatus: \"superseded by ADR-0002\"\n---\n\n# A choice\n\n### Confirmation\n\nStill guarded.\n";
        let found = Docs::of(&with_adr(record, ADR_INDEX), &[]).problems();
        assert!(
            found
                .iter()
                .any(|p| p.contains("but its index row does not say so")),
            "got {found:?}"
        );
    }

    /// A link that climbs out of its directory resolves against the parent.
    ///
    /// `docs/adr/README.md` links `../specs/README.md`, so getting this wrong
    /// would report every one of those as missing.
    #[test]
    fn a_link_that_climbs_out_of_its_directory_resolves() {
        assert_eq!(
            normalize("docs/specs", "../adr/README.md"),
            "docs/adr/README.md"
        );
        assert_eq!(normalize("docs", "./specs/"), "docs/specs");
        assert_eq!(
            normalize("docs/specs", "./crates.md"),
            "docs/specs/crates.md"
        );
    }
}
