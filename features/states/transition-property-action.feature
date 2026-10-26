# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertyaction.html#value-prop
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seeds 101 and 202: bucket value:load:number (see
# crates/ljos_fuzz/known/states.txt).

@qt-parity
Feature: What a transition's PropertyAction sets
  A PropertyAction without a value sets what the state gives the property, even a value the
  property already had. One given `undefined` sets the property to what its type has before
  anything is assigned; a value its property cannot take leaves the property as it is.

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

  Scenario Outline: A PropertyAction without a value sets what the state gives, though nothing changed
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int p0: 3
        states: [State { name: "s2"; PropertyChanges { target: root; p0: 3 } }]
        transitions: [Transition { <actions> }]
        function run() { state = "s2" }
      }
      """
    When the method "run" of "root" is called
    And 50 milliseconds pass
    Then the property "p0" of "root" is 3

    Examples:
      | actions                                                                                                                                                             |
      | PropertyAction { target: root; property: "p0"; value: 255 } PropertyAction { target: root; property: "p0" }                                                         |
      | SequentialAnimation { PropertyAction { target: root; property: "p0"; value: 255 } PauseAnimation { duration: 10 } PropertyAction { target: root; property: "p0" } } |
      | SequentialAnimation { PropertyAction { target: root; property: "p0"; value: 255 } PauseAnimation { duration: 10 } PropertyAction { property: "p0" } }               |
