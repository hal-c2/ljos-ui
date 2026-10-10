# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#activeFocusOnTab-prop
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html#tabPressed-signal
#   https://doc.qt.io/qt-6/qml-qtquick-shortcut.html
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Tab among the other key handlers
  Tab moves focus only if nothing else wanted the key: a Shortcut and the
  focus item's Keys handlers come first.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string active:
          Window.activeFocusItem ? Window.activeFocusItem.objectName : ""
        property bool take: false
        property string log: ""
        Keys.onTabPressed: (event) => { log += "root "; event.accepted = take }
        Item {
          objectName: "one"; activeFocusOnTab: true; focus: true
          property bool take: false
          Keys.onTabPressed: (event) => { root.log += "one "; event.accepted = take }
        }
        Item { objectName: "plain" }
        Item {
          objectName: "two"; activeFocusOnTab: true
          Keys.onReleased: (event) => root.log += "two-up "
        }
        Shortcut {
          objectName: "tabKey"; sequence: "Tab"; enabled: false
          onActivated: root.log += "shortcut "
        }
      }
      """

  Scenario: Tab moves focus before the parents of the focus item see the key
    When the user presses "Tab"
    Then the property "active" of "root" is "two"
    And the property "log" of "root" is "one two-up "

  Scenario: A Keys handler that accepts Tab keeps focus where it is
    When the property "take" of "one" is set to true
    And the user presses "Tab"
    Then the property "active" of "root" is "one"
    And the property "log" of "root" is "one "

  Scenario: From an item outside the tab order the parents see Tab first
    When the property "focus" of "plain" is set to true
    And the user presses "Tab"
    Then the property "log" of "root" is "root "
    And the property "active" of "root" is "one"

  Scenario: A parent that accepts Tab stops it moving focus
    When the property "focus" of "plain" is set to true
    And the property "take" of "root" is set to true
    And the user presses "Tab"
    Then the property "active" of "root" is "plain"

  Scenario: A Shortcut on Tab takes the key
    When the property "enabled" of "tabKey" is set to true
    And the user presses "Tab"
    Then the property "log" of "root" is "shortcut "
    And the property "active" of "root" is "one"
