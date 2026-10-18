# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle)
#   docs/BENCHMARKS.md (the whole gallery against qml6: about eight passes
#   a click, where qml6 wakes about three times)

# The oracle's event loop cannot be read. What a pass costs is Ljós's own.
@ljos-only @desktop
Feature: A click runs only the passes it needs
  A pass runs the window's scene and paints what changed. A click is a
  press and a release, and each is handled in a pass of its own. A pass
  that shows nothing new and that the scene asked for nothing after asks
  for no further pass: the window sleeps again until something happens.
  Anything that wakes it later (a message, a timer, a picture) still does.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        MouseArea {
          objectName: "area"
          property int clicks: 0
          anchors.fill: parent
          onClicked: clicks += 1
        }
      }
      """

  # On the desktop surface: the passes are the shell's, which the headless
  # runner does not have. The count comes from the inspector, whose own
  # passes are left out of it.
  Scenario: A click that changes nothing on screen runs a pass for the press and one for the release
    When the user moves the pointer to 50, 30
    And 500 milliseconds pass
    And the window's passes are counted
    And the user presses the pointer at 50, 30
    And the user releases the pointer at 50, 30
    And 1000 milliseconds pass
    Then the window ran at most 2 passes since they were counted
    And the property "clicks" of "area" is 1
