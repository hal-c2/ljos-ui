# Sources:
#   https://doc.qt.io/qt-6/qt-add-qml-module.html#resource-prefix
#   https://doc.qt.io/qt-6/qtqml-qml-script-compiler.html
#   docs/quick3d.md

# On Qt these files are compiled into the program; a run of Ljós reads them
# from the module's directory, so qml6 with no build has nothing to compare.
@ljos-only
Feature: The files of a module, by the address a build gives them
  A build keeps a module's files in the program at
  qrc:/qt/qml/<the module's name as a path>/. Run from its sources, a program
  finds them in the directory of the module it knows, whatever loads them.

  Background:
    Given the text file "Kit/Parts/qmldir":
      """
      module Kit.Parts
      """
    And the text file "Kit/Parts/art/red.svg":
      """
      <svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="40" height="30" fill="#ff0000"/></svg>
      """
    And the text file "Kit/secret.svg":
      """
      <svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="40" height="30"/></svg>
      """

  Rule: A picture

    Background:
      Given the QML:
        """qml
        import QtQuick
        Image { objectName: "picture"; width: 40; height: 30 }
        """

    Scenario: A picture of the module is found at its address
      When the property "source" of "picture" is set to "qrc:/qt/qml/Kit/Parts/art/red.svg"
      Then the property "status" of "picture" is 1
      And the property "implicitWidth" of "picture" is 40

    Scenario: A file the module's directory does not have is an error
      When the property "source" of "picture" is set to "qrc:/qt/qml/Kit/Parts/art/blue.svg"
      Then the property "status" of "picture" is 3
      And the log has a warning matching "Cannot open: qrc:/qt/qml/Kit/Parts/art/blue.svg"

    Scenario: An address cannot climb out of the module's directory
      When the property "source" of "picture" is set to "qrc:/qt/qml/Kit/Parts/../secret.svg"
      Then the property "status" of "picture" is 3

    Scenario: A resource address no module has names nothing
      When the property "source" of "picture" is set to "qrc:/qt/qml/Other/art/red.svg"
      Then the property "status" of "picture" is 3

  Rule: A shader

    Scenario: A shader of the module is found at its address
      Given the text file "Kit/Parts/tint.frag":
        """
        void MAIN()
        {
            FRAGCOLOR = vec4(0.0, 1.0, 0.0, 1.0);
        }
        """
      And the QML:
        """qml
        import QtQuick
        import QtQuick3D
        Rectangle {
          width: 100; height: 100; color: "black"
          View3D {
            anchors.fill: parent
            environment: SceneEnvironment {
              backgroundMode: SceneEnvironment.Color; clearColor: "red"
              effects: Effect {
                passes: Pass {
                  shaders: Shader { stage: Shader.Fragment; shader: "qrc:/qt/qml/Kit/Parts/tint.frag" }
                }
              }
            }
          }
        }
        """
      Then the pixel at 50, 50 of the window is within 2 of "#00ff00"
