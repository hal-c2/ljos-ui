# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shadereffectsource.html
# Verified on Qt 6.11.2 (qml6, offscreen). Qt's software scene graph
# draws a ShaderEffectSource as it does on GL, so its pixels are compared.

@qt-parity
Feature: ShaderEffectSource
  A ShaderEffectSource draws a picture of another item, its source,
  stretched over itself. It can hide the source, show only a part of it,
  take the picture at another resolution, or keep the first picture it
  took.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        property bool sameSource: s.sourceItem === src
        width: 200; height: 100
        color: "#000000"
        Item {
          id: src; objectName: "src"; width: 30; height: 30
          Rectangle { objectName: "a"; width: 10; height: 30; color: "#ff0000" }
          Rectangle { x: 10; width: 10; height: 30; color: "#00ff00" }
          Rectangle { x: 20; width: 10; height: 30; color: "#0000ff" }
        }
        ShaderEffectSource {
          id: s; objectName: "s"; sourceItem: src
          x: 100; width: 60; height: 30
        }
      }
      """

  Rule: Properties

    Scenario Outline: Defaults
      Then the property "<property>" of "s" is <value>

      Examples:
        | property         | value |
        | live             | true  |
        | hideSource       | false |
        | recursive        | false |
        | format           | 1     |
        | mipmap           | false |
        | smooth           | true  |
        | wrapMode         | 0     |
        | textureMirroring | 2     |
        | samples          | 0     |

    Scenario: The source is the item it was given
      Then the property "sameSource" of "root" is true

  Rule: Enumerations

    Scenario Outline: ShaderEffectSource.<name> is <value>
      Given the QML:
        """qml
        import QtQuick
        QtObject { objectName: "e"; property int v: ShaderEffectSource.<name> }
        """
      Then the property "v" of "e" is <value>

      Examples:
        | name               | value |
        | RGBA8              | 1     |
        | RGBA16F            | 2     |
        | RGBA32F            | 3     |
        | ClampToEdge        | 0     |
        | RepeatHorizontally | 1     |
        | RepeatVertically   | 2     |
        | Repeat             | 3     |
        | NoMirroring        | 0     |
        | MirrorHorizontally | 1     |
        | MirrorVertically   | 2     |

  Rule: Pixels

    Scenario: The picture is stretched over the item, and the source stays
      Then the pixel at 105, 15 of the window is "#ff0000"
      And the pixel at 130, 15 of the window is "#00ff00"
      And the pixel at 155, 15 of the window is "#0000ff"
      And the pixel at 5, 15 of the window is "#ff0000"

    Scenario: hideSource hides the source and not the picture
      When the property "hideSource" of "s" is set to true
      Then the pixel at 5, 15 of the window is "#000000"
      And the pixel at 105, 15 of the window is "#ff0000"
      And the property "visible" of "src" is true

    Scenario: sourceRect shows a part of the source
      When the property "sourceRect" of "s" is bound to "Qt.rect(10, 0, 20, 30)"
      Then the pixel at 105, 15 of the window is "#00ff00"
      And the pixel at 155, 15 of the window is "#0000ff"

    Scenario: A one-pixel texture is the colour at the source's centre
      When the property "smooth" of "s" is set to false
      And the property "textureSize" of "s" is bound to "Qt.size(1, 1)"
      Then the pixel at 105, 15 of the window is "#00ff00"
      And the pixel at 155, 15 of the window is "#00ff00"

    Scenario: A live picture follows the source
      When the property "color" of "a" is set to "#ffffff"
      Then the pixel at 105, 15 of the window is "#ffffff"

    Scenario: A picture that is not live keeps the first one it took
      When the property "live" of "s" is set to false
      And 50 milliseconds pass
      And the property "color" of "a" is set to "#ffffff"
      Then the pixel at 5, 15 of the window is "#ffffff"
      And the pixel at 105, 15 of the window is "#ff0000"

    Scenario: A source that is not visible is still in the picture
      When the property "visible" of "src" is set to false
      Then the pixel at 5, 15 of the window is "#000000"
      And the pixel at 105, 15 of the window is "#ff0000"
