# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-control.html#focusPolicy-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#activeFocusOnTab-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Which controls take the keyboard
  What is pressed takes the keyboard on Tab and on a click. What holds
  other things, and a row of a list, leaves it where it is.

  @api-QtQuick.Controls.Control.focusPolicy
  Scenario Outline: A <control> has the focus policy <policy>
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        <control> { objectName: "control" }
      }
      """
    Then the property "focusPolicy" of "control" is <policy>
    And the property "activeFocusOnTab" of "control" is <stop>

    Examples:
      | control        | policy | stop  |
      | AbstractButton | 11     | true  |
      | Button         | 11     | true  |
      | ToolButton     | 11     | true  |
      | CheckBox       | 11     | true  |
      | RadioButton    | 11     | true  |
      | Switch         | 11     | true  |
      | MenuItem       | 11     | true  |
      | ComboBox       | 11     | true  |
      | ItemDelegate   | 0      | false |
      | Control        | 0      | false |
      | Frame          | 0      | false |
      | Pane           | 0      | false |
      | GroupBox       | 0      | false |
      | Page           | 0      | false |
      | ToolBar        | 0      | false |
      | ScrollBar      | 0      | false |
      | ScrollView     | 0      | false |
      | SpinBox        | 0      | false |

  Scenario Outline: The policy a button is written with says whether Tab stops at it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Button { objectName: "control"; focusPolicy: <policy> }
      }
      """
    Then the property "activeFocusOnTab" of "control" is <stop>

    Examples:
      | policy         | stop  |
      | Qt.NoFocus     | false |
      | Qt.ClickFocus  | false |
      | Qt.TabFocus    | true  |
      | Qt.StrongFocus | true  |

  Scenario: Tab can be told to pass a button by and the policy stays
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Button { objectName: "control"; activeFocusOnTab: false }
      }
      """
    Then the property "activeFocusOnTab" of "control" is false
    And the property "focusPolicy" of "control" is 11

  # TODO(label): a Label has no `focusPolicy` yet.
  @backlog
  Scenario: A Label has a focus policy and takes no focus by it
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        Label { objectName: "control" }
      }
      """
    Then the property "focusPolicy" of "control" is 0
    And the property "activeFocusOnTab" of "control" is false
