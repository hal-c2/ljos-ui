# Sources:
#   docs/design/accessibility.md (the rules and why)
#   https://www.w3.org/WAI/WCAG22/Understanding/label-in-name.html
#   https://www.w3.org/WAI/WCAG22/Understanding/name-role-value.html
#   https://www.w3.org/WAI/WCAG22/Understanding/status-messages.html
#   https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/
#   https://doc.qt.io/qt-6/qml-qtquick-accessible.html
# The design audit is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @a11y
Feature: The design audit checks names and roles in the live tree
  What an assistive technology reads is the accessibility tree the window
  builds as it runs. The design audit reads that tree and says where a
  name is missing, says something other than what is shown, or cannot be
  told from another's, and where a dialog, a status line or the current
  row does not say what it is.

  Scenario: An icon button with no name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        ToolButton {
          x: 20; y: 20
          icon.name: "edit-copy"
          ToolTip.text: "Copy"
        }
      }
      """
    Then the design audit reports "a11y-unnamed-icon-button" on the "button" named ""
    And the design audit does not report "a11y-unnamed-interactive"

  Scenario: An icon button that nothing explains on hover
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        ToolButton {
          x: 20; y: 20
          icon.name: "edit-copy"
          Accessible.name: "Copy"
        }
      }
      """
    Then the design audit reports "a11y-icon-button-no-tooltip" on the "button" named "Copy"

  Scenario: A button whose name leaves out what it shows
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        Button { x: 20; y: 20; text: "Send"; Accessible.name: "Submit the form" }
      }
      """
    Then the design audit reports "a11y-name-mismatch" on the "button" named "Submit the form"

  Scenario: A name may say more than the label, in the label's order
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        Button { x: 20; y: 20; text: "More options"; Accessible.name: "More file options" }
      }
      """
    Then the design audit does not report "a11y-name-mismatch"

  Scenario: A button inside a row that is pressed too
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        ItemDelegate {
          width: 300; text: "Report.pdf"
          Button { x: 200; text: "Delete" }
        }
      }
      """
    Then the design audit reports "a11y-nested-interactive" on the "button" named "Delete"

  Scenario: Tab stops on an item left out of the tree
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        Rectangle {
          x: 20; y: 20; width: 120; height: 40; color: "#dddddd"
          activeFocusOnTab: true
          Accessible.ignored: true
        }
      }
      """
    Then the design audit reports "a11y-ignored-focusable" at line 6
    And the design audit does not report "a11y-unnamed-interactive"

  Scenario: A dialog with no title
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        Dialog {
          x: 50; y: 50; width: 300; height: 150
          modal: true
          standardButtons: Dialog.Ok
          Component.onCompleted: open()
        }
      }
      """
    Then the design audit reports "a11y-unnamed-dialog" at line 6

  Scenario: A dialog with a title has its name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        Dialog {
          x: 50; y: 50; width: 300; height: 150
          modal: true
          title: "Reload the file?"
          standardButtons: Dialog.Ok
          Component.onCompleted: open()
        }
      }
      """
    Then the design audit does not report "a11y-unnamed-dialog"

  Scenario: Two buttons in different bars with one name
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        ToolBar { width: 400; ToolButton { text: "More options" } }
        ToolBar { y: 100; width: 400; ToolButton { text: "More options" } }
      }
      """
    Then the design audit reports "a11y-ambiguous-name" on the "button" named "More options"
    And the design audit does not report "a11y-duplicate-name"

  Scenario: The same name in each row of a list is told apart by its row
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        ListView {
          width: 400; height: 300
          model: ["Report.pdf", "Notes.txt"]
          delegate: Pane {
            width: 400
            Accessible.role: Accessible.ListItem
            Accessible.name: modelData
            Button { text: "Open" }
          }
        }
      }
      """
    Then the design audit does not report "a11y-ambiguous-name"

  Scenario: The current row of a list looks like the others
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        ListView {
          anchors.fill: parent
          currentIndex: 1
          model: ["Light", "Dark", "High contrast"]
          delegate: ItemDelegate { width: 400; text: modelData }
        }
      }
      """
    Then the design audit reports "a11y-current-unmarked" on the "listitem" named "Dark"

  Scenario: The current row of a list is highlighted
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        ListView {
          anchors.fill: parent
          currentIndex: 1
          model: ["Light", "Dark", "High contrast"]
          delegate: ItemDelegate {
            width: 400; text: modelData
            highlighted: ListView.isCurrentItem
          }
        }
      }
      """
    Then the design audit does not report "a11y-current-unmarked"

  Scenario: A status line that does not say it is one
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 400; height: 300
        color: "white"
        property int saved: 2
        Text { id: statusLine; x: 20; y: 20; text: "Saved " + saved + " times" }
      }
      """
    Then the design audit reports "a11y-live-status" at line 6

  Scenario: A status line marked as a status bar
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 400; height: 300
        color: "white"
        property int saved: 2
        Text {
          id: statusLine; x: 20; y: 20; text: "Saved " + saved + " times"
          Accessible.role: Accessible.StatusBar
          Accessible.name: text
        }
      }
      """
    Then the design audit does not report "a11y-live-status"
