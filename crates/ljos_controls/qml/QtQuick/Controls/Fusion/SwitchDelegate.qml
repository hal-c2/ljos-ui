// The Fusion style of Ljós (CLEAN_ROOM.md), drawn with its parts in
// QtQuick.Controls.Fusion.impl: drawings of our own, fitted to what qml6 shows.
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Controls.Fusion.impl
import QtQuick.Templates as T

T.SwitchDelegate {
    // Not `control`: inside a part's component that names the part's
    // own property, which Fusion's parts have.
    id: host

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding,
                             implicitIndicatorHeight + topPadding + bottomPadding)

    padding: 6
    spacing: 6

    icon.width: 16
    icon.height: 16

    // A row of a list takes no focus of its own (its view has the keyboard),
    // so it is not a stop for an assistive technology's focus either,
    // whatever its role says.
    Accessible.focusable: host.focusPolicy !== Qt.NoFocus

    // At the far end of the row: the right, or the left when mirrored.
    indicator: Component {
        SwitchIndicator {
            x: host.mirrored ? host.leftPadding : host.width - width - host.rightPadding
            y: host.topPadding + (host.availableHeight - height) / 2
            control: host
        }
    }

    // The text keeps clear of the indicator at the far end.
    contentItem: Component {
        IconLabel {
            readonly property real gap: host.indicator ? host.indicator.width + host.spacing : 0
            leftPadding: host.mirrored ? gap : 0
            rightPadding: host.mirrored ? 0 : gap
            spacing: host.spacing
            mirrored: host.mirrored
            display: host.display
            alignment: host.display === IconLabel.IconOnly
                       || host.display === IconLabel.TextUnderIcon ? Qt.AlignCenter
                                                                   : Qt.AlignLeft
            text: host.text
            font: host.font
            color: host.highlighted ? host.palette.highlightedText : host.palette.text
        }
    }

    background: Component {
        Rectangle {
            implicitWidth: 100
            implicitHeight: 20
            readonly property color base: host.palette.base
            color: host.down ? Qt.rgba(base.r * 0.86, base.g * 0.86, base.b * 0.86, base.a)
                 : host.highlighted ? host.palette.highlight : base
        }
    }
}
