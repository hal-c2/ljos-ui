# Sources:
#   docs/ARCHITECTURE.md sections 19.4 and 19.6 (`app_resize`)
#   https://doc.qt.io/qt-6/qml-qtquick-window-window.html#width-prop
#   https://doc.qt.io/qt-6/qquickview.html#ResizeMode-enum
# The inspector is a Ljós extension: Qt has nothing to compare it with.

@ljos-only @desktop
Feature: An agent resizes the window of a running application
  On the desktop the window is the platform's. `app_resize` asks the
  platform to give the primary window the size, held within the window's
  smallest and largest size, and answers once the window has it, or with
  the size it has and the size asked for when it was given another.
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

  # The window's own limits make the platform's answer the same on every
  # desktop: the size asked for is past the largest the window may have.
  @api-QtQuick.Window.maximumWidth
  Scenario: A size past the window's largest is answered with the size given
    Given the QML:
      """qml
      import QtQuick
      Window {
        objectName: "root"
        width: 200; height: 100; visible: true
        maximumWidth: 250
        Rectangle { objectName: "fill"; anchors.fill: parent }
      }
      """
    When the agent calls the tool "app_resize" with:
      """json
      {"width": 400, "height": 150}
      """
    Then the answer's "/width" is 250
    And the answer's "/height" is 150
    And the answer's "/asked/width" is 400
    And the answer's "/asked/height" is 150
    And the answer's text contains "asked for 400x150; the platform gave the window 250x150"
    And the property "width" of "root" is 250
    And the property "width" of "fill" is 250
