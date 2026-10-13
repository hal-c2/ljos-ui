# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-canvas.html
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html
#   https://doc.qt.io/qt-6/qml-qtquick-canvasimagedata.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Canvas
  A Canvas is an item drawn by script. It asks for its first picture by itself once it is shown,
  and after that only when told to: an idle canvas does no work. What was drawn stays until it is
  drawn over or cleared, and the picture is laid in the window like an image.

  Scenario: The first paint covers the whole canvas and comes once it is available
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 100; height: 100
        property bool availableAtStart: true
        property string region: ""
        property int paints: 0
        property bool sameContext: false
        property bool canvasLink: false
        Canvas {
          id: c
          objectName: "canvas"
          width: 40; height: 30
          Component.onCompleted: root.availableAtStart = available
          onPaint: (rect) => {
            root.paints++
            root.region = [rect.x, rect.y, rect.width, rect.height].join(",")
            var ctx = getContext("2d")
            root.sameContext = ctx === getContext("2d") && ctx === context
            root.canvasLink = ctx.canvas === c
          }
        }
      }
      """
    When 200 milliseconds pass
    Then the property "availableAtStart" of "root" is false
    And the property "available" of "canvas" is true
    And the property "paints" of "root" is 1
    And the property "region" of "root" is "0,0,40,30"
    And the property "sameContext" of "root" is true
    And the property "canvasLink" of "root" is true
    And the property "contextType" of "canvas" is "2d"

  Scenario: An idle canvas paints no more; requestPaint asks for one more paint
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 100; height: 100
        property int paints: 0
        Canvas {
          objectName: "canvas"
          width: 20; height: 20
          onPaint: root.paints++
        }
      }
      """
    When 300 milliseconds pass
    Then the property "paints" of "root" is 1
    When the method "requestPaint" of "canvas" is called
    And 200 milliseconds pass
    Then the property "paints" of "root" is 2

  Scenario: Growing a canvas paints it again and keeps what was drawn
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        width: 100; height: 100
        property string region: ""
        property string kept: ""
        property int paints: 0
        Canvas {
          id: c
          objectName: "canvas"
          width: 40; height: 40
          onPaint: (rect) => {
            root.paints++
            var ctx = getContext("2d")
            root.region = [rect.x, rect.y, rect.width, rect.height].join(",")
            if (root.paints === 1) {
              ctx.fillStyle = "#0000ff"
              ctx.fillRect(0, 0, 10, 10)
            } else {
              root.kept = Array.from(ctx.getImageData(2, 2, 1, 1).data).join(",")
            }
          }
        }
      }
      """
    When 200 milliseconds pass
    And the property "width" of "canvas" is set to 60
    And 200 milliseconds pass
    Then the property "paints" of "root" is 2
    And the property "region" of "root" is "0,0,60,40"
    And the property "kept" of "root" is "0,0,255,255"
    And the property "canvasSize.width" of "canvas" is 60

  Scenario: The picture is drawn in the window
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        width: 100; height: 100; color: "black"
        Canvas {
          x: 10; y: 10; width: 50; height: 50
          onPaint: {
            var ctx = getContext("2d")
            ctx.fillStyle = "#ff0000"
            ctx.fillRect(0, 0, 20, 20)
            ctx.fillStyle = "#00ff00"
            ctx.fillRect(20, 20, 30, 30)
          }
        }
      }
      """
    When 200 milliseconds pass
    Then the pixel at 15, 15 of the window is "#ff0000"
    And the pixel at 45, 45 of the window is "#00ff00"
    And the pixel at 45, 15 of the window is "#000000"
    And the pixel at 75, 75 of the window is "#000000"

  Scenario: Drawing again changes the window's picture
    Given the QML:
      """qml
      import QtQuick
      Rectangle {
        objectName: "root"
        width: 100; height: 100; color: "black"
        property string fill: "#ff0000"
        Canvas {
          objectName: "canvas"
          width: 100; height: 100
          onPaint: {
            var ctx = getContext("2d")
            ctx.fillStyle = parent.fill
            ctx.fillRect(0, 0, 100, 100)
          }
        }
      }
      """
    When 200 milliseconds pass
    And the property "fill" of "root" is set to "#0000ff"
    And the method "requestPaint" of "canvas" is called
    And 200 milliseconds pass
    Then the pixel at 50, 50 of the window is "#0000ff"

  Scenario: An image is drawn once it has loaded
    Given a 4 by 4 image file "blue.png" filled with "#0000ff"
    And the QML:
      """qml
      import QtQuick
      Rectangle {
        id: root; objectName: "root"
        width: 100; height: 100; color: "black"
        property bool loadingAtStart: false; property bool loaded: false
        property string at: ""; property string stretched: ""
        Canvas {
          objectName: "canvas"
          width: 100; height: 100
          function px(ctx, x, y) { return Array.from(ctx.getImageData(x, y, 1, 1).data).join(",") }
          Component.onCompleted: { loadImage("blue.png"); root.loadingAtStart = isImageLoading("blue.png") }
          onImageLoaded: requestPaint()
          onPaint: {
            var ctx = getContext("2d")
            if (!isImageLoaded("blue.png")) return
            root.loaded = true
            ctx.drawImage("blue.png", 20, 20)
            root.at = px(ctx, 22, 22) + " " + px(ctx, 19, 19)
            ctx.drawImage("blue.png", 0, 0, 2, 2, 60, 0, 8, 8)
            root.stretched = px(ctx, 61, 1) + " " + px(ctx, 67, 7)
          }
        }
      }
      """
    When 300 milliseconds pass
    Then the property "loadingAtStart" of "root" is true
    And the property "loaded" of "root" is true
    And the property "at" of "root" is "0,0,255,255 0,0,0,0"
    And the property "stretched" of "root" is "0,0,255,255 0,0,255,255"
    And the pixel at 22, 22 of the window is "#0000ff"

  Scenario: Images load in the background and each says imageLoaded once, a missing one too
    # An image being loaded draws nothing.
    Given a 4 by 4 image file "blue.png" filled with "#0000ff"
    And a 4 by 4 image file "red.png" filled with "#ff0000"
    And the QML:
      """qml
      import QtQuick
      Canvas {
        objectName: "canvas"
        width: 20; height: 20
        property string atStart: ""; property string atEnd: ""; property string whileLoading: ""
        property string reloaded: ""; property int loads: 0
        function st(u) { return [isImageLoading(u), isImageLoaded(u), isImageError(u)].join("/") }
        function again() {
          unloadImage("blue.png"); reloaded = st("blue.png")
          loadImage("blue.png"); reloaded += " " + st("blue.png")
          unloadImage("missing.png"); loadImage("missing.png"); reloaded += " " + st("missing.png")
        }
        Component.onCompleted: {
          loadImage("blue.png"); loadImage("missing.png"); loadImage("blue.png")
          atStart = [st("blue.png"), st("missing.png"), st("other.png")].join(" ")
        }
        onImageLoaded: { loads++; atEnd = st("blue.png") + " " + st("missing.png"); requestPaint() }
        onPaint: {
          if (whileLoading !== "") return
          var ctx = getContext("2d")
          loadImage("red.png"); ctx.drawImage("red.png", 0, 0)
          whileLoading = st("red.png") + " " + ctx.getImageData(1, 1, 1, 1).data[3]
        }
      }
      """
    When 300 milliseconds pass
    Then the property "atStart" of "canvas" is "true/false/false true/false/false false/false/false"
    And the property "whileLoading" of "canvas" is "true/false/false 0"
    And the property "loads" of "canvas" is 3
    And the property "atEnd" of "canvas" is "false/true/false false/false/true"
    # Loaded again from the cache: at once and without a signal. A failure is not kept.
    When the method "again" of "canvas" is called
    Then the property "reloaded" of "canvas" is "false/false/false false/true/false true/false/false"
    When 300 milliseconds pass
    Then the property "loads" of "canvas" is 4
    And the property "atEnd" of "canvas" is "false/true/false false/false/true"

  Scenario: The render target and strategy name their kinds
    Given the QML:
      """qml
      import QtQuick
      Canvas {
        objectName: "canvas"
        property string targets: [Canvas.Image, Canvas.FramebufferObject].join(",")
        property string strategies: [Canvas.Immediate, Canvas.Threaded, Canvas.Cooperative].join(",")
        renderTarget: Canvas.Image
        renderStrategy: Canvas.Cooperative
      }
      """
    Then the property "targets" of "canvas" is "0,1"
    And the property "strategies" of "canvas" is "0,1,2"
    And the property "renderTarget" of "canvas" is 0
    And the property "renderStrategy" of "canvas" is 2
