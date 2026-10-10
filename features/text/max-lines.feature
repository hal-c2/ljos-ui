# Sources:
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#maximumLineCount-prop
#   https://doc.qt.io/qt-6/qml-qtquick-text.html#truncated-prop
# Verified on Qt 6.11.2 (qml6, offscreen).

@qt-parity
Feature: Limiting the number of lines
  maximumLineCount cuts a wrapped Text after that many lines.

  Background:
    Given the QML:
      """qml
      import QtQuick
      Item {
        id: root
        objectName: "root"
        property string words: "Hello world, this is a long line of text"
        property bool twoLinesTall: two.height === 2 * one.height
        property bool implicitHeightIsCut: two.implicitHeight === two.height
        property bool roomyIsWhole: roomy.lineCount === whole.lineCount
        Text { id: one; objectName: "one"; text: "x" }
        Text { id: whole; objectName: "whole"; width: 100; text: root.words; wrapMode: Text.WordWrap }
        Text {
          id: two; objectName: "two"; width: 100; text: root.words
          wrapMode: Text.WordWrap; maximumLineCount: 2
        }
        Text {
          id: roomy; objectName: "roomy"; width: 100; text: root.words
          wrapMode: Text.WordWrap; maximumLineCount: 50
        }
        Text { id: single; objectName: "single"; text: root.words; maximumLineCount: 1 }
        Text { id: lines; objectName: "lines"; text: "a\nb\nc"; maximumLineCount: 2 }
      }
      """
    When 100 milliseconds pass

  Scenario: Lines after the limit are dropped
    Then the property "lineCount" of "whole" is at least 3
    And the property "lineCount" of "two" is 2
    And the property "truncated" of "two" is true

  Scenario: The item is as tall as the lines it keeps
    Then the property "twoLinesTall" of "root" is true
    And the property "implicitHeightIsCut" of "root" is true

  Scenario: A limit the text does not reach changes nothing
    Then the property "roomyIsWhole" of "root" is true
    And the property "truncated" of "roomy" is false

  Scenario: A text on one line is not truncated by a limit of one
    Then the property "lineCount" of "single" is 1
    And the property "truncated" of "single" is false

  Scenario: Line breaks in the text count towards the limit
    Then the property "lineCount" of "lines" is 2
    And the property "truncated" of "lines" is true

  Scenario: Raising the limit brings lines back
    When the property "maximumLineCount" of "two" is set to 50
    Then the property "roomyIsWhole" of "root" is true
    And the property "truncated" of "two" is false

  Scenario: Lowering the limit to one leaves one line
    When the property "maximumLineCount" of "two" is set to 1
    Then the property "lineCount" of "two" is 1
    And the property "truncated" of "two" is true

  Scenario Outline: A limit of <limit> is no limit
    When the property "maximumLineCount" of "two" is set to <limit>
    Then the property "lineCount" of "two" is at least 3
    And the property "truncated" of "two" is false
    And the property "maximumLineCount" of "two" is <limit>

    Examples:
      | limit |
      | 0     |
      | -3    |
