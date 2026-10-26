# Sources:
#   docs/design/color.md, docs/design/interaction.md, docs/design/motion.md
#   https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html
#   https://www.w3.org/WAI/WCAG22/Understanding/use-of-color.html
#   https://www.w3.org/WAI/WCAG22/Understanding/focus-visible.html
#   https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/
#   https://m3.material.io/styles/color/roles
# The design audit is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @a11y
Feature: The design audit acts on the window to check colour, focus and motion
  Some rules cannot be read off one frame. The design audit draws a part
  of the window with and without a control's part, flips a control's
  state and draws it again, and looks through and around the popups that
  are open. It reports what a user would see, and puts back what it
  changed.

  Rule: Colour

    Scenario: An icon button drawn in the colour of what is behind it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 200; height: 80
          color: "white"
          Button {
            anchors.centerIn: parent
            Accessible.name: "Record"
            background: Item {}
            contentItem: Rectangle { implicitWidth: 20; implicitHeight: 20; radius: 10; color: "#f4f4f4" }
          }
        }
        """
      Then the design audit reports "color-nontext-contrast"

    Scenario: An icon button drawn in the text colour
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 200; height: 80
          color: "white"
          Button {
            anchors.centerIn: parent
            Accessible.name: "Record"
            background: Item {}
            contentItem: Rectangle { implicitWidth: 20; implicitHeight: 20; radius: 10; color: "#202020" }
          }
        }
        """
      Then the design audit does not report "color-nontext-contrast"

    Scenario: A toggle whose two states differ only in a tint
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 100
          color: "white"
          Button {
            objectName: "bold"
            text: "Bold"; checkable: true
            background: Item {}
            contentItem: Text {
              text: parent.text
              color: parent.checked ? "#1c71d8" : "#707070"
            }
          }
        }
        """
      Then the design audit reports "color-state-only"
      And the property "checked" of "bold" is false

    Scenario: A toggle whose checked state is filled
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 100
          color: "white"
          Button {
            text: "Bold"; checkable: true
            background: Rectangle {
              implicitWidth: 80; implicitHeight: 32
              color: parent.checked ? "#1c71d8" : "white"
              border.color: "#707070"
            }
            contentItem: Text {
              text: parent.text
              color: parent.checked ? "white" : "black"
            }
          }
        }
        """
      Then the design audit does not report "color-state-only"

    Scenario: A switch whose handle moves across a pale track
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 100
          color: "white"
          Switch {
            id: control
            Accessible.name: "Heating"
            indicator: Rectangle {
              implicitWidth: 56; implicitHeight: 24
              radius: 12
              color: control.checked ? "#a8e6c4" : "white"
              border.color: "#b0b0b0"
              Rectangle {
                x: control.checked ? parent.width - width - 2 : 2
                width: 20; height: 20; radius: 10
                border.color: "#b0b0b0"
              }
            }
          }
        }
        """
      Then the design audit does not report "color-state-only"

    Scenario: A menu that lets the text behind it show through
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "white"
          Text { anchors.fill: parent; wrapMode: Text.Wrap; text: "Body text that runs behind the menu. ".repeat(12) }
          Menu {
            x: 20; y: 20
            Component.onCompleted: open()
            background: Rectangle { implicitWidth: 200; color: Qt.rgba(1, 1, 1, 0.7); border.color: "#808080" }
            MenuItem { text: "Cut" }
            MenuItem { text: "Copy" }
          }
        }
        """
      Then the design audit reports "color-translucent-surface"

    Scenario: An opaque menu over text
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "white"
          Text { anchors.fill: parent; wrapMode: Text.Wrap; text: "Body text that runs behind the menu. ".repeat(12) }
          Menu {
            x: 20; y: 20
            Component.onCompleted: open()
            background: Rectangle { implicitWidth: 200; color: "white"; border.color: "#808080" }
            MenuItem { text: "Cut" }
            MenuItem { text: "Copy" }
          }
        }
        """
      Then the design audit does not report "color-translucent-surface"

    Scenario: A popup in the colour of the page, with no edge
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "#f0f0f0"
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            background: Rectangle { color: "#f0f0f0" }
            Label { text: "Saved" }
          }
        }
        """
      Then the design audit reports "color-surface-merge"

    Scenario: A popup with a border
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "#f0f0f0"
          Popup {
            x: 40; y: 40
            width: 200; height: 120
            Component.onCompleted: open()
            background: Rectangle { color: "#f0f0f0"; border.color: "#808080" }
            Label { text: "Saved" }
          }
        }
        """
      Then the design audit does not report "color-surface-merge"

    Scenario: A destructive action drawn green
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "white"
          Button { x: 40; y: 40; text: "Delete"; palette.button: "#26a269"; palette.buttonText: "white" }
        }
        """
      Then the design audit reports "color-semantic-hue" on the "button" named "Delete"

    Scenario: A destructive action drawn red
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "white"
          Button { x: 40; y: 40; text: "Delete"; palette.button: "#c01c28"; palette.buttonText: "white" }
        }
        """
      Then the design audit does not report "color-semantic-hue"

    Scenario: Two highlighted buttons side by side
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "white"
          Row {
            x: 40; y: 40; spacing: 8
            Button { text: "Save"; highlighted: true }
            Button { text: "Publish"; highlighted: true }
          }
        }
        """
      Then the design audit reports "color-accent-weight" on the "button" named "Publish"

    Scenario: One highlighted button
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 360; height: 240
          color: "white"
          Row {
            x: 40; y: 40; spacing: 8
            Button { text: "Save" }
            Button { text: "Publish"; highlighted: true }
          }
        }
        """
      Then the design audit does not report "color-accent-weight"

    Scenario: A page drawn all in the accent
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        ApplicationWindow {
          width: 360; height: 240
          visible: true
          color: palette.highlight
          Label { anchors.centerIn: parent; text: "Welcome"; color: palette.highlightedText }
        }
        """
      Then the design audit reports "color-accent-weight"

  Rule: Focus

    Scenario: A custom row that looks the same with keyboard focus
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 100
          color: "white"
          Rectangle {
            width: 240; height: 40; color: "#e0e0e0"
            activeFocusOnTab: true
            Accessible.role: Accessible.Button
            Accessible.name: "Inbox"
            Label { anchors.centerIn: parent; text: "Inbox" }
          }
        }
        """
      Then the design audit reports "focus-indicator-missing" on the "button" named "Inbox"

    Scenario: A custom row that draws a ring with keyboard focus
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 100
          color: "white"
          Rectangle {
            width: 240; height: 40; color: "#e0e0e0"
            activeFocusOnTab: true
            border.width: activeFocus ? 2 : 0
            border.color: "#1c71d8"
            Accessible.role: Accessible.Button
            Accessible.name: "Inbox"
            Label { anchors.centerIn: parent; text: "Inbox" }
          }
        }
        """
      Then the design audit does not report "focus-indicator-missing"

    Scenario: Focus is where it was after the audit
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 200
          color: "white"
          Column {
            TextField { objectName: "first"; placeholderText: "Name" }
            TextField { objectName: "second"; placeholderText: "City"; focus: true }
          }
        }
        """
      Then the design audit does not report "focus-indicator-missing"
      And the property "activeFocus" of "second" is true
      And the property "activeFocus" of "first" is false

    Scenario: Tab that goes back up the column
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Column {
            x: 20; y: 20; spacing: 8
            TextField { id: name; placeholderText: "Name"; KeyNavigation.tab: city }
            TextField { id: street; placeholderText: "Street"; KeyNavigation.tab: done }
            TextField { id: city; placeholderText: "City"; KeyNavigation.tab: street }
            Button { id: done; text: "Done"; KeyNavigation.tab: name }
          }
        }
        """
      Then the design audit reports "focus-order"

    Scenario: Tab down a column, then to the next column
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Row {
            x: 20; y: 20; spacing: 20
            Column {
              spacing: 8
              Button { text: "Inbox" }
              Button { text: "Sent" }
            }
            Column {
              spacing: 8
              TextField { placeholderText: "To" }
              TextField { placeholderText: "Subject" }
            }
          }
        }
        """
      Then the design audit does not report "focus-order"

    Scenario: A dialog that Escape does not close
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Dialog {
            title: "Settings"; standardButtons: Dialog.Close
            closePolicy: Popup.NoAutoClose
            Component.onCompleted: open()
            TextField { placeholderText: "Search" }
          }
        }
        """
      Then the design audit reports "focus-escape-dismiss"

    Scenario: A dialog that closes on Escape is open after the audit
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Dialog {
            objectName: "settings"
            title: "Settings"; standardButtons: Dialog.Close
            Component.onCompleted: open()
            TextField { placeholderText: "Search" }
          }
        }
        """
      Then the design audit does not report "focus-escape-dismiss"
      And the property "opened" of "settings" is true

    Scenario: A dialog that handles Escape itself
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Dialog {
            id: dialog
            objectName: "settings"
            title: "Settings"; standardButtons: Dialog.Close
            closePolicy: Popup.NoAutoClose
            Component.onCompleted: open()
            TextField { placeholderText: "Search"; Keys.onEscapePressed: dialog.close() }
          }
        }
        """
      Then the design audit does not report "focus-escape-dismiss"
      And the property "opened" of "settings" is true

    Scenario: A modal popup that leaves focus behind it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Popup {
            modal: true; focus: false
            closePolicy: Popup.NoAutoClose
            Component.onCompleted: open()
            Label { text: "Choose a colour" }
          }
        }
        """
      Then the design audit reports "focus-popup-stranded"

    Scenario: A modal popup that takes focus
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Popup {
            modal: true; focus: true
            Component.onCompleted: open()
            Label { text: "Choose a colour" }
          }
        }
        """
      Then the design audit does not report "focus-popup-stranded"

    Scenario: A window shortcut that still acts behind a modal dialog
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Shortcut { sequence: "Ctrl+W"; onActivated: console.log("close tab") }
          Dialog {
            modal: true; focus: true; title: "Rename"; standardButtons: Dialog.Ok
            Component.onCompleted: open()
            TextField {}
          }
        }
        """
      Then the design audit reports "focus-shortcut-under-modal"

    Scenario: A window shortcut switched off while the dialog is open
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 400; height: 300
          color: "white"
          Shortcut { sequence: "Ctrl+W"; enabled: !dlg.opened; onActivated: console.log("close tab") }
          Dialog {
            id: dlg
            modal: true; focus: true; title: "Rename"; standardButtons: Dialog.Ok
            Component.onCompleted: open()
            TextField {}
          }
        }
        """
      Then the design audit does not report "focus-shortcut-under-modal"

    Scenario: A list whose every row is a Tab stop
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300
          color: "white"
          ListView {
            anchors.fill: parent
            model: ["Inbox", "Drafts", "Sent", "Archive", "Trash"]
            delegate: CheckBox { text: modelData }
          }
        }
        """
      Then the design audit reports "focus-arrow-keys"

    Scenario: Three radio buttons are few enough to Tab through
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 300; height: 300
          color: "white"
          Column {
            RadioButton { text: "Small"; checked: true }
            RadioButton { text: "Medium" }
            RadioButton { text: "Large" }
          }
        }
        """
      Then the design audit does not report "focus-arrow-keys"
