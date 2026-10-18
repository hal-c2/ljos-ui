// Shift+Return in a TextEdit starts a new line on every platform: the
// standard key InsertLineSeparator is Shift+Enter and Shift+Return on all
// of them, and Meta+Enter, Meta+Return and Meta+O on macOS besides
// (doc.qt.io/qt-6/qkeysequence.html#standard-shortcuts). Passes on
// qmltestrunner on Linux.
import QtQuick
import QtTest

Item {
    width: 300
    height: 200

    TextEdit {
        id: edit
        width: 300
        height: 200
        focus: true
        text: "first"
        Component.onCompleted: cursorPosition = length
    }

    TestCase {
        name: "ShiftReturn"
        when: windowShown

        function init() {
            edit.text = "first";
            edit.cursorPosition = edit.length;
            edit.forceActiveFocus();
        }

        function test_shift_return_starts_a_new_line() {
            keyClick(Qt.Key_Return, Qt.ShiftModifier);
            keyClick(Qt.Key_X);
            compare(edit.lineCount, 2);
        }

        function test_shift_enter_starts_a_new_line() {
            keyClick(Qt.Key_Enter, Qt.ShiftModifier);
            keyClick(Qt.Key_X);
            compare(edit.lineCount, 2);
        }

        function test_return_starts_a_new_line() {
            keyClick(Qt.Key_Return);
            keyClick(Qt.Key_X);
            compare(edit.lineCount, 2);
        }
    }
}
