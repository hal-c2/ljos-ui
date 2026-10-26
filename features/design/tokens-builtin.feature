# Sources:
#   docs/ARCHITECTURE.md section 14 (design guardrails)
#   https://doc.qt.io/qt-6/qml-qtquick-application.html#styleHints-prop
#   https://doc.qt.io/qt-6/qaccessibilityhints.html
# Ljós builds the module in; on Qt an application imports the files
# `ljos design export` writes (tokens.feature). The system settings are
# given to Ljós only.

@ljos-only
Feature: The design module built in
  `import Ljos.Design` needs no files on Ljós. Its colours follow the
  colour scheme and the contrast the system asks for, and its motion
  stays still when the user asks for reduced motion.

  Scenario: The module is there with no files of the application's
    Given the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color canvas: Colors.canvas
        property int enter: Motion.enterPanel
      }
      """
    Then the property "canvas" of "root" is "#f8f9fa"
    And the property "enter" of "root" is 250

  Scenario Outline: The palette follows what the system asks for
    Given the system setting "<setting>" is <value>
    And the QML:
      """qml
      import QtQuick
      import Ljos.Design
      Item {
        objectName: "root"
        property color accent: Colors.accent
        property color textMuted: Colors.textMuted
      }
      """
    Then the property "accent" of "root" is "<accent>"
    And the property "textMuted" of "root" is "<muted>"

    Examples:
      | setting      | value  | accent  | muted   |
      | color scheme | "dark" | #007cb7 | #b5bbc0 |
      | contrast     | "high" | #005e8c | #242729 |

  # The transition fades on its own with reduced motion and the runtime
  # keeps that fade, capped at the same 150 milliseconds, rather than
  # ending it or adding a second one.
  Scenario: With reduced motion a popup fades in without growing
    Given the system setting "reduced motion" is true
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import Ljos.Design
      Item {
        objectName: "root"; width: 400; height: 400
        property bool reduced: Motion.reduced
        property bool faint: p.opacity > 0 && p.opacity < 1
        Popup { id: p; objectName: "p"; width: 100; height: 100; enter: EnterTransition {} }
      }
      """
    When the method "open" of "p" is called
    And 32 milliseconds pass
    Then the property "reduced" of "root" is true
    And the property "scale" of "p" is 1
    And the property "faint" of "root" is true
    And the property "opened" of "p" is false
    When 150 milliseconds pass
    Then the property "opened" of "p" is true
    And the property "opacity" of "p" is 1
    And the property "scale" of "p" is 1
