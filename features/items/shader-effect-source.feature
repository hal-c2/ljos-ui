# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-shadereffectsource.html
# Verified on Qt 6.11.2 (qml6, offscreen). Qt's software scene graph
# draws a ShaderEffectSource as it does on GL, so its pixels are compared.
#
# How a picture is sampled (mipmap, textureMirroring, wrapMode) and a
# source that holds its ShaderEffectSource are what the software scene
# graph does not do as GL does: it never mipmaps, it flips the
# ShaderEffectSource's own drawing for NoMirroring, and it never finishes
# drawing a source that holds it. Those are pinned on Ljós only, as qml6
# on GL (xcb in the off-screen desktop) drew them; wrapMode, which only a
# custom shader shows, is in crates/ljos_quick/tests/shader_source.rs.

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
        id: root; objectName: "root"
        property bool sameSource: s.sourceItem === src
        property int presses: 0
        width: 200; height: 100
        color: "#000000"
        Item {
          id: src; objectName: "src"; width: 30; height: 30
          Rectangle { objectName: "a"; width: 10; height: 30; color: "#ff0000" }
          Rectangle { x: 10; width: 10; height: 30; color: "#00ff00" }
          Rectangle { x: 20; width: 10; height: 30; color: "#0000ff" }
          MouseArea { anchors.fill: parent; onPressed: root.presses++ }
        }
        ShaderEffectSource {
          id: s; objectName: "s"; sourceItem: src
          x: 100; width: 60; height: 30
        }
      }
      """

  @qt-parity
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

  @qt-parity
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

  @qt-parity
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

    Scenario: scheduleUpdate takes a picture that is not live once more
      When the property "live" of "s" is set to false
      And 50 milliseconds pass
      And the property "color" of "a" is set to "#ffffff"
      And 50 milliseconds pass
      And the method "scheduleUpdate" of "s" is called
      And 50 milliseconds pass
      And the property "color" of "a" is set to "#ffff00"
      And 50 milliseconds pass
      Then the pixel at 105, 15 of the window is "#ffffff"
      And the pixel at 5, 15 of the window is "#ffff00"

    Scenario: format and samples are taken and the picture is the same
      When the property "format" of "s" is set to 2
      And the property "samples" of "s" is set to 4
      Then the pixel at 105, 15 of the window is "#ff0000"
      And the pixel at 155, 15 of the window is "#0000ff"

    Scenario: A hidden source still takes the pointer, as in Qt
      When the property "hideSource" of "s" is set to true
      And the user presses the pointer at 15, 15
      And the user releases the pointer at 15, 15
      Then the property "presses" of "root" is 1
      And the pixel at 15, 15 of the window is "#000000"

  @ljos-only
  Rule: Sampling and recursion, as on GL

    Background:
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 200; height: 100; color: "#000000"
          Item {
            id: quads; x: 150; width: 20; height: 20
            Rectangle { width: 10; height: 10; color: "#ff0000" }
            Rectangle { x: 10; width: 10; height: 10; color: "#00ff00" }
            Rectangle { y: 10; width: 10; height: 10; color: "#0000ff" }
            Rectangle { x: 10; y: 10; width: 10; height: 10; color: "#ffffff" }
          }
          Item {
            id: stripes; x: 150; y: 30; width: 64; height: 64
            Repeater {
              model: 8
              Rectangle { x: index * 8; width: 1; height: 64; color: "#ffffff" }
            }
          }
          ShaderEffectSource { id: q; objectName: "q"; sourceItem: quads; width: 20; height: 20 }
          ShaderEffect { x: 30; width: 20; height: 20; property var source: q }
          ShaderEffectSource {
            objectName: "m"; sourceItem: stripes; y: 30; width: 8; height: 8
          }
        }
        """

    Scenario: A shader samples the picture the right way up by default
      Then the pixel at 35, 5 of the window is "#ff0000"
      And the pixel at 35, 15 of the window is "#0000ff"

    Scenario Outline: textureMirroring <mode> flips what a shader samples
      When the property "textureMirroring" of "q" is set to <mode>
      Then the pixel at 35, 5 of the window is "<top left>"
      And the pixel at 45, 15 of the window is "<bottom right>"
      And the pixel at 5, 5 of the window is "#ff0000"

      Examples:
        | mode | top left | bottom right |
        | 0    | #0000ff  | #00ff00      |
        | 1    | #ffffff  | #ff0000      |

    Scenario: A mipmapped picture drawn smaller averages what it covers
      When the property "mipmap" of "m" is set to true
      Then the pixel at 3, 33 of the window is within 4 of "#1f1f1f"

    Scenario Outline: A source that holds its ShaderEffectSource is drawn into itself
      Given the QML:
        """qml
        import QtQuick
        Rectangle {
          width: 200; height: 100; color: "#000000"
          Item {
            id: rec; width: 80; height: 40
            Rectangle { width: 20; height: 40; color: "#ff0000" }
            ShaderEffectSource { x: 20; width: 60; height: 40; sourceItem: rec; recursive: <recursive> }
          }
        }
        """
      When 200 milliseconds pass
      Then the pixel at 30, 20 of the window is "#ff0000"
      And the pixel at 75, 20 of the window is within 40 of "#ff0000"

      Examples:
        | recursive |
        | true      |
        | false     |
