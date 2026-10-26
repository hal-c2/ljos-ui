// expect: layout-fixed-size at 7
import QtQuick
import QtQuick.Controls

Popup {
    property string message
    width: 320
    height: 120
    Label {
        anchors.fill: parent
        text: message
        wrapMode: Text.Wrap
    }
}
