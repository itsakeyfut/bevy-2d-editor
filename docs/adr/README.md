# Architecture Decision Records

How the code came to be shaped the way it is.

Format: [MADR 4.0](https://adr.github.io/madr/).
Copy [`adr-template.md`](./adr-template.md) to start one.

---

## The line between this and `docs/specs/`

**Two homes for reasoning will eventually disagree.** So the line is drawn.

| | [`docs/specs/`](../specs/README.md) | `docs/adr/` |
| --- | --- | --- |
| Decided when | in a design session, before there is code | during `/spec` on an issue, or while implementing it |
| Decided about | **what to build** | **how the code is shaped** |
| Example | BSN over a custom format; a manuscript UI instead of a script language; avian2d behind a default-on feature | how the undo stack holds a tile stroke; what type the chunk index is |
| Written by | the project's owner | drafted by `/spec`, decided by the owner |

The test: **could it have been decided with no code in front of you?**

If yes, it belongs in `docs/specs/`. If the question only arises once there is
code to look at, it is a record.

### Do not write it twice

* **A record does not restate a decision from `docs/specs/`.** It links to it.
* When a decision in `docs/specs/` comes down to a shape in the code, a record
  is born from it. That record links the section and does not repeat what to
  build.
* When it is not obvious which of the two it is, **read `docs/specs/` first**.
  If it is a rewording of something already decided, it belongs in neither.

## Index

| # | Decision | Status | Confirmed by |
| --- | --- | --- | --- |

**By status**: accepted: none · proposed: none · superseded: none

Records are numbered consecutively from `0001`. There are none yet.

## Where each kind of writing belongs

| Location | Holds | Does not hold |
| --- | --- | --- |
| [`docs/concepts.md`](../concepts.md) | what the project is for. Intent, not specification | how a decision was reached |
| [`docs/specs/`](../specs/README.md) | what to build, why it was decided that way, and what was rejected | type or signature detail |
| doc comments in the code | what a type is, how to use it, and the local reason it is shaped that way | cross-cutting rationale; link here instead |
| `docs/adr/**` | why the code is shaped this way, when, and what would reverse it | type or signature detail; link to the code |

## When to write one

A record is a cache of reasoning the code cannot show. The question that decides
whether to open one is **whether somebody will later undo this in good faith**:
reading the code, seeing something that looks better, and having no way to find
out what it costs.

* Two or more implementations are possible and one is chosen, especially when
  the choice shapes an interface that later phases will be written against.
* **A choice that affects whether the user's work survives.** What gets saved,
  what an undo restores, whether the document or the viewport is the truth.
  Getting one of these wrong lands on row 6 of `CLAUDE.md`'s failure list: it
  saves, and the user's work is silently wrong.
* **A choice that changes what a user's game has to carry.** What
  `crates/runtime` exposes, where a feature boundary sits, the shape of a saved
  document. Taking one of those back means migrating every project that already
  exists.
* The code was knowingly left in a shape that looks wrong: a duplication kept on
  purpose, a slower path chosen for a reason, a guard that appears redundant.
  Without a record, the next reader cleans these up.
* An existing decision is reversed: write a new record, mark the old one
  `superseded by ADR-NNNN`, and say what changed.
* A design knowingly diverges from a document in `docs/specs/`: record why, and
  update that document in the same change.
* You are about to write "undecided" into a doc comment: open one as `proposed`.

## When not to write one

A reason has cheaper places to live, and each of these is a complete answer on
its own:

1. Nothing. The decision is local and the code already shows it.
2. A comment where it happens.
3. A doc comment on the type or function it constrains.
4. A test whose name states the rule.
5. A record here.

**Take the lowest one that holds.** This project leans hard on 4: a test named
for the rule it protects, and the mutation that makes it fail, is the proof that
the rule is real (`CLAUDE.md`, *What a guard is*). Open a record only when the
reasoning does not fit into a name and an assertion, which usually means
**the rejected alternatives are the part worth keeping**.

Specifically, do not open one when:

* It affects one call site, or it is formatting.
* Reversing it would be cheap and local. A record earns its keep by making a
  reversal informed, so something that costs an afternoon to undo does not need
  one.
* A test name already says it, and somebody who broke the rule would read that
  name and understand what they broke.
* The choice was made for us, by Bevy's interface, by the platform, or by a file
  format's specification. Writing that down records a fact rather than a
  decision.
* It is a performance choice a benchmark can settle. The benchmark is the
  record, and it stays true when the numbers change.

Naming is usually below the line, but it rises above it for **a name written
into a saved document**. Changing one of those later means migrating every
project that already has one.

**Expected rate.** Phases 1 and 2 will be dense, because they are almost
entirely interface and that is where records are worth the most. It does not
continue. A few records per phase is the shape to expect; weeks with none are
normal, and several in a week is a sign the bar in *When not to write one*
slipped rather than a sign of progress.

## Conventions

* Filename `NNNN-short-slug.md`, numbers consecutive. **The slug is English**,
  because the filename is what sorts and what goes in a URL.
* The four digits exist so that sorting filenames as text puts them in the order
  they were written. They are not a limit. A number is never reused, never
  renumbered, and stays with a record that is superseded, because it is what
  every link, doc comment and commit message refers to.
* MADR statuses: `proposed`, `accepted`, `rejected`, `deprecated`,
  `superseded by ADR-NNNN`.
* **Every record fills in Confirmation**: which test or guard fails if the
  decision is violated, **and the change to the code that makes it fail**. This
  project verifies guards by breaking the code and watching the suite go red, so
  a record names that mutation. If nothing would fail, say so.
* A `proposed` status while the code already relies on the decision is itself a
  defect; say so in *Context and Problem Statement*.
* Keep the status in sync between a record's front matter and its row in this
  index.
* When a record says something is not yet enforced and it then is, **update
  Confirmation in the same change**. That section answers what guards the
  decision *now*, not what was true when it was written.
* When code a record names moves or is renamed, update the record's pointers.
  The reasoning is a record of what was thought at the time and is not
  rewritten, but the pointers are navigation rather than history, and a record
  whose pointers are dead is a record nobody can check.
* No em dashes.

`bash .claude/scripts/docs.sh` checks the parts of this that a machine can: the
index and the records agreeing, the statuses matching, and Confirmation being
filled in.

## More information

* [MADR 4.0](https://adr.github.io/madr/)
* [`adr-template.md`](./adr-template.md)
