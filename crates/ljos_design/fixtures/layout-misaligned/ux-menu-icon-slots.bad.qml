// expect: layout-misaligned at 11, layout-misaligned at 12
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "Open"; icon.name: "document-open" }
        MenuItem { text: "Save" }
        MenuItem { text: "Quit" }
    }
}
