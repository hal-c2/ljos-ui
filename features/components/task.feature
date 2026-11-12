# Sources:
#   docs/DIALECT.md §4.19 (Task: run, args, autoRun, state, start, abort), §4.20 (a Task is a Controller)
#   https://lit.dev/docs/data/task/
#   https://dom.spec.whatwg.org/#interface-abortcontroller
#   https://www.typescriptlang.org/docs/handbook/2/narrowing.html#discriminated-unions
# Qt has no Task: a request is an XMLHttpRequest a script starts and follows by hand.
# Every scenario is `@ljos-only`.

@ljos-only
Feature: Task: async work run from its arguments, and where it got to
  A `Task` (`Ljos.Core`) runs `run`, an async function of the elements of
  `args` and then an `AbortSignal`, as it completes and again when an
  element of `args` changes. A run a newer one replaced is aborted through
  its signal, and what it gives is dropped. Where the Task got to is
  `state`, one value that says its `status` and the `value` and `error`
  it has, and is `status`, `value` and `error` one at a time; each changes
  once a step. A Task is a Controller: when the item it is declared in
  goes, its run is aborted. The checker types a Task from its `run`.

  Background:
    Given the web server serves the text file "act.txt":
      """
      Read a book
      """

  Rule: A Task runs as it completes, and says where it got to

    Scenario: A Task runs with its arguments and is done with what its run gives
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property string name: "act.txt"
          Task {
            id: activity; objectName: "activity"
            args: [root.name]
            run: async (name: string, signal: AbortSignal): Promise<string> =>
              (await (await fetch(`$SERVER/${name}`, { signal })).text()).trim()
          }
          property string shown: activity.value ?? activity.status
        }
        """
      Then the property "shown" of "root" is "pending"
      When 1 second passes
      Then the property "shown" of "root" is "Read a book"
      And the property "status" of "activity" is "done"
      And the application has nothing scheduled

    Scenario: A Task waiting on a reply is pending, with no value yet
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          Task {
            id: waiting; objectName: "waiting"
            run: async (signal: AbortSignal): Promise<string> =>
              (await fetch("$SERVER/held", { signal })).text()
          }
          property string shown: waiting.status + " " + (waiting.value ?? "none")
        }
        """
      Then the property "shown" of "root" is "pending none"

    Scenario: A Task that is not told to run is initial
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property int runs: 0
          Task {
            id: later; objectName: "later"
            autoRun: false
            run: async (signal: AbortSignal): Promise<string> => {
              root.runs = root.runs + 1
              return "ran"
            }
          }
          property string shown: JSON.stringify(later.state) + " " + (later.value ?? "none") + " " + told(later.error)
          function told(error: unknown): string {
            if (typeof error === "object" && error !== null && "name" in error && "message" in error
                && typeof error.name === "string" && typeof error.message === "string") {
              return error.name + ": " + error.message
            }
            return "none"
          }
        }
        """
      Then the property "shown" of "root" is "{\"status\":\"initial\"} none none"
      And the property "runs" of "root" is 0

    Scenario: A run that changes its own args as it starts is aborted, and the run it began is the Task's
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property string city: "Oslo"
          property int aborts: 0
          property string ran: ""
          Task {
            id: task; objectName: "task"
            args: [root.city]
            run: async (city: string, signal: AbortSignal): Promise<string> => {
              signal.onabort = () => { root.aborts = root.aborts + 1 }
              root.ran = root.ran + city + ";"
              if (city === "Oslo") { root.city = "Bergen" }
              return city
            }
          }
        }
        """
      Then the property "ran" of "root" is "Oslo;Bergen;"
      And the property "aborts" of "root" is 1
      And the property "value" of "task" is "Bergen"
      And the log has no warnings
      When the property "city" of "root" is set to "Oslo"
      Then the property "ran" of "root" is "Oslo;Bergen;Oslo;Bergen;"
      And the property "aborts" of "root" is 2

  Rule: A change of args aborts the run in progress and starts another

    Background:
      Given the web server serves the text file "other.txt":
        """
        Something else
        """
      And the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property string file: "held"
          property int tick: 0
          property int runs: 0
          property int aborts: 0
          property string said: ""
          Task {
            id: task; objectName: "task"
            args: [root.file, root.tick >= 0]
            // The request is not given the signal: what a run that goes on
            // after it is aborted gives is dropped all the same.
            run: async (file: string, always: boolean, signal: AbortSignal): Promise<string> => {
              root.runs = root.runs + 1
              signal.onabort = () => { root.aborts = root.aborts + 1 }
              return (await (await fetch(`$SERVER/${file}`)).text()).trim()
            }
            onDone: (value: string) => { root.said = root.said + value + ";" }
          }
          property string shown: task.status + " " + (task.value ?? "none")
        }
        """

    Scenario: A new element aborts the run in progress and starts another
      When the property "file" of "root" is set to "act.txt"
      Then the property "runs" of "root" is 2
      And the property "aborts" of "root" is 1
      And the property "shown" of "root" is "pending none"
      When 1 second passes
      Then the property "shown" of "root" is "done Read a book"

    Scenario: What a run a newer one replaced gives is dropped, even when it comes
      When the property "file" of "root" is set to "other.txt"
      And the property "file" of "root" is set to "act.txt"
      And 1 second passes
      Then the property "said" of "root" is "Read a book;"
      And the property "shown" of "root" is "done Read a book"
      And the property "aborts" of "root" is 2

    Scenario: args made again with the same elements runs nothing
      When the property "tick" of "root" is set to 5
      Then the property "runs" of "root" is 1
      And the property "aborts" of "root" is 0

  Rule: state is one value, and status, value and error are its parts

    Scenario: A pending and a failed Task keep the value the last run gave
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property string file: "act.txt"
          Task {
            id: task
            args: [root.file]
            run: async (file: string, signal: AbortSignal): Promise<string> => {
              const response = await fetch(`$SERVER/${file}`, { signal })
              if (!response.ok) {
                throw new Error(response.statusText)
              }
              return (await response.text()).trim()
            }
          }
          property string shown: task.status + " " + (task.value ?? "none") + " " + told(task.error)
          function told(error: unknown): string {
            if (typeof error === "object" && error !== null && "name" in error && "message" in error
                && typeof error.name === "string" && typeof error.message === "string") {
              return error.name + ": " + error.message
            }
            return "none"
          }
        }
        """
      When 1 second passes
      Then the property "shown" of "root" is "done Read a book none"
      When the property "file" of "root" is set to "held"
      Then the property "shown" of "root" is "pending Read a book none"
      When the property "file" of "root" is set to "missing.txt"
      And 1 second passes
      Then the property "shown" of "root" is "failed Read a book Error: Not Found"

    Scenario: Each part changes once a step, and a binding of two of them once
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property string file: "act.txt"
          Task {
            id: task
            args: [root.file]
            run: async (file: string, signal: AbortSignal): Promise<string> =>
              (await (await fetch(`$SERVER/${file}`, { signal })).text()).trim()
            onStateChanged: root.states = root.states + 1
            onStatusChanged: root.statuses = root.statuses + 1
            onValueChanged: root.values = root.values + 1
            onErrorChanged: root.errors = root.errors + 1
          }
          property int states: 0
          property int statuses: 0
          property int values: 0
          property int errors: 0
          property int boths: 0
          property string both: task.status + (task.value ?? "")
          onBothChanged: boths = boths + 1
          function forget(): void { states = 0; statuses = 0; values = 0; errors = 0; boths = 0 }
        }
        """
      When the method "forget" of "root" is called
      And 1 second passes
      Then the property "states" of "root" is 1
      And the property "statuses" of "root" is 1
      And the property "values" of "root" is 1
      And the property "errors" of "root" is 0
      And the property "boths" of "root" is 1
      When the method "forget" of "root" is called
      And the property "file" of "root" is set to "held"
      Then the property "states" of "root" is 1
      And the property "statuses" of "root" is 1
      And the property "values" of "root" is 0
      And the property "boths" of "root" is 1

  Rule: A Task runs when told, and stops when told

    Scenario: With autoRun false, a Task runs only when started, with start's arguments or args'
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property string word: "given"
          property string said: ""
          Task {
            id: task; objectName: "task"
            autoRun: false
            args: [root.word]
            run: async (word: string, signal: AbortSignal): Promise<string> => word + "!"
            onDone: (value: string) => { root.said = root.said + value }
          }
        }
        """
      When the property "word" of "root" is set to "changed"
      Then the property "status" of "task" is "initial"
      When the method "start" of "task" is called
      Then the property "value" of "task" is "changed!"
      When the method "start" of "task" is called with "other"
      Then the property "value" of "task" is "other!"
      And the property "said" of "root" is "changed!other!"

    Scenario: abort() fails the run in progress with its signal's reason, at once
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property int failures: 0
          Task {
            id: task; objectName: "task"
            // Not given the signal: what the request gives is dropped.
            run: async (signal: AbortSignal): Promise<string> =>
              (await fetch("$SERVER/held")).text()
            onFailed: (error: unknown) => { root.failures = root.failures + 1 }
          }
          property string shown: task.status + " " + told(task.error)
          function told(error: unknown): string {
            if (typeof error === "object" && error !== null && "name" in error && "message" in error
                && typeof error.name === "string" && typeof error.message === "string") {
              return error.name + ": " + error.message
            }
            return "none"
          }
          function stop(): void { task.abort() }
        }
        """
      When the method "stop" of "root" is called
      Then the property "shown" of "root" is "failed AbortError: signal is aborted without reason"
      And the property "failures" of "root" is 1
      And the property "value" of "task" is null

    Scenario: done and failed say what a run gave, and a run that throws has failed
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          property string said: ""
          property bool broken: false
          Task {
            id: task; objectName: "task"
            args: [root.broken]
            run: (broken: boolean, signal: AbortSignal): string => {
              if (broken) { throw new Error("broken") }
              return "whole"
            }
            onDone: (value: string) => { root.said = root.said + "done " + value + "; " }
            onFailed: (error: unknown) => { root.said = root.said + "failed " + root.told(error) + "; " }
          }
          function told(error: unknown): string {
            if (typeof error === "object" && error !== null && "name" in error && "message" in error
                && typeof error.name === "string" && typeof error.message === "string") {
              return error.name + ": " + error.message
            }
            return "none"
          }
        }
        """
      When the property "broken" of "root" is set to true
      Then the property "said" of "root" is "done whole; failed Error: broken; "
      And the property "status" of "task" is "failed"

  Rule: A Task is a Controller: its run goes with the item it is declared in

    Scenario: A Task's run is aborted when the item it is declared in is unloaded
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          objectName: "root"
          Loader {
            objectName: "loader"
            component: Item {
              Task {
                run: async (signal: AbortSignal): Promise<string> => {
                  signal.onabort = () => { console.warn("aborted " + String(signal.aborted)) }
                  return (await fetch("$SERVER/held", { signal })).text()
                }
              }
            }
          }
        }
        """
      When the property "active" of "loader" is set to false
      Then the log has a warning matching "^aborted true$"
      And the application has nothing scheduled

    Scenario: A delegate taken out of a view aborts its Task's run
      Given the QML:
        """qml
        import Ljos.Core
        import Ljos.Models
        import Ljos.Ui
        Item {
          id: root; objectName: "root"
          ListModel { id: rows
            ListElement { row: 0 }
            ListElement { row: 1 }
            ListElement { row: 2 }
          }
          Repeater {
            model: rows
            delegate: Item {
              required property int row
              Task {
                args: [row]
                run: async (row: number, signal: AbortSignal): Promise<string> => {
                  signal.onabort = () => { console.warn("row " + row + " aborted") }
                  return (await fetch("$SERVER/held", { signal })).text()
                }
              }
            }
          }
          function shrink(): void { rows.remove(1, 2) }
        }
        """
      When the method "shrink" of "root" is called
      Then the log has 2 warnings matching "^row [12] aborted$"
      And the log has no warning matching "^row 0 aborted$"

  Rule: The checker types a Task from its run

    Scenario: A Task's state taken apart with switch gives a value that is never undefined where it is done
      Given the file "Weather.qml":
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root
          property string city: "Oslo"
          Task {
            id: weather
            args: [root.city]
            run: async (name: string, signal: AbortSignal): Promise<string> =>
              (await fetch(`https://example.org/w/${name}`, { signal })).text()
          }
          function label(): string {
            const state = weather.state
            switch (state.status) {
              case "initial": return ""
              case "pending": return state.value ?? qsTr("Loading…")
              case "done": return state.value
              case "failed": return qsTr("No forecast")
            }
          }
          property bool loading: weather.status === "pending"
          property string last: weather.value ?? ""
          property string shown: label()
          function again() { weather.start("Bergen") }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: The checker holds args, start and what is read to run's types
      Given the file "Weather.qml":
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root
          property int count: 2
          Task {
            id: weather
            args: [root.count]
            run: async (name: string, signal: AbortSignal): Promise<string> => name
          }
          Task {
            id: two
            args: ["a", "b"]
            run: async (name: string, signal: AbortSignal): Promise<string> => name
          }
          property string last: weather.value
          property int length: weather.state.status === "done" ? weather.state.value : 0
          function again() { weather.start(3) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Weather.qml:8:20: error: `name` is a string; this gives it a number [ts-assign-type]
        Weather.qml:13:19: error: `run` takes 1 argument before its signal, and `args` gives it 2 [ts-assign-type]
        Weather.qml:16:33: error: `last` is a string; this gives it a string or undefined [ts-assign-type]
        Weather.qml:17:32: error: `length` is an int; this gives it a string or a number [ts-assign-type]
        Weather.qml:18:44: error: `name` is a string; this gives it a number [ts-assign-type]
        1 files, 5 errors, 0 warnings
        ljos: 5 errors
        """

    @headless
    Scenario: start is given none of run's arguments, or all it needs
      Given the file "Weather.qml":
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          Task {
            id: weather
            autoRun: false
            run: async (city: string, unit: string, signal: AbortSignal): Promise<string> => city + unit
          }
          function own() { weather.start() }
          function all() { weather.start("Oslo", "C") }
          function part() { weather.start("Oslo") }
        }
        """
      When the files are checked
      Then the check reports:
        """
        Weather.qml:11:43: error: `start` is given none of `run`'s arguments or all it needs, 2, and is given 1 [ts-assign-type]
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

    @headless
    Scenario: TypeScript gives start none of run's arguments, or all of them
      Given the file "Weather.qml":
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          Task {
            id: weather
            autoRun: false
            run: async (city: string, unit: string, signal: AbortSignal): Promise<string> => city + unit
          }
          function own() { weather.start() }
          function all() { weather.start("Oslo", "C") }
          function part() { weather.start("Oslo") }
        }
        """
      When the files are checked with TypeScript
      Then the check reports the lines:
        """
        Weather.qml:11:43: error: `start` is given none of `run`'s arguments or all it needs, 2, and is given 1 [ts-assign-type]
        """
      And TypeScript reports the lines:
        """
        Weather.qml:11:43: error: Argument of type '["Oslo"]' is not assignable to parameter of type '[] | [city: string, unit: string]'. [TS2345]
        """

    @headless
    Scenario: TypeScript reads a Task as the checker does
      Given the file "Weather.qml":
        """qml
        import Ljos.Core
        import Ljos.Ui
        Item {
          id: root
          property string city: "Oslo"
          Task {
            id: weather
            args: [root.city]
            run: async (name: string, signal: AbortSignal): Promise<string> =>
              (await fetch(`https://example.org/w/${name}`, { signal })).text()
          }
          function label(): string {
            const state = weather.state
            switch (state.status) {
              case "initial": return ""
              case "pending": return state.value ?? qsTr("Loading…")
              case "done": return state.value
              case "failed": return qsTr("No forecast")
            }
          }
          property string last: weather.value
        }
        """
      When the files are checked with TypeScript
      Then the check reports the lines:
        """
        Weather.qml:21:33: error: `last` is a string; this gives it a string or undefined [ts-assign-type]
        """
      And TypeScript reports the lines:
        """
        Weather.qml:21:33: error: Type 'string | undefined' is not assignable to type 'string'. [TS2322]
        """
