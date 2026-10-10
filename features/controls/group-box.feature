# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-controls-groupbox.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: GroupBox
  A group box is a frame with a title above it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      Item {
        width: 300; height: 200
        GroupBox { objectName: "empty" }
        GroupBox {
          objectName: "g"
          y: 50
          title: "T"
          Rectangle { objectName: "kid"; implicitWidth: 30; implicitHeight: 20 }
        }
      }
      """

  @api-QtQuick.Controls.GroupBox.title @api-QtQuick.Controls.GroupBox.implicitLabelWidth
  Scenario Outline: A group box without a title is a frame
    Then the property "<property>" of "empty" is <value>

    Examples:
      | property                | value       |
      | title                   | ""          |
      | implicitWidth           | 24          |
      | implicitHeight          | 24          |
      | padding                 | 12          |
      | topPadding              | 12          |
      | spacing                 | 6           |
      | implicitLabelWidth      | 0           |
      | background.y            | 0           |
      | background.height       | 24          |
      | background.color        | "#00000000" |
      | background.border.color | "#bdbdbd"   |

  Scenario: The title is shown by the label
    Then the property "label.text" of "g" is "T"
    And the property "label.x" of "g" is 12
    And the property "label.y" of "g" is 0
    And the property "label.width" of "g" is 30
    When the property "title" of "g" is set to "Other"
    Then the property "label.text" of "g" is "Other"

  Scenario: The title makes room for itself above the content
    When the property "spacing" of "g" is set to 10
    And the property "title" of "g" is bound to "'' + (topPadding - implicitLabelHeight)"
    Then the property "title" of "g" is "22"

  Scenario: The frame starts below the title
    When the property "title" of "g" is bound to "'' + (background.y + bottomPadding - topPadding)"
    Then the property "title" of "g" is "0"
    When the property "title" of "g" is bound to "'' + (background.y + background.height - height)"
    Then the property "title" of "g" is "0"

  Scenario Outline: The content is as in a frame
    Then the property "<property>" of "g" is <value>

    Examples:
      | property           | value |
      | contentWidth       | 30    |
      | contentHeight      | 20    |
      | implicitWidth      | 54    |
      | contentItem.x      | 12    |
      | contentItem.width  | 30    |
      | contentItem.height | 20    |
      | bottomPadding      | 12    |
      | background.x       | 0     |
      | background.width   | 54    |

  Scenario: What is put into a group box becomes a child of its content item
    Then the property "parent.parent.objectName" of "kid" is "g"

  @api-QtQuick.Controls.GroupBox.label
  Scenario: The application can replace the label
    Given the QML:
      """qml
      import QtQuick
      import QtQuick.Controls.Basic
      GroupBox {
        objectName: "own"
        title: "T"
        label: Rectangle { objectName: "mine"; implicitWidth: 80; implicitHeight: 30 }
      }
      """
    Then the property "implicitLabelWidth" of "own" is 80
    And the property "implicitLabelHeight" of "own" is 30
    And the property "topPadding" of "own" is 48
    And the property "implicitWidth" of "own" is 104
    And the property "parent.objectName" of "mine" is "own"
