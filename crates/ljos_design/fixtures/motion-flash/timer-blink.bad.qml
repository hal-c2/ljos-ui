// expect: motion-flash at 9
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    Label { id: banner; text: qsTr("Alert") }
    Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: banner.visible = !banner.visible
    }
}
