// expect: color-scheme-ignored at 11
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Popup {
        Component.onCompleted: open()
        padding: 16
        background: Rectangle { color: "#202020"; radius: 6 }
        Label { text: "Delete this item?"; color: palette.windowText }
    }
}
