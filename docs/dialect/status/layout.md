# Layout

Milestone M1 (Ui and Layout) in [DIALECT.md §13](../../DIALECT.md#13-milestones). Written only by the lane building this area.

Built:

- the `Ljos.Layout` module (`Box`, `Row`, `Column`, `Grid`, `Flow`, `Pages`, `Layout`)
- positioner `Grid` and `Flow` lowered
- `GridLayout` → `Grid`, with Qt's spacing and a child's vertical centring written out
- `StackLayout` → `Pages`

Remaining:

- a `GridLayout` Qt reads otherwise is kept: a count across its flow, a child in a cell of its own, `layoutDirection`, uniform cells, room no child fills
- a `StackLayout` whose attached object is read is kept
