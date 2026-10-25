// expect: focus-popup-stranded at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Popup {
        modal: true; focus: false
        closePolicy: Popup.NoAutoClose
        Component.onCompleted: open()
        Label { text: "Choose a colour" }
    }
}
