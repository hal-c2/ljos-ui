import QtQuick
import QtQuick.Pdf

// One whole page at a time: the view is as large as the page.
Rectangle {
    id: root

    required property PdfDocument document
    property real renderScale: 1
    property bool zoomEnabled: true
    property string searchString
    readonly property var searchModel: null
    readonly property string selectedText: ""
    readonly property int currentPage: navigator.currentPage
    readonly property bool backEnabled: navigator.backAvailable
    readonly property bool forwardEnabled: navigator.forwardAvailable
    readonly property int status: image.status
    readonly property size sourceSize: Qt.size(Math.round(width), Math.round(height))

    readonly property size points: document.status, document.pagePointSize(Math.max(currentPage, 0))

    function back() { navigator.back() }
    function forward() { navigator.forward() }
    // A page past either end counts round, as qml6 does.
    function goToPage(page) {
        // Past either end comes round to the other, as paging on would.
        const last = Math.max(document.pageCount - 1, 0)
        goToLocation(page < 0 ? last : page > last ? 0 : page, Qt.point(0, 0), renderScale)
    }
    function goToLocation(page, location, zoom) { navigator.jump(page, location, zoom) }
    function resetScale() { renderScale = 1 }
    function scaleToWidth(width, height) { renderScale = width / points.width }
    function scaleToPage(width, height) {
        renderScale = Math.min(width / points.width, height / points.height)
    }

    // Finding and marking text in a page is not there yet: saying so beats
    // doing nothing without a word.
    function selectAll() { console.warn("PdfPageView: selecting text is not implemented") }
    function copySelectionToClipboard() { console.warn("PdfPageView: selecting text is not implemented") }
    function searchBack() { console.warn("PdfPageView: searching is not implemented") }
    function searchForward() { console.warn("PdfPageView: searching is not implemented") }

    width: Math.round(points.width * renderScale)
    height: Math.round(points.height * renderScale)
    color: "white"

    PdfPageNavigator {
        id: navigator
        onJumped: function(page, location, zoom) {
            if (zoom > 0)
                root.renderScale = zoom
        }
    }

    PdfPageImage {
        id: image
        anchors.fill: parent
        sourceSize: Qt.size(root.width, root.height)
        document: root.document
        currentFrame: root.currentPage
        asynchronous: true
    }

    WheelHandler {
        enabled: root.zoomEnabled
        acceptedModifiers: Qt.ControlModifier
        onWheel: function(event) {
            root.renderScale *= event.angleDelta.y > 0 ? 1.25 : 0.8
        }
    }
}
