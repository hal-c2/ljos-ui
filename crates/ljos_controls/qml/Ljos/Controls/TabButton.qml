// Ljós's own style (docs/DIALECT.md §4.16): a tab of a TabBar, as wide as
// its text, so a strip of them reads as a row of words. The current tab is
// raised, and its text is heavier and in the text's colour; the others are
// muted, and the pointer over one lifts it a little. The weight is what
// says which is current to a reader who cannot tell the colours apart. `iconName`, one of the style's own
// icons, is drawn before the text.
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.TabButton {
    id: control

    property string iconName: ""
    readonly property color ink: checked ? Theme.text : Theme.textMuted

    // Its own width, so the bar does not share its width out among the
    // tabs: a tab is as wide as what it says.
    width: implicitWidth
    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    horizontalPadding: Theme.spacing
    verticalPadding: 0
    spacing: Theme.spacingSmall
    hoverEnabled: true
    font.pixelSize: Theme.fontSizeSmall
    opacity: enabled ? 1 : 0.64

    contentItem: Component {
        Item {
            implicitWidth: row.implicitWidth
            implicitHeight: row.implicitHeight

            Row {
                id: row
                anchors.centerIn: parent
                spacing: control.spacing
                layoutDirection: control.mirrored ? Qt.RightToLeft : Qt.LeftToRight

                Icon {
                    objectName: "glyph"
                    visible: control.iconName.length > 0
                    anchors.verticalCenter: parent.verticalCenter
                    name: control.iconName
                    size: Theme.iconSizeSmall
                    color: control.ink
                }

                Text {
                    objectName: "label"
                    anchors.verticalCenter: parent.verticalCenter
                    // As wide as the heavier text, so a tab keeps its
                    // width as it becomes current and the strip stays put.
                    width: Math.max(implicitWidth, Math.ceil(heavy.advanceWidth))
                    horizontalAlignment: Text.AlignHCenter
                    text: control.text
                    font.family: control.font.family
                    font.pixelSize: control.font.pixelSize
                    font.weight: control.checked ? Font.DemiBold : control.font.weight
                    color: control.ink
                    elide: Text.ElideRight

                    TextMetrics {
                        id: heavy
                        font.family: control.font.family
                        font.pixelSize: control.font.pixelSize
                        font.weight: Font.DemiBold
                        text: control.text
                    }
                }
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: Theme.controlHeight
            implicitHeight: Theme.controlHeight
            radius: Theme.radiusMedium
            color: control.checked ? Theme.surfaceRaised
                   : control.hovered || control.down ? Theme.surface : "transparent"
            Behavior on color {
                ColorAnimation { duration: Motion.moveSmallFrequent; easing.type: Easing.OutCubic }
            }
            // Outside the part, against the strip.
            Rectangle {
                objectName: "focusRing"
                x: -3
                y: -3
                width: parent.width + 6
                height: parent.height + 6
                radius: parent.radius + 3
                color: "transparent"
                border.width: 2
                border.color: Theme.focus
                visible: control.visualFocus
            }
        }
    }
}
