# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#incubateObject-method
#   https://doc.qt.io/qt-6/qml-qtquick-loader.html#setSource-method
# Verified on Qt 6.11.2 and 6.12.0 (qml6, offscreen); the scenario marked
# @qt-diverges says where the two differ and which one Ljós follows.

@qt-parity
Feature: An initial property that names what an object property's object lacks
  A dotted name among the initial properties that goes through properties
  holding objects ("q.zz", "q.r.zz") and ends at a name the last object does
  not have is an error of the script that called createObject,
  incubateObject or Loader.setSource: "Cannot assign to non-existent
  property" with the last name, at the file and line of the call. A path
  that leads nowhere before its end is passed over in silence.

  Background:
    Given the file "Holder.qml":
      """qml
      import QtQuick
      Item {
        objectName: "holder"
        property QtObject q: QtObject { property QtObject r: QtObject { property int y } }
        property int k: 1
      }
      """
    And the file "Caller.qml":
      """qml
      import QtQuick
      Item {
        Loader { id: loader; objectName: "loader" }
        property bool nothing: false
        function make(component, props) {
          nothing = component.createObject(parent, props) === null
        }
        function incubate(component, props, later) {
          component.incubateObject(parent, props, later ? Qt.Asynchronous : Qt.Synchronous)
        }
        function load(props, later) {
          loader.asynchronous = later
          loader.setSource("Holder.qml", props)
        }
      }
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string log: ""
        Component {
          id: holder
          Holder { Component.onDestruction: root.log += "destroyed;" }
        }
        Caller { id: caller; objectName: "caller" }
        function make(name) { var p = { k: 5 }; p[name] = 1; caller.make(holder, p) }
        function incubate(later) { caller.incubate(holder, { "q.zz": 1, k: 5 }, later) }
        function load(later) { caller.load({ "q.zz": 1, k: 5 }, later) }
      }
      """

  Scenario Outline: createObject tells it at the line of the call, <path>
    When the method "make" of "root" is called with "<path>"
    Then the log has a warning matching "^(.*/)?Caller[.]qml:6: Error: Cannot assign to non-existent property .zz.$"

    Examples:
      | path   |
      | q.zz   |
      | q.r.zz |

  Scenario: A path that leads nowhere before its end is not told
    When the method "make" of "root" is called with "q.zz.w"
    Then the log has no warning matching "zz"

  # Qt 6.11.2 gives back the object with the other properties set; Qt 6.12.0
  # completes it, destroys it and gives back null. Ljós follows 6.12.
  @qt-diverges
  Scenario: createObject gives back no object
    When the method "make" of "root" is called with "q.zz"
    Then the property "nothing" of "caller" is true
    And the object "root/holder" does not exist
    And the property "log" of "root" is "destroyed;"

  Scenario Outline: incubateObject tells it at the line of the call, <how>
    When the method "incubate" of "root" is called with <later>
    And 50 milliseconds pass
    Then the property "k" of "root/holder" is 5
    And the log has a warning matching "^(.*/)?Caller[.]qml:9: Error: Cannot assign to non-existent property .zz.$"

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |

  Scenario Outline: setSource tells it at the line of the call, <how>
    When the method "load" of "root" is called with <later>
    And 50 milliseconds pass
    Then the property "k" of "loader/holder" is 5
    And the log has a warning matching "^(.*/)?Caller[.]qml:13: Error: Cannot assign to non-existent property .zz.$"

    Examples:
      | how            | later |
      | at once        | false |
      | asynchronously | true  |
