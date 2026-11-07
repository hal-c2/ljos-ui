# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#doubleClicked-signal
#   https://doc.qt.io/qt-6/qml-qtquick-mousearea.html#clicked-signal
# Verified on Qt 6.11.2 (qml6, offscreen).
# Only the double-click step keeps its two presses inside Qt's double-click interval on the Qt
# backend; presses in steps of their own are as far apart as the machine is slow.

@qt-parity
Feature: A MouseArea's double-click
  A press soon after a click, where it was, says doubleClicked. When
  something hears it, the release after it is no click, even if the
  handler turns it down; when nothing does, it is a click as any other.

  Scenario: A double-click nothing hears is two clicks
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        property string log: ""
        MouseArea {
          anchors.fill: parent
          onReleased: root.log += "released|"
          onClicked: root.log += "clicked|"
        }
      }
      """
    When the user double-clicks at 50, 50
    Then the property "log" of "root" is "released|clicked|released|clicked|"

  Scenario: A double-click heard is one click and the double
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        property string log: ""
        MouseArea {
          anchors.fill: parent
          onReleased: root.log += "released|"
          onClicked: root.log += "clicked|"
          onDoubleClicked: root.log += "double|"
        }
      }
      """
    When the user double-clicks at 50, 50
    Then the property "log" of "root" is "released|clicked|double|released|"

  Scenario: A double-click heard and turned down is still one click
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        width: 200; height: 100
        property string log: ""
        MouseArea {
          anchors.fill: parent
          onClicked: root.log += "clicked|"
          onDoubleClicked: mouse => { mouse.accepted = false; root.log += "double|" }
        }
      }
      """
    When the user double-clicks at 50, 50
    Then the property "log" of "root" is "clicked|double|"
