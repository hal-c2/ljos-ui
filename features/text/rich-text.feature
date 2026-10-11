# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
#   https://doc.qt.io/qt-6/richtext-html-subset.html
# Verified on Qt 6.11.2 (qml6, offscreen). The documentation lists the
# elements; the margins and indents are Qt 6.11.2's, found by comparing
# sizes, and they are pixels whatever the font.

@qt-parity
Feature: Rich text
  RichText reads the HTML subset Qt documents and lays it out as blocks:
  paragraphs, headings, lists, quotes, preformatted text, tables and rules,
  each with its margins. It is tolerant: a missing end tag or an unknown
  element never loses the text.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property real line: plain.height
        property bool paragraphsAreSetApart: two.height === 2 * line + 12
        property bool outerMarginsAreDropped: one.height === line && one.width === plain.width
        property bool listStandsIn: list.width === plain.width + 40 && list.height === 2 * line
        property bool quoteStandsIn: quote.width === plain.width + 80
        property bool headingIsLarger: heading.height > line && heading.width > plain.width
        property bool ruleIsALine: rule.height === 3 * line
        property bool tableIsARow: table.height < 2 * line && table.width > 2 * plain.width
        property bool scriptIsLeftOut: script.width === plain.width && script.height === line
        property bool unknownIsDropped: unknown.width === plain.width
        property bool openTagsEnd: open.height === two.height
        component R: Text { textFormat: Text.RichText }
        Text { id: plain; objectName: "plain"; text: "abc" }
        R { id: one; text: "<p>abc</p>" } R { id: two; objectName: "two"; text: "<p>abc</p><p>abc</p>" }
        R { id: list; text: "<ul><li>abc</li><li>abc</li></ul>" }
        R { id: quote; text: "<blockquote>abc</blockquote>" } R { id: heading; text: "<h1>abc</h1>" }
        R { id: rule; text: "abc<hr>abc" } R { id: table; text: "<table><tr><td>abc</td><td>abc</td></tr></table>" }
        R { id: script; text: "<script>alert(1)</script>abc<style>p { color: red }</style>" }
        R { id: unknown; text: "<blink>abc</blink>" } R { id: open; text: "<p>abc<p><b>abc" }
        R { objectName: "wrapped"; width: 50; wrapMode: Text.WordWrap; text: "<p>abc abc abc</p>" }
      }
      """
    When 100 milliseconds pass

  Scenario: Paragraphs are set apart by a margin
    Then the property "paragraphsAreSetApart" of "root" is true

  Scenario: The margins around the whole text are dropped
    Then the property "outerMarginsAreDropped" of "root" is true

  Scenario: A list stands in from the left
    Then the property "listStandsIn" of "root" is true

  Scenario: A quote stands in from both sides
    Then the property "quoteStandsIn" of "root" is true

  Scenario: A heading is larger and bold
    Then the property "headingIsLarger" of "root" is true

  Scenario: A rule takes a line of its own
    Then the property "ruleIsALine" of "root" is true

  Scenario: The cells of a table row stand side by side
    Then the property "tableIsARow" of "root" is true

  Scenario: Scripts and style sheets are left out
    Then the property "scriptIsLeftOut" of "root" is true

  Scenario: An unknown element is left out and its content kept
    Then the property "unknownIsDropped" of "root" is true

  Scenario: A paragraph that is never closed ends where the next starts
    Then the property "openTagsEnd" of "root" is true

  Scenario: Rich text counts as one line however many it shows
    Then the property "lineCount" of "two" is 1
    And the property "lineCount" of "wrapped" is 1

  Scenario: Wrapped rich text is as wide as its item
    Then the property "contentWidth" of "wrapped" is 50
    And the property "implicitWidth" of "wrapped" is at least 51
