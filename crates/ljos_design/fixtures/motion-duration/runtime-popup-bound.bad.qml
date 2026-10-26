// expect: motion-duration at 16
// audit: reduced-motion=false
import QtQuick
import QtQuick.Controls

Rectangle {
    id: page
    width: 400; height: 300
    color: palette.window
    property int slow: 600
    Popup {
        x: 40; y: 40
        width: 200; height: 120
        Component.onCompleted: open()
        enter: Transition {
            NumberAnimation {
                property: "opacity"
                from: 0
                to: 1
                duration: page.slow
                easing.type: Easing.OutCubic
            }
        }
        Label { text: qsTr("Saved") }
    }
}
