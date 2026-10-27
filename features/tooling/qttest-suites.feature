# Sources:
#   https://doc.qt.io/qt-6/qml-qttest-testcase.html
#   https://doc.qt.io/qt-6/qtquicktest-index.html
#   docs/ARCHITECTURE.md sections 17 and 18.1
# Verified on Qt 6.11.2 (qmltestrunner, offscreen).

Feature: Test files, test cases and what they import
  A run takes a directory: every `tst_*.qml` file in it is a test file,
  every `TestCase` in a file is run when it is ready, and a test file
  imports what an application file can, plus modules that exist for the
  tests alone.

  Rule: The test files of a directory are found and run in the order of their paths

    @qt-parity
    Scenario: Files whose names start with tst_ are run, also in directories below
      Given the file "tst_b.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase { name: "B"; function test_it() {} }
        """
      And the file "deep/tst_a.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase { name: "A"; function test_it() {} }
        """
      And the file "other.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase { name: "Other"; function test_it() {} }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : A::initTestCase()
        PASS   : A::test_it()
        PASS   : A::cleanupTestCase()
        PASS   : B::initTestCase()
        PASS   : B::test_it()
        PASS   : B::cleanupTestCase()
        Totals: 6 passed, 0 failed, 0 skipped
        """

    @qt-parity
    Scenario: A run in which no test case ran reports nothing at all
      Given the file "tst_empty.qml":
        """qml
        import QtQuick
        Item {}
        """
      When the tests are run
      Then the test report is:
        """
        """
      And the test run exits with status 0

    @qt-parity
    Scenario: A file that cannot be compiled is a failed test, and the other files still run
      Given the file "tst_a.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase { name: "Good"; function test_it() {} }
        """
      And the file "tst_broken.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase { name: "Broken"; function test_it() {
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Good::test_it()
        PASS   : Good::cleanupTestCase()
        """
      And the test report has the lines:
        """
           Loc: [tst_broken.qml(4)]
        Totals: 3 passed, 1 failed, 0 skipped
        """
      And the test run exits with status 1

    @ljos-only
    Scenario: The compile failure says what the compiler said
      Given the file "tst_broken.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Broken"
          nothing: 1
        }
        """
      When the tests are run
      Then the test report is:
        """
        FAIL!  : tst_broken::compile() Cannot assign to non-existent property "nothing"
           Loc: [tst_broken.qml(5)]
        Totals: 0 passed, 1 failed, 0 skipped
        """

  Rule: The test cases of a file run when they are ready, the last one first

    @qt-parity
    Scenario: Several test cases in one file run from the last to the first
      Given the file "tst_cases.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          TestCase { name: "First"; function test_it() {} }
          TestCase { name: "Second"; function test_it() {} }
          TestCase { name: "Third"; function test_it() {} }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Third::initTestCase()
        PASS   : Third::test_it()
        PASS   : Third::cleanupTestCase()
        PASS   : Second::initTestCase()
        PASS   : Second::test_it()
        PASS   : Second::cleanupTestCase()
        PASS   : First::initTestCase()
        PASS   : First::test_it()
        PASS   : First::cleanupTestCase()
        Totals: 9 passed, 0 failed, 0 skipped
        """

    @qt-parity
    Scenario: A test case that waits for the window runs after the ones that do not
      Given the file "tst_shown.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          width: 100; height: 100
          TestCase {
            name: "Shown"
            when: windowShown
            function test_it() { verify(windowShown) }
          }
          TestCase {
            name: "AtOnce"
            function test_it() {}
          }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : AtOnce::initTestCase()
        PASS   : AtOnce::test_it()
        PASS   : AtOnce::cleanupTestCase()
        PASS   : Shown::initTestCase()
        PASS   : Shown::test_it()
        PASS   : Shown::cleanupTestCase()
        Totals: 6 passed, 0 failed, 0 skipped
        """

    @qt-parity
    Scenario: A test case runs when what it waits for becomes true
      Given the file "tst_when.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          id: root
          property bool ready: false
          Timer { interval: 50; running: true; onTriggered: root.ready = true }
          TestCase {
            name: "Waits"
            when: root.ready
            function test_it() { verify(root.ready) }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Waits::test_it()
        """

    @qt-parity
    Scenario: The state of a test case can be read while it runs and after
      Given the file "tst_state.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          TestCase {
            id: first
            name: "A"
            function test_it() {
              verify(running)
              verify(!completed)
            }
          }
          TestCase {
            name: "B"
            function test_it() {
              verify(!first.running)
              verify(!first.completed)
            }
          }
          TestCase {
            name: "C"
            when: first.completed
            function test_it() { verify(!first.running) }
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : B::test_it()
        PASS   : B::cleanupTestCase()
        PASS   : A::initTestCase()
        PASS   : A::test_it()
        PASS   : A::cleanupTestCase()
        PASS   : C::initTestCase()
        PASS   : C::test_it()
        """

    @qt-parity
    Scenario: An optional test case that never becomes ready is left out
      Given the file "tst_optional.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          TestCase {
            name: "Never"
            when: false
            optional: true
            function test_it() {}
          }
          TestCase { name: "Always"; function test_it() {} }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Always::initTestCase()
        PASS   : Always::test_it()
        PASS   : Always::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """
      And the test run exits with status 0

    # Qt's runner waits for such a test case for ever. A run that cannot end
    # is of no use to a build, so Ljós ends it and says which case it was.
    @qt-diverges
    Scenario: A test case that never becomes ready is a failed test
      Given the file "tst_stuck.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          TestCase {
            name: "Stuck"
            when: false
            function test_it() {}
          }
          TestCase { name: "Fine"; function test_it() {} }
        }
        """
      When the tests are run
      Then the test report is:
        """
        PASS   : Fine::initTestCase()
        PASS   : Fine::test_it()
        PASS   : Fine::cleanupTestCase()
        FAIL!  : Stuck::initTestCase() 'when' never became true
           Loc: [tst_stuck.qml(4)]
        Totals: 3 passed, 1 failed, 0 skipped
        """
      And the test run exits with status 1

  Rule: A run can be narrowed to the functions asked for

    Background:
      Given the file "tst_pick.qml":
        """qml
        import QtQuick
        import QtTest
        Item {
          TestCase {
            name: "One"
            function test_a() {}
            function test_b_data() { return [{ tag: "low", n: 1 }, { tag: "high", n: 9 }] }
            function test_b(data) { verify(data.n > 0) }
          }
          TestCase {
            name: "Two"
            function test_a() {}
          }
        }
        """

    @qt-parity
    Scenario: One function of one test case
      When the tests matching "One::test_a" are run
      Then the test report is:
        """
        PASS   : One::initTestCase()
        PASS   : One::test_a()
        PASS   : One::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """

    @qt-parity
    Scenario: One row of a function
      When the tests matching "One::test_b:high" are run
      Then the test report is:
        """
        PASS   : One::initTestCase()
        PASS   : One::test_b(high)
        PASS   : One::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """

    @qt-parity
    Scenario: A name that no function has runs nothing and reports nothing
      When the tests matching "One::test_z" are run
      Then the test report is:
        """
        """

    @ljos-only
    Scenario: The name of a test case alone runs all of its functions
      When the tests matching "Two" are run
      Then the test report is:
        """
        PASS   : Two::initTestCase()
        PASS   : Two::test_a()
        PASS   : Two::cleanupTestCase()
        Totals: 3 passed, 0 failed, 0 skipped
        """

    @ljos-only
    Scenario: The name of a function alone runs it in every test case
      When the tests matching "test_a" are run
      Then the test report is:
        """
        PASS   : Two::initTestCase()
        PASS   : Two::test_a()
        PASS   : Two::cleanupTestCase()
        PASS   : One::initTestCase()
        PASS   : One::test_a()
        PASS   : One::cleanupTestCase()
        Totals: 6 passed, 0 failed, 0 skipped
        """

  Rule: A test file imports its neighbours and the modules of the import paths given

    @qt-parity
    Scenario: A type in a file next to the test file needs no import
      Given the file "Doubler.qml":
        """qml
        import QtQuick
        QtObject {
          property int input: 0
          readonly property int output: input * 2
        }
        """
      And the file "tst_near.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Near"
          Doubler { id: doubler; input: 4 }
          function test_it() { compare(doubler.output, 8) }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Near::test_it()
        """

    @qt-parity
    Scenario: A run of the tests' directory imports a directory above it by its relative path
      Given the file "qml/Doubler.qml":
        """qml
        import QtQuick
        QtObject {
          property int input: 0
          readonly property int output: input * 2
        }
        """
      And the file "tests/tst_up.qml":
        """qml
        import QtQuick
        import QtTest
        import "../qml"
        TestCase {
          name: "Up"
          Doubler { id: doubler; input: 5 }
          function test_it() { compare(doubler.output, 10) }
        }
        """
      When the tests in "tests" are run
      Then the test report has the lines:
        """
        PASS   : Up::test_it()
        """

    @qt-parity
    Scenario: A module in an import path given to the run can be imported
      Given the file "imports/Helpers/qmldir":
        """
        module Helpers
        Tripler 1.0 Tripler.qml
        """
      And the file "imports/Helpers/Tripler.qml":
        """qml
        import QtQuick
        QtObject {
          property int input: 0
          readonly property int output: input * 3
        }
        """
      And the file "tst_far.qml":
        """qml
        import QtQuick
        import QtTest
        import Helpers
        TestCase {
          name: "Far"
          Tripler { id: tripler; input: 4 }
          function test_it() { compare(tripler.output, 12) }
        }
        """
      When the tests are run with the import path "imports"
      Then the test report has the lines:
        """
        PASS   : Far::test_it()
        """

    @qt-parity
    Scenario: Without the import path the module is not found and the file fails
      Given the file "imports/Helpers/qmldir":
        """
        module Helpers
        Tripler 1.0 Tripler.qml
        """
      And the file "imports/Helpers/Tripler.qml":
        """qml
        import QtQuick
        QtObject {}
        """
      And the file "tst_far.qml":
        """qml
        import QtQuick
        import QtTest
        import Helpers
        TestCase { name: "Far"; function test_it() {} }
        """
      When the tests are run
      Then the test report has the lines:
        """
           Loc: [tst_far.qml(3)]
        Totals: 0 passed, 1 failed, 0 skipped
        """
      And the test run exits with status 1

  Rule: A test file imports the modules an application file can

    @qt-parity
    Scenario: A test keeps rows in a local database
      Given the file "tst_storage.qml":
        """qml
        import QtQuick
        import QtQuick.LocalStorage
        import QtTest
        TestCase {
          name: "Storage"
          function test_rows() {
            let db = LocalStorage.openDatabaseSync("Notes", "1.0", "notes")
            let names = []
            db.transaction(function (tx) {
              tx.executeSql("CREATE TABLE IF NOT EXISTS notes (title TEXT)")
              tx.executeSql("DELETE FROM notes")
              tx.executeSql("INSERT INTO notes VALUES (?)", ["milk"])
              tx.executeSql("INSERT INTO notes VALUES (?)", ["bread"])
              let rs = tx.executeSql("SELECT title FROM notes ORDER BY title")
              for (let i = 0; i < rs.rows.length; i++)
                names.push(rs.rows.item(i).title)
            })
            compare(names.join(","), "bread,milk")
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Storage::test_rows()
        """
      And the test run exits with status 0

    @qt-parity
    Scenario: A test reads the items of an XML file next to it
      Given the file "feed.xml":
        """
        <feed><item><title>First</title></item><item><title>Second</title></item></feed>
        """
      And the file "tst_xml.qml":
        """qml
        import QtQuick
        import QtQml.XmlListModel
        import QtTest
        TestCase {
          name: "Xml"
          XmlListModel {
            id: feed
            source: Qt.resolvedUrl("feed.xml")
            query: "/feed/item"
            XmlListModelRole { name: "title"; elementName: "title" }
          }
          function test_items() {
            tryCompare(feed, "status", XmlListModel.Ready)
            compare(feed.count, 2)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Xml::test_items()
        """
      And the test run exits with status 0

    @qt-parity
    Scenario: A test lists the files of a folder next to it
      Given the file "notes/one.txt":
        """
        one
        """
      And the file "notes/two.txt":
        """
        two
        """
      And the file "notes/three.md":
        """
        three
        """
      And the file "tst_folder.qml":
        """qml
        import QtQuick
        import Qt.labs.folderlistmodel
        import QtTest
        TestCase {
          name: "Folder"
          FolderListModel {
            id: folder
            folder: Qt.resolvedUrl("notes")
            nameFilters: ["*.txt"]
          }
          function test_files() {
            tryCompare(folder, "status", FolderListModel.Ready)
            compare(folder.count, 2)
            compare(folder.get(0, "fileName"), "one.txt")
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Folder::test_files()
        """
      And the test run exits with status 0

  Rule: A test reaches the network

    @qt-parity
    Scenario: A test waits for what it fetched
      Given the web server serves the text file "act.json":
        """
        {"activity": "Read a book"}
        """
      And the file "tst_fetch.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Fetch"
          function test_fetch() {
            let http = new XMLHttpRequest()
            let done = false
            http.onload = () => done = true
            http.open("GET", "$SERVER/act.json")
            http.send()
            tryVerify(() => done, 5000)
            compare(http.status, 200)
            compare(JSON.parse(http.responseText).activity, "Read a book")
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Fetch::test_fetch()
        """

  Rule: The results can be read by a program

    @ljos-only
    Scenario: The results are also written as data
      Given the file "tst_data.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          name: "Data"
          function test_bad() { compare(1, 2) }
          function test_good() {}
          function test_later() { skip("not yet") }
        }
        """
      When the tests are run
      Then the test results' "/totals/passed" is 3
      And the test results' "/totals/failed" is 1
      And the test results' "/totals/skipped" is 1
      And the test results' "/tests/1/case" is "Data"
      And the test results' "/tests/1/function" is "test_bad"
      And the test results' "/tests/1/outcome" is "fail"
      And the test results' "/tests/1/messages/0/kind" is "fail"
      And the test results' "/tests/1/messages/0/message" is "Compared values are not the same"
      And the test results' "/tests/1/messages/0/actual" is "1"
      And the test results' "/tests/1/messages/0/expected" is "2"
      And the test results' "/tests/1/messages/0/line" is 5
      And the test results' "/tests/2/outcome" is "pass"
      And the test results' "/tests/3/outcome" is "skip"

  Rule: Waiting costs no time

    @ljos-only
    Scenario: An hour of the application's time passes in a moment
      Given the file "tst_hour.qml":
        """qml
        import QtQuick
        import QtTest
        TestCase {
          id: root
          name: "Hour"
          property int ticks: 0
          Timer { interval: 60000; repeat: true; running: true; onTriggered: root.ticks += 1 }
          function test_wait() {
            const before = ticks
            wait(3600000)
            compare(ticks - before, 60)
          }
        }
        """
      When the tests are run
      Then the test report has the lines:
        """
        PASS   : Hour::test_wait()
        """
