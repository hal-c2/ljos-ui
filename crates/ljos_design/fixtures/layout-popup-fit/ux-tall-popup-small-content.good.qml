import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 700
    Popup {
        Component.onCompleted: open()
        width: 240; implicitHeight: contentHeight + topPadding + bottomPadding
        Column { Label { text: "Alpha" } Label { text: "Beta" } Label { text: "Gamma" } }
    }
}
