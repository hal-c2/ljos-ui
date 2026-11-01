# Sources:
#   https://doc.qt.io/qt-6/qtquick3d-custom.html
# Not yet verified on Qt: QtQuick3D is not installed beside qml6 on the
# reference machine. The expectations come from the documentation and the
# shipped examples, whose snippets write these.

Feature: A snippet's globals start with what they are given
  A variable outside any function of a snippet may be given a value made of
  the material's properties. Every function of the snippet then finds that
  value, and it follows the property.

  Background:
    Given the text file "global.frag":
      """
      float twice = level * 2.0;
      vec3 tone = vec3(0.0, twice,
                       0.0);
      const float KEPT = 1.0;
      void MAIN()
      {
          FRAGCOLOR = vec4(tone * KEPT, 1.0);
      }
      """
    And the text file "global.vert":
      """
      float across = 0.5 + shift;
      VARYING float seen;
      void MAIN()
      {
          seen = across;
      }
      """
    And the text file "seen.frag":
      """
      VARYING float seen;
      void MAIN()
      {
          FRAGCOLOR = vec4(0.0, seen, 0.0, 1.0);
      }
      """
    And the QML:
      """qml
      import QtQuick
      import QtQuick3D
      Rectangle {
        width: 200; height: 100; color: "black"
        View3D {
          anchors.fill: parent
          OrthographicCamera { z: 600 }
          Model {
            source: "#Rectangle"; x: -50; scale: Qt.vector3d(0.9, 0.9, 0.9)
            materials: CustomMaterial {
              objectName: "fragment"
              shadingMode: CustomMaterial.Unshaded
              fragmentShader: "global.frag"
              property real level: 0.5
            }
          }
          Model {
            source: "#Rectangle"; x: 50; scale: Qt.vector3d(0.9, 0.9, 0.9)
            materials: CustomMaterial {
              shadingMode: CustomMaterial.Unshaded
              vertexShader: "global.vert"
              fragmentShader: "seen.frag"
              property real shift: 0.5
            } }
        } }
      """

  Scenario: A global of the fragment snippet is made of a property
    Then the pixel at 50, 50 of the window is within 2 of "#00ff00"
    And the log has no warnings

  Scenario: A global of the vertex snippet is made of a property
    Then the pixel at 150, 50 of the window is within 2 of "#00ff00"
    And the log has no warnings

  Scenario: The global follows the property
    When the property "level" of "fragment" is set to 0
    Then the pixel at 50, 50 of the window is within 2 of "#000000"
