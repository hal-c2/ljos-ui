# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#wrapMode-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#lineCount-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#contentWidth-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Text wrapping
  A Text with a width and a wrapMode breaks its text into lines. Exact widths
  depend on the font, so these scenarios compare items with each other.

  Rule: Sentences

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string words: "Hello world, this is a long line of text"
          property bool wordFits: word.contentWidth <= word.width
          property bool plainOverflows: plain.contentWidth > plain.width
          property bool naturalWidthKept: word.implicitWidth === free.implicitWidth
          property bool heightIsLines: word.height === word.lineCount * free.height
          property bool implicitHeightIsWrapped: word.implicitHeight === word.contentHeight
          property bool wrapLikeWord: both.lineCount === word.lineCount
          Text { id: free; objectName: "free"; text: root.words; wrapMode: Text.WordWrap }
          Text { id: plain; objectName: "plain"; width: 100; text: root.words }
          Text { id: word; objectName: "word"; width: 100; text: root.words; wrapMode: Text.WordWrap }
          Text { id: both; objectName: "both"; width: 100; text: root.words; wrapMode: Text.Wrap }
        }
        """
      When 100 milliseconds pass

    Scenario: Without a wrap mode the text stays on one line and overflows
      Then the property "lineCount" of "plain" is 1
      And the property "plainOverflows" of "root" is true

    Scenario: Without a width nothing wraps
      Then the property "lineCount" of "free" is 1

    Scenario: WordWrap breaks between words so every line fits
      Then the property "lineCount" of "word" is at least 2
      And the property "wordFits" of "root" is true

    Scenario: Each line adds one line height
      Then the property "heightIsLines" of "root" is true
      And the property "implicitHeightIsWrapped" of "root" is true

    Scenario: The implicit width stays that of the unwrapped text
      Then the property "naturalWidthKept" of "root" is true

    Scenario: Wrap breaks between words when it can
      Then the property "wrapLikeWord" of "root" is true

    Scenario: A wider item needs fewer lines
      When the property "width" of "word" is set to 2000
      Then the property "lineCount" of "word" is 1
      And the property "heightIsLines" of "root" is true

    Scenario: Turning wrapping off joins the lines again
      When the property "wrapMode" of "word" is set to 0
      Then the property "lineCount" of "word" is 1

  Rule: Words longer than the line

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string longWord: "Incomprehensibilities"
          property bool wordOverflows: word.contentWidth > word.width
          property bool anywhereFits: anywhere.contentWidth <= anywhere.width
          property bool wrapLikeAnywhere: both.lineCount === anywhere.lineCount
          Text { id: word; objectName: "word"; width: 50; text: root.longWord; wrapMode: Text.WordWrap }
          Text {
            id: anywhere; objectName: "anywhere"
            width: 50; text: root.longWord; wrapMode: Text.WrapAnywhere
          }
          Text { id: both; objectName: "both"; width: 50; text: root.longWord; wrapMode: Text.Wrap }
        }
        """
      When 100 milliseconds pass

    Scenario: WordWrap never breaks inside a word
      Then the property "lineCount" of "word" is 1
      And the property "wordOverflows" of "root" is true

    Scenario: WrapAnywhere breaks inside a word
      Then the property "lineCount" of "anywhere" is at least 2
      And the property "anywhereFits" of "root" is true

    Scenario: Wrap breaks inside a word that has no other break
      Then the property "lineCount" of "both" is at least 2
      And the property "wrapLikeAnywhere" of "root" is true

  Rule: Line breaks in the text

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property bool twiceAsTall: two.height === 2 * one.height
          property bool widestLineWins: two.contentWidth === one.contentWidth
          Text { id: one; objectName: "one"; text: "second line" }
          Text { id: two; objectName: "two"; text: "first\nsecond line" }
          Text { id: gap; objectName: "gap"; text: "a\n\nb" }
        }
        """
      When 100 milliseconds pass

    Scenario: A line break starts a new line without any wrap mode
      Then the property "lineCount" of "two" is 2
      And the property "twiceAsTall" of "root" is true

    Scenario: The content is as wide as the widest line
      Then the property "widestLineWins" of "root" is true

    Scenario: An empty line counts
      Then the property "lineCount" of "gap" is 3
