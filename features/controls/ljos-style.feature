# Sources:
#   docs/DIALECT.md (§4.16 the controls, §6 the compat pass)
#   docs/design/color.md
#   https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html
#   https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html
#   https://www.w3.org/WAI/WCAG22/Understanding/focus-appearance.html
# Ljós's own style is Ljós's: Qt has no Ljos.Controls to compare it with, so
# every scenario is `@ljos-only`. The colours expected are those
# crates/ljos_design/src/roles.rs derives from the same three colours.

@ljos-only
Feature: Ljós's own style: the controls of Ljos.Controls
  A file that imports Ljos.Controls gets Ljós's own style: compact
  controls drawn with the roles of one Theme. Every role is derived from
  three colours, the ground, the text and the accent, which follow
  Ljos.Design's palette in light and dark until an app gives its own;
  each role that is read is moved toward the text until it meets its
  WCAG ratio against the fills it is drawn on. An app may set any role
  itself. A file that wants Qt's Basic look imports Ljos.Controls.Basic,
  and that is what a file lowered from QtQuick.Controls.Basic imports, so
  it looks as it did.

  Rule: A file that imports Ljos.Controls gets Ljós's style

    Scenario: A button and a field are drawn with the theme's roles
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 320; height: 200
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Button { objectName: "save"; x: 10; y: 10; text: "Save" }
          Button { objectName: "send"; x: 10; y: 50; text: "Send"; primary: true }
          TextField { objectName: "field"; x: 10; y: 90; placeholderText: "Name" }
        }
        """
      Then the property "implicitHeight" of "save" is 28
      And the property "background.color" of "save" is "#ededed"
      And the property "background.radius" of "save" is 8
      And the property "background.color" of "send" is "#0066cc"
      And the property "implicitHeight" of "field" is 28
      And the property "background.color" of "field" is "#ededed"
      And the property "background.border.color" of "field" is "#868686"
      And the property "placeholderTextColor" of "field" is "#6a6a6a"

    Scenario: A file that wants Basic's look imports Ljos.Controls.Basic
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          Button { objectName: "save"; text: "Save" }
        }
        """
      Then the property "implicitHeight" of "save" is 40

  Rule: The roles derive from three colours and meet their ratios

    Scenario Outline: The <role> of white, black and blue is <color>
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          readonly property color role: Theme["<role>"]
        }
        """
      Then the property "role" of "root" is "<color>"

      Examples:
        | role              | color   |
        | canvas            | #ffffff |
        | surface           | #f5f5f5 |
        | surfaceRaised     | #ededed |
        | surfaceOverlay    | #e6e6e6 |
        | border            | #d6d6d6 |
        | input             | #868686 |
        | text              | #000000 |
        | textMuted         | #575757 |
        | placeholder       | #6a6a6a |
        | iconMuted         | #707070 |
        | accent            | #0066cc |
        | accentForeground  | #ffffff |
        | accentSurface     | #bdcfe1 |
        | focus             | #0066cc |
        | errorForeground   | #9b3134 |
        | warningForeground | #824704 |

    Scenario: A dark ground, a light text and a violet give their own roles
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          Component.onCompleted: {
            Theme.canvas = "#1e1e2e"
            Theme.text = "#e6e6f0"
            Theme.accent = "#7c3aed"
          }
          readonly property color raised: Theme.surfaceRaised
          readonly property color input: Theme.input
          readonly property color muted: Theme.textMuted
          readonly property color focus: Theme.focus
          readonly property color error: Theme.errorForeground
        }
        """
      Then the property "raised" of "root" is "#2c2c3c"
      And the property "input" of "root" is "#757582"
      And the property "muted" of "root" is "#a4a4af"
      And the property "focus" of "root" is "#9461ee"
      And the property "error" of "root" is "#e58b92"

    Scenario Outline: In <scheme>, the roles follow Ljos.Design's palette and meet their ratios
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Design
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          readonly property bool follows: "" + Theme.canvas === "" + Colors.canvas
            && "" + Theme.text === "" + Colors.text && "" + Theme.accent === "" + Colors.accent
          // A module's functions say nothing of what they give, so a ratio
          // is held as `unknown`.
          readonly property unknown input: Theme.contrast(Theme.input, Theme.surfaceRaised)
          readonly property unknown focus: Theme.contrast(Theme.focus, Theme.surfaceRaised)
          readonly property unknown muted: Theme.contrast(Theme.textMuted, Theme.surfaceOverlay)
          readonly property unknown mutedSelected: Theme.contrast(Theme.textMuted, Theme.accentSurface)
          readonly property unknown placeholder: Theme.contrast(Theme.placeholder, Theme.surfaceRaised)
          readonly property unknown icon: Theme.contrast(Theme.iconMuted, Theme.surfaceOverlay)
          readonly property unknown error: Theme.contrast(Theme.errorForeground, Theme.surfaceOverlay)
          readonly property unknown warning: Theme.contrast(Theme.warningForeground, Theme.surfaceOverlay)
          readonly property unknown onAccent: Theme.contrast(Theme.accentForeground, Theme.accent)
        }
        """
      Then the property "follows" of "root" is true
      And the property "input" of "root" is at least 3
      And the property "focus" of "root" is at least 3
      And the property "muted" of "root" is at least 4.5
      And the property "mutedSelected" of "root" is at least 4.5
      And the property "placeholder" of "root" is at least 4.5
      And the property "icon" of "root" is at least 3
      And the property "error" of "root" is at least 4.5
      And the property "warning" of "root" is at least 4.5
      And the property "onAccent" of "root" is at least 4.5

      Examples:
        | scheme |
        | light  |
        | dark   |

  Rule: An app may set any role itself

    Scenario: A role set by the app is the one the controls draw with
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
            Theme.input = "#b00020"
          }
          TextField { objectName: "field"; placeholderText: "Name" }
        }
        """
      Then the property "background.border.color" of "field" is "#b00020"
      And the property "placeholderTextColor" of "field" is "#6a6a6a"

    Scenario: A new accent derives the roles drawn from it again
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#ffcc00"
          }
          readonly property color onAccent: Theme.accentForeground
          Button { objectName: "send"; text: "Send"; primary: true }
        }
        """
      Then the property "onAccent" of "root" is "#000000"
      And the property "background.color" of "send" is "#ffcc00"

  Rule: Every control passes the design audit

    # The audit asks of each control a name, a target of 24 px, 4.5:1 for its
    # text and 3:1 for its parts, a ring where the keyboard is, and motion that
    # is short and eases out (ljos_audit). `QtQuick` is imported for
    # `Accessible`, which Ljos.Ui has not taken yet.
    Scenario Outline: The controls in <scheme> have nothing to report
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Rectangle {
          width: 480; height: 400; color: Theme.canvas
          Grid {
            x: 16; y: 16; columns: 2; spacing: 12
            Button { text: "Save" }
            Button { text: "Send"; primary: true }
            Button { text: "Delete"; destructive: true }
            Button { text: "More"; subtle: true; chevron: true }
            Switch { text: "Wrap lines" }
            CheckBox { text: "Show hidden files"; checked: true }
            ComboBox { model: ["Small", "Medium", "Large"]; Accessible.name: "Size" }
            ComboBox { model: ["One", "Two"]; outline: true; Accessible.name: "Count" }
            TextField { placeholderText: "Name"; Accessible.name: "Name" }
            SpinBox { value: 4; Accessible.name: "Columns" }
            Slider { value: 0.5; Accessible.name: "Volume" }
            Label { text: "Ready" }
          }
        }
        """
      Then the design audit finds nothing

      Examples:
        | scheme |
        | light  |
        | dark   |

    Scenario Outline: A menu that has faded in, in <scheme>, has nothing to report
      Given the system setting "color scheme" is "<scheme>"
      And the QML:
        """qml
        import QtQuick
        import Ljos.Controls
        Rectangle {
          width: 480; height: 400; color: Theme.canvas
          Menu {
            Component.onCompleted: open()
            MenuItem { text: "Cut"; shortcut: "Ctrl+X"; iconName: "minus" }
            MenuItem { text: "Paste"; shortcut: "Ctrl+V" }
            MenuSeparator {}
            MenuItem { text: "Wrap"; checkable: true; checked: true }
            MenuItem { text: "Delete"; destructive: true }
          }
        }
        """
      When 200 milliseconds pass
      Then the design audit finds nothing

      Examples:
        | scheme |
        | light  |
        | dark   |

  Rule: The ring of the focus shows where the keyboard is

    Scenario: A key that brings the focus to a button shows its ring
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Button { objectName: "save"; x: 10; y: 10; text: "Save" }
        }
        """
      Then the property "visible" of "focusRing" is false
      When the user presses "Tab"
      Then the property "visualFocus" of "save" is true
      And the property "visible" of "focusRing" is true
      And the property "border.width" of "focusRing" is 2
      And the property "border.color" of "focusRing" is "#0066cc"

    Scenario: A field with the keyboard has the focus colour as its edge
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          TextField { objectName: "field"; x: 10; y: 10; placeholderText: "Name" }
        }
        """
      When the user presses "Tab"
      Then the property "background.border.width" of "field" is 2
      And the property "background.border.color" of "field" is "#0066cc"

  Rule: Reduced motion moves nothing, and a change still shows

    Scenario: A switch's thumb slides to its end
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Switch { objectName: "wrap"; text: "Wrap lines" }
        }
        """
      When the property "checked" of "wrap" is set to true
      And 50 milliseconds pass
      Then the property "x" of "thumb" is at least 3
      And the property "x" of "thumb" is at most 13
      When 200 milliseconds pass
      Then the property "x" of "thumb" is 14

    Scenario: With reduced motion a switch's thumb is at its end at once, and its track still fades
      Given the system setting "reduced motion" is true
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Switch { objectName: "wrap"; text: "Wrap lines" }
        }
        """
      When the property "checked" of "wrap" is set to true
      And 50 milliseconds pass
      Then the property "x" of "thumb" is 14
      And the property "indicator.color" of "wrap" is not "#0066cc"
      When 200 milliseconds pass
      Then the property "indicator.color" of "wrap" is "#0066cc"

    Scenario: A pressed button gives a little
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Button { objectName: "save"; x: 10; y: 10; text: "Save" }
        }
        """
      When the user presses the pointer at 30, 24
      And 200 milliseconds pass
      Then the property "scale" of "save" is 0.97

    Scenario: With reduced motion a pressed button keeps its size, and its fill still changes
      Given the system setting "reduced motion" is true
      And the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 240; height: 120
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Button { objectName: "save"; x: 10; y: 10; text: "Save" }
        }
        """
      When the user presses the pointer at 30, 24
      And 200 milliseconds pass
      Then the property "scale" of "save" is 1
      And the property "background.color" of "save" is "#bdcfe1"

  Rule: A menu's rows line up

    Scenario: A row without an icon keeps the icons' column where another row has one
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 320; height: 240
          Menu {
            Component.onCompleted: open()
            MenuItem { objectName: "cut"; text: "Cut"; iconName: "minus"; shortcut: "Ctrl+X" }
            MenuItem { objectName: "paste"; text: "Paste"; shortcut: "Ctrl+V" }
          }
          Menu {
            Component.onCompleted: open()
            MenuItem { objectName: "undo"; text: "Undo" }
          }
        }
        """
      Then the property "iconSpace" of "paste" is true
      And the property "contentItem.gutter" of "paste" is 24
      And the property "contentItem.gutter" of "cut" is 24
      And the property "iconSpace" of "undo" is false
      And the property "contentItem.gutter" of "undo" is 0

    Scenario: A row's shortcut is muted at its end
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Core
        import Ljos.Controls
        Item {
          width: 320; height: 240
          Component.onCompleted: {
            Theme.canvas = "#ffffff"
            Theme.text = "#000000"
            Theme.accent = "#0066cc"
          }
          Menu {
            Component.onCompleted: open()
            MenuItem { objectName: "cut"; text: "Cut"; shortcut: "Ctrl+X" }
          }
        }
        """
      Then the property "text" of "keys" is "Ctrl+X"
      And the property "color" of "keys" is "#575757"
      And the property "visible" of "keys" is true

  Rule: The tool tip of a control is drawn in the control's style

    Scenario: A button's tool tip is the style's
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          id: root; objectName: "root"
          width: 240; height: 120
          property unknown tip: save.ToolTip.toolTip
          Button { id: save; x: 10; y: 60; text: "Save"; ToolTip.text: "Writes the file"; ToolTip.visible: true }
        }
        """
      Then the property "tip.background.radius" of "root" is 4
      And the property "tip.font.pixelSize" of "root" is 12

    Scenario: A Basic button's tool tip stays Basic's
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls.Basic
        Item {
          id: root; objectName: "root"
          width: 240; height: 120
          property unknown tip: save.ToolTip.toolTip
          Button { id: save; x: 10; y: 60; text: "Save"; ToolTip.text: "Writes the file"; ToolTip.visible: true }
        }
        """
      Then the property "tip.background.radius" of "root" is 0

  Rule: Ljos.Controls has every control Ljos.Controls.Basic has

    Scenario: What the style has no look of its own for yet is Basic's
      Given the QML:
        """qml
        import Ljos.Ui
        import Ljos.Controls
        Item {
          width: 320; height: 240
          TextArea { objectName: "notes"; text: "Notes" }
          ItemDelegate { objectName: "row"; text: "Row" }
          Dialog { objectName: "confirm"; title: "Delete?" }
          Popup { objectName: "popup" }
          ScrollView { objectName: "scroll" }
          FileDialog { objectName: "open"; title: "Open notes" }
          FolderDialog { objectName: "where"; title: "Where" }
        }
        """
      Then the property "implicitHeight" of "row" is 40
      And the property "text" of "notes" is "Notes"
      And the property "visible" of "confirm" is false
      And the property "title" of "open" is "Open notes"
      And the property "title" of "where" is "Where"

  Rule: A file lowered from QtQuick.Controls.Basic keeps Basic's look

    @headless
    Scenario: Basic's controls are lowered to Ljos.Controls.Basic
      Given the file "Main.qml":
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Button { text: "Save" }
        """
      When the author runs "ljos migrate --write ."
      Then the command prints:
        """
        lowered ./Main.qml (2 edits)
        1 files, 1 lowered, 0 kept
        """
      And the file "Main.qml" is:
        """qml
        pragma Scripts: JavaScript; import Ljos.Ui
        import Ljos.Controls.Basic
        Button { text: "Save" }
        """

    Scenario: A file lowered from Basic draws Basic's button
      Given Qt QML is lowered to the Ljós API
      And the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          Button { objectName: "save"; text: "Save" }
        }
        """
      Then the property "implicitHeight" of "save" is 40
