# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
# Verified on Qt 6.11.2 (qml6, offscreen). The documentation says AutoText
# "will automatically determine" whether the text is styled; the rule in
# the scenarios below is what Qt 6.11.2 does, found by comparing widths.

@qt-parity
Feature: Text format
  textFormat says how a Text reads its text: as it is (PlainText), as the
  small set of tags of StyledText, as the HTML subset of RichText, or as
  Markdown. AutoText, the default, picks StyledText for text that starts
  like markup and PlainText for everything else.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool tagsAreRead: auto.width > word.width && auto.width < shown.width
        property bool autoIsStyled: auto.width === styled.width && auto.height === styled.height
        property bool starsStay: stars.width === starsShown.width
        property bool markdownReadsStars: md.width === auto.width
        property bool loneSignIsPlain: less.width === lessShown.width
        property bool unknownIsPlain: odd.width === oddShown.width
        property bool laterLineIsPlain: late.width === lateShown.width
        Text { id: word; objectName: "word"; text: "abc" }
        Text { id: auto; objectName: "auto"; text: "<b>abc</b>" }
        Text { id: shown; objectName: "shown"; text: "<b>abc</b>"; textFormat: Text.PlainText }
        Text { id: styled; text: "<b>abc</b>"; textFormat: Text.StyledText }
        Text { id: stars; text: "**abc**" }
        Text { id: starsShown; text: "**abc**"; textFormat: Text.PlainText }
        Text { id: md; text: "**abc**"; textFormat: Text.MarkdownText }
        Text { id: less; text: "a < b" }
        Text { id: lessShown; text: "a < b"; textFormat: Text.PlainText }
        Text { id: odd; text: "<wrong>abc</wrong>" }
        Text { id: oddShown; text: "<wrong>abc</wrong>"; textFormat: Text.PlainText }
        Text { id: late; text: "abc\n<b>abc</b>" }
        Text { id: lateShown; text: "abc\n<b>abc</b>"; textFormat: Text.PlainText }
      }
      """
    When 100 milliseconds pass

  Scenario: The format is AutoText until it is set
    Then the property "textFormat" of "auto" is 2

  Scenario: AutoText reads text that starts with a tag as styled text
    Then the property "tagsAreRead" of "root" is true
    And the property "autoIsStyled" of "root" is true

  Scenario: PlainText shows the tags
    Then the property "tagsAreRead" of "root" is true

  Scenario: AutoText never takes text for Markdown
    Then the property "starsStay" of "root" is true
    And the property "markdownReadsStars" of "root" is true

  Scenario: A less-than sign alone does not make text styled
    Then the property "loneSignIsPlain" of "root" is true

  Scenario: A tag Qt does not know does not make text styled
    Then the property "unknownIsPlain" of "root" is true

  Scenario: Only the first line decides
    Then the property "laterLineIsPlain" of "root" is true

  Scenario: Setting the format reads the same text again
    When the property "textFormat" of "auto" is set to 0
    Then the property "tagsAreRead" of "root" is false
    And the property "text" of "auto" is "<b>abc</b>"
