# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-propertychanges.html
#   https://doc.qt.io/qt-6/qml-qtquick-state.html#extend-prop
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (states), seeds 101 and 202: buckets value:step:invalid-qt,
# warning:QPointF_to_int and warning:QML_Case (see crates/ljos_fuzz/known/states.txt).
# warning:[undefined]_to_double (seed 101) is the item's own binding a state that does not
# restore takes the place of.
# value:step:invalid-qt (seeds 101 and 202) is a StateChangeScript between a state's binding and
# the value of the state that extends it.
# warning:binding-loop(ljos-only):load is a state binding that reads another property the state
# binds, whose own binding reads the first.
# Backlog: Ljós takes the state left off a property before the next state's change is on it,
# and the graph works the item's own binding out again in between; going from one change to the
# next waits on the graph (bindings infrastructure). Buckets warning:QString_to_int,
# warning:[undefined]_to_double (ljos-only) and warning:binding-loop(ljos-only):step-set.

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

  Scenario Outline: A script among the changes sees the values before it and none of the bindings
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string log: ""
        property int a: 1
        property int b: 1
        property int k: 5
        states: [State { name: "s1"; <base> }, State { name: "s2"; extend: "s1"; <own> }]
      }
      """
    When the property "state" of "root" is set to "s2"
    Then the property "log" of "root" is "<log>"
    And the property "a" of "root" is <a>
    And the property "b" of "root" is <b>

    Examples:
      | base                                                                                                          | own                                               | log   | a | b |
      | StateChangeScript { script: root.log += "a" + root.a + "b" + root.b + ";" }                                   | PropertyChanges { target: root; a: 2; b: root.k } | a1b1; | 2 | 5 |
      | PropertyChanges { target: root; a: 2 } StateChangeScript { script: root.log += "a" + root.a + "b" + root.b + ";" }      | PropertyChanges { target: root; b: root.k }       | a2b1; | 2 | 5 |
      | PropertyChanges { target: root; a: 2; b: root.k } StateChangeScript { script: root.log += "a" + root.a + "b" + root.b + ";" } |                                                   | a2b1; | 2 | 5 |
      | PropertyChanges { target: root; a: root.k } StateChangeScript { script: root.log += "a" + root.a + "b" + root.b + ";" } | PropertyChanges { target: root; a: 3 }            | a1b1; | 5 | 1 |
      | PropertyChanges { target: root; a: 4 } StateChangeScript { script: root.log += "a" + root.a + "b" + root.b + ";" }      | PropertyChanges { target: root; a: 3 }            | a4b1; | 3 | 1 |

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

  Scenario Outline: The item's binding a state does not give back says nothing while the item is made
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real p4: "#000000".a
        states: [State { name: "s2"; when: true; PropertyChanges { target: root; p4: 3; restoreEntryValues: <restore> } }]
      }
      """
    Then the property "p4" of "root" is 3
    And the log has <n> warnings matching "Unable to assign .undefined. to double"

    Examples:
      | restore | n |
      | true    | 1 |
      | false   | 0 |

  Scenario Outline: What a state's binding and the item's say while the item is made are each their own
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property real p4: <own>
        states: [State { name: "s2"; when: true; PropertyChanges { target: root; p4: <state>; restoreEntryValues: <restore> } }]
      }
      """
    Then the property "p4" of "root" is <p4>
    And the log has 1 warning matching "Scenario.qml:<line>:[0-9]+: Unable to assign .undefined. to double"

    Examples:
      | own         | state              | restore | p4 | line |
      | "#000000".a | Math.abs(-3)       | true    | 3  | 4    |
      | 2           | "#000000".a        | false   | 2  | 5    |
      | 2           | "#000000".a        | true    | 2  | 5    |

  Scenario Outline: A state's binding that reads another the state binds is no loop through that one's own
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string p0
        property int p2: root.p0.length
        states: [State { name: "s0"; PropertyChanges { target: root; p0: "x" + Math.round(root.p2) } <more> }]
        state: "s0"
      }
      """
    Then the property "p0" of "root" is "<p0>"
    And the log has <n> warnings matching "Binding loop detected for property .p0."

    Examples:
      | more                                                   | p0 | n |
      |                                                        | x0 | 1 |
      | PropertyChanges { target: root; p2: 4 }                | x4 | 0 |
      | PropertyChanges { target: root; p2: Math.sqrt(42) }    | x6 | 0 |

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

  @backlog
  Scenario Outline: From one state's change of a property to another's, the item's binding is not run
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property int p2: ("" + "")
        states: [State { name: "s0"; PropertyChanges { target: root; p2: 4 } },
                 State { name: "s2"; <when> PropertyChanges { target: root; p2: 3 } }]
        function run() { <run> }
      }
      """
    When the method "run" of "root" is called
    Then the property "p2" of "root" is 4
    And the log has 1 warning matching "Unable to assign QString to int"

    Examples:
      | when        | run                        |
      | when: true; | state = "s0"               |
      |             | state = "s2"; state = "s0" |

  @backlog
  Scenario: From one state's change of a property to another's, a binding that reads it is no loop
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property string p0
        states: [State { name: "s0"; PropertyChanges { target: root; p0: root.p0.padStart(5, "*") } },
                 State { name: "s2"; PropertyChanges { target: root; p0: "*****" } }]
        function run() { state = "s2"; state = "s0" }
      }
      """
    When the method "run" of "root" is called
    Then the property "p0" of "root" is "*****"
    And the log has no warning matching "Binding loop"
