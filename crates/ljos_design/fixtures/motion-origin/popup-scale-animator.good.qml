import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 200
    Popup {
        enter: Transition {
            NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 150; easing.type: Easing.OutCubic }
        }
        Label { text: qsTr("Details") }
    }
}
