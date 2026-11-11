# Sources:
#   https://doc.qt.io/qt-6/qquickpainteditem.html
#   https://doc.qt.io/qt-6/qml-qt-labs-lottieqt-lottieanimation.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: LottieAnimation is a painted item
  A LottieAnimation paints itself, as every painted item of Qt Quick does,
  so it has what a painted item has: contentsScale, the scale it paints at;
  fillColor, the colour behind what it paints; contentsSize, how much it
  paints beyond its own size; textureSize, the resolution of what it paints;
  and renderTarget. It paints the file at the file's own size from its
  top-left corner, whatever size the item is given, and nothing of what it
  paints shows outside the item and its contents.

  The corner file is 40 by 20 with a red square 20 wide in its top-left
  corner; the halves file is 40 by 20, red on its left half and green on its
  right. The window behind it is black.

  Background:
    Given the text file "corner.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":40,"h":20,"layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"rc","p":{"a":0,"k":[10,10]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}]}]}
      """
    And the text file "halves.json":
      """
      {"v":"5.7.0","fr":10,"ip":0,"op":10,"w":40,"h":20,"layers":[{"ty":4,"ip":0,"op":10,"st":0,"ks":{},"shapes":[
      {"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[10,10]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}}]},
      {"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[30,10]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[0,1,0,1]},"o":{"a":0,"k":100}}]}]}]}
      """

  Rule: The properties of a painted item

    Scenario: A painted item's properties start at Qt's defaults
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property string seen: "?"
          LottieAnimation { id: anim; source: "corner.json" }
          Component.onCompleted: seen = [anim.contentsScale, anim.fillColor,
            anim.renderTarget, anim.contentsSize.width, anim.contentsSize.height,
            anim.textureSize.width, anim.textureSize.height].join(" ")
        }
        """
      Then the property "seen" of "root" is "1 #00000000 0 -1 -1 -1 -1"

    Scenario: The names of the render targets and the performance hint
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          objectName: "root"
          property string seen: [LottieAnimation.Image, LottieAnimation.FramebufferObject,
            LottieAnimation.InvertedYFramebufferObject,
            LottieAnimation.FastFBOResizing].join(" ")
        }
        """
      Then the property "seen" of "root" is "0 1 2 1"

    Scenario: Each property says when it changes
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          id: root; objectName: "root"
          property var seen: []
          LottieAnimation {
            source: "corner.json"
            onContentsScaleChanged: root.seen = root.seen.concat(["scale"])
            onFillColorChanged: root.seen = root.seen.concat(["fill"])
            onContentsSizeChanged: root.seen = root.seen.concat(["contents"])
            onTextureSizeChanged: root.seen = root.seen.concat(["texture"])
            onRenderTargetChanged: root.seen = root.seen.concat(["target"])
            Component.onCompleted: {
              contentsScale = 2; fillColor = "red"; contentsSize = Qt.size(5, 5)
              textureSize = Qt.size(5, 5); renderTarget = LottieAnimation.FramebufferObject
            }
          }
        }
        """
      Then the property "seen" of "root" is ["scale", "fill", "contents", "texture", "target"]

  Rule: What a painted item paints

    Scenario: The file is painted at its own size, not stretched to the item
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 120; height: 80
          LottieAnimation {
            source: "corner.json"; autoPlay: false
            Component.onCompleted: { width = 120; height = 80 }
          }
        }
        """
      Then the pixel at 10, 10 of the window is "#ff0000"
      And the pixel at 30, 10 of the window is "#000000"
      And the pixel at 10, 30 of the window is "#000000"

    Scenario: An animation scaled by contentsScale fits its box
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 120; height: 80
          LottieAnimation {
            anchors.fill: parent
            source: "halves.json"; autoPlay: false
            contentsScale: Math.min(width / 40, height / 20)
          }
        }
        """
      Then the pixel at 30, 30 of the window is "#ff0000"
      And the pixel at 90, 30 of the window is "#00ff00"
      And the pixel at 60, 70 of the window is "#000000"

    Scenario: What is painted beyond the item is cut off
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation { source: "corner.json"; autoPlay: false; contentsScale: 2 }
        }
        """
      Then the pixel at 30, 10 of the window is "#ff0000"
      And the pixel at 10, 30 of the window is "#000000"

    Scenario: fillColor paints behind the animation
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation {
            source: "corner.json"; autoPlay: false; fillColor: "#0000ff"
            Component.onCompleted: { width = 80; height = 60 }
          }
        }
        """
      Then the pixel at 10, 10 of the window is "#ff0000"
      And the pixel at 60, 40 of the window is "#0000ff"
      And the pixel at 90, 10 of the window is "#000000"
      And the pixel at 10, 70 of the window is "#000000"

    Scenario: A fill that lets light through shows what is behind it
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          Rectangle { anchors.fill: parent; color: "#ffffff" }
          LottieAnimation { source: "corner.json"; autoPlay: false; fillColor: "#800000ff" }
        }
        """
      Then the pixel at 10, 10 of the window is "#ff0000"
      And the pixel at 30, 10 of the window is within 3 of "#7f7fff"

    Scenario Outline: contentsSize times contentsScale widens what is painted
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 200; height: 200
          LottieAnimation {
            source: "corner.json"; autoPlay: false; fillColor: "#0000ff"
            contentsSize: Qt.size(50, 30); contentsScale: <scale>
          }
        }
        """
      Then the pixel at <inside> of the window is "#0000ff"
      And the pixel at <outside> of the window is "#000000"

      Examples:
        | scale | inside  | outside  |
        | 1     | 45, 25  | 55, 10   |
        | 2     | 95, 55  | 105, 10  |

    Scenario: textureSize leaves where the item paints as it was
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation {
            source: "corner.json"; autoPlay: false; fillColor: "#0000ff"
            textureSize: Qt.size(80, 40)
          }
        }
        """
      Then the pixel at 10, 10 of the window is "#ff0000"
      And the pixel at 30, 10 of the window is "#0000ff"
      And the pixel at 50, 10 of the window is "#000000"
      And the pixel at 10, 30 of the window is "#000000"

    Scenario: A contentsScale of zero paints nothing, not even the fill
      Given the QML:
        """qml
        import QtQuick
        import Qt.labs.lottieqt
        Item {
          width: 100; height: 100
          LottieAnimation {
            source: "corner.json"; autoPlay: false; fillColor: "#0000ff"; contentsScale: 0
          }
        }
        """
      Then the pixel at 10, 10 of the window is "#000000"
      And the pixel at 30, 10 of the window is "#000000"
