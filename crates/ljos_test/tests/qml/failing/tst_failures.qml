import QtQuick
import QtTest

// Every test here fails on purpose: the runner has to report each one the
// way Qt's runner does, and keep going.
Item {
    id: root
    width: 100
    height: 100

    property int n: 1
    signal ping()

    SignalSpy {
        id: spy
        target: root
        signalName: "ping"
    }

    TestCase {
        name: "Failures"
        when: windowShown

        function test_compare() {
            compare(1 + 1, 3)
            compare("not", "reached")
        }

        function test_compare_message() {
            compare("left", "right", "the two sides differ")
        }

        function test_compare_lists() {
            compare([1, 2, 3], [1, 2, 4])
        }

        function test_compare_types() {
            compare("1", 1)
        }

        function test_verify() {
            verify(root.n > 5)
        }

        function test_verify_message() {
            verify(false, "n is too small")
        }

        function test_fuzzyCompare() {
            fuzzyCompare(1.0, 1.5, 0.25)
        }

        function test_fail() {
            fail("given up")
        }

        function test_throw() {
            throw new Error("thrown by the test")
        }

        function test_tryCompare() {
            tryCompare(root, "n", 2, 200)
        }

        function test_tryCompare_message() {
            tryCompare(root, "n", 2, 200, "n never became 2")
        }

        function test_tryVerify() {
            tryVerify(function() { return root.n > 5 }, 200)
        }

        function test_spy_wait() {
            spy.wait(200)
        }

        function test_unexpected_pass() {
            expectFail("", "thought to be broken")
            compare(root.n, 1)
        }

        function test_rows_data() {
            return [
                { tag: "right", value: 1 },
                { tag: "wrong", value: 2 },
                { tag: "skipped", value: 3 }
            ]
        }

        function test_rows(row) {
            if (row.value === 3)
                skip("no third row yet")
            compare(row.value, root.n)
        }

        function test_passes() {
            compare(root.n, 1)
        }
    }

    TestCase {
        name: "FailingHooks"

        property int runs: 0

        function init() {
            runs += 1
        }

        function cleanup() {
            verify(runs !== 2, "cleanup after the second test")
        }

        function test_a() {
            compare(runs, 1)
        }

        function test_b() {
            compare(runs, 2)
        }

        function test_c() {
            compare(runs, 3)
        }
    }

    TestCase {
        name: "FailingStart"

        function initTestCase() {
            fail("nothing can run")
        }

        function test_never_runs() {
        }
    }
}
