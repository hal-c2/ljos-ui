import QtQuick
import QtQuick.Controls
import QtQuick.Pdf

// All the pages one below the other, each centred, in a view that scrolls.
Item {
    id: root

    required property PdfDocument document
    property int currentPageRenderingStatus: Image.Null
    property real renderScale: 1
    property real pageRotation: 0
    property string searchString
    readonly property var searchModel: null
    readonly property string selectedText: ""
    readonly property int currentPage: navigator.currentPage
    readonly property bool backEnabled: navigator.backAvailable
    readonly property bool forwardEnabled: navigator.forwardAvailable

    function back() { navigator.back() }
    function forward() { navigator.forward() }
    function goToPage(page) { goToLocation(page, Qt.point(0, 0), renderScale) }
    function goToLocation(page, location, zoom) { navigator.jump(page, location, zoom) }
    function resetScale() { renderScale = 1 }
    function scaleToWidth(width, height) {
        const first = turnedSize(document.pagePointSize(0))
        renderScale = (turned() ? height : width) / first.width
    }
    function scaleToPage(width, height) {
        const first = turnedSize(document.pagePointSize(0))
        renderScale = Math.min(width / first.width, height / first.height)
    }

    // Finding and marking text in a page is not there yet: saying so beats
    // doing nothing without a word.
    function selectAll() { console.warn("PdfMultiPageView: selecting text is not implemented") }
    function copySelectionToClipboard() { console.warn("PdfMultiPageView: selecting text is not implemented") }
    function searchBack() { console.warn("PdfMultiPageView: searching is not implemented") }
    function searchForward() { console.warn("PdfMultiPageView: searching is not implemented") }

    function turned() { return (Math.abs(pageRotation) % 180) === 90 }
    function turnedSize(points) {
        return turned() ? Qt.size(points.height, points.width) : points
    }

    onRenderScaleChanged: pages.showCurrentPage()

    PdfPageNavigator {
        id: navigator
        onJumped: function(page, location, zoom) {
            if (zoom > 0)
                root.renderScale = zoom
            pages.show(page, location)
        }
    }

    ListView {
        id: pages

        property bool placing: false
        // Where the pages' middle is in the middle of the view, once the
        // widest is wider than it.
        readonly property real middle: (contentWidth - root.width - 12) / 2

        function show(page, location) {
            placing = true
            positionViewAtIndex(page, ListView.Beginning)
            contentX = middle + location.x * root.renderScale
            contentY += location.y * root.renderScale
            returnToBounds()
            placing = false
        }
        function showCurrentPage() { show(navigator.currentPage, Qt.point(0, 0)) }

        anchors.fill: parent
        anchors.leftMargin: 2
        spacing: 6
        // Wider than the view by a little, so that a page as wide as the view
        // has room around it.
        contentWidth: Math.max(root.width + 12, widest * root.renderScale + 12)
        flickableDirection: Flickable.AutoFlickDirection
        model: root.document ? root.document.pageCount : 0

        readonly property real widest: root.document
            ? (root.turned() ? root.document.maxPageHeight : root.document.maxPageWidth) : 0

        onContentYChanged: {
            if (placing)
                return
            const page = indexAt(contentX, contentY + height / 2)
            if (page >= 0)
                navigator.update(page, Qt.point((contentX - middle) / root.renderScale, 0), root.renderScale)
        }

        ScrollBar.vertical: ScrollBar {}
        ScrollBar.horizontal: ScrollBar {}

        delegate: Item {
            id: row

            required property int index
            // Read again when the document opens something else with as many pages.
            readonly property size points: root.document.status, root.document.pagePointSize(index)
            readonly property size shown: root.turnedSize(points)

            width: pages.contentWidth
            height: shown.height * root.renderScale

            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: row.shown.width * root.renderScale
                height: row.height
                color: "white"

                PdfPageImage {
                    anchors.centerIn: parent
                    width: row.points.width * root.renderScale
                    height: row.points.height * root.renderScale
                    sourceSize: Qt.size(width, height)
                    rotation: root.pageRotation
                    document: root.document
                    currentFrame: row.index
                    asynchronous: true
                    onStatusChanged: if (row.index === root.currentPage) root.currentPageRenderingStatus = status
                    Component.onCompleted: if (row.index === root.currentPage) root.currentPageRenderingStatus = status
                }
            }
        }
    }
}
