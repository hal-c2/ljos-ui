// expect: decl-imperative-animation at 9
import QtQuick

Item {
    Rectangle { id: panel; width: 80; height: 80 }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onEntered: show.start()
        onExited: hide.start()
    }
    NumberAnimation { id: show; target: panel; property: "opacity"; to: 1; duration: 150; easing.type: Easing.OutCubic }
    NumberAnimation { id: hide; target: panel; property: "opacity"; to: 0; duration: 150; easing.type: Easing.InCubic }
}
