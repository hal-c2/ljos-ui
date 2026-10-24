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
