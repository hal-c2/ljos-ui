import QtQuick
import QtTest

// What a pointer handler's signals are given of the point: an eventPoint,
// whose positions are in the handler's item and in the scene.
Item {
    id: root
    width: 400
    height: 400

    property var tapped: null
    property var single: null
    property int button: 0
    property bool accepted: false
    property var canceled: null
    property var grabs: []
    property point menuAt: Qt.point(-1, -1)

    Rectangle {
        id: box
        x: 50
        y: 30
        width: 200
        height: 200
        TapHandler {
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            onTapped: (eventPoint, button) => {
                root.tapped = eventPoint
                root.button = button
                root.accepted = eventPoint.accepted
                root.menuAt = Qt.point(eventPoint.position.x, eventPoint.position.y)
            }
            onSingleTapped: eventPoint => root.single = eventPoint
            onCanceled: point => root.canceled = point
        }
    }

    Rectangle {
        id: dragged
        x: 50
        y: 250
        width: 100
        height: 100
        DragHandler {
            onGrabChanged: (transition, point) => root.grabs = root.grabs.concat([{
                transition: transition, state: point.state,
                position: point.position, scenePosition: point.scenePosition
            }])
        }
    }

    TestCase {
        name: "HandlerEventPoint"
        when: windowShown

        function init() {
            root.tapped = null
            root.single = null
            root.canceled = null
            root.grabs = []
            wait(500)
        }

        function test_tapped_is_given_where_the_button_came_up() {
            mouseClick(box, 20, 10, Qt.RightButton)
            verify(root.tapped !== null)
            compare(root.tapped.position, Qt.point(20, 10))
            compare(root.tapped.scenePosition, Qt.point(70, 40))
            compare(root.tapped.pressPosition, Qt.point(20, 10))
            compare(root.tapped.scenePressPosition, Qt.point(70, 40))
            compare(root.tapped.state, 8)
            compare(root.accepted, true)
            compare(root.tapped.pressure, 1)
            compare(root.button, Qt.RightButton)
            compare(root.menuAt, Qt.point(20, 10))
        }

        function test_single_tapped_is_given_the_same_point() {
            mouseClick(box, 30, 40)
            tryVerify(() => root.single !== null)
            compare(root.single.position, Qt.point(30, 40))
            compare(root.single.scenePosition, Qt.point(80, 70))
        }

        function test_canceled_is_given_where_the_pointer_went() {
            mousePress(box, 20, 10)
            mouseMove(box, 120, 120)
            mouseRelease(box, 120, 120)
            verify(root.canceled !== null)
            compare(root.canceled.position, Qt.point(120, 120))
            compare(root.canceled.pressPosition, Qt.point(20, 10))
            compare(root.canceled.scenePosition, Qt.point(170, 150))
            compare(root.canceled.state, 2)
        }

        function test_grab_changed_is_given_the_point_it_grabbed() {
            mousePress(dragged, 10, 10)
            mouseMove(dragged, 20, 20)
            mouseMove(dragged, 40, 40)
            mouseRelease(dragged, 40, 40)
            const first = root.grabs[0]
            compare(first.transition, PointerDevice.GrabPassive)
            compare(first.state, 1)
            compare(first.position, Qt.point(10, 10))
            compare(first.scenePosition, Qt.point(60, 260))
            const last = root.grabs[root.grabs.length - 1]
            compare(last.transition, PointerDevice.UngrabPassive)
            compare(last.state, 8)
        }
    }
}
