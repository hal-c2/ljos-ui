# AT-SPI `EditableText` through AccessKit

Date: 2026-10-18. Versions read: accesskit 0.24.1, accesskit_consumer 0.35.0,
accesskit_atspi_common 0.18.1, accesskit_unix 0.21.1 and atspi-common 0.13.0.
These are the versions that egui 0.36 uses (ARCHITECTURE.md section 24).

This note records what upstream would need. Ljós does not patch third-party
code.

## Where things stand

- **What Ljós takes.** An entry (`TextInput`, `TextField`, `TextEdit`,
  `TextArea`) already takes two requests through the egui host
  (`ljos_egui/src/a11y.rs`, `ljos_quick/src/text_input/a11y.rs`):
  - AccessKit's `ReplaceSelectedText` with `ActionData::Value`. This is typing
    over the selection: a validator judges it, it can be undone, and it says
    `textEdited`.
  - `SetValue` with `ActionData::Value`. This replaces the whole text.

  It also publishes its text as `TextRun` children (`ljos_a11y/src/text.rs`),
  so the node supports text ranges.
- **What AT-SPI shows.** An AT-SPI client sees the entry as:
  - `Accessible`, `Action`, `Component` and `Text`;
  - with the `editable` state, unless it is read-only.

  Through `Text`, a client can read the text, move the caret
  (`SetCaretOffset`) and select (`SetSelection`/`AddSelection`). Both become
  `SetTextSelection` requests.
- **What is missing.** `org.a11y.atspi.EditableText` is not registered, so an
  AT-SPI client cannot type into an entry, delete from it, or set its text:
  - `accesskit_atspi_common` has no `Interface::EditableText` in
    `PlatformNode::interfaces()`.
  - `accesskit_unix` has no zbus object for it in `atspi/interfaces/`, and
    nothing registers it in `atspi/bus.rs`.
  - The `Value` interface does not stand in for it either: it is numeric, and
    it is offered only when the node has a numeric value.

  A client has to send key events instead. Orca and the cua-driver both fall
  back to that, and it goes through the focus and the input method rather than
  straight to the node.

## What upstream would need

1. **`accesskit_atspi_common`, `node.rs`**
   - **Offer the interface.** Add `Interface::EditableText` to `interfaces()`
     when all of these hold:
     - the node is a text input (`is_text_input()`);
     - it is not read-only (`is_read_only()` is false);
     - it supports text ranges.

     The atspi-common `Interface` enum already has `EditableText`.
   - **Add `PlatformNode` methods,** resolved like `set_selection` (through
     `resolve_for_text_with_context`, `text_range_from_offsets` and
     `locate_node`):
     - **`set_text_contents(text)`** sends one `SetValue` request with
       `ActionData::Value(text)`.
     - **`insert_text(position, text, length)`** sends two requests:
       - first `SetTextSelection`, with a degenerate range at `position`;
       - then `ReplaceSelectedText`, with the first `length` characters of
         `text`.
     - **`delete_text(start, end)`** also sends two requests:
       - first `SetTextSelection`, with `start..end`;
       - then `ReplaceSelectedText`, with an empty value.
     - **`copy_text`, `cut_text` and `paste_text`** have no AccessKit action.
       - The simplest course is to answer `false`.
       - Alternatively, AccessKit could gain clipboard actions. Every adapter
         would then have to say where they go, because the adapter holds no
         clipboard.
   - **Support each action.** Each method should answer `false` unless the
     node supports the action it sends: `ReplaceSelectedText`, or `SetValue`
     for `set_text_contents`.

     Ljós sets `ReplaceSelectedText` on every editable entry. The other
     clients of AccessKit would need to set it too for the interface to work
     for them.
2. **`accesskit_unix`**
   - Add `atspi/interfaces/editable_text.rs`. It has the six methods of
     `org.a11y.atspi.EditableText`, each forwarding to the methods above and
     mapping errors as `text.rs` does.
   - Register the object in `register_interfaces` and unregister it in
     `unregister_interfaces`, beside `Text`.
   - The adapter already works out a node's interfaces again when the node
     changes, registering and unregistering the difference
     (`accesskit_atspi_common/src/adapter.rs`). So an entry that turns
     read-only would lose the interface with no more work.
3. **One request, not two**
   - AT-SPI's `InsertText` and `DeleteText` are single calls, while the
     mapping above is two AccessKit requests. A host that runs them in order
     on one thread, as Ljós does, ends in the right state.
   - An atomic form would need a new `ActionData`: a range together with its
     replacement text. That is a protocol change in `accesskit`, and is worth
     proposing only if a client is seen to depend on it.

## What Ljós would do once it lands

- Move to the AccessKit release that has these changes, which waits on egui as
  the 0.25 move does.
- Add a scenario to `features/a11y/` in which an assistive technology types
  into an entry, deletes from it and sets its text over AT-SPI. Each change is
  checked as `textEdited`, and a read-only entry turns each one down.
- No Ljós code changes otherwise: the requests are already taken.
