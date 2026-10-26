// expect: layout-fixed-size at 7
import QtQuick
import QtQuick.Controls

Popup {
    property var items
    width: 320
    height: 120
    ListView {
        anchors.fill: parent
        model: items
        delegate: Label {
            required property string modelData
            text: modelData
        }
    }
}
