# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle) and section 19.4
#   (`app_frames`' `wakes`)
#   docs/BENCHMARKS.md (FRAME2: the turns of a frame)

# The oracle's event loop cannot be read. When a window draws is Ljós's own.
@ljos-only @desktop
Feature: A frame is drawn in the wake its time comes in
  A window that animates draws a frame at a time of its own: no sooner
  than the display would have shown the last one. When that time comes
  the UI thread wakes, asks the platform to let the window draw, and the
  platform lets it later in the same wake, after the input that came with
  it. Asked only as the wake ended, the yes came in a wake of its own,
  one more each frame, and the frame a little later.

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

  Scenario: The wake at a frame's time draws the frame
    When the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "record": true}
      """
    And 500 milliseconds pass
    And the agent calls the tool "app_frames" with:
      """json
      {"wakes": true, "max": 30}
      """
    Then the answer's text contains "woke by animation, deadline, requested_redraw; led to drawn"
    And the answer does not contain "led to redraw asked"
