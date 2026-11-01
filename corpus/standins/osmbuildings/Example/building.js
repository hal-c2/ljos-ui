// What geometry.cpp makes of the buildings of a tile: one mesh with every
// building in it, each the walls its perimeter is raised to, the roof that
// closes them and, for one whose shape is a sphere, the sphere on top.
//
// A corner is 20 numbers: where it is (3), which way it faces (3), its
// tangent (3) and binormal (3), its colour (4), where in a picture it is (2)
// and, as a second such pair, how many levels the building has and whether
// the corner is of the roof.
.pragma library

const strideVertexLen = 20

// A place on the map in the scene's units, the scene's middle being the
// tile the camera starts at.
function convertGeoCoordToVertexPosition(lat, lon) {
    const scale = 1.212
    const geoToPositionScale = 1000000 * scale
    const XOffsetFromCenter = 537277 * scale
    const YOffsetFromCenter = 327957 * scale
    const radians = lat * Math.PI / 180
    const x = (lon / 360.0 + 0.5) * geoToPositionScale
    const y = (1.0 - Math.log(Math.tan(radians) + 1.0 / Math.cos(radians)) / Math.PI) * 0.5 * geoToPositionScale
    return [Math.fround(x - XOffsetFromCenter), Math.fround(YOffsetFromCenter - y), 0.0]
}

// What QVariant makes of a property that is asked for as a whole number.
function toLongLong(value) {
    const number = Number(value)
    return Number.isFinite(number) ? Math.sign(number) * Math.round(Math.abs(number)) : 0
}

// QColor::fromString, and null where the name is none.
function toColor(name) {
    try {
        return Qt.color(String(name ?? ""))
    } catch (error) {
        return null
    }
}

const isBlack = (color) => color.r === 0 && color.g === 0 && color.b === 0 && color.a === 1

function normalized(vector) {
    const length = Math.hypot(vector[0], vector[1], vector[2])
    return length > 0 ? [vector[0] / length, vector[1] / length, vector[2] / length] : [0, 0, 0]
}

function crossProduct(a, b) {
    return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

// The triangles that fill a polygon with no holes, as the corners of each
// counted from the polygon's first, counter-clockwise. The C++ has mapbox's
// earcut for it; this cuts ears the plain way: a corner that turns the way
// the polygon does, with no other corner inside the triangle it makes with
// its neighbours, is cut off, until three are left. A polygon that crosses
// itself has no such corner at some point, and is cut where it stands.
function triangulate(points) {
    const count = points.length
    const ring = []
    for (let i = 0; i < count; ++i) {
        const next = points[(i + 1) % count]
        if (points[i][0] !== next[0] || points[i][1] !== next[1])
            ring.push(i)
    }
    let area = 0
    for (let i = 0; i < ring.length; ++i) {
        const a = points[ring[i]]
        const b = points[ring[(i + 1) % ring.length]]
        area += a[0] * b[1] - b[0] * a[1]
    }
    if (area < 0)
        ring.reverse()

    const turn = (a, b, c) => (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    const same = (a, b) => a[0] === b[0] && a[1] === b[1]
    const indices = []
    let at = 0
    let passed = 0
    while (ring.length > 3) {
        const length = ring.length
        const before = ring[(at + length - 1) % length]
        const here = ring[at % length]
        const after = ring[(at + 1) % length]
        const a = points[before]
        const b = points[here]
        const c = points[after]
        const turned = turn(a, b, c)
        let ear = turned > 0
        for (let i = 0; ear && i < length; ++i) {
            const p = points[ring[i]]
            if (same(p, a) || same(p, b) || same(p, c))
                continue
            ear = !(turn(a, b, p) >= 0 && turn(b, c, p) >= 0 && turn(c, a, p) >= 0)
        }
        if (ear || turned === 0 || passed > length) {
            // A corner on the line between its neighbours takes nothing with it.
            if (turned !== 0)
                indices.push(before, here, after)
            ring.splice(at % length, 1)
            passed = 0
        } else {
            ++at
            ++passed
        }
        at %= ring.length
    }
    if (ring.length === 3)
        indices.push(ring[0], ring[1], ring[2])
    return indices
}

// In C++ a mesh is worked out on another thread. Here it is worked out on the
// UI's, a little at a time: this many milliseconds to a turn, and a tile with
// thousands of buildings neither holds up a frame nor outlasts what a script
// is given.
const millisecondsToATurn = 3
const jobs = []
let heardWhenOneIsDone = null

function pending() {
    return jobs.length
}

function whenOneIsDone(heard) {
    heardWhenOneIsDone = heard
}

// Hears `done` with the mesh of `geoVariantsList` some turns later.
function build(geoVariantsList, done) {
    jobs.push({ steps: mesh(geoVariantsList), done: done })
    Qt.callLater(work)
}

function work() {
    const end = Date.now() + millisecondsToATurn
    while (jobs.length > 0 && Date.now() < end) {
        const step = jobs[0].steps.next()
        if (step.done) {
            jobs.shift().done(step.value)
            if (heardWhenOneIsDone)
                heardWhenOneIsDone()
        }
    }
    if (jobs.length > 0)
        Qt.callLater(work)
}

// The mesh of the buildings in what `GeoJson.importGeoJson` gave: its
// corners, the order they are joined in as triangles, and the box it is in.
// Stops after each building.
function* mesh(geoVariantsList) {
    const vertices = []
    const indices = []
    const maxFloat = 3.4028234663852886e+38
    const minFloat = 1.1754943508222875e-38
    const meshMinBound = [maxFloat, maxFloat, maxFloat]
    const meshMaxBound = [minFloat, minFloat, minFloat]
    let globalVertexCounter = 0

    const writeVertex = (pos, normal, tangent, binormal, color, alpha, texCoordX, texCoordY, levels, isRoofTop) => {
        vertices.push(pos[0], pos[1], pos[2], normal[0], normal[1], normal[2],
                      tangent[0], tangent[1], tangent[2], binormal[0], binormal[1], binormal[2],
                      color.r, color.g, color.b, alpha, texCoordX, texCoordY, levels, isRoofTop)
    }

    for (const baseData of Array.isArray(geoVariantsList) ? geoVariantsList : []) {
        for (const featureMap of Array.isArray(baseData?.data) ? baseData.data : []) {
            const properties = featureMap.properties ?? {}
            const buildingCoords = featureMap.data?.perimeter ?? []
            const height = 0.15 * toLongLong(properties.height)
            const levels = toLongLong(properties.levels)

            let color = toColor(properties.color)
            if (!color || isBlack(color))
                color = Qt.color("white")
            let roofColor = toColor(properties.roofColor)
            if (!roofColor || isBlack(roofColor))
                roofColor = color

            // The walls: two corners to a point of the perimeter, one on the
            // ground and one raised, and two triangles between a point and
            // the next. A wall faces away from the building's inside.
            const numSubsetVertices = buildingCoords.length * 2
            let subsetVertexCounter = 0
            let lastBaseVertexPos = [0, 0, 0]
            let lastExtrudedVertexPos = [0, 0, 0]
            let currentBaseVertexPos = [0, 0, 0]
            let currentExtrudedVertexPos = [0, 0, 0]
            const subsetPolygonCenter = [0, 0, 0]
            const roofPolygonVertices = []

            for (const buildingPoint of buildingCoords) {
                lastBaseVertexPos = currentBaseVertexPos
                lastExtrudedVertexPos = currentExtrudedVertexPos
                currentBaseVertexPos = convertGeoCoordToVertexPosition(buildingPoint.latitude, buildingPoint.longitude)
                currentExtrudedVertexPos = [currentBaseVertexPos[0], currentBaseVertexPos[1], height]
                roofPolygonVertices.push([currentBaseVertexPos[0], currentBaseVertexPos[1]])

                subsetPolygonCenter[0] += currentBaseVertexPos[0]
                subsetPolygonCenter[1] += currentBaseVertexPos[1]
                for (let axis = 0; axis < 3; ++axis) {
                    meshMinBound[axis] = Math.min(meshMinBound[axis], currentBaseVertexPos[axis])
                    meshMaxBound[axis] = Math.max(meshMaxBound[axis], currentExtrudedVertexPos[axis])
                }

                if (subsetVertexCounter < numSubsetVertices - 2) {
                    indices.push(globalVertexCounter + 3, globalVertexCounter + 2, globalVertexCounter + 0)
                    indices.push(globalVertexCounter + 1, globalVertexCounter + 3, globalVertexCounter + 0)
                }

                if (subsetVertexCounter >= 2) {
                    const tangent = normalized([0, 0, currentExtrudedVertexPos[2] - currentBaseVertexPos[2]])
                    const binormal = normalized([lastBaseVertexPos[0] - currentBaseVertexPos[0],
                                                 lastBaseVertexPos[1] - currentBaseVertexPos[1], 0])
                    const normal = normalized(crossProduct(binormal, tangent))
                    if (subsetVertexCounter === 2) {
                        writeVertex(lastBaseVertexPos, normal, tangent, binormal, color, 1.0, 0.0, 0.0, levels, 0.0)
                        writeVertex(lastExtrudedVertexPos, normal, tangent, binormal, color, 1.0, 0.0, 1.0, levels, 0.0)
                    }
                    const xCoord = subsetVertexCounter % 4 !== 0 ? 1.0 : 0.0
                    writeVertex(currentBaseVertexPos, normal, tangent, binormal, color, 1.0, xCoord, 0.0, levels, 0.0)
                    writeVertex(currentExtrudedVertexPos, normal, tangent, binormal, color, 1.0, xCoord, 1.0, levels, 0.0)
                }
                subsetVertexCounter += 2
                globalVertexCounter += 2
            }
            // A perimeter of one point has its two corners all the same,
            // with nothing said of them.
            while (vertices.length < globalVertexCounter * strideVertexLen)
                vertices.push(0)

            if (properties.shape === "sphere" && roofPolygonVertices.length > 0) {
                subsetPolygonCenter[0] /= roofPolygonVertices.length
                subsetPolygonCenter[1] /= roofPolygonVertices.length
                subsetPolygonCenter[2] = height

                const sphereRadius = Math.max(2.0 * Math.abs(roofPolygonVertices[0][0] - subsetPolygonCenter[0]), 1.0)
                const sphereRadiuslengthInv = 1.0 / sphereRadius
                const sphereSectorCount = 10
                const sphereStackCount = 10
                const sphereSectorStep = 2.0 * Math.PI / sphereSectorCount
                const sphereStackStep = Math.PI / sphereStackCount
                const sphereVertexCount = (sphereStackCount + 1) * (sphereSectorCount + 1)

                for (let stackIndex = 0; stackIndex <= sphereStackCount; ++stackIndex) {
                    let k1 = stackIndex * (sphereSectorCount + 1)
                    let k2 = k1 + sphereSectorCount + 1
                    const sphereStackAngle = Math.PI / 2.0 - stackIndex * sphereStackStep
                    const xy = sphereRadius * Math.cos(sphereStackAngle)
                    const z = sphereRadius * Math.sin(sphereStackAngle)
                    for (let sectorIndex = 0; sectorIndex <= sphereSectorCount; ++sectorIndex, ++k1, ++k2) {
                        if (stackIndex !== sphereStackCount) {
                            // One triangle only in the first stack and the last.
                            if (stackIndex !== 0)
                                indices.push(k1 + globalVertexCounter, k2 + globalVertexCounter, k1 + 1 + globalVertexCounter)
                            if (stackIndex !== sphereStackCount - 1)
                                indices.push(k1 + 1 + globalVertexCounter, k2 + globalVertexCounter, k2 + 1 + globalVertexCounter)
                        }
                        const sphereSectorAngle = sectorIndex * sphereSectorStep
                        const x = xy * Math.cos(sphereSectorAngle)
                        const y = xy * Math.sin(sphereSectorAngle)
                        writeVertex([x + subsetPolygonCenter[0], y + subsetPolygonCenter[1], z + subsetPolygonCenter[2]],
                                    [x * sphereRadiuslengthInv, y * sphereRadiuslengthInv, z * sphereRadiuslengthInv],
                                    [0, 0, 0], [0, 0, 0], roofColor, 1.0, 1.0, 1.0, 0.0, 1.0)
                    }
                }
                globalVertexCounter += sphereVertexCount
            }

            // The roof: the perimeter again, raised, and filled.
            for (const roofIndex of triangulate(roofPolygonVertices))
                indices.push(roofIndex + globalVertexCounter)
            for (const polygonVertex of roofPolygonVertices) {
                writeVertex([polygonVertex[0], polygonVertex[1], height], [0, 0, 1], [1, 0, 0], [0, 1, 0],
                            roofColor, 1.0, 1.0, 1.0, 0.0, 1.0)
                ++globalVertexCounter
            }
            yield
        }
    }

    return {
        vertexData: new Float32Array(vertices),
        indexData: new Uint32Array(indices),
        strideVertex: strideVertexLen * 4,
        min: meshMinBound,
        max: meshMaxBound
    }
}
