import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    width: 400; height: 300
    Shortcut { sequence: "Ctrl+W"; enabled: !dlg.opened; onActivated: console.log("close tab") }
    Dialog {
        id: dlg; modal: true; title: "Rename"; standardButtons: Dialog.Ok
        Component.onCompleted: open()
        TextField {}
    }
}
