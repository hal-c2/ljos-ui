// The message Ljós draws when the system cannot show it (its buttons are
// not ones every system has, or there is no system dialog at all).
import QtQuick
import QtQuick.Controls

Dialog {
    id: popup

    // The MessageDialog this shows; set when the popup is made.
    property var dialog

    // How the popup ended: a button pressed (1 and its flag), or closed
    // without one (0, 0).
    signal answered(int accepted, int button)

    title: dialog ? dialog.title : ""
    modal: true
    anchors.centerIn: parent

    onRejected: answered(0, 0)

    Column {
        spacing: 8

        Label {
            text: popup.dialog ? popup.dialog.text : ""
        }

        Label {
            visible: text.length > 0
            text: popup.dialog ? popup.dialog.informativeText : ""
        }

        DialogButtonBox {
            id: box
            focus: true
            standardButtons: popup.dialog ? popup.dialog.buttons : 0
            onClicked: button => {
                // The standard buttons are the bits from Ok to RestoreDefaults.
                for (var flag = 0x400; flag <= 0x8000000; flag *= 2) {
                    if (box.standardButton(flag) === button) {
                        popup.answered(1, flag)
                        return
                    }
                }
            }
        }
    }
}
