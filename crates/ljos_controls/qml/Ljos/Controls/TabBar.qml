// Ljós's own style (docs/DIALECT.md §4.16): a strip of tabs, each as wide
// as its text, from the start of the strip. The strip is the ground under
// it; the current tab is raised from it (TabButton.qml).
import QtQuick
import QtQuick.Templates as T

T.TabBar {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    // Tabs that do not fit end at the strip's edge rather than drawing over
    // what is beside it; the padding inside keeps a tab's ring whole.
    clip: true
    spacing: 2
    horizontalPadding: Theme.spacing
    verticalPadding: Theme.spacingSmall

    // A row of the tabs that scrolls when they do not fit, and keeps the
    // current one in view.
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

    // Nothing of its own: a strip on a pane takes the pane's colour.
    background: Component {
        Item {
            implicitHeight: Theme.controlHeight + 2 * Theme.spacingSmall
        }
    }
}
