# Sources:
#   https://doc.qt.io/qt-6/qtquick-positioning-anchors.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#anchors-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Anchors
  An anchored edge follows the edge it is anchored to, with its margin.

  Scenario: fill with margins, one side overridden
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 201; height: 101
        Item {
          objectName: "c"
          anchors.fill: parent
          anchors.margins: 10
          anchors.leftMargin: 3
        }
      }
      """
    Then the property "x" of "c" is 3
    And the property "y" of "c" is 10
    And the property "width" of "c" is 188
    And the property "height" of "c" is 81

  Scenario: Anchored edges follow the parent when it resizes
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 201; height: 101
        Item {
          objectName: "d"
          width: 20; height: 20
          anchors.right: parent.right
          anchors.bottom: parent.bottom
          anchors.margins: 4
        }
      }
      """
    Then the property "x" of "d" is 177
    And the property "y" of "d" is 77
    When the property "width" of "root" is set to 300
    Then the property "x" of "d" is 276

  Scenario Outline: centerIn snaps to whole pixels unless told not to
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Item {
          objectName: "a"
          width: 51; height: 31
          anchors.centerIn: parent
          anchors.alignWhenCentered: <align>
        }
      }
      """
    Then the property "x" of "a" is <x>
    And the property "y" of "a" is <y>

    Examples:
      | align | x    | y    |
      | true  | 74   | 34   |
      | false | 74.5 | 34.5 |

  Scenario: The parent's centre line is a whole pixel
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 201; height: 101
        Item {
          objectName: "a"
          width: 50; height: 30
          anchors.centerIn: parent
        }
      }
      """
    Then the property "x" of "a" is 76
    And the property "y" of "a" is 36

  Scenario: Anchoring to a sibling's edge
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Item { id: s; objectName: "s"; x: 3; width: 41; height: 10 }
        Item {
          objectName: "b"
          width: 5; height: 5
          anchors.left: s.right
          anchors.leftMargin: 2
        }
      }
      """
    Then the property "x" of "b" is 46
    When the property "x" of "s" is set to 13
    Then the property "x" of "b" is 56

  Scenario: left and horizontalCenter together set the width
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Item {
          objectName: "lc"
          height: 10
          anchors.left: parent.left
          anchors.horizontalCenter: parent.horizontalCenter
        }
      }
      """
    Then the property "width" of "lc" is 200

  Scenario: left, right and horizontalCenter together warn and keep left and right
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Item {
          objectName: "k"
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.horizontalCenter: parent.horizontalCenter
        }
      }
      """
    Then the log has a warning matching "Cannot specify left, right, and horizontalCenter anchors at the same time"
    And the property "width" of "k" is 200

  Scenario: Anchoring to an item that is not a parent or sibling warns
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        width: 200; height: 100
        Item { Item { id: inner; width: 10 } }
        Item { objectName: "b"; anchors.left: inner.right }
      }
      """
    Then the log has a warning matching "Cannot anchor to an item that isn't a parent or sibling"

  Rule: A side margin that was never set is anchors.margins

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          width: 100; height: 100
          Item {
            objectName: "c"
            anchors.fill: parent
            anchors.margins: 7
            anchors.leftMargin: 3
          }
          Item { objectName: "plain"; anchors.fill: parent }
        }
        """

    Scenario Outline: Reading a side margin gives its own value or anchors.margins
      Then the property "anchors.<side>" of "c" is <value>

      Examples:
        | side         | value |
        | leftMargin   | 3     |
        | rightMargin  | 7     |
        | topMargin    | 7     |
        | bottomMargin | 7     |

    Scenario: Unset side margins follow a new anchors.margins
      When the property "anchors.margins" of "c" is set to 9
      Then the property "anchors.rightMargin" of "c" is 9
      And the property "anchors.leftMargin" of "c" is 3
      And the property "width" of "c" is 88

    Scenario: A side margin that was set stops following
      When the property "anchors.topMargin" of "c" is set to 2
      And the property "anchors.margins" of "c" is set to 11
      Then the property "anchors.topMargin" of "c" is 2
      And the property "anchors.bottomMargin" of "c" is 11
      And the property "y" of "c" is 2

    Scenario: Setting anchors.margins for the first time reaches every side
      Then the property "anchors.leftMargin" of "plain" is 0
      When the property "anchors.margins" of "plain" is set to 5
      Then the property "anchors.leftMargin" of "plain" is 5
      And the property "x" of "plain" is 5
      And the property "height" of "plain" is 90
