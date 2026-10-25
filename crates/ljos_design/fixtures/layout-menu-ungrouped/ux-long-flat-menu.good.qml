import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 500
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "Item 1" } MenuItem { text: "Item 2" } MenuItem { text: "Item 3" }
        MenuItem { text: "Item 4" } MenuItem { text: "Item 5" } MenuItem { text: "Item 6" }
        MenuSeparator {}
        MenuItem { text: "Item 7" } MenuItem { text: "Item 8" } MenuItem { text: "Item 9" }
        MenuItem { text: "Item 10" } MenuItem { text: "Item 11" } MenuItem { text: "Item 12" }
    }
}
