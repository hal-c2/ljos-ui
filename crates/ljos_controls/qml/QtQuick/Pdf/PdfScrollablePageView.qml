import QtQuick
import QtQuick.Controls
import QtQuick.Pdf

// One page at a time, in a view that scrolls around it.
Flickable {
    id: root

    required property PdfDocument document
    property real renderScale: 1
    property real pageRotation: 0
    property string searchString
    readonly property var searchModel: null
    readonly property string selectedText: ""
    readonly property int currentPage: navigator.currentPage
    readonly property bool backEnabled: navigator.backAvailable
    readonly property bool forwardEnabled: navigator.forwardAvailable
    readonly property int status: image.status
    readonly property size sourceSize: Qt.size(Math.round(points.width * renderScale), Math.round(points.height * renderScale))

    readonly property size points: document.status, document.pagePointSize(Math.max(currentPage, 0))
    readonly property bool turned: (Math.abs(pageRotation) % 180) === 90
    readonly property size shown: turned ? Qt.size(points.height, points.width) : points

    function back() { navigator.back() }
    function forward() { navigator.forward() }
    function goToPage(page) { goToLocation(page, Qt.point(0, 0), renderScale) }
    function goToLocation(page, location, zoom) { navigator.jump(page, location, zoom) }
    function resetScale() { renderScale = 1 }
    function scaleToWidth(width, height) {
        renderScale = width / shown.width
    }
    function scaleToPage(width, height) {
        renderScale = Math.min(width / shown.width, height / shown.height)
    }

    // Finding and marking text in a page is not there yet: saying so beats
    // doing nothing without a word.
    function selectAll() { console.warn("PdfScrollablePageView: selecting text is not implemented") }
    function copySelectionToClipboard() { console.warn("PdfScrollablePageView: selecting text is not implemented") }
    function searchBack() { console.warn("PdfScrollablePageView: searching is not implemented") }
    function searchForward() { console.warn("PdfScrollablePageView: searching is not implemented") }

    clip: true
    contentWidth: shown.width * renderScale
    contentHeight: shown.height * renderScale

    ScrollBar.vertical: ScrollBar {}
    ScrollBar.horizontal: ScrollBar {}

    PdfPageNavigator {
        id: navigator
        onJumped: function(page, location, zoom) {
            if (zoom > 0)
                root.renderScale = zoom
            root.contentX = Math.max(0, Math.min(location.x * root.renderScale, root.contentWidth - root.width))
            root.contentY = Math.max(0, Math.min(location.y * root.renderScale, root.contentHeight - root.height))
        }
    }

    Rectangle {
        width: root.contentWidth
        height: root.contentHeight
        color: "white"

        PdfPageImage {
            id: image
            anchors.centerIn: parent
            width: root.points.width * root.renderScale
            height: root.points.height * root.renderScale
            sourceSize: Qt.size(width, height)
            rotation: root.pageRotation
            document: root.document
            currentFrame: root.currentPage
            asynchronous: true
        }
    }
}
