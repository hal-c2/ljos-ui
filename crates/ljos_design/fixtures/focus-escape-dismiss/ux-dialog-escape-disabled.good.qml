import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Dialog {
        title: "Settings"; standardButtons: Dialog.Close
        closePolicy: Popup.CloseOnEscape
        Component.onCompleted: open()
        TextField { placeholderText: "Search" }
    }
}
