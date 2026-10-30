import QtQuick
import QtQuick3D
import QtQuick3D.Helpers

// The three axes as coloured bars, and grids in the planes between them:
// a scale to read a scene against. X is red, Y green and Z blue.
Node {
    id: root

    property bool enableAxisLines: true
    property bool enableXYGrid: false
    property bool enableXZGrid: true
    property bool enableYZGrid: false
    property color gridColor: Qt.rgba(0.8, 0.8, 0.8, 1)
    property real gridOpacity: 0.5

    // 21 lines ten units apart reach 100 units from the origin each way.
    // The grid is made in the XY plane; the others turn it.
    Model {
        visible: root.enableXYGrid
        opacity: root.gridOpacity
        geometry: GridGeometry {
            horizontalLines: 21; horizontalStep: 10
            verticalLines: 21; verticalStep: 10
        }
        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            diffuseColor: root.gridColor
            cullMode: DefaultMaterial.NoCulling
        }
    }
    Model {
        visible: root.enableXZGrid
        opacity: root.gridOpacity
        eulerRotation.x: 90
        geometry: GridGeometry {
            horizontalLines: 21; horizontalStep: 10
            verticalLines: 21; verticalStep: 10
        }
        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            diffuseColor: root.gridColor
            cullMode: DefaultMaterial.NoCulling
        }
    }
    Model {
        visible: root.enableYZGrid
        opacity: root.gridOpacity
        eulerRotation.y: 90
        geometry: GridGeometry {
            horizontalLines: 21; horizontalStep: 10
            verticalLines: 21; verticalStep: 10
        }
        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            diffuseColor: root.gridColor
            cullMode: DefaultMaterial.NoCulling
        }
    }

    Model {
        visible: root.enableAxisLines
        geometry: CuboidGeometry { xExtent: 200; yExtent: 0.5; zExtent: 0.5 }
        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            diffuseColor: "red"
        }
    }
    Model {
        visible: root.enableAxisLines
        geometry: CuboidGeometry { xExtent: 0.5; yExtent: 200; zExtent: 0.5 }
        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            diffuseColor: "green"
        }
    }
    Model {
        visible: root.enableAxisLines
        geometry: CuboidGeometry { xExtent: 0.5; yExtent: 0.5; zExtent: 200 }
        materials: DefaultMaterial {
            lighting: DefaultMaterial.NoLighting
            diffuseColor: "blue"
        }
    }
}
