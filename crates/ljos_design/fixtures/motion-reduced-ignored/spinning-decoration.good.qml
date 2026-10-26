import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: decoration
    width: 48; height: 48
    Label {
        anchors.centerIn: parent
        text: qsTr("*")
        OpacityAnimator on opacity {
            from: 1; to: 0.4; duration: 1200
            loops: Animation.Infinite
            running: false
        }
    }
}
