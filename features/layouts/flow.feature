# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-flow.html
#   https://doc.qt.io/qt-6/qtquick-positioning-layouts.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Flow
  A Flow places its children side by side and wraps to a new line when the
  next one does not fit.

  Scenario Outline: A left-to-right Flow wraps at its width
    Given the QML:
      """qml
      import QtQuick
      Flow {
        objectName: "flow"
        width: 100; spacing: 5
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "b"; width: 40; height: 20 }
        Item { objectName: "c"; width: 40; height: 15 }
        Item { objectName: "d"; width: 55; height: 5 }
        Item { objectName: "e"; width: 120; height: 5 }
        Item { objectName: "f"; width: 10; height: 5 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "implicitWidth" of "flow" is 120
    And the property "implicitHeight" of "flow" is 60
    And the property "height" of "flow" is 60

    Examples:
      | item | x  | y  |
      | a    | 0  | 0  |
      | b    | 45 | 0  |
      | c    | 0  | 25 |
      | d    | 45 | 25 |
      | e    | 0  | 45 |
      | f    | 0  | 55 |

  Scenario Outline: A top-to-bottom Flow wraps at its height
    Given the QML:
      """qml
      import QtQuick
      Flow {
        objectName: "flow"
        height: 50; spacing: 5; flow: Flow.TopToBottom
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "b"; width: 20; height: 20 }
        Item { objectName: "c"; width: 45; height: 40 }
        Item { objectName: "d"; width: 50; height: 30 }
        Item { objectName: "e"; width: 10; height: 5 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "implicitWidth" of "flow" is 145
    And the property "implicitHeight" of "flow" is 40

    Examples:
      | item | x  | y  |
      | a    | 0  | 0  |
      | b    | 0  | 15 |
      | c    | 45 | 0  |
      | d    | 95 | 0  |
      | e    | 95 | 35 |

  Scenario: A Flow without a width is one line
    Given the QML:
      """qml
      import QtQuick
      Flow {
        objectName: "flow"
        spacing: 5
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "hidden"; width: 40; height: 90; visible: false }
        Item { objectName: "b"; width: 40; height: 20 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "b" is 45
    And the property "y" of "b" is 0
    And the property "width" of "flow" is 85
    And the property "height" of "flow" is 20

  Scenario Outline: A right-to-left Flow starts each line at the right, inside its padding
    Given the QML:
      """qml
      import QtQuick
      Flow {
        objectName: "flow"
        width: 120; spacing: 5; layoutDirection: Qt.RightToLeft
        padding: 4; leftPadding: 10; topPadding: 1
        Item { objectName: "a"; width: 40; height: 20 }
        Item { objectName: "b"; width: 40; height: 10 }
        Item { objectName: "c"; width: 40; height: 15 }
      }
      """
    When 100 milliseconds pass
    Then the property "x" of "<item>" is <x>
    And the property "y" of "<item>" is <y>
    And the property "implicitWidth" of "flow" is 99
    And the property "implicitHeight" of "flow" is 45

    Examples:
      | item | x  | y  |
      | a    | 76 | 1  |
      | b    | 31 | 1  |
      | c    | 76 | 26 |

  Scenario: A Flow wraps again when its width changes
    Given the QML:
      """qml
      import QtQuick
      Flow {
        objectName: "flow"
        width: 100
        Item { objectName: "a"; width: 40; height: 10 }
        Item { objectName: "b"; width: 40; height: 10 }
        Item { objectName: "c"; width: 40; height: 10 }
      }
      """
    When the property "width" of "flow" is set to 130
    And 100 milliseconds pass
    Then the property "x" of "c" is 80
    And the property "y" of "c" is 0
    And the property "height" of "flow" is 10
