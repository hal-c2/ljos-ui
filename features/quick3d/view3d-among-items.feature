# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-sceneenvironment.html
#   https://doc.qt.io/qt-6/qtquick-visualcanvas-visualparent.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation.

@backlog
Feature: A View3D among other items
  A View3D is an item like any other: the items declared after it are drawn
  over it, and its picture is clipped to its own area.

  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 100; height: 100; color: "black"
        View3D {
          x: 20; y: 20; width: 60; height: 60
          environment: SceneEnvironment {
            clearColor: "#0000ff"; backgroundMode: SceneEnvironment.Color
          }
          PerspectiveCamera { z: 300 }
        }
        Rectangle {
          objectName: "badge"; x: 40; y: 40; width: 20; height: 20
          color: "#00ff00"
        }
      }
      """

  Scenario: The view fills only its own area
    Then the pixel at 10, 10 of the window is "#000000"
    And the pixel at 25, 25 of the window is "#0000ff"

  Scenario: An item declared after the view is drawn over it
    Then the pixel at 50, 50 of the window is "#00ff00"

  Scenario: An item moved off the view shows the view again
    When the property "x" of "badge" is set to 0
    Then the pixel at 50, 50 of the window is "#0000ff"
