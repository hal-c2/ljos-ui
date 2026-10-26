// expect: motion-ease-in-enter at 16
// audit: reduced-motion=false
import QtQuick
import QtQuick.Controls

Rectangle {
    id: page
    width: 400; height: 300
    color: palette.window
    property int curve: Easing.InCubic
    Popup {
        x: 40; y: 40
        width: 200; height: 120
        Component.onCompleted: open()
        enter: Transition {
            NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: 200
                easing.type: page.curve
            }
        }
        Label { text: qsTr("Saved") }
    }
}
