# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertyaction.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html#when-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seeds 101 and 202: buckets warning:other(qt-only) "Can't apply"
# and warning:binding-loop(ljos-only):when (see crates/ljos_fuzz/known/states.txt).
# Backlog: Ljós puts a state's changes in place and holds them at their start in one batch, and
# its graph tells a property set and set back in a batch as changed. Running a transition's
# first actions outside that batch, as qml6 does, would tell each held change twice; it waits
# for the graph to leave out a change that came to nothing (bindings infrastructure).

@qt-parity
Feature: The actions at a transition's start
  A transition's actions at its start run before the state's other changes are in place, and
  each is told as it is done. They are part of the state change: a `when` they turn is refused.

  @backlog
  Scenario: The first actions run, and are told, before the other changes are in place
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int p1
        property int q
        states: [State { name: "s0"; PropertyChanges { target: root; p1: 27; q: 5 } }]
        transitions: [Transition {
          to: "s0"
          SequentialAnimation {
            ScriptAction { script: root.log += "t1 p1=" + root.p1 + " q=" + root.q + ";" }
            PropertyAction { target: root; property: "q" }
            ScriptAction { script: root.log += "t2 p1=" + root.p1 + " q=" + root.q + ";" }
            PauseAnimation { duration: 50 }
            ScriptAction { script: root.log += "t3;" }
          }
        }]
        onStateChanged: log += "st=" + state + ";"
        onP1Changed: log += "p1=" + p1 + ";"
        onQChanged: log += "q=" + q + ";"
        function run() { log += "|"; state = "s0"; log += "|" }
      }
      """
    When the method "run" of "root" is called
    And 100 milliseconds pass
    Then the property "log" of "root" is "|st=s0;t1 p1=0 q=0;q=5;t2 p1=0 q=5;p1=27;|t3;"

  @backlog
  Scenario: A when that a first action works out again is refused, and the state kept
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property color p0
        states: [State {
          name: "s0"; when: Qt.colorEqual(root.p0, "#00ff00")
          PropertyChanges { target: root; p0: "steelblue" }
        }]
        transitions: [Transition {
          to: "s0"
          SequentialAnimation {
            PropertyAction { target: root; property: "p0"; value: "red" }
            ScriptAction { script: root.log += "t;" }
          }
        }]
        onStateChanged: log += "st=" + state + ";"
        onP0Changed: log += "p0=" + p0 + ";"
        function run() { log += "|"; state = "s0"; log += "|" }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "|st=s0;p0=#ff0000;t;|"
    And the property "state" of "root" is "s0"
    And the log has no warning matching "(?i)binding loop"
    And the log has a warning matching "QML StateGroup: Can't apply a state change as part of a state definition"
