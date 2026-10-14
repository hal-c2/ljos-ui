// The Universal style of Ljós (CLEAN_ROOM.md). Draws with Basic-derived
// components under Universal's own metrics and attached palette.
import QtQuick
import QtQuick.Controls.Universal
import QtQuick.Templates as T

T.Pane {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    padding: 12

    background: Component {
        Rectangle {
            color: control.Universal.background
        }
    }
}
