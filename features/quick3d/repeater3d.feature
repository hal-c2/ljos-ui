# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-repeater3d.html
#   https://doc.qt.io/qt-6/qml-qtquick-repeater.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Repeater3D
  A Repeater3D makes one node from its delegate for each row of its model:
  a number, an array or a ListModel. It says how many it made (count),
  gives the one made for a row (objectAt), and tells when one is added or
  removed. It is a node itself: the nodes it makes move with it.

  # An orthographic camera draws one unit a pixel. Each row makes a red
  # cube 20 units across, 30 units right of the one before, the first at
  # x -30: drawn about pixels 20, 50 and 80 across the middle of the view.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        id: view; objectName: "view"; width: 100; height: 100
        property string log: ""
        property string picked: ""
        property bool listed: false; property int rowCount: 3
        function pick(i) { const o = cubes.objectAt(i); picked = o ? o.objectName : "none" }
        environment: SceneEnvironment { clearColor: "black"; backgroundMode: SceneEnvironment.Color }
        OrthographicCamera { z: 300 }
        ListModel { id: rows; objectName: "rows"; ListElement { n: 0 } ListElement { n: 1 } }
        Repeater3D {
          id: cubes; objectName: "cubes"; model: view.listed ? rows : view.rowCount
          onObjectAdded: (i, o) => view.log += "+" + i + o.objectName
          onObjectRemoved: (i, o) => view.log += "-" + i
          Model {
            objectName: "m" + index; source: "#Cube"; x: -30 + index * 30
            scale: Qt.vector3d(0.2, 0.2, 0.2)
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "red" }
          }
        }
      }
      """

  Rule: One node a row

    Scenario: A number model makes that many nodes
      Then the property "count" of "cubes" is 3
      And the object "m2" exists
      And the property "log" of "view" is "+0m0+1m1+2m2"

    Scenario Outline: objectAt gives the node of a row, or nothing outside the model
      When the method "pick" of "view" is called with <row>
      Then the property "picked" of "view" is "<name>"

      Examples:
        | row | name |
        | 1   | m1   |
        | 3   | none |

    Scenario: A ListModel makes a node per element, and one more when one is added
      When the property "listed" of "view" is set to true
      Then the property "count" of "cubes" is 2
      When the method "append" of "rows" is called with {"n": 2}
      Then the property "count" of "cubes" is 3
      And the property "log" of "view" is "+0m0+1m1+2m2-2-1-0+0m0+1m1+2m2"

    Scenario: A model of no rows removes every node
      When the property "rowCount" of "view" is set to 0
      Then the property "count" of "cubes" is 0
      And the object "m0" does not exist
      And the pixel at 50, 50 of the window is within 2 of "#000000"

  Rule: The nodes are drawn, and move with the Repeater3D

    Scenario: Each node is drawn
      Then the pixel at 20, 50 of the window is within 2 of "#ff0000"
      And the pixel at 50, 50 of the window is within 2 of "#ff0000"
      And the pixel at 80, 50 of the window is within 2 of "#ff0000"
      And the pixel at 35, 50 of the window is within 2 of "#000000"

    Scenario: Moving the Repeater3D moves its nodes
      When the property "x" of "cubes" is set to 15
      Then the pixel at 20, 50 of the window is within 2 of "#000000"
      And the pixel at 35, 50 of the window is within 2 of "#ff0000"
