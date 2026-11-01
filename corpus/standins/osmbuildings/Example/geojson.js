// What request.cpp makes of the GeoJSON a tile of buildings is: a list with
// one map in it, of the `type` the document has and its `data`. For a
// FeatureCollection the data is a list of its features, each the `type`
// and `data` of its geometry with the feature's `properties` and `id`; for
// a Polygon it is the polygon, which in C++ is a QGeoPolygon and here an
// object with its `perimeter` and its `holes`, lists of coordinates.
//
// Only polygons are read. A document that is no object, or of another type,
// gives an empty list.
.pragma library

// QGeoCoordinate::isValid.
function isValid(coordinate) {
    return coordinate.latitude >= -90 && coordinate.latitude <= 90
            && coordinate.longitude >= -180 && coordinate.longitude <= 180
}

// A position is its longitude, its latitude and maybe its altitude.
function importPosition(position) {
    const coordinate = { latitude: NaN, longitude: NaN, altitude: NaN }
    if (Array.isArray(position) && position.length > 0) {
        coordinate.longitude = Number(position[0])
        if (position.length > 1) {
            coordinate.latitude = Number(position[1])
            if (position.length > 2)
                coordinate.altitude = Number(position[2])
        }
    }
    return coordinate
}

function importArrayOfPositions(positions) {
    const coordinates = []
    for (const position of Array.isArray(positions) ? positions : []) {
        const coordinate = importPosition(position)
        if (isValid(coordinate))
            coordinates.push(coordinate)
    }
    return coordinates
}

// The first ring is the perimeter, the others are holes in it.
function importPolygon(map) {
    const polygon = { perimeter: [], holes: [] }
    const rings = Array.isArray(map.coordinates) ? map.coordinates : []
    for (let i = 0; i < rings.length; ++i) {
        if (i === 0)
            polygon.perimeter = importArrayOfPositions(rings[i])
        else
            polygon.holes.push(importArrayOfPositions(rings[i]))
    }
    return polygon
}

const isMap = (value) => value !== null && typeof value === "object" && !Array.isArray(value)

function importGeometry(map) {
    if (isMap(map) && map.type === "Polygon")
        return { type: "Polygon", data: importPolygon(map) }
    return {}
}

function importFeature(map) {
    const feature = importGeometry(map.geometry)
    feature.properties = isMap(map.properties) ? map.properties : {}
    if ("id" in map)
        feature.id = map.id
    return feature
}

function importGeoJson(document) {
    if (!isMap(document))
        return []
    let parsed
    switch (document.type) {
    case "Polygon":
        parsed = { type: "Polygon", data: importPolygon(document) }
        break
    case "Feature":
        parsed = importFeature(document)
        break
    case "FeatureCollection":
        parsed = {
            type: "FeatureCollection",
            data: (Array.isArray(document.features) ? document.features : []).filter(isMap).map(importFeature)
        }
        break
    default:
        return []
    }
    if (document.bbox !== undefined)
        parsed.bbox = document.bbox
    return [parsed]
}
