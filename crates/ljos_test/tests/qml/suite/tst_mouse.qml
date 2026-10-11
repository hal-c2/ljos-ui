import QtQuick
import QtTest

Item {
    id: root
    width: 300
    height: 300

    property var seen: []
    function note(what) {
        var next = seen.slice()
        next.push(what)
        seen = next
    }

    Rectangle {
        id: pad
        x: 20
        y: 30
        width: 100
        height: 80
        color: "teal"

        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            property int lastButton: 0
            property int lastModifiers: 0
            property point at
            onPressed: (mouse) => {
                root.note("pressed")
                lastButton = mouse.button
                lastModifiers = mouse.modifiers
                at = Qt.point(mouse.x, mouse.y)
            }
            onReleased: root.note("released")
            onClicked: root.note("clicked")
            onDoubleClicked: root.note("doubleClicked")
            onPositionChanged: (mouse) => { at = Qt.point(mouse.x, mouse.y) }
        }
    }

    MouseArea {
        id: track
        x: 150
        y: 150
        width: 140
        height: 140
        property var path: []
        function note(what, mouse) {
            var next = path.slice()
            next.push(what + " " + mouse.x + "," + mouse.y)
            path = next
        }
        onPressed: (mouse) => note("press", mouse)
        onPositionChanged: (mouse) => note("move", mouse)
        onReleased: (mouse) => note("release", mouse)
    }

    Flickable {
        id: scroller
        x: 150
        y: 10
        width: 120
        height: 100
        contentWidth: 120
        contentHeight: 1000

        Rectangle {
            width: 120
            height: 1000
            color: "khaki"
        }
    }

    TestCase {
        name: "Mouse"
        when: windowShown

        function init() {
            root.seen = []
            scroller.contentY = 0
            track.path = []
        }

        function test_mouseClick() {
            mouseClick(pad)
            compare(root.seen, ["pressed", "released", "clicked"])
            compare(area.at, Qt.point(50, 40))
            compare(area.lastButton, Qt.LeftButton)
        }

        function test_mouseClick_at() {
            mouseClick(pad, 3, 4, Qt.RightButton, Qt.ShiftModifier)
            compare(area.at, Qt.point(3, 4))
            compare(area.lastButton, Qt.RightButton)
            compare(area.lastModifiers, Qt.ShiftModifier)
        }

        function test_mouseClick_outside() {
            mouseClick(root, 5, 5)
            compare(root.seen, [])
        }

        function test_mousePress_mouseRelease() {
            mousePress(pad, 10, 10)
            compare(area.pressed, true)
            compare(root.seen, ["pressed"])
            mouseRelease(pad, 10, 10)
            compare(area.pressed, false)
            compare(root.seen, ["pressed", "released", "clicked"])
        }

        function test_mouseMove() {
            mouseMove(pad, 5, 5)
            compare(area.containsMouse, true)
            // The move that enters is told apart from the ones inside by
            // the two runners, so the position is read after one more.
            mouseMove(pad, 6, 7)
            compare(area.at, Qt.point(6, 7))
            mouseMove(pad, 60, 20)
            compare(area.at, Qt.point(60, 20))
            mouseMove(root, 1, 1)
            compare(area.containsMouse, false)
        }

        function test_mouseDoubleClickSequence() {
            mouseDoubleClickSequence(pad)
            verify(root.seen.indexOf("doubleClicked") >= 0)
            compare(root.seen[0], "pressed")
        }

        function test_mouseDrag() {
            mouseDrag(track, 20, 30, 50, -10)
            var path = track.path
            verify(path.length >= 3)
            compare(path[0], "press 20,30")
            compare(path[path.length - 2], "move 70,20")
            compare(path[path.length - 1], "release 70,20")
            compare(track.pressed, false)
        }

        function test_mouseWheel() {
            compare(scroller.contentY, 0)
            mouseWheel(scroller, 10, 10, 0, -120)
            tryVerify(function() { return scroller.contentY > 0 })
            wait(500)
            var down = scroller.contentY
            mouseWheel(scroller, 10, 10, 0, 120)
            tryVerify(function() { return scroller.contentY < down })
        }
    }
}
