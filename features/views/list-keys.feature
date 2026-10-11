# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-listview.html (keyNavigationEnabled, keyNavigationWraps, "Keyboard navigation")
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity @api-QtQuick.ListView.keyNavigationEnabled
Feature: Moving the current row of a ListView with the keyboard
  A view with the keyboard focus moves its current row with the arrow keys along its orientation.

  Background:
    Given the QML:
      """qml
      import QtQuick
      ListView {
        objectName: "list"
        width: 200; height: 300
        focus: true
        model: 5
        property string seen: ""
        property bool mine: false
        Keys.onPressed: (event) => { seen += "k"; event.accepted = mine }
        delegate: Item { objectName: "r" + index; width: 40; height: 20 }
      }
      """

  Scenario: Down and Up move the current row of a vertical list
    When the user presses "Down"
    And the user presses "Down"
    Then the property "currentIndex" of "list" is 2
    When the user presses "Up"
    Then the property "currentIndex" of "list" is 1

  Scenario: Left and Right do nothing in a vertical list
    When the user presses "Right"
    And the user presses "Left"
    Then the property "currentIndex" of "list" is 0

  Scenario: Right and Left move the current row of a horizontal list
    When the property "orientation" of "list" is set to 1
    And the user presses "Right"
    And the user presses "Right"
    And the user presses "Left"
    Then the property "currentIndex" of "list" is 1

  Scenario: In a list from right to left the Left key goes on
    When the property "orientation" of "list" is set to 1
    And the property "layoutDirection" of "list" is set to 1
    And the user presses "Left"
    Then the property "currentIndex" of "list" is 1

  Scenario: In a list from bottom to top the Up key goes on
    When the property "verticalLayoutDirection" of "list" is set to 3
    And the user presses "Up"
    Then the property "currentIndex" of "list" is 1

  Scenario: The keys stop at the ends
    When the user presses "Up"
    Then the property "currentIndex" of "list" is 0
    When the property "currentIndex" of "list" is set to 4
    And the user presses "Down"
    Then the property "currentIndex" of "list" is 4

  Scenario: The keys wrap around when key navigation wraps
    When the property "keyNavigationWraps" of "list" is set to true
    And the user presses "Up"
    Then the property "currentIndex" of "list" is 4
    When the user presses "Down"
    Then the property "currentIndex" of "list" is 0

  Scenario: The view's own Keys handler sees a key before the view uses it
    When the user presses "Down"
    Then the property "seen" of "list" is "k"
    And the property "currentIndex" of "list" is 1

  Scenario: A key the Keys handler accepted does not move the current row
    When the property "mine" of "list" is set to true
    And the user presses "Down"
    Then the property "seen" of "list" is "k"
    And the property "currentIndex" of "list" is 0

  Scenario: With key navigation off the keys do nothing
    When the property "keyNavigationEnabled" of "list" is set to false
    And the user presses "Down"
    Then the property "currentIndex" of "list" is 0
