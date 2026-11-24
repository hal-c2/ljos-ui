// Ljós's own style (docs/DIALECT.md §4.16): a button that acts only
// once it has been held for its delay. While it is held a bar in the
// focus colour fills along its foot, so the text stays on the surface it
// is read on; once it has acted it is filled with the accent, as a
// primary button is. The bar follows `progress`, which the template
// moves, and with reduced motion the template's own progress is still
// the time held: it says how long is left, it is not an ornament.
import QtQuick
import QtQuick.Templates as T

T.DelayButton {
    id: control

    readonly property color ink: checked ? Theme.accentForeground : Theme.text

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: Theme.spacingSmall
    leftPadding: Theme.spacing + Theme.spacingSmall
    rightPadding: Theme.spacing + Theme.spacingSmall
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium
    opacity: enabled ? 1 : 0.64

    contentItem: Component {
        Text {
            objectName: "label"
            text: control.text
            font: control.font
            color: control.ink
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 80
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            color: control.checked ? Theme.accent
                   : control.down || control.hovered ? Theme.accentSurface : Theme.surface
            border.width: control.checked ? 0 : 1
            border.color: Theme.input

            Rectangle {
                objectName: "held"
                // Inside the rounded corners, so it needs no clip that
                // would cut the focus ring off too.
                readonly property real inset: parent.radius
                x: control.mirrored ? parent.width - inset - width : inset
                y: parent.height - height - 2
                width: control.progress * (parent.width - 2 * inset)
                height: 3
                radius: height / 2
                color: Theme.focus
                visible: !control.checked && control.progress > 0
            }
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
