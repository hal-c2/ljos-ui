import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 200
    Menu {
        transformOrigin: Popup.TopLeft
        enter: Transition {
            NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: 150; easing.type: Easing.OutCubic }
        }
        MenuItem { text: qsTr("Cut") }
        MenuItem { text: qsTr("Copy") }
    }
}
