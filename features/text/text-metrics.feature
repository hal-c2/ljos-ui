# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-textmetrics.html
#   https://doc.qt.io/qt-6/qml-qtquick-fontmetrics.html
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: TextMetrics
  TextMetrics measures one text in one font without showing it.

  Rule: Measuring

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool widthIsTheBox: tm.width === tm.boundingRect.width
          property bool heightIsTheBox: tm.height === tm.boundingRect.height
          property bool advanceIsTheFonts: tm.advanceWidth === fm.advanceWidth(tm.text)
          function same(a, b) { return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height }
          property bool boxIsTheFonts: same(tm.boundingRect, fm.boundingRect(tm.text))
          property bool tightIsTheFonts: same(tm.tightBoundingRect, fm.tightBoundingRect(tm.text))
          property bool tightIsInside: tm.tightBoundingRect.height < tm.height
          property bool sameAsLonger: tm.advanceWidth === longer.advanceWidth && tm.width === longer.width
          property bool bigIsBigger: big.width > tm.width && big.height > tm.height
          TextMetrics { id: tm; objectName: "tm"; text: "Hello world" }
          TextMetrics { id: longer; objectName: "longer"; text: "Hello world, and more" }
          TextMetrics { id: big; objectName: "big"; text: "Hello world"; font.pixelSize: 40 }
          TextMetrics { id: blank; objectName: "blank" }
          FontMetrics { id: fm }
        }
        """
      When 100 milliseconds pass

    Scenario: width and height are those of the bounding rectangle
      Then the property "widthIsTheBox" of "root" is true
      And the property "heightIsTheBox" of "root" is true
      And the property "width" of "tm" is at least 20

    Scenario: The numbers are the ones FontMetrics gives for the same text
      Then the property "advanceIsTheFonts" of "root" is true
      And the property "boxIsTheFonts" of "root" is true
      And the property "tightIsTheFonts" of "root" is true

    Scenario: The tight bounding rectangle is lower than the line
      Then the property "tightIsInside" of "root" is true

    Scenario: The numbers follow the text
      Then the property "sameAsLonger" of "root" is false
      When the property "text" of "tm" is set to "Hello world, and more"
      Then the property "sameAsLonger" of "root" is true

    Scenario: The numbers follow the font
      Then the property "bigIsBigger" of "root" is true
      When the property "font.pixelSize" of "tm" is set to 40
      Then the property "bigIsBigger" of "root" is false

    Scenario: Without a text everything is zero
      Then the property "text" of "blank" is ""
      And the property "width" of "blank" is 0
      And the property "height" of "blank" is 0
      And the property "advanceWidth" of "blank" is 0
      And the property "elidedText" of "blank" is ""

    Scenario: The numbers cannot be assigned
      Then setting the property "width" of "tm" to 3 is rejected

  Rule: Eliding

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string words: "Hello world, this is a long line of text"
          property bool cutAtTheEnd: right.elidedText.startsWith("Hello") && right.elidedText.endsWith("…")
          property bool cutAtTheStart: left.elidedText.startsWith("…") && left.elidedText.endsWith("text")
          property bool fits: fm.advanceWidth(right.elidedText) <= 100 && fm.advanceWidth(left.elidedText) <= 100
          property bool sameAsTheFonts: right.elidedText === fm.elidedText(words, Qt.ElideRight, 100)
          property bool widthIsOfTheWholeText: right.width === plain.width
          TextMetrics { id: plain; objectName: "plain"; text: root.words }
          TextMetrics { id: right; objectName: "right"; text: root.words; elide: Qt.ElideRight; elideWidth: 100 }
          TextMetrics { id: left; objectName: "left"; text: root.words; elide: Text.ElideLeft; elideWidth: 100 }
          TextMetrics { id: narrow; objectName: "narrow"; text: root.words; elide: Qt.ElideRight }
          FontMetrics { id: fm }
        }
        """
      When 100 milliseconds pass

    Scenario: Nothing is elided until it is asked for
      Then the property "elide" of "plain" is 3
      And the property "elideWidth" of "plain" is 0
      And the property "elidedText" of "plain" is "Hello world, this is a long line of text"

    Scenario: The elided text is cut to the elide width
      Then the property "cutAtTheEnd" of "root" is true
      And the property "cutAtTheStart" of "root" is true
      And the property "fits" of "root" is true
      And the property "sameAsTheFonts" of "root" is true

    Scenario: Eliding does not change the measured size
      Then the property "widthIsOfTheWholeText" of "root" is true

    Scenario: An elide width of zero leaves no text
      Then the property "elidedText" of "narrow" is ""

    Scenario: A wider elide width brings the text back
      When the property "elideWidth" of "right" is set to 5000
      Then the property "elidedText" of "right" is "Hello world, this is a long line of text"

    Scenario: Turning eliding off brings the text back
      When the property "elide" of "right" is set to 3
      Then the property "elidedText" of "right" is "Hello world, this is a long line of text"
