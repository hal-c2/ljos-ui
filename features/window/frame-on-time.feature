# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle) and section 19.4
#   (`app_frames`' `wakes`)
#   docs/BENCHMARKS.md (FRAME2: the turns of a frame)

# The oracle's event loop cannot be read. When a window draws is Ljós's own.
@ljos-only @desktop
Feature: A frame is drawn in the wake its time comes in
  A window that animates draws a frame at a time of its own: no sooner
  than the display would have shown the last one. The window asks for
  the frame a little before its time, so that a display a little faster
  than the window thinks is not missed one frame in twenty; the platform
  lets it draw once the display has taken the last frame, in the wake
  that brings that news, after the input that came with it. A wake asked
  for before the window may draw again is put off to when it may, so no
  wake between two frames finds nothing to do. A message that wakes the
  thread once the time has come, such as an agent's request, asks the
  same way.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        Rectangle {
          width: 10; height: 10; color: "black"
          NumberAnimation on x {
            from: 0; to: 190; duration: 1000; loops: Animation.Infinite
          }
        }
      }
      """

  Scenario: An animation's frame is drawn once the display has taken the last one
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And 500 milliseconds pass
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "max": 30}
      """
    Then the answer's text contains "woke by animation, requested_redraw; led to drawn"
    And the answer does not contain "woke by deadline; led to nothing"
