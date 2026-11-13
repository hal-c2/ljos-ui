# Ui

Milestone M1 (Ui and Layout) in [DIALECT.md §13](../../DIALECT.md#13-milestones). Written only by the lane building this area.

Built:

- `ljos run --compat keep|lower` without `--watch` (a watched run refuses `lower`: its saves are recompiled as written)
- `Repeater`, `ListView` and `ListView.*` in `Ljos.Ui` (§4.8)
- a `Repeater`'s and a `ListView`'s `key`, a `Repeater`'s `enter` and `exit`, and `ListView`'s `add` and `remove` called `enter` and `exit` (§4.15)
- `Image`, `Flickable`, `TextInput` and its validators, `TextEdit`, `TextMetrics`, `FontMetrics`, `Shortcut`, `DropArea`, `Keys` and `Accessible` in `Ljos.Ui`, as they are (§4.14)
- `GridView` in `Ljos.Ui`, with `add` and `remove` called `enter` and `exit` (§4.15); `PathView`, `Canvas`, `KeyNavigation`, and the transforms `Rotation`, `Scale` and `Translate`, as they are (§4.14)
- a child of a `Row` or `Column` anchored across it keeps the file, where Ljós's places it across by `Layout.alignment`

Remaining:

- `--watch` with `lower`, then the rest
