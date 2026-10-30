# Sources:
#   https://doc.qt.io/qt-6/quick3d-instancing.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-instancing.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-instancelist.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-instancelistentry.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-model.html
#   https://doc.qt.io/qt-6/qml-pickresult.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Many copies of a model from an instance table
  A Model whose instancing is set is not drawn by itself: it is drawn once
  for each entry of the instance table, each copy moved, turned and scaled
  by its entry and its colour blended with the material's. The model's own
  scale and rotation shape each copy; its position and its parents carry
  the whole group. An InstanceList is a table written in QML, one
  InstanceListEntry a copy.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. The square is 100
  # units across, scaled to 20; its copies sit 50 units left and right.
  # Whether colours blend in linear light or in sRGB is not documented, so
  # a half-transparent colour over another takes either answer, as in
  # transparency.feature.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        objectName: "root"; width: 200; height: 200; color: "black"
        property var result
        function pickAt(x, y) { result = view.pick(x, y) }
        View3D {
          id: view; anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          Model {
            id: square; objectName: "square"; source: "#Rectangle"; pickable: true
            scale: Qt.vector3d(0.2, 0.2, 0.2)
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" }
            instancing: InstanceList {
              id: table; objectName: "table"
              InstanceListEntry { objectName: "left"; position: Qt.vector3d(-50, 0, 0); color: "red" }
              InstanceListEntry { objectName: "right"; position: Qt.vector3d(50, 0, 0); color: "blue" }
            }
          }
        }
      }
      """

  Rule: Each entry is one copy, where it says and in its colour

    Scenario: Two entries draw two copies and not the model itself
      Then the pixel at 50, 100 of the window is within 2 of "#ff0000"
      And the pixel at 150, 100 of the window is within 2 of "#0000ff"
      And the pixel at 100, 100 of the window is "#202020"
      And the property "instanceCount" of "table" is 2

    Scenario: Moving an entry moves its copy
      When the property "position" of "left" is set to Qt.vector3d(-50, 50, 0)
      Then the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 50, 100 of the window is "#202020"

    Scenario: An entry's scale and rotation shape its copy
      When the property "scale" of "left" is set to Qt.vector3d(2, 1, 1)
      Then the pixel at 33, 100 of the window is within 2 of "#ff0000"
      And the pixel at 50, 85 of the window is "#202020"

    Scenario: Without a table the model is drawn by itself again
      When the property "instancing" of "square" is set to null
      Then the pixel at 100, 100 of the window is within 2 of "#ffffff"
      And the pixel at 50, 100 of the window is "#202020"

  Rule: The model's own transform carries the group and shapes each copy

    Scenario: Moving the model moves every copy
      When the property "y" of "square" is set to 40
      Then the pixel at 50, 60 of the window is within 2 of "#ff0000"
      And the pixel at 150, 60 of the window is within 2 of "#0000ff"

    Scenario: The model's scale sizes each copy, not the spaces between
      When the property "scale" of "square" is set to Qt.vector3d(0.4, 0.2, 0.2)
      Then the pixel at 50, 100 of the window is within 2 of "#ff0000"
      And the pixel at 32, 100 of the window is within 2 of "#ff0000"
      And the pixel at 150, 100 of the window is within 2 of "#0000ff"

  Rule: An instance count override draws fewer copies

    Scenario: Only the first copies are drawn
      When the property "instanceCountOverride" of "table" is set to 1
      Then the pixel at 50, 100 of the window is within 2 of "#ff0000"
      And the pixel at 150, 100 of the window is "#202020"

  Rule: A colour's alpha counts only where the table says it has transparency

    Scenario: An opaque model ignores the table's alpha by default
      When the property "color" of "left" is set to "#80ff0000"
      Then the pixel at 50, 100 of the window is within 2 of "#ff0000"

    Scenario: With transparency the copy is blended over what is behind it
      When the property "color" of "left" is set to "#80ff0000"
      And the property "hasTransparency" of "table" is set to true
      Then the pixel at 50, 100 of the window is within 30 of "#a61313"

  Rule: A pick names the model and the copy it hit

    Scenario: Picking a copy gives its index in the table
      When the method "pickAt" of "root" is called with 150, 100
      Then the property "result.objectHit.objectName" of "root" is "square"
      And the property "result.instanceIndex" of "root" is 1
      And the property "result.scenePosition.x" of "root" is within 0.01 of 50

    Scenario: Between the copies nothing is picked
      When the method "pickAt" of "root" is called with 100, 100
      Then the property "result.hitType" of "root" is 0

    Scenario: A plain model's pick has no instance index
      When the property "instancing" of "square" is set to null
      And the method "pickAt" of "root" is called with 100, 100
      Then the property "result.objectHit.objectName" of "root" is "square"
      And the property "result.instanceIndex" of "root" is -1
