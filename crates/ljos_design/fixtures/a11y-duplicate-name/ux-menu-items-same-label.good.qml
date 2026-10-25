import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 300; height: 200
    Menu {
        Component.onCompleted: open()
        MenuItem { text: "notes (personal)" }
        MenuItem { text: "notes (work)" }
    }
}
