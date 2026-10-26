# Sources:
#   docs/design/README.md (the design rules and how each is checked)
#   https://www.w3.org/TR/WCAG22/#contrast-minimum
#   https://www.w3.org/TR/WCAG22/#reflow
#   https://doc.qt.io/qt-6/qml-qtquick-controls-button.html
# The design audit is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @a11y
Feature: The design audit looks at what the window shows
  The design audit lays out and draws the window as the user sees it and
  reports what breaks a design rule: a control the window cuts off, text
  too faint to read on what is behind it. Each finding names the rule and
  the accessibility node it is about. It can look at the window with the
  text larger and the window smaller, and afterwards the window is as it
  was.

  Scenario: A window that breaks no rule
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        Button { x: 20; y: 20; text: "Save" }
      }
      """
    Then the design audit finds nothing

  Scenario: A button the window cuts off
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Rectangle {
        width: 400; height: 300
        color: "white"
        Button { x: 360; y: 20; text: "Save" }
      }
      """
    Then the design audit reports "layout-offscreen" on the "button" named "Save"

  Scenario: Text too faint to read on its background
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 400; height: 300
        color: "white"
        Text { x: 20; y: 20; text: "Saved a minute ago"; color: "#c8c8c8" }
      }
      """
    Then the design audit reports "color-text-contrast" on the "label" named "Saved a minute ago"

  Rule: Larger text in a smaller window

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Rectangle {
          width: 560; height: 420
          color: "white"
          Row {
            objectName: "row"
            spacing: 8
            Button { text: "Open the file" }
            Button { text: "Save the file" }
            Button { text: "Print the file" }
            Button { text: "Close the file" }
          }
        }
        """

    Scenario: A row that fits until the text is larger
      When the design audit runs at font scale 2 and window size 560x420
      Then the design audit reports "layout-offscreen" on the "button" named "Close the file"

    Scenario: The window is as it was after the audit
      When the design audit runs at font scale 2 and window size 560x420
      Then the property "width" of "row" is at most 560
