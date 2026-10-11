# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#text-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
# Verified on Qt 6.11.2 (qml6, offscreen). The scenarios use the default
# font on purpose: with another font Qt 6.11.2 sets the blocks of Markdown
# apart differently when the text is set a second time (see the last
# scenario of markdown.feature), so text that arrived in pieces would not
# be the size of the same text given at the start.

@qt-parity
Feature: Text that arrives in pieces
  A long answer is often shown while it is still being written: the text
  property is set again and again, each time a little longer. Whatever the
  pieces were, the Text ends up exactly as if the whole text had been given
  at once, and on the way it shows the text so far.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root; objectName: "root"
        property var pieces: ["# He", "ad", "ing\n\nSome *te", "xt* and **mo", "re**\n",
          "\n- on", "e\n- t", "wo\n\n``", "`\ncode\nmore\n`", "``\n\n> quo", "te\n\nla", "st"]
        property int next: 0
        property bool sameAsWhole: stream.width === whole.width && stream.height === whole.height
        property bool sameAsSoFar: stream.width === soFar.width && stream.height === soFar.height
        property bool grew: stream.height > soFar.height
        property bool emptyIsALine: stream.height === line.height && stream.width === 0
        function feed(count) {
          for (let i = 0; i < count && next < pieces.length; ++i) stream.text += pieces[next++]
        }
        component M: Text { textFormat: Text.MarkdownText }
        M { id: stream; objectName: "stream" } M { id: line; text: "abc" }
        M { id: whole; text: root.pieces.join("") }
        M { id: soFar; text: root.pieces.slice(0, 5).join("") }
      }
      """
    When 100 milliseconds pass

  Scenario: Before anything has arrived the Text is one empty line
    Then the property "text" of "stream" is ""
    And the property "emptyIsALine" of "root" is true

  Scenario: The text so far is shown as it would be on its own
    When the method "feed" of "root" is called with 5
    Then the property "sameAsSoFar" of "root" is true
    And the property "sameAsWhole" of "root" is false

  Scenario: Each piece adds to what is shown
    When the method "feed" of "root" is called with 5
    And the method "feed" of "root" is called with 3
    Then the property "grew" of "root" is true

  Scenario: The finished text is the text given whole
    When the method "feed" of "root" is called with 5
    And the method "feed" of "root" is called with 3
    And the method "feed" of "root" is called with 4
    Then the property "sameAsWhole" of "root" is true
    And the property "next" of "root" is 12

  Scenario: One piece at a time ends the same way
    When the method "feed" of "root" is called with 1
    And the method "feed" of "root" is called with 1
    And the method "feed" of "root" is called with 1
    And the method "feed" of "root" is called with 1
    And the method "feed" of "root" is called with 8
    Then the property "sameAsWhole" of "root" is true
