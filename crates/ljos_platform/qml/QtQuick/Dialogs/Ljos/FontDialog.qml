// The font dialog Ljós draws when the system shows none: the family, and
// whether the font is bold and italic.
import QtQuick
import QtQuick.Controls

Dialog {
    id: popup

    // The FontDialog this shows; set when the popup is made.
    property var dialog
    readonly property string family: familyField.text
    readonly property bool bold: boldBox.checked
    readonly property bool italic: italicBox.checked

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

        TextField {
            id: familyField
            focus: true
            width: 200
            text: popup.dialog ? popup.dialog.selectedFont.family : ""
            Accessible.name: "Family"
        }

        CheckBox {
            id: boldBox
            text: qsTr("Bold")
            checked: popup.dialog ? popup.dialog.selectedFont.bold : false
        }

        CheckBox {
            id: italicBox
            text: qsTr("Italic")
            checked: popup.dialog ? popup.dialog.selectedFont.italic : false
        }
    }
}
