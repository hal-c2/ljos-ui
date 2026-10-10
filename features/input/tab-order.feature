# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#activeFocusOnTab-prop
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#nextItemInFocusChain-method
#   https://doc.qt.io/qt-6/qtquick-input-focus.html
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Tab order
  Tab moves active focus to the next item that has activeFocusOnTab, in the
  order the items are written, and Shift+Tab to the one before. Items that
  are hidden or disabled are passed over.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        property string seen: ""
        function look(name) {
          var from = name === "plain" ? plain : one
          var next = from.nextItemInFocusChain(true)
          var previous = from.nextItemInFocusChain(false)
          seen = next.objectName + " " + previous.objectName
        }
        Item { id: one; objectName: "one"; activeFocusOnTab: true; focus: true }
        Item { id: plain; objectName: "plain" }
        Item {
          objectName: "group"
          Item { objectName: "two"; activeFocusOnTab: true }
        }
        Item { objectName: "three"; activeFocusOnTab: true }
      }
      """

  Scenario: Tab goes to the next item, into nested items
    When the user presses "Tab"
    Then the property "active" of "root" is "two"
    And the property "focus" of "one" is false

  Scenario: Tab skips items without activeFocusOnTab
    When the user presses "Tab"
    And the user presses "Tab"
    Then the property "active" of "root" is "three"

  Scenario: Tab wraps from the last item to the first
    When the property "focus" of "three" is set to true
    And the user presses "Tab"
    Then the property "active" of "root" is "one"

  Scenario: Shift+Tab goes backwards and wraps to the last item
    When the user presses "Shift+Tab"
    Then the property "active" of "root" is "three"
    When the user presses "Shift+Tab"
    Then the property "active" of "root" is "two"

  Scenario: Tab skips a hidden item
    When the property "visible" of "group" is set to false
    And the user presses "Tab"
    Then the property "active" of "root" is "three"

  Scenario: Tab skips a disabled item
    When the property "enabled" of "two" is set to false
    And the user presses "Tab"
    Then the property "active" of "root" is "three"

  Scenario: An item joins the tab order when activeFocusOnTab is set
    When the property "activeFocusOnTab" of "plain" is set to true
    And the user presses "Tab"
    Then the property "active" of "root" is "plain"

  Scenario: An item leaves the tab order when activeFocusOnTab is cleared
    When the user presses "Tab"
    And the property "activeFocusOnTab" of "three" is set to false
    And the user presses "Tab"
    Then the property "active" of "root" is "one"

  Scenario: With no focus item, Tab goes to the first item
    When the property "focus" of "one" is set to false
    And the user presses "Tab"
    Then the property "active" of "root" is "one"

  Scenario: With no focus item, Shift+Tab goes to the last item
    When the property "focus" of "one" is set to false
    And the user presses "Shift+Tab"
    Then the property "active" of "root" is "three"

  Scenario: From an item outside the tab order, Tab starts at the first item
    When the property "focus" of "plain" is set to true
    And the user presses "Tab"
    Then the property "active" of "root" is "one"

  Scenario: Tab does nothing when the only item in the order has focus
    When the property "activeFocusOnTab" of "two" is set to false
    And the property "activeFocusOnTab" of "three" is set to false
    And the user presses "Tab"
    Then the property "active" of "root" is "one"

  Scenario: nextItemInFocusChain names the next and the previous item
    When the method "look" of "root" is called with "one"
    Then the property "seen" of "root" is "two three"

  Scenario: nextItemInFocusChain works from an item outside the tab order
    When the method "look" of "root" is called with "plain"
    Then the property "seen" of "root" is "two one"
