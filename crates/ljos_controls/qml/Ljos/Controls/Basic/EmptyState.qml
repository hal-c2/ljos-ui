// Ljos.Controls.Basic's empty state (docs/DIALECT.md §4.16): the same
// title, description and actions as Ljos.Controls' (its file says what
// each is for), in the Basic style's labels and buttons. The Basic style
// has no icons, so "loading" is said in words instead.
import QtQuick
import QtQuick.Templates as T
import QtQuick.Controls.Basic as Basic

T.Control {
    id: control

    property string status: "empty"
    property string iconName: "inbox"
    property string title: ""
    property string description: ""
    property string actionText: ""
    property string secondaryActionText: ""
    property real maximumTextWidth: 360

    signal actionTriggered()
    signal secondaryActionTriggered()

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 12
    spacing: 6

    contentItem: Component {
        Column {
            spacing: control.spacing

            Basic.Label {
                objectName: "title"
                visible: text.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(implicitWidth, control.maximumTextWidth)
                text: control.status === "loading" ? qsTr("Loading…")
                      : control.title
                font.pixelSize: control.font.pixelSize + 4
                font.bold: true
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                Accessible.role: Accessible.Heading
            }

            Basic.Label {
                objectName: "description"
                visible: text.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(implicitWidth, control.maximumTextWidth)
                text: control.description
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }

            Row {
                visible: control.actionText.length > 0 || control.secondaryActionText.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: control.spacing
                layoutDirection: control.mirrored ? Qt.RightToLeft : Qt.LeftToRight

                Basic.Button {
                    objectName: "secondaryAction"
                    visible: control.secondaryActionText.length > 0
                    text: control.secondaryActionText
                    onClicked: control.secondaryActionTriggered()
                }

                Basic.Button {
                    objectName: "action"
                    visible: control.actionText.length > 0
                    highlighted: true
                    text: control.actionText
                    onClicked: control.actionTriggered()
                }
            }
        }
    }
}
