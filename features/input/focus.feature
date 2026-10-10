# Sources:
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#focus-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#activeFocus-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#forceActiveFocus-method
#   https://doc.qt.io/qt-6/qml-qtquick-window.html#activeFocusItem-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key and pointer input over its vnc platform).

@qt-parity
Feature: Keyboard focus
  At most one item in a window has active focus: the one key presses go to.
  An item asks for it with focus: true. Nothing but the application moves it:
  not a click, not hiding the item.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 200; height: 100
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        property int changes: 0
        function drop() { second.destroy() }
        Rectangle {
          objectName: "first"; width: 100; height: 100; focus: true
          onActiveFocusChanged: root.changes++
          MouseArea { anchors.fill: parent }
        }
        Rectangle {
          id: second
          objectName: "second"; x: 100; width: 100; height: 100; focus: true
          onActiveFocusChanged: root.changes++
          MouseArea { anchors.fill: parent }
        }
        Item { objectName: "third"; onActiveFocusChanged: root.changes++ }
      }
      """

  Scenario: The first item that asks for focus has it
    Then the property "activeFocus" of "first" is true
    And the property "active" of "root" is "first"
    And the property "focus" of "second" is false
    And the property "activeFocus" of "second" is false

  Scenario: An item that never asked has neither focus nor active focus
    Then the property "focus" of "third" is false
    And the property "activeFocus" of "third" is false
    And the property "activeFocusOnTab" of "third" is false

  Scenario: Setting focus on another item takes it from the first
    When the property "focus" of "second" is set to true
    Then the property "activeFocus" of "second" is true
    And the property "focus" of "first" is false
    And the property "activeFocus" of "first" is false
    And the property "active" of "root" is "second"

  Scenario: Only the items that lose and gain active focus are told
    When the property "changes" of "root" is set to 0
    And the property "focus" of "second" is set to true
    Then the property "changes" of "root" is 2

  Scenario: Giving up focus leaves no item with active focus
    When the property "focus" of "first" is set to false
    Then the property "activeFocus" of "first" is false
    And the property "active" of "root" is ""

  Scenario: forceActiveFocus gives an item focus and active focus
    When the method "forceActiveFocus" of "third" is called
    Then the property "focus" of "third" is true
    And the property "activeFocus" of "third" is true
    And the property "focus" of "first" is false

  Scenario: forceActiveFocus takes a focus reason
    When the method "forceActiveFocus" of "third" is called with 1
    Then the property "active" of "root" is "third"

  Scenario: A click does not move focus
    When the user touches 150, 50
    Then the property "active" of "root" is "first"
    And the property "focus" of "second" is false

  Scenario: Hiding the item with active focus does not take it away
    When the property "visible" of "first" is set to false
    Then the property "focus" of "first" is true
    And the property "activeFocus" of "first" is true
    And the property "active" of "root" is "first"

  Scenario: A disabled item keeps focus but loses active focus
    When the property "enabled" of "first" is set to false
    Then the property "focus" of "first" is true
    And the property "activeFocus" of "first" is false
    And the property "active" of "root" is ""

  Scenario: Enabling the item again gives active focus back
    When the property "enabled" of "first" is set to false
    And the property "enabled" of "first" is set to true
    Then the property "activeFocus" of "first" is true
    And the property "active" of "root" is "first"

  Scenario: Destroying the item with active focus leaves no one with it
    When the property "focus" of "second" is set to true
    And the method "drop" of "root" is called
    And 50 milliseconds pass
    Then the object "second" does not exist
    And the property "active" of "root" is ""
    And the property "activeFocus" of "first" is false
