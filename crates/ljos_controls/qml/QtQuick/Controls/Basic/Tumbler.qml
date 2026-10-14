// The Basic style of Ljós, written from the Qt Quick Controls documentation
// and from the sizes a running Qt reports (CLEAN_ROOM.md).
import QtQuick
import QtQuick.Templates as T

T.Tumbler {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            60 + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             200 + topPadding + bottomPadding)

    // A row fades towards the ends, so the middle one reads as chosen.
    delegate: Text {
        required property var modelData
        required property int index

        text: modelData
        font: control.font
        color: control.palette.windowText
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        opacity: 1.0 - Math.abs(Tumbler.displacement) / (control.visibleItemCount / 2)
    }

    // The rows are a path that turns when there are enough of them, and
    // a list that stops at its ends when there are not.
    contentItem: Component {
        Loader {
            sourceComponent: control.wrap ? turning : listed
        }
    }

    Component {
        id: turning
        PathView {
            width: control.availableWidth
            height: control.availableHeight
            model: control.model
            delegate: control.delegate
            currentIndex: control.currentIndex
            clip: true
            pathItemCount: control.visibleItemCount + 1
            preferredHighlightBegin: 0.5
            preferredHighlightEnd: 0.5
            highlightRangeMode: PathView.StrictlyEnforceRange
            flickDeceleration: control.flickDeceleration
            path: Path {
                startX: control.availableWidth / 2
                startY: -control.availableHeight / control.visibleItemCount / 2
                PathLine {
                    x: control.availableWidth / 2
                    y: control.availableHeight + control.availableHeight / control.visibleItemCount / 2
                }
            }
        }
    }

    Component {
        id: listed
        ListView {
            width: control.availableWidth
            height: control.availableHeight
            model: control.model
            delegate: control.delegate
            currentIndex: control.currentIndex
            clip: true
            orientation: ListView.Vertical
            boundsBehavior: Flickable.StopAtBounds
            // The template keeps the current row in the middle; the margins
            // let the first and the last come there.
            topMargin: (height - height / control.visibleItemCount) / 2
            bottomMargin: topMargin
            flickDeceleration: control.flickDeceleration
        }
    }
}
