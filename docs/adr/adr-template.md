---
status: "proposed | accepted | rejected | deprecated | superseded by ADR-NNNN"
date: YYYY-MM-DD
decision-makers: who decided
---

# Short title, in the form "do X because Y" or "X is Z"

## Context and Problem Statement

What is the problem, and why does it need deciding now? Two or three sentences.
If something already depends on the answer while this record is still
`proposed`, say so here.

If a section of `docs/specs/` relates to this, link it. **Do not repeat it.**

## Decision Drivers

* the constraints that actually narrow the choice
* which row of `CLAUDE.md`'s failure list this can land on, where one applies

## Considered Options

* option 1
* option 2

## Decision Outcome

Chosen option: "option 1", because ...

### Confirmation

Which test or guard fails if this decision is violated, **and what change to the
code makes it fail**?

Name the mutation, not just the test: a test that passes whatever the code does
confirms nothing. If nothing would fail, say so plainly. A decision that looks
enforced and is not is worse than one that is honestly unenforced.

A guard the compiler holds is the strongest available: a crate boundary, an
exhaustive `match`, a private field.

### Consequences

* Good, because ...
* Bad, because ...
* What would reverse this: ...

## Pros and Cons of the Options

### option 1

* Good, because ...
* Bad, because ...

### option 2

* Good, because ...
* Bad, because ...

## More Information

Links to the code, the section of `docs/specs/` this serves, and the review or
measurement it rests on.
