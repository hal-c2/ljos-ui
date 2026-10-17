// The Material style of Ljós (CLEAN_ROOM.md): Basic's row under Material's
// metrics and colours, as a running Qt reports them.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Material
import QtQuick.Templates as T

T.ItemDelegate {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 16
    verticalPadding: 8
    spacing: 16

    icon.width: 24
    icon.height: 24

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
            color: control.enabled ? control.Material.foreground : control.Material.hintTextColor
        }
    }

    // A tint of the theme's text colour: faint when the row is current,
    // fainter while it is pressed.
    background: Component {
        Rectangle {
            readonly property bool dark: control.Material.theme === Material.Dark
            implicitHeight: control.Material.delegateHeight
            color: control.highlighted ? (dark ? "#1effffff" : "#1e000000")
                 : control.down ? (dark ? "#1fffffff" : "#10000000")
                 : "transparent"
        }
    }
}
