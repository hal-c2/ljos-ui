# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-effects-multieffect.html
# Verified on Qt 6.11.2 (qml6, offscreen). Qt's software scene graph,
# which the Qt backend uses, draws no MultiEffect at all (measured:
# every effect pixel is the background), so what Ljós paints is pinned
# on Ljós only, from the mask model the documentation describes.

Feature: MultiEffect
  A MultiEffect draws its source item with post-processing applied.
  Ljós renders the mask and the unmodified source; blur, shadow and
  the colour adjustments are kept as properties and not painted yet.

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
        | autoPaddingEnabled     | true      |
        | implicitWidth          | 0         |
        | implicitHeight         | 0         |

  @ljos-only
  Rule: What it paints

    Scenario: The source is drawn unchanged when no effect is enabled
      When 200 milliseconds pass
      Then the pixel at 20, 20 of the window is "#ff0000"

    Scenario: The source item is not drawn in its own place
      When 200 milliseconds pass
      Then the pixel at 20, 70 of the window is "#000000"

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
          Image { id: red; source: "red.png" }
          Image { id: half; source: "half.png" }
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
