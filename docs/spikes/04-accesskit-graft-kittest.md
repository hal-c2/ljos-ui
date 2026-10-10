# Spike 04: AccessKit graft resolution in egui_kittest

Date: 2026-10-10. Milestone: M0 (prerequisite for M2, ARCHITECTURE.md 7.3 and
14, and for the `ljos_bdd` accessibility steps in 18.3). Crates under test:
egui 0.36.2, egui_kittest 0.36.2, kittest 0.4.0, accesskit_consumer 0.35.0,
accesskit 0.24.1 (what egui uses) and accesskit 0.25.1 with accesskit_consumer
0.39.1 (what the workspace pins).

All code is in `<scratch>/ljos-spike4/` (standalone cargo project, 15 tests, outside
the repo on purpose; build output went to `<scratch>/ljos-spike4-target`). It
runs with `cargo test` (`opt-level = 3` for the cost test). Only this file is
in the repo.

## Result

| Question | Answer |
|---|---|
| 1. Can a custom egui widget add N child nodes with roles, names, bounds, actions and stable ids, and does kittest find them and dispatch actions back? | **Yes.** One child `Ui` per item, built with `UiBuilder::id(..).accessibility_parent(..).sense(..)`, then `ctx.accesskit_node_builder(id, ..)` to set role, label, bounds, value and actions. `get_by_role_and_label` finds every node. `click_accesskit()` reaches our code as `Response::clicked()`; `SetValue` and `Increment` reach it through `input_mut(|i| i.consume_accesskit_action_requests(..))`; a pointer `click()` hits the item's rect. The hierarchy is ours, not the `Ui` nesting. |
| 2. Does AccessKit 0.25 have grafting, and do egui 0.36 and kittest resolve it? | **AccessKit has it, since 0.23** (`Node::set_tree_id` on a graft node, `TreeUpdate::tree_id`, `ActionRequest::target_tree`, `NodeId` unique per tree). The consumer (0.35 and 0.39) and kittest's query API resolve it. **egui does not produce it**: it emits only `TreeId::ROOT`, ignores action requests for other trees, and `egui_kittest::Harness` can only feed that one update into its private `kittest::State`. So grafting needs our own driver in tests and our own call to the adapter in the shell. Both are small (below). |
| 3. Stable ids, moving widgets, cost of 1,000 nodes? | Ids are a pure function of our key when we pass `UiBuilder::id`, so they survive frames, reparenting and rect changes (tested). egui **always sends the whole tree** (1,009 nodes in a `TreeUpdate` for 1,000 items), so "send only changes" is impossible through egui and possible only for a graft. Numbers below. |
| 4. Focus? | **Yes, in both modes**, with one landmine. Embedded: kittest `focus()`, Tab and `memory.request_focus(id)` all work and `is_focused()` reports it. Grafted: egui ignores `Action::Focus` for the sub-tree (we get it in the raw events); our sub-tree update sets `focus`, and egui must hold focus on the graft node for it to count. Focusing the graft node before its sub-tree exists **panics in `accesskit_consumer`** (0.35 and 0.39). |
| Surprise | **The workspace pin `accesskit = "0.25.1"` does not match egui.** egui 0.36.2, kittest 0.4.0, egui-winit's `accesskit_winit` 0.32 and the Android adapter 0.7.5 are all on accesskit **0.24.1**. The types are different crates; they do not interoperate. ARCHITECTURE.md 14 names Android adapter 0.9.0 and iOS 0.2.1, which are the 0.25 line. See "Version split". |

## Proof 1: child nodes (embedded mode, option a)

The widget (`<scratch>/ljos-spike4/src/lib.rs`), reduced to what matters:

```rust
let mut b = UiBuilder::new().id(item_id(key)).max_rect(rect).sense(sense);
if let Some(p) = parent { b = b.accessibility_parent(item_id(p)); }
let mut child = ui.new_child(b);
child.expand_to_include_rect(rect);          // so egui's own hit rect is right
let focusable = sense.is_focusable();        // read BEFORE the closure, see below
child.ctx().accesskit_node_builder(id, |n| {
    n.set_role(role); n.set_label(name);
    n.set_bounds(Rect { x0, y0, x1, y1 });   // logical points; the root node scales
    n.add_action(Action::Click);             // or SetValue, Increment, Focus ...
});
```

Findings:

- **Why a child `Ui` per node.** egui finds the parent of an AccessKit node by
  looking up the node's `Id` in a map filled only by `Ui::new`, `new_child`
  (`accessibility_parent`) and `Ui::interact`. A bare
  `accesskit_node_builder(id)` for an unregistered id is attached to the
  **root**. The child `Ui` with `UiBuilder::id` (an explicit id: `unique_id == id`)
  is the only way to express an arbitrary tree. The parent need not be the
  enclosing `Ui`, so every item can be created flat from the view's `Ui` and the
  QML parent given as `accessibility_parent`. Children appear in the parent in
  **creation order**, so the walk must be depth first.
- **`accesskit_node_builder` holds the `Context` lock.** Calling `ui.response()`
  or anything else on the context inside the closure deadlocks; egui's debug
  mutex panics after 10 s (`Failed to acquire RwLock write after 10s`). Compute
  everything first.
- **Click** is routed by egui itself: with `Sense::click()` the `Click` action
  becomes `Response::clicked()` on the next frame (kittest `run()` takes
  several frames).
- **Other actions** (`SetValue`, `Increment`, `Decrement`, custom) are in
  `input.events`; `consume_accesskit_action_requests(id, ..)` removes the ones
  we handle. It only matches `target_tree == ROOT`.
- **Bounds** are logical points; egui's root node carries `Affine::scale(ppp)`.
  kittest's `Node::rect()` divides it back, so `rect()` equals what we set.
- **Stable ids and moves.** `Row 2` moved from the list to the toolbar with a
  new rect: its `(NodeId, TreeId)` is unchanged, its position in the new
  parent's children follows creation order, and the old parent no longer lists
  it. The id is `item_id(key).accesskit_id()`, i.e. egui's 64-bit hash of our
  key. Ljós must hash `(view, ObjectKey or (list node, model key))` into an `Id`;
  `Id::from_high_entropy_bits` is `#[doc(hidden)]` and `unsafe`, avoid it.
- **Focus (embedded).** An item with `Sense::focusable_noninteractive()` gets
  `Action::Focus`; kittest `focus()` makes `is_focused()` true next frame; `Tab`
  moves to the next focusable node in creation order; our code can move focus
  with `ctx.memory_mut(|m| m.request_focus(id))`. egui writes
  `TreeUpdate::focus` from `memory.focused()`, falling back to the root.
- **Role::Label** stores its text in `value`, not `label` (kittest's `label`
  filter compensates). Our emitter must follow the same rule or
  `named "X"` steps will miss static text.

## Proof 2: grafting (shell mode, option b)

AccessKit 0.25.1 docs, `Node::tree_id`: "The tree that this node grafts. When
set, this node acts as a graft point, and its child is the root of the
specified subtree. A graft node must be created before its subtree is pushed.
Removing a graft node ... removes its subtree." `TreeUpdate::tree_id` selects the
tree; the first update of a sub-tree needs `tree: Some(..)`. Introduced in
0.23.0 ("Multiple tree support", 2026-01-15), so it exists in 0.24.1 as well.

Pieces that exist: `accesskit_consumer` (processes grafts since 0.35, and
`Node::locate()` returns `(NodeId, TreeId)`), every platform adapter on top of
it (the Windows and Android sources call `locate`/`locate_node` and put
`target_tree` into the `ActionRequest`; the Unix one was not read), and `egui_winit::State.accesskit`,
which is a **public** `Option<accesskit_winit::Adapter>` whose
`update_if_active(|| TreeUpdate)` takes any update. egui-winit itself only
ever sends egui's ROOT update from `handle_platform_output`.

Pieces that do not exist: egui emitting a graft node with a tree id (we can do
that ourselves, below), and a way to give egui_kittest a second update.

The host side, in a normal egui widget (`tests/p2_graft.rs`):

```rust
let child = ui.new_child(UiBuilder::new().id(graft_id).max_rect(view)
                         .sense(Sense::focusable_noninteractive()));
child.ctx().accesskit_node_builder(graft_id, |n| {
    n.set_role(Role::Group); n.set_label("QML view");
    n.set_bounds(view_in_window);
    n.set_transform(Affine::translate((view.x, view.y)));   // sub-tree bounds stay local
    n.set_tree_id(SUB);                                      // makes it a graft node
});
```

and the sub-tree is a separate `TreeUpdate { tree_id: SUB, tree: first.then(..), nodes, focus }`
produced by Ljós's own a11y pass with local `NodeId`s (we can use our 64-bit
keys directly, no hashing through egui).

Verified with a 40-line driver (`src/driver.rs`: an `egui::Context`, a
`kittest::State`, and `egui_kittest::Node::new(.., &queue, ppp)` for the
helpers; ROOT update first, sub-tree updates after it):

| Check | Result |
|---|---|
| `get_by_role_and_label` for nodes inside the graft | found; `locate()` is `(NodeId(1), SUB)` |
| Bounds through the graft | local `(0,0)-(60,30)` reports as `(100,50)-(160,80)` via the graft's `transform` |
| `click_accesskit()` on a grafted node | arrives with `target_tree == SUB` in `input.events`; egui does not consume it (it filters on `ROOT`), our code reads it. Same for `SetValue` payloads. |
| Delta update (one node, `tree: None`) | applied; unchanged nodes untouched |
| Dropping the graft node from the root tree | the whole sub-tree disappears |
| `objectName` as `author_id` | `Node::set_author_id`; found with `By::new().predicate(|n| n.author_id() == Some("x"))` |
| accesskit 0.25.1 + consumer 0.39.1 | same graft behaviour (`tests/p5_accesskit25.rs`); the unfiltered graft node is skipped by `common_filter`, the sub-tree root becomes the child |

`egui_kittest::Harness` keeps `kittest: kittest::State` private and calls
`state.update(output.platform_output.accesskit_update.take())` once per step,
so no grafted tree can reach it. Options: our `Driver` (above), or an upstream
PR adding something like `Harness::push_accesskit_update`.

### Proof 4 (grafted focus) and the landmine

- `Node::focus()` sends `Action::Focus` for the sub-tree node; egui's
  `Memory` ignores it (`target_tree != ROOT`); the request is in `input.events`.
- A sub-tree `TreeUpdate::focus` only counts when focus is on the graft node of
  the parent tree (AccessKit docs for `TreeUpdate::focus`). egui writes the root
  `focus` from `memory.focused()`, so the host `Ui` must be focusable and the
  shell calls `request_focus(graft_id)` when Ljós's `FocusTree` holds focus.
  Then `is_focused()` is true on exactly the sub-tree's focus node, and moving
  focus inside QML is a sub-tree-only update (`nodes: []`).
- If egui focus is not on the graft node, the sub-tree's `focus` field is ignored
  (test g5).
- **Panic.** If the root update carries `focus = graft node` before the
  sub-tree exists, the consumer panics: "Focus is on graft node ... but subtree
  ... does not exist. Graft nodes cannot be focused without their subtree." This
  is `accesskit_consumer` 0.35.0 (`tree.rs:95`) and 0.39.1 (`tree.rs:112`), i.e.
  inside the platform adapters, not just in tests. Rule for the shell: never
  request egui focus on the graft host in the frame it first appears, and send
  the sub-tree update in the same frame as, and straight after, the root update.

## Proof 3: cost of 1,000 unchanged nodes

Release-like (`opt-level = 3`), one core, mean over 300 frames, 1,000 rows plus
9 structural nodes, `tests/p3_cost.rs`:

| Step | µs per frame |
|---|---|
| egui frame with 1,000 child `Ui`s, AccessKit **off** | 509 |
| same, AccessKit **on** | 658 (so the node building adds about 150) |
| `kittest::State::update` with the full 1,009-node update (includes cloning it) | 106 |
| consumer update with an empty delta (focus only) | 0.1 |
| consumer update with one changed node of 1,000 | 0.3 |
| building 1,000 nodes by hand (a Ljós a11y pass doing a full rebuild) | 91 |

Reading the numbers:

- **Embedded** costs the child `Ui` per node (about 0.5 µs per node, paid
  whether or not AccessKit is on, so the pass must be skipped when it is off),
  plus about 0.15 µs per node of builder work, plus whatever the adapter spends
  diffing the whole tree every frame (about 0.1 ms in the consumer; the platform
  calls it makes are for changed nodes only). About 0.25 ms per frame at 1,000
  nodes, only while an assistive technology is active. Acceptable, but not free
  in the streaming-chat case at 60 Hz.
- **Grafted** can send exactly the changed nodes: 0.3 µs for one change. This is
  also what ARCHITECTURE.md 14 wants (`needs_a11y` bubbling, only dirty subtrees
  walked); with egui in the middle that saving would be thrown away, because egui
  rebuilds and resends the whole tree.
- Probing for activity is cheap: `ctx.accesskit_node_builder(view_id, |_| ())`
  returns `None` when AccessKit is off, so the pass can be gated without a private
  flag.

## Version split (act on this first)

```
accesskit v0.24.1   <- egui 0.36.2, kittest 0.4.0, accesskit_consumer 0.35.0,
                       accesskit_winit 0.32.2 (egui-winit's), accesskit_windows 0.32.1,
                       accesskit_android 0.7.5, accesskit_macos 0.26.3
accesskit v0.25.1   <- workspace pin; accesskit_winit 0.34.1, accesskit_android 0.9.0
```

Cargo resolves both, and `egui::accesskit::Node` is not `accesskit::Node`
(0.25 also changed `PropertyValue` and `Node` cloning, and the consumer API: 0.39
has `NodeRef` and `TreeChangeHandler` over `NodeRef`, kittest 0.4.0 uses 0.35's
`Node`). Today nothing in the repo depends on `accesskit`, which is why
`Cargo.lock` has only 0.24.1.

egui 0.36.2 (2026-09-08) came out after accesskit 0.25.0 (2026-08-29) and still
depends on 0.24.1. So until egui and egui_kittest move, a crate that builds
`TreeUpdate`s for egui must use **0.24.1**. Converting between versions is a field
copy (or a serde round trip); it is not worth doing every frame.

## Recommendation

**`ljos_a11y` (new crate, no egui dependency):**

1. Pin `accesskit = "0.24.1"` in the workspace until egui bumps (a root
   `Cargo.toml` change, so the lead should make it), and note in section 14/24 that
   Android 0.9.0 and iOS 0.2.1 are only usable after egui moves to 0.25; until then
   Android uses 0.7.5. Revisit on every egui release (D9).
2. Own the a11y pass and the role table as data. Output is `accesskit::TreeUpdate`
   for one **sub-tree** (own `TreeId`, own local `NodeId`s = Ljós keys), with
   `nodes` limited to dirty subtrees and parents re-sent with full child lists as
   section 14 already says. The id mapper and the final write are the only parts
   that differ between modes, so isolate them behind a small `Emitter` trait.
3. Two emitters, same pass:
   - **Embedded** (`ljos_egui`, M0–M1 and any app that embeds `QmlView`): walk the
     sub-tree update's nodes depth first and, for each, make a child `Ui`
     (`UiBuilder::id(Id::new((view, key))).accessibility_parent(..)`) and write the
     node with `*builder = node` after remapping every `NodeId` property
     (`children`, `labelled_by`, `controls`, `active_descendant`, ...) with the same
     hash. Gate the whole thing on `accesskit_node_builder(view_id, ..).is_some()`.
     Route `ROOT` actions via `consume_accesskit_action_requests` per node. All
     of this is proven above, minus the remapping of `NodeId` properties.
   - **Shell** (`ljos_shell`, from M2): add the graft host node as above and call
     `egui_winit::State.accesskit.update_if_active(|| subtree_update)` right after
     `handle_platform_output` (the field is public; keep using egui-winit's adapter,
     do not create a second one). Read grafted `ActionRequest`s from
     `ctx.input(|i| i.events)` before egui drains them, and remove them. Follow the
     focus rules above.
4. Do not let `ljos_a11y` pick between the emitters at run time by guessing:
   `QmlView` always uses embedded; only `ljos_shell` uses the graft.

**`ljos_bdd` accessibility steps:**

1. Keep steps on kittest queries and out of egui types, behind
   `Backend::a11y_tree` / `click(role, name)`:
   - `the accessibility tree has a "button" named "Save"` is
     `query_all_by_role_and_label(role, name).count() >= 1`;
   - `... has no "button" named "X"` is `count() == 0`;
   - a name of `""` needs `By::new().role(r).predicate(|n| n.label().is_none_or(|l| l.is_empty()))`
     because kittest's `label` filter never matches a missing label;
   - `objectName` lookups use `predicate(|n| n.author_id() == Some(..))`;
   - never use the singular `get_by_*` for "has a": it panics on two matches.
2. Map the role word to `accesskit::Role` through the `ljos_a11y` role table
   (tested data), not through `Debug` strings.
3. `click` and `key` steps use the pointer and key paths (`Node::click()`,
   `key_press`), as the inspector does; the dedicated "assistive technology
   activates" steps use `click_accesskit()` / `focus()`. Both are proven.
4. **Harness versus Driver.** For M0–M1 build the backend on `egui_kittest::Harness`
   in embedded mode. From M2, do *not* try to make `Harness` carry the graft: the
   scenarios are role/name based, so they pass identically through the embedded
   emitter, and the graft path gets its own tests in `ljos_shell` using the
   40-line `Driver` (or an upstream `Harness::push_accesskit_update`), including
   a property test that the embedded and grafted outputs flatten to the same set
   of `(role, name, rect, actions)`.

## Not verified

- Platform behaviour of grafts in a real adapter (AT-SPI, UIA, Android). No window
  or screen reader here; everything above is at the `accesskit_consumer` level,
  which all adapters wrap. The adapters' `locate` handling was checked in source
  (Windows, Android) but not run.
- eframe: it does not expose its `egui_winit::State`, so option (b) on eframe is
  not possible and is not needed before M2.
- Whether `ctx.enable_accesskit()` is called on activation by egui-winit/eframe
  (egui-winit 0.36.2 has no such call; eframe was not inspected). The shell must
  call it when the adapter requests the initial tree and `disable_accesskit` on
  deactivation, or `accesskit_update` is always `None`.
- The memory cost of 1,000 nodes (not measured, only time).
- Android and iOS adapters (blocked by the version split; also no device).

## Reproduce

```
cd /tmp/ljos-spike4
CARGO_TARGET_DIR=<scratch>/ljos-spike4-target cargo test -- --test-threads=1 --nocapture
```

(15 tests: 6 in `p1_nodes`, 7 in `p2_graft` of which one is `should_panic`
documenting the focus landmine, 1 in `p3_cost` which prints the table above, 1 in
`p5_accesskit25`.)
