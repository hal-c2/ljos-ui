# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
#   https://doc.qt.io/qt-6/qml-qtqml-connections.html
# Verified on Qt 6.12.0 (qml6, offscreen).

@qt-parity
Feature: An incubated object is completed as a created one is
  An object incubateObject makes, at once or later, is given its initial
  properties and completed as one createObject makes: a change handler of
  an initial property reads the object's bindings as they will be, and
  whatever listens to the attached Component, a Connections or a function
  connected to its completed signal, is told it completed.

  Background:
    Given the file "Made.qml":
      """qml
      import QtQuick
      Item {
        id: me
        property int w: 0
        property int dbl: w * 2
        property string seen: ""
        property string heard: ""
        onWChanged: seen += "w" + w + "/" + dbl
        Connections {
          target: Component
          function onCompleted() { me.heard += "completed" + me.dbl }
        }
      }
      """
    And the file "Lone.qml":
      """qml
      import QtQuick
      Item {
        id: me
        property string seen: ""
        property string heard: ""
        Component.onCompleted: {
          seen = "done"
          Component.completed.connect(function() { me.heard += "late" })
        }
      }
      """
    And the file "Heard.qml":
      """qml
      import QtQuick
      Item {
        id: me
        property string seen: ""
        property string heard: ""
        Component.onCompleted: {
          seen = "done"
          Component.completed.connect(function() { me.heard += " late" })
        }
        Connections { target: me.Component; function onCompleted() { me.heard += "conn" } }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        Component { id: made; Made { } }
        Component { id: lone; Lone { } }
        Component { id: heard; Heard { } }
        function tell(o) { log = o.seen + " " + o.heard }
        function make(o) {
          const c = { made: made, lone: lone, heard: heard }[o.file]
          const props = o.file === "made" ? { w: 3 } : {}
          if (o.how === "create") {
            tell(c.createObject(root, props))
            return
          }
          const inc = c.incubateObject(root, props,
                                       o.how === "later" ? Qt.Asynchronous : Qt.Synchronous)
          if (inc.status === Component.Ready)
            tell(inc.object)
          else
            inc.onStatusChanged = function(s) { if (s === Component.Ready) tell(inc.object) }
        }
      }
      """

  Scenario Outline: The object is given its properties and completed, <how>
    When the method "make" of "root" is called with {"file": "made", "how": "<how>"}
    And 50 milliseconds pass
    Then the property "log" of "root" is "w3/6 completed6"
    And the log has no warnings

    Examples:
      | how    |
      | create |
      | sync   |
      | later  |

  Scenario Outline: A function onCompleted connects to completed is called, <file> <how>
    When the method "make" of "root" is called with {"file": "<file>", "how": "<how>"}
    And 50 milliseconds pass
    Then the property "log" of "root" is "<log>"
    And the log has no warnings

    Examples:
      | file  | how    | log            |
      | lone  | create | done late      |
      | lone  | sync   | done late      |
      | lone  | later  | done late      |
      | heard | create | done conn late |
      | heard | sync   | done conn late |
      | heard | later  | done conn late |
