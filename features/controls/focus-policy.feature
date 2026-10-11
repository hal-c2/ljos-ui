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

  Scenario Outline: A <control> has the focus policy <policy> though it is no control
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
      | control   | policy | stop  |
      | Label     | 0      | false |
      | TextField | 1      | true  |
      | TextArea  | 1      | true  |

  Rule: Tab stopping at a control reads back as the Tab bit of its policy

    Scenario Outline: A <control> that Tab is told to stop at has the policy <policy>
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          <control> { objectName: "control"; activeFocusOnTab: true }
        }
        """
      Then the property "focusPolicy" of "control" is <policy>
      And the property "activeFocusOnTab" of "control" is true

      Examples:
        | control      | policy |
        | Control      | 1      |
        | Label        | 1      |
        | Pane         | 1      |
        | ItemDelegate | 1      |
        | SpinBox      | 1      |
        | Button       | 11     |

    Scenario: The Tab bit goes when Tab is told to pass the control by again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          Control { objectName: "control"; activeFocusOnTab: true }
        }
        """
      When the property "activeFocusOnTab" of "control" is set to false
      Then the property "focusPolicy" of "control" is 0

    Scenario: The Tab bit comes when Tab is told to stop at the control later
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          Label { objectName: "control" }
        }
        """
      When the property "activeFocusOnTab" of "control" is set to true
      Then the property "focusPolicy" of "control" is 1

    Scenario: A policy that is written over the Tab bit takes Tab away again
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          Control { objectName: "control"; activeFocusOnTab: true }
        }
        """
      When the property "focusPolicy" of "control" is set to 2
      Then the property "focusPolicy" of "control" is 2
      And the property "activeFocusOnTab" of "control" is false

    Scenario Outline: The policy of a <control> says whether Tab stops at it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
          width: 300; height: 200
          <control> { objectName: "control" }
        }
        """
      When the property "focusPolicy" of "control" is set to <policy>
      Then the property "focusPolicy" of "control" is <policy>
      And the property "activeFocusOnTab" of "control" is <stop>

      Examples:
        | control   | policy | stop  |
        | Label     | 11     | true  |
        | Label     | 2      | false |
        | TextField | 2      | false |
        | TextField | 11     | true  |
        | TextArea  | 2      | false |

  Rule: A label takes the keyboard the way its policy says

    Scenario: A click brings the keyboard to a label whose policy has click focus
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
            width: 200; height: 100
            Label {
                objectName: "label"
                width: 200; height: 40
                text: "Name"
                focusPolicy: Qt.ClickFocus
            }
        }
        """
      When the user touches 20, 20
      Then the property "activeFocus" of "label" is true

    Scenario: Tab stops at a label whose policy has Tab focus
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
            width: 200; height: 100
            Label {
                objectName: "label"
                text: "Name"
                focusPolicy: Qt.TabFocus
            }
        }
        """
      When the user presses "Tab"
      Then the property "activeFocus" of "label" is true

    Scenario Outline: What is behind a label with the policy <policy> gets <clicks> of one click
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Controls.Basic
        Item {
            width: 200; height: 100
            MouseArea {
                objectName: "behind"
                property int clicks: 0
                anchors.fill: parent
                onClicked: clicks += 1
            }
            Label {
                objectName: "label"
                width: 200; height: 40
                text: "Name"
                focusPolicy: <policy>
            }
        }
        """
      When the user touches 20, 20
      Then the property "clicks" of "behind" is <clicks>
      And the property "activeFocus" of "label" is <keyboard>

      Examples:
        | policy        | clicks | keyboard |
        | Qt.NoFocus    | 1      | false    |
        | Qt.TabFocus   | 1      | false    |
        | Qt.ClickFocus | 0      | true     |
