// expect: layout-offscreen at 16
// audit: font-scale=2
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 360; height: 120
    Rectangle {
        width: 360; height: 48; clip: true; color: palette.window
        Row {
            spacing: 4
            Button { text: "Share" }
            Button { text: "Export" }
            Button { text: "Archive" }
            Button { text: "Delete" }
        }
    }
}
