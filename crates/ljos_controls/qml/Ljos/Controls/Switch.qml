// Ljós's own style (docs/DIALECT.md §4.16): a small track whose thumb
// slides to the accent when on.
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.Switch {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding,
                             Theme.controlHeight)

    padding: 0
    spacing: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    opacity: enabled ? 1 : 0.64

    indicator: Component {
        Rectangle {
            implicitWidth: 30
            implicitHeight: 18
            x: control.text.length > 0
               ? (control.mirrored ? control.width - width - control.rightPadding : control.leftPadding)
               : control.leftPadding + (control.availableWidth - width) / 2
            y: control.topPadding + (control.availableHeight - height) / 2
            radius: height / 2
            // Off, the track is the field's edge colour, which has 3:1 on a
            // raised fill; on, the accent.
            color: control.checked ? Theme.accent : Theme.input
            // Fades, so that with reduced motion, where the thumb is at its
            // end at once, the change still shows as it happens.
            Behavior on color {
                ColorAnimation { duration: Motion.moveSmall; easing.type: Easing.OutCubic }
            }

            Rectangle {
                objectName: "thumb"
                x: 2 + control.visualPosition * (parent.width - width - 4)
                y: 2
                width: parent.height - 4
                height: width
                radius: width / 2
                color: control.checked ? Theme.accentForeground : Theme.canvas

                Behavior on x {
                    NumberAnimation { duration: Motion.moveSmall; easing.type: Easing.OutCubic }
                }
            }
            // Outside the part, against the canvas: a part filled with the
            // accent would hide a ring drawn on its own edge.
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

    contentItem: Component {
        Item {
            property alias text: label.text
            property alias font: label.font
            property alias color: label.color

            implicitWidth: label.implicitWidth + label.gap
            implicitHeight: label.implicitHeight

            Text {
                id: label
                readonly property real gap: control.indicator
                                            ? control.indicator.width + control.spacing : 0
                x: control.mirrored ? 0 : gap
                width: parent.width - gap
                height: parent.height
                text: control.text
                font: control.font
                color: Theme.text
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
        }
    }
}
