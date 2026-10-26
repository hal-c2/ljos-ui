// expect: focus-indicator-missing at 14
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Popup {
        Component.onCompleted: open()
        modal: true
        focus: true
        padding: 16
        background: Rectangle { color: palette.window; border.color: palette.mid; radius: 6 }
        Rectangle {
            width: 200; height: 40; color: palette.base
            focus: true
            activeFocusOnTab: true
            Accessible.role: Accessible.EditableText
            Accessible.name: "Hex"
        }
    }
}
