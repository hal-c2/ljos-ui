// expect: layout-under-minimum at 15
import QtQuick.Layouts
import QtQuick
import QtQuick.Controls
Rectangle {
    width: 640; height: 480
    color: "white"
    RowLayout {
        width: 200
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 32
            color: "#eeeeee"
        }
        Rectangle {
            Layout.minimumWidth: 300
            Layout.maximumWidth: 120
            Layout.preferredHeight: 32
            color: "#90caf9"
        }
    }
}
