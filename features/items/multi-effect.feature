# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-effects-multieffect.html
#   https://doc.qt.io/qt-6/qml-qt5compat-graphicaleffects-brightnesscontrast.html
# Verified on Qt 6.11.2 (qml6, offscreen). Qt's software scene graph,
# which the Qt backend uses, draws no MultiEffect at all (measured:
# every effect pixel is the background), so what Ljós paints is pinned
# on Ljós only, from the mask model the documentation describes.
# The documentation gives ranges, defaults and padding rules but describes
# the colour adjustments, blur and shadow qualitatively; the exact numbers
# below follow the formulations named in crates/ljos_quick/src/effects.rs,
# worked by hand in each comment. Saturation's and colorization's were
# measured on qml6 drawing with OpenGL in the desktop:cua sandbox (a
# grabToImage of colour swatches).

Feature: MultiEffect
  A MultiEffect draws its source item with post-processing applied:
  colour adjustments (brightness, contrast, saturation, colorization),
  alpha masking, separable Gaussian blur, and drop shadow inside an
  automatically or manually padded rectangle.

  Background:
    Given a 40 by 40 image file "red.png" filled with "#ff0000"
    And a 40 by 40 image file "half.png" filled with "#80ffffff"
    And a 40 by 40 image file "clear.png" filled with "#00ffffff"
    And the QML:
      """qml
      import QtQuick
      import QtQuick.Effects
      Rectangle {
        objectName: "root"
        width: 200; height: 100
        color: "#000000"
        Image { id: red; objectName: "red"; source: "red.png"; x: 0; y: 50 }
        Image { id: half; source: "half.png"; x: 50; y: 50 }
        Image { id: clear; source: "clear.png"; x: 100; y: 50 }
        MultiEffect { id: fx; objectName: "fx"; source: red; width: 40; height: 40 }
        MultiEffect {
          objectName: "maskedHalf"; source: red; maskSource: half; maskEnabled: true
          width: 40; height: 40; x: 50
        }
        MultiEffect {
          objectName: "maskedClear"; source: red; maskSource: clear; maskEnabled: true
          width: 40; height: 40; x: 100
        }
        MultiEffect {
          objectName: "belowMin"; source: red; maskSource: half; maskEnabled: true
          maskThresholdMin: 0.6; width: 40; height: 40; x: 150
        }
      }
      """

  @qt-parity
  Rule: Properties

    Scenario Outline: Defaults
      Then the property "<property>" of "fx" is <value>

      Examples:
        | property               | value     |
        | brightness             | 0         |
        | contrast               | 0         |
        | saturation             | 0         |
        | colorization           | 0         |
        | colorizationColor      | "#ff0000" |
        | blurEnabled            | false     |
        | blur                   | 0         |
        | blurMax                | 32        |
        | blurMultiplier         | 0         |
        | shadowEnabled          | false     |
        | shadowBlur             | 1         |
        | shadowOpacity          | 1         |
        | shadowColor            | "#000000" |
        | shadowScale            | 1         |
        | shadowHorizontalOffset | 0         |
        | shadowVerticalOffset   | 0         |
        | maskEnabled            | false     |
        | maskInverted           | false     |
        | maskThresholdMin       | 0         |
        | maskThresholdMax       | 1         |
        | maskSpreadAtMin        | 0         |
        | maskSpreadAtMax        | 0         |
        | autoPaddingEnabled     | true             |
        | paddingRect.x          | 0                |
        | paddingRect.y          | 0                |
        | paddingRect.width      | 0                |
        | paddingRect.height     | 0                |
        | itemRect.x             | 0                |
        | itemRect.y             | 0                |
        | itemRect.width         | 40               |
        | itemRect.height        | 40               |
        | hasProxySource         | false            |
        | fragmentShader         | "multieffect_c0" |
        | vertexShader           | "multieffect_c"  |
        | implicitWidth          | 0                |
        | implicitHeight         | 0                |

    Scenario: itemRect grows with blur, shadow, blurMultiplier and paddingRect
      When the property "blurEnabled" of "fx" is set to true
      Then the property "itemRect.x" of "fx" is -32
      And the property "itemRect.y" of "fx" is -32
      And the property "itemRect.width" of "fx" is 104
      And the property "itemRect.height" of "fx" is 104
      When the property "blurMultiplier" of "fx" is set to 0.5
      Then the property "itemRect.x" of "fx" is -48
      And the property "itemRect.width" of "fx" is 136
      When the property "autoPaddingEnabled" of "fx" is set to false
      And the property "paddingRect" of "fx" is bound to "Qt.rect(5, 10, 15, 20)"
      Then the property "itemRect.x" of "fx" is -5
      And the property "itemRect.y" of "fx" is -10
      And the property "itemRect.width" of "fx" is 60
      And the property "itemRect.height" of "fx" is 70


  @ljos-only
  Rule: What it paints

    Scenario: The source is drawn unchanged when no effect is enabled
      When 200 milliseconds pass
      Then the pixel at 20, 20 of the window is "#ff0000"

    # "MultiEffect type renders a new visual item alongside the source
    # item ... both the items can be visible": hiding the source is up to
    # the user (visible: false or opacity: 0).
    Scenario: The source item is still drawn in its own place
      When 200 milliseconds pass
      Then the pixel at 20, 70 of the window is "#ff0000"

    Scenario: The source's children are drawn with it
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Effects
        Rectangle {
          width: 100; height: 60
          color: "#000000"
          Rectangle {
            id: card
            width: 40; height: 40; color: "#0000ff"
            Rectangle { x: 10; y: 10; width: 20; height: 20; color: "#ff0000" }
          }
          MultiEffect { source: card; x: 50; width: 40; height: 40 }
        }
        """
      When 200 milliseconds pass
      Then the pixel at 70, 20 of the window is "#ff0000"
      And the pixel at 55, 5 of the window is "#0000ff"

    Scenario: A hidden source is drawn by its effect, children and all
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Effects
        Rectangle {
          width: 100; height: 60
          color: "#000000"
          Rectangle {
            id: card
            visible: false
            width: 40; height: 40; color: "#0000ff"
            Rectangle { x: 10; y: 10; width: 20; height: 20; color: "#ff0000" }
          }
          MultiEffect { source: card; x: 50; width: 40; height: 40 }
        }
        """
      When 200 milliseconds pass
      Then the pixel at 20, 20 of the window is "#000000"
      And the pixel at 70, 20 of the window is "#ff0000"
      And the pixel at 55, 5 of the window is "#0000ff"

    Scenario: A half-transparent mask halves the source
      When 200 milliseconds pass
      Then the pixel at 70, 20 of the window is within 4 of "#800000"

    Scenario: A clear mask hides the source
      When 200 milliseconds pass
      Then the pixel at 120, 20 of the window is "#000000"

    Scenario: A mask alpha below the minimum threshold hides the source
      When 200 milliseconds pass
      Then the pixel at 170, 20 of the window is "#000000"

    Scenario: A mask alpha above the maximum threshold hides the source
      Given the QML:
        """qml
        import QtQuick
        import QtQuick.Effects
        Rectangle {
          width: 60; height: 60
          color: "#000000"
          Image { id: red; source: "red.png"; visible: false }
          Image { id: half; source: "half.png"; visible: false }
          MultiEffect {
            source: red
            maskSource: half
            maskEnabled: true
            maskThresholdMax: 0.4
            width: 40; height: 40; x: 0; y: 0
          }
        }
        """
      When 200 milliseconds pass
      Then the pixel at 20, 20 of the window is "#000000"

    # Formula (documented range [-1, 1]; additive shift chosen):
    #   c' = clamp(c + brightness, 0, 1) for c in (r, g, b).
    # Source #ff0000 = (1, 0, 0):
    #   brightness = -0.5 -> (0.5, 0, 0) -> round(0.5 * 255) = 128 = #800000
    #   brightness = +0.5 -> (1.0, 0.5, 0.5) -> (255, 128, 128) = #ff8080
    Scenario: Brightness shifts each colour channel linearly
      When 200 milliseconds pass
      And the property "brightness" of "fx" is set to -0.5
      Then the pixel at 20, 20 of the window is "#800000"
      When the property "brightness" of "fx" is set to 0.5
      Then the pixel at 20, 20 of the window is "#ff8080"

    # Formula (BrightnessContrast docs: linear decrease, non-linear increase;
    # the 1 / (1 - contrast) curve is the one Ljós picks):
    #   k = 1 + contrast           for contrast <= 0 (linear decrease)
    #   k = 1 / (1 - contrast)     for contrast > 0  (non-linear increase)
    #   c' = clamp((c - 0.5) * k + 0.5, 0, 1)
    # Source #ff0000 = (1, 0, 0):
    #   contrast = -0.5 (k = 0.5):
    #     r' = (1 - 0.5)*0.5 + 0.5 = 0.75 -> round(0.75 * 255) = 191 = 0xbf
    #     g' = b' = (0 - 0.5)*0.5 + 0.5 = 0.25 -> round(0.25 * 255) = 64 = 0x40
    #     -> #bf4040
    #   contrast = -1.0 (k = 0):
    #     r' = g' = b' = 0.5 -> round(0.5 * 255) = 128 = 0x80 -> #808080
    #   brightness = -0.375 then contrast = +0.5 (k = 1 / (1 - 0.5) = 2):
    #     after brightness: r = 1 - 0.375 = 0.625, g = b = 0
    #     after contrast:   r' = (0.625 - 0.5)*2 + 0.5 = 0.75 -> 191 = 0xbf
    #                       g' = b' = clamp((0 - 0.5)*2 + 0.5, 0, 1) = 0 -> 0x00
    #     -> #bf0000 (also verifies brightness is applied before contrast).
    Scenario: Contrast scales channels around the midpoint
      When 200 milliseconds pass
      And the property "contrast" of "fx" is set to -0.5
      Then the pixel at 20, 20 of the window is "#bf4040"
      When the property "contrast" of "fx" is set to -1
      Then the pixel at 20, 20 of the window is "#808080"
      When the property "brightness" of "fx" is set to -0.375
      And the property "contrast" of "fx" is set to 0.5
      Then the pixel at 20, 20 of the window is "#bf0000"

    # Formula (documented: -1 is "totally desaturated", no upper bound;
    # measured on qml6 drawing with OpenGL, under desktop:cua: chroma
    # scaling around the Rec. 601 luma of the sRGB channels):
    #   Y  = 0.299 * r + 0.587 * g + 0.114 * b
    #   c' = clamp(Y + (c - Y) * (1 + saturation), 0, 1)
    # Source #ff0000 = (1, 0, 0) -> Y = 0.299:
    #   saturation = -1.0 (full desaturation):
    #     r' = g' = b' = 0.299 -> round(76.245) = 76 = 0x4c -> #4c4c4c
    #   saturation = -0.5:
    #     r' = 0.299 + (1 - 0.299)*0.5 = 0.6495 -> round(165.62) = 166 = 0xa6
    #     g' = b' = 0.299 * 0.5 = 0.1495 -> round(38.12) = 38 = 0x26
    #     -> #a62626
    #   brightness = 0.2 then saturation = 1.0 (factor = 2):
    #     after brightness: (r, g, b) = (1.0, 0.2, 0.2)
    #     Y = 0.299*1.0 + 0.587*0.2 + 0.114*0.2 = 0.4392
    #     g' = b' = clamp(0.4392 + (0.2 - 0.4392)*2, 0, 1) = 0 -> 0x00
    #     r' = clamp(0.4392 + (1.0 - 0.4392)*2, 0, 1) = 1.0 -> 0xff
    #     -> #ff0000 (#ff3333 had saturation come first)
    Scenario: Saturation scales chroma around Rec. 601 luma
      When 200 milliseconds pass
      And the property "saturation" of "fx" is set to -1
      Then the pixel at 20, 20 of the window is "#4c4c4c"
      When the property "saturation" of "fx" is set to -0.5
      Then the pixel at 20, 20 of the window is "#a62626"
      When the property "brightness" of "fx" is set to 0.2
      And the property "saturation" of "fx" is set to 1
      Then the pixel at 20, 20 of the window is "#ff0000"

    # Formula (documented range [0, 1] towards an RGBA colour; measured on
    # qml6 drawing with OpenGL: the colour is scaled by the source's Rec. 601
    # luma, so white takes it and black stays black):
    #   t  = clamp(colorization, 0, 1) * colorizationColor.a
    #   Y  = 0.299 * r + 0.587 * g + 0.114 * b
    #   c' = c + (colorizationColor.c * Y - c) * t
    # Source #ff0000 = (1, 0, 0) -> Y = 0.299:
    #   colorization = 0.5, colorizationColor = "#0000ff" (0, 0, 1, 1):
    #     r' = 1 + (0 - 1)*0.5 = 0.5 -> 128 = 0x80
    #     g' = 0
    #     b' = 0 + (0.299 - 0)*0.5 = 0.1495 -> 38 = 0x26
    #     -> #800026
    #   colorization = 1.0, colorizationColor = "#00ff00" (0, 1, 0, 1):
    #     (r', g', b') = (0, 0.299, 0) -> #004c00
    #   On `maskedHalf` (alpha = 128/255), over the black root:
    #     colorization = 1.0, colorizationColor = "#00ff00"
    #     -> 76 * 128/255 = 38.15 -> #002600.
    Scenario: Colorization scales colorizationColor by the source's luma and preserves mask alpha
      When 200 milliseconds pass
      And the property "colorizationColor" of "fx" is set to "#0000ff"
      And the property "colorization" of "fx" is set to 0.5
      Then the pixel at 20, 20 of the window is "#800026"
      When the property "colorizationColor" of "fx" is set to "#00ff00"
      And the property "colorization" of "fx" is set to 1
      Then the pixel at 20, 20 of the window is "#004c00"
      When the property "colorizationColor" of "maskedHalf" is set to "#00ff00"
      And the property "colorization" of "maskedHalf" is set to 1
      Then the pixel at 70, 20 of the window is "#002600"

    # Separable 1D Gaussian blur on a 5x5 #ff0000 source at (40..45, 40..45),
    # centre at (42, 42):
    #   R = blur * blurMax * (1 + blurMultiplier) = 0.5 * 4 * 1 = 2, sigma = R / 2 = 1.
    #   Unnormalized weights w(k) = exp(-k^2 / (2*sigma^2)) for k in -2..=2:
    #     w(0) = 1, w(+-1) = exp(-0.5) = 0.606531, w(+-2) = exp(-2) = 0.135335
    #     sum S = 2.483732 -> normalized w_hat = [0.054489, 0.244201, 0.402620, 0.244201, 0.054489]
    #   Centre (42, 42): full 5x5 kernel lies inside the 5x5 source -> sum = 1.0 -> #ff0000.
    #   Inner cardinal edge centres (40, 42), (44, 42), (42, 40), (42, 44):
    #     1.0 * (w_hat(0) + w_hat(1) + w_hat(2)) = 0.701310 -> round(178.83) = 179 = #b30000.
    #   Outer cardinal neighbours (39, 42), (45, 42), (42, 39), (42, 45) (in auto-padded area):
    #     1.0 * (w_hat(1) + w_hat(2)) = 0.298690 -> round(76.17) = 76 = #4c0000.
    #   3px outside at (37, 42): outside support r = 2 -> #000000;
    #   setting blurMultiplier = 1 doubles R to 4 (sigma = 2) so at (37, 42):
    #     w_x = (w(3)+w(4))/S = 0.093913, w_y = (w(0)+2w(1)+2w(2))/S = 0.812174
    #     w_x * w_y * 255 = 19.45 -> 19 = 0x13 -> #130000.
    Scenario: Blur spreads a small sharp source symmetrically while keeping its centre brightest
      When 200 milliseconds pass
      And the property "x" of "fx" is set to 40
      And the property "y" of "fx" is set to 40
      And the property "width" of "fx" is set to 5
      And the property "height" of "fx" is set to 5
      And the property "blurEnabled" of "fx" is set to true
      And the property "blur" of "fx" is set to 0.5
      And the property "blurMax" of "fx" is set to 4
      Then the pixel at 42, 42 of the window is "#ff0000"
      And the pixel at 40, 42 of the window is "#b30000"
      And the pixel at 44, 42 of the window is "#b30000"
      And the pixel at 42, 40 of the window is "#b30000"
      And the pixel at 42, 44 of the window is "#b30000"
      And the pixel at 39, 42 of the window is "#4c0000"
      And the pixel at 45, 42 of the window is "#4c0000"
      And the pixel at 42, 39 of the window is "#4c0000"
      And the pixel at 42, 45 of the window is "#4c0000"
      And the pixel at 37, 42 of the window is "#000000"
      When the property "blurMultiplier" of "fx" is set to 1
      Then the pixel at 37, 42 of the window is "#130000"

    # Shadow at (+10, +8) under a 40x40 #ff0000 source at (20, 20):
    #   Source covers (20..60, 20..60); shadow covers (30..70, 28..68).
    #   Inside source at (35, 35): opaque #ff0000 covers the shadow -> #ff0000.
    #   Right overhang at (65, 45) and bottom overhang at (45, 64):
    #     shadowColor = "#00ff00", shadowOpacity = 1.0, shadowBlur = 0 -> #00ff00.
    #   With shadowOpacity = 0.5:
    #     alpha = 0.5 -> round(0.5 * 255) = 128 = 0x80 -> #008000.
    Scenario: Shadow is drawn under the source at its offset, colour and opacity
      When 200 milliseconds pass
      And the property "x" of "fx" is set to 20
      And the property "y" of "fx" is set to 20
      And the property "shadowEnabled" of "fx" is set to true
      And the property "shadowBlur" of "fx" is set to 0
      And the property "shadowColor" of "fx" is set to "#00ff00"
      And the property "shadowHorizontalOffset" of "fx" is set to 10
      And the property "shadowVerticalOffset" of "fx" is set to 8
      Then the pixel at 35, 35 of the window is "#ff0000"
      And the pixel at 65, 45 of the window is "#00ff00"
      And the pixel at 45, 64 of the window is "#00ff00"
      And the pixel at 15, 35 of the window is "#000000"
      When the property "shadowOpacity" of "fx" is set to 0.5
      Then the pixel at 65, 45 of the window is "#008000"

    # 1. Masked source (`maskedHalf` at x: 50, y: 0, 40x40):
    #    mask `half.png` gives source alpha = 128/255.
    #    With shadowEnabled = true, shadowBlur = 0, shadowColor = "#00ff00",
    #    shadowHorizontalOffset = 10:
    #    Right overhang at (95, 20) has only the shadow with alpha = 128/255 -> #008000.
    #    Overlap at (65, 20) composites premultiplied fg (128, 0, 0, 128) over
    #    shadow (0, 128, 0, 128):
    #      r = 128 = 0x80, g = round(128 * (1 - 128/255)) = round(63.75) = 64 = 0x40
    #      -> #804000.
    # 2. Shadow scale (`fx` at x: 20, y: 20, 40x40, centre (40, 40)):
    #    shadowScale = 1.5 expands 40x40 shadow to 60x60 around (40, 40) -> (10..70, 10..70),
    #    so all 4 outer sides (15, 40), (65, 40), (40, 15), (40, 65) show #00ff00.
    Scenario: Shadow uses the masked source alpha and scales around the item centre
      When 200 milliseconds pass
      And the property "shadowEnabled" of "maskedHalf" is set to true
      And the property "shadowBlur" of "maskedHalf" is set to 0
      And the property "shadowColor" of "maskedHalf" is set to "#00ff00"
      And the property "shadowHorizontalOffset" of "maskedHalf" is set to 10
      Then the pixel at 95, 20 of the window is "#008000"
      And the pixel at 65, 20 of the window is "#804000"
      When the property "x" of "fx" is set to 20
      And the property "y" of "fx" is set to 20
      And the property "shadowEnabled" of "fx" is set to true
      And the property "shadowBlur" of "fx" is set to 0
      And the property "shadowColor" of "fx" is set to "#00ff00"
      And the property "shadowScale" of "fx" is set to 1.5
      Then the pixel at 15, 40 of the window is "#00ff00"
      And the pixel at 65, 40 of the window is "#00ff00"
      And the pixel at 40, 15 of the window is "#00ff00"
      And the pixel at 40, 65 of the window is "#00ff00"

    # With `fx` at (20, 20) of size 40x40 (covering 20..60, 20..60) and a shadow
    # offset by (+10, 0) (reaching x = 60..70):
    #   1. autoPaddingEnabled = false and paddingRect = 0,0,0x0:
    #      the canvas is not padded beyond 20..60, so (65, 40) is #000000.
    #   2. paddingRect = Qt.rect(0, 0, 15, 0) (adds 15px on the right):
    #      the canvas now reaches x = 20..75, so (65, 40) paints #00ff00.
    #   3. Resetting paddingRect to Qt.rect(0, 0, 0, 0) and enabling
    #      autoPaddingEnabled = true expands all sides by blurMax (32px),
    #      so (65, 40) still paints #00ff00.
    Scenario: Padding rectangle and autoPaddingEnabled control painted bounds outside the item
      When 200 milliseconds pass
      And the property "x" of "fx" is set to 20
      And the property "y" of "fx" is set to 20
      And the property "autoPaddingEnabled" of "fx" is set to false
      And the property "shadowEnabled" of "fx" is set to true
      And the property "shadowBlur" of "fx" is set to 0
      And the property "shadowColor" of "fx" is set to "#00ff00"
      And the property "shadowHorizontalOffset" of "fx" is set to 10
      Then the pixel at 65, 40 of the window is "#000000"
      When the property "paddingRect" of "fx" is bound to "Qt.rect(0, 0, 15, 0)"
      Then the pixel at 65, 40 of the window is "#00ff00"
      When the property "paddingRect" of "fx" is bound to "Qt.rect(0, 0, 0, 0)"
      And the property "autoPaddingEnabled" of "fx" is set to true
      Then the pixel at 65, 40 of the window is "#00ff00"

