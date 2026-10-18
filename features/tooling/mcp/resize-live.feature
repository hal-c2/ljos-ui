# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.6 (`app_resize`)
#   https://doc.qt.io/qt-6/qml-qtquick-window-window.html#width-prop
#   https://doc.qt.io/qt-6/qquickview.html#ResizeMode-enum
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: An agent resizes the window of a running application
  On the desktop the window is the platform's. `app_resize` asks the
  platform to give the primary window the size and answers once the
  window has it, or with the size it has when the platform gave another.
  The scene is then laid out at that size as if the user had dragged
  the window's edge: a root item takes the window's size, a root
  `Window` is told its new width and height, and what is anchored to
  them follows.

  Scenario: A root item takes the size of its window, and what fills it follows
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        width: 200; height: 100; color: "white"
        Rectangle { objectName: "fill"; anchors.fill: parent; anchors.margins: 10 }
      }
      """
    When the agent calls the tool "app_resize" with:
      """json
      {"width": 320, "height": 240}
      """
    Then the answer's "/width" is 320
    And the answer's "/height" is 240
    And the property "width" of "root" is 320
    And the property "width" of "fill" is 300
    And the property "height" of "fill" is 220
    When the agent calls the tool "app_status"
    Then the answer's "/windows/0/width" is 320
    And the answer's "/windows/0/height" is 240

  Scenario: A root window is told its new size
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "root"
        width: 200; height: 100; visible: true
        Rectangle { objectName: "fill"; anchors.fill: parent }
      }
      """
    When the agent calls the tool "app_resize" with:
      """json
      {"width": 300, "height": 150}
      """
    Then the answer's "/width" is 300
    And the property "width" of "root" is 300
    And the property "height" of "root" is 150
    And the property "width" of "fill" is 300
