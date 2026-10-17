# Sources:
#   https://doc.qt.io/qt-6/qml-qtqml-component.html#progress-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: How far a component is loaded
  A component made from a local file, or declared in place, is loaded
  whole: its progress is 1, whatever its status. progressChanged is a
  signal a Connections can handle.

  Scenario: Every local component is loaded whole
    Given the file "Good.qml":
      """qml
      import QtQuick
      Item {}
      """
    And the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        Component { id: inline; Item {} }
        property real here: inline.progress
        property string kind: typeof inline.progress
        property real good: Qt.createComponent("Good.qml").progress
        property real missing: Qt.createComponent("Missing.qml").progress
        Connections { target: inline; function onProgressChanged() {} }
      }
      """
    Then the property "here" of "root" is 1
    And the property "kind" of "root" is "number"
    And the property "good" of "root" is 1
    And the property "missing" of "root" is 1
    And the log has no warning matching "progress|Detected function"
