# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-keys.html
#   https://doc.qt.io/qt-6/qtquick-input-focus.html#key-handling-overview
#   https://doc.qt.io/qt-6/qml-qtquick-keyevent.html#accepted-prop
# Verified on Qt 6.11.2 (qml6, offscreen; key input over its vnc platform).

@qt-parity
Feature: Keys
  A key press goes to the item with active focus and then to each of its
  parents in turn, until a handler accepts it. A release takes the same road.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        Keys.onPressed: (event) => log += "root:" + event.key + " "
        Keys.onReleased: (event) => log += "up "
        Item {
          objectName: "outer"
          Keys.onPressed: (event) => root.log += "outer "
          Item {
            id: inner
            objectName: "inner"; focus: true
            property bool take: false
            Keys.onPressed: (event) => { root.log += "inner "; event.accepted = take }
            Keys.onReturnPressed: root.log += "return "
            Keys.onSpacePressed: (event) => { root.log += "space "; event.accepted = false }
          }
        }
        function mute() { inner.Keys.enabled = false }
      }
      """

  Scenario: A press goes to the focus item and on to its parents
    When the user presses "A"
    Then the property "log" of "root" is "inner outer root:65 up "

  Scenario: An accepted press goes no further
    When the property "take" of "inner" is set to true
    And the user presses "A"
    Then the property "log" of "root" is "inner up "

  Scenario: A handler for one key runs before onPressed and accepts the press
    When the user presses "Return"
    Then the property "log" of "root" is "return up "

  Scenario: A handler for one key can pass the press on
    When the user presses "Space"
    Then the property "log" of "root" is "space inner outer root:32 up "

  Scenario: With Keys.enabled false an item's handlers are passed over
    When the method "mute" of "root" is called
    And the user presses "Return"
    Then the property "log" of "root" is "outer root:16777220 up "

  Scenario: A parent of the focus item is not asked when it is not on the way
    When the property "focus" of "outer" is set to true
    And the user presses "A"
    Then the property "log" of "root" is "outer root:65 up "

  Scenario: Without a focus item a press reaches no handler
    When the property "focus" of "inner" is set to false
    And the user presses "A"
    Then the property "log" of "root" is ""
