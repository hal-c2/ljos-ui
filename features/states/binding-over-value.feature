# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html#extend-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seeds 101 and 202: buckets value:step:invalid-qt,
# warning:QPointF_to_int and warning:QML_Case (see crates/ljos_fuzz/known/states.txt).

@qt-parity
Feature: Which change of a property a state keeps
  A state puts its plain values in place first and its bindings after them, so a property it both
  binds and gives a value is bound, whichever change comes first and in whichever of the states it
  extends. A binding that a later binding replaces is still worked out, but a value it cannot
  give is not reported. A state that would not give a property back on leaving takes over giving
  back what the state it replaces would have given back.

  Scenario Outline: A binding and a value in one state: the binding is kept
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property color p2
        property color k: "lime"
        states: [State { name: "s2"; <changes> }]
        function run() { state = "s2"; k = "cyan" }
      }
      """
    When the method "run" of "root" is called
    Then the property "p2" of "root" is "#00ffff"

    Examples:
      | changes                                                                                  |
      | PropertyChanges { target: root; p2: root.k } PropertyChanges { target: root; p2: "red" } |
      | PropertyChanges { target: root; p2: "red" } PropertyChanges { target: root; p2: root.k } |

  Scenario Outline: The extended state's binding is kept over the extending state's value
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property color p2
        property color k: "lime"
        states: [State { name: "s1"; PropertyChanges { target: root; p2: <base> } },
                 State { name: "s2"; extend: "s1"; PropertyChanges { target: root; p2: <own> } }]
        function run() { state = "s2"; k = "cyan" }
      }
      """
    When the method "run" of "root" is called
    Then the property "p2" of "root" is "<p2>"

    Examples:
      | base             | own              | p2      |
      | Qt.rgba(0,0,1,1) | "red"            | #0000ff |
      | root.k           | "red"            | #00ffff |
      | "blue"           | Qt.rgba(1,0,0,1) | #ff0000 |
      | "blue"           | "red"            | #ff0000 |
      | root.k           | root.k           | #00ffff |

  Scenario: A binding that cannot be assigned leaves the value in place, and is reported
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int p1
        states: [State { name: "s0"; PropertyChanges { target: root; p1: Qt.point(1, 2) } },
                 State { name: "s2"; extend: "s0"; PropertyChanges { target: root; p1: 5 } }]
        function run() { state = "s2" }
      }
      """
    When the method "run" of "root" is called
    Then the property "p1" of "root" is 5
    And the log has a warning matching "Unable to assign QPointF to int"

  Scenario Outline: A binding replaced by a later binding is reported, once
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property <type> p1
        property int p0
        states: [State { name: "s0"; PropertyChanges { target: root; p1: <base> } },
                 State { name: "s2"; extend: "s0"; PropertyChanges { target: root; p1: root.p0 + 5 } }]
        function run() { state = "s2"; p0 = 3 }
      }
      """
    When the method "run" of "root" is called
    Then the property "p1" of "root" is 8
    And the log has 1 warning matching "<warning>"

    Examples:
      | type | base                | warning                                |
      | int  | Qt.point(root.p0, 2) | Unable to assign QPointF to int        |
      | real | "#336699".a         | Unable to assign \\[undefined\\] to double |

  Scenario Outline: A binding replaced by a later binding while the item is made is not reported
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property <type> p1
        property int p0
        states: [State { name: "s0"; PropertyChanges { target: root; p1: <base> } },
                 State { name: "s2"; extend: "s0"; <when> PropertyChanges { target: root; p1: root.p0 + 5 } }]
        <completed>
      }
      """
    Then the property "p1" of "root" is 5
    And the log has no warnings

    Examples:
      | type | base           | when        | completed                             |
      | int  | Qt.point(1, 2) | when: true; |                                       |
      | int  | Qt.point(1, 2) |             | Component.onCompleted: state = "s2"   |
      | real | "#336699".a    | when: true; |                                       |
      | real | "#336699".a    |             | Component.onCompleted: state = "s2"   |

  Scenario Outline: A state that does not restore gives back what the state before it would have
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property color p2
        property string log
        states: [State { name: "s2"; PropertyChanges { target: root; p2: "red" } },
                 State { name: "s0"; PropertyChanges { target: root; p2: "blue"; restoreEntryValues: false } }]
        function run() {
          var r = [];
          [<states>].forEach(function (s) { root.state = s; r.push(s + ":" + root.p2) });
          log = r.join(" ")
        }
      }
      """
    When the method "run" of "root" is called
    Then the property "log" of "root" is "<log>"

    Examples:
      | states                | log                                       |
      | "s0", ""              | s0:#0000ff :#0000ff                       |
      | "s2", "s0", ""        | s2:#ff0000 s0:#0000ff :#000000            |
      | "s2", "s0", "s2", ""  | s2:#ff0000 s0:#0000ff s2:#ff0000 :#000000 |
      | "s0", "s2", ""        | s0:#0000ff s2:#ff0000 :#0000ff            |
