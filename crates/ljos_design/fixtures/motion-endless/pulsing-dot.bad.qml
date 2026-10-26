// expect: motion-endless at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 120; height: 40
    Label {
        text: qsTr("Live")
        SequentialAnimation on opacity { loops: Animation.Infinite
            NumberAnimation { to: 0.3; duration: 900; easing.type: Easing.InOutSine }
            NumberAnimation { to: 1; duration: 900; easing.type: Easing.InOutSine }
        }
    }
}
