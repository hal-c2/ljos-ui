# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#createObject-method
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#status-prop
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#errorString-method
#   https://doc.qt.io/qt-6/qtqml-syntax-objectattributes.html#required-properties
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A createObject that leaves a required property unset
  Each required property left out is told where it is declared, and the
  component is in Error until a createObject succeeds: its errorString says
  why, and incubateObject makes nothing meanwhile. The status changes without
  a statusChanged.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property int status: -1
        property string error: "none"
        property bool made: false
        property int seen: needy.status
        Component { id: needy; Item { objectName: "needy"; required property int n
            required property int m } }
        function make(props) { made = needy.createObject(root, props) !== null; look() }
        function incubate() { made = needy.incubateObject(root, { n: 1, m: 2 }, Qt.Synchronous) !== null; look() }
        function look() {
          status = needy.status
          error = needy.errorString().replace(/^.*[.]qml:/gm, "").replace(/\n/g, "|")
        }
      }
      """

  Scenario: The component is in Error, and says why
    When the method "make" of "root" is called with {"m": 1}
    Then the property "made" of "root" is false
    And the property "status" of "root" is 3
    And the property "error" of "root" is "9:60: Required property n was not initialized|"
    And the property "seen" of "root" is 1
    And the log has a warning matching "^.*[.]qml:9:60: Required property n was not initialized$"

  Scenario: Each property left out is told at its declaration
    When the method "make" of "root" is called with {}
    Then the log has a warning matching "^.*[.]qml:9:60: Required property n was not initialized$"
    And the log has a warning matching "^.*[.]qml:10:13: Required property m was not initialized$"

  Scenario: incubateObject makes nothing while the component is in Error
    When the method "make" of "root" is called with {"m": 1}
    And the method "incubate" of "root" is called
    Then the property "made" of "root" is false
    And the object "root/needy" does not exist
    And the log has a warning matching "^QQmlComponent: Component is not ready$"

  Scenario: A createObject that succeeds makes it Ready again
    When the method "make" of "root" is called with {"m": 1}
    And the method "make" of "root" is called with {"n": 1, "m": 2}
    Then the property "made" of "root" is true
    And the property "status" of "root" is 1
    And the property "error" of "root" is ""
    When the method "incubate" of "root" is called
    Then the property "made" of "root" is true
    And the log has no warning matching "not ready"
