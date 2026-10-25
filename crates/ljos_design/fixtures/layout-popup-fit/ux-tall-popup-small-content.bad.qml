// expect: layout-popup-fit at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 700
    Popup {
        Component.onCompleted: open()
        width: 240; height: 640
        Column { Label { text: "Alpha" } Label { text: "Beta" } Label { text: "Gamma" } }
    }
}
