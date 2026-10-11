import QtQuick
import QtTest

Item {
    id: root
    width: 200
    height: 200

    Rectangle {
        id: pad
        x: 50
        y: 50
        width: 100
        height: 100
        color: "plum"
        property int taps: 0

        TapHandler {
            id: tap
            onTapped: pad.taps += 1
        }
    }

    // A touch nothing takes is a mouse to a MouseArea, which says where
    // it was.
    MouseArea {
        id: slate
        x: 0
        y: 160
        width: 40
        height: 40
        property point at
        onPressed: (mouse) => { at = Qt.point(mouse.x, mouse.y) }
        onPositionChanged: (mouse) => { at = Qt.point(mouse.x, mouse.y) }
    }

    TestCase {
        name: "Touch"
        when: windowShown

        function init() {
            pad.taps = 0
        }

        function test_press_release() {
            var touch = touchEvent(pad)
            touch.press(0, pad, 10, 10).commit()
            compare(tap.pressed, true)
            touch.release(0, pad, 10, 10).commit()
            compare(tap.pressed, false)
            compare(pad.taps, 1)
        }

        function test_defaults_to_the_middle() {
            var touch = touchEvent(pad)
            touch.press(1).commit()
            compare(tap.pressed, true)
            touch.release(1).commit()
            compare(pad.taps, 1)
            touch = touchEvent(slate)
            touch.press(1).commit()
            compare(slate.pressed, true)
            compare(slate.at, Qt.point(20, 20))
            touch.release(1).commit()
            compare(slate.pressed, false)
        }

        function test_move() {
            var touch = touchEvent(pad)
            touch.press(0, pad, 10, 10).commit()
            touch.move(0, pad, 12, 11).commit()
            compare(tap.pressed, true)
            touch.release(0, pad, 12, 11).commit()
            compare(tap.pressed, false)
            compare(pad.taps, 1)
            touch = touchEvent(slate)
            touch.press(0, slate, 5, 6).commit()
            compare(slate.at, Qt.point(5, 6))
            touch.release(0, slate, 5, 6).commit()
            compare(slate.pressed, false)
        }

        function test_nothing_is_sent_before_commit() {
            var touch = touchEvent(pad)
            touch.press(0, pad, 10, 10)
            compare(tap.pressed, false)
            touch.commit()
            compare(tap.pressed, true)
            touch.release(0, pad, 10, 10)
            touch.commit()
            compare(pad.taps, 1)
        }
    }
}
