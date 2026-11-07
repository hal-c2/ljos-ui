# Core

Milestone M2 in [DIALECT.md §13](../../DIALECT.md#13-milestones). Written only by the lane building this area.

Built:

- `Ljos.Core` with `QtObject`, `Timer`, `Component`, `Connections`,
  `Binding` and `Locale` (§4.1).
- `QtQml` lowered to it; `Ljos.Ui`'s `QtQml` import dropped.
- The value-type and colour constructors, `App`, `later` and `url`, with
  `Qt.*` lowered to them by member where the name is free (§4.3).

Remaining:

- The enums (`Qt.Key_*`, `Qt.Align*`).
