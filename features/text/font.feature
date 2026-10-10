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
          property bool pointsArePixels: points.width === pixels.width && points.height === pixels.height
          property bool pixelsWin: both.width === pixels.width
          property bool pointsRoundUp: eleven.width === fifteen.width && eleven.height === fifteen.height
          property bool pointsRoundDown: ten.width === thirteen.width && ten.height === thirteen.height
          Text { id: plain; objectName: "plain"; text: "Hello world" }
          Text { id: big; objectName: "big"; text: "Hello world"; font.pixelSize: 40 }
          Text { id: pixels; objectName: "pixels"; text: "Hello world"; font.pixelSize: 16 }
          Text { id: points; objectName: "points"; text: "Hello world"; font.pointSize: 12 }
          Text { id: both; objectName: "both"; text: "Hello world"; font.pixelSize: 16; font.pointSize: 30 }
          Text { id: eleven; text: "Hello world"; font.pointSize: 11 }
          Text { id: fifteen; text: "Hello world"; font.pixelSize: 15 }
          Text { id: ten; text: "Hello world"; font.pointSize: 10 }
          Text { id: thirteen; text: "Hello world"; font.pixelSize: 13 }
        }
        """
      When 100 milliseconds pass

    Scenario: A larger pixel size makes the text wider and taller
      Then the property "bigIsWider" of "root" is true
      And the property "bigIsTaller" of "root" is true
      And the property "bigBaselineIsLower" of "root" is true

    Scenario: Twelve points are sixteen pixels
      Then the property "pointsArePixels" of "root" is true

    # Seen on Qt 6.11.2: 11 pt is 14.67 px and is set at 15 px; 10 pt is
    # 13.33 px and is set at 13 px.
    Scenario: A point size is set at the nearest whole pixel size
      Then the property "pointsRoundUp" of "root" is true
      And the property "pointsRoundDown" of "root" is true

    Scenario: The pixel size wins over the point size
      Then the property "pixelsWin" of "root" is true

    Scenario: Changing the size resizes the item
      When the property "font.pixelSize" of "plain" is set to 40
      Then the property "bigIsWider" of "root" is false
      And the property "bigIsTaller" of "root" is false

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
