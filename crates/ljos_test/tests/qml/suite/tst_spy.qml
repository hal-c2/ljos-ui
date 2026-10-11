import QtQuick
import QtTest

Item {
    id: root
    width: 200
    height: 200

    Counter {
        id: counter
    }

    Timer {
        id: later
        interval: 200
        onTriggered: counter.bump("timer")
    }

    SignalSpy {
        id: spy
        target: counter
        signalName: "bumped"
    }

    SignalSpy {
        id: countSpy
        target: counter
        signalName: "countChanged"
    }

    SignalSpy {
        id: idle
    }

    TestCase {
        name: "SignalSpy"
        when: windowShown

        function init() {
            later.stop()
            counter.count = 0
            spy.clear()
            countSpy.clear()
        }

        function test_count() {
            compare(spy.valid, true)
            compare(idle.valid, false)
            compare(spy.count, 0)
            counter.bump("one")
            counter.bump("two")
            compare(spy.count, 2)
            compare(countSpy.count, 2)
        }

        function test_signalArguments() {
            counter.bump("one")
            counter.bump("two")
            compare(spy.signalArguments.length, 2)
            compare(spy.signalArguments[0][0], 1)
            compare(spy.signalArguments[0][1], "one")
            compare(spy.signalArguments[1][0], 2)
            compare(spy.signalArguments[1][1], "two")
        }

        function test_clear() {
            counter.bump("one")
            compare(spy.count, 1)
            spy.clear()
            compare(spy.count, 0)
            compare(spy.signalArguments.length, 0)
        }

        function test_wait() {
            later.start()
            compare(spy.count, 0)
            spy.wait()
            compare(spy.count, 1)
            compare(spy.signalArguments[0][1], "timer")
        }

        function test_wait_timeout() {
            later.start()
            spy.wait(1000)
            compare(spy.count, 1)
        }

        function test_retarget() {
            counter.bump("one")
            compare(spy.count, 1)
            spy.signalName = "countChanged"
            spy.clear()
            counter.bump("two")
            compare(spy.count, 1)
            spy.target = null
            compare(spy.valid, false)
            spy.target = counter
            spy.signalName = "bumped"
            compare(spy.valid, true)
            spy.clear()
            counter.bump("three")
            compare(spy.signalArguments[0][1], "three")
        }
    }
}
