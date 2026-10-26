// expect: motion-flash at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Label {
        text: qsTr("Alert")
        SequentialAnimation on opacity { loops: Animation.Infinite
            NumberAnimation { to: 0; duration: 100 }
            NumberAnimation { to: 1; duration: 100 }
        }
    }
}
