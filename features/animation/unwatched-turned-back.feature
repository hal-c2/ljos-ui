# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
#   https://doc.qt.io/qt-6/qml-qtquick-behavior.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# Verified on Qt 6.12 (qml6, offscreen). Transition: back from 51.2 at
# 530 ms, 38.9 at 770 ms. Behavior: back from 51.3 at 529 ms, 39.0 at
# 769 ms. Both are 37.5 at exact times.

@qt-parity
Feature: An animation nobody watches turned back on its way
  An animation of a property that nothing binds to, listens to or draws is
  worked out only when the property is read. A change that turns it back,
  a change of state or a value given through a Behavior, starts the next
  animation from where the property got to, as when something watches it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real p: 0
        property real q: 0
        Behavior on q { NumberAnimation { duration: 1000 } }
        states: State { name: "b"; PropertyChanges { target: root; p: 100 } }
        transitions: Transition {
          NumberAnimation { properties: "p"; duration: 1000 }
        }
      }
      """

  Scenario: A transition turned back on its way starts from where the property got to
    When the property "state" of "root" is set to "b"
    And 500 milliseconds pass
    And the property "state" of "root" is set to ""
    And 250 milliseconds pass
    Then the property "p" of "root" is within 1 of 37.5

  Scenario: A Behavior turned back on its way starts from where the property got to
    When the property "q" of "root" is set to 100
    And 500 milliseconds pass
    And the property "q" of "root" is set to 0
    And 250 milliseconds pass
    Then the property "q" of "root" is within 1 of 37.5
