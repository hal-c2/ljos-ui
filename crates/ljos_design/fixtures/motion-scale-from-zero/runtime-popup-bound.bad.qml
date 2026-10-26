// expect: motion-scale-from-zero at 16
// audit: reduced-motion=false
import QtQuick
import QtQuick.Controls

Rectangle {
    id: page
    width: 400; height: 300
    color: palette.window
    property real start: 0
    Popup {
        x: 40; y: 40
        width: 200; height: 120
        Component.onCompleted: open()
        enter: Transition {
            NumberAnimation {
                property: "scale"
                from: page.start
                to: 1
                duration: 200
                easing.type: Easing.OutCubic
            }
        }
        Label { text: qsTr("Saved") }
    }
}
