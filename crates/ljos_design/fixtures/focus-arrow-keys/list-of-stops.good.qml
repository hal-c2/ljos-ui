import QtQuick
import QtQuick.Controls

Rectangle {
    width: 300; height: 300
    color: palette.window
    // One stop: the arrows move the current row, Space checks it.
    ListView {
        id: list
        anchors.fill: parent
        activeFocusOnTab: true
        model: ["Inbox", "Drafts", "Sent", "Archive", "Trash"]
        delegate: CheckBox {
            text: modelData
            focusPolicy: Qt.NoFocus
        }
    }
}
