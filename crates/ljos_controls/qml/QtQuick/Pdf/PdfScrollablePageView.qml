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
    readonly property PdfSearchModel searchModel: search
    readonly property string selectedText: selection.text
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

    function selectAll() { selection.selectAll() }
    function copySelectionToClipboard() { selection.copyToClipboard() }
    function searchBack() { search.show(search.currentResult - 1) }
    function searchForward() { search.show(search.currentResult + 1) }

    clip: true
    contentWidth: shown.width * renderScale
    contentHeight: shown.height * renderScale

    ScrollBar.vertical: ScrollBar {}
    ScrollBar.horizontal: ScrollBar {}

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
            root.contentX = Math.max(0, Math.min(current.location.x * root.renderScale, root.contentWidth - root.width))
            root.contentY = Math.max(0, Math.min(current.location.y * root.renderScale, root.contentHeight - root.height))
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
    }
}
