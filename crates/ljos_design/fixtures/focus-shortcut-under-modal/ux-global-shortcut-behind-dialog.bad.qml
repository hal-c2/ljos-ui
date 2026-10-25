// expect: focus-shortcut-under-modal at 8
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Shortcut { sequence: "Ctrl+W"; onActivated: console.log("close tab") }
    Dialog {
        id: dlg; modal: true; title: "Rename"; standardButtons: Dialog.Ok
        Component.onCompleted: open()
        TextField {}
    }
}
