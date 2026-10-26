# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyaction.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seed 202: bucket value:onChanged:text (see
# crates/ljos_fuzz/known/states.txt).

@qt-parity
Feature: A state's binding under a transition
  A PropertyChanges that binds a property, under a transition, is told at the state's value,
  then at the value the transition starts from, and the binding is in force once the
  transition reaches it. A value that is no binding is held from the start instead.

  Scenario: A binding a PropertyAction sets is told at its value, at its old one, then at its value
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property real p0
        states: [State { name: "s0"; PropertyChanges { target: root; p0: (-100) } }]
        transitions: [Transition { to: "s0"; PropertyAction { target: root; property: "p0" } }]
        onP0Changed: log += "p0=" + p0 + ";"
      }
      """
    When the property "state" of "root" is set to "s0"
    Then the property "log" of "root" is "p0=-100;p0=0;p0=-100;"

  Scenario: A value a PropertyAction sets is told once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property real p0
        states: [State { name: "s0"; PropertyChanges { target: root; p0: -100 } }]
        transitions: [Transition { to: "s0"; PropertyAction { target: root; property: "p0" } }]
        onP0Changed: log += "p0=" + p0 + ";"
      }
      """
    When the property "state" of "root" is set to "s0"
    Then the property "log" of "root" is "p0=-100;"

  Scenario: A binding the transition does not name waits for the transition's end
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property real k: -100
        property real p0
        property real q
        states: [State { name: "s0"; PropertyChanges { target: root; p0: root.k; q: 1 } }]
        transitions: [Transition {
          to: "s0"; NumberAnimation { target: root; property: "q"; duration: 50 }
        }]
        onP0Changed: log += "p0=" + p0 + ";"
      }
      """
    When the property "state" of "root" is set to "s0"
    Then the property "log" of "root" is "p0=-100;p0=0;"
    When 100 milliseconds pass
    Then the property "log" of "root" is "p0=-100;p0=0;p0=-100;"

  Scenario: A state that binds a property shows its plain values, takes them back, then shows them
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property real k: -100
        property real p0
        property real q
        states: [State { name: "s0"; PropertyChanges { target: root; p0: root.k; q: 5 } }]
        transitions: [Transition {
          to: "s0"; NumberAnimation { target: root; property: "p0"; duration: 50 }
        }]
        onQChanged: log += "q=" + q + ";"
      }
      """
    When the property "state" of "root" is set to "s0"
    Then the property "log" of "root" is "q=5;q=0;q=5;"
