# Sources:
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html
#   https://doc.qt.io/qt-6/qml-qttest-signalspy.html
#   docs/ARCHITECTURE.md sections 5.5 (the type description feeds the compiler), 9.4 (ErrorBoundary) and 19.3
# `ljos check` is Ljós's own program, and `Ljos` its own module: Qt has nothing to compare them with.
# A reported column counts the characters of the line as it is in the file, and a file given here is
# written with the eight spaces it is indented by: the first visible column is 9.

@ljos-only @headless
Feature: Checking files that use the modules Ljós adds
  `ljos check` reads a project without running it. It knows the types of
  `QtTest` and of `Ljos` as it knows Qt's: by the properties, signals and
  functions the types themselves declare, so a name that is none of them
  is found before anything runs.

  Rule: The types of QtTest are known with their members

    Scenario: A test case with a signal spy has nothing to report
      Given the file "tst_poke.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Poke"
          when: windowShown
          Item { id: box; signal poked }
          SignalSpy { id: spy; target: box; signalName: "poked" }
          function test_poke() {
            box.poked()
            compare(spy.count, 1)
            spy.clear()
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A misspelt property of a test case is reported
      Given the file "tst_case.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Case"
          wen: true
        }
        """
      When the files are checked
      Then the check reports:
        """
        tst_case.qml:5:11: warning: `wen` is not a property of `TestCase`; did you mean `when`? [unknown-property]
        1 files, 0 errors, 1 warnings
        """

    Scenario: A misspelt property of a signal spy is reported
      Given the file "tst_spy.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Spy"
          SignalSpy { signalNam: "poked" }
        }
        """
      When the files are checked
      Then the check reports:
        """
        tst_spy.qml:5:23: warning: `signalNam` is not a property of `SignalSpy`; did you mean `signalName`? [unknown-property]
        1 files, 0 errors, 1 warnings
        """

    Scenario: A handler of a signal a signal spy does not have is reported
      Given the file "tst_spy.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Spy"
          SignalSpy { onCountChange: console.log(count) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        tst_spy.qml:5:23: warning: `onCountChange` handles the signal `countChange`, which `SignalSpy` does not have; did you mean `onCountChanged`? [unknown-signal]
        1 files, 0 errors, 1 warnings
        """

    Scenario: A function a test case does not have is not a name in sight
      Given the file "tst_case.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Case"
          function test_sum() { compre(1 + 1, 2) }
        }
        """
      When the files are checked
      Then the check reports:
        """
        tst_case.qml:5:33: warning: `compre` is not a name in sight: it is looked up when the program runs
        1 files, 0 errors, 1 warnings
        """

    Scenario: A test case is no type of a file that does not import QtTest
      Given the file "tst_case.qml":
        """qml
        import QtQuick
        TestCase {
          name: "Case"
        }
        """
      When the files are checked
      Then the check reports:
        """
        tst_case.qml:2:9: error: `TestCase` is not a type of anything the file imports
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

  Rule: The types of Ljos are known with their members

    Scenario: An error boundary with a fallback has nothing to report
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Ljos
        Item {
          ErrorBoundary {
            id: boundary
            maxRestarts: 2
            within: 1000
            catchExceptions: true
            onFailedChanged: console.log(failed, fault.kind, fault.line, restarts)
            fallback: Rectangle {
              Timer { interval: 500; running: true; onTriggered: boundary.retry() }
            }
            Rectangle { }
          }
        }
        """
      When the files are checked
      Then the check reports:
        """
        1 files, 0 errors, 0 warnings
        """

    Scenario: A misspelt property of an error boundary is reported
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Ljos
        ErrorBoundary {
          maxRestart: 2
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:4:11: warning: `maxRestart` is not a property of `ErrorBoundary`; did you mean `maxRestarts`? [unknown-property]
        1 files, 0 errors, 1 warnings
        """

    Scenario: A misspelt part of an error boundary's fault is reported
      Given the file "Main.qml":
        """qml
        import QtQuick
        import Ljos
        ErrorBoundary {
          fault.mesage: "none"
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:4:11: warning: `mesage` is not a property of `fault`; did you mean `message`? [unknown-property]
        1 files, 0 errors, 1 warnings
        """

    Scenario: An error boundary is no type of a file that does not import Ljos
      Given the file "Main.qml":
        """qml
        import QtQuick
        ErrorBoundary { }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:2:9: error: `ErrorBoundary` is not a type of anything the file imports
        1 files, 1 errors, 0 warnings
        ljos: 1 errors
        """

  Rule: A misspelt member of one of Qt's types is reported the same way

    Scenario: A misspelt property of a rectangle is reported
      Given the file "Main.qml":
        """qml
        import QtQuick
        Rectangle {
          colr: "red"
        }
        """
      When the files are checked
      Then the check reports:
        """
        Main.qml:3:11: warning: `colr` is not a property of `Rectangle`; did you mean `color`? [unknown-property]
        1 files, 0 errors, 1 warnings
        """
