# Sources:
#   docs/DIALECT.md §4.19 (Task: run, args, autoRun, state, start, abort)
#   docs/ARCHITECTURE.md §9.3 (faults: JsInterrupted, JsOutOfMemory)
#   docs/ARCHITECTURE.md §9.4 (Supervisor: content a resource fault took is made again)
# Qt has no Task.

@ljos-only
Feature: A Task whose run could not be started has failed, and is started again after a resource fault
  A run that could not be started has failed: `status` is "failed" and
  `error` says why, so the application can show it and start the run again.
  When a script was interrupted or the script heap was full as it started,
  which may not happen again, the Task starts the run again by itself, at
  once after the first such fault and after a wait of 1 second, doubling up to
  a minute, after each one soon after.

  Background:
    Given JavaScript is timed by a clock on which native work a script asks for takes 0 seconds

  Scenario: A run interrupted as it starts is started again and is done
    Given the QML:
      """qml
      import Ljos.Core
      import Ljos.Ui
      Item {
        id: root; objectName: "root"
        property int tries: 0
        property bool always: false
        Task {
          id: work; objectName: "work"
          args: []
          run: async (signal: AbortSignal): Promise<string> => {
            root.tries += 1
            if (root.always || root.tries === 1) for (;;) { }
            return "made"
          }
        }
      }
      """
    Then the property "tries" of "root" is 2
    And the property "status" of "work" is "done"
    And the property "value" of "work" is "made"
    And the log has a warning matching "Task: the run could not be started: .*; a script was interrupted, so it is started again at once"
    And the application has no fault

  Scenario: A run interrupted each time it starts has failed, and is started again after a wait
    Given the QML:
      """qml
      import Ljos.Core
      import Ljos.Ui
      Item {
        id: root; objectName: "root"
        property int tries: 0
        Task {
          id: work; objectName: "work"
          args: []
          run: async (signal: AbortSignal): Promise<string> => {
            root.tries += 1
            for (;;) { }
          }
        }
      }
      """
    Then the property "tries" of "root" is 2
    And the property "status" of "work" is "failed"
    And the log has a warning matching "so it is started again in 1 s"
    When 999 milliseconds pass
    Then the property "tries" of "root" is 2
    When 1 millisecond passes
    Then the property "tries" of "root" is 3
