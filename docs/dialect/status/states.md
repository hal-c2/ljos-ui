# States

Part of `Ljos.Ui`, milestone M1 in [DIALECT.md §13](../../DIALECT.md#13-milestones). Written only by the lane building this area.

Built:

- `PropertyChanges { target: }` lowered to the id form (§4.9)
- `Behavior`, `Transition` and Qt's animations in `Ljos.Ui` as they are: `Animation`, `PropertyAnimation`, `NumberAnimation`, `ColorAnimation`, `RotationAnimation`, `SmoothedAnimation`, `SpringAnimation`, `SequentialAnimation`, `ParallelAnimation`, `PauseAnimation`, `ScriptAction`, `PropertyAction`, and `Easing` for `easing.type` (§4.9)
- `Behavior on color` and a value source's `on` lowered by the member's Ljós name on the object it is in (`on fill` on a `Rectangle`)
- An animation's `property` and `properties` strings renamed by what it moves: its `target` and `targets` ids, the object of its `Behavior` or value source, or in an object's `transitions` the objects its states change. The file is kept where that is not known (`Animated::Unknown`), where the strings are made as it runs (`Animated::Made`), or where one name is a different member on different objects (`Animated::Split`)

Remaining:

- `AnchorAnimation` and `ParentAnimation`, with `AnchorChanges` and `ParentChange`; the Animators, `AnimationController` and `FrameAnimation` stay `QtQuick`'s
- `Transition.from`/`to` checked against the state names, as `state` is
- A view's `add`/`remove`/`displaced` transitions name members of its delegates, which the pass does not type there: a renamed name in them keeps the file
