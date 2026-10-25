# ADR 0001: The scene's bookkeeping lives outside the reactive graph

- **Status:** accepted, 2026-10-25. Built in steps; until each step lands,
  the part it replaces still runs as an effect.
- **Terms:** [CONTEXT.md](../../CONTEXT.md) (Item class, Item record,
  Native field change).

## Context

`QuickScene` keeps, for every item it shows, what it needs to place, draw
and describe it: where the item is, whether it is visible and enabled once
its parents are counted, its anchors, the text it lays out, the node it
syncs to the render tree and its accessibility node. Today each of these is
kept up to date by an effect in the reactive graph (section 6.2 of
ARCHITECTURE.md): an effect per item that reads the properties it depends
on and writes the scene's maps when one of them changes. Among them are the
structure, inherit, anchors, place, geometry and text-polish effects.

That was simple to build and it is correct: the graph tracks what each one
read and runs it again only when that changed. It is also what makes a new
delegate expensive. Measured with callgrind on `bench/listview.qml` (B22,
docs/BENCHMARKS.md):

- a new delegate costs about 386 k instructions, against about 200 k on
  qml6;
- about 75 k of it is adoption, the scene installing and first running its
  effects for the delegate's two items (anchors alone 20.7 k, the first
  reads of the item's fields 12.8 k, geometry 18 k);
- about 62 k is teardown, the same effects and their edges being disposed
  when the delegate leaves.

`crates/ljos_quick/tests/budget.rs` counts 16 effects per ListView delegate
(Rectangle and Text), of which only the delegate's own QML bindings are
the user's. Each effect costs a node, its closure, its source edges, its
first run and its disposal; B19–B22 made each of those cheaper and the
total barely moved. The cost is in how many there are, not in what each
one does.

The values these effects follow are almost all native fields of the item
(`x`, `width`, `visible`, `anchors.*`, `text`, …), written either by Rust
code or by a binding's result. Both writes already pass through the engine,
which knows the object and the field.

## Decision

The scene tracks its own item state with dirty bits. A write to a native
field the scene cares about (a *native field change*) marks the item's
record dirty for that concern, whether the write came from Rust or from a
binding's result; the scene's passes read the dirty items and clear the
bits. The parent-to-child state (inherited `visible` and `enabled`) is
pushed down the item tree by the scene when it changes, not pulled by an
effect per child.

Only user QML bindings, handlers and the effects that stand for them
(Behavior interceptors, `Connections`, Rust subscribers) live in the
reactive graph. A binding still reads native fields through the graph, so
its dependencies are tracked as before; what changes is that the scene no
longer needs a node of its own to hear about the same field.

## Consequences

- Adoption becomes writing an item record and setting its bits; teardown
  becomes dropping the record. The budget tests pin the effect count per
  delegate, and each step of the seam lowers it.
- The order in which the scene sees changes is its own (by pass, then by
  item), not the graph's height order. That is what Qt does too: a polish
  or a sync runs over the dirty items once per frame. A scene pass that
  relied on running in the middle of a flush has to be moved to the pass
  that reads the bit.
- A write to a native field that bypassed the engine would be missed, so
  the hook has to sit where every native write goes through, a binding's
  result included. Each batch of the seam checks that for the fields it
  moves before it removes the effect that covered them.
- Laziness is kept: a binding no one reads still does not run. A native
  field change marks a bit; it does not evaluate anything.
- 0 % CPU when idle still holds: a bit set asks for a frame through the
  same scheduler a dirty node does, and nothing polls.
- The steps are batches B27–B30 of the delegate track (item record first,
  then geometry, place and anchors; inherited state; structure and text
  polish). Each lowers a pinned number in `budget.rs` and changes no
  scenario.
