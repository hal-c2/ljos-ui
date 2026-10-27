// expect: decl-imperative-animation at 10
import QtQuick
import QtQuick.Controls

Button {
    id: button
    text: qsTr("Pulse")
    onClicked: grow.start()
    NumberAnimation { id: grow; target: button; property: "scale"; to: 1.1; duration: 120; easing.type: Easing.OutCubic
        onFinished: shrink.start()
    }
    NumberAnimation { id: shrink; target: button; property: "scale"; to: 1; duration: 120; easing.type: Easing.OutCubic }
}
