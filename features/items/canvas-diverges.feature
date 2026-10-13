# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-context2d.html#putImageData-method
#   https://html.spec.whatwg.org/multipage/canvas.html#dom-context-2d-putimagedata
#
# putImageData: the Qt documentation says it paints the data into the canvas at the place given.
# Qt 6.11.2 (qml6, offscreen) leaves the canvas as it was, both for data read back with
# getImageData and for data made with createImageData. Tried, all with the same result: reading
# back in the same onPaint, in onPainted, and in the next paint after requestPaint(); the
# window's own picture (grabToImage) after the paint; renderStrategy Immediate, Threaded and
# Cooperative; renderTarget Image and FramebufferObject. Ljós does what the documentation and the
# HTML canvas say: the pixels are replaced, with no composition.

@qt-diverges
Feature: Where a Canvas differs from Qt
  Image data put into a canvas replaces the pixels under it.

  Scenario: putImageData writes the pixels as they are
    Given the QML:
      """qml
      import QtQuick
      Canvas {
        objectName: "canvas"
        width: 40; height: 40
        property var seen: []
        function px(ctx, x, y) { return Array.from(ctx.getImageData(x, y, 1, 1).data).join(",") }
        onPaint: {
          var ctx = getContext("2d")
          ctx.fillStyle = "red"; ctx.fillRect(0, 0, 40, 40)
          var d = ctx.createImageData(2, 2)
          for (var i = 0; i < 16; i += 4) { d.data[i + 2] = 255; d.data[i + 3] = 128 }
          ctx.putImageData(d, 10, 10)
          var s = [px(ctx, 10, 10), px(ctx, 11, 11), px(ctx, 12, 12)]
          ctx.putImageData(ctx.getImageData(0, 0, 4, 4), 20, 20, 0, 0, 2, 2)
          s.push(px(ctx, 21, 21), px(ctx, 23, 23))
          seen = s
        }
      }
      """
    When 200 milliseconds pass
    Then the property "seen" of "canvas" is ["0,0,255,128", "0,0,255,128", "255,0,0,255", "255,0,0,255", "255,0,0,255"]
