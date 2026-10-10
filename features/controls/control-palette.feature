# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#palette-prop
#   https://doc.qt.io/qt-6/qml-qtquick-colorgroup.html
#   https://doc.qt.io/qt-6/qtquickcontrols-customize.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.Controls.Control.palette
Feature: The palette of a control
  The palette holds the colours a style draws with. A control takes it from
  the control or window it is in; a colour it sets itself stays.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Pane {
          objectName: "outer"
          palette.button: "#aa0000"
          Column {
            Button { objectName: "b"; text: "B"; palette.buttonText: "#00aa00" }
            Pane {
              objectName: "inner"
              palette.window: "#0000aa"
              Button { objectName: "deep" }
            }
          }
        }
        Button { objectName: "free"; y: 150 }
      }
      """

  Scenario Outline: The Basic style is light
    Then the property "palette.<role>" of "free" is "<colour>"

    Examples:
      | role            | colour    |
      | window          | #ffffff   |
      | windowText      | #26282a   |
      | base            | #ffffff   |
      | text            | #353637   |
      | button          | #e0e0e0   |
      | buttonText      | #26282a   |
      | brightText      | #ffffff   |
      | light           | #f6f6f6   |
      | midlight        | #e4e4e4   |
      | mid             | #bdbdbd   |
      | dark            | #353637   |
      | shadow          | #28282a   |
      | highlight       | #0066ff   |
      | highlightedText | #090909   |
      | link            | #45a7d7   |
      | toolTipBase     | #ffffff   |
      | toolTipText     | #000000   |
      | placeholderText | #88353637 |

  Scenario Outline: A control that is not enabled has paler colours
    When the property "enabled" of "free" is set to false
    Then the property "palette.<role>" of "free" is "<colour>"

    Examples:
      | role       | colour    |
      | buttonText | #4d26282a |
      | windowText | #bdbebf   |
      | text       | #7f353637 |
      | base       | #d6d6d6   |
      | brightText | #4dffffff |
      | highlight  | #f0f6ff   |
      | button     | #e0e0e0   |
      | window     | #ffffff   |

  Scenario Outline: A control takes the colours of the control it is in
    Then the property "<property>" of "<object>" is "<colour>"

    Examples:
      | object | property           | colour  |
      | b      | palette.button     | #aa0000 |
      | b      | palette.buttonText | #00aa00 |
      | b      | palette.window     | #ffffff |
      | b      | background.color   | #aa0000 |
      | inner  | palette.button     | #aa0000 |
      | inner  | background.color   | #0000aa |
      | deep   | palette.button     | #aa0000 |
      | deep   | palette.window     | #0000aa |
      | deep   | palette.buttonText | #26282a |
      | outer  | palette.window     | #ffffff |
      | free   | background.color   | #e0e0e0 |

  Scenario: A later change reaches everything inside
    When the property "palette.button" of "outer" is set to "#00aaaa"
    Then the property "palette.button" of "b" is "#00aaaa"
    And the property "palette.button" of "deep" is "#00aaaa"
    And the property "background.color" of "deep" is "#00aaaa"

  Scenario: A colour a control set itself stays
    When the property "palette.button" of "b" is set to "#123456"
    And the property "palette.button" of "outer" is set to "#00aaaa"
    Then the property "palette.button" of "b" is "#123456"
    And the property "palette.button" of "deep" is "#00aaaa"

  Scenario: A colour set in between reaches only what is inside
    When the property "palette.mid" of "inner" is set to "#654321"
    Then the property "palette.mid" of "deep" is "#654321"
    And the property "palette.mid" of "b" is "#bdbdbd"

  Scenario: A control in a disabled one has the paler colours too
    When the property "enabled" of "outer" is set to false
    Then the property "enabled" of "deep" is false
    And the property "palette.buttonText" of "deep" is "#4d26282a"
    And the property "palette.button" of "deep" is "#aa0000"
