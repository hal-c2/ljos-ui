// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Templates as T

T.PageIndicator {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             implicitContentHeight + topPadding + bottomPadding)

    padding: 4
    spacing: 4

    delegate: Component {
        Rectangle {
            required property int index

            implicitWidth: 6
            implicitHeight: 6
            radius: width / 2
            color: control.palette.shadow
            opacity: index === control.currentIndex ? 0.95 : pressed ? 0.7 : 0.45

            Behavior on opacity { NumberAnimation { duration: 100 } }
        }
    }

    contentItem: Component {
        Row {
            spacing: control.spacing
        }
    }
}
