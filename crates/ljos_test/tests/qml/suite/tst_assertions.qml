import QtQuick
import QtTest

TestCase {
    id: testCase
    name: "Assertions"

    property int started: 0
    property int cleaned: 0
    property bool prepared: false

    function initTestCase() {
        prepared = true
        compare(running, true)
        compare(completed, false)
    }

    function cleanupTestCase() {
        verify(started > 0)
        compare(cleaned, started)
    }

    function init() {
        started += 1
    }

    function cleanup() {
        cleaned += 1
    }

    function test_compare() {
        verify(prepared)
        compare(1 + 1, 2)
        compare("a" + "b", "ab")
        compare([1, 2, [3]], [1, 2, [3]])
        compare({ a: 1, b: { c: "d" } }, { a: 1, b: { c: "d" } })
        compare(Qt.point(1, 2), Qt.point(1, 2))
        compare(Qt.rgba(1, 0, 0, 1), Qt.rgba(1, 0, 0, 1))
        compare(null, null)
        compare(undefined, undefined)
        compare(testCase, testCase)
        compare(0.1 + 0.2, 0.3)
    }

    function test_verify() {
        verify(true)
        verify(1, "numbers that are not zero hold")
        verify("text")
        verify(testCase)
    }

    function test_fuzzyCompare() {
        fuzzyCompare(1.0, 1.04, 0.05)
        fuzzyCompare(10, 12, 2)
        fuzzyCompare(Qt.rgba(1, 0, 0, 1), "#fd0102", 2)
    }

    function test_rows_data() {
        return [
            { tag: "small", a: 1, b: 2, sum: 3 },
            { tag: "negative", a: -4, b: 1, sum: -3 },
            { a: 10, b: 20, sum: 30 }
        ]
    }

    function test_rows(row) {
        compare(row.a + row.b, row.sum)
    }

    function test_skip() {
        skip("not on this platform")
        fail("skip leaves the function")
    }

    function test_expectFail() {
        expectFail("", "a known difference")
        compare(1, 2)
        fail("a failed expectation leaves the function")
    }

    function test_expectFail_rows_data() {
        return [ { tag: "broken", n: 1 }, { tag: "whole", n: 2 } ]
    }

    function test_expectFail_rows(row) {
        expectFail("broken", "only this row is known to fail")
        compare(row.n, 2)
    }

    function test_caught_failure() {
        var caught = false
        try {
            undefinedFunctionName()
        } catch (e) {
            caught = true
        }
        verify(caught)
    }

    function test_name() {
        compare(name, "Assertions")
        compare(when, true)
        compare(optional, false)
        compare(running, true)
    }
}
