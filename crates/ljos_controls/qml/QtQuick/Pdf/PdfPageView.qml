import QtQuick
import QtQuick.Pdf

// One whole page at a time: the view is as large as the page.
Rectangle {
    id: root

    required property PdfDocument document
    property real renderScale: 1
    property bool zoomEnabled: true
    property string searchString
    readonly property PdfSearchModel searchModel: search
    readonly property string selectedText: selection.text
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

    function selectAll() { selection.selectAll() }
    function copySelectionToClipboard() { selection.copyToClipboard() }
    function searchBack() { search.show(search.currentResult - 1) }
    function searchForward() { search.show(search.currentResult + 1) }

    width: Math.round(points.width * renderScale)
    height: Math.round(points.height * renderScale)
    color: "white"

    onCurrentPageChanged: search.currentPage = currentPage

    PdfPageNavigator {
        id: navigator
        onJumped: function(current) {
            if (current.zoom > 0)
                root.renderScale = current.zoom
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

    PdfPageImage {
        id: image
        anchors.fill: parent
        sourceSize: Qt.size(root.width, root.height)
        document: root.document
        currentFrame: root.currentPage
        asynchronous: true

        // Each result on the page, and a line around the one the reader is on.
        Repeater {
            model: search.count, search.boundingPolygonsOnPage(root.currentPage)
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
            model: search.currentPage === root.currentPage ? search.currentResultBoundingPolygons : []
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
            page: root.currentPage
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
                selection.from = centroid.pressPosition
                selection.to = centroid.pressPosition
            }
            onCentroidChanged: if (active) selection.to = centroid.position
        }

        Repeater {
            model: PdfLinkModel {
                document: root.document
                page: root.currentPage
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

    WheelHandler {
        enabled: root.zoomEnabled
        acceptedModifiers: Qt.ControlModifier
        onWheel: function(event) {
            root.renderScale *= event.angleDelta.y > 0 ? 1.25 : 0.8
        }
    }
}
