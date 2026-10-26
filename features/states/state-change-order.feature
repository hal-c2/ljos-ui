# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-state.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#state-prop
#   https://doc.qt.io/qt-6/qml-qtquick-transition.html
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seeds 101 and 202: buckets value:onChanged:text,
# onChanged:empty-ljos, value:load:empty-ljos, value:load:number, value:step:color,
# binding-loop(ljos-only), warning:assign(ljos-only) and the restoreEntryValues ones (see
# crates/ljos_fuzz/known/states.txt); after B26, value:step:invalid-qt, a colour given a list.
# Backlog: a state's changes are made in one batch and the handlers they set off run after it,
# where qml6 runs each as its change is made; that is the graph's batching (bindings
# infrastructure). Seed 303, connections bucket value:step:bool-vs-record.

@qt-parity
Feature: The order of a state change
  An item's state given where it is made is told as a change, and the state's scripts run
  after; a state a `when` picks where the item is made runs the transitions to it, and one
  given by name does not. A state no state has is still a change transitions run for. A
  PropertyChanges that reads what it changes reads it once, and a PropertyAction reads its value
  before the changes are made.

  Scenario: A state given where the item is made is told before its script runs
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        states: [State { name: "s0"; StateChangeScript { script: root.log += "scs;" } }]
        state: "s0"
        onStateChanged: log += "st=" + state + ";"
      }
      """
    Then the property "log" of "root" is "st=s0;scs;"

  Scenario: A state given where the item is made is told when no state has its name
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        state: "a"
        onStateChanged: log += "st=" + state + ";"
      }
      """
    Then the property "log" of "root" is "st=a;"

  Scenario: A state a when picks where the item is made is told before its script runs
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        states: [State { name: "s0"; when: true; StateChangeScript { script: root.log += "scs;" } }]
        onStateChanged: log += "st=" + state + ";"
      }
      """
    Then the property "log" of "root" is "st=s0;scs;"

  Scenario: A state a when picks where the item is made runs the transition to it
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property string p
        property string q: "x"
        states: [State { name: "s0"; when: true }]
        transitions: [Transition {
          ScriptAction { script: root.log += "sa;" }
          PropertyAction { target: root; property: "p"; value: root.q }
        }]
        property string seen: p + "|" + log
      }
      """
    Then the property "seen" of "root" is "x|sa;"

  Scenario: A state's own scripts run once the transition to it has started
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log
        property bool go
        states: [State { name: "s0"; when: root.go
          StateChangeScript { script: root.log += "scs;" } }]
        transitions: [Transition { SequentialAnimation {
          ScriptAction { script: root.log += "a;" }
          PauseAnimation { duration: 50 }
          ScriptAction { script: root.log += "b;" } } }]
        function run() { go = true; log += "|" }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "a;scs;|"

  Scenario: A state given by name where the item is made runs no transition
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property string p
        states: [State { name: "s0" }]
        state: "s0"
        transitions: [Transition {
          ScriptAction { script: root.log += "sa;" }
          PropertyAction { target: root; property: "p"; value: "x" }
        }]
        property string seen: p + "|" + log
      }
      """
    Then the property "seen" of "root" is "|"

  Scenario: A transition runs to a state no state has
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int p
        transitions: [Transition { from: "*"; to: "*"
          ScriptAction { script: root.log += "sa;" }
          PropertyAction { target: root; property: "p"; value: 5 }
        }]
        property string seen
        function run() { state = "nope"; seen = p + "|" + log }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "5|sa;"

  Scenario: A state a when picks extends one whose when is false
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property color p
        states: [
          State { name: "s0"; when: false; PropertyChanges { target: root; p: "#336699" } },
          State { name: "s1"; when: true; extend: "s0"; PropertyChanges { target: root; p: "#ff0000" } }
        ]
        transitions: [Transition { to: "*"; ScriptAction { script: console.log("t") } }]
        property string seen: state + "|" + p
      }
      """
    Then the property "seen" of "root" is "s1|#ff0000"

  Scenario: A state whose when is false is still entered by name
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int p
        states: [State { name: "s0"; when: false; PropertyChanges { target: root; p: 27 } }]
        property string seen
        function run() { state = "s0"; seen = state + "|" + p }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "s0|27"

  Scenario: A state given where the item is made whose when is false is left before it is entered
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int v
        states: [State {
          name: "s0"; when: false
          PropertyChanges { root.v: 5 }
          StateChangeScript { script: root.log += "scs;" }
        }]
        state: "s0"
        onStateChanged: log += "st=" + state + ";"
        property string seen: log + "|" + state + "|" + v
      }
      """
    Then the property "seen" of "root" is "st=;||0"

  Scenario: A change that reads the property it changes reads it once, and is warned of a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real p: 2
        states: [State { name: "s0"; PropertyChanges { target: root; p: root.p + 1 } }]
        property string seen
        function run() { state = "s0"; seen = "" + p }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "3"
    And the log has a warning matching "Binding loop detected for property .p."

  Scenario: A change in an extending state reads what the state it extends gave the property
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int p
        property int q
        states: [State { name: "s0"; PropertyChanges { root.p: 5; root.q: 2 * 3 } },
          State { name: "s1"; extend: "s0"; PropertyChanges { root.p: root.p + 1; root.q: root.q + 1 } }]
        property var seen: [p, q]
        function run() { state = "s1" }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is [6, 7]
    And the log has a warning matching "Binding loop detected for property .q."

  Scenario: A change of a colour to a bool makes it invalid, and one to no number is warned of
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property color c: "#00ff00"
        property real r: 4
        property int i: 4
        states: [State { name: "s0"; PropertyChanges { target: root; c: (1 == 2); r: undefined; i: [] } }]
        property string seen
        function run() { state = "s0"; seen = c + " " + c.valid + " " + r + " " + i }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "#000000 false 4 4"
    And the log has 2 warnings matching "Unable to assign"

  Scenario Outline: A change of a colour to <what> makes it invalid, unwarned
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property color c: Qt.rgba(0, 0, 1, 1)
        states: [State { name: "s0"; PropertyChanges { target: root; c: <value> } }]
      }
      """
    When the property "state" of "root" is set to "s0"
    Then the property "c" of "root" is "#000000"
    And the log has no warnings

    Examples:
      | what              | value                |
      | a worked-out list | "".split(",")        |
      | an empty list     | []                   |
      | an object's keys  | Object.keys({k0: 1}) |
      | an object         | ({a: 1})             |
      | a date            | new Date(0)          |

  Scenario: A change of a colour to a function is refused, and warned of
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property color c: "blue"
        states: [State { name: "s0"; PropertyChanges { target: root; c: (function () { return 1 }) } }]
      }
      """
    When the property "state" of "root" is set to "s0"
    Then the property "c" of "root" is "#0000ff"
    And the log has 1 warning matching "Scenario.qml:5:70: Unable to assign a function to a property of any type other than var."

  Scenario: A change an extended state keeps is restored when the default state is set
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string p: "-0"
        states: [
          State { name: "s0"; when: false },
          State { name: "s1"; extend: "s0"
            PropertyChanges { target: root; p: "red"; restoreEntryValues: false } },
          State { name: "s2"; when: true; extend: "s1"; PropertyChanges { target: root; p: "hello" } }
        ]
        property string seen
        function run() { state = ""; seen = state + "|" + p }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "|-0"

  Scenario: A PropertyAction value that would throw is not read while its transition never runs
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real p
        transitions: [Transition { from: "s0"
          PropertyAction { target: root; property: "p"; value: ("ab").toFixed(2) } }]
      }
      """
    Then the log has no warnings

  Scenario: A PropertyAction value is read before the state's changes are made
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string p: "old"
        states: [State { name: "s0"; PropertyChanges { target: root; p: "new" } }]
        transitions: [Transition { to: "s0"
          PropertyAction { target: root; property: "p"; value: root.p } }]
        property string seen
        function run() { state = "s0"; seen = p }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is "old"

  Scenario: A state's binding has the last word over a PropertyAction, and its value does not
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int a
        property int b
        states: [State { name: "s0"; PropertyChanges { root.a: Math.min(4, 9); root.b: 4 } }]
        transitions: [Transition { to: "s0"
          PropertyAction { targets: [root]; properties: "a,b"; value: 97 } }]
        property var seen: [a, b]
        function run() { state = "s0" }
      }
      """
    When the method "run" of "root" is called
    Then the property "seen" of "root" is [4, 97]

  Scenario: Two changes that read each other through a binding are no loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real a
        property int b: a + 1
        states: [State { name: "s0"
          PropertyChanges { target: root; a: root.b * 2 }
          PropertyChanges { target: root; b: 5 } }]
        function run() { state = "s0" }
      }
      """
    When the method "run" of "root" is called
    Then the log has no warnings

  Scenario: A change that reads a binding of the property it changes is warned of a loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real a: 1
        property real b: a
        states: [State { name: "s0"; PropertyChanges { target: root; a: root.b - 1 } }]
        function run() { state = "s0" }
      }
      """
    When the method "run" of "root" is called
    Then the log has a warning matching "Binding loop detected for property .a."

  @backlog
  Scenario Outline: A handler a state's change sets off runs as the change is made
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int p0
        property var p1
        states: [State { name: "s0"; PropertyChanges { target: root; p1: <v> } },
                 State { name: "s1"; extend: "s0"; PropertyChanges { root.p0: 42 } }]
        onP0Changed: { log += "p0;"; root.p1 = ({k0: 1}); }
        onP1Changed: log += "p1=" + JSON.stringify(p1).replace(/"/g, "") + ";"
      }
      """
    When the property "state" of "root" is set to "s1"
    Then the property "log" of "root" is "<log>"

    Examples:
      | v               | log                    |
      | (true && false) | p0;p1={k0:1};p1=false; |
      | false           | p1=false;p0;p1={k0:1}; |
