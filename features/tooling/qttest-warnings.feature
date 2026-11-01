# Sources:
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html#ignoreWarning-method
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html#failOnWarning-method
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @headless
Feature: Warnings a test expects, and warnings it refuses
  `ignoreWarning` says a warning will come: it is not printed, and the test function fails if it
  never came. `failOnWarning` says a warning must not come. Both hold for the function that said so,
  from its `init` to its `cleanup`.

  Rule: An expected warning is not printed, and it has to come

    Scenario Outline: A warning that was expected is not printed
      Given the file "tst_warn.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Warn"
          function test_a() { ignoreWarning(<expected>); console.warn("expected") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Warn::initTestCase()
        PASS   : Warn::test_a()
        PASS   : Warn::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """
      And the test run exits with status 0

      Examples:
        | expected   |
        | "expected" |
        | /exp.*d/   |
        | /EXPECTED/i |

    Scenario: A warning the engine gives can be expected by its whole text
      Given the file "tst_engine.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Engine"
          property int wrong: 0
          function test_a() {
            ignoreWarning(Qt.resolvedUrl("tst_engine.qml") + ":8: ReferenceError: nope is not defined")
            wrong = Qt.binding(function () { return nope })
            wrong
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Engine::test_a()
        """
      And the test run exits with status 0

    Scenario: A text is the warning's whole text, not a part of it
      Given the file "tst_part.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Part"
          function test_a() { ignoreWarning("expec"); console.warn("expected") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Part::initTestCase()
        QWARN  : Part::test_a() qml: expected
        INFO   : Part::test_a() Did not receive message: "expec"
        FAIL!  : Part::test_a() Not all expected messages were received
        PASS   : Part::cleanupTestCase()
        Totals: 2 passed, 1 failed, 0 skipped
        """
      And the test run exits with status 1

    Scenario: An expected warning that never comes fails the function, saying which it was
      Given the file "tst_never.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Never"
          function test_regexp() { ignoreWarning(/ne+ver/) }
          function test_text() { ignoreWarning("never") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Never::initTestCase()
        INFO   : Never::test_regexp() Did not receive any message matching: "ne+ver"
        FAIL!  : Never::test_regexp() Not all expected messages were received
        INFO   : Never::test_text() Did not receive message: "never"
        FAIL!  : Never::test_text() Not all expected messages were received
        PASS   : Never::cleanupTestCase()
        Totals: 2 passed, 2 failed, 0 skipped
        """
      And the test run exits with status 2

    Scenario: A function that failed already is not told about what never came
      Given the file "tst_failed.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Failed"
          function test_a() { ignoreWarning("never"); verify(false, "its own") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Failed::initTestCase()
        FAIL!  : Failed::test_a() 'its own' returned FALSE. ()
           Loc: [tst_failed.qml(5)]
        PASS   : Failed::cleanupTestCase()
        Totals: 2 passed, 1 failed, 0 skipped
        """

    Scenario: One expectation takes one warning; two take two
      Given the file "tst_count.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Count"
          function test_once() { ignoreWarning("t"); console.warn("t"); console.warn("t") }
          function test_twice() { ignoreWarning("t"); ignoreWarning("t"); console.warn("t"); console.warn("t") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Count::initTestCase()
        QWARN  : Count::test_once() qml: t
        PASS   : Count::test_once()
        PASS   : Count::test_twice()
        PASS   : Count::cleanupTestCase()
        Totals: 4 passed, 0 failed, 0 skipped
        """

    Scenario: Only warnings can be expected
      Given the file "tst_kinds.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Kinds"
          function test_critical() { ignoreWarning("c"); console.error("c") }
          function test_log() { ignoreWarning("l"); console.log("l") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Kinds::initTestCase()
        QCRITICAL: Kinds::test_critical() qml: c
        INFO   : Kinds::test_critical() Did not receive message: "c"
        FAIL!  : Kinds::test_critical() Not all expected messages were received
        QDEBUG : Kinds::test_log() qml: l
        INFO   : Kinds::test_log() Did not receive message: "l"
        FAIL!  : Kinds::test_log() Not all expected messages were received
        PASS   : Kinds::cleanupTestCase()
        Totals: 2 passed, 2 failed, 0 skipped
        """

    Scenario: An expectation lasts from init to cleanup of one row, and no further
      Given the file "tst_scope.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Scope"
          function init() { ignoreWarning("from init") }
          function cleanup() { console.warn("from cleanup") }
          function test_a_data() { return [{ tag: "one" }, { tag: "two" }] }
          function test_a(row) {
            ignoreWarning("from cleanup")
            console.warn("from init")
            if (row.tag === "one") ignoreWarning("in one")
            console.warn("in " + row.tag)
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Scope::initTestCase()
        PASS   : Scope::test_a(one)
        QWARN  : Scope::test_a(two) qml: in two
        PASS   : Scope::test_a(two)
        PASS   : Scope::cleanupTestCase()
        Totals: 4 passed, 0 failed, 0 skipped
        """

    Scenario: A warning a timer gives while the test waits counts
      Given the file "tst_late.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Late"
          Timer { id: timer; interval: 10; onTriggered: console.warn("late") }
          function test_a() { ignoreWarning("late"); timer.start(); wait(50) }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Late::test_a()
        """
      And the test run exits with status 0

    Scenario: A skipped function that expected a warning says so, and stays skipped
      Given the file "tst_skip.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Skip"
          function test_a() { ignoreWarning("never"); skip("not here") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Skip::initTestCase()
        SKIP   : Skip::test_a() not here
           Loc: [tst_skip.qml(5)]
        INFO   : Skip::test_a() Did not receive message: "never"
        FAIL!  : Skip::test_a() Not all expected messages were received
        PASS   : Skip::cleanupTestCase()
        Totals: 2 passed, 0 failed, 1 skipped
        """

  Rule: A warning that is refused fails the function, which goes on

    Scenario: Each warning that matches is a failure, and the function runs to its end
      Given the file "tst_refuse.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Refuse"
          function test_a() {
            failOnWarning(/x/)
            console.warn("axb")
            console.warn("other")
            console.log("after")
          }
          function test_gone() { console.warn("axb") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Refuse::initTestCase()
        FAIL!  : Refuse::test_a() Received a warning that resulted in a failure:
        axb
           Loc: [file://tst_refuse.qml(7)]
        QWARN  : Refuse::test_a() qml: other
        QDEBUG : Refuse::test_a() qml: after
        QWARN  : Refuse::test_gone() qml: axb
        PASS   : Refuse::test_gone()
        PASS   : Refuse::cleanupTestCase()
        Totals: 3 passed, 1 failed, 0 skipped
        """

    Scenario: A warning that is both expected and refused is expected
      Given the file "tst_both.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Both"
          function test_a() { failOnWarning("x"); ignoreWarning("x"); console.warn("x") }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Both::test_a()
        """

    Scenario Outline: Refusing no text refuses nothing
      Given the file "tst_nothing.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Nothing"
          function test_a() { failOnWarning(<refused>); console.warn("x") }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        QWARN  : Nothing::test_a() qml: x
        PASS   : Nothing::test_a()
        """

      Examples:
        | refused |
        | ""      |
        |         |
