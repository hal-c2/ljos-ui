// Ljós's own style (docs/DIALECT.md §4.16): a round button, as a button
// is but with ends as round as its `radius` says (half its height unless
// told). Highlighted, it is filled with the accent, as a primary button
// is; checked, it has a ring inside its edge.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T
import Ljos.Design

T.RoundButton {
    id: control

    property string iconName: ""
    readonly property color ink: highlighted ? Theme.accentForeground : Theme.text

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: Theme.spacingSmall + 2
    spacing: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium
    opacity: enabled ? 1 : 0.64
    scale: down && !Motion.reduced ? Motion.pressScale : 1
    PressBehavior on scale {}

    icon.width: Theme.iconSize
    icon.height: Theme.iconSize
    icon.color: ink

    contentItem: Component {
        Item {
            readonly property bool glyphShown: control.iconName.length > 0
                                               && control.display !== T.AbstractButton.TextOnly
            readonly property bool textShown: control.text.length > 0
                                              && (control.display !== T.AbstractButton.IconOnly || !glyphShown)
            // The glyph over the text, both shown: one block in the middle.
            readonly property bool under: glyphShown && textShown
                                          && control.display === T.AbstractButton.TextUnderIcon
            readonly property real start: glyphShown && !under ? glyph.width + (textShown ? control.spacing : 0) : 0
            readonly property real textTop: under ? glyph.height + control.spacing : 0
            readonly property real stacked: textTop + label.implicitHeight
            implicitWidth: under ? Math.max(glyph.width, label.implicitWidth)
                                 : start + (textShown ? label.implicitWidth : 0)
            implicitHeight: under ? stacked
                                  : Math.max(textShown ? label.implicitHeight : 0, glyphShown ? glyph.height : 0)

            Icon {
                id: glyph
                objectName: "glyph"
                visible: parent.glyphShown
                x: parent.textShown && !parent.under ? (control.mirrored ? parent.width - width : 0)
                                                     : (parent.width - width) / 2
                y: parent.under ? (parent.height - parent.stacked) / 2 : (parent.height - height) / 2
                name: control.iconName
                color: control.ink
            }

            IconLabel {
                id: label
                objectName: "label"
                visible: parent.textShown || !parent.glyphShown
                x: control.mirrored ? 0 : parent.start
                y: parent.under ? (parent.height - parent.stacked) / 2 + parent.textTop : 0
                width: parent.width - parent.start
                height: parent.under ? implicitHeight : parent.height
                spacing: control.spacing
                mirrored: control.mirrored
                display: parent.glyphShown ? T.AbstractButton.TextOnly : control.display
                text: control.text
                font: control.font
                color: control.ink
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: Theme.controlHeight
            implicitHeight: Theme.controlHeight
            radius: control.radius
            color: control.highlighted
                   ? (control.down ? Theme.accentPressed : control.hovered ? Theme.accentHover : Theme.accent)
                   : control.down || control.hovered || control.checked ? Theme.accentSurface : Theme.surface
            border.width: control.highlighted ? 0 : 1
            border.color: Theme.input
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
