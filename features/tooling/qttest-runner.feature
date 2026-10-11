# Sources:
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html
#   https://doc.qt.io/qt-6/qtquicktest-index.html
#   docs/ARCHITECTURE.md sections 17 and 18.1
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

@qt-parity @headless
Feature: Running QML test files
  The `tst_*.qml` files an application wrote for Qt Quick Test run
  unchanged. Every `TestCase` in them is run function by function, and the
  result is printed in the shape Qt's own runner prints it in.

  Rule: Every test function is run and reported

    Scenario: A passing function is reported between the start and the end of its case
      Given the file "tst_math.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Math"
          function test_add() { compare(1 + 1, 2) }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Math::initTestCase()
        PASS   : Math::test_add()
        PASS   : Math::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """
      And the test run exits with status 0

    Scenario: Functions run in the order of their names, capitals first
      Given the file "tst_order.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Order"
          function test_b() {}
          function test_a() {}
          function test_C() {}
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Order::initTestCase()
        PASS   : Order::test_C()
        PASS   : Order::test_a()
        PASS   : Order::test_b()
        PASS   : Order::cleanupTestCase()
        Totals: 5 passed, 0 failed, 0 skipped
        """

    Scenario: Only functions whose names start with test_ are tests
      Given the file "tst_names.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Names"
          function helper() { fail("a helper is not a test") }
          function testing() { fail("nor is this") }
          function test_one() { verify(true) }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Names::initTestCase()
        PASS   : Names::test_one()
        PASS   : Names::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """

    Scenario: A test case without a name is reported by its functions alone
      Given the file "tst_plain.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          function test_one() {}
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : initTestCase()
        PASS   : test_one()
        PASS   : cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """

  Rule: A function that fails does not stop the ones after it

    Scenario: The failure is reported with its file and line, and the next function runs
      Given the file "tst_two.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Two"
          function test_a() {
            compare(1 + 1, 3)
            fail("not reached")
          }
          function test_b() { verify(true) }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Two::initTestCase()
        FAIL!  : Two::test_a() Compared values are not the same
           Actual   (): 2
           Expected (): 3
           Loc: [tst_two.qml(6)]
        PASS   : Two::test_b()
        PASS   : Two::cleanupTestCase()
        Totals: 3 passed, 1 failed, 0 skipped
        """
      And the test run exits with status 1

    Scenario: A function that throws is a failed test and the next function runs
      Given the file "tst_throw.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Throw"
          function test_a() {
            throw new Error("boom")
          }
          function test_b() { verify(true) }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Throw::initTestCase()
        FAIL!  : Throw::test_a() Uncaught exception: boom
           Loc: [tst_throw.qml(6)]
        PASS   : Throw::test_b()
        PASS   : Throw::cleanupTestCase()
        Totals: 3 passed, 1 failed, 0 skipped
        """

    Scenario: The exit status is the number of failed tests
      Given the file "tst_three.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Three"
          function test_a() { fail("first") }
          function test_b() { fail("second") }
          function test_c() { fail("third") }
          function test_d() {}
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        Totals: 3 passed, 3 failed, 0 skipped
        """
      And the test run exits with status 3

    Scenario: A failed check fails the function even when the test catches what it throws
      Given the file "tst_caught.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Caught"
          function test_a() {
            try { compare(1, 2) } catch (e) { console.log("went on") }
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Caught::initTestCase()
        FAIL!  : Caught::test_a() Compared values are not the same
           Actual   (): 1
           Expected (): 2
           Loc: [tst_caught.qml(6)]
        QDEBUG : Caught::test_a() qml: went on
        PASS   : Caught::cleanupTestCase()
        Totals: 2 passed, 1 failed, 0 skipped
        """

    Scenario: An exception that the test catches itself is not a failure
      Given the file "tst_caught.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Caught"
          function test_a() {
            let caught = ""
            try { throw new Error("boom") } catch (e) { caught = e.message }
            compare(caught, "boom")
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Caught::test_a()
        """

  Rule: init and cleanup run around every function, initTestCase and cleanupTestCase around the case

    Scenario: The four special functions run in their places
      Given the file "tst_steps.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Steps"
          function initTestCase() { console.log("start") }
          function cleanupTestCase() { console.log("end") }
          function init() { console.log("before") }
          function cleanup() { console.log("after") }
          function test_a() { console.log("a") }
          function test_b() { console.log("b") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        QDEBUG : Steps::initTestCase() qml: start
        PASS   : Steps::initTestCase()
        QDEBUG : Steps::test_a() qml: before
        QDEBUG : Steps::test_a() qml: a
        QDEBUG : Steps::test_a() qml: after
        PASS   : Steps::test_a()
        QDEBUG : Steps::test_b() qml: before
        QDEBUG : Steps::test_b() qml: b
        QDEBUG : Steps::test_b() qml: after
        PASS   : Steps::test_b()
        QDEBUG : Steps::cleanupTestCase() qml: end
        PASS   : Steps::cleanupTestCase()
        Totals: 4 passed, 0 failed, 0 skipped
        """

    Scenario: When initTestCase fails no function runs, and cleanupTestCase still does
      Given the file "tst_start.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Start"
          function initTestCase() { fail("nothing can run") }
          function cleanupTestCase() { console.log("tidied") }
          function test_a() {}
        }
        """
      When the tests are run
      Then the test report is:
        """
        FAIL!  : Start::initTestCase() nothing can run
           Loc: [tst_start.qml(5)]
        QDEBUG : Start::cleanupTestCase() qml: tidied
        PASS   : Start::cleanupTestCase()
        Totals: 1 passed, 1 failed, 0 skipped
        """

    Scenario: A failing cleanup fails the function it ran after
      Given the file "tst_tidy.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Tidy"
          property int runs: 0
          function cleanup() {
            runs += 1
            verify(runs !== 2, "left a mess")
          }
          function test_a() {}
          function test_b() {}
          function test_c() {}
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Tidy::initTestCase()
        PASS   : Tidy::test_a()
        FAIL!  : Tidy::test_b() 'left a mess' returned FALSE. ()
           Loc: [tst_tidy.qml(8)]
        PASS   : Tidy::test_c()
        PASS   : Tidy::cleanupTestCase()
        Totals: 4 passed, 1 failed, 0 skipped
        """

    Scenario: A function still runs after a failing init, and is failed
      Given the file "tst_before.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Before"
          function init() { fail("not ready") }
          function test_a() { console.log("ran anyway") }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Before::initTestCase()
        FAIL!  : Before::test_a() not ready
           Loc: [tst_before.qml(5)]
        QDEBUG : Before::test_a() qml: ran anyway
        PASS   : Before::cleanupTestCase()
        Totals: 2 passed, 1 failed, 0 skipped
        """

  Rule: A function with data runs once for every row

    Scenario: Each row is reported under its tag, or under its number
      Given the file "tst_rows.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Rows"
          function test_double_data() {
            return [
              { tag: "small", n: 1, twice: 2 },
              { tag: "negative", n: -3, twice: -6 },
              { n: 0, twice: 0 },
            ]
          }
          function test_double(row) { compare(row.n * 2, row.twice) }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Rows::initTestCase()
        PASS   : Rows::test_double(small)
        PASS   : Rows::test_double(negative)
        PASS   : Rows::test_double(row 2)
        PASS   : Rows::cleanupTestCase()
        Totals: 5 passed, 0 failed, 0 skipped
        """

    Scenario: A failing row does not stop the rows after it
      Given the file "tst_rows.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Rows"
          function test_even_data() {
            return [{ tag: "two", n: 2 }, { tag: "three", n: 3 }, { tag: "four", n: 4 }]
          }
          function test_even(row) {
            compare(row.n % 2, 0)
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Rows::initTestCase()
        PASS   : Rows::test_even(two)
        FAIL!  : Rows::test_even(three) Compared values are not the same
           Actual   (): 1
           Expected (): 0
           Loc: [tst_rows.qml(9)]
        PASS   : Rows::test_even(four)
        PASS   : Rows::cleanupTestCase()
        Totals: 4 passed, 1 failed, 0 skipped
        """

    Scenario: init_data gives rows to every function that has none of its own
      Given the file "tst_shared.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Shared"
          function init_data() { return [{ tag: "x", v: 1 }, { tag: "y", v: 2 }] }
          function test_own_data() { return [{ tag: "mine", v: 7 }] }
          function test_own(row) { compare(row.v, 7) }
          function test_positive(row) { verify(row.v > 0) }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Shared::initTestCase()
        PASS   : Shared::test_own(mine)
        PASS   : Shared::test_positive(x)
        PASS   : Shared::test_positive(y)
        PASS   : Shared::cleanupTestCase()
        Totals: 5 passed, 0 failed, 0 skipped
        """

  Rule: A test can be skipped, or be expected to fail

    Scenario: A skipped function is reported with the reason and the place
      Given the file "tst_skip.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Skip"
          function test_a() {
            skip("not on this machine")
            fail("not reached")
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Skip::initTestCase()
        SKIP   : Skip::test_a() not on this machine
           Loc: [tst_skip.qml(6)]
        PASS   : Skip::cleanupTestCase()
        Totals: 2 passed, 0 failed, 1 skipped
        """
      And the test run exits with status 0

    Scenario: An expected failure is reported and the function passes
      Given the file "tst_known.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Known"
          function test_a() {
            expectFail("", "a known difference")
            compare(1, 2)
            fail("not reached")
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Known::initTestCase()
        XFAIL  : Known::test_a() a known difference
           Loc: [tst_known.qml(7)]
        PASS   : Known::test_a()
        PASS   : Known::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """

    Scenario: A failure expected for one row is not expected for the others
      Given the file "tst_known.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Known"
          function test_a_data() { return [{ tag: "good", n: 1 }, { tag: "bad", n: 2 }] }
          function test_a(row) {
            expectFail("bad", "the second row is known")
            compare(row.n, 1)
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Known::initTestCase()
        PASS   : Known::test_a(good)
        XFAIL  : Known::test_a(bad) the second row is known
           Loc: [tst_known.qml(8)]
        PASS   : Known::test_a(bad)
        PASS   : Known::cleanupTestCase()
        Totals: 4 passed, 0 failed, 0 skipped
        """

    Scenario: A check that passes although it was expected to fail is a failure
      Given the file "tst_fixed.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Fixed"
          function test_a() {
            expectFail("", "was broken once")
            compare(1, 1)
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Fixed::initTestCase()
        XPASS  : Fixed::test_a() QCOMPARE(, ) returned TRUE unexpectedly.
           Loc: [tst_fixed.qml(7)]
        PASS   : Fixed::cleanupTestCase()
        Totals: 2 passed, 1 failed, 0 skipped
        """
      And the test run exits with status 1

  Rule: What a test logs is part of the report

    Scenario: Console output is printed under the function that wrote it
      Given the file "tst_log.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Log"
          function test_a() {
            console.log("said")
            console.info("told")
            console.warn("warned")
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Log::initTestCase()
        QDEBUG : Log::test_a() qml: said
        QINFO  : Log::test_a() qml: told
        QWARN  : Log::test_a() qml: warned
        PASS   : Log::test_a()
        PASS   : Log::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """
