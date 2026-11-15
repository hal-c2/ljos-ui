# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
#   https://doc.qt.io/qt-6/qtquick-statesanimations-animations.html
# Verified on Qt 6.12 (qml6, offscreen): back from 51.2 at 530 ms, the
# property read 38.9 at 770 ms, where it would be 37.5 at exact times.

@qt-parity
Feature: A transition of a property nobody watches
  A transition of a property that nothing binds to, listens to or draws
  needs no frames on its way. A change of state that ends it starts the
  next one from where the property got to, as when something watches it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real p: 0
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
