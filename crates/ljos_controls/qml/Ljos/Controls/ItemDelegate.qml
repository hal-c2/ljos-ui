// Ljós's own style (docs/DIALECT.md §4.16): a row of a list, as tall as
// a control with room to breathe, with an optional icon (`iconName`,
// one of the style's own, wins over `icon.source`) before its text. The
// row under the pointer or pressed is on the accent's surface; the
// highlighted one, and the one the keys are on, have a ring as well, so
// which it is is not its fill alone.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.ItemDelegate {
    id: control

    property string iconName: ""

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: Theme.spacing
    leftPadding: Theme.spacing + Theme.spacingSmall
    rightPadding: Theme.spacing + Theme.spacingSmall
    spacing: Theme.spacing
    hoverEnabled: true
    font.pixelSize: Theme.fontSize
    opacity: enabled ? 1 : 0.64

    icon.width: Theme.iconSize
    icon.height: Theme.iconSize
    icon.color: Theme.textMuted

    contentItem: Component {
        Item {
            readonly property bool glyphShown: control.iconName.length > 0
                                               && control.display !== T.AbstractButton.TextOnly
            readonly property real start: glyphShown ? glyph.width + control.spacing : 0
            implicitWidth: start + label.implicitWidth
            implicitHeight: Math.max(label.implicitHeight, glyphShown ? glyph.height : 0)

            Icon {
                id: glyph
                objectName: "glyph"
                visible: parent.glyphShown
                x: control.mirrored ? parent.width - width : 0
                y: (parent.height - height) / 2
                name: control.iconName
                color: Theme.textMuted
            }

            // The row's own icon, or its action's, as IconLabel draws it,
            // where the style's own icon is not given.
            IconLabel {
                id: label
                objectName: "label"
                x: control.mirrored ? 0 : parent.start
                width: parent.width - parent.start
                height: parent.height
                spacing: control.spacing
                mirrored: control.mirrored
                display: parent.glyphShown && control.display === T.AbstractButton.TextBesideIcon
                         ? T.AbstractButton.TextOnly : control.display
                alignment: control.display === T.AbstractButton.IconOnly
                           || control.display === T.AbstractButton.TextUnderIcon
                           ? Qt.AlignCenter : Qt.AlignLeft | Qt.AlignVCenter
                text: control.text
                font: control.font
                color: Theme.text
            }
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 120
            implicitHeight: Theme.controlHeight + Theme.spacing
            radius: Theme.radiusMedium
            color: control.down || control.hovered || control.highlighted || control.visualFocus
                   ? Theme.accentSurface : "transparent"
            border.width: control.highlighted || control.visualFocus ? 2 : 0
            border.color: Theme.focus
        }
    }
}
