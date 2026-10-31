# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick3d-custommaterial.html
#   https://doc.qt.io/qt-6/qtquick3d-custom.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-instancelistentry.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation; each
# Rule is tagged as checked against Qt once qml6 has checked it.

Feature: A CustomMaterial on an instanced model reads its copy
  A model with a CustomMaterial and an instance table is drawn once for
  each entry, as any instanced model is. The vertex snippet reads the copy
  it draws: INSTANCE_MODEL_MATRIX and INSTANCE_MODELVIEWPROJECTION_MATRIX
  are MODEL_MATRIX and MODELVIEWPROJECTION_MATRIX with the copy's
  transform in them, INSTANCE_COLOR is its colour (for the snippet to
  combine with COLOR), INSTANCE_DATA its custom data and INSTANCE_INDEX
  its number. A snippet that writes no POSITION is placed as a
  PrincipledMaterial is, so each copy lands where its entry says; one that
  writes POSITION from MODELVIEWPROJECTION_MATRIX, a keyword that does not
  apply to instanced drawing, puts every copy where the model is.

  # An orthographic camera at magnification 1 shows one scene unit as one
  # pixel, so the view's centre is the scene's origin. The square is 100
  # units across, scaled to 20; its copies sit 50 units left and right.
  # The material is unshaded, so FRAGCOLOR is written as it is.
  Background:
    Given the text file "copy.vert":
      """
      VARYING vec4 tint;
      VARYING vec4 data;
      void MAIN()
      {
          tint = INSTANCE_COLOR;
          data = INSTANCE_DATA;
      }
      """
    And the text file "colour.frag":
      """
      VARYING vec4 tint;
      void MAIN()
      {
          FRAGCOLOR = tint;
      }
      """
    And the text file "data.frag":
      """
      VARYING vec4 data;
      void MAIN()
      {
          FRAGCOLOR = vec4(data.xyz, 1.0);
      }
      """
    And the text file "lift.vert":
      """
      void MAIN()
      {
          VERTEX.y += 250.0;
          POSITION = INSTANCE_MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
      }
      """
    And the text file "stacked.vert":
      """
      void MAIN()
      {
          POSITION = MODELVIEWPROJECTION_MATRIX * vec4(VERTEX, 1.0);
      }
      """
    And the text file "index.vert":
      """
      VARYING vec4 tint;
      void MAIN()
      {
          tint = INSTANCE_INDEX == 1 ? vec4(0.0, 1.0, 0.0, 1.0) : vec4(1.0, 0.0, 1.0, 1.0);
      }
      """
    And the text file "world.vert":
      """
      VARYING vec4 tint;
      void MAIN()
      {
          vec4 w = INSTANCE_MODEL_MATRIX * vec4(VERTEX, 1.0);
          tint = w.x < 0.0 ? vec4(1.0, 1.0, 0.0, 1.0) : vec4(0.0, 1.0, 1.0, 1.0);
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 200; color: "black"
        View3D {
          anchors.fill: parent
          environment: SceneEnvironment { clearColor: "#202020"; backgroundMode: SceneEnvironment.Color }
          OrthographicCamera { z: 600 }
          Model {
            objectName: "square"; source: "#Rectangle"
            scale: Qt.vector3d(0.2, 0.2, 0.2)
            materials: CustomMaterial {
              objectName: "custom"; shadingMode: CustomMaterial.Unshaded
              vertexShader: "copy.vert"; fragmentShader: "colour.frag"
            }
            instancing: InstanceList {
              objectName: "table"
              InstanceListEntry { position: Qt.vector3d(-50, 0, 0); color: "red"; customData: Qt.vector4d(0, 0, 1, 0) }
              InstanceListEntry { position: Qt.vector3d(50, 0, 0); color: "blue"; customData: Qt.vector4d(0, 1, 0, 0) }
            }
          }
        }
      }
      """

  Rule: Each copy is drawn where its entry puts it

    Scenario: Two entries draw two copies and not the model itself
      Then the pixel at 50, 100 of the window is within 2 of "#ff0000"
      And the pixel at 150, 100 of the window is within 2 of "#0000ff"
      And the pixel at 100, 100 of the window is "#202020"

    Scenario: Without a table the model is drawn once where it is
      When the property "instancing" of "square" is set to null
      Then the pixel at 100, 100 of the window is within 2 of "#ffffff"
      And the pixel at 50, 100 of the window is "#202020"

  Rule: The vertex snippet reads its copy's built-ins

    Scenario: INSTANCE_DATA is the entry's custom data
      When the property "fragmentShader" of "custom" is set to "data.frag"
      Then the pixel at 50, 100 of the window is within 2 of "#0000ff"
      And the pixel at 150, 100 of the window is within 2 of "#00ff00"

    # The square moves 250 units up in its own space, 50 once scaled.
    Scenario: INSTANCE_MODELVIEWPROJECTION_MATRIX places a copy the snippet moved
      When the property "vertexShader" of "custom" is set to "lift.vert"
      Then the pixel at 50, 50 of the window is within 2 of "#ffffff"
      And the pixel at 150, 50 of the window is within 2 of "#ffffff"
      And the pixel at 50, 100 of the window is "#202020"

    Scenario: INSTANCE_MODEL_MATRIX takes a vertex into the scene through its copy
      When the property "vertexShader" of "custom" is set to "world.vert"
      Then the pixel at 50, 100 of the window is within 2 of "#ffff00"
      And the pixel at 150, 100 of the window is within 2 of "#00ffff"

    Scenario: INSTANCE_INDEX counts the copies from 0
      When the property "vertexShader" of "custom" is set to "index.vert"
      Then the pixel at 50, 100 of the window is within 2 of "#ff00ff"
      And the pixel at 150, 100 of the window is within 2 of "#00ff00"

  Rule: MODELVIEWPROJECTION_MATRIX does not carry the copy

    Scenario: A snippet that places with MODELVIEWPROJECTION_MATRIX stacks every copy on the model
      When the property "vertexShader" of "custom" is set to "stacked.vert"
      Then the pixel at 100, 100 of the window is within 2 of "#ffffff"
      And the pixel at 50, 100 of the window is "#202020"
      And the pixel at 150, 100 of the window is "#202020"
