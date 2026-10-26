import QtQuick.Layouts
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    RowLayout {
        width: 400
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 32
            color: "#eeeeee"
        }
        Rectangle {
            Layout.minimumWidth: 120
            Layout.preferredHeight: 32
            color: "#90caf9"
        }
    }
}
