# Sources:
#   https://doc.qt.io/qt-6/qquick3dgeometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-geometry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
# Ljós only: Qt's QML cannot create a Geometry or call these functions, an
# application's C++ subclass of QQuick3DGeometry does. Here QML can, under
# the names of that C++ API, so what QML stands in for such a subclass gives
# its data. There is nothing to compare with on Qt but the drawing.

@ljos-only
Feature: A geometry written in QML
  A Geometry can be declared in QML. Its functions are the ones Qt documents
  for QQuick3DGeometry: clear, setVertexData, setIndexData, setStride,
  setBounds, setPrimitiveType, addAttribute, and update, which commits what
  was set. Data is an ArrayBuffer or a typed array, little endian.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel. The triangle has its right angle at the view's centre and reaches
  # 50 units right and up.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 300 }
          Model {
            objectName: "model"
            geometry: Geometry {
              id: mesh; objectName: "mesh"
              function fill(reach) {
                clear()
                setVertexData(new Float32Array([0, 0, 0, reach, 0, 0, 0, reach, 0]))
                setIndexData(new Uint16Array([0, 1, 2]).buffer)
                setStride(12); setBounds(Qt.vector3d(0, 0, 0), Qt.vector3d(reach, reach, 0))
                setPrimitiveType(Geometry.Triangles)
                addAttribute(Geometry.PositionSemantic, 0, Geometry.F32Type)
                addAttribute(Geometry.IndexSemantic, 0, Geometry.U16Type)
                update() }
              function bad() { clear(); setVertexData(new Float32Array(3)); setStride(12); update() }
              Component.onCompleted: fill(50) }
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" } }
        } }
      """

  Rule: What QML commits is drawn

    Scenario: The triangle is where its vertices say
      Then the pixel at 110, 90 of the window is "#ff0000"
      And the pixel at 90, 110 of the window is "#202020"
      And the pixel at 170, 90 of the window is "#202020"

    Scenario: Calling the fill again draws the new data
      When the method "fill" of "mesh" is called with 80
      Then the pixel at 130, 60 of the window is "#ff0000"

  Rule: What cannot be drawn is reported

    Scenario: Vertices with no position draw nothing, with a warning
      When the method "bad" of "mesh" is called
      Then the log has a warning matching "no position attribute"
      And the pixel at 110, 90 of the window is "#202020"
