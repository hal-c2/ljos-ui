# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-view3d.html
#   https://doc.qt.io/qt-6/qml-pickresult.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-material.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Picking everything on a ray, and rays from anywhere
  View3D.pickAll(x, y) gives every model the ray through a point of the
  view meets, nearest first. rayPick(origin, direction) and
  rayPickAll(origin, direction) shoot the ray from any point of the scene.
  A face the material's cullMode leaves out is not picked.

  # On the Z axis: a cube at z 200 (its front face at 250), a sphere at the
  # origin and a rectangle at z -200 facing +Z, all pickable.
  Background:
    Given the QML:
      """qml
      import QtQuick
      import QtQuick3D
      View3D {
        id: view; objectName: "view"; width: 100; height: 100
        property string hits: ""
        property real distance: -1
        property bool twoSided: false
        function names(all) { return all.map(r => r.objectHit.objectName).join(",") }
        function pickAllAt(x, y) { hits = names(pickAll(x, y)) }
        function rayAll(z, dz) { hits = names(rayPickAll(Qt.vector3d(0, 0, z), Qt.vector3d(0, 0, dz))) }
        function ray(y, z, dz) {
          const r = rayPick(Qt.vector3d(0, y, z), Qt.vector3d(0, 0, dz))
          hits = r.objectHit ? r.objectHit.objectName : ""; distance = r.distance
        }
        PerspectiveCamera { z: 600 }
        Model { objectName: "near"; source: "#Cube"; z: 200; pickable: true }
        Model { objectName: "far"; source: "#Sphere"; pickable: true }
        Model {
          objectName: "back"; source: "#Rectangle"; z: -200; pickable: true
          materials: DefaultMaterial {
            cullMode: view.twoSided ? Material.NoCulling : Material.BackFaceCulling
          }
        }
      }
      """

  Rule: Every hit, nearest first

    Scenario: pickAll lists every model on the ray from the camera
      When the method "pickAllAt" of "view" is called with 50, 50
      Then the property "hits" of "view" is "near,far,back"

    Scenario: rayPickAll lists them by distance from the ray's origin
      When the method "rayAll" of "view" is called with -600, 1
      Then the property "hits" of "view" is "far,near"

    Scenario: A face the material does not cull can be picked from behind
      When the property "twoSided" of "view" is set to true
      And the method "rayAll" of "view" is called with -600, 1
      Then the property "hits" of "view" is "back,far,near"

  Rule: A ray from anywhere picks what it meets first

    Scenario: rayPick gives the nearest model and its distance from the origin
      When the method "ray" of "view" is called with 0, 600, -1
      Then the property "hits" of "view" is "near"
      And the property "distance" of "view" is within 0.01 of 350

    Scenario: A ray that meets nothing picks nothing
      When the method "ray" of "view" is called with 500, 600, -1
      Then the property "hits" of "view" is ""
