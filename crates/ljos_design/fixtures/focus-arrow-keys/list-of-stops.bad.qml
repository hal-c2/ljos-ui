// expect: focus-arrow-keys at 9
import QtQuick
import QtQuick.Controls

Rectangle {
    width: 300; height: 300
    color: palette.window
    // Each row is a Tab stop of its own.
    ListView {
        anchors.fill: parent
        model: ["Inbox", "Drafts", "Sent", "Archive", "Trash"]
        delegate: CheckBox { text: modelData }
    }
}
