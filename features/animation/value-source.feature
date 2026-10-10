# Sources:
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
#   https://doc.qt.io/qt-6/qml-qtquick-animation.html#running-prop
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#property-modifier-objects
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: An animation declared on a property
  "NumberAnimation on x { }" inside an object animates that object's x and starts by itself.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log
        property alias moving: move.running
        property alias held: later.running
        Rectangle {
          id: box; objectName: "box"
          x: 5; color: "#000000"
          NumberAnimation on x {
            id: move; from: 10; to: 110; duration: 100
            onStarted: log += "started:" + box.x + " "; onFinished: log += "finished "
          }
          NumberAnimation on y { id: later; running: false; from: 40; to: 50; duration: 100 }
          ColorAnimation on color { to: "#0000ff"; duration: 100 }
          NumberAnimation on width { to: 30; duration: 100; loops: 2 }
        }
        Component.onCompleted: log += "completed:" + box.x + " "
      }
      """

  Scenario: The animation is running as soon as the object exists
    Then the property "moving" of "root" is true

  Scenario: It starts before Component.onCompleted, and the property is at from by then
    Then the property "log" of "root" is "started:5 completed:10 "

  Scenario: It runs to its end by itself
    When 1 second passes
    Then the property "x" of "box" is 110
    And the property "log" of "root" is "started:5 completed:10 finished "

  Scenario: running: false keeps it from starting
    When 1 second passes
    Then the property "y" of "box" is 0
    And the property "held" of "root" is false

  Scenario: An animation that was kept from starting can be started later
    When the property "held" of "root" is set to true
    And 1 second passes
    Then the property "y" of "box" is 50

  Scenario: A ColorAnimation on a colour property
    When 1 second passes
    Then the property "color" of "box" is "#0000ff"

  Scenario: Without from, it starts at the value the property was given
    When 1 second passes
    Then the property "width" of "box" is 30
