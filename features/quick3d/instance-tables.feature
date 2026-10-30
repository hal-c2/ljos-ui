# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-instancing.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-fileinstancing.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-randominstancing.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-helpers-instancerange.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: Instance tables read from a file or made at random
  Every instance table answers what it holds: instancePosition,
  instanceScale, instanceRotation, instanceColor and instanceCustomData of
  an index. A FileInstancing reads its table from an XML file: an
  InstanceTable element with an Instance element a copy, its attributes
  the copy's position, scale, eulerRotation, quaternion, custom data and
  colour; a vector's missing trailing numbers are zeros. A RandomInstancing
  (QtQuick3D.Helpers) makes instanceCount entries, each attribute picked
  within an InstanceRange; the same randomSeed makes the same table. A
  proportional range moves every component by the same share, as a scale
  that keeps its shape; a range left alone moves each by its own, as the
  documentation's examples have it (its text calls true the default).
  Ljós builds no more of QtQuick3D.Helpers than these two: its other types
  stand in as empty Items, as a module Ljós does not build does.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin.
  Rule: A file's table is drawn and can be read back

    Background:
      Given the text file "table.xml":
        """
        <InstanceTable>
          <Instance position="-50 0 0" color="red"/>
          <Instance position="50" color="#0000ff" custom="1 2"/>
          <Unknown position="0 0 0"/>
        </InstanceTable>
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          objectName: "root"; width: 200; height: 200; color: "black"
          property real x1
          property real scale1
          property real custom1
          function read() {
            x1 = file.instancePosition(1).x
            scale1 = file.instanceScale(1).y
            custom1 = file.instanceCustomData(1).y
          }
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.2, 0.2, 0.2)
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" }
              instancing: FileInstancing { id: file; objectName: "file"; source: "table.xml" }
            }
          }
        }
        """

    Scenario: Each Instance element is one copy
      Then the property "instanceCount" of "file" is 2
      And the pixel at 50, 100 of the window is within 2 of "#ff0000"
      And the pixel at 150, 100 of the window is within 2 of "#0000ff"
      And the pixel at 100, 100 of the window is "#202020"

    Scenario: Missing numbers are zeros and a missing scale is one
      When the method "read" of "root" is called
      Then the property "x1" of "root" is within 0.001 of 50
      And the property "scale1" of "root" is within 0.001 of 1
      And the property "custom1" of "root" is within 0.001 of 2

  Rule: A random table stays within its ranges

    Background:
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          objectName: "root"; width: 200; height: 200; color: "black"
          property bool inside: true
          property bool same: true
          property bool uniform: true
          property bool red: true
          property bool spread: false
          function check() { for (let i = 0; i < 20; ++i) {
            const p = a.instancePosition(i), s = a.instanceScale(i)
            if (p.x < -50 || p.x > 50 || p.y < -50 || p.y > 50 || p.z !== 0) inside = false
            if (!p.fuzzyEquals(b.instancePosition(i), 0.0001)) same = false
            if (Math.abs(p.x - p.y) > 1) spread = true
            if (Math.abs(s.x - s.y) > 0.0001 || Math.abs(s.x - s.z) > 0.0001 || s.x < 1 || s.x > 2) uniform = false
            if (!Qt.colorEqual(a.instanceColor(i), "red")) red = false } }
          component Table: RandomInstancing {
            instanceCount: 20; randomSeed: 7; color: InstanceRange { from: "red"; to: "red" }
            position: InstanceRange { from: Qt.vector3d(-50, -50, 0); to: Qt.vector3d(50, 50, 0) }
            scale: InstanceRange { from: Qt.vector3d(1, 1, 1); to: Qt.vector3d(2, 2, 2); proportional: true }
          }
          Table { id: a }
          Table { id: b; objectName: "b" }
        }
        """

    Scenario: Every entry is within the ranges and a proportional range scales evenly
      When the method "check" of "root" is called
      Then the property "inside" of "root" is true
      And the property "uniform" of "root" is true
      And the property "spread" of "root" is true
      And the property "red" of "root" is true

    Scenario: The same seed makes the same table
      When the method "check" of "root" is called
      Then the property "same" of "root" is true

    Scenario: Another seed makes another table
      When the property "randomSeed" of "b" is set to 8
      And the method "check" of "root" is called
      Then the property "same" of "root" is false

  Rule: A random table is drawn

    Scenario: Copies are drawn where the table put them
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { z: 600 }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.2, 0.2, 0.2)
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" }
              instancing: RandomInstancing {
                instanceCount: 50; randomSeed: 3
                position: InstanceRange { from: Qt.vector3d(-5, -5, 0); to: Qt.vector3d(5, 5, 0) }
                color: InstanceRange { from: "lime"; to: "lime" }
              }
            }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#00ff00"
      And the pixel at 20, 20 of the window is "#202020"
      And the pixel at 180, 180 of the window is "#202020"

  @ljos-only
  Rule: The helpers Ljós does not build stand in beside the ones it does

    Scenario: A camera controller stands in and the random table is drawn
      Given the QML:
        """qml
        import QtQuick
        import QtQuick3D
        import QtQuick3D.Helpers
        Rectangle {
          width: 200; height: 200; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
            OrthographicCamera { id: camera; z: 600 }
            OrbitCameraController { camera: camera; origin: camera }
            Model {
              source: "#Rectangle"; scale: Qt.vector3d(0.2, 0.2, 0.2)
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseColor: "white" }
              instancing: RandomInstancing {
                instanceCount: 1
                color: InstanceRange { from: "lime"; to: "lime" }
              }
            }
          }
        }
        """
      Then the pixel at 100, 100 of the window is within 2 of "#00ff00"
