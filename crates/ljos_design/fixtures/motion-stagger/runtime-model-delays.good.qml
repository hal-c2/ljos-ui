// audit: reduced-motion=false
import QtQuick

Column {
    width: 200; height: 200
    Repeater {
        // The delays come from the model, which only the made rows know.
        model: [0, 30, 60, 90]
        Rectangle {
            required property int modelData
            width: 100
            height: 20
            opacity: 0
            SequentialAnimation on opacity {
                PauseAnimation {
                    duration: modelData
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
