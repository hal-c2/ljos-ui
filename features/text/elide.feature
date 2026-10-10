# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#elide-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#truncated-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Eliding text that does not fit
  A Text with a width and an elide mode cuts its text and shows "…" in its place.

  Rule: One line

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string words: "Hello world, this is a long line of text"
          property bool leftFits: left.contentWidth <= 100 && left.contentWidth > 50
          property bool middleFits: middle.contentWidth <= 100 && middle.contentWidth > 50
          property bool rightFits: right.contentWidth <= 100 && right.contentWidth > 50
          property bool roomyIsWhole: roomy.contentWidth === free.contentWidth
          property bool naturalWidthKept: right.implicitWidth === free.implicitWidth
          Text { id: free; objectName: "free"; text: root.words; elide: Text.ElideRight }
          Text { id: left; objectName: "left"; width: 100; text: root.words; elide: Text.ElideLeft }
          Text { id: middle; objectName: "middle"; width: 100; text: root.words; elide: Text.ElideMiddle }
          Text { id: right; objectName: "right"; width: 100; text: root.words; elide: Text.ElideRight }
          Text { id: roomy; objectName: "roomy"; width: 2000; text: root.words; elide: Text.ElideRight }
          Text { id: tiny; objectName: "tiny"; width: 2; text: root.words; elide: Text.ElideRight }
        }
        """
      When 100 milliseconds pass

    Scenario Outline: The elided text fills most of the width and never more
      Then the property "truncated" of "<item>" is true
      And the property "lineCount" of "<item>" is 1
      And the property "<fits>" of "root" is true

      Examples:
        | item   | fits       |
        | left   | leftFits   |
        | middle | middleFits |
        | right  | rightFits  |

    Scenario: Text that fits is left alone
      Then the property "truncated" of "roomy" is false
      And the property "roomyIsWhole" of "root" is true

    Scenario: Without a width nothing is elided
      Then the property "truncated" of "free" is false

    Scenario: The implicit width stays that of the whole text
      Then the property "naturalWidthKept" of "root" is true

    Scenario: When not even the ellipsis fits nothing is shown
      Then the property "truncated" of "tiny" is true
      And the property "contentWidth" of "tiny" is 0

    Scenario: A wider item shows the whole text again
      When the property "width" of "right" is set to 2000
      Then the property "truncated" of "right" is false

    Scenario: A longer text is elided when it stops fitting
      When the property "text" of "roomy" is set to "x"
      And the property "width" of "roomy" is set to 100
      Then the property "truncated" of "roomy" is false
      When the property "text" of "roomy" is set to "Hello world, this is a long line of text"
      Then the property "truncated" of "roomy" is true

  Rule: Wrapped text

    Background:
      Given the QML:
        """qml
        import QtQuick
        Item {
          id: root
          objectName: "root"
          property string words: "Hello world, this is a long line of text"
          property bool lastLineFits: capped.contentWidth <= capped.width
          property bool shortIsOneLine: short.contentHeight === one.height
          property bool fullHeightKept: short.implicitHeight === whole.height
          Text { id: one; objectName: "one"; text: "x" }
          Text { id: whole; objectName: "whole"; width: 100; text: root.words; wrapMode: Text.WordWrap }
          Text {
            id: capped; objectName: "capped"; width: 100; text: root.words
            wrapMode: Text.WordWrap; elide: Text.ElideRight; maximumLineCount: 2
          }
          Text {
            id: short; objectName: "short"; width: 100; height: one.height + 4; text: root.words
            wrapMode: Text.WordWrap; elide: Text.ElideRight
          }
          Text { id: tall; objectName: "tall"; width: 100; height: 4; text: root.words; wrapMode: Text.WordWrap }
        }
        """
      When 100 milliseconds pass

    Scenario: ElideRight ends the last allowed line with an ellipsis
      Then the property "lineCount" of "capped" is 2
      And the property "truncated" of "capped" is true
      And the property "lastLineFits" of "root" is true

    Scenario: ElideRight keeps only the lines that fit the height
      Then the property "lineCount" of "short" is 1
      And the property "truncated" of "short" is true
      And the property "shortIsOneLine" of "root" is true

    Scenario: The implicit height stays that of all the lines
      Then the property "fullHeightKept" of "root" is true

    Scenario: Without an elide mode a short item overflows instead
      Then the property "truncated" of "tall" is false
      And the property "lineCount" of "tall" is at least 2
