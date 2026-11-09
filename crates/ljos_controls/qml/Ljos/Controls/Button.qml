// Ljós's own style (docs/DIALECT.md §4.16): a compact button on the
// theme's roles. `primary` and `destructive` fill it; `subtle` leaves it
// on its ground until the pointer is over it; `chevron` says it opens
// something; `iconName` puts an icon before the text. A checked button
// has a ring inside its edge, so its state is not its colour alone.
import QtQuick
import QtQuick.Controls.impl
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
    // Each state's fill is a role, so its ratio with the ink is promised.
    readonly property color fill: destructive
                                  ? (down ? Theme.destructivePressed
                                     : hovered ? Theme.destructiveHover : Theme.destructive)
                                  : (down ? Theme.accentPressed
                                     : hovered ? Theme.accentHover : Theme.accent)
    readonly property color ink: filled ? (destructive ? Theme.destructiveForeground
                                                       : Theme.accentForeground)
                                        : Theme.text

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // A checkable one keeps room for the ring a check draws inside its
    // edge, whether checked or not, so checking it does not resize it.
    padding: control.checkable ? Theme.spacing + 4 : Theme.spacing
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

            // Mirroring moves anchors only, so each part is placed for
            // either direction.
            Icon {
                id: glyph
                objectName: "glyph"
                visible: control.iconName.length > 0
                x: control.mirrored ? parent.width - width : 0
                y: (parent.height - height) / 2
                name: control.iconName
                size: control.iconSize
                color: control.ink
            }

            // The text as its mnemonic says: the letter after a lone "&"
            // underlined, "&&" one "&". The keys read `control.text` as
            // written. Its own icon is none; the one at the start is drawn
            // beside it.
            IconLabel {
                id: label
                objectName: "label"
                readonly property real start: glyph.visible ? glyph.width + (text.length > 0 ? control.spacing : 0) : 0
                readonly property real end: more.visible ? control.spacing + more.width : 0
                x: control.mirrored ? end : start
                width: parent.width - start - end
                height: parent.height
                icon.name: ""
                icon.source: ""
                text: control.text
                font: control.font
                color: control.ink
                mirrored: control.mirrored
                // Left is the start: a mirrored label sets it at the right.
                alignment: (!glyph.visible && !more.visible ? Qt.AlignHCenter : Qt.AlignLeft)
                           | Qt.AlignVCenter
            }

            Icon {
                id: more
                objectName: "more"
                visible: control.chevron
                x: control.mirrored ? 0 : parent.width - width
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
            color: control.filled ? control.fill
                   : control.down || control.hovered || control.checked ? Theme.accentSurface
                   : control.subtle ? "transparent" : Theme.surfaceRaised
            // A ring where the keyboard is, when a key brought it there.
            border.width: control.filled || control.subtle ? 0 : 1
            border.color: Theme.input
            // Checked: a ring in the ink, inside the edge. A filled button's
            // fill stays the accent when checked, and an unfilled one's is
            // the same as hovered, so the ring is what says it.
            Rectangle {
                objectName: "checkedMark"
                x: 3
                y: 3
                width: parent.width - 6
                height: parent.height - 6
                radius: Math.max(0, parent.radius - 3)
                color: "transparent"
                border.width: 2
                border.color: control.ink
                visible: control.checked
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
}
