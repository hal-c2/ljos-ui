import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    property bool pending: false
    Label {
        text: qsTr("Alert")
        SequentialAnimation on opacity {
            loops: Animation.Infinite
            running: pending
            NumberAnimation { to: 0.4; duration: 800; easing.type: Easing.InOutSine }
            NumberAnimation { to: 1; duration: 800; easing.type: Easing.InOutSine }
        }
    }
}
