# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-item.html (layer.* properties)
#   https://doc.qt.io/qt-6/qtquick-visualcanvas-scenegraph-renderer.html
# Verified on Qt 6.11.2 (qml6, offscreen) for the properties. Qt's
# software scene graph draws no ShaderEffect, so what one samples of an
# item with a layer is pinned on Ljós only, as qml6 on GL (xcb in the
# off-screen desktop) drew it: the item's picture, and nothing for an
# item without a layer.
#
# Ljós takes the picture of a layer an effect samples. Qt also draws the
# item through that picture, so a layer that is not live or is taken at
# a smaller textureSize shows on the item itself; Ljós does not yet, and
# `layer.effect` is kept and not drawn: the item draws itself.

Feature: Item layer
  An item's layer turns it into a texture: a ShaderEffect that names the
  item samples the picture of it and its children, as it samples a
  ShaderEffectSource's.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"
        width: 200; height: 100; color: "#000000"
        Item {
          id: src; objectName: "src"; width: 30; height: 30
          Rectangle { objectName: "a"; width: 10; height: 30; color: "#ff0000" }
          Rectangle { x: 10; width: 10; height: 30; color: "#00ff00" }
          Rectangle { x: 20; width: 10; height: 30; color: "#0000ff" }
        }
        ShaderEffect { x: 100; width: 60; height: 30; property var source: src }
        QtObject {
          id: probe; objectName: "probe"
          property bool enabled: src.layer.enabled; property bool smooth: src.layer.smooth
          property bool mipmap: src.layer.mipmap; property int wrapMode: src.layer.wrapMode
          property int textureMirroring: src.layer.textureMirroring
          property string samplerName: src.layer.samplerName
          property int format: src.layer.format; property int samples: src.layer.samples
          property bool live: src.layer.live
          property bool noEffect: src.layer.effect === null
          property real textureWidth: src.layer.textureSize.width
          property real rectWidth: src.layer.sourceRect.width
        }
      }
      """

  @qt-parity
  Rule: Properties

    Scenario Outline: Defaults
      Then the property "<property>" of "probe" is <value>

      Examples:
        | property         | value    |
        | enabled          | false    |
        | smooth           | false    |
        | mipmap           | false    |
        | wrapMode         | 0        |
        | textureMirroring | 2        |
        | samplerName      | "source" |
        | format           | 1        |
        | samples          | 0        |
        | live             | true     |
        | noEffect         | true     |
        | textureWidth     | -1       |
        | rectWidth        | 0        |

    Scenario: A layer is set in the item's own declaration
      Given the QML:
        """qml
        import QtQuick
        Item {
          Rectangle {
            id: r; width: 10; height: 10
            layer.enabled: true; layer.smooth: true; layer.samplerName: "tex"
          }
          QtObject {
            objectName: "q"
            property bool enabled: r.layer.enabled
            property bool smooth: r.layer.smooth
            property string samplerName: r.layer.samplerName
          }
        }
        """
      Then the property "enabled" of "q" is true
      And the property "smooth" of "q" is true
      And the property "samplerName" of "q" is "tex"

  @ljos-only
  Rule: What a ShaderEffect samples, as on GL

    Scenario: An item without a layer gives nothing to sample
      Then the pixel at 105, 15 of the window is "#000000"
      And the pixel at 5, 15 of the window is "#ff0000"

    Scenario: An item with a layer is sampled as its picture
      When the property "layer.enabled" of "src" is set to true
      Then the pixel at 105, 15 of the window is "#ff0000"
      And the pixel at 130, 15 of the window is "#00ff00"
      And the pixel at 155, 15 of the window is "#0000ff"
      And the pixel at 5, 15 of the window is "#ff0000"

    Scenario: The picture follows the item
      When the property "layer.enabled" of "src" is set to true
      And the property "color" of "a" is set to "#ffffff"
      Then the pixel at 105, 15 of the window is "#ffffff"

    Scenario: The layer's sourceRect is the part that is sampled
      When the property "layer.enabled" of "src" is set to true
      And the property "layer.sourceRect" of "src" is bound to "Qt.rect(10, 0, 20, 30)"
      Then the pixel at 105, 15 of the window is "#00ff00"
      And the pixel at 155, 15 of the window is "#0000ff"

    Scenario: A layer that is not live keeps the first picture it took
      When the property "layer.enabled" of "src" is set to true
      And the property "layer.live" of "src" is set to false
      And 50 milliseconds pass
      And the property "color" of "a" is set to "#ffffff"
      Then the pixel at 105, 15 of the window is "#ff0000"
