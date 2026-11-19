// Ljos.Controls.Basic's empty state (docs/DIALECT.md §4.16): the same
// title, description and actions as Ljos.Controls' (its file says what
// each is for), in the Basic style's labels and buttons. The Basic style
// has no icons, so "loading" and an error without a title are said in
// words instead.
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

    // The text keeps to the width the control is given; what it asks for
    // is still its text's width up to `maximumTextWidth` (EmptyState.qml
    // of Ljos.Controls says why).
    contentItem: Component {
        Item {
            implicitWidth: Math.max(title.text.length > 0 ? Math.min(title.implicitWidth, control.maximumTextWidth) : 0,
                                    description.text.length > 0 ? Math.min(description.implicitWidth, control.maximumTextWidth) : 0,
                                    control.actionText.length > 0 || control.secondaryActionText.length > 0 ? actions.implicitWidth : 0)
            implicitHeight: column.implicitHeight

            Column {
                id: column
                width: parent.width
                spacing: control.spacing

                Basic.Label {
                    id: title
                    objectName: "title"
                    visible: text.length > 0
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: Math.min(implicitWidth, control.maximumTextWidth, column.width)
                    text: control.status === "loading" ? qsTr("Loading…")
                          : control.title.length > 0 ? control.title
                          : control.status === "error" ? qsTr("Something went wrong") : ""
                    font.pixelSize: control.font.pixelSize + 4
                    font.bold: true
                    horizontalAlignment: Text.AlignHCenter
                    elide: Text.ElideRight
                    Accessible.role: Accessible.Heading
                }

                Basic.Label {
                    id: description
                    objectName: "description"
                    visible: text.length > 0
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: Math.min(implicitWidth, control.maximumTextWidth, column.width)
                    text: control.description
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                }

                Row {
                    id: actions
                    visible: control.actionText.length > 0 || control.secondaryActionText.length > 0
                    anchors.horizontalCenter: parent.horizontalCenter
                    spacing: control.spacing
                    layoutDirection: control.mirrored ? Qt.RightToLeft : Qt.LeftToRight
                    // The direction is the control's own mirroring already: the row
                    // mirrored again by what the control passes on would undo it.
                    LayoutMirroring.enabled: false

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
}
