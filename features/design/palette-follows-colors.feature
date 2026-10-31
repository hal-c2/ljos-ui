# Sources:
#   docs/ARCHITECTURE.md (Ljos.Design)
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop

# Ljos.Design is Ljós's own module: the oracle has none to import.
@ljos-only
Feature: The controls follow Colors.dark
  An application that imports Ljos.Design says light or dark with
  `Colors.dark`, which follows the system until the application sets it. Its
  controls take the style's light or dark palette from there, so that their
  text is not left dark on dark. A colour the application gave stays.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      import Ljos.Design
      ApplicationWindow {
        objectName: "root"
        width: 300; height: 300; visible: true
        function pick(dark) { Colors.dark = dark }
        ToolButton { objectName: "tool"; text: "T" }
        Button { objectName: "given"; y: 50; text: "B"; palette.buttonText: "#123456" }
        Label { objectName: "words"; y: 100; text: "L" }
      }
      """

  Scenario: Set dark, the controls are dark
    When the method "pick" of "root" is called with true
    And 100 milliseconds pass
    Then the property "palette.buttonText" of "tool" is "#d4d6d8"
    And the property "palette.windowText" of "words" is "#d4d6d8"
    And the property "palette.buttonText" of "given" is "#123456"

  Scenario: Set light again, they are light
    When the method "pick" of "root" is called with true
    And 100 milliseconds pass
    And the method "pick" of "root" is called with false
    And 100 milliseconds pass
    Then the property "palette.buttonText" of "tool" is "#26282a"
    And the property "palette.buttonText" of "given" is "#123456"

  Scenario: Left alone, they follow the system
    When the system setting "color scheme" changes to dark
    And 100 milliseconds pass
    Then the property "palette.buttonText" of "tool" is "#d4d6d8"
