# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#visible-prop
#   https://doc.qt.io/qt-6/qml-qtqml-timer.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Hidden items stay alive
  Setting visible to false hides an item but does not stop its bindings or timers.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property int base: 1
        Item {
          objectName: "panel"
          property int doubled: root.base * 2
          property int ticks: 0
          visible: false
          Timer { running: true; repeat: true; interval: 50; onTriggered: parent.ticks++ }
        }
        id: root
      }
      """

  Scenario: A hidden item still re-evaluates its bindings
    When the property "base" of "root" is set to 21
    Then the property "doubled" of "panel" is 42

  Scenario: A timer inside a hidden item keeps running
    When 500 milliseconds pass
    Then the property "ticks" of "panel" is at least 3

  Scenario: A hidden item reports itself not visible
    Then the property "visible" of "panel" is false
