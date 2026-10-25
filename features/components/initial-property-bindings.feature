# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
# Verified on Qt 6.11.2 (qml6, offscreen).
# Issue #128.

@qt-parity
Feature: A Qt.binding among the first property values of a new object
  A value made with Qt.binding among the properties Loader.setSource, createObject or
  incubateObject give a new object is a binding on it, as an assignment of it is: the
  property has what the binding gives, and follows it.

  Background:
    Given the file "Panel.qml":
      """qml
      import QtQuick
      Item {
        objectName: "panel"
        property var source: null
        width: 100; height: 100
        ListView {
          objectName: "view"; anchors.fill: parent
          model: parent.source
          delegate: Text { required property string name; text: name }
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 300; height: 300
        property var current: fruits
        property int n: 2
        property string seen: ""
        ListModel { id: fruits; ListElement { name: "apple" } ListElement { name: "pear" } }
        ListModel { id: more; ListElement { name: "fig" } }
        Component { id: comp; QtObject { property int m: 0 } }
        Loader { id: loader; objectName: "loader"; anchors.fill: parent }
        function swap() { current = more }
        function load() { loader.setSource("Panel.qml", { source: Qt.binding(() => root.current) }) }
        function make() {
          const a = comp.createObject(root, { m: Qt.binding(() => root.n * 10) })
          const b = comp.incubateObject(root, { m: Qt.binding(() => root.n * 100) }, Qt.Synchronous).object
          seen = a.m + "," + b.m; n = 3; seen += ";" + a.m + "," + b.m
        }
      }
      """

  Scenario: setSource gives the loaded item a binding
    When the method "load" of "root" is called
    Then the property "count" of "loader/panel/view" is 2
    And the log has no warning matching "Required property"
    When the method "swap" of "root" is called
    Then the property "count" of "loader/panel/view" is 1

  Scenario: createObject and incubateObject give the new object a binding
    When the method "make" of "root" is called
    Then the property "seen" of "root" is "20,200;30,300"
