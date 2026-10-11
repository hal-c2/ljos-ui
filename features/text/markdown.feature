# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
#   https://spec.commonmark.org/0.31.2/
#   https://github.github.com/gfm/
#   docs/ARCHITECTURE.md (section 7.5)
# Verified on Qt 6.11.2 (qml6, offscreen). The documentation says
# MarkdownText is CommonMark with the GitHub extensions for tables and task
# lists; the indents and the 7 pixels between blocks are Qt 6.11.2's, found
# by comparing sizes (the 7 pixels are the same at every font size tried,
# for the text a Text is created with).

Feature: Markdown text
  MarkdownText reads CommonMark, with tables, strike-through and task
  lists, and lays it out as blocks like rich text.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real line: plain.height
        property bool strongIsBold: strong.width === bold.width && strong.height === line
        property bool softBreakIsASpace: soft.width === spaced.width && soft.height === line
        property bool paragraphsAreSetApart: two.height === 2 * line + 7 && two.width === plain.width
        property bool headingIsLarger: heading.height > line && heading.width > bold.width
        property bool listStandsIn: list.width === plain.width + 40 && list.height === 2 * line + 7
        property bool quoteStandsIn: quote.width === plain.width + 80
        property bool codeKeepsItsLines: code.height === 2 * line
        property bool tableHasRows: table.height > 2 * line && table.width > 2 * plain.width
        property bool htmlIsText: html.width > plain.width
        function again() { two.text = "abc\n\nabc " }
        component M: Text { textFormat: Text.MarkdownText; font.pixelSize: 16 }
        Text { id: plain; text: "abc"; font.pixelSize: 16 }
        Text { id: spaced; text: "abc abc"; font.pixelSize: 16 }
        Text { id: bold; text: "<b>abc</b>"; font.pixelSize: 16; textFormat: Text.StyledText }
        M { id: strong; text: "**abc**" } M { id: soft; text: "abc\nabc" }
        M { id: two; objectName: "two"; text: "abc\n\nabc" } M { id: heading; text: "# abc" }
        M { id: list; text: "- abc\n- abc" } M { id: quote; text: "> abc" }
        M { id: code; text: "```\nabc\nabc\n```" } M { id: table; text: "|abc|abc|\n|-|-|\n|abc|abc|" }
        M { id: html; objectName: "html"; text: "<b>abc</b>" }
      }
      """
    When 100 milliseconds pass

  @qt-parity
  Scenario: Strong emphasis is bold
    Then the property "strongIsBold" of "root" is true

  @qt-parity
  Scenario: A line break inside a paragraph is a space
    Then the property "softBreakIsASpace" of "root" is true

  @qt-parity
  Scenario: Paragraphs are set apart by a margin
    Then the property "paragraphsAreSetApart" of "root" is true

  @qt-parity
  Scenario: A heading is larger and bold
    Then the property "headingIsLarger" of "root" is true

  @qt-parity
  Scenario: A list stands in from the left, its items set apart like paragraphs
    Then the property "listStandsIn" of "root" is true

  @qt-parity
  Scenario: A quote stands in from both sides
    Then the property "quoteStandsIn" of "root" is true

  @qt-parity
  Scenario: A fenced code block keeps its lines
    Then the property "codeKeepsItsLines" of "root" is true

  @qt-parity
  Scenario: A table has a row for its header and one for each row
    Then the property "tableHasRows" of "root" is true

  @qt-parity
  Scenario: Markdown counts as one line however many it shows
    Then the property "lineCount" of "two" is 1

  # Qt hands HTML inside Markdown to its HTML importer, so "<b>abc</b>" is
  # bold there. Markdown often comes from the network or from a model, and
  # in Ljós a document cannot bring in a second language: the HTML is shown
  # as the text it is (ARCHITECTURE section 7.5).
  @qt-diverges
  Scenario: HTML in Markdown is shown as text
    Then the property "htmlIsText" of "root" is true

  # Qt 6.11.2 works the margin between blocks out from a font: the first
  # time a Text reads its text from the application's font (7 pixels), any
  # later time from the item's own (nothing for a font sized in pixels, two
  # thirds of the point size otherwise), so the same text is 47 pixels high
  # when it is there from the start and 40 when it is set again. In Ljós
  # what a Text shows does not depend on how often its text was set: the
  # margin stays 7 pixels.
  @qt-diverges
  Scenario: Text that is set again is set apart as it was the first time
    When the method "again" of "root" is called
    Then the property "paragraphsAreSetApart" of "root" is true
