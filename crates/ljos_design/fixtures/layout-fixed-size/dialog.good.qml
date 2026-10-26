import QtQuick
import QtQuick.Controls

Popup {
    property string message
    width: Math.min(implicitWidth, 480)
    Label {
        anchors.fill: parent
        text: message
        wrapMode: Text.Wrap
    }
}
