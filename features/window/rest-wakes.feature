# Sources:
#   docs/ARCHITECTURE.md (principle 6: 0 % CPU when idle)

# The oracle's event loop cannot be read. When the window wakes is Ljós's own.
@ljos-only @desktop
Feature: A resting window does not wake
  A window with no timer, animation or input to answer sleeps until
  something happens. What the scripts left behind is collected once the
  window has rested a while after a burst of work, once a rest, and only
  when the scripts made something to collect: a window that rests with
  nothing to collect is not woken to look.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 200; height: 100; color: "white"
        MouseArea {
          objectName: "area"
          property int made: 0
          anchors.fill: parent
          // Garbage only the collector frees: objects nobody keeps, each
          // holding itself, so counting references does not free them.
          onClicked: {
            let kept = []
            for (let i = 0; i < 40000; i++) {
              const o = { n: i }
              o.self = o
              kept.push(o)
            }
            made = kept.length
          }
        }
      }
      """

  Scenario: A window at rest stays asleep
    When 1000 milliseconds pass
    And the wakes of the application's threads are counted
    And 2000 milliseconds pass
    Then the window's thread woke at most 0 times since the wakes were counted

  # The click and its passes are over within a frame or two, and the
  # collection a rest later: the script heap no longer holds the click's
  # objects, and the window sleeps.
  Scenario: After scripts made garbage, the window collects it while resting, then rests
    When the user presses the pointer at 50, 30
    And the user releases the pointer at 50, 30
    And 1000 milliseconds pass
    Then the property "made" of "area" is 40000
    When the agent calls the tool "app_metrics" with:
      """json
      {"memory": {}}
      """
    Then the answer's "/owners/0/owner" is "script"
    And the answer's "/owners/0/count" is at most 20000
    When 1000 milliseconds pass
    And the wakes of the application's threads are counted
    And 2000 milliseconds pass
    Then the window's thread woke at most 0 times since the wakes were counted

