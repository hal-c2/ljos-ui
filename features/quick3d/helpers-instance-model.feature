# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-instancemodel.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-instancerepeater.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@ljos-only
Feature: A model of an instance table, and a repeater over it
  An InstanceModel has a row for each copy its instancingTable holds, with
  the roles modelPosition, modelRotation, modelScale, modelColor and
  modelData. An InstanceRepeater makes a delegate for each copy and places
  it where the copy is, turned and scaled as the copy is: it can stand in
  for the copies where they must be picked one by one.

  Rule: A delegate of the model sees its copy's roles

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        View3D {
          id: scene; objectName: "scene"; width: 200; height: 200
          property real secondX: -1
          property real secondRed: -1
          property real secondData: -1
          InstanceListEntry { id: a; position: Qt.vector3d(-50, 0, 0) }
          InstanceListEntry { id: b; position: Qt.vector3d(50, 0, 0); color: "red"; customData: Qt.vector4d(7, 0, 0, 0) }
          InstanceList { id: table; instances: [a, b] }
          Repeater3D {
            objectName: "rows"
            model: InstanceModel { instancingTable: table }
            delegate: Node {
              Component.onCompleted: if (index === 1) {
                scene.secondX = modelPosition.x
                scene.secondRed = modelColor.r
                scene.secondData = modelData.x
              }
            }
          }
        }
        """

    Scenario: The model has a row for each copy
      Then the property "count" of "rows" is 2

    Scenario: A delegate sees its copy's roles
      Then the property "secondX" of "scene" is 50
      And the property "secondRed" of "scene" is 1
      And the property "secondData" of "scene" is 7

  Rule: The repeater places a delegate where each copy is

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          id: scene; objectName: "scene"; width: 200; height: 200; color: "black"
          property real lift: 0; property bool three: false
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 300 }
            InstanceListEntry { id: a; position: Qt.vector3d(-50, 0, 0); scale: Qt.vector3d(0.2, 0.2, 0.2) }
            InstanceListEntry { id: b; position: Qt.vector3d(50, scene.lift, 0); scale: Qt.vector3d(0.2, 0.2, 0.2) }
            InstanceListEntry { id: c; position: Qt.vector3d(0, 60, 0); scale: Qt.vector3d(0.2, 0.2, 0.2) }
            InstanceList { id: table; instances: scene.three ? [a, b, c] : [a, b] }
            Repeater3D {
              objectName: "rows"
              model: InstanceModel { instancingTable: table }
              delegate: Node {}
            }
            InstanceRepeater {
              objectName: "repeater"; instancingTable: table
              Model { source: "#Cube"; materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" } }
            }
          }
        }
        """

    Scenario: The repeater puts a delegate where each copy is
      Then the property "count" of "repeater" is 2
      And the pixel at 50, 100 of the window is "#ff0000"
      And the pixel at 150, 100 of the window is "#ff0000"
      And the pixel at 100, 100 of the window is "#202020"
      And the pixel at 70, 100 of the window is "#202020"

    Scenario: A copy that moves takes its delegate along
      When the property "lift" of "scene" is set to 50
      Then the pixel at 150, 50 of the window is "#ff0000"
      And the pixel at 150, 100 of the window is "#202020"

    Scenario: A copy added to the table gets a row and a delegate
      When the property "three" of "scene" is set to true
      Then the property "count" of "rows" is 3
      And the property "count" of "repeater" is 3
      And the pixel at 100, 40 of the window is "#ff0000"
