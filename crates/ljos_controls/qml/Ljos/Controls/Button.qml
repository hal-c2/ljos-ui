// Ljós's own style (docs/DIALECT.md §4.16): a compact button on the
// theme's roles. `primary` and `destructive` fill it; `subtle` leaves it
// on its ground until the pointer is over it; `chevron` says it opens
// something; `iconName` puts an icon before the text.
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.Button {
    id: control

    property bool primary: false
    property bool destructive: false
    property bool subtle: false
    property bool chevron: false
    property string iconName: ""
    property real iconSize: Theme.iconSize
    readonly property bool filled: primary || destructive
    readonly property color fill: destructive ? Theme.errorForeground : Theme.accent
    readonly property color ink: filled ? (destructive ? Theme.on(Theme.errorForeground)
                                                       : Theme.accentForeground)
                                        : Theme.text

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: Theme.spacing
    topPadding: 0
    bottomPadding: 0
    spacing: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium
    opacity: enabled ? 1 : 0.64
    // Gives under a press; with reduced motion it keeps its size, and the
    // fill alone says it is pressed.
    scale: down && !Motion.reduced ? Motion.pressScale : 1
    PressBehavior on scale {}

    contentItem: Component {
        Item {
            implicitWidth: (glyph.visible ? glyph.width + (label.text.length > 0 ? control.spacing : 0) : 0)
                           + label.implicitWidth
                           + (more.visible ? control.spacing + more.width : 0)
            implicitHeight: Math.max(label.implicitHeight, control.iconSize)

            Icon {
                id: glyph
                visible: control.iconName.length > 0
                y: (parent.height - height) / 2
                name: control.iconName
                size: control.iconSize
                color: control.ink
            }

            Text {
                id: label
                x: glyph.visible ? glyph.width + (text.length > 0 ? control.spacing : 0) : 0
                width: parent.width - x - (more.visible ? control.spacing + more.width : 0)
                height: parent.height
                text: control.text
                font: control.font
                color: control.ink
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: glyph.visible || more.visible ? Text.AlignLeft : Text.AlignHCenter
            }

            Icon {
                id: more
                visible: control.chevron
                x: parent.width - width
                y: (parent.height - height) / 2
                name: "chevron-down"
                size: Theme.iconSizeSmall
                color: control.filled ? control.ink : Theme.textMuted
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: Theme.controlHeight
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            color: control.filled
                   ? (control.down ? Qt.darker(control.fill, 1.15)
                      : control.hovered ? Qt.lighter(control.fill, 1.08) : control.fill)
                   : control.down || control.hovered || control.checked ? Theme.accentSurface
                   : control.subtle ? "transparent" : Theme.surfaceRaised
            // A ring where the keyboard is, when a key brought it there.
            border.width: control.filled || control.subtle ? 0 : 1
            border.color: Theme.input
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
}
