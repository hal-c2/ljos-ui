import QtQuick
import QtTest

// TapHandler's tapped(eventPoint, button) names only its first argument: a
// handler that does not declare `button` is not given it, so an id of that
// name is the object. A handler that declares both gets both.
Item {
    id: root
    width: 200
    height: 200

    property int n: 0
    property var declared: "none"
    property string kind: "none"

    Rectangle {
        id: button
        signal clicked
        width: 24
        height: 24
        TapHandler { onTapped: button.clicked() }
        onClicked: root.n++
    }

    Rectangle {
        id: other
        x: 50
        width: 24
        height: 24
        TapHandler {
            onTapped: (eventPoint, button) => root.declared = button
            onSingleTapped: root.kind = typeof eventPoint + " " + typeof button
        }
    }

    TestCase {
        name: "TapHandlerArguments"
        when: windowShown

        function test_an_id_named_like_an_unnamed_argument_is_the_object() {
            mouseClick(button, 5, 5)
            compare(root.n, 1)
        }

        function test_a_handler_that_declares_the_arguments_gets_them() {
            mouseClick(other, 5, 5)
            compare(root.declared, Qt.LeftButton)
        }

        function test_only_the_named_argument_is_injected() {
            wait(500)
            mouseClick(other, 5, 5)
            tryCompare(root, "kind", "object object")
        }
    }
}
