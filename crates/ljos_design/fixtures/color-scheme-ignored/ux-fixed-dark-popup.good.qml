import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Popup {
        Component.onCompleted: open()
        padding: 16
        background: Rectangle { color: palette.window; border.color: palette.mid; radius: 6 }
        Label { text: "Delete this item?"; color: palette.windowText }
    }
}
