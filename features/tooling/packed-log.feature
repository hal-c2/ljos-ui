# Sources:
#   docs/ARCHITECTURE.md principle 6 (0 % CPU when idle)
#   docs/site/rust/overview.md (starting an application with `ljos::App`)
#   https://doc.qt.io/qt-6/qtquick-debugging.html (console API)
# An application shipped as a program of its own is Ljós's: Qt's nearest,
# a program with a QML engine and the default message handler, also writes
# its messages to stderr, with no level in front of them.

@ljos-only @headless
Feature: Where a packed application's log goes
  An application shipped as a program of its own, its QML compiled into a
  pack, writes what it logs to its errors: `console.log`, `console.warn`
  and the warnings of the runtime, a line each after its level, as
  `ljos run` writes them. A program that wants the log for itself sets a
  log handler of its own, which stderr then never sees, or keeps the log
  in the engine. A handler set on the engine as the program starts wins
  over both. The errors are written by a thread of their own, started by
  the first line: a program whose errors nobody reads, or that has none,
  goes on as if they were read, and one that logs nothing starts no such
  thread and stays asleep.

  Background:
    Given the file "Main.qml":
      """qml
      import QtQuick
      Item {
          Timer {
              interval: 10; running: true
              onTriggered: {
                  console.log("hello", 1 + 1)
                  console.warn("careful")
                  Qt.quit()
              }
          }
      }
      """

  Rule: The log goes to stderr unless the program says otherwise

    Scenario: What it logs is on its errors after its level
      When the packed application runs
      Then the packed application ended within 10 seconds
      And the packed application's errors mention "debug: hello 2"
      And the packed application's errors mention "warning: careful"

    Scenario: `ljos run` writes the same lines
      When the author runs "ljos run Main.qml --headless"
      Then the command exits with status 0
      And the command's output mentions "debug: hello 2" once
      And the command's output mentions "warning: careful" once

    Scenario: `ljos run` with an inspector writes each line once
      When the author runs "ljos run Main.qml --headless --mcp=inspect"
      Then the command exits with status 0
      And the command's output mentions "debug: hello 2" once
      And the command's output mentions "warning: careful" once

    Scenario: A handler of the program's own has the log, and stderr none of it
      When the packed application runs with its own log handler
      Then the packed application's output mentions "heard debug: hello 2"
      And the packed application's output mentions "heard warning: careful"
      And the packed application's errors do not mention "hello"
      And the packed application's errors do not mention "careful"

    Scenario: A handler set on the engine as it starts wins over stderr
      When the packed application runs with a log handler set as it starts
      Then the packed application's output mentions "heard debug: hello 2"
      And the packed application's errors do not mention "hello"

    Scenario: A program that keeps its log writes none of it
      When the packed application runs with its log kept
      Then the packed application's output mentions "kept 2 entries"
      And the packed application's errors do not mention "hello"

  Rule: Stderr never holds the application up

    Background:
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Timer {
                interval: 10; running: true
                onTriggered: {
                    const line = "x".repeat(200)
                    for (let i = 0; i < 20000; i++)
                        console.log(i, line)
                    Qt.quit()
                }
            }
        }
        """

    Scenario: Errors nobody reads
      When the packed application runs with its errors never read
      Then the packed application ended within 10 seconds

    Scenario: Errors that are closed
      When the packed application runs with its errors closed
      Then the packed application ended within 10 seconds

    Scenario: What stderr could not take is counted where it was lost
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            Component.onCompleted: {
                const line = "x".repeat(200)
                for (let i = 0; i < 20000; i++)
                    console.log(i, line)
            }
            Timer { interval: 4000; running: true; onTriggered: Qt.quit() }
        }
        """
      When the packed application runs with its errors read late
      Then the packed application ended within 10 seconds
      And the packed application's errors mention "lines of the log dropped"

  Rule: An application that logs nothing costs nothing

    Scenario: Nothing logged, nothing started
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item { width: 100; height: 100 }
        """
      When the author starts the packed application
      # Past the heap's trim, a second after the start's last work.
      And 2000 milliseconds pass
      Then the application has started no thread to write its errors
      And the application's errors are empty
      When the wakes of the application's threads are counted
      And 2000 milliseconds pass
      Then the window's thread woke at most 0 times since the wakes were counted
      And no thread but the window's woke since they were counted

    Scenario: After its one line it sleeps
      Given the file "Main.qml":
        """qml
        import QtQuick
        Item {
            width: 100; height: 100
            Component.onCompleted: console.log("ready")
        }
        """
      When the author starts the packed application
      # Past the heap's trim, a second after the start's last work.
      And 2000 milliseconds pass
      Then the application has started a thread to write its errors
      And the application's errors mention "debug: ready"
      When the wakes of the application's threads are counted
      And 2000 milliseconds pass
      Then the window's thread woke at most 0 times since the wakes were counted
      And no thread but the window's woke since they were counted
