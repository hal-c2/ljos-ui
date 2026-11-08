# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.pixelSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.pointSize-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.bold-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.weight-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.letterSpacing-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.wordSpacing-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.underline-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: The font of a Text
  The font decides how large a Text is. Numbers depend on the font file, so
  these scenarios compare items with each other.

  Rule: Size

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool bigIsWider: big.width > plain.width
          property bool bigIsTaller: big.height > plain.height
          property bool bigBaselineIsLower: big.baselineOffset > plain.baselineOffset
          property bool pixelsWin: both.width === pixels.width
          property bool pointsRoundUp: eleven.width === elevenInPixels.width && eleven.height === elevenInPixels.height
          property bool pointsRoundDown: ten.width === tenInPixels.width && ten.height === tenInPixels.height
          readonly property real perPoint: Screen.logicalPixelDensity * 25.4 / 72
          Text { id: plain; objectName: "plain"; text: "Hello world" }
          Text { id: big; objectName: "big"; text: "Hello world"; font.pixelSize: 40 }
          Text { id: pixels; objectName: "pixels"; text: "Hello world"; font.pixelSize: 16 }
          Text { id: both; objectName: "both"; text: "Hello world"; font.pixelSize: 16; font.pointSize: 30 }
          Text { id: eleven; text: "Hello world"; font.pointSize: 11 }
          Text { id: elevenInPixels; text: "Hello world"; font.pixelSize: Math.round(11 * root.perPoint) }
          Text { id: ten; text: "Hello world"; font.pointSize: 10 }
          Text { id: tenInPixels; text: "Hello world"; font.pixelSize: Math.round(10 * root.perPoint) }
        }
        """
      When 100 milliseconds pass

    Scenario: A larger pixel size makes the text wider and taller
      Then the property "bigIsWider" of "root" is true
      And the property "bigIsTaller" of "root" is true
      And the property "bigBaselineIsLower" of "root" is true

    # Seen on Qt 6.11.2 at 96 dpi (Linux, Windows, qml6 offscreen): 11 pt
    # is 14.67 px and is set at 15 px; 10 pt is 13.33 px and is set at
    # 13 px. At macOS's 72 a point is a whole pixel.
    Scenario: A point size is set at the nearest whole pixel size
      Then the property "pointsRoundUp" of "root" is true
      And the property "pointsRoundDown" of "root" is true

    Scenario: The pixel size wins over the point size
      Then the property "pixelsWin" of "root" is true

    Scenario: Changing the size resizes the item
      When the property "font.pixelSize" of "plain" is set to 40
      Then the property "bigIsWider" of "root" is false
      And the property "bigIsTaller" of "root" is false

  # A point is 1/72 of a logical inch, and how many pixels make that inch
  # is the platform's: 96 on Linux and Windows (and qml6's offscreen
  # platform anywhere), 72 on macOS. 18 pt is a whole pixel size at both.
  Rule: Points follow the screen's logical pixel density

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          readonly property real perPoint: Screen.logicalPixelDensity * 25.4 / 72
          property bool hasDensity: perPoint > 0
          property bool pointsArePlatformPixels: points.font.pixelSize === Math.round(18 * perPoint)
          property bool pointsAreThosePixels: points.width === pixels.width && points.height === pixels.height
          Text { id: points; text: "Hello world"; font.pointSize: 18 }
          Text { id: pixels; text: "Hello world"; font.pixelSize: Math.round(18 * root.perPoint) }
        }
        """
      When 100 milliseconds pass

    Scenario: A point is as many pixels as the logical density makes it
      Then the property "hasDensity" of "root" is true
      And the property "pointsArePlatformPixels" of "root" is true

    Scenario: Text set in points is as large as text set in those pixels
      Then the property "pointsAreThosePixels" of "root" is true

  Rule: Weight, spacing and decoration

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool boldIsWider: bold.width > plain.width
          property bool heavyIsBold: heavy.width === bold.width
          property real letterExtra: letters.width - plain.width
          property real wordExtra: words.width - plain.width
          property bool linesKeepSize: lined.width === plain.width && lined.height === plain.height
          Text { id: plain; objectName: "plain"; text: "Hello world" }
          Text { id: bold; objectName: "bold"; text: "Hello world"; font.bold: true }
          Text { id: heavy; objectName: "heavy"; text: "Hello world"; font.weight: 700 }
          Text { id: letters; objectName: "letters"; text: "Hello world"; font.letterSpacing: 2 }
          Text { id: words; objectName: "words"; text: "Hello world"; font.wordSpacing: 5 }
          Text {
            id: lined; objectName: "lined"; text: "Hello world"
            font.underline: true; font.strikeout: true
          }
          Text { id: odd; objectName: "odd"; text: "Hello world"; font.family: "No Such Family 123" }
        }
        """
      When 100 milliseconds pass

    Scenario: Bold text is wider
      Then the property "boldIsWider" of "root" is true

    Scenario: A weight of 700 is bold
      Then the property "heavyIsBold" of "root" is true
      And the property "font.bold" of "heavy" is true

    Scenario: Letter spacing is added after every character
      Then the property "letterExtra" of "root" is 22

    Scenario: Word spacing is added to every space
      Then the property "wordExtra" of "root" is 5

    Scenario: Underline and strikeout do not change the size
      Then the property "linesKeepSize" of "root" is true

    Scenario: An unknown family falls back to a font that exists
      Then the property "width" of "odd" is at least 20
      And the property "lineCount" of "odd" is 1
