# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-anchoranimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-anchorchanges.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: AnchorAnimation
  An AnchorAnimation in a transition moves an item from where its old
  anchors put it to where the state's anchors put it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        component Box: Rectangle { width: 10; height: 10; anchors.left: parent.left }
        Box { id: box; objectName: "box"; anchors.top: parent.top }
        Box { id: still; objectName: "still" }
        states: State { name: "corner"
          AnchorChanges { target: box
            anchors.left: undefined; anchors.right: root.right
            anchors.top: undefined; anchors.bottom: root.bottom }
          AnchorChanges { target: still
            anchors.left: undefined; anchors.right: root.right } }
        transitions: Transition {
          AnchorAnimation { targets: [box]; duration: 2000 }
        }
      }
      """

  @api-QtQuick.AnchorAnimation.targets
  Scenario: The item starts where its old anchors put it
    When the property "state" of "root" is set to "corner"
    Then the property "x" of "box" is 0
    And the property "y" of "box" is 0

  @api-QtQuick.AnchorAnimation.duration
  Scenario Outline: The item is on its way while the transition runs: <property>
    When the property "state" of "root" is set to "corner"
    And 1 second passes
    Then the property "<property>" of "box" is at least 45
    And the property "<property>" of "box" is at most 145

    Examples:
      | property |
      | x        |
      | y        |

  Scenario: The item arrives where the state's anchors put it
    When the property "state" of "root" is set to "corner"
    And 3 seconds pass
    Then the property "x" of "box" is 190
    And the property "y" of "box" is 190

  Scenario: An item the animation does not name stays until the transition is over
    When the property "state" of "root" is set to "corner"
    And 1 second passes
    Then the property "x" of "still" is 0

  Scenario: An item the animation does not name is moved when the transition is over
    When the property "state" of "root" is set to "corner"
    And 3 seconds pass
    Then the property "x" of "still" is 190

  Scenario: The way back is animated as well
    When the property "state" of "root" is set to "corner"
    And 3 seconds pass
    And the property "state" of "root" is set to ""
    And 1 second passes
    Then the property "x" of "box" is at least 45
    And the property "x" of "box" is at most 145

  Scenario: The way back ends where the item's own anchors put it
    When the property "state" of "root" is set to "corner"
    And 3 seconds pass
    And the property "state" of "root" is set to ""
    And 3 seconds pass
    Then the property "x" of "box" is 0
    And the property "y" of "box" is 0
    And the property "x" of "still" is 0
