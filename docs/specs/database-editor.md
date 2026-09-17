# Database editor

Editing table-shaped game data: items, monsters, skills and the rest.

---

## 1. User-defined types, scaffolded from templates

### Decision

**The user defines the schema in Rust.** The editor reads it through reflection
and edits it as a table.

**Standard templates are scaffolded as Rust code.** A beginner picks the RPG
template and is moving. The generated code is theirs, and they can change it.

**One row is one file**, saved as `.bsn`.

```rust
// in the user's game crate
#[derive(Reflect, Default)]
struct Item {
    name:  String,
    price: u32,
    icon:  AssetPath,
}
```

```text
assets/db/items/potion.bsn
assets/db/items/ether.bsn
```

### What Jackdaw already implements

`src/definition_assets.rs`, at 1,968 lines, **already implements four fifths of
this.**

> A file says which type it holds, so the editor finds a kind's files by
> reading them rather than by where they sit. ... **A type the editor has
> compiled in** loads through `load_bsn_assets`; **a type the open project
> reported in its schema** is known no other way, so its files load as the
> patch they hold and save back through the same emitter. Either way a file
> holds only what the value changes from its default.
>
> Opening an asset puts it in the inspector: the field rows read the value at
> the path it names instead of a component, and their edits come back here as
> `SetDefinitionField` undo entries.

So Jackdaw already has:

* recognition of a type the user defined in Rust, through schema extraction
  ([runtime-and-play.md §1](./runtime-and-play.md))
* instances of that type saved as `.bsn` files
* reflection-driven editing in the inspector
* **undo integration**
* **saving only what differs from the default**, as a patch

**What is missing is the table view.** Jackdaw edits one at a time in the
inspector; a database needs many rows listed and edited together.

### Rejected options

**A. A fixed schema, the way RPG Maker has one**

Item, Weapon, Skill and Enemy defined here. Usable immediately, but **it fixes
the genre.**

Hades' boons, Dead Cells' weapons and the keywords of an investigation adventure
do not fit RPG Maker's schema. Having claimed 2D games generally in
[concepts.md §2](../concepts.md), this option contradicts that.

**B. User-defined types with no templates**

Handles any game and agrees completely with
[data-model.md §3](./data-model.md), principle 5 of
[concepts.md §9](../concepts.md) and
[runtime-and-play.md §1](./runtime-and-play.md), but **nothing starts until you
can write Rust.** That fails principle 1.

**The option taken removes both weaknesses.** And because what is generated is
Rust code, **the editor never has to carry a dynamic schema**, which would be a
complicated thing to hold. The game gets typed access too, which agrees with
principle 5.

The shape is Unity's ScriptableObject: define a C# class and an inspector
appears. Here, define a Rust struct and a table editor appears.

### One row per file

**Rejected: one file per table**

Fewer files, but two people editing the same table collide.

**Why one row per file:**

* **Jackdaw's 1,968 lines transfer almost unchanged**
* **merge conflicts effectively stop happening.** Game data is edited in
  parallel by writers and designers, and that matters more in practice than the
  file count
* references work naturally as paths, the same mechanism as asset references in
  [data-model.md §4](./data-model.md)

An RPG can reach thousands of files, but the editor indexes them and nobody
browses the folder by hand.

### The build wait

Adding a new data type means writing Rust, which means a build. That looks like
the nine-minute problem from
[runtime-and-play.md §1](./runtime-and-play.md) returning, but **it is a
different thing.**

```text
adding a field    → a rebuild (1 to 4 seconds, measured on Jackdaw)
adding a row      → no build at all; it is only data
```

**The daily work, editing rows, never waits for a build.** This is Unity's
experience exactly: changing a ScriptableObject class recompiles, changing a
value does not.

### What it buys, per target game

By the table in [concepts.md §2](../concepts.md), a database is needed by six of
the nine target games. **It is the second most important thing to edit, after
levels.**

| Game | What the database holds |
| --- | --- |
| Final Fantasy 5, Dragon Quest 7 | items, weapons, armour, spells, monsters, enemy groups, jobs, shops, encounter tables |
| Hades | boons, weapons, gods, dialogue conditions |
| Dead Cells | weapons, mutations, enemies, room kinds |
| An investigation adventure | keywords, evidence, cases |
| A metroidvania | items, abilities, enemies |

**That all of these are one mechanism** is the point of this design.

### Still open

* the table view's UI design; it belongs in `editor_ui`
  ([crates.md §3](./crates.md))
* what the templates contain, and how scaffolding works
* how a level or a scenario references a database row
  ([open-questions.md §1](./open-questions.md))
