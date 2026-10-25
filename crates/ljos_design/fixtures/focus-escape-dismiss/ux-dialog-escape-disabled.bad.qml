// expect: focus-escape-dismiss at 10
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Dialog {
        title: "Settings"; standardButtons: Dialog.Close
        closePolicy: Popup.NoAutoClose
        Component.onCompleted: open()
        TextField { placeholderText: "Search" }
    }
}
