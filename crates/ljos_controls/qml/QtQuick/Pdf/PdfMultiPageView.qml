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
    readonly property PdfSearchModel searchModel: search
    readonly property string selectedText: chosen.selection ? chosen.selection.text : ""
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

    function selectAll() {
        const row = pages.itemAtIndex(currentPage)
        if (row) {
            chosen.take(row.selection)
            row.selection.selectAll()
        }
    }
    function copySelectionToClipboard() {
        if (chosen.selection)
            chosen.selection.copyToClipboard()
    }
    function searchBack() { search.show(search.currentResult - 1) }
    function searchForward() { search.show(search.currentResult + 1) }

    function turned() { return (Math.abs(pageRotation) % 180) === 90 }
    function turnedSize(points) {
        return turned() ? Qt.size(points.height, points.width) : points
    }

    onRenderScaleChanged: pages.showCurrentPage()

    onCurrentPageChanged: search.currentPage = currentPage

    // A document that loads is shown at its own size, whatever scale the
    // view was given before, as qml6 does (features/pdf/pdf-views.feature).
    Connections {
        target: root.document
        function onStatusChanged() {
            if (root.document.status === PdfDocument.Ready)
                root.resetScale()
        }
    }

    PdfPageNavigator {
        id: navigator
        onJumped: function(current) {
            if (current.zoom > 0)
                root.renderScale = current.zoom
            pages.show(current.page, current.location)
        }
    }

    PdfSearchModel {
        id: search
        document: root.document
        searchString: root.searchString

        // The search wraps past either end and goes to the result's page;
        // the view keeps its scale.
        function show(index) {
            currentResult = index
            const found = currentResultLink
            if (found.valid)
                root.goToLocation(found.page, found.location, root.renderScale)
        }
    }

    PdfStyle { id: style }

    // Text is selected on one page at a time: selecting on another lets go
    // of what was selected before.
    QtObject {
        id: chosen
        property PdfSelection selection: null
        function take(next) {
            if (selection && selection !== next)
                selection.clear()
            selection = next
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
            // Kept in bounds at once: a rebound would scroll after this,
            // and the page the scroll ends on would be taken for the one
            // gone to.
            const within = (at, origin, size, view) => Math.max(origin, Math.min(at, origin + size - view))
            contentX = within(middle + location.x * root.renderScale, originX, contentWidth, width)
            contentY = within(contentY + location.y * root.renderScale, originY, contentHeight, height)
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
            readonly property alias selection: selection
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

                    // Each result on the page, and a line around the one the reader is on.
                    Repeater {
                        model: search.count, search.boundingPolygonsOnPage(row.index)
                        delegate: Rectangle {
                            required property var modelData
                            x: modelData[0].x * root.renderScale
                            y: modelData[0].y * root.renderScale
                            width: (modelData[2].x - modelData[0].x) * root.renderScale
                            height: (modelData[2].y - modelData[0].y) * root.renderScale
                            color: style.pageSearchResultsColor
                        }
                    }
                    Repeater {
                        model: search.currentPage === row.index ? search.currentResultBoundingPolygons : []
                        delegate: Rectangle {
                            required property var modelData
                            x: modelData[0].x * root.renderScale
                            y: modelData[0].y * root.renderScale
                            width: (modelData[2].x - modelData[0].x) * root.renderScale
                            height: (modelData[2].y - modelData[0].y) * root.renderScale
                            color: "transparent"
                            border.color: style.currentSearchResultStrokeColor
                            border.width: style.currentSearchResultStrokeWidth
                        }
                    }

                    PdfSelection {
                        id: selection
                        anchors.fill: parent
                        document: root.document
                        page: row.index
                        renderScale: root.renderScale

                        Repeater {
                            model: selection.geometry
                            delegate: Rectangle {
                                required property var modelData
                                x: modelData[0].x
                                y: modelData[0].y
                                width: modelData[2].x - modelData[0].x
                                height: modelData[2].y - modelData[0].y
                                color: style.selectionColor
                            }
                        }
                    }

                    // A mouse selects text by dragging over it; a finger scrolls.
                    DragHandler {
                        target: null
                        acceptedDevices: PointerDevice.Mouse | PointerDevice.Stylus
                        onActiveChanged: if (active) {
                            chosen.take(selection)
                            selection.from = centroid.pressPosition
                            selection.to = centroid.pressPosition
                        }
                        onCentroidChanged: if (active) selection.to = centroid.position
                    }

                    Repeater {
                        model: PdfLinkModel {
                            document: root.document
                            page: row.index
                        }
                        delegate: PdfLinkDelegate {
                            x: rectangle.x * root.renderScale
                            y: rectangle.y * root.renderScale
                            width: rectangle.width * root.renderScale
                            height: rectangle.height * root.renderScale
                            onTapped: function(link) {
                                if (link.valid)
                                    root.goToLocation(link.page, link.location, root.renderScale)
                                else
                                    Qt.openUrlExternally(link.url)
                            }
                        }
                    }
                }
            }
        }
    }
}
