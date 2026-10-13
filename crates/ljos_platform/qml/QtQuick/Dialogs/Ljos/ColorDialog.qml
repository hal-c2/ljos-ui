// The colour dialog Ljós draws when the system shows none: the colour as
// text, which a screen reader reads and a keyboard edits.
import QtQuick
import QtQuick.Controls

Dialog {
    id: popup

    // The ColorDialog this shows; set when the popup is made.
    property var dialog
    readonly property string colorText: hex.text

    // How the popup ended: accepted (1) or not (0), and no button.
    signal answered(int accepted, int button)

    title: dialog ? dialog.title : ""
    modal: true
    anchors.centerIn: parent
    standardButtons: Dialog.Ok | Dialog.Cancel

    onAccepted: answered(1, 0)
    onRejected: answered(0, 0)

    Column {
        spacing: 8

        Label {
            text: qsTr("Colour")
        }

        TextField {
            id: hex
            focus: true
            width: 200
            text: popup.dialog ? "" + popup.dialog.selectedColor : ""
            Accessible.name: "Hex"
        }
    }
}
