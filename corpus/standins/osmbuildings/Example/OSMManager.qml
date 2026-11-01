// Stands in for OSMManager (manager.cpp) and the OSMRequest it asks through
// (request.cpp): it works out which tiles of the map are around what the
// camera looks at, asks one service for the buildings of each and another
// for its picture, and announces each as it arrives.
//
// As in C++ the tiles are 41 by 41 around the camera, those nearest first,
// each asked for once its buildings are not here yet, and every move of the
// camera replaces what was still to ask for. Six requests of a kind are out
// at a time, as in a release build.
//
// Not here: the `data` directory beside the program in which the C++ looks
// for a tile before it asks the network, which nothing fills.
import QtQuick
import "geojson.js" as GeoJson
import "building.js" as Building

QtObject {
    id: manager

    readonly property int tileSizeX: 37
    readonly property int tileSizeY: 37

    // The buildings are a list as geojson.js makes it, the picture is the
    // bytes of its file. A tile is counted from the one the camera starts at.
    signal buildingsDataReady(var geoVariantsList, int tileX, int tileY, int zoomLevel)
    signal mapsDataReady(var mapData, int tileX, int tileY, int zoomLevel)

    readonly property int startBuildingTileX: 17605
    readonly property int startBuildingTileY: 10746

    // %1 is the zoom level (15 is the only one that seems to work), %2 and %3
    // the tile.
    readonly property string buildingsUrl: "https://983wdxn2c2.execute-api.eu-north-1.amazonaws.com/production/osmbuildingstile?z=%1&x=%2&y=%3&token=%4"
    readonly property string mapUrl: "https://tile-a.openstreetmap.fr/hot/%1/%2/%3.png"
    readonly property int concurrentRequests: 6

    property string heldToken: ""
    property bool stopped: false
    // The tiles whose buildings are here, by their key.
    property var buildingsHash: ({})
    property var buildingsQueue: []
    property var mapsQueue: []
    property int buildingsInFlight: 0
    property int mapsInFlight: 0
    property string lastBuildingsMessage: ""
    property string lastMapsMessage: ""

    Component.onCompleted: Building.whenOneIsDone(advance)

    function isDemoToken() {
        return heldToken === ""
    }

    function setToken(token) {
        heldToken = token
    }

    function token() {
        return heldToken
    }

    function stop() {
        stopped = true
    }

    function tileKey(tile) {
        return tile.ZoomLevel + "," + tile.TileX + "," + tile.TileY
    }

    function setCameraProperties(position, right, cameraZoom, minimumZoom, maximumZoom,
                                 cameraTilt, minimumTilt, maximumTilt) {
        const tiltFactor = (cameraTilt - minimumTilt) / Math.max(maximumTilt - minimumTilt, 1.0)
        const zoomFactor = (cameraZoom - minimumZoom) / Math.max(maximumZoom - minimumZoom, 1.0)

        // The forward vector aligned to the XY plane: right x (0, 0, -1).
        const length = Math.hypot(right.x, right.y)
        const forwardX = length > 0 ? -right.y / length : 0
        const forwardY = length > 0 ? right.x / length : 0
        const reach = tiltFactor * zoomFactor * 50.0
        const projectedX = position.x + forwardX * reach
        const projectedY = position.y + forwardY * reach

        const queue = []
        for (let forwardIndex = -20; forwardIndex <= 20; ++forwardIndex) {
            for (let sidewardIndex = -20; sidewardIndex <= 20; ++sidewardIndex) {
                const tile = {
                    TileX: startBuildingTileX + Math.trunc((projectedX + tileSizeX * sidewardIndex) / tileSizeX),
                    TileY: startBuildingTileY - Math.trunc((projectedY + tileSizeY * forwardIndex) / tileSizeY),
                    ZoomLevel: 15
                }
                if (!buildingsHash[tileKey(tile)])
                    queue.push(tile)
            }
        }

        const projectedTileX = startBuildingTileX + Math.trunc(projectedX / tileSizeX)
        const projectedTileY = startBuildingTileY - Math.trunc(projectedY / tileSizeY)
        const distance = (tile) => Math.hypot(projectedTileX - tile.TileX, projectedTileY - tile.TileY)
        queue.sort((a, b) => distance(a) - distance(b))

        if (queue.length === 0)
            return
        buildingsQueue = queue
        mapsQueue = queue.slice()
        advance()
    }

    // Run when there is more to ask for, when a request is answered and when
    // a mesh is done, so that nothing runs while waiting for those and nothing
    // once all are in. Tiles waiting for their mesh count as requests: faster
    // answers than meshes would otherwise fill the JS heap.
    function advance() {
        while (!stopped && buildingsQueue.length > 0 && buildingsInFlight + Building.pending() < concurrentRequests) {
            ++buildingsInFlight
            getBuildingsDataRequest(buildingsQueue.shift())
        }
        while (!stopped && mapsQueue.length > 0 && mapsInFlight < concurrentRequests) {
            ++mapsInFlight
            getMapsDataRequest(mapsQueue.shift())
        }
    }

    function getBuildingsDataRequest(tile) {
        const url = buildingsUrl.arg(tile.ZoomLevel).arg(tile.TileX).arg(tile.TileY).arg(heldToken)
        const request = new XMLHttpRequest()
        request.onreadystatechange = () => {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            if (request.status >= 200 && request.status < 300) {
                // What is no JSON is a document with nothing in it.
                let document = null
                try {
                    document = JSON.parse(request.responseText)
                } catch (error) {
                }
                if (!stopped) {
                    buildingsHash[tileKey(tile)] = true
                    buildingsDataReady(GeoJson.importGeoJson(document), tile.TileX - startBuildingTileX,
                                       tile.TileY - startBuildingTileY, tile.ZoomLevel)
                }
            } else {
                const message = request.responseText
                if (message !== lastBuildingsMessage) {
                    lastBuildingsMessage = message
                    console.warn("OSMRequest::getBuildingsData ", request.status, url, message)
                }
            }
            --buildingsInFlight
            advance()
        }
        request.open("GET", url)
        request.send()
    }

    function getMapsDataRequest(tile) {
        const url = mapUrl.arg(tile.ZoomLevel).arg(tile.TileX).arg(tile.TileY)
        const request = new XMLHttpRequest()
        request.responseType = "arraybuffer"
        request.onreadystatechange = () => {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            if (request.status >= 200 && request.status < 300) {
                if (!stopped)
                    mapsDataReady(request.response, tile.TileX - startBuildingTileX,
                                  tile.TileY - startBuildingTileY, tile.ZoomLevel)
            } else {
                const bytes = request.response ? new Uint8Array(request.response) : []
                let message = ""
                for (const byte of bytes)
                    message += String.fromCharCode(byte)
                if (message !== lastMapsMessage) {
                    lastMapsMessage = message
                    console.warn("OSMRequest::getMapsDataRequest", request.status, url, message)
                }
            }
            --mapsInFlight
            advance()
        }
        request.open("GET", url)
        request.send()
    }
}
