# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle)
#   https://doc.qt.io/qt-6/qquickwindow.html (frameSwapped, update)

# When a picture reaches the window system is Ljós's own to say; the oracle
# runs in no compositor the scenario drives.
@ljos-only @desktop
Feature: A window says when its picture was shown
  `frameSwapped` comes each time a picture of the window reached the window
  system, and `update()` asks for one more though nothing changed. A clock
  that paces itself by them stands still with the window: none comes while
  nothing in the window changes, nor while the window system lets the
  window draw no more because nobody can see it.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Window {
        id: root
        objectName: "root"
        width: 200; height: 100; visible: true
        property int shown: 0
        property int from: 0
        readonly property int since: shown - from
        property bool moving: false
        function mark() { from = shown }
        onFrameSwapped: shown++
        Rectangle {
          width: 10; height: 10; color: "black"
          NumberAnimation on x {
            running: root.moving
            from: 0; to: 190; duration: 1000; loops: Animation.Infinite
          }
        }
      }
      """

  Scenario: A window at rest shows no picture
    When 500 milliseconds pass
    Then the property "shown" of "root" is at least 1
    When the method "mark" of "root" is called
    And 1000 milliseconds pass
    Then the property "since" of "root" is 0

  Scenario: update asks for one picture
    When 500 milliseconds pass
    And the method "mark" of "root" is called
    And the method "update" of "root" is called
    And 500 milliseconds pass
    Then the property "since" of "root" is 1

  Scenario: Each picture of an animation is shown
    When the property "moving" of "root" is set to true
    And the method "mark" of "root" is called
    And 500 milliseconds pass
    Then the property "since" of "root" is at least 10

  # The window system stops letting a window it does not show draw, and
  # the window waits for it rather than drawing pictures nobody sees.
  Scenario: A window out of sight shows no picture until it is seen again
    When the property "moving" of "root" is set to true
    And 300 milliseconds pass
    And the window is put out of sight
    And 500 milliseconds pass
    And the method "mark" of "root" is called
    And 1000 milliseconds pass
    Then the property "since" of "root" is 0
    When the window is brought back into sight
    And 500 milliseconds pass
    Then the property "since" of "root" is at least 10
