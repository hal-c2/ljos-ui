import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 120; height: 40
    property bool syncing: false
    Label {
        text: qsTr("Live")
        SequentialAnimation on opacity {
            loops: Animation.Infinite
            running: syncing
            NumberAnimation { to: 0.3; duration: 900; easing.type: Easing.InOutSine }
            NumberAnimation { to: 1; duration: 900; easing.type: Easing.InOutSine }
        }
    }
}
