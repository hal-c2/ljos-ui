# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-fontmetrics.html
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#font.pixelSize-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: FontMetrics
  FontMetrics tells what a font measures without showing any text. Numbers
  depend on the font file, so these scenarios compare them with each other.

  Rule: The numbers of a font

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool heightIsAscentPlusDescent: fm.height === fm.ascent + fm.descent
          property bool spacingAddsLeading: fm.lineSpacing === fm.height + fm.leading
          property bool overlineIsAboveTheAscent: fm.overlinePosition === fm.ascent + 1
          property bool strikeOutIsInsideTheAscent: fm.strikeOutPosition > 0 && fm.strikeOutPosition < fm.ascent
          property bool xIsLowerThanTheAscent: fm.xHeight > 0 && fm.xHeight < fm.ascent
          property bool widestIsWiderThanAverage: fm.maximumCharacterWidth > fm.averageCharacterWidth
          property bool textIsOneLineTall: Math.abs(label.height - fm.lineSpacing) < 1
          property bool baselineIsTheAscent: Math.abs(label.baselineOffset - fm.ascent) < 1
          property bool bigIsBigger: big.height > fm.height && big.ascent > fm.ascent && big.xHeight > fm.xHeight
          property bool sameAsBig: fm.height === big.height && fm.averageCharacterWidth === big.averageCharacterWidth
          FontMetrics { id: fm; objectName: "fm" }
          FontMetrics { id: big; objectName: "big"; font.pixelSize: 40 }
          Text { id: label; text: "Hello" }
        }
        """
      When 100 milliseconds pass

    Scenario: The height of a font is its ascent plus its descent
      Then the property "heightIsAscentPlusDescent" of "root" is true
      And the property "ascent" of "fm" is at least 1
      And the property "descent" of "fm" is at least 1

    Scenario: The line spacing adds the leading
      Then the property "spacingAddsLeading" of "root" is true

    Scenario: The lines a font is decorated with
      Then the property "overlineIsAboveTheAscent" of "root" is true
      And the property "strikeOutIsInsideTheAscent" of "root" is true
      And the property "underlinePosition" of "fm" is at least 0
      And the property "lineWidth" of "fm" is at least 1

    Scenario: A lower-case x is lower than the ascent
      Then the property "xIsLowerThanTheAscent" of "root" is true

    Scenario: The widest character is wider than the average one
      Then the property "widestIsWiderThanAverage" of "root" is true

    Scenario: A line of text is as tall as the font says
      Then the property "textIsOneLineTall" of "root" is true
      And the property "baselineIsTheAscent" of "root" is true

    Scenario: A larger font has larger numbers
      Then the property "bigIsBigger" of "root" is true
      And the property "sameAsBig" of "root" is false

    Scenario: The numbers follow the font
      When the property "font.pixelSize" of "fm" is set to 40
      Then the property "sameAsBig" of "root" is true

    Scenario: The numbers cannot be assigned
      Then setting the property "ascent" of "fm" to 3 is rejected

  Rule: Measuring text

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property rect box: fm.boundingRect("Hello")
          property rect tight: fm.tightBoundingRect("Hello")
          property rect low: fm.tightBoundingRect("x")
          property bool advanceIsTheTextWidth: fm.advanceWidth("Hello world") === label.width
          property bool longerIsWider: fm.advanceWidth("Hello world") > fm.advanceWidth("Hello")
          property bool boxStartsAboveTheBaseline: Math.abs(box.y + fm.ascent) < 1 && box.x <= 1
          property bool boxIsAsTallAsTheFont: Math.abs(box.height - fm.height) < 1
          property bool boxIsAboutTheAdvance: Math.abs(box.width - fm.advanceWidth("Hello")) < 2
          property bool tightIsInside: tight.y > box.y && tight.height < box.height && tight.width <= box.width
          property bool xIsLowerThanH: low.height < tight.height
          property bool lowIsAboutTheXHeight: Math.abs(low.height - fm.xHeight) < 1.5
          property rect nothing: fm.boundingRect("")
          FontMetrics { id: fm; objectName: "fm" }
          Text { id: label; text: "Hello world" }
        }
        """
      When 100 milliseconds pass

    Scenario: The advance width is how wide a Text with that text is
      Then the property "advanceIsTheTextWidth" of "root" is true
      And the property "longerIsWider" of "root" is true

    Scenario: The bounding rectangle is a line of the font around the text
      Then the property "boxStartsAboveTheBaseline" of "root" is true
      And the property "boxIsAsTallAsTheFont" of "root" is true
      And the property "boxIsAboutTheAdvance" of "root" is true

    Scenario: The tight bounding rectangle holds the ink only
      Then the property "tightIsInside" of "root" is true
      And the property "xIsLowerThanH" of "root" is true
      And the property "lowIsAboutTheXHeight" of "root" is true

    Scenario: No text measures nothing
      Then the property "nothing.width" of "root" is 0
      And the property "nothing.height" of "root" is 0

  Rule: Eliding text

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string words: "Hello world, this is a long line of text"
          property string atEnd: fm.elidedText(words, Qt.ElideRight, 100)
          property string atStart: fm.elidedText(words, Qt.ElideLeft, 100)
          property string inMiddle: fm.elidedText(words, Qt.ElideMiddle, 100)
          property string asIs: fm.elidedText(words, Qt.ElideNone, 100)
          property string roomy: fm.elidedText(words, Qt.ElideRight, 5000)
          property bool rightKeepsTheStart: atEnd.startsWith("Hello") && atEnd.endsWith("…")
          property bool leftKeepsTheEnd: atStart.startsWith("…") && atStart.endsWith("text")
          property bool middleKeepsBothEnds: inMiddle.startsWith("H") && inMiddle.endsWith("t") && inMiddle.includes("…")
          property bool middleKeepsEqualEnds: inMiddle.indexOf("…") * 2 + 1 === inMiddle.length
          property bool allFit: fm.advanceWidth(atEnd) <= 100 && fm.advanceWidth(atStart) <= 100 && fm.advanceWidth(inMiddle) <= 100
          property bool allAreShorter: atEnd.length < words.length && atStart.length < words.length && inMiddle.length < words.length
          FontMetrics { id: fm; objectName: "fm" }
        }
        """
      When 100 milliseconds pass

    Scenario: Text that does not fit is cut and marked with an ellipsis
      Then the property "rightKeepsTheStart" of "root" is true
      And the property "leftKeepsTheEnd" of "root" is true
      And the property "middleKeepsBothEnds" of "root" is true
      And the property "allAreShorter" of "root" is true

    Scenario: The elided text fits the width
      Then the property "allFit" of "root" is true

    # Seen on Qt 6.11.2: a cut in the middle gives up a character at each
    # end at a time, even where one more would fit on one side.
    Scenario: A cut in the middle keeps as many characters at each end
      Then the property "middleKeepsEqualEnds" of "root" is true

    Scenario: Text that fits is atStart alone
      Then the property "roomy" of "root" is "Hello world, this is a long line of text"

    Scenario: ElideNone leaves the text alone
      Then the property "asIs" of "root" is "Hello world, this is a long line of text"
