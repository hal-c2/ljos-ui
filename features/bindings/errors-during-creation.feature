# Sources:
#   https://doc.qt.io/qt-6/qtqml-syntax-propertybinding.html
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A binding that fails while its object is made is forgiven if it recovers
  While a component is being made, a binding may read something that is
  only set in Component.onCompleted. What it throws then is held until
  the creation is complete. A binding that runs again without throwing
  before then is not logged; one that is still failing is.

  Rule: What Component.onCompleted sets puts the bindings right

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: r
          objectName: "root"
          property var gs
          property bool b: r.gs.x == 1
          Item { objectName: "child"; property bool c: r.gs.x == 1 }
          states: State { name: "on"; when: r.gs.x == 1 }
          Component.onCompleted: gs = {x: 1}
        }
        """

    @api-QtQml.Component.completed
    Scenario Outline: A binding set right by Component.onCompleted is not logged
      Then the property "<property>" of "<object>" is <value>
      And the log has no warning matching "Cannot read property"

      Examples:
        | object | property | value |
        | root   | b        | true  |
        | child  | c        | true  |
        | root   | state    | "on"  |

  Rule: Only a binding that recovers before the creation is complete is forgiven

    @api-QtQml.Component.completed
    Scenario: A binding that runs again without throwing is not logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: r
          objectName: "root"
          property var gs
          property bool b: r.gs.x == 1
          Component.onCompleted: gs = 5
        }
        """
      Then the property "b" of "root" is false
      And the log has no warning matching "Cannot read property"

    @api-QtQml.Component.completed
    Scenario: A binding still failing once the creation is complete is logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: r
          objectName: "root"
          property var gs
          property bool b: r.gs.x == 1
        }
        """
      Then the log has a warning matching "Scenario.qml:6: TypeError: Cannot read property 'x' of undefined"

    @api-QtQml.Component.completed
    Scenario: A binding that recovers after the creation is still logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: r
          objectName: "root"
          property var gs
          property bool b: r.gs.x == 1
        }
        """
      When the property "gs" of "root" is bound to "({x: 1})"
      Then the property "b" of "root" is true
      And the log has a warning matching "Scenario.qml:6: TypeError: Cannot read property 'x' of undefined"

  # Found by ljos-fuzz (bindings), buckets warning:TypeError(ljos-only):startsWith+typeof,
  # warning:TypeError(ljos-only):Array.from+String+reduce and
  # warning:RangeError(ljos-only):function-call+loose-eq+rect.height.
  Rule: A binding replaced before the creation is complete is not logged

    @backlog
    Scenario: A binding Component.onCompleted replaces is not logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          objectName: "root"
          property string text: "x".toPrecision(3)
          Component.onCompleted: text = ""
        }
        """
      Then the property "text" of "root" is ""
      And the log has no warning matching "TypeError"

    @api-QtQml.Component.completed
    Scenario: A binding that runs again with a value its property takes is not logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: r
          objectName: "root"
          property real k
          property real p: r.k ? 1 : "#000000".a
          Component.onCompleted: r.k = 1
        }
        """
      Then the property "p" of "root" is 1
      And the log has no warning matching "Unable to assign"

    # Backlog: an assignment that takes a binding away does not yet forget what the binding
    # holds, once it has run (bindings infrastructure).
    @backlog
    Scenario: A binding whose value its property refuses, replaced by Component.onCompleted, is not logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: r
          objectName: "root"
          property real p: "#000000".a
          Component.onCompleted: r.p = 3
        }
        """
      Then the property "p" of "root" is 3
      And the log has no warning matching "Unable to assign"

    @backlog
    Scenario: A binding a change handler replaces while the object is made is not logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property list<int> numbers: []
          property bool flag: typeof ({}).startsWith("")
          onNumbersChanged: root.flag = true
        }
        """
      Then the property "flag" of "root" is true
      And the log has no warning matching "TypeError"

  # Found by ljos-fuzz (bindings), buckets value:int:size.width and
  # value:var(bool):default.
  Rule: Change handlers run while the object is made

    @backlog
    Scenario: A handler that fails to write a later binding still takes it away
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property point at: Qt.point(3, 16)
          property int n: Qt.size(18, 3).width
          onAtChanged: root.n = "#000000".hslLightness
        }
        """
      Then the property "n" of "root" is 0
      And the log has a warning matching "Cannot assign .undefined. to int"

    @backlog
    Scenario: The handlers of a tree run from its last child to its root
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string log: ""
          property list<real> a: []
          onAChanged: root.log += "root;"
          QtObject {
            property rect b: Qt.rect(0, 0, 0, 100)
            onBChanged: root.log += "first;"
          }
          QtObject {
            property point c: Qt.point(3, 16)
            onCChanged: root.log += "second;"
          }
        }
        """
      Then the property "log" of "root" is "second;first;root;"

    # Found by ljos-fuzz (bindings), bucket warning:assign(ljos-only):Unable_to_assign_int_to_QFont:
    # as a binding a change handler replaces, with a value refused in place of one that throws.
    @backlog
    Scenario: A refused value a change handler replaces while the object is made is not logged
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property list<int> numbers: []
          property font face: (1)
          onNumbersChanged: root.face = Qt.font({pixelSize: 10})
        }
        """
      Then the log has no warning matching "Unable to assign"
