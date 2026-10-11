import QtQuick
import QtTest

Item {
    id: root
    width: 200
    height: 200

    property int ticks: 0
    property bool late: false

    Timer {
        id: ticker
        interval: 100
        repeat: true
        onTriggered: root.ticks += 1
    }

    Timer {
        id: once
        interval: 250
        onTriggered: root.late = true
    }

    TestCase {
        name: "Time"
        when: windowShown

        function init() {
            ticker.stop()
            once.stop()
            root.ticks = 0
            root.late = false
        }

        function test_windowShown() {
            compare(windowShown, true)
        }

        function test_wait() {
            ticker.start()
            wait(350)
            verify(root.ticks >= 3)
            ticker.stop()
            var seen = root.ticks
            wait(300)
            compare(root.ticks, seen)
        }

        function test_sleep() {
            once.start()
            sleep(20)
            compare(root.late, false)
            tryCompare(root, "late", true)
        }

        function test_tryCompare() {
            once.start()
            compare(root.late, false)
            tryCompare(root, "late", true, 1000)
            compare(root.late, true)
        }

        function test_tryCompare_timeout() {
            once.start()
            expectFail("", "the timer needs 250 ms")
            tryCompare(root, "late", true, 100, "late in 100 ms")
        }

        function test_tryVerify() {
            ticker.start()
            tryVerify(function() { return root.ticks >= 2 })
            tryVerify(function() { return root.ticks >= 4 }, 1000, "four ticks")
            verify(root.ticks >= 4)
        }

        function test_waitForRendering() {
            root.late = !root.late
            waitForRendering(root, 100)
            verify(true)
        }
    }
}
