# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertyaction.html#value-prop
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seed 101: bucket value:load:number (see
# crates/ljos_fuzz/known/states.txt).

@qt-parity
Feature: A transition's PropertyAction given undefined
  A PropertyAction without a value sets what the state gives the property. One given `undefined`
  sets the property to what its type has before anything is assigned; a value its property cannot
  take leaves the property as it is.

  Scenario Outline: A <type> given undefined by a PropertyAction is <p2>
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property <type> p2: <init>
        states: [State { name: "s2"; <when> }]
        transitions: [Transition { PropertyAction { target: root; property: "p2"; value: <v> } }]
        function run() { state = "s2" }
      }
      """
    When the method "run" of "root" is called
    And 50 milliseconds pass
    Then the property "p2" of "root" is <p2>
    And the log has no warnings

    Examples:
      | type   | init | when        | v             | p2 |
      | int    | 16   | when: true; | undefined     | 0  |
      | int    | 16   |             | undefined     | 0  |
      | int    | 16   |             | "steelblue".a | 0  |
      | int    | 16   |             | "x"           | 16 |
      | real   | 1.5  |             | undefined     | 0  |
      | string | "a"  |             | undefined     | "" |
