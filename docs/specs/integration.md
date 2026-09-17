# Level and scenario integration

How they reach each other, through events and triggers.

---

## 1. Level and scenario integration

The level editor and the scenario editor are independent, but at runtime the two
reach each other.

For example:

```text
player
  │
  ▼
enters a trigger
  │
  ▼
starts a scenario
  │
  ▼
dialogue
  │
  ▼
sets a flag
  │
  ▼
a door opens
```

The other direction works too:

```text
scenario
  │
  ▼
starts a level
  │
  ▼
the forest level
  │
  ▼
the player reaches the boss
  │
  ▼
fires a scenario
  │
  ▼
the conversation before the fight
```

This is what lets one foundation carry:

* Hollow Knight style exploration
* adventure games
* visual novels
* story-driven action

---

## 2. The event and trigger system

Events and triggers are the shared concept that connects levels and scenarios.

For example:

```text
the player enters an area
        ↓
trigger
        ↓
StartScenario("intro")
```

```text
a scenario finishes
        ↓
SetFlag("met_npc")
        ↓
the level reacts
        ↓
a door unlocks
```

What this builds towards is one event model:

```text
Level
  ↕
Event / Trigger
  ↕
Scenario
```
