import QtQuick
import QtTest

Item {
    id: root
    width: 200
    height: 200

    property var seen: []
    function note(what) {
        var next = seen.slice()
        next.push(what)
        seen = next
    }

    Item {
        id: catcher
        focus: true
        property int lastKey: 0
        property int lastModifiers: 0
        property string lastText: ""
        Keys.onPressed: (event) => {
            lastKey = event.key
            lastModifiers = event.modifiers
            lastText = event.text
            root.note("press")
        }
        Keys.onReleased: (event) => { root.note("release") }
        Keys.onReturnPressed: root.note("return")
    }

    TextInput {
        id: field
        y: 100
        width: 100
    }

    TestCase {
        name: "Keys"
        when: windowShown

        function init() {
            root.seen = []
            field.text = ""
            catcher.forceActiveFocus()
        }

        function test_keyClick() {
            keyClick(Qt.Key_Left)
            compare(catcher.lastKey, Qt.Key_Left)
            compare(root.seen, ["press", "release"])
        }

        function test_keyClick_character() {
            keyClick("a")
            compare(catcher.lastKey, Qt.Key_A)
            compare(catcher.lastText, "a")
        }

        function test_keyClick_modifiers() {
            keyClick(Qt.Key_B, Qt.ControlModifier | Qt.ShiftModifier)
            compare(catcher.lastKey, Qt.Key_B)
            compare(catcher.lastModifiers, Qt.ControlModifier | Qt.ShiftModifier)
        }

        function test_keyPress_keyRelease() {
            keyPress(Qt.Key_Return)
            compare(root.seen, ["return"])
            keyRelease(Qt.Key_Return)
            compare(root.seen, ["return", "release"])
        }

        function test_typing() {
            field.forceActiveFocus()
            keyClick("h")
            keyClick("i")
            compare(field.text, "hi")
            keyClick(Qt.Key_Backspace)
            compare(field.text, "h")
        }

        function test_keySequence() {
            field.forceActiveFocus()
            keyClick("a")
            keyClick("b")
            keyClick("c")
            keySequence("Ctrl+A")
            compare(field.selectedText, "abc")
            keySequence("x,y")
            compare(field.text, "xy")
        }
    }
}
