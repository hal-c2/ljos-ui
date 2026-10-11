# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#textFormat-prop
# Verified on Qt 6.11.2 (qml6, offscreen). The documentation lists the tags
# of StyledText; what an unknown tag, a missing end tag and white space do
# is Qt 6.11.2's behaviour, found by comparing sizes.

@qt-parity
Feature: Styled text
  StyledText reads a small, strict set of tags: b, strong, i, u, s, font,
  a, the headings, p, br, pre, lists and img. Anything else between angle
  brackets is left out and what it holds is kept. The text stays one
  flowing text: line counts, eliding and wrapping work as on plain text.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        objectName: "root"
        property bool boldIsWider: bold.width > plain.width
        property bool strongIsBold: strong.width === bold.width
        property bool italicKeepsLine: italic.height === plain.height
        property bool unknownIsDropped: unknown.width === plain.width
        property bool entityIsOneCharacter: entity.width === sign.width
        property bool spaceCollapses: spaced.width === single.width
        property bool headingIsLarger: heading.height > plain.height && heading.width > bold.width
        property bool bigFont: big.height > plain.height
        property bool listIsIndented: list.width > plain.width && list.lineCount === 2
        component T: Text { textFormat: Text.StyledText }
        Text { id: plain; objectName: "plain"; text: "abc" }
        Text { id: sign; text: "<" } Text { id: single; text: "a b" }
        T { id: bold; text: "<b>abc</b>" } T { id: strong; text: "<strong>abc</strong>" }
        T { id: italic; text: "<i>abc</i>" } T { id: unknown; text: "<span>a</span><x>bc</x>" }
        T { id: entity; text: "&lt;" } T { id: spaced; text: "a  \n  b" }
        T { id: heading; text: "<h1>abc</h1>" } T { id: big; text: "<font size=\"7\">abc</font>" }
        T { id: list; objectName: "list"; text: "<ul><li>abc<li>abc</ul>" }
        T { objectName: "broken"; text: "abc<br>abc" } T { objectName: "para"; text: "<p>abc</p>abc" }
        T { objectName: "open"; text: "<b>abc" }
      }
      """
    When 100 milliseconds pass

  Scenario: Bold text is set in the bold face
    Then the property "boldIsWider" of "root" is true
    And the property "strongIsBold" of "root" is true

  Scenario: Italic text stays on its line
    Then the property "italicKeepsLine" of "root" is true

  Scenario: An unknown tag is left out and its content kept
    Then the property "unknownIsDropped" of "root" is true

  Scenario: A character reference is the character
    Then the property "entityIsOneCharacter" of "root" is true

  Scenario: White space collapses to one space
    Then the property "spaceCollapses" of "root" is true

  Scenario: A heading is larger and bold
    Then the property "headingIsLarger" of "root" is true

  Scenario: A font size makes the line taller
    Then the property "bigFont" of "root" is true

  Scenario: A line break and a paragraph start new lines
    Then the property "lineCount" of "broken" is 2
    And the property "lineCount" of "para" is 2

  Scenario: List items are lines of their own, indented
    Then the property "listIsIndented" of "root" is true

  Scenario: A tag that is never closed styles the rest of the text
    Then the property "lineCount" of "open" is 1
    And the property "implicitWidth" of "open" is at least 1

  Scenario: The text is kept as it was written
    Then the property "text" of "list" is "<ul><li>abc<li>abc</ul>"
