// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

T.ItemDelegate {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 16
    icon.height: 16

    // A row starts at its edge: the icon, then the text. An icon alone,
    // or one over its text, is in the middle.
    contentItem: Component {
        IconLabel {
            spacing: control.spacing
            mirrored: control.mirrored
            display: control.display
            alignment: control.display === IconLabel.IconOnly
                       || control.display === IconLabel.TextUnderIcon ? Qt.AlignCenter
                                                                      : Qt.AlignLeft
            text: control.text
            font: control.font
            color: control.highlighted ? control.palette.highlightedText : control.palette.text
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 100
            implicitHeight: 20
            readonly property color base: control.palette.base
            color: control.down ? Qt.rgba(base.r * 0.86, base.g * 0.86, base.b * 0.86, base.a)
                 : control.highlighted ? control.palette.highlight : base
        }
    }
}
