# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#completed-signal
# Verified on Qt 6.11.2 (qml6, offscreen).
# Found by ljos-fuzz (creation), buckets warning:binding-loop(ljos-only):load-handlers,
# value:onCompleted:text and value:load-handlers:number.

@qt-parity
Feature: An object made by a binding
  A binding that makes an object depends on nothing the object does while it is made, and
  the object is completed before what holds it.

  @backlog
  Scenario: A binding that makes an object is not run again by what the object wrote
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        property var held: maker.createObject(root)
        Component {
          id: maker
          QtObject { Component.onCompleted: root.log += "made;" }
        }
        Component.onCompleted: log += "root;"
        function make() { held = maker.createObject(root) }
      }
      """
    Then the property "log" of "root" is "made;root;"
    When the method "make" of "root" is called
    Then the property "log" of "root" is "made;root;made;"
    And the log has no warning matching "Binding loop"
