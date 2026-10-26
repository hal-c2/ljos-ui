# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-state.html#when-prop
#   https://doc.qt.io/qt-6/qtquick-statesanimations-states.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seeds 101 and 202: buckets value:step:invalid-ljos and
# value:step:text (see crates/ljos_fuzz/known/states.txt).

@qt-parity
Feature: A when while a state is applied
  A `when` that the changes of the state being entered make pick another state is refused: the
  item stays in the state it entered, and is warned. A `when` that is worked out again picks its
  state again, though it was true before. With a transition, the changes land when it starts, and
a `when` they make false leaves the state.

  Scenario: A state a when picks where the item is made, whose change makes it false, is kept
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int p: 0
        states: [State { name: "s0"; when: root.p == 0; PropertyChanges { target: root; p: 1 } }]
        onStateChanged: log += "st=" + state + ";"
        onPChanged: log += "p=" + p + ";"
      }
      """
    Then the property "log" of "root" is "st=s0;p=1;"
    And the log has a warning matching "QML StateGroup: Can't apply a state change as part of a state definition"

  Scenario: A state a when picks, whose change makes it false, is kept, and is a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int a: 0
        property int p: 0
        states: [State {
          name: "s0"; when: root.a == 1 && root.p == 0; PropertyChanges { target: root; p: 1 }
        }]
        onStateChanged: log += "st=" + state + ";"
        onPChanged: log += "p=" + p + ";"
        function run() { log += "|"; a = 1; log += "|" }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "|st=s0;p=1;|"
    And the log has a warning matching "QML State: Binding loop detected for property .when."

  Scenario: A state given by name whose change makes another's when true is kept
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int a: 0
        states: [State { name: "s0"; PropertyChanges { target: root; a: 1 } },
                 State { name: "s1"; when: root.a == 1 }]
        onStateChanged: log += "st=" + state + ";"
        function run() { log += "|"; state = "s0"; log += "|" }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "|st=s0;|"
    And the log has a warning matching "QML StateGroup: Can't apply a state change as part of a state definition"

  Scenario: A when worked out again leaves a state given by name for its own
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int a: 0
        states: [State { name: "s0" }, State { name: "s1"; when: root.a >= 0 }]
        onStateChanged: log += "st=" + state + ";"
        function run() { log += "|"; state = "s0"; log += "|"; a = 1; log += "|" }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "st=s1;|st=s0;|st=s1;|"

  Scenario: A state given by name, kept while its when is worked out again, is left after it
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int p1
        property int k
        states: [State {
          name: "s0"; when: isNaN(root.p1 + root.k); PropertyChanges { target: root; p1: 27 }
        }]
        onStateChanged: log += "st=" + state + ";"
        function run() { log += "|"; state = "s0"; log += "|"; k = 1; log += "|" }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "|st=s0;|st=;|"
    And the property "p1" of "root" is 0
    And the log has a warning matching "QML StateGroup: Can't apply a state change as part of a state definition"
    And the log has a warning matching "QML State: Binding loop detected for property .when."

  Scenario: Under a transition, a state given by name is left at once for its false when
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int p1
        property int k
        states: [State {
          name: "s0"; when: isNaN(root.p1 + root.k); PropertyChanges { target: root; p1: 27 }
        }]
        transitions: [Transition { to: "*"; ScriptAction { script: root.log += "t;" } }]
        onStateChanged: log += "st=" + state + ";"
        function run() { log += "|"; state = "s0"; log += "|"; k = 1; log += "|" }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "|st=s0;t;st=;t;||"
    And the property "p1" of "root" is 0
    And the log has no warning matching "Can't apply"
    And the log has a warning matching "QML State: Binding loop detected for property .when."
