# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html#shadowBlur-prop
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html#shadowColor-prop
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html#shadowOffsetX-prop
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html#createPattern-method
#   https://doc.qt.io/qt-6/qt.html#BrushStyle-enum
# Verified on Qt 6.11.2 (qml6, offscreen). Each scenario reads the canvas back with getImageData,
# which gives straight (not premultiplied) red, green, blue and alpha. The documentation says
# little about how a shadow is drawn; what these scenarios fix was measured. Unlike the HTML
# canvas, the default shadow colour is opaque black and the offsets and the blur take the
# transform.

@qt-parity
Feature: Shadows and hatchings on a Canvas
  Everything a context fills or strokes, text and images included, casts a shadow when the shadow
  is offset or blurred. createPattern with a colour and a Qt.BrushStyle fills with one of Qt's
  hatchings, an 8 by 8 pixel cell repeated from the origin of the canvas.

  Background:
    Given the file "Board.qml":
      """qml
      import QtQuick
      Canvas {
        width: 100; height: 100
        property var ctx: null
        signal draw()
        function px(x, y) { return Array.from(ctx.getImageData(x, y, 1, 1).data).join(",") }
        function al(x, y) { return ctx.getImageData(x, y, 1, 1).data[3] }
        function row(y, x0, n) { var s = ""; for (var x = x0; x < x0 + n; x++) s += al(x, y) ? "#" : "."; return s }
        function column(x, y0, n) { var s = ""; for (var y = y0; y < y0 + n; y++) s += al(x, y) ? "#" : "."; return s }
        function cell() { var r = []; for (var y = 0; y < 8; y++) r.push(row(y, 0, 8)); return r.join("/") }
        function inked(x0, y0, x1, y1) {
          var n = 0
          for (var y = y0; y < y1; y++) for (var x = x0; x < x1; x++) if (al(x, y) > 0) n++
          return n
        }
        function fresh() { ctx.reset(); ctx.clearRect(0, 0, width, height) }
        onPaint: { ctx = getContext("2d"); draw() }
      }
      """

  Scenario: An offset shadow is a sharp copy of the shape in the shadow colour
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string hard: ""
        property string halfPixel: ""
        onDraw: {
          ctx.shadowColor = "blue"; ctx.shadowOffsetX = 30; ctx.shadowOffsetY = 10
          ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 20)
          hard = [px(15, 15), px(45, 35), px(45, 15), px(61, 35), px(35, 25)].join(" ")
          fresh()
          ctx.shadowColor = "blue"; ctx.shadowOffsetX = 0.5
          ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 20)
          var edge = al(30, 20)
          halfPixel = [px(29, 20), edge > 120 && edge < 135, px(31, 20)].join(" ")
        }
      }
      """
    When 200 milliseconds pass
    Then the property "hard" of "board" is "255,0,0,255 0,0,255,255 0,0,0,0 0,0,0,0 0,0,0,0"
    And the property "halfPixel" of "board" is "255,0,0,255 true 0,0,0,0"

  Scenario: The shadow is opaque black by default and a blur of zero or less is ignored
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string defaults: ""
        property string offsetOnly: ""
        property string blurOnly: ""
        property string kept: ""
        onDraw: {
          defaults = [ctx.shadowColor, ctx.shadowBlur, ctx.shadowOffsetX, ctx.shadowOffsetY].join(" ")
          ctx.shadowOffsetX = 30; ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 20)
          offsetOnly = px(45, 20)
          fresh()
          ctx.shadowBlur = 4; ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 20)
          blurOnly = [px(8, 20).slice(0, 6), al(8, 20) > 0, al(2, 20)].join(" ")
          var seen = []
          ctx.shadowBlur = 0; seen.push(ctx.shadowBlur)
          ctx.shadowBlur = -1; seen.push(ctx.shadowBlur)
          ctx.shadowBlur = NaN; seen.push(ctx.shadowBlur)
          ctx.shadowBlur = 0.5; seen.push(ctx.shadowBlur)
          ctx.shadowOffsetX = 5; ctx.shadowOffsetX = Infinity; seen.push(ctx.shadowOffsetX)
          ctx.reset(); seen.push(ctx.shadowBlur)
          kept = seen.join(" ")
        }
      }
      """
    When 200 milliseconds pass
    Then the property "defaults" of "board" is "#000000 0 0 0"
    And the property "offsetOnly" of "board" is "0,0,0,255"
    And the property "blurOnly" of "board" is "0,0,0, true 0"
    And the property "kept" of "board" is "4 4 4 0.5 5 0"

  Scenario: A shadow takes the alpha of its colour, of globalAlpha and of the shape
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string halfColour: ""
        property string global: ""
        property string halfShape: ""
        onDraw: {
          ctx.shadowColor = Qt.rgba(0, 0, 1, 0.5); ctx.shadowOffsetX = 30
          ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 20)
          halfColour = px(45, 20)
          fresh()
          ctx.shadowColor = "blue"; ctx.shadowOffsetY = 20; ctx.globalAlpha = 0.5
          ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 10)
          global = px(15, 15) + " " + px(15, 35)
          fresh()
          ctx.shadowColor = "blue"; ctx.shadowOffsetY = 20
          ctx.fillStyle = Qt.rgba(1, 0, 0, 0.5); ctx.fillRect(10, 10, 20, 10)
          halfShape = px(15, 15) + " " + px(15, 35)
        }
      }
      """
    When 200 milliseconds pass
    Then the property "halfColour" of "board" is "0,0,255,128"
    And the property "global" of "board" is "255,0,0,127 0,0,255,127"
    And the property "halfShape" of "board" is "255,0,0,128 0,0,255,128"

  Scenario: The offsets go through the transform
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string scaled: ""
        property string rotated: ""
        onDraw: {
          ctx.shadowColor = "blue"; ctx.shadowOffsetX = 10
          ctx.scale(2, 2); ctx.fillStyle = "red"; ctx.fillRect(5, 5, 5, 5)
          scaled = row(15, 8, 36)
          fresh()
          ctx.shadowColor = "blue"; ctx.shadowOffsetX = 10
          ctx.translate(50, 0); ctx.rotate(Math.PI / 2)
          ctx.fillStyle = "red"; ctx.fillRect(10, 10, 10, 10)
          rotated = row(15, 28, 28) + " " + column(35, 8, 26)
        }
      }
      """
    When 200 milliseconds pass
    Then the property "scaled" of "board" is "..##########..........##########...."
    And the property "rotated" of "board" is "..##########................ ..####################...."

  Scenario: Strokes and text cast shadows; clearing casts none
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string stroke: ""
        property bool textCasts: false
        property string cleared: ""
        onDraw: {
          ctx.shadowColor = "blue"; ctx.shadowOffsetY = 20
          ctx.strokeStyle = "red"; ctx.lineWidth = 2
          ctx.beginPath(); ctx.moveTo(10, 10); ctx.lineTo(50, 10); ctx.stroke()
          stroke = [px(20, 10), px(20, 28), px(20, 29), px(20, 30), px(20, 31)].join(" ")
          fresh()
          ctx.font = "bold 30px sans-serif"; ctx.fillStyle = "red"
          ctx.shadowColor = "lime"; ctx.shadowOffsetY = 50
          ctx.fillText("MM", 10, 40)
          textCasts = inked(0, 50, 100, 100) > 100
          fresh()
          ctx.fillStyle = "lime"; ctx.fillRect(0, 0, 100, 100)
          ctx.shadowColor = "blue"; ctx.shadowOffsetX = 30; ctx.clearRect(10, 10, 20, 20)
          cleared = px(45, 20) + " " + px(20, 20)
        }
      }
      """
    When 200 milliseconds pass
    Then the property "stroke" of "board" is "255,0,0,255 0,0,0,0 0,0,255,255 0,0,255,255 0,0,0,0"
    And the property "textCasts" of "board" is true
    And the property "cleared" of "board" is "0,255,0,255 0,0,0,0"

  Scenario: The clip cuts the shadow; copy leaves the canvas beyond shape and shadow alone
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string clipped: ""
        property string copied: ""
        onDraw: {
          ctx.shadowColor = "blue"; ctx.shadowOffsetX = 30
          ctx.beginPath(); ctx.rect(0, 0, 50, 100); ctx.clip()
          ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 20)
          clipped = px(45, 20) + " " + px(55, 20)
          fresh()
          ctx.fillStyle = "lime"; ctx.fillRect(0, 0, 100, 100)
          ctx.shadowColor = "blue"; ctx.shadowOffsetX = 30; ctx.globalCompositeOperation = "copy"
          ctx.fillStyle = "red"; ctx.fillRect(10, 10, 20, 20)
          copied = [px(15, 15), px(45, 20), px(80, 80), px(35, 50)].join(" ")
        }
      }
      """
    When 200 milliseconds pass
    Then the property "clipped" of "board" is "0,0,255,255 0,0,0,0"
    And the property "copied" of "board" is "255,0,0,255 0,0,255,255 0,255,0,255 0,255,0,255"

  Scenario: An image casts a shadow
    Given a 4 by 4 image file "blue.png" filled with "#0000ff"
    And the QML:
      """qml
      import QtQuick
      Canvas {
        objectName: "canvas"
        width: 100; height: 100
        property string drawn: ""
        function px(ctx, x, y) { return Array.from(ctx.getImageData(x, y, 1, 1).data).join(",") }
        Component.onCompleted: loadImage("blue.png")
        onImageLoaded: requestPaint()
        onPaint: {
          if (!isImageLoaded("blue.png")) return
          var ctx = getContext("2d")
          ctx.shadowColor = "red"; ctx.shadowOffsetX = 30
          ctx.drawImage("blue.png", 0, 0, 4, 4, 10, 10, 20, 20)
          drawn = px(ctx, 20, 20) + " " + px(ctx, 45, 20) + " " + px(ctx, 35, 20)
        }
      }
      """
    When 300 milliseconds pass
    Then the property "drawn" of "canvas" is "0,0,255,255 255,0,0,255 0,0,0,0"

  Scenario: A blurred shadow fades out across the edge of the shape
    # Alpha just outside a straight edge at x = 100 (at 50 when scaled), within 6 of qml6's.
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        width: 200; height: 40
        property string misses: "not drawn"
        function edge(scale, blur, edgeX, want) {
          fresh()
          ctx.scale(scale, scale); ctx.shadowColor = "blue"; ctx.shadowBlur = blur
          ctx.fillStyle = "red"; ctx.fillRect(edgeX / scale, -100, 200, 300)
          var bad = [], from = edgeX - want.length
          for (var i = 0; i < want.length; i++)
            if (Math.abs(al(from + i, 20) - want[i]) > 6) bad.push(blur + "@" + (from + i) + "=" + al(from + i, 20))
          return bad
        }
        onDraw: misses = [].concat(edge(1, 2, 100, [0, 0, 9, 38, 94]),
            edge(1, 4, 100, [0, 0, 2, 8, 20, 41, 71, 108]),
            edge(1, 10, 100, [0, 1, 2, 4, 7, 11, 16, 23, 32, 42, 55, 69, 85, 102, 119]),
            edge(1, 20, 100, [0, 1, 1, 2, 2, 3, 5, 6, 8, 10, 13, 15, 19, 23, 27, 31, 37, 42, 49, 56, 63, 71, 79, 87, 96, 105, 114, 123]),
            edge(2, 4, 50, [0, 0, 1, 3, 6, 10, 16, 25, 35, 48, 63, 80, 98, 117])).join(" ")
      }
      """
    When 200 milliseconds pass
    Then the property "misses" of "board" is ""

  Scenario Outline: A hatching fills its 8 by 8 cell
    Given the QML:
      """qml
      import QtQuick
      Board {
        id: board
        objectName: "board"
        property string dense1; property string dense2; property string dense3; property string dense4
        property string dense5; property string dense6; property string dense7; property string hor
        property string ver; property string cross; property string bDiag; property string fDiag
        property string diagCross
        onDraw: {
          var names = ["dense1", "dense2", "dense3", "dense4", "dense5", "dense6", "dense7",
                       "hor", "ver", "cross", "bDiag", "fDiag", "diagCross"]
          for (var k = 0; k < names.length; k++) {
            fresh()
            ctx.fillStyle = ctx.createPattern("red", Qt.Dense1Pattern + k)
            ctx.fillRect(0, 0, 8, 8)
            board[names[k]] = cell()
          }
        }
      }
      """
    When 200 milliseconds pass
    Then the property "<style>" of "board" is "<cell>"

    Examples:
      | style     | cell                                                                    |
      | dense1    | ########/##.###.#/########/########/########/##.###.#/########/######## |
      | dense2    | ###.###./########/#.###.##/########/###.###./########/#.###.##/######## |
      | dense3    | #.#.#.#./##.###.#/#.#.#.#./.###.###/#.#.#.#./##.###.#/#.#.#.#./.###.### |
      | dense4    | .#.#.#.#/#.#.#.#./.#.#.#.#/#.#.#.#./.#.#.#.#/#.#.#.#./.#.#.#.#/#.#.#.#. |
      | dense5    | .#.#.#.#/..#...#./.#.#.#.#/#...#.../.#.#.#.#/..#...#./.#.#.#.#/#...#... |
      | dense6    | ...#...#/......../.#...#../......../...#...#/......../.#...#../........ |
      | dense7    | ......../..#...#./......../......../......../..#...#./......../........ |
      | hor       | ......../......../......../########/......../......../......../........ |
      | ver       | ....#.../....#.../....#.../....#.../....#.../....#.../....#.../....#... |
      | cross     | ....#.../....#.../....#.../########/....#.../....#.../....#.../....#... |
      | bDiag     | .......#/......#./.....#../....#.../...#..../..#...../.#....../#....... |
      | fDiag     | #......./.#....../..#...../...#..../....#.../.....#../......#./.......# |
      | diagCross | #......#/.#....#./..#..#../...##.../...##.../..#..#../.#....#./#......# |

  Scenario: A hatching keeps its pixel size and follows only the translation
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string translated: ""
        property string scaled: ""
        property string offset: ""
        property string stroked: ""
        onDraw: {
          ctx.translate(1, 0); ctx.fillStyle = ctx.createPattern("lime", Qt.VerPattern)
          ctx.fillRect(0, 0, 12, 2)
          translated = row(0, 0, 12)
          fresh()
          ctx.scale(2, 2); ctx.fillStyle = ctx.createPattern("lime", Qt.HorPattern)
          ctx.fillRect(0, 0, 20, 20)
          scaled = column(1, 0, 20)
          fresh()
          ctx.fillStyle = ctx.createPattern("lime", Qt.Dense4Pattern); ctx.fillRect(3, 5, 8, 4)
          offset = [5, 6, 7, 8].map(y => row(y, 0, 12)).join("|")
          fresh()
          ctx.strokeStyle = ctx.createPattern("blue", Qt.HorPattern); ctx.lineWidth = 4
          ctx.beginPath(); ctx.moveTo(0, 10); ctx.lineTo(20, 10); ctx.stroke()
          stroked = column(0, 0, 20)
        }
      }
      """
    When 200 milliseconds pass
    Then the property "translated" of "board" is ".....#......"
    And the property "scaled" of "board" is "...#.......#.......#"
    And the property "offset" of "board" is "....#.#.#.#.|...#.#.#.#..|....#.#.#.#.|...#.#.#.#.."
    And the property "stroked" of "board" is "...........#........"

  Scenario: A hatching takes alpha and casts a hatched shadow; other styles fill or draw nothing
    Given the QML:
      """qml
      import QtQuick
      Board {
        objectName: "board"
        property string alphas: ""
        property string shadow: ""
        property string others: ""
        onDraw: {
          ctx.fillStyle = ctx.createPattern(Qt.rgba(0, 0, 1, 0.5), Qt.HorPattern); ctx.fillRect(0, 0, 10, 10)
          alphas = px(0, 3)
          fresh()
          ctx.fillStyle = ctx.createPattern("lime", Qt.HorPattern); ctx.globalAlpha = 0.5
          ctx.fillRect(0, 0, 20, 20)
          alphas += " " + px(3, 3) + " " + px(3, 4)
          fresh()
          ctx.fillStyle = ctx.createPattern("lime", Qt.HorPattern)
          ctx.shadowColor = "red"; ctx.shadowOffsetX = 30; ctx.fillRect(0, 0, 20, 20)
          shadow = px(32, 3) + " " + px(32, 2)
          var seen = []
          for (var s of [Qt.NoBrush, Qt.SolidPattern, 15, 24, 99]) {
            fresh(); ctx.fillStyle = ctx.createPattern("lime", s); ctx.fillRect(0, 0, 20, 20)
            seen.push(px(4, 3))
          }
          others = seen.join(" ")
        }
      }
      """
    When 200 milliseconds pass
    Then the property "alphas" of "board" is "0,0,255,128 0,255,0,127 0,0,0,0"
    And the property "shadow" of "board" is "255,0,0,255 0,0,0,0"
    And the property "others" of "board" is "0,0,0,0 0,255,0,255 0,255,0,255 0,255,0,255 0,255,0,255"
