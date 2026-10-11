# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-parentanimation.html
#   https://doc.qt.io/qt-6/qml-qtquick-parentchange.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: ParentAnimation
  A ParentAnimation in a transition gives an item its new parent at once
  and moves it from where it was on the screen to its place in that
  parent.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"; width: 200; height: 200
        property string under: inner.parent === away ? "away" : "holder"
        property alias moving: move.running
        Item { id: holder; x: 50; y: 60; width: 100; height: 100
          Rectangle { id: inner; objectName: "inner"; x: 10; y: 10; width: 20; height: 20 } }
        Item { id: away; x: 100; y: 20; width: 50; height: 50 }
        states: State { name: "moved"
          ParentChange { target: inner; parent: away; x: 0; y: 0 } }
        transitions: Transition {
          id: move
          ParentAnimation { NumberAnimation { properties: "x,y"; duration: 2000 } }
        }
      }
      """

  @api-QtQuick.ParentAnimation.animations
  Scenario: The item has its new parent at once
    When the property "state" of "root" is set to "moved"
    Then the property "under" of "root" is "away"

  Scenario: The item starts where it was on the screen
    When the property "state" of "root" is set to "moved"
    Then the property "x" of "inner" is -40
    And the property "y" of "inner" is 50

  Scenario: The item is on its way while the transition runs
    When the property "state" of "root" is set to "moved"
    And 1 second passes
    Then the property "x" of "inner" is at least -30
    And the property "x" of "inner" is at most -10
    And the property "y" of "inner" is at least 12
    And the property "y" of "inner" is at most 38

  Scenario: The item arrives at its place in the new parent
    When the property "state" of "root" is set to "moved"
    And 3 seconds pass
    Then the property "x" of "inner" is 0
    And the property "y" of "inner" is 0

  Scenario: The transition is running while the item moves
    When the property "state" of "root" is set to "moved"
    Then the property "moving" of "root" is true

  Scenario: Going back the item is put back at once
    When the property "state" of "root" is set to "moved"
    And 3 seconds pass
    And the property "state" of "root" is set to ""
    Then the property "under" of "root" is "holder"
    And the property "x" of "inner" is 10
    And the property "y" of "inner" is 10
