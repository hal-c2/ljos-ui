// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
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

    padding: 12
    spacing: 8

    icon.width: 24
    icon.height: 24

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

    // A row shows only while something is going on in it.
    background: Component {
        Rectangle {
            implicitWidth: 100
            implicitHeight: 40
            visible: control.down || control.highlighted || control.visualFocus
            color: control.down ? control.palette.midlight : control.palette.light
        }
    }
}
