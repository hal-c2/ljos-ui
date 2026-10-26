// expect: motion-origin at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 200; height: 200
    ToolTip {
        text: qsTr("Saves the file")
        enter: Transition {
            NumberAnimation { property: "scale"; from: 0.9; to: 1; duration: 120; easing.type: Easing.OutCubic }
        }
    }
}
