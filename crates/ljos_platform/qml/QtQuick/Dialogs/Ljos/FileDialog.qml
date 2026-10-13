// The file and folder dialog Ljós draws when the system shows none, or
// when it is told not to use the system's: the path, typed.
import QtQuick
import QtQuick.Controls

Dialog {
    id: popup

    // The FileDialog or FolderDialog this shows; set when the popup is made.
    property var dialog
    readonly property string path: field.text

    // How the popup ended: accepted (1) or not (0), and no button.
    signal answered(int accepted, int button)

    // The path of a file: URL, for the field to start with.
    function pathOf(url) {
        var s = url === undefined || url === null ? "" : "" + url
        return s.indexOf("file://") === 0 ? decodeURIComponent(s.substring(7)) : s
    }

    title: dialog ? dialog.title : ""
    modal: true
    anchors.centerIn: parent
    standardButtons: Dialog.Ok | Dialog.Cancel

    onAccepted: answered(1, 0)
    onRejected: answered(0, 0)

    Column {
        spacing: 8

        Label {
            text: qsTr("Path")
        }

        TextField {
            id: field
            focus: true
            width: 300
            text: !popup.dialog ? ""
                  : popup.pathOf(popup.dialog.selectedFile !== undefined
                                 ? popup.dialog.selectedFile : popup.dialog.selectedFolder)
            Accessible.name: "Path"
        }
    }
}
