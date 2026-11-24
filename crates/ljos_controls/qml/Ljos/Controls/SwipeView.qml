// Ljós's own style (docs/DIALECT.md §4.16): one page at a time. A swipe
// comes to rest on a page, and the view moves to the current one. With
// reduced motion it does not slide: it is on the page at once, and the
// page fades in, so the change still shows.
import QtQuick
import QtQuick.Templates as T
import Ljos.Design

T.SwipeView {
    id: control

    implicitWidth: Math.max(implicitBackgroundWidth + leftInset + rightInset,
                            contentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(implicitBackgroundHeight + topInset + bottomInset,
                             contentHeight + topPadding + bottomPadding)

    font.pixelSize: Theme.fontSize

    contentItem: Component {
        ListView {
            id: pages
            model: control.contentModel
            interactive: control.interactive
            currentIndex: control.currentIndex
            focus: control.focus

            spacing: control.spacing
            orientation: control.orientation
            snapMode: ListView.SnapOneItem
            boundsBehavior: Flickable.StopAtBounds

            highlightRangeMode: ListView.StrictlyEnforceRange
            preferredHighlightBegin: 0
            preferredHighlightEnd: 0
            highlightMoveDuration: Motion.reduced ? 0 : Motion.movePanel

            // A page reached without a swipe: what the slide shows is
            // shown by a fade instead.
            onCurrentIndexChanged: if (Motion.reduced && !moving) arrive.restart()
            NumberAnimation {
                id: arrive
                target: pages
                property: "opacity"
                from: 0
                to: 1
                duration: Motion.enterSmall
                easing.type: Easing.OutCubic
            }
        }
    }
}
