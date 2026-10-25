// expect: a11y-tab-trap at 13
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 240
    Column {
        spacing: 8
        TextArea {
            width: 360; height: 120
            placeholderText: "Notes"
            Keys.onPressed: event => {
                if (event.key === Qt.Key_Tab) {
                    insert(cursorPosition, "\t")
                    event.accepted = true
                }
            }
        }
        Button { text: "Save" }
    }
}
