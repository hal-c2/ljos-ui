// Ljós's own style (docs/DIALECT.md §4.16): a compact button on the
// theme's roles. `primary` and `destructive` fill it; `subtle` leaves it
// on its ground until the pointer is over it; `chevron` says it opens
// something. Its icon (`icon.source`, or its action's) is drawn before
// the text in the text's colour; `iconName`, one of the style's own
// icons, wins over it. `display` shows the icon alone, the text alone or
// the icon over the text instead (T.AbstractButton.IconOnly, TextOnly,
// TextUnderIcon). A checked button
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

    // A picture larger than the style's icons is made no larger.
    icon.width: iconSize
    icon.height: iconSize

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    // A checkable one keeps room for the ring a check draws inside its
    // edge, whether checked or not, so checking it does not resize it.
    padding: control.checkable ? Theme.spacing + 4 : Theme.spacing
    // One line is centred in the background's height. The icon over the
    // text is taller than that, so the button grows to it and keeps half a
    // spacing above and below, as it keeps a spacing beside its text.
    topPadding: display === T.AbstractButton.TextUnderIcon ? Theme.spacing / 2 : 0
    bottomPadding: topPadding
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
            // What `display` lets it show: the icon alone, the text alone,
            // or both, the icon over the text or, by default, before it.
            readonly property bool showsIcon: control.display !== T.AbstractButton.TextOnly
            readonly property bool showsText: control.display !== T.AbstractButton.IconOnly
            readonly property bool under: control.display === T.AbstractButton.TextUnderIcon
            readonly property bool hasText: showsText && control.text.length > 0
            // What is drawn at the start, or over the text: the style's own
            // icon, else the button's or its action's.
            readonly property Item mark: glyph.visible ? glyph : picture.visible ? picture : null
            readonly property real textWidth: showsText ? label.implicitWidth : 0
            readonly property real textHeight: hasText ? label.implicitHeight : 0
            readonly property real start: mark && !under ? mark.width + (hasText ? control.spacing : 0) : 0
            readonly property real end: more.visible ? control.spacing + more.width : 0
            // Under, the mark and the text are one block in the middle.
            readonly property real top: mark && under ? mark.height + (hasText ? control.spacing : 0) : 0
            readonly property real stacked: top + textHeight
            implicitWidth: (under ? Math.max(mark ? mark.width : 0, textWidth) : start + textWidth) + end
            implicitHeight: under ? stacked
                                  : Math.max(showsText ? label.implicitHeight : 0,
                                             showsIcon ? control.iconSize : 0)

            // Mirroring moves anchors only, so each part is placed for
            // either direction. Under the text, the mark is in the middle of
            // what the chevron leaves.
            Icon {
                id: glyph
                objectName: "glyph"
                visible: control.iconName.length > 0 && parent.showsIcon
                x: parent.under ? (control.mirrored ? parent.end : 0) + (parent.width - parent.end - width) / 2
                                : control.mirrored ? parent.width - width : 0
                y: parent.under ? (parent.height - parent.stacked) / 2 : (parent.height - height) / 2
                name: control.iconName
                size: control.iconSize
                color: control.ink
            }

            // Text-less, it is the icon alone, taken from the button or the
            // action it follows, and drawn in the ink unless `icon.color`
            // says otherwise.
            IconLabel {
                id: picture
                objectName: "picture"
                visible: control.iconName.length === 0 && parent.showsIcon && implicitWidth > 0
                x: parent.under ? (control.mirrored ? parent.end : 0) + (parent.width - parent.end - width) / 2
                                : control.mirrored ? parent.width - width : 0
                y: parent.under ? (parent.height - parent.stacked) / 2 : (parent.height - height) / 2
                color: control.ink
            }

            // The text as its mnemonic says: the letter after a lone "&"
            // underlined, "&&" one "&". The keys read `control.text` as
            // written. Its own icon is none; the one at the start is drawn
            // beside it.
            IconLabel {
                id: label
                objectName: "label"
                visible: parent.showsText
                x: control.mirrored ? parent.end : parent.start
                y: parent.under ? (parent.height - parent.stacked) / 2 + parent.top : 0
                width: parent.width - parent.start - parent.end
                height: parent.under ? parent.textHeight : parent.height
                icon.name: ""
                icon.source: ""
                text: control.text
                font: control.font
                color: control.ink
                mirrored: control.mirrored
                // Left is the start: a mirrored label sets it at the right.
                // Under its mark, or with nothing beside it, it is centred.
                alignment: (parent.under || !parent.mark && !more.visible ? Qt.AlignHCenter : Qt.AlignLeft)
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
                   : control.subtle ? "transparent" : Theme.surface
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
