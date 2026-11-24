// Ljós's own style (docs/DIALECT.md §4.16): a button of a tool bar,
// flat on the bar until the pointer is over it or it is pressed or
// checked, then on the accent's surface. Its icon (`iconName`, one of the
// style's own, wins over `icon.source`) is drawn before its text.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T
import Ljos.Design

T.ToolButton {
    id: control

    property string iconName: ""

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: Theme.spacingSmall
    leftPadding: Theme.spacing
    rightPadding: Theme.spacing
    spacing: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    font.weight: Font.Medium
    opacity: enabled ? 1 : 0.64
    scale: down && !Motion.reduced ? Motion.pressScale : 1
    PressBehavior on scale {}

    icon.width: Theme.iconSize
    icon.height: Theme.iconSize
    icon.color: Theme.text

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
            readonly property real top: under ? glyph.height + control.spacing : 0
            readonly property real stacked: top + label.implicitHeight
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
                color: Theme.text
            }

            // The button's own icon, or its action's, as IconLabel draws
            // it, where the style's own icon is not given.
            IconLabel {
                id: label
                objectName: "label"
                visible: parent.textShown || !parent.glyphShown
                x: control.mirrored ? 0 : parent.start
                y: parent.under ? (parent.height - parent.stacked) / 2 + parent.top : 0
                width: parent.width - parent.start
                height: parent.under ? implicitHeight : parent.height
                spacing: control.spacing
                mirrored: control.mirrored
                display: parent.glyphShown ? T.AbstractButton.TextOnly : control.display
                text: control.text
                font: control.font
                color: Theme.text
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: Theme.controlHeight
            implicitHeight: Theme.controlHeight
            radius: Theme.radius
            color: control.down || control.hovered || control.checked || control.highlighted
                   ? Theme.accentSurface : "transparent"
            // Checked: a ring inside its edge, so the state is not its fill alone.
            border.width: control.checked ? 2 : 0
            border.color: Theme.focus
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
