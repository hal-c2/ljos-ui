# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle; work is
#   scheduled by dirty state, timers, animations, input or messages)
#   docs/BENCHMARKS.md (what wakes an animated application)

# The oracle's threads cannot be read. How often Ljós's own wake is its own.
@ljos-only @desktop
Feature: An animating window wakes about once a frame
  A window that animates is woken for each frame the display takes, and
  little else wakes for it. A frame that changes only a small part of
  the window draws that part on the window's own thread: handing a few
  pixels to helper threads costs more wakes than drawing them, and a
  large part, where helpers draw sooner, is still shared out. The
  display in the sandbox takes 60 frames a second. Each frame wakes the
  window's thread about three times: the timer that asks for the frame
  half a frame early, the display's frame callback, and the display
  handing the drawn buffer back. A second of an animation is then about
  180 wakes, and 240 leaves a wake a frame for the machine's load; the
  twenty a frame this used to take fail it.

  Scenario: A small animation drawn on the processor wakes about once a frame
    Given the window is drawn on the processor
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 400; height: 300; color: "white"
        Rectangle {
          x: 360; y: 14; width: 20; height: 20; color: "#3060c0"
          RotationAnimation on rotation {
            from: 0; to: 360; duration: 1000; loops: Animation.Infinite
          }
        }
      }
      """
    When 500 milliseconds pass
    And the wakes of the application's threads are counted
    And 1000 milliseconds pass
    Then the application's threads woke at most 240 times in all since the wakes were counted
