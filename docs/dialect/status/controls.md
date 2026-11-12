# Controls

Milestone M6 in [DIALECT.md §13](../../DIALECT.md#13-milestones). Written only by the lane building this area.

Built:

- `Ljos.Controls.Basic`, with Basic's `Label`, `AbstractButton`, `Button`, `ToolTip`, `ScrollBar`, `ScrollView`, `Popup` (K1), `TextField`, `TextArea` (K2), `Menu`, `MenuItem`, `MenuSeparator`, `Dialog`, `ItemDelegate` (K3), `CheckBox`, `Switch`, `ComboBox`, `SpinBox`, `Slider` (K4), `QtQuick.Dialogs`' `FileDialog`, `FolderDialog` (K5), and `ApplicationWindow`, `Control`, `Pane`, `Frame`, `GroupBox`, `Page`, `ToolBar`, `ToolButton`, `RoundButton`, `DelayButton`, `ProgressBar`, `BusyIndicator`, `TabBar`, `TabButton`, `SwipeView`, `RadioButton`, `ButtonGroup` (K6)
- `QtQuick.Controls.Basic` lowered to it, and `QtQuick.Controls` where the program starts with Basic (§4.16)
- `Ljos.Controls` in Ljós's own style (#204): `Button`, `Switch`, `CheckBox`, `ComboBox`, `TextField`, `SpinBox`, `Slider`, `Menu`, `MenuItem`, `MenuSeparator`, `Label`, `ToolTip`, `Icon`, on a `Theme` whose roles derive from three colours (`ljos_design::roles`); Basic's look moved to `Ljos.Controls.Basic`

Remaining:

- the other controls (`StackView`, `MenuBar`, `Drawer`, `Dial`, `CheckDelegate`, `RangeSlider`, `Tumbler` …), the other styles
- a look of Ljós's own style for `TextArea`, `Dialog`, `ItemDelegate`, `Popup`, `ScrollBar`, `ScrollView` and the K6 controls, which it takes from Basic today
