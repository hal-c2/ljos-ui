// Ljós's own style (docs/DIALECT.md §4.16): what a view shows where it has
// nothing to show yet: an icon in a tile, a title, a line or two that says
// why, and at most two actions, the main one filled. `status` is "empty",
// "loading" (the icon turns, or pulses with reduced motion) or "error" (the
// icon is a warning).
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.Control {
    id: control

    property string status: "empty"
    property string iconName: "inbox"
    property string title: ""
    property string description: ""
    property string actionText: ""
    property string secondaryActionText: ""
    // Text wider than this wraps; a description keeps to two lines.
    property real maximumTextWidth: 360

    signal actionTriggered()
    signal secondaryActionTriggered()

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // Placed by its top, so the tile and title stay put as the actions
    // come and go; the view it fills gives the room around it.
    padding: 0
    spacing: Theme.spacing
    font.pixelSize: Theme.fontSize

    contentItem: Component {
        Column {
            spacing: control.spacing

            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: tile.width
                height: tile.height + Theme.spacingSmall

                Rectangle {
                    id: tile
                    objectName: "tile"
                    width: 40
                    height: 40
                    radius: Theme.radius
                    // A well in the page rather than a raised part, so the
                    // tile reads as a mark, not as something to press.
                    color: Theme.canvas
                    border.width: 1
                    border.color: Theme.border

                    Icon {
                        id: glyph
                        objectName: "glyph"
                        anchors.centerIn: parent
                        name: control.status === "loading" ? "circle-dashed"
                              : control.status === "error" ? "circle-alert" : control.iconName
                        size: Theme.iconSize + 8
                        color: control.status === "error" ? Theme.warningForeground : Theme.iconMuted
                        // Loading turns it; with reduced motion it pulses
                        // instead, so the wait still shows.
                        rotation: 0
                        opacity: 1

                        RotationAnimation on rotation {
                            running: control.status === "loading" && control.visible && !Motion.reduced
                            from: 0
                            to: 360
                            duration: 1600
                            loops: Animation.Infinite
                            // A stopped animation leaves the angle where it
                            // was, so the icon that follows would be tilted.
                            onRunningChanged: if (!running) glyph.rotation = 0
                        }
                        SequentialAnimation on opacity {
                            running: control.status === "loading" && control.visible && Motion.reduced
                            loops: Animation.Infinite
                            onRunningChanged: if (!running) glyph.opacity = 1
                            NumberAnimation { to: 0.4; duration: 800; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1; duration: 800; easing.type: Easing.InOutSine }
                        }
                    }
                }
            }

            Text {
                objectName: "title"
                visible: text.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(implicitWidth, control.maximumTextWidth)
                text: control.title
                color: Theme.text
                font.pixelSize: Theme.fontSize + 3
                font.weight: Font.DemiBold
                lineHeight: 22
                lineHeightMode: Text.FixedHeight
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                Accessible.role: Accessible.Heading
            }

            Text {
                objectName: "description"
                visible: text.length > 0
                anchors.horizontalCenter: parent.horizontalCenter
                width: Math.min(implicitWidth, control.maximumTextWidth)
                text: control.description
                color: Theme.textMuted
                font: control.font
                lineHeight: 18
                lineHeightMode: Text.FixedHeight
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
            }

            Item {
                visible: actions.visible
                anchors.horizontalCenter: parent.horizontalCenter
                width: actions.width
                height: actions.height + Theme.spacingSmall

                Row {
                    id: actions
                    y: Theme.spacingSmall
                    visible: control.actionText.length > 0 || control.secondaryActionText.length > 0
                    spacing: Theme.spacing
                    layoutDirection: control.mirrored ? Qt.RightToLeft : Qt.LeftToRight

                    Button {
                        objectName: "secondaryAction"
                        visible: control.secondaryActionText.length > 0
                        text: control.secondaryActionText
                        onClicked: control.secondaryActionTriggered()
                    }

                    Button {
                        objectName: "action"
                        visible: control.actionText.length > 0
                        primary: true
                        text: control.actionText
                        onClicked: control.actionTriggered()
                    }
                }
            }
        }
    }
}
