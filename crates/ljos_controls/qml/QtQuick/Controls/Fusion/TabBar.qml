// The Fusion style of Ljós (CLEAN_ROOM.md), from the sizes and colours a
// running Qt reports.
import QtQuick
import QtQuick.Templates as T

T.TabBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    // The tabs overlap by their border.
    spacing: -1

    // A row of the buttons that scrolls when they do not fit, and keeps
    // the current one in view.
    contentItem: Component {
        ListView {
            model: control.contentModel
            currentIndex: control.currentIndex

            spacing: control.spacing
            orientation: ListView.Horizontal
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.AutoFlickIfNeeded
            snapMode: ListView.SnapToItem

            highlightMoveDuration: 0
            highlightRangeMode: ListView.ApplyRange
            preferredHighlightBegin: 40
            preferredHighlightEnd: width - 40
        }
    }

    // The tabs stand on a line along the bottom of the bar.
    background: Component {
        Item {
            implicitHeight: 21

            Rectangle {
                y: parent.height - 1
                width: parent.width
                height: 1
                color: Qt.darker(control.palette.window, 1.4)
            }
        }
    }
}
