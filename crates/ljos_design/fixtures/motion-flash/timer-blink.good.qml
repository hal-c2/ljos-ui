import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 80
    property bool pending: false
    Label { id: banner; text: qsTr("Alert") }
    Timer {
        interval: 500
        repeat: true
        running: pending
        onTriggered: banner.visible = !banner.visible
    }
}
