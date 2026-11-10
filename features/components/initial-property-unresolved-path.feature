# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#grouped-properties
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen).

@qt-parity
Feature: Who tells of a path among the initial properties that leads nowhere
  A dotted name such as "font.bold" among the properties given to
  incubateObject or Loader.setSource, on an object that has no such path,
  is told as "Cannot resolve property" in the name of the file whose script
  called, without a line. That is neither the file the component is written
  in nor the file of the type it makes. The object is made all the same,
  with the other properties set, at once or asynchronously.

  Background:
    Given the file "Viewer.qml":
      """qml
      import QtQuick
      Item { objectName: "viewer"; property int count: 1 }
      """
    And the file "Caller.qml":
      """qml
      import QtQuick
      Item {
        Loader { id: loader; objectName: "loader" }
        function incubate(component, props, later) {
          component.incubateObject(parent, props, later ? Qt.Asynchronous : Qt.Synchronous)
        }
        function load(props, later) {
          loader.asynchronous = later
          loader.setSource("Viewer.qml", props)
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        Component { id: viewer; Viewer { } }
        Caller { id: caller }
        function incubate(later) { caller.incubate(viewer, { "font.bold": true, count: 5 }, later) }
        function load(later) { caller.load({ "font.bold": true, count: 5 }, later) }
      }
      """

  Scenario Outline: incubateObject tells it in the caller's name, <how>
    When the method "incubate" of "root" is called with <later>
    And 50 milliseconds pass
    Then the property "count" of "root/viewer" is 5
    And the log has a warning matching "^(.*/)?Caller[.]qml: Cannot resolve property .font[.]bold.[.]$"

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |

  Scenario Outline: setSource tells it in the caller's name, <how>
    When the method "load" of "root" is called with <later>
    And 50 milliseconds pass
    Then the property "count" of "loader/viewer" is 5
    And the log has a warning matching "^(.*/)?Caller[.]qml: Cannot resolve property .font[.]bold.[.]$"

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |
