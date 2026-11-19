# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-multipointtoucharea.html
#   https://doc.qt.io/qt-6/qml-qtquick-item.html#enabled-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: A finger goes to a MultiPointTouchArea only when nothing above it takes the press
  A finger that comes down goes to the topmost item that takes it: a
  MultiPointTouchArea, or what takes a press, such as a MouseArea or a
  TapHandler that grabs the point. An area that is disabled lets fingers
  through, and one disabled while a finger is down hears no more of it,
  not even `canceled`.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 400; height: 200
        property string log: ""
        MouseArea { anchors.fill: parent; onPressed: root.log += "under " }
        MultiPointTouchArea {
          id: area
          objectName: "area"
          anchors.fill: parent
          onPressed: root.log += "P "
          onReleased: root.log += "R "
          onCanceled: root.log += "C "
        }
        Rectangle {
          x: 0; width: 100; height: 100
          TapHandler { gesturePolicy: TapHandler.WithinBounds; onTapped: root.log += "tap " }
        }
        MouseArea { x: 100; width: 100; height: 100; onPressed: root.log += "mouse " }
        MultiPointTouchArea { x: 300; width: 100; height: 100; onPressed: { root.log += "stop "; area.enabled = false } }
      }
      """

  Scenario: A TapHandler that grabs, above the area, has the finger
    When the user's fingers go through these positions:
      """
      0 at 50, 50
      """
    Then the property "log" of "root" is "tap "

  Scenario: A MouseArea above the area has the finger
    When the user's fingers go through these positions:
      """
      0 at 150, 50
      """
    Then the property "log" of "root" is "mouse "

  Scenario: A disabled area lets the finger go to what is under it
    When the property "enabled" of "area" is set to false
    And the user's fingers go through these positions:
      """
      0 at 250, 150
      """
    Then the property "log" of "root" is "under "

  Scenario: An area disabled while a finger is down hears no more of it
    When the user's fingers go through these positions:
      """
      0 at 250, 150
      0 at 250, 150; 1 at 350, 50
      0 at 260, 150; 1 at 350, 50
      0 at 270, 150
      """
    Then the property "log" of "root" is "P stop "
