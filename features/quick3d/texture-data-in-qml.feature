# Sources:
#   https://doc.qt.io/qt-6/qquick3dtexturedata.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-texturedata.html
#   https://doc.qt.io/qt-6/qml-qtquick3d-texture.html#textureData-prop
#   corpus/qtdoc/examples/demos/osmbuildings/customshadertiles.frag, which reads
#   a TextureData at 1 - UV0.y to have the first row on top
# Ljós only: Qt's QML cannot create a TextureData or call these functions, an
# application's C++ subclass of QQuick3DTextureData does, with QImage to read
# a file. Here QML can, under the names of that C++ API, and setImageFileData
# stands in for what QImage does.

@ljos-only
Feature: Texture data written in QML
  A TextureData can be declared in QML. setSize, setFormat, setDepth,
  setHasTransparency and setTextureData are the functions Qt documents for
  QQuick3DTextureData, and update commits what they set. setImageFileData
  takes the bytes of an image file, reads them away from the UI, and shows
  the picture when it is read.

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
          OrthographicCamera { z: 600 }
          Model {
            source: "#Rectangle"
            materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting
              diffuseMap: Texture { textureData: TextureData {
                objectName: "data"
                function pixels(r, g, b) {
                  setSize(Qt.size(1, 1)); setFormat(TextureData.RGBA8); setHasTransparency(false)
                  setTextureData(new Uint8Array([r, g, b, 255])); update()
                }
                readonly property var greenPng: [137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1,
                  0, 0, 0, 1, 8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 96, 248, 207,
                  240, 31, 0, 4, 1, 1, 255, 113, 235, 71, 229, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]
                function file() { setImageFileData(new Uint8Array(greenPng).buffer) }
                function notAFile() { setImageFileData(new Uint8Array([1, 2, 3, 4])) }
              } }
            } }
        } }
      """

  Rule: Pixels QML commits are shown

    Scenario: A red pixel fills the model
      When the method "pixels" of "data" is called with 255, 0, 0
      Then the pixel at 100, 100 of the window is "#ff0000"

    Scenario: Nothing shows before update
      When the method "setTextureData" of "data" is called with "not committed"
      Then the log has no warning matching "TextureData"

  Rule: The bytes of an image file are read to pixels

    Scenario: A PNG's picture is shown once read
      When the method "file" of "data" is called
      And 200 milliseconds pass
      Then the pixel at 100, 100 of the window is "#00ff00"

    Scenario: The size and format come from the file
      When the method "pixels" of "data" is called with 255, 0, 0
      And the method "file" of "data" is called
      And 200 milliseconds pass
      Then the pixel at 100, 100 of the window is "#00ff00"

    Scenario: Bytes that are no image are reported
      When the method "notAFile" of "data" is called
      And 200 milliseconds pass
      Then the log has a warning matching "TextureData"

  Rule: The first row of the data is the bottom of the picture

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
            OrthographicCamera { z: 600 }
            Texture { id: rows; minFilter: Texture.Nearest; magFilter: Texture.Nearest; textureData: TextureData {
              objectName: "rows"
              function red_then_blue() {
                setSize(Qt.size(1, 2)); setFormat(TextureData.RGBA8); setHasTransparency(false)
                setTextureData(new Uint8Array([255, 0, 0, 255, 0, 0, 255, 255])); update() }
              // One pixel wide, a red row and then a blue one.
              readonly property var redThenBluePng: [137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 2, 8, 6, 0, 0, 0, 153, 129, 182, 39, 0, 0, 0, 18, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 159, 129, 129, 225, 255, 127, 0, 17, 248, 3, 253, 223, 215, 157, 159, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]
              function red_then_blue_file() { setImageFileData(new Uint8Array(redThenBluePng).buffer) }
            } }
            Model { x: -50; scale: Qt.vector3d(1, 2, 1); source: "#Rectangle"
              materials: DefaultMaterial { lighting: DefaultMaterial.NoLighting; diffuseMap: rows } }
            Model { x: 50; scale: Qt.vector3d(1, 2, 1); source: "#Rectangle"
              materials: CustomMaterial {
                property TextureInput shown: TextureInput { texture: rows }; shadingMode: CustomMaterial.Unshaded
                __fragmentShaderCode: "void MAIN() { FRAGCOLOR = vec4(texture(shown, vec2(UV0.x, 1.0 - UV0.y)).rgb, 1.0); }"
              } }
          } }
        """

    Scenario: A material shows the first row at the bottom
      When the method "red_then_blue" of "rows" is called
      Then the pixel at 50, 60 of the window is "#0000ff"
      And the pixel at 50, 140 of the window is "#ff0000"

    Scenario: A snippet reading at 1 - UV0.y has the first row on top
      When the method "red_then_blue" of "rows" is called
      Then the pixel at 150, 60 of the window is "#ff0000"
      And the pixel at 150, 140 of the window is "#0000ff"

    Scenario: A file's first row counts as the data's first
      When the method "red_then_blue_file" of "rows" is called
      And 200 milliseconds pass
      Then the pixel at 50, 60 of the window is "#0000ff"
      And the pixel at 50, 140 of the window is "#ff0000"
      And the pixel at 150, 60 of the window is "#ff0000"
