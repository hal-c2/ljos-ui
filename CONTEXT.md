# Context: terms

Words the design documents and the code use with a meaning of their own.
Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md). Decisions:
[docs/adr/](docs/adr/).

**Component plan.** What every instance of a QML component shares, linked
by the runtime the first time the component is instantiated and kept beside
its `LoadedFile`: the native property index each binding writes and each
expression reads, the metas of the objects it makes, the attached types it
uses, each id's object slot. Instancing a component with a plan is
allocation, indexed writes and indexed binding installs, with no lookup by
name. Built from IR facts that do not depend on native types, so a change
to a native type never leaves the IR stale. See
[ADR 0002](docs/adr/0002-component-plans.md).

**Item class.** What the scene needs to know about an item's type, worked
out once per type and cached on its meta: what kind of item it is, whether
it paints, whether it polishes (a Text, a positioner, a Layout), whether it
is a Loader or a MouseArea. Replaces asking the type hierarchy (`is_a`)
again for each item.

**Item record.** The scene's one dense record per item it shows, indexed by
the item: place, geometry, anchors, the text it lays out, its render node,
its accessibility node, and dirty bits for each. Replaces a map per concern
keyed by object. See [ADR 0001](docs/adr/0001-scene-bookkeeping-outside-the-graph.md).

**Native field change.** A write to a field of a native type that the
scene follows (`x`, `width`, `visible`, `anchors.*`, `text`, …), from Rust
code or from a binding's result. It sets a dirty bit on the item's record
instead of running an effect; the scene's next pass reads it. See
[ADR 0001](docs/adr/0001-scene-bookkeeping-outside-the-graph.md).

**Row plan.** For one delegate component and one set of model roles, how a
view hands a row to a delegate: which required properties it writes
directly, and whether the delegate reads `model.*` or unqualified roles at
all, so that a row object is made only when it does. Rebuilt when the view
gets a new model or the model's roles change.

**Scene driver.** What drives a scene as a host does, for everything that
is not one: an application's tests, the headless pictures, the scenario
runner, the QtTest runner and the MCP inspector. It settles the scene, lets
time pass on a test clock, finds items by object path, sends input as a
person would (aimed at an item's middle, refused where a person could not
reach it) and reads the accessibility tree and the picture, by one set of
rules, so that they all see a scene behave the same. Its clock is the test
clock or the real one. Lives in `crates/ljos_drive`, whose module comment
gives the rules.
