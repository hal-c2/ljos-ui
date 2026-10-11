# Sources:
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html
#   https://doc.qt.io/qt-6/qml-qttest-signalspy.html
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @headless
Feature: The checks of a TestCase
  `compare`, `verify`, `fuzzyCompare`, `tryCompare`, `tryVerify` and `fail`
  end the test function when they fail, and say what was wrong in the
  words Qt's runner uses.

  Rule: compare takes values of the same kind that are equal

    Scenario Outline: <actual> is the same as <expected>
      Given the file "tst_same.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Same"
          Rectangle { id: box; color: "red" }
          function test_it() { compare(<actual>, <expected>) }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Same::test_it()
        """

      Examples:
        | actual          | expected        |
        | 1 + 1           | 2               |
        | 0.1 + 0.2       | 0.3             |
        | "a" + "b"       | "ab"            |
        | [1, [2, 3]]     | [1, [2, 3]]     |
        | ({ a: 1 })      | ({ a: 1 })      |
        | Qt.point(1, 2)  | Qt.point(1, 2)  |
        | box.color       | "#ff0000"       |
        | box             | box             |
        | null            | null            |
        | undefined       | undefined       |
        | NaN             | NaN             |

    Scenario Outline: <actual> is not the same as <expected>
      Given the file "tst_differ.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Differ"
          Rectangle { id: box; color: "red" }
          Item { id: other }
          function test_it() {
            compare(<actual>, <expected>)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        FAIL!  : Differ::test_it() Compared values are not the same
           Actual   (): <printed actual>
           Expected (): <printed expected>
           Loc: [tst_differ.qml(8)]
        """

      Examples:
        | actual          | expected       | printed actual | printed expected |
        | 1 + 1           | 3              | 2              | 3                |
        | "1"             | 1              | 1              | 1                |
        | "1.0"           | "1"            | 1.0            | 1                |
        | [1, 2, 3]       | [1, 2, 4]      | [1,2,3]        | [1,2,4]          |
        | [1, 2]          | [1, 2, 3]      | [1,2]          | [1,2,3]          |
        | ({ a: 1 })      | ({ a: 2 })     | [object Object] | [object Object] |
        | Qt.point(1, 2)  | Qt.point(1, 3) | QPointF(1, 2)  | QPointF(1, 3)    |
        | box.color       | "blue"         | #ff0000        | blue             |
        | true            | 1              | true           | 1                |
        | null            | undefined      | null           | undefined        |
        | 1               | 1.001          | 1              | 1.001            |

    Scenario: The message given to compare replaces the usual one
      Given the file "tst_message.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Message"
          function test_it() {
            compare("left", "right", "the two sides differ")
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        FAIL!  : Message::test_it() the two sides differ
           Actual   (): left
           Expected (): right
           Loc: [tst_message.qml(6)]
        """

  Rule: fuzzyCompare allows a difference

    Scenario Outline: fuzzyCompare(<actual>, <expected>, <delta>) passes
      Given the file "tst_near.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Near"
          function test_it() { fuzzyCompare(<actual>, <expected>, <delta>) }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Near::test_it()
        """

      Examples:
        | actual              | expected  | delta |
        | 1.0                 | 1.04      | 0.05  |
        | 10                  | 12        | 2     |
        | -1                  | 1         | 2     |
        | Qt.rgba(1, 0, 0, 1) | "#fd0102" | 2     |

    Scenario Outline: fuzzyCompare(<actual>, <expected>, <delta>) fails
      Given the file "tst_far.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Far"
          function test_it() {
            fuzzyCompare(<actual>, <expected>, <delta>)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        FAIL!  : Far::test_it() Compared values are not the same with delta(<delta>)
           Actual   (): <printed actual>
           Expected (): <printed expected>
           Loc: [tst_far.qml(6)]
        """

      Examples:
        | actual              | expected  | delta | printed actual | printed expected |
        | 1.0                 | 1.5       | 0.25  | 1              | 1.5              |
        | 10                  | 13        | 2     | 10             | 13               |
        | Qt.rgba(1, 0, 0, 1) | "#f00500" | 2     | #ff0000        | #f00500          |

  Rule: verify, fail, tryVerify and tryCompare say what did not hold

    Scenario Outline: <check> fails with its own words
      Given the file "tst_fails.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Fails"
          property int n: 1
          function test_it() {
            <check>
            console.log("not reached")
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Fails::initTestCase()
        FAIL!  : Fails::test_it() <says>
           Loc: [tst_fails.qml(8)]
        PASS   : Fails::cleanupTestCase()
        Totals: 2 passed, 1 failed, 0 skipped
        """

      Examples:
        | check                                               | says                                          |
        | verify(false)                                       | 'verify()' returned FALSE. ()                 |
        | verify(n > 1, "n is too small")                     | 'n is too small' returned FALSE. ()           |
        | verify(0)                                           | 'verify()' returned FALSE. ()                 |
        | fail("given up")                                    | given up                                      |
        | fail()                                              |                                               |
        | tryVerify(function() { return n > 1 }, 100)         | 'function returned false' returned FALSE. ()  |
        | tryVerify(function() { return n > 1 }, 100, "slow") | 'slow' returned FALSE. ()                     |

    Scenario Outline: A tryCompare that runs out of time fails like a compare
      Given the file "tst_never.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Never"
          property int n: 1
          function test_it() {
            <check>
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        FAIL!  : Never::test_it() <says>
           Actual   (): 1
           Expected (): 2
           Loc: [tst_never.qml(8)]
        """

      Examples:
        | check                                            | says             |
        | tryCompare(root, "n", 2, 100)                    | property n       |
        | tryCompare(root, "n", 2, 100, "n never became 2") | n never became 2 |

  Rule: tryCompare and tryVerify wait for what a timer or an animation brings

    Scenario: A value that a timer sets is waited for
      Given the file "tst_arrives.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Arrives"
          property int n: 1
          Timer { id: later; interval: 150; onTriggered: root.n = 2 }
          function test_compare() {
            later.start()
            compare(n, 1)
            tryCompare(root, "n", 2)
          }
          function test_verify() {
            n = 1
            later.start()
            tryVerify(function() { return n === 2 })
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Arrives::test_compare()
        PASS   : Arrives::test_verify()
        """

  Rule: A SignalSpy counts the emissions of a signal and keeps their arguments

    Scenario: Emissions are counted and their arguments kept in order
      Given the file "tst_spy.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Spy"
          signal sent(int number, string word)
          SignalSpy { id: spy; target: root; signalName: "sent" }
          function test_spy() {
            verify(spy.valid)
            compare(spy.count, 0)
            root.sent(1, "one")
            root.sent(2, "two")
            compare(spy.count, 2)
            compare(spy.signalArguments[1][0], 2)
            compare(spy.signalArguments[1][1], "two")
            spy.clear()
            compare(spy.count, 0)
            compare(spy.signalArguments.length, 0)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Spy::test_spy()
        """

    Scenario: wait returns when the signal arrives and fails the test when it does not
      Given the file "tst_spy.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Spy"
          signal ping()
          Timer { id: later; interval: 100; onTriggered: root.ping() }
          SignalSpy { id: spy; target: root; signalName: "ping" }
          function test_1_arrives() {
            later.start()
            spy.wait()
            compare(spy.count, 1)
          }
          function test_2_never() {
            spy.wait(100)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Spy::test_1_arrives()
        FAIL!  : Spy::test_2_never() 'wait for signal ping' returned FALSE. ()
           Loc: [tst_spy.qml(15)]
        """

    Scenario: A spy on a signal the target does not have is not valid
      Given the file "tst_spy.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Spy"
          SignalSpy { id: spy; target: root; signalName: "nothing" }
          function test_spy() {
            verify(!spy.valid)
            compare(spy.count, 0)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Spy::test_spy()
        """
