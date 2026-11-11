# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-qt.html#binding-method
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity
Feature: An initial Qt.binding, made at once or asynchronously
  A property given a Qt.binding among the initial properties of
  Loader.setSource or incubateObject is set before the object is complete,
  in the same way whether the object is made at once or asynchronously:
  its change is told first, then that of a binding of the object that reads
  it, as the bindings run, and the object is complete with both. It follows
  its binding after.

  Background:
    Given the file "Shown.qml":
      """qml
      import QtQuick
      Item {
        objectName: "shown"
        property int w: 0
        property int dbl: w * 2
        onWChanged: root.log += "w" + w + " "
        onDblChanged: root.log += "dbl" + w + "/" + dbl + " "
        Component.onCompleted: root.log += "completed" + w + "/" + dbl + " "
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int v: 3
        property string log: ""
        Component { id: shown; Shown { } }
        Loader { id: loader; objectName: "loader"; onLoaded: root.log += "loaded" }
        function load(later) {
          loader.asynchronous = later
          loader.setSource("Shown.qml", { w: Qt.binding(function() { return root.v }) })
        }
        function incubate(later) {
          shown.incubateObject(root, { w: Qt.binding(function() { return root.v }) },
                               later ? Qt.Asynchronous : Qt.Synchronous)
        }
      }
      """

  Scenario Outline: setSource sets it before the item is complete, <how>
    When the method "load" of "root" is called with <later>
    And 50 milliseconds pass
    Then the property "log" of "root" is "w3 dbl3/6 completed3/6 loaded"
    When the property "v" of "root" is set to 4
    Then the property "dbl" of "loader/shown" is 8

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |

  Scenario Outline: incubateObject sets it before the object is complete, <how>
    When the method "incubate" of "root" is called with <later>
    And 50 milliseconds pass
    Then the property "log" of "root" is "w3 dbl3/6 completed3/6 "
    When the property "v" of "root" is set to 4
    Then the property "dbl" of "root/shown" is 8

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |
