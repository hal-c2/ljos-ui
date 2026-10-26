// expect: motion-frequent at 25
// audit: reduced-motion=false
import QtQuick
import QtQuick.Controls

Rectangle {
    id: page
    width: 300; height: 200
    color: palette.window
    property int fade: 300
    Column {
        x: 20; y: 20
        spacing: 8
        Repeater {
            model: [qsTr("Open"), qsTr("Save")]
            Button {
                id: button
                required property string modelData
                text: modelData
                background: Rectangle {
                    implicitWidth: 100; implicitHeight: 40
                    radius: 4
                    color: button.activeFocus ? button.palette.highlight : button.palette.button
                    Behavior on color {
                        ColorAnimation { duration: page.fade }
                    }
                }
            }
        }
    }
}
