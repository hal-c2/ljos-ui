// Stands in for OSMGeometry (geometry.cpp): the shape of the buildings of
// one tile of the map, made of what OSMManager announced of them.
//
// In C++ the shape is worked out on another thread, and `geometryReady` is
// heard a turn of the event loop later. Here too it takes turns: building.js
// says how.
import QtQuick
import QtQuick3D
import "building.js" as Building

Geometry {
    id: geometry

    signal geometryReady()

    function updateData(geoVariantsList) {
        Building.build(geoVariantsList, (mesh) => {
            // The app keeps the list on a node for good and nothing reads it again.
            // In C++ it is compact; these objects would fill the JS heap's limit.
            if (Array.isArray(geoVariantsList))
                geoVariantsList.length = 0
            const float = 4
            geometry.clear()
            geometry.setIndexData(mesh.indexData)
            geometry.setVertexData(mesh.vertexData)
            geometry.setStride(mesh.strideVertex)
            geometry.setBounds(Qt.vector3d(mesh.min[0], mesh.min[1], mesh.min[2]), Qt.vector3d(mesh.max[0], mesh.max[1], mesh.max[2]))
            geometry.setPrimitiveType(Geometry.Triangles)
            geometry.addAttribute(Geometry.IndexSemantic, 0, Geometry.U32Type)
            geometry.addAttribute(Geometry.PositionSemantic, 0, Geometry.F32Type)
            geometry.addAttribute(Geometry.NormalSemantic, 3 * float, Geometry.F32Type)
            geometry.addAttribute(Geometry.TangentSemantic, 6 * float, Geometry.F32Type)
            geometry.addAttribute(Geometry.BinormalSemantic, 9 * float, Geometry.F32Type)
            geometry.addAttribute(Geometry.ColorSemantic, 12 * float, Geometry.F32Type)
            geometry.addAttribute(Geometry.TexCoord0Semantic, 16 * float, Geometry.F32Type)
            geometry.addAttribute(Geometry.TexCoord1Semantic, 18 * float, Geometry.F32Type)
            geometry.update()
            geometry.geometryReady()
        })
    }
}
