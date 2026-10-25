import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "Open"; icon.name: "document-open" }
        MenuItem { text: "Save"; icon.name: "document-save" }
        MenuItem { text: "Quit"; icon.name: "application-exit" }
    }
}
