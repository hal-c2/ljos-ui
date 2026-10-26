import QtQuick

Column {
    Repeater {
        model: 3
        Rectangle {
            required property int index
            width: 100
            height: 20
            opacity: 0
            SequentialAnimation on opacity {
                PauseAnimation {
                    duration: index * 40
                }
                NumberAnimation {
                    to: 1
                    duration: 150
                    easing.type: Easing.OutCubic
                }
            }
        }
    }
}
