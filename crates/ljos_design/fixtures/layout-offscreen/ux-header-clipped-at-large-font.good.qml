import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 120
    Rectangle {
        width: 360; implicitHeight: bar.height; height: implicitHeight; color: palette.window
        Flow {
            id: bar
            width: parent.width; spacing: 4
            Button { text: "Share" }
            Button { text: "Export" }
            Button { text: "Archive" }
            Button { text: "Delete" }
        }
    }
}
