# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
#   https://doc.qt.io/qt-6/qml-qtquick-controls-popup.html#palette-prop
#   https://doc.qt.io/qt-6/qstylehints.html#colorScheme-prop
#   https://doc.qt.io/qt-6/qtquickcontrols-styles.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Control.palette
Feature: The palette of a control in a dark colour scheme
  The Basic style has a second palette for a dark colour scheme. A control
  has it while `Qt.styleHints.colorScheme` is dark, whether the system or
  the application said so, and what the style draws follows.

  # Qt hands a new scheme to the controls the next time the application
  # has nothing else to do, which is what the wait in each scenario is for.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      ApplicationWindow {
        objectName: "root"
        width: 300; height: 300; visible: true
        function ask(scheme) { Qt.styleHints.colorScheme = scheme }
        property Item head: null
        function show() { head = bar.itemAt(0); head.triggered() }
        Pane {
          objectName: "outer"
          palette.button: "#aa0000"
          Button { objectName: "inside"; text: "B" }
        }
        Button { objectName: "free"; y: 100; text: "B" }
        Button { objectName: "off"; y: 150; text: "B"; enabled: false }
        Label { objectName: "words"; y: 200; text: "L" }
        TextField { objectName: "field"; y: 230 }
        Popup { objectName: "over" }
        Button { objectName: "lit"; x: 150; text: "B"; highlighted: true }
        Button { objectName: "held"; x: 150; y: 50; text: "B"; down: true }
        ComboBox { objectName: "paper"; x: 150; y: 100; model: ["A4"]; down: true }
        MenuBar { id: bar; y: 260; Menu { title: "File"; MenuItem { text: "New" } } }
      }
      """

  Scenario Outline: The Basic style is dark in a dark scheme
    When the method "ask" of "root" is called with 2
    And 100 milliseconds pass
    Then the property "palette.<role>" of "free" is "<colour>"

    Examples:
      | role            | colour    |
      | window          | #000000   |
      | windowText      | #d4d6d8   |
      | base            | #000000   |
      | text            | #eff0f2   |
      | button          | #2f2f2f   |
      | buttonText      | #d4d6d8   |
      | brightText      | #000000   |
      | light           | #1a1a1a   |
      | midlight        | #2c2c2c   |
      | mid             | #626262   |
      | dark            | #c8c9cb   |
      | shadow          | #28282a   |
      | highlight       | #0d69f2   |
      | highlightedText | #fdfdfd   |
      | link            | #2f86b1   |
      | linkVisited     | #ff00ff   |
      | toolTipBase     | #000000   |
      | toolTipText     | #ffffff   |
      | placeholderText | #88c8c9cb |

  Scenario Outline: A control that is not enabled has paler dark colours
    When the method "ask" of "root" is called with 2
    And 100 milliseconds pass
    Then the property "palette.<role>" of "off" is "<colour>"

    Examples:
      | role       | colour    |
      | buttonText | #4dd4d6d8 |
      | windowText | #3f4040   |
      | text       | #7fc8c9cb |
      | base       | #292929   |
      | brightText | #4d000000 |
      | highlight  | #01060f   |
      | button     | #2f2f2f   |
      | window     | #000000   |

  Scenario Outline: What the style draws is dark too
    When the method "ask" of "root" is called with 2
    And 100 milliseconds pass
    Then the property "<property>" of "<object>" is "<colour>"

    Examples:
      | object | property             | colour    |
      | free   | background.color     | #2f2f2f   |
      | free   | contentItem.color    | #d4d6d8   |
      | outer  | background.color     | #000000   |
      | words  | color                | #d4d6d8   |
      | words  | palette.windowText   | #d4d6d8   |
      | field  | color                | #eff0f2   |
      | field  | background.color     | #000000   |
      | field  | placeholderTextColor | #88c8c9cb |
      | lit    | background.color     | #c8c9cb   |
      | lit    | contentItem.color    | #000000   |
      | held   | background.color     | #494949   |
      | held   | contentItem.color    | #d4d6d8   |
      | paper  | background.color     | #626262   |
      | paper  | contentItem.color    | #d4d6d8   |
      | over   | palette.window       | #000000   |
      | over   | background.color     | #000000   |
      | root   | palette.window       | #000000   |
      | root   | color                | #000000   |

  # 4.19 to 1, where the accessibility audit asks for 4.5 to 1: the colours
  # are Qt's, and they are kept.
  Scenario: The item of an open menu of a menu bar is dark too
    When the method "ask" of "root" is called with 2
    And 100 milliseconds pass
    And the method "show" of "root" is called
    Then the property "head.highlighted" of "root" is true
    And the property "head.background.color" of "root" is "#626262"
    And the property "head.contentItem.color" of "root" is "#d4d6d8"

  Scenario: A colour the application set stays in a dark scheme
    When the method "ask" of "root" is called with 2
    And 100 milliseconds pass
    Then the property "palette.button" of "inside" is "#aa0000"
    And the property "background.color" of "inside" is "#aa0000"
    And the property "palette.buttonText" of "inside" is "#d4d6d8"

  Scenario: The light palette is back with the light scheme
    When the method "ask" of "root" is called with 2
    And 100 milliseconds pass
    And the method "ask" of "root" is called with 1
    And 100 milliseconds pass
    Then the property "palette.button" of "free" is "#e0e0e0"
    And the property "palette.buttonText" of "free" is "#26282a"
    And the property "palette.window" of "over" is "#ffffff"
    And the property "background.color" of "free" is "#e0e0e0"
    And the property "palette.button" of "inside" is "#aa0000"
