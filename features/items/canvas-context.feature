# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html
#   https://doc.qt.io/qt-6/qml-qtquick-canvasgradient.html
#   https://doc.qt.io/qt-6/qml-qtquick-canvasimagedata.html
#   https://doc.qt.io/qt-6/qml-qtquick-textmetrics.html
# Verified on Qt 6.11.2 (qml6, offscreen). Each scenario reads the canvas back with getImageData,
# which gives straight (not premultiplied) red, green, blue and alpha.

@qt-parity
Feature: Drawing on a Canvas with its 2D context
  The context keeps its state between paints: styles, line settings, the transform, the path and
  the clip. Paths take the transform that is current when each point is added.

  Background:
    Given the file "Board.qml":
      """qml
      import QtQuick
      Canvas {
        width: 100; height: 100
        property var ctx: null
        signal draw()
        function px(x, y) { return Array.from(ctx.getImageData(x, y, 1, 1).data).join(",") }
        function red(x, y) { return ctx.getImageData(x, y, 1, 1).data[0] }
        function firstInkRow(x0, x1) {
          var d = ctx.getImageData(0, 0, 100, 100).data
          for (var y = 0; y < 100; y++)
            for (var x = x0; x < x1; x++)
              if (d[(y * 100 + x) * 4 + 3] > 0) return y
          return -1
        }
        function lastInkColumn(y0, y1) {
          var d = ctx.getImageData(0, 0, 100, 100).data
          for (var x = 99; x >= 0; x--)
            for (var y = y0; y < y1; y++)
              if (d[(y * 100 + x) * 4 + 3] > 0) return x
          return -1
        }
        onPaint: { ctx = getContext("2d"); draw() }
      }
      """

  Scenario: A new context starts from the documented defaults
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string defaults: ""
        onDraw: defaults = [ctx.fillStyle, ctx.strokeStyle, ctx.lineWidth, ctx.lineCap, ctx.lineJoin,
            ctx.miterLimit, ctx.globalAlpha, ctx.globalCompositeOperation, ctx.textAlign,
            ctx.textBaseline, ctx.shadowBlur, ctx.getLineDash().length, ctx.lineDashOffset].join("|")
      }
      """
    When 200 milliseconds pass
    Then the property "defaults" of "board" is "#000000|#000000|1|butt|miter|10|1|source-over|start|alphabetic|0|0|0"

  Scenario: Colours read back in a normal form and bad values are ignored
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          var s = []
          ctx.fillStyle = "red"; s.push(ctx.fillStyle)
          ctx.fillStyle = "rgba(0,255,0,0.5)"; s.push(ctx.fillStyle)
          ctx.strokeStyle = Qt.rgba(0, 0, 1, 1); s.push(ctx.strokeStyle)
          ctx.lineWidth = 3; ctx.lineWidth = -1; s.push(ctx.lineWidth)
          ctx.lineCap = "round"; ctx.lineCap = "bogus"; s.push(ctx.lineCap)
          ctx.globalAlpha = 0.5; ctx.globalAlpha = 2; s.push(ctx.globalAlpha)
          ctx.globalCompositeOperation = "copy"; ctx.globalCompositeOperation = "bogus"
          s.push(ctx.globalCompositeOperation)
          seen = s
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is ["#ff0000", "rgba(0, 255, 0, 0.501961)", "#0000ff", 3, "round", 0.5, "copy"]

  Scenario: A path takes the transform current when its points were added
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string seen: ""
        onDraw: {
          ctx.fillStyle = "red"
          ctx.beginPath(); ctx.rect(0, 0, 10, 10); ctx.translate(20, 0); ctx.fill()
          seen = px(5, 5) + " " + px(25, 5)
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is "255,0,0,255 0,0,0,0"

  Scenario: Transparency, clearing and clipping
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          var s = []
          ctx.fillStyle = "red"; ctx.fillRect(0, 0, 10, 10); ctx.clearRect(0, 0, 5, 10)
          s.push(px(2, 5), px(7, 5))
          ctx.fillStyle = "rgba(0,0,255,0.5)"; ctx.fillRect(0, 20, 10, 10)
          s.push(px(5, 25))
          ctx.globalAlpha = 0.5; ctx.fillStyle = "#00ff00"; ctx.fillRect(20, 20, 10, 10); ctx.globalAlpha = 1
          s.push(px(25, 25))
          ctx.save(); ctx.beginPath(); ctx.rect(40, 0, 5, 10); ctx.clip()
          ctx.fillStyle = "red"; ctx.fillRect(40, 0, 10, 10); ctx.restore()
          s.push(px(42, 5), px(47, 5))
          ctx.fillRect(60, 0, 10, 10)
          s.push(px(67, 5))
          seen = s
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is ["0,0,0,0", "255,0,0,255", "0,0,255,128", "0,255,0,127", "255,0,0,255", "0,0,0,0", "0,255,0,255"]

  Scenario: A composition that removes changes only what lies under the shape
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          ctx.fillStyle = "#00ff00"; ctx.fillRect(0, 0, 100, 100)
          ctx.globalCompositeOperation = "destination-out"
          ctx.fillStyle = "black"; ctx.fillRect(0, 0, 10, 10)
          ctx.globalCompositeOperation = "copy"
          ctx.fillStyle = "rgba(0,0,255,0.5)"; ctx.fillRect(50, 50, 10, 10)
          ctx.globalCompositeOperation = "source-over"
          seen = [px(5, 5), px(15, 5), px(55, 55), px(75, 75)]
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is ["0,0,0,0", "0,255,0,255", "0,0,255,128", "0,255,0,255"]

  Scenario: An arc runs clockwise on the screen unless told otherwise
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          ctx.fillStyle = "red"
          ctx.beginPath(); ctx.moveTo(25, 25); ctx.arc(25, 25, 20, 0, Math.PI / 2, false); ctx.closePath(); ctx.fill()
          ctx.beginPath(); ctx.moveTo(75, 25); ctx.arc(75, 25, 20, 0, Math.PI / 2, true); ctx.closePath(); ctx.fill()
          seen = [px(33, 33), px(33, 17), px(83, 33), px(83, 17), px(67, 17)]
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is ["255,0,0,255", "0,0,0,0", "0,0,0,0", "255,0,0,255", "255,0,0,255"]

  Scenario: Line width, caps and dashes
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          ctx.strokeStyle = "blue"; ctx.lineWidth = 4
          ctx.beginPath(); ctx.moveTo(10, 10); ctx.lineTo(90, 10); ctx.stroke()
          ctx.lineCap = "square"
          ctx.beginPath(); ctx.moveTo(10, 30); ctx.lineTo(90, 30); ctx.stroke()
          ctx.lineCap = "butt"; ctx.lineWidth = 2; ctx.setLineDash([10, 10])
          ctx.beginPath(); ctx.moveTo(0, 50); ctx.lineTo(100, 50); ctx.stroke()
          seen = [px(50, 9), px(50, 11), px(50, 7), px(9, 10), px(9, 30), px(91, 30),
                  px(5, 50), px(15, 50), px(25, 50), JSON.stringify(ctx.getLineDash())]
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is ["0,0,255,255", "0,0,255,255", "0,0,0,0", "0,0,0,0", "0,0,255,255", "0,0,255,255", "0,0,255,255", "0,0,255,255", "0,0,0,0", "[10,10]"]

  Scenario: Transforms move what is drawn after them
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          ctx.fillStyle = "red"
          ctx.translate(50, 50); ctx.rotate(Math.PI / 2); ctx.fillRect(0, 0, 20, 5)
          var s = [px(48, 60), px(60, 52)]
          ctx.setTransform(2, 0, 0, 2, 10, 10); ctx.fillRect(0, 0, 5, 5)
          s.push(px(19, 19), px(21, 21))
          ctx.resetTransform(); ctx.scale(2, 1); ctx.fillRect(40, 80, 5, 5)
          s.push(px(85, 82), px(91, 82))
          seen = s
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is ["255,0,0,255", "0,0,0,0", "255,0,0,255", "0,0,0,0", "255,0,0,255", "0,0,0,0"]

  Scenario: Linear, radial and conical gradients
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property int linearStart: -1
        property int linearMiddle: -1
        property int radialCentre: -1
        property int radialHalf: -1
        property int conicalBelow: -1
        property int conicalAbove: -1
        property int conicalTop: -1
        onDraw: {
          var g = ctx.createLinearGradient(0, 0, 100, 0)
          g.addColorStop(0, "black"); g.addColorStop(1, "white")
          ctx.fillStyle = g; ctx.fillRect(0, 0, 100, 10)
          linearStart = red(0, 5); linearMiddle = red(50, 5)
          var r = ctx.createRadialGradient(50, 50, 0, 50, 50, 50)
          r.addColorStop(0, "red"); r.addColorStop(1, "blue")
          ctx.fillStyle = r; ctx.fillRect(0, 20, 100, 80)
          radialCentre = red(50, 50); radialHalf = red(50, 75)
          var c = ctx.createConicalGradient(50, 50, 0)
          c.addColorStop(0, "red"); c.addColorStop(1, "blue")
          ctx.fillStyle = c; ctx.fillRect(0, 20, 100, 80)
          conicalBelow = red(90, 51); conicalAbove = red(90, 49); conicalTop = red(50, 30)
        }
      }
      """
    When 200 milliseconds pass
    Then the property "linearStart" of "board" is within 3 of 1
    And the property "linearMiddle" of "board" is within 3 of 129
    And the property "radialCentre" of "board" is within 6 of 252
    And the property "radialHalf" of "board" is within 6 of 125
    And the property "conicalBelow" of "board" is within 6 of 1
    And the property "conicalAbove" of "board" is within 6 of 255
    And the property "conicalTop" of "board" is within 6 of 192

  Scenario: Ellipses, rounded rectangles and arcs between lines
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          ctx.fillStyle = "red"
          ctx.beginPath(); ctx.ellipse(0, 0, 40, 20); ctx.fill()
          var s = [px(20, 10), px(2, 2)]
          ctx.beginPath(); ctx.roundedRect(50, 0, 40, 40, 10, 10); ctx.fill()
          s.push(px(51, 1), px(55, 5), px(70, 1))
          ctx.strokeStyle = "blue"; ctx.lineWidth = 2
          ctx.beginPath(); ctx.moveTo(0, 60); ctx.lineTo(40, 60); ctx.arcTo(80, 60, 80, 100, 20); ctx.lineTo(80, 100); ctx.stroke()
          s.push(px(50, 60), px(80, 90), px(78, 62))
          seen = s
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is ["255,0,0,255", "0,0,0,0", "0,0,0,0", "255,0,0,255", "255,0,0,255", "0,0,255,255", "0,0,255,255", "0,0,0,0"]

  Scenario: isPointInPath asks in canvas pixels
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          ctx.beginPath(); ctx.translate(50, 0); ctx.rect(0, 0, 10, 10)
          seen = [ctx.isPointInPath(5, 5), ctx.isPointInPath(55, 5)]
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is [false, true]

  Scenario: Image data has four bytes a pixel and is empty outside the canvas
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property var seen: []
        onDraw: {
          ctx.fillStyle = "red"; ctx.fillRect(0, 0, 100, 100)
          var made = ctx.createImageData(2, 3)
          var read = ctx.getImageData(98, 98, 4, 4)
          var copy = ctx.createImageData(read)
          seen = [made.width, made.height, made.data.length, made.data[0],
                  read.width, read.data.length, read.data[3], read.data[4 * 3 + 3],
                  copy.width, copy.height]
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is [2, 3, 24, 0, 4, 64, 255, 0, 4, 4]

  Scenario: A solid pattern fills like its colour
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string seen: ""
        onDraw: {
          ctx.fillStyle = ctx.createPattern("red", Qt.SolidPattern)
          ctx.fillRect(0, 0, 10, 10)
          seen = px(5, 5)
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "board" is "255,0,0,255"

  Scenario: Text is measured and placed by its alignment and baseline
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property bool wider: false
        property bool doubled: false
        property bool topIsLower: false
        property bool rightEndsBefore: false
        onDraw: {
          ctx.font = "20px sans-serif"
          var m = ctx.measureText("M").width
          wider = m > 5 && ctx.measureText("MMMM").width > m
          doubled = Math.abs(ctx.measureText("MM").width - 2 * m) <= 1
          ctx.fillText("M", 0, 30)
          var alphabetic = firstInkRow(0, 30)
          ctx.textBaseline = "top"; ctx.fillText("M", 40, 30)
          topIsLower = firstInkRow(40, 70) > alphabetic + 5
          ctx.clearRect(0, 0, 100, 100); ctx.textBaseline = "alphabetic"
          ctx.textAlign = "right"; ctx.fillText("M", 50, 80)
          var last = lastInkColumn(60, 85)
          rightEndsBefore = last >= 40 && last <= 50
        }
      }
      """
    When 200 milliseconds pass
    Then the property "wider" of "board" is true
    And the property "doubled" of "board" is true
    And the property "topIsLower" of "board" is true
    And the property "rightEndsBefore" of "board" is true

  Scenario: Drawing methods return the context so calls can be chained
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property bool chained: false
        onDraw: chained = ctx.fillRect(0, 0, 1, 1) === ctx && ctx.beginPath() === ctx && ctx.save() === ctx
      }
      """
    When 200 milliseconds pass
    Then the property "chained" of "board" is true
