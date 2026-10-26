// expect: motion-frequent at 25
// audit: reduced-motion=false
import QtQuick
import QtQuick.Controls

Rectangle {
    id: page
    width: 400; height: 300
    color: palette.window
    property int slide: 300
    TabBar {
        id: bar
        width: parent.width
        TabButton { text: qsTr("General") }
        TabButton { text: qsTr("Advanced") }
    }
    Item {
        anchors.top: bar.bottom
        anchors.bottom: parent.bottom
        width: parent.width
        clip: true
        Row {
            x: -bar.currentIndex * page.width
            Behavior on x {
                NumberAnimation {
                    duration: page.slide
                    easing.type: Easing.OutCubic
                }
            }
            Label { width: page.width; text: qsTr("General settings") }
            Label { width: page.width; text: qsTr("Advanced settings") }
        }
    }
}
