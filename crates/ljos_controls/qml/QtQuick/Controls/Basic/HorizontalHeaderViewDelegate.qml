// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes and colours a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Controls.impl
import QtQuick.Templates as T

// The section's role named by the header's textRole, centred on the
// light colour in a midlight frame.
T.HeaderViewDelegate {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 8
    hoverEnabled: false
    focusPolicy: Qt.NoFocus
    highlighted: selected

    background: Component {
        Rectangle {
            color: control.palette.light
            border.color: control.palette.midlight
        }
    }

    contentItem: Component {
        Label {
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            color: control.palette.windowText
            text: control.model[control.headerView.textRole] ?? ""
        }
    }
}
